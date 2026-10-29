//! Normalized domain model for Decktector.
//!
//! Everything the UI consumes is expressed with these types — the renderer
//! never sees Pro DJ Link structures. Both the real engine and the simulation
//! engine emit this model.
//!
//! The TypeScript counterparts are generated from these definitions by
//! `ts-rs`, so a divergence between Rust and the renderer is a build failure
//! rather than a bug in the middle of a set. `cargo test` writes them to the
//! directory named by `TS_RS_EXPORT_DIR` in `.cargo/config.toml`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Player number on the Pro DJ Link network (1-6).
pub type DeckId = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "PlayState.ts")]
pub enum PlayState {
    Empty,
    Loading,
    Cued,
    Playing,
    Paused,
    Searching,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "NetworkState.ts")]
pub enum NetworkState {
    /// No engine running yet — the state every session starts and ends in.
    #[default]
    Offline,
    Searching,
    Connected,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeviceInfo.ts")]
pub struct DeviceInfo {
    pub deck_id: DeckId,
    pub name: String,
    pub ip: String,
}

/// Live state of a deck, refreshed on every status packet (~5 Hz).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeckState.ts")]
pub struct DeckState {
    pub deck_id: DeckId,
    pub play_state: PlayState,
    pub is_master: bool,
    pub is_synced: bool,
    pub is_on_air: bool,
    /// Pitch fader as a percentage, e.g. `2.35` for +2.35%.
    pub pitch: f64,
    /// BPM of the loaded track at 0% pitch, null when unknown.
    pub track_bpm: Option<f64>,
    /// `track_bpm` adjusted by pitch, null when unknown.
    pub effective_bpm: Option<f64>,
    /// Beat within the current bar (1-4), null when not playing.
    pub beat_in_measure: Option<u8>,
    /// Identifier of the loaded track, null when no track is loaded.
    pub track_id: Option<u32>,
    /// Estimated playhead position in ms, derived from the last beat and the
    /// beat grid (Pro DJ Link has no absolute position on CDJ-2000-era
    /// hardware). null when it cannot be inferred yet.
    pub playhead_ms: Option<f64>,
    /// Epoch ms at which this state was computed (interpolation anchor).
    pub reported_at: f64,
    /// The loop the deck is playing, null when it isn't looping.
    #[serde(default)]
    pub active_loop: Option<ActiveLoop>,
}

/// A loop a deck is playing right now.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "ActiveLoop.ts")]
pub struct ActiveLoop {
    /// Where the loop sits in the track; null when the player only says it
    /// is looping (only a CDJ-3000 reports the loop's bounds).
    pub range: Option<LoopRange>,
}

/// A stretch of a track, in ms from its start.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "LoopRange.ts")]
pub struct LoopRange {
    pub start_ms: f64,
    pub end_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "BeatGridBeat.ts")]
pub struct BeatGridBeat {
    /// Offset of the beat from the start of the track, in ms.
    pub offset_ms: f64,
    /// Position of the beat within its bar (1-4).
    pub beat_in_measure: u8,
}

/// One column of a waveform. `height` is normalized to 0-1, `color` is an
/// `[r, g, b]` triplet (0-255).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "WaveformSegment.ts")]
pub struct WaveformSegment {
    pub height: f32,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "WaveformData.ts")]
pub struct WaveformData {
    /// Detailed waveform (~150 columns per second), null when unavailable.
    pub detail: Option<Vec<WaveformSegment>>,
    /// How many ms one detail column covers.
    pub ms_per_detail_segment: f64,
    /// Whole-track preview for the minimap (fixed column count).
    pub preview: Option<Vec<WaveformSegment>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "CueKind.ts")]
pub enum CueKind {
    Memory,
    Hotcue,
    Loop,
    Hotloop,
}

/// A cue point, hot cue or loop stored with the track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TrackCue.ts")]
pub struct TrackCue {
    pub kind: CueKind,
    pub offset_ms: f64,
    /// Loop length in ms; null for plain cue points.
    pub length_ms: Option<f64>,
    /// Hot cue button letter (A-H); null for memory cues and plain loops.
    pub button: Option<String>,
    /// Comment attached to the cue in Rekordbox.
    pub label: Option<String>,
    /// CSS color of the marker, resolved engine-side.
    pub color: String,
}

/// A structural section of a track, as DJs name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "TrackEventKind.ts")]
pub enum TrackEventKind {
    Intro,
    /// Energy climbing toward the next section.
    Build,
    /// Full-energy section entered from a build or a break.
    Drop,
    /// The kick drops out; the track breathes.
    Break,
    /// Steady full-energy section — the default when nothing else fits.
    Groove,
    Outro,
}

/// One section of the track: a phrase rekordbox's own analysis found when
/// the track carries one, otherwise inferred from its waveform and beat
/// grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TrackEvent.ts")]
pub struct TrackEvent {
    pub kind: TrackEventKind,
    pub start_ms: f64,
    pub end_ms: f64,
    /// 0-1 — how much the label can be trusted. A rekordbox phrase or a
    /// section a DJ cue confirms scores 1.0; a purely inferred one stays
    /// lower, so the UI can hedge.
    pub confidence: f32,
    /// Rekordbox's name for the phrase ("Verse 2", "Up 3"); null for an
    /// inferred section, which goes by its kind.
    #[serde(default)]
    pub label: Option<String>,
}

/// Static description of a loaded track, fetched once per track change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TrackInfo.ts")]
pub struct TrackInfo {
    pub track_id: u32,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    /// Musical key as displayed by Rekordbox, e.g. "8A" or "Am".
    pub key: Option<String>,
    pub label: Option<String>,
    pub comment: Option<String>,
    pub duration_ms: f64,
    pub bpm: Option<f64>,
    /// Album artwork as a ready-to-use `data:` URL.
    ///
    /// The bytes are base64-encoded engine-side rather than shipped raw: the
    /// Tauri event payload is JSON, which has no binary representation, and a
    /// `data:` URL drops straight into an `<img src>` with no `Blob` and no
    /// object URL to revoke.
    pub artwork: Option<String>,
    pub beat_grid: Option<Vec<BeatGridBeat>>,
    pub waveform: Option<WaveformData>,
    pub cues: Vec<TrackCue>,
    /// Sections found by `structure::analyze`; empty when the track carries
    /// too little to analyse (no waveform, no beat grid).
    pub events: Vec<TrackEvent>,
}

/// A deck and whatever track it currently holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "DeckTrack.ts")]
pub struct DeckTrack {
    pub deck_id: DeckId,
    pub track: Option<TrackInfo>,
}

/// Full engine state, used to hydrate the UI on (re)connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "EngineSnapshot.ts")]
pub struct EngineSnapshot {
    pub network_state: NetworkState,
    pub devices: Vec<DeviceInfo>,
    pub decks: Vec<DeckState>,
    pub tracks: Vec<DeckTrack>,
}
