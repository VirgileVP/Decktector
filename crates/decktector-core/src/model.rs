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
    /// The loaded track with the media it lives on, null when no track is
    /// loaded. Compare it with [`TrackInfo::source`] to tell whether the
    /// deck's metadata is already this track's (see [`is_identified`]).
    #[serde(default)]
    pub loaded_track: Option<TrackRef>,
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

/// A track as the player addresses it: the id is unique per media only.
///
/// Every rekordbox stick numbers its tracks from 1, so the next DJ's track 15
/// is not the previous DJ's: only the player holding the media, its slot and
/// the id together name one track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TrackRef.ts")]
pub struct TrackRef {
    /// The player whose media holds the track — the deck itself on a
    /// stand-alone player, another player on a linked booth.
    pub source_device: u8,
    /// Wire name of the slot (`usb`, `sd`, `rekordbox`, `cd`, `streaming`, `unknown`).
    pub slot: String,
    pub track_id: u32,
}

/// Whether `info` is the metadata of the track `loaded` names, rather than
/// what the deck still shows from the previous track while the new one's
/// metadata is on its way (or never arrives).
///
/// Only meaningful for a deck holding a track: an empty deck has no
/// `loaded` to ask about.
pub fn is_identified(loaded: &TrackRef, info: Option<&TrackInfo>) -> bool {
    info.and_then(|info| info.source.as_ref()) == Some(loaded)
}

/// Static description of a loaded track, fetched once per track change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TrackInfo.ts")]
pub struct TrackInfo {
    pub track_id: u32,
    /// The track this metadata was fetched for, null when the engine did not
    /// say. A deck keeps showing its previous track's metadata until the new
    /// one's lands, so this is what tells the two apart.
    #[serde(default)]
    pub source: Option<TrackRef>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn track_ref(source_device: u8, slot: &str, track_id: u32) -> TrackRef {
        TrackRef {
            source_device,
            slot: slot.to_owned(),
            track_id,
        }
    }

    fn info_for(source: Option<TrackRef>) -> TrackInfo {
        TrackInfo {
            track_id: source.as_ref().map_or(0, |source| source.track_id),
            source,
            title: "Shown".to_owned(),
            artist: None,
            album: None,
            genre: None,
            key: None,
            label: None,
            comment: None,
            duration_ms: 0.0,
            bpm: None,
            artwork: None,
            beat_grid: None,
            waveform: None,
            cues: Vec::new(),
            events: Vec::new(),
        }
    }

    #[test]
    fn a_deck_whose_track_info_is_from_the_previous_track_is_pending() {
        let loaded = track_ref(2, "usb", 15);

        let same_track = info_for(Some(track_ref(2, "usb", 15)));
        assert!(is_identified(&loaded, Some(&same_track)));

        // The previous DJ's track 15, off another player's stick.
        let other_media = info_for(Some(track_ref(3, "usb", 15)));
        assert!(!is_identified(&loaded, Some(&other_media)));
        let other_slot = info_for(Some(track_ref(2, "sd", 15)));
        assert!(!is_identified(&loaded, Some(&other_slot)));

        assert!(!is_identified(&loaded, None), "no metadata yet");
        assert!(
            !is_identified(&loaded, Some(&info_for(None))),
            "metadata that does not say which track it describes"
        );
        // A deck holding no track has no `loaded` to pass: the question
        // does not apply to it, and the signature cannot ask it.
    }

    #[test]
    fn snapshots_from_before_tracks_named_their_media_still_load() {
        let deck: DeckState = serde_json::from_str(
            r#"{"deckId":1,"playState":"playing","isMaster":false,"isSynced":false,
                "isOnAir":false,"pitch":0,"trackBpm":null,"effectiveBpm":null,
                "beatInMeasure":null,"trackId":15,"playheadMs":null,"reportedAt":0}"#,
        )
        .expect("an older deck state deserializes");
        assert_eq!(deck.loaded_track, None);
    }
}
