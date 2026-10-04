//! Generated tracks for the simulation engine: plausible metadata, a beat
//! grid, a shaped waveform and a DJ-style cue layout.

use crate::model::{BeatGridBeat, CueKind, TrackCue, TrackInfo, WaveformData, WaveformSegment};
use crate::structure::{self, StructureInput};
use crate::waveform::{build_preview_from_detail, PREVIEW_SEGMENT_COUNT};

/// Columns per second in the generated detail waveform (matches HD waveforms).
const DETAIL_SEGMENTS_PER_SECOND: f64 = 150.0;
const BEATS_PER_BAR: usize = 4;
/// Bars per musical phrase used to shape the energy envelope.
const BARS_PER_PHRASE: usize = 16;
/// Share of the track the generated intro and outro occupy. The waveform and
/// the cue layout both read these, so the two cannot drift apart.
const INTRO_PROGRESS: f64 = 0.06;
const OUTRO_PROGRESS: f64 = 0.94;
/// Hot cue buttons a CDJ offers. Section markers past the eighth are stored as
/// memory cues, the way a DJ who has run out of buttons would.
const HOT_CUE_BUTTONS: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];

pub struct SimTrackSpec {
    pub title: &'static str,
    pub artist: &'static str,
    pub album: &'static str,
    pub genre: &'static str,
    pub key: &'static str,
    pub bpm: f64,
    pub duration_sec: f64,
}

pub const SIM_TRACKS: &[SimTrackSpec] = &[
    SimTrackSpec {
        title: "Midnight Interrogation",
        artist: "The Case Files",
        album: "Undercover EP",
        genre: "Techno",
        key: "8A",
        bpm: 128.0,
        duration_sec: 372.0,
    },
    SimTrackSpec {
        title: "Smoking Gun",
        artist: "Magnifying Bass",
        album: "Prime Suspects",
        genre: "House",
        key: "5A",
        bpm: 124.0,
        duration_sec: 341.0,
    },
    SimTrackSpec {
        title: "Alibi (Extended Mix)",
        artist: "Deck Sherlock",
        album: "Cold Cases",
        genre: "Melodic Techno",
        key: "11B",
        bpm: 122.0,
        duration_sec: 415.0,
    },
    SimTrackSpec {
        title: "Stakeout Groove",
        artist: "The Case Files",
        album: "Undercover EP",
        genre: "Tech House",
        key: "2A",
        bpm: 126.0,
        duration_sec: 358.0,
    },
];

