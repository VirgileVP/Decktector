//! Tempo and beat grid, recovered from the onset envelope.
//!
//! A player reads its grid from the analysis rekordbox wrote next to the
//! track; an untagged MP3 carries none, so the grid here is detected:
//! autocorrelation for the period, a comb filter for the phase, low-band
//! energy for which beat starts the bar. The result is a regular grid — a
//! real one drifts with a live take, and this does not pretend to.

use decktective_core::model::BeatGridBeat;

use crate::columns::{Column, DETAIL_COLUMNS_PER_SECOND};

/// Tempo range searched, in BPM. Wide enough for a downtempo intro or a
/// drum-and-bass roll, narrow enough that the autocorrelation is not asked
/// to choose between periods no DJ track uses.
const MIN_BPM: f64 = 70.0;
const MAX_BPM: f64 = 190.0;
/// Where an ambiguous octave is resolved to. Autocorrelation scores a tempo
/// and its half equally well; club tracks live here, so a candidate inside
/// this range wins over its double or half outside it.
const PREFERRED_BPM: std::ops::Range<f64> = 85.0..160.0;
/// Fine search around the coarse estimate: ±`FINE_SPAN_BPM` in
/// `FINE_STEP_BPM` increments. A grid that should not drift by a beat over
/// a six-minute track needs the tempo to about 0.05 BPM.
const FINE_SPAN_BPM: f64 = 3.0;
const FINE_STEP_BPM: f64 = 0.05;
const BEATS_PER_BAR: usize = 4;

/// A detected tempo, anchored on the first downbeat of the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo {
    pub bpm: f64,
    /// Offset of the first beat-one, in ms.
    pub first_beat_ms: f64,
}

/// Detects the tempo, or `None` when the track holds too few onsets to
/// place a grid on at all — an ambient pad, or a file too short to fit a
/// bar.
pub fn detect(envelope: &[f32], columns: &[Column]) -> Option<Tempo> {
    let coarse = coarse_period(envelope)?;
    let (period, phase) = refine(envelope, coarse)?;
    let downbeat = first_downbeat(columns, period, phase);

    Some(Tempo {
        bpm: period_to_bpm(period),
        first_beat_ms: columns_to_ms(phase + downbeat as f64 * period),
    })
}

/// Beats from the first downbeat to the end of the track.
pub fn build_beat_grid(tempo: &Tempo, duration_ms: f64) -> Vec<BeatGridBeat> {
    let beat_ms = 60_000.0 / tempo.bpm;
    let mut beats = Vec::new();
    let mut index = 0usize;
    loop {
        let offset_ms = tempo.first_beat_ms + index as f64 * beat_ms;
        if offset_ms >= duration_ms {
            return beats;
        }
        beats.push(BeatGridBeat {
            offset_ms,
            beat_in_measure: (index % BEATS_PER_BAR) as u8 + 1,
        });
        index += 1;
    }
}

fn period_to_bpm(period_columns: f64) -> f64 {
    60.0 * DETAIL_COLUMNS_PER_SECOND / period_columns
}

fn bpm_to_period(bpm: f64) -> f64 {
    60.0 * DETAIL_COLUMNS_PER_SECOND / bpm
}

fn columns_to_ms(columns: f64) -> f64 {
    columns * 1000.0 / DETAIL_COLUMNS_PER_SECOND
}

/// Best beat period in whole columns, by autocorrelation of the onset
/// envelope, with the octave resolved toward [`PREFERRED_BPM`].
fn coarse_period(envelope: &[f32]) -> Option<f64> {
    let shortest = bpm_to_period(MAX_BPM).floor() as usize;
    let longest = bpm_to_period(MIN_BPM).ceil() as usize;
    if envelope.len() <= longest * 2 {
        return None;
    }

    let mean = envelope.iter().sum::<f32>() / envelope.len() as f32;
    let centered: Vec<f32> = envelope.iter().map(|value| value - mean).collect();
    let best = (shortest..=longest)
        .max_by(|&a, &b| autocorrelation(&centered, a).total_cmp(&autocorrelation(&centered, b)))?;

    Some(resolve_octave(&centered, best as f64))
}

fn autocorrelation(centered: &[f32], lag: usize) -> f32 {
    let overlap = centered.len().saturating_sub(lag);
    if overlap == 0 {
        return 0.0;
    }
    let sum: f32 = (0..overlap).map(|i| centered[i] * centered[i + lag]).sum();
    sum / overlap as f32
}

/// Half and double of a period correlate nearly as well as the period
/// itself. Prefers whichever of the three lands in [`PREFERRED_BPM`].
fn resolve_octave(centered: &[f32], period: f64) -> f64 {
    let in_range = |candidate: &f64| PREFERRED_BPM.contains(&period_to_bpm(*candidate));
    if in_range(&period) {
        return period;
    }

    [period * 2.0, period / 2.0]
        .into_iter()
        .filter(in_range)
        .filter(|candidate| (candidate.round() as usize) < centered.len())
        .max_by(|a, b| {
            autocorrelation(centered, a.round() as usize)
                .total_cmp(&autocorrelation(centered, b.round() as usize))
        })
        .unwrap_or(period)
}

