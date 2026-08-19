//! Track structure analysis: where the intro, the builds, the drops and the
//! breaks are.
//!
//! Three stages, one module each — bar features from the waveform and beat
//! grid, boundaries from a novelty curve over those features, then DJ labels
//! for the sections in between. Each stage is pure, so a bad result can be
//! traced to the stage that produced it.
//!
//! No audio file is decoded. Everything comes from what the players already
//! publish, which keeps the analysis available for any track on the link —
//! including one playing off a USB stick this machine cannot see — at the cost
//! of the detail a decoded signal would give.

mod boundaries;
mod features;
mod labels;

use crate::model::{BeatGridBeat, TrackCue, TrackEvent, WaveformData};
use features::BarFeatures;

/// Two novelty windows' worth of bars. Below that the curve is all edge effect
/// and any boundary it reports is an artefact.
const MIN_BARS_FOR_ANALYSIS: usize = 16;

/// Everything the analysis reads off a track, borrowed from a `TrackInfo`
/// being assembled or from one already built.
pub struct StructureInput<'a> {
    pub waveform: Option<&'a WaveformData>,
    pub beat_grid: Option<&'a [BeatGridBeat]>,
    pub duration_ms: f64,
    pub cues: &'a [TrackCue],
}

/// Infers the track's sections, in playing order and covering it end to end.
///
/// Returns an empty vec when the track carries too little to analyse — no
/// waveform, no beat grid, or too few bars. That is not an error: it is what
/// the UI shows as "not analysed", and every player that hands us a partial
/// track produces it.
pub fn analyze(input: &StructureInput<'_>) -> Vec<TrackEvent> {
    let (Some(waveform), Some(beat_grid)) = (input.waveform, input.beat_grid) else {
        return Vec::new();
    };

    let bars = features::bar_features(waveform, beat_grid, input.duration_ms);
    if bars.len() < MIN_BARS_FOR_ANALYSIS {
        return Vec::new();
    }

    let vectors = features::normalize(&bars);
    let novelty = boundaries::novelty_curve(&vectors);
    let boundaries = boundaries::find_boundaries(&novelty, &cue_bars(&bars, input.cues));

    labels::label_sections(&bars, &boundaries, input.cues)
}

/// Bars a DJ cue falls in, so the boundary search can favour them.
fn cue_bars(bars: &[BarFeatures], cues: &[TrackCue]) -> Vec<usize> {
    cues.iter()
        .filter_map(|cue| {
            bars.iter()
                .position(|bar| (bar.start_ms..bar.end_ms).contains(&cue.offset_ms))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackEventKind;
    use crate::sim::{build_sim_track, SIM_TRACKS};

    #[test]
    fn a_simulated_track_is_analysed_end_to_end() {
        let track = build_sim_track(&SIM_TRACKS[0], 1);

        assert!(
            track.events.len() > 2,
            "expected several sections, got {:?}",
            track.events
        );
        assert_eq!(track.events.first().unwrap().start_ms, 0.0);
        for pair in track.events.windows(2) {
            assert_eq!(pair[0].end_ms, pair[1].start_ms);
        }
    }

    #[test]
    fn every_simulated_track_finds_its_intro_and_a_drop() {
        for (index, spec) in SIM_TRACKS.iter().enumerate() {
            let track = build_sim_track(spec, index as u32 + 1);
            let kinds: Vec<_> = track.events.iter().map(|event| event.kind).collect();

            assert_eq!(kinds.first(), Some(&TrackEventKind::Intro), "{}", spec.title);
            assert!(
                kinds.contains(&TrackEventKind::Drop),
                "{} found no drop in {kinds:?}",
                spec.title
            );
        }
    }

    #[test]
    fn a_track_without_a_waveform_is_not_analysed() {
        let events = analyze(&StructureInput {
            waveform: None,
            beat_grid: None,
            duration_ms: 300_000.0,
            cues: &[],
        });

        assert!(events.is_empty());
    }
}
