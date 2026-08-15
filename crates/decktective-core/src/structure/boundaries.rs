//! Where one section of a track ends and the next begins.
//!
//! The novelty curve is Foote's idea reduced to what bar-level features need:
//! compare the bars just before a position with the bars just after, and a
//! large distance means the music changed there.

use super::features::{FeatureVector, FEATURE_COUNT};

/// Half-width of the novelty comparison window, in bars. Eight bars is half a
/// phrase: long enough to average out a single fill, short enough that two
/// adjacent sections do not blur into each other.
const NOVELTY_WINDOW_BARS: usize = 8;
/// Boundaries only ever land on this grid. Club tracks change on phrase
/// multiples, and snapping there removes most false positives outright.
const CANDIDATE_GRID_BARS: usize = 4;
/// Shortest section worth naming. Also the minimum spacing between two
/// boundaries, and between a boundary and either end of the track.
const MIN_SECTION_BARS: usize = 8;
/// Novelty a candidate needs before it counts as a boundary at all.
const NOVELTY_THRESHOLD: f32 = 0.12;
/// Added to a candidate a DJ cue already marks. Cues are hand-placed, so they
/// outrank the waveform when the two disagree about a few bars.
const CUE_ANCHOR_BONUS: f32 = 0.15;
/// How far from a cue a candidate still counts as marked, in bars.
const CUE_ANCHOR_TOLERANCE_BARS: usize = 1;

/// How different the music is on either side of each bar, in 0-1.
pub fn novelty_curve(vectors: &[FeatureVector]) -> Vec<f32> {
    (0..vectors.len())
        .map(|bar| {
            let before = &vectors[bar.saturating_sub(NOVELTY_WINDOW_BARS)..bar];
            let after = &vectors[bar..(bar + NOVELTY_WINDOW_BARS).min(vectors.len())];
            // The very start and end of a track have nothing to compare
            // against, and are boundaries by construction anyway.
            if before.is_empty() || after.is_empty() {
                return 0.0;
            }
            distance(&mean(before), &mean(after))
        })
        .collect()
}

/// Bars where a new section starts, ascending, always starting at bar 0.
///
/// `cue_bars` are the bars DJ cues fall in; they bias the search rather than
/// dictate it, so an unmarked drop is still found and a stray cue in the
/// middle of a groove does not split it.
pub fn find_boundaries(novelty: &[f32], cue_bars: &[usize]) -> Vec<usize> {
    let bar_count = novelty.len();
    let mut candidates: Vec<(usize, f32)> = (CANDIDATE_GRID_BARS..bar_count)
        .step_by(CANDIDATE_GRID_BARS)
        .map(|bar| (bar, novelty[bar] + cue_bonus(bar, cue_bars)))
        .filter(|&(_, score)| score >= NOVELTY_THRESHOLD)
        .collect();

    // Strongest first, so a weak candidate never crowds out a strong one by
    // reaching the minimum-spacing check earlier.
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));

    let mut accepted = vec![0usize];
    for (bar, _) in candidates {
        let clear_of_end = bar_count - bar >= MIN_SECTION_BARS;
        let clear_of_others = accepted
            .iter()
            .all(|&taken| bar.abs_diff(taken) >= MIN_SECTION_BARS);
        if clear_of_end && clear_of_others {
            accepted.push(bar);
        }
    }

    accepted.sort_unstable();
    accepted
}

fn cue_bonus(bar: usize, cue_bars: &[usize]) -> f32 {
    let marked = cue_bars
        .iter()
        .any(|&cue| bar.abs_diff(cue) <= CUE_ANCHOR_TOLERANCE_BARS);
    if marked {
        CUE_ANCHOR_BONUS
    } else {
        0.0
    }
}

fn mean(vectors: &[FeatureVector]) -> FeatureVector {
    let mut total = [0.0f32; FEATURE_COUNT];
    for vector in vectors {
        for (sum, value) in total.iter_mut().zip(vector) {
            *sum += value;
        }
    }
    total.map(|sum| sum / vectors.len() as f32)
}

/// Euclidean distance scaled back into 0-1, so the threshold stays meaningful
/// if a feature is ever added.
fn distance(left: &FeatureVector, right: &FeatureVector) -> f32 {
    let squares: f32 = left
        .iter()
        .zip(right)
        .map(|(a, b)| (a - b) * (a - b))
        .sum();
    (squares / FEATURE_COUNT as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `bar_count` bars, all features flat at `value`.
    fn flat(bar_count: usize, value: f32) -> Vec<FeatureVector> {
        vec![[value; FEATURE_COUNT]; bar_count]
    }

    /// Quiet for `switch` bars, then loud — a single unmistakable boundary.
    fn step(bar_count: usize, switch: usize) -> Vec<FeatureVector> {
        (0..bar_count)
            .map(|bar| [if bar < switch { 0.0 } else { 1.0 }; FEATURE_COUNT])
            .collect()
    }

    #[test]
    fn a_flat_track_has_no_novelty() {
        let novelty = novelty_curve(&flat(40, 0.5));

        assert!(novelty.iter().all(|&value| value == 0.0));
    }

    #[test]
    fn novelty_peaks_on_the_step() {
        let novelty = novelty_curve(&step(64, 32));
        let peak = novelty
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(bar, _)| bar);

        assert_eq!(peak, Some(32));
    }

    #[test]
    fn a_flat_track_is_one_section() {
        assert_eq!(find_boundaries(&novelty_curve(&flat(64, 0.5)), &[]), vec![0]);
    }

    #[test]
    fn the_step_becomes_a_boundary() {
        let boundaries = find_boundaries(&novelty_curve(&step(64, 32)), &[]);

        assert_eq!(boundaries.first(), Some(&0));
        assert!(boundaries.contains(&32));
    }

    #[test]
    fn boundaries_land_on_the_phrase_grid_and_keep_their_distance() {
        let boundaries = find_boundaries(&novelty_curve(&step(96, 40)), &[]);

        assert!(boundaries.iter().all(|bar| bar % CANDIDATE_GRID_BARS == 0));
        for pair in boundaries.windows(2) {
            assert!(pair[1] - pair[0] >= MIN_SECTION_BARS);
        }
    }

    #[test]
    fn a_cue_promotes_a_boundary_the_waveform_alone_would_miss() {
        // Novelty just under the threshold everywhere: only the bonus can lift
        // a candidate over it.
        let novelty = vec![NOVELTY_THRESHOLD - 0.01; 64];

        assert_eq!(find_boundaries(&novelty, &[]), vec![0]);
        assert!(find_boundaries(&novelty, &[32]).contains(&32));
    }

    #[test]
    fn no_boundary_lands_within_a_section_of_the_track_end() {
        let boundaries = find_boundaries(&novelty_curve(&step(64, 60)), &[]);

        assert!(boundaries
            .iter()
            .all(|&bar| 64 - bar >= MIN_SECTION_BARS));
    }
}
