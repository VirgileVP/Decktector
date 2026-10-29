//! Track builders shared by the simulation's unit tests.

use crate::model::{BeatGridBeat, CueKind, TrackCue, TrackInfo};

const TEST_TRACK_DURATION_MS: f64 = 600_000.0;
const MS_PER_MINUTE: f64 = 60_000.0;
const BEATS_PER_BAR: usize = 4;

/// A track with no metadata beyond a title and an artist.
pub(super) fn titled(title: &str, artist: Option<&str>) -> TrackInfo {
    TrackInfo {
        track_id: 1,
        title: title.to_owned(),
        artist: artist.map(str::to_owned),
        album: None,
        genre: None,
        key: None,
        label: None,
        comment: None,
        duration_ms: TEST_TRACK_DURATION_MS,
        bpm: None,
        artwork: None,
        beat_grid: None,
        waveform: None,
        cues: Vec::new(),
        events: Vec::new(),
    }
}

/// A track whose grid starts at 0 ms and holds `beats` regular beats.
pub(super) fn track_with_grid(bpm: f64, beats: usize) -> TrackInfo {
    let beat_ms = MS_PER_MINUTE / bpm;
    TrackInfo {
        bpm: Some(bpm),
        beat_grid: Some(
            (0..beats)
                .map(|beat| BeatGridBeat {
                    offset_ms: beat as f64 * beat_ms,
                    beat_in_measure: (beat % BEATS_PER_BAR) as u8 + 1,
                })
                .collect(),
        ),
        ..titled("Grid Track", Some("Test Artist"))
    }
}

pub(super) fn track_without_grid(bpm: f64) -> TrackInfo {
    TrackInfo {
        bpm: Some(bpm),
        ..titled("Gridless Track", None)
    }
}

pub(super) fn memory_cue(offset_ms: f64) -> TrackCue {
    TrackCue {
        kind: CueKind::Memory,
        offset_ms,
        length_ms: None,
        button: None,
        label: None,
        color: "#ffffff".to_owned(),
    }
}

/// A hot cue on `button`, or a hot loop when `length_ms` is given.
pub(super) fn hot_cue(button: &str, offset_ms: f64, length_ms: Option<f64>) -> TrackCue {
    TrackCue {
        kind: if length_ms.is_some() {
            CueKind::Hotloop
        } else {
            CueKind::Hotcue
        },
        button: Some(button.to_owned()),
        length_ms,
        ..memory_cue(offset_ms)
    }
}
