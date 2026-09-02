//! DJ names for the sections between boundaries.
//!
//! Sections are labelled in playing order, each one seeing the label the
//! previous one got: what makes a drop a drop is as much what came before it
//! as its own energy. A cue the DJ wrote a name on overrides the inference.

use super::features::{median_energy, BarFeatures};
use crate::model::{TrackCue, TrackEvent, TrackEventKind};

/// Energy ratios against the track median that separate the section types.
const INTRO_ENERGY_RATIO: f32 = 0.92;
const OUTRO_ENERGY_RATIO: f32 = 0.88;
const BREAK_ENERGY_RATIO: f32 = 0.72;
const DROP_ENERGY_RATIO: f32 = 0.98;
/// Energy a section has to gain from its first third to its last, as a share
/// of the track median, before it reads as a build rather than a groove.
const BUILD_ENERGY_GAIN_RATIO: f32 = 0.15;
/// Confidence band for a section nothing but the waveform vouches for. A DJ
/// cue confirming the label scores 1.0 instead.
const INFERRED_CONFIDENCE_FLOOR: f32 = 0.5;
const INFERRED_CONFIDENCE_CEILING: f32 = 0.9;
/// How close to a section start a cue has to sit for its name to apply —
/// roughly one bar at club tempo.
const CUE_LABEL_WINDOW_MS: f64 = 2_000.0;

/// What the labelling pass needs to know about a section, once its bars are
/// averaged: nothing about waveform columns survives this far.
struct Section {
    start_ms: f64,
    end_ms: f64,
    energy: f32,
    /// Energy gained from the section's first third to its last.
    gain: f32,
}

/// Names each section between the given bar boundaries.
pub fn label_sections(
    bars: &[BarFeatures],
    boundaries: &[usize],
    cues: &[TrackCue],
) -> Vec<TrackEvent> {
    let sections = split(bars, boundaries);
    let median = median_energy(bars);
    if sections.is_empty() || median <= 0.0 {
        return Vec::new();
    }

    let mut events: Vec<TrackEvent> = Vec::with_capacity(sections.len());
    for (index, section) in sections.iter().enumerate() {
        let previous = events.last().map(|event: &TrackEvent| event.kind);
        let next_energy = sections.get(index + 1).map(|next| next.energy);
        events.push(TrackEvent {
            kind: classify(
                section,
                index,
                sections.len(),
                previous,
                next_energy,
                median,
            ),
            start_ms: section.start_ms,
            end_ms: section.end_ms,
            confidence: confidence(section, median),
        });
    }

    apply_cue_labels(&mut events, cues);
    coalesce(events)
}

/// Merges neighbouring sections that ended up with the same label.
///
/// The boundary search works on novelty alone, so it happily splits one long
/// build in two where the rise steepens. Once both halves are named the same
/// thing they are one section, and listing them twice would tell the DJ a
/// change is coming that never arrives. The merged section keeps the higher
/// confidence: a cue vouching for either half vouches for the whole.
fn coalesce(events: Vec<TrackEvent>) -> Vec<TrackEvent> {
    events.into_iter().fold(Vec::new(), |mut merged, event| {
        match merged.last_mut() {
            Some(previous) if previous.kind == event.kind => {
                previous.end_ms = event.end_ms;
                previous.confidence = previous.confidence.max(event.confidence);
            }
            _ => merged.push(event),
        }
        merged
    })
}

fn classify(
    section: &Section,
    index: usize,
    count: usize,
    previous: Option<TrackEventKind>,
    next_energy: Option<f32>,
    median: f32,
) -> TrackEventKind {
    let energy = section.energy;

    if index == 0 && energy < median * INTRO_ENERGY_RATIO {
        return TrackEventKind::Intro;
    }
    if index + 1 == count && energy < median * OUTRO_ENERGY_RATIO {
        return TrackEventKind::Outro;
    }

    // A rise that leads nowhere is a groove that happens to swell; a build is
    // a rise the next section pays off.
    let pays_off = next_energy.is_some_and(|next| next > energy);
    if section.gain > median * BUILD_ENERGY_GAIN_RATIO && pays_off {
        return TrackEventKind::Build;
    }

    if energy < median * BREAK_ENERGY_RATIO {
        return TrackEventKind::Break;
    }

    // Full energy on its own is just the groove. It is a drop only where the
    // track arrives there from something quieter.
    let after_a_lull = matches!(
        previous,
        Some(TrackEventKind::Break | TrackEventKind::Build | TrackEventKind::Intro)
    );
    if energy >= median * DROP_ENERGY_RATIO && after_a_lull {
        return TrackEventKind::Drop;
    }

    TrackEventKind::Groove
}

