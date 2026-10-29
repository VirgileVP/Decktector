//! What the manual rig reports about itself: the view the control page
//! renders, and the app-facing [`DeckState`] derived from it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::command::TempoRange;
use crate::model::{ActiveLoop, DeckId, DeckState, LoopRange, PlayState, TrackInfo};

/// One entry of the sorted library the control page can load from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SimTrackSummary.ts")]
pub struct SimTrackSummary {
    /// Position in the sorted library; what [`super::SimCommand::Load`] takes.
    pub index: usize,
    pub title: String,
    pub artist: Option<String>,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub duration_ms: f64,
}

impl SimTrackSummary {
    pub(super) fn of(index: usize, track: &TrackInfo) -> Self {
        Self {
            index,
            title: track.title.clone(),
            artist: track.artist.clone(),
            bpm: track.bpm,
            key: track.key.clone(),
            duration_ms: track.duration_ms,
        }
    }
}

/// A loop stored on a deck, whether or not it is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SimLoop.ts")]
pub struct SimLoop {
    pub start_ms: f64,
    pub end_ms: f64,
    pub active: bool,
}

impl SimLoop {
    /// The loop as the rest of the app sees it, only while it is running.
    fn as_active_loop(self) -> Option<ActiveLoop> {
        self.active.then_some(ActiveLoop {
            range: Some(LoopRange {
                start_ms: self.start_ms,
                end_ms: self.end_ms,
            }),
        })
    }
}

/// Everything about one deck, including what [`DeckState`] has no room for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SimDeckView.ts")]
pub struct SimDeckView {
    pub deck_id: DeckId,
    /// Index into the sorted library of the loaded track.
    pub track_index: Option<usize>,
    pub track_id: Option<u32>,
    pub play_state: PlayState,
    pub position_ms: f64,
    pub duration_ms: Option<f64>,
    pub cue_point_ms: f64,
    /// Pitch fader position, -1 to 1.
    pub pitch_fader: f64,
    pub tempo_range: TempoRange,
    /// Pitch actually applied, in percent: the fader, or the master's tempo
    /// while synced, plus any jog nudge.
    pub effective_pitch: f64,
    pub track_bpm: Option<f64>,
    pub effective_bpm: Option<f64>,
    /// Beat within the bar (1-4), only while playing.
    pub beat_in_measure: Option<u8>,
    pub loop_state: Option<SimLoop>,
    pub is_master: bool,
    pub is_synced: bool,
    pub is_on_air: bool,
    /// The jog top is being touched.
    pub scratching: bool,
}

impl SimDeckView {
    /// The same deck as the rest of the app sees it.
    pub fn to_deck_state(&self, reported_at: f64) -> DeckState {
        DeckState {
            deck_id: self.deck_id,
            play_state: self.play_state,
            is_master: self.is_master,
            is_synced: self.is_synced,
            is_on_air: self.is_on_air,
            pitch: self.effective_pitch,
            track_bpm: self.track_bpm,
            effective_bpm: self.effective_bpm,
            beat_in_measure: self.beat_in_measure,
            track_id: self.track_id,
            playhead_ms: self.track_index.map(|_| self.position_ms),
            reported_at,
            active_loop: self.loop_state.and_then(SimLoop::as_active_loop),
        }
    }
}

/// The whole rig at one instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "SimRigState.ts")]
pub struct SimRigState {
    pub decks: Vec<SimDeckView>,
    /// Channel fader level per deck, in deck order.
    pub channel_faders: Vec<f64>,
    pub library: Vec<SimTrackSummary>,
    /// Set when the library the user asked for could not be used and a
    /// stand-in is playing instead.
    pub fallback_warning: Option<String>,
    /// Epoch ms at which the positions were computed.
    pub reported_at: f64,
}