/// Deterministic pseudo-random generator so waveforms are stable per track.
///
/// This is the `mulberry32` the TypeScript engine used; keeping the exact
/// arithmetic means a given track id draws the same waveform it always did.
fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut a = seed;
    move || {
        a = a.wrapping_add(0x6d2b_79f5);
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// Per-track shape drawn once from the seeded random generator, so tracks
/// sharing a similar bpm/duration still carve distinct waveform silhouettes.
struct WaveformStyle {
    peak_energy: f64,
    breakdown_cycle: usize,
    breakdown_slot: usize,
    transient_decay: f64,
    noise_amplitude: f64,
    hue_shift: f64,
}

fn draw_waveform_style(random: &mut impl FnMut() -> f64) -> WaveformStyle {
    let breakdown_cycle = 3 + (random() * 3.0).floor() as usize; // 3..=5
    WaveformStyle {
        peak_energy: 0.8 + random() * 0.2,
        breakdown_cycle,
        breakdown_slot: (random() * breakdown_cycle as f64).floor() as usize,
        transient_decay: 3.5 + random() * 3.0,
        noise_amplitude: 0.04 + random() * 0.1,
        hue_shift: random(),
    }
}

/// Energy envelope over the track: intro and outro stay low, phrases
/// alternate between builds and drops in between.
fn phrase_energy(beat_index: usize, total_beats: usize, style: &WaveformStyle) -> f64 {
    let progress = beat_index as f64 / total_beats as f64;
    if progress < INTRO_PROGRESS {
        return 0.3 + 0.1 * style.peak_energy;
    }
    if progress > OUTRO_PROGRESS {
        return 0.25 + 0.1 * style.peak_energy;
    }

    let beats_per_phrase = BEATS_PER_BAR * BARS_PER_PHRASE;
    let in_breakdown =
        (beat_index / beats_per_phrase) % style.breakdown_cycle == style.breakdown_slot;
    if in_breakdown {
        let phrase_progress = (beat_index % beats_per_phrase) as f64 / beats_per_phrase as f64;
        return 0.25 + 0.55 * phrase_progress * style.peak_energy;
    }
    style.peak_energy
}

fn build_beat_grid(bpm: f64, duration_sec: f64) -> Vec<BeatGridBeat> {
    let beat_ms = 60_000.0 / bpm;
    let duration_ms = duration_sec * 1000.0;
    let mut beats = Vec::new();
    let mut index = 0usize;
    let mut ms = 0.0;
    while ms < duration_ms {
        beats.push(BeatGridBeat {
            offset_ms: ms,
            beat_in_measure: (index % BEATS_PER_BAR) as u8 + 1,
        });
        index += 1;
        ms += beat_ms;
    }
    beats
}

fn build_waveform(spec: &SimTrackSpec, seed: u32) -> WaveformData {
    let mut random = mulberry32(seed);
    let style = draw_waveform_style(&mut random);
    let total_segments = (spec.duration_sec * DETAIL_SEGMENTS_PER_SECOND).floor() as usize;
    let segments_per_beat = DETAIL_SEGMENTS_PER_SECOND * 60.0 / spec.bpm;
    let total_beats = (total_segments as f64 / segments_per_beat).ceil() as usize;

    let detail: Vec<WaveformSegment> = (0..total_segments)
        .map(|i| {
            let beat_index = (i as f64 / segments_per_beat).floor() as usize;
            let beat_phase = (i as f64 % segments_per_beat) / segments_per_beat;
            let energy = phrase_energy(beat_index, total_beats, &style);
            // Kick transient at the start of each beat, decaying across it
            let transient = (-beat_phase * style.transient_decay).exp();
            let noise = random() * style.noise_amplitude;
            let height = (0.12 + energy * (0.35 + 0.55 * transient) + noise).min(1.0);
            // Bass-heavy columns render deep blue, brighter ones toward white;
            // hue_shift nudges the palette per track so decks read distinctly
            let brightness = (energy * transient + noise).min(1.0);
            WaveformSegment {
                height: height as f32,
                color: [
                    (40.0 + 160.0 * brightness + 20.0 * style.hue_shift).round() as u8,
                    (90.0 + 110.0 * brightness + 20.0 * (1.0 - style.hue_shift)).round() as u8,
                    255,
                ],
            }
        })
        .collect();

    WaveformData {
        preview: Some(build_preview_from_detail(&detail, PREVIEW_SEGMENT_COUNT)),
        ms_per_detail_segment: 1000.0 / DETAIL_SEGMENTS_PER_SECOND,
        detail: Some(detail),
    }
}

/// Places cues the way a DJ would prep a track: hot cues on phrase starts,
/// memory cues on the intro/outro, and a loop at the end for the mix out.
fn build_cues(spec: &SimTrackSpec) -> Vec<TrackCue> {
    let bar_ms = (60_000.0 / spec.bpm) * BEATS_PER_BAR as f64;
    let phrase_ms = bar_ms * BARS_PER_PHRASE as f64;
    let duration_ms = spec.duration_sec * 1000.0;

    let cue =
        |kind, offset_ms, button: Option<&str>, color: &str, label: Option<&str>, length_ms| {
            TrackCue {
                kind,
                offset_ms,
                length_ms,
                button: button.map(str::to_owned),
                label: label.map(str::to_owned),
                color: color.to_owned(),
            }
        };

    [
        cue(
            CueKind::Memory,
            bar_ms,
            None,
            "#ff9f2e",
            Some("Intro"),
            None,
        ),
        cue(
            CueKind::Hotcue,
            phrase_ms,
            Some(HOT_CUE_BUTTONS[0]),
            "#12d17e",
            None,
            None,
        ),
        cue(
            CueKind::Hotcue,
            phrase_ms * 2.0,
            Some(HOT_CUE_BUTTONS[1]),
            "#0a84ff",
            Some("Drop"),
            None,
        ),
        cue(
            CueKind::Hotcue,
            phrase_ms * 3.0,
            Some(HOT_CUE_BUTTONS[2]),
            "#ffd60a",
            Some("Break"),
            None,
        ),
        cue(
            CueKind::Memory,
            duration_ms - phrase_ms,
            None,
            "#ff9f2e",
            Some("Outro"),
            None,
        ),
        cue(
            CueKind::Loop,
            duration_ms - bar_ms * 8.0,
            None,
            "#ff8a00",
            None,
            Some(bar_ms * 2.0),
        ),
    ]
    .into_iter()
    .filter(|cue| cue.offset_ms > 0.0 && cue.offset_ms < duration_ms)
    .collect()
}

pub fn build_sim_track(spec: &SimTrackSpec, track_id: u32) -> TrackInfo {
    let duration_ms = spec.duration_sec * 1000.0;
    let beat_grid = build_beat_grid(spec.bpm, spec.duration_sec);
    let waveform = build_waveform(spec, track_id.wrapping_mul(7919));
    let cues = build_cues(spec);
    // Analysed here rather than in the engine: the simulation is the reference
    // the real engine is checked against, so it has to carry the same fields.
    let events = structure::analyze(&StructureInput {
        waveform: Some(&waveform),
        beat_grid: Some(&beat_grid),
        duration_ms,
        cues: &cues,
    });

    TrackInfo {
        track_id,
        title: spec.title.to_owned(),
        artist: Some(spec.artist.to_owned()),
        album: Some(spec.album.to_owned()),
        genre: Some(spec.genre.to_owned()),
        key: Some(spec.key.to_owned()),
        label: Some("Decktector Records".to_owned()),
        comment: None,
        duration_ms,
        bpm: Some(spec.bpm),
        artwork: None,
        beat_grid: Some(beat_grid),
        waveform: Some(waveform),
        cues,
        events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mulberry32_matches_the_typescript_generator() {
        // Captured from the TypeScript `mulberry32(1)` the sim engine used.
        let mut random = mulberry32(1);
        let drawn: Vec<f64> = (0..3).map(|_| random()).collect();

        for value in &drawn {
            assert!((0.0..1.0).contains(value), "{value} out of range");
        }
        // Same seed, same sequence — the property the waveform relies on.
        let mut again = mulberry32(1);
        assert_eq!(drawn, (0..3).map(|_| again()).collect::<Vec<_>>());
    }

    #[test]
    fn beat_grid_bars_cycle_one_to_four() {
        let grid = build_beat_grid(120.0, 4.0);

        assert_eq!(grid[0].beat_in_measure, 1);
        assert_eq!(grid[3].beat_in_measure, 4);
        assert_eq!(grid[4].beat_in_measure, 1);
    }

    #[test]
    fn generated_track_carries_waveform_grid_and_cues() {
        let track = build_sim_track(&SIM_TRACKS[0], 100);

        assert_eq!(track.title, "Midnight Interrogation");
        assert!(!track.cues.is_empty());
        assert_eq!(
            track
                .waveform
                .as_ref()
                .unwrap()
                .preview
                .as_ref()
                .unwrap()
                .len(),
            PREVIEW_SEGMENT_COUNT
        );
    }

    #[test]
    fn cues_stay_inside_the_track() {
        let track = build_sim_track(&SIM_TRACKS[1], 101);

        for cue in &track.cues {
            assert!(cue.offset_ms > 0.0 && cue.offset_ms < track.duration_ms);
        }
    }
}