/// Maps how far the section's energy sits from the track median into the
/// inferred-confidence band: an unmistakable break or drop scores high, a
/// section sitting right on the median stays near the floor.
fn confidence(section: &Section, median: f32) -> f32 {
    let separation = ((section.energy - median).abs() / median).clamp(0.0, 1.0);
    let span = INFERRED_CONFIDENCE_CEILING - INFERRED_CONFIDENCE_FLOOR;
    INFERRED_CONFIDENCE_FLOOR + separation * span
}

/// A cue comment naming a section is the DJ's own annotation. Where one lands
/// on a section start it replaces whatever the waveform suggested.
fn apply_cue_labels(events: &mut [TrackEvent], cues: &[TrackCue]) {
    for cue in cues {
        let Some(kind) = cue.label.as_deref().and_then(kind_from_label) else {
            continue;
        };
        let Some(event) = events.iter_mut().find(|event| {
            let offset = cue.offset_ms - event.start_ms;
            (0.0..CUE_LABEL_WINDOW_MS).contains(&offset)
        }) else {
            continue;
        };
        event.kind = kind;
        event.confidence = 1.0;
    }
}

fn kind_from_label(label: &str) -> Option<TrackEventKind> {
    let label = label.to_lowercase();
    // Ordered, not a match: cue comments are free text, and "drop break" names
    // a break, so `break` has to be tested before `drop`.
    [
        ("intro", TrackEventKind::Intro),
        ("outro", TrackEventKind::Outro),
        ("break", TrackEventKind::Break),
        ("build", TrackEventKind::Build),
        ("drop", TrackEventKind::Drop),
    ]
    .into_iter()
    .find_map(|(needle, kind)| label.contains(needle).then_some(kind))
}

fn split(bars: &[BarFeatures], boundaries: &[usize]) -> Vec<Section> {
    boundaries
        .iter()
        .enumerate()
        .filter_map(|(index, &start)| {
            let end = boundaries.get(index + 1).copied().unwrap_or(bars.len());
            let section = bars.get(start..end).filter(|bars| !bars.is_empty())?;
            Some(Section {
                start_ms: section[0].start_ms,
                end_ms: section[section.len() - 1].end_ms,
                energy: mean_energy(section),
                gain: energy_gain(section),
            })
        })
        .collect()
}

fn mean_energy(bars: &[BarFeatures]) -> f32 {
    bars.iter().map(|bar| bar.energy).sum::<f32>() / bars.len() as f32
}