/// Sharpens the period and finds its phase: the offset at which beats land
/// on the envelope's peaks rather than between them.
fn refine(envelope: &[f32], coarse: f64) -> Option<(f64, f64)> {
    let coarse_bpm = period_to_bpm(coarse);
    let steps = (2.0 * FINE_SPAN_BPM / FINE_STEP_BPM).round() as usize;

    (0..=steps)
        .map(|step| {
            let bpm =
                (coarse_bpm - FINE_SPAN_BPM + step as f64 * FINE_STEP_BPM).clamp(MIN_BPM, MAX_BPM);
            let period = bpm_to_period(bpm);
            let (phase, score) = comb_phase(envelope, period);
            (period, phase, score)
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(period, phase, _)| (period, phase))
}

/// Folds the envelope over one beat period and returns the fullest offset,
/// with a score of how much fuller it is than the average offset.
///
/// The ratio, not the raw sum, is what makes scores from different periods
/// comparable: every period folds the same total energy, just into a
/// different number of bins.
fn comb_phase(envelope: &[f32], period: f64) -> (f64, f32) {
    let bins = period.ceil().max(1.0) as usize;
    let mut folded = vec![0.0f32; bins];
    for (index, value) in envelope.iter().enumerate() {
        let bin = ((index as f64 % period).floor() as usize).min(bins - 1);
        folded[bin] += value;
    }

    let mean = folded.iter().sum::<f32>() / bins as f32;
    let (bin, peak) = folded
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(bin, peak)| (bin, *peak))
        .unwrap_or((0, 0.0));

    let score = if mean > 0.0 { peak / mean } else { 0.0 };
    (bin as f64 + 0.5, score)
}

/// Which of the first four beats starts a bar, by the beat carrying the
/// most low-band energy — the kick a four-to-the-floor track puts on the
/// one. An approximation, and the part of the grid most likely to sit a bar
/// phase off on a track that does not work that way.
fn first_downbeat(columns: &[Column], period: f64, phase: f64) -> usize {
    (0..BEATS_PER_BAR)
        .max_by(|&a, &b| {
            bar_phase_energy(columns, period, phase, a)
                .total_cmp(&bar_phase_energy(columns, period, phase, b))
        })
        .unwrap_or(0)
}

/// Total low-band energy on every fourth beat starting from `offset`.
fn bar_phase_energy(columns: &[Column], period: f64, phase: f64, offset: usize) -> f32 {
    let mut beat = offset;
    let mut total = 0.0;
    loop {
        let index = (phase + beat as f64 * period).round() as usize;
        let Some(column) = columns.get(index) else {
            return total;
        };
        total += column.bands[0];
        beat += BEATS_PER_BAR;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An envelope with a spike every beat at `bpm`, starting `offset`
    /// columns in, and a stronger one on every bar's first beat.
    fn click_track(bpm: f64, seconds: f64, offset: usize) -> (Vec<f32>, Vec<Column>) {
        let length = (seconds * DETAIL_COLUMNS_PER_SECOND) as usize;
        let period = bpm_to_period(bpm);
        let mut envelope = vec![0.0f32; length];
        let mut columns = vec![Column::default(); length];
        let mut beat = 0usize;
        loop {
            let index = offset + (beat as f64 * period).round() as usize;
            if index >= length {
                return (envelope, columns);
            }
            let downbeat = beat.is_multiple_of(BEATS_PER_BAR);
            envelope[index] = if downbeat { 1.0 } else { 0.8 };
            columns[index].bands[0] = if downbeat { 1.0 } else { 0.3 };
            beat += 1;
        }
    }

    #[test]
    fn finds_the_tempo_of_a_click_track() {
        let (envelope, columns) = click_track(128.0, 60.0, 20);

        let tempo = detect(&envelope, &columns).expect("a click track has a tempo");
        assert!((tempo.bpm - 128.0).abs() < 0.5, "{}", tempo.bpm);
    }

    #[test]
    fn anchors_the_grid_on_the_beat_carrying_the_kick() {
        let (envelope, columns) = click_track(128.0, 60.0, 20);

        let tempo = detect(&envelope, &columns).unwrap();
        assert!(
            (tempo.first_beat_ms - columns_to_ms(20.0)).abs() < 10.0,
            "{}",
            tempo.first_beat_ms
        );
    }

    #[test]
    fn halves_a_tempo_detected_above_the_range_djs_play() {
        // Eighth notes at 128 BPM: the period that correlates best is half
        // the one a DJ would call the tempo.
        let (envelope, columns) = click_track(256.0, 60.0, 0);

        let tempo = detect(&envelope, &columns).unwrap();
        assert!(PREFERRED_BPM.contains(&tempo.bpm), "{}", tempo.bpm);
    }

    #[test]
    fn a_track_too_short_to_hold_a_bar_has_no_tempo() {
        assert!(detect(&[0.0; 10], &[Column::default(); 10]).is_none());
    }

    #[test]
    fn the_grid_starts_on_beat_one_and_stops_at_the_end() {
        let tempo = Tempo {
            bpm: 120.0,
            first_beat_ms: 100.0,
        };

        let beats = build_beat_grid(&tempo, 2_100.0);

        assert_eq!(beats.len(), 4);
        assert_eq!(beats[0].beat_in_measure, 1);
        assert_eq!(beats[3].beat_in_measure, 4);
        assert!(beats.last().unwrap().offset_ms < 2_100.0);
    }
}