/// Compares the section's outer thirds. Thirds rather than first and last bar:
/// a single fill or a dropped beat should not read as a build.
fn energy_gain(bars: &[BarFeatures]) -> f32 {
    let third = (bars.len() / 3).max(1);
    mean_energy(&bars[bars.len() - third..]) - mean_energy(&bars[..third])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CueKind;

    /// Bars of 500 ms whose energies are given one per bar.
    fn bars(energies: &[f32]) -> Vec<BarFeatures> {
        energies
            .iter()
            .enumerate()
            .map(|(index, &energy)| BarFeatures {
                start_ms: index as f64 * 500.0,
                end_ms: (index + 1) as f64 * 500.0,
                energy,
                low_share: energy,
                brightness: energy,
                transient_rate: 0.2,
            })
            .collect()
    }

    /// Bar boundaries of [`arranged_track`].
    const ARRANGED_BOUNDARIES: [usize; 6] = [0, 8, 16, 32, 40, 56];

    /// A conventionally arranged track: quiet intro, rising build, drop,
    /// break, second drop, quiet outro. Most of its bars sit at full energy,
    /// the way a club track does — that is what puts the median at the groove
    /// level, which every threshold here is stated against.
    fn arranged_track() -> Vec<BarFeatures> {
        let mut energies = vec![0.35; 8];
        energies.extend((0..8).map(|bar| 0.45 + bar as f32 * 0.05));
        energies.extend(vec![0.95; 16]);
        energies.extend(vec![0.3; 8]);
        energies.extend(vec![0.95; 16]);
        energies.extend(vec![0.28; 8]);
        bars(&energies)
    }

    fn labelled(bars: &[BarFeatures]) -> Vec<TrackEventKind> {
        label_kinds(bars, &ARRANGED_BOUNDARIES)
    }

    fn label_kinds(bars: &[BarFeatures], boundaries: &[usize]) -> Vec<TrackEventKind> {
        label_sections(bars, boundaries, &[])
            .into_iter()
            .map(|event| event.kind)
            .collect()
    }

    fn cue(offset_ms: f64, label: &str) -> TrackCue {
        TrackCue {
            kind: CueKind::Hotcue,
            offset_ms,
            length_ms: None,
            button: Some("A".to_owned()),
            label: Some(label.to_owned()),
            color: "#ffffff".to_owned(),
        }
    }

    #[test]
    fn an_arranged_track_reads_as_intro_build_drop_break_outro() {
        assert_eq!(
            labelled(&arranged_track()),
            vec![
                TrackEventKind::Intro,
                TrackEventKind::Build,
                TrackEventKind::Drop,
                TrackEventKind::Break,
                TrackEventKind::Drop,
                TrackEventKind::Outro,
            ]
        );
    }

    #[test]
    fn sections_span_the_whole_track_without_a_gap() {
        let bars = arranged_track();
        let events = label_sections(&bars, &ARRANGED_BOUNDARIES, &[]);

        assert_eq!(events.first().unwrap().start_ms, 0.0);
        assert_eq!(events.last().unwrap().end_ms, bars.last().unwrap().end_ms);
        for pair in events.windows(2) {
            assert_eq!(pair[0].end_ms, pair[1].start_ms);
        }
    }

    #[test]
    fn full_energy_reached_without_a_lull_stays_a_groove() {
        // No quiet section anywhere: nothing here is a drop.
        let events = labelled(&bars(&[1.0; 40]));

        assert!(events.iter().all(|&kind| kind == TrackEventKind::Groove));
    }

    #[test]
    fn a_cue_comment_overrides_the_inferred_label() {
        let bars = arranged_track();
        // Bar 16 starts at 8000 ms — the section the waveform calls a drop.
        let events = label_sections(&bars, &ARRANGED_BOUNDARIES, &[cue(8_000.0, "Fake drop")]);

        assert_eq!(events[2].kind, TrackEventKind::Drop);
        assert_eq!(events[2].confidence, 1.0);
    }

    #[test]
    fn a_cue_named_drop_break_reads_as_a_break() {
        let bars = arranged_track();
        let events = label_sections(&bars, &ARRANGED_BOUNDARIES, &[cue(8_000.0, "Drop break")]);

        assert_eq!(events[2].kind, TrackEventKind::Break);
    }

    #[test]
    fn a_cue_in_the_middle_of_a_section_is_ignored() {
        let bars = arranged_track();
        // 11 000 ms is bar 22, six bars past the section start.
        let events = label_sections(&bars, &ARRANGED_BOUNDARIES, &[cue(11_000.0, "Intro")]);

        assert_eq!(events[2].kind, TrackEventKind::Drop);
    }

    #[test]
    fn inferred_confidence_stays_inside_its_band() {
        let events = label_sections(&arranged_track(), &ARRANGED_BOUNDARIES, &[]);

        assert!(events.iter().all(|event| {
            (INFERRED_CONFIDENCE_FLOOR..=INFERRED_CONFIDENCE_CEILING).contains(&event.confidence)
        }));
    }

    /// [`ARRANGED_BOUNDARIES`] with the break cut in two at bar 36, the way a
    /// novelty peak inside a section would cut it.
    const SPLIT_BREAK_BOUNDARIES: [usize; 7] = [0, 8, 16, 32, 36, 40, 56];

    #[test]
    fn a_section_split_by_a_weak_boundary_comes_back_as_one() {
        let bars = arranged_track();

        assert_eq!(labelled(&bars), label_kinds(&bars, &SPLIT_BREAK_BOUNDARIES));
    }

    #[test]
    fn merging_keeps_the_confidence_of_the_half_a_cue_vouched_for() {
        let bars = arranged_track();
        // 18 000 ms is bar 36 — the start of the break's second half only.
        let events = label_sections(&bars, &SPLIT_BREAK_BOUNDARIES, &[cue(18_000.0, "Break")]);

        let break_section = events
            .iter()
            .find(|event| event.kind == TrackEventKind::Break)
            .expect("the arrangement has a break");
        assert_eq!(break_section.confidence, 1.0);
        // Bar 32, where the break started — not the cue's own half.
        assert_eq!(break_section.start_ms, 16_000.0);
    }

    #[test]
    fn a_track_with_no_bars_yields_no_events() {
        assert!(label_sections(&[], &[0], &[]).is_empty());
    }
}
