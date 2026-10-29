//! The messages carried over the control WebSocket.
//!
//! Inbound, the page sends a bare [`SimCommand`] per text frame; this module
//! only parses them. Outbound is the [`SimControlMessage`] envelope.

use decktector_core::model::{DeckId, TrackInfo};
use decktector_core::sim::{SimCommand, SimRigState};
use serde::Serialize;
use ts_rs::TS;

/// One message from the server to the control page.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "SimControlMessage.ts")]
// Built and serialized on the spot, never stored, so the size gap between the
// variants costs nothing.
#[allow(clippy::large_enum_variant)]
pub enum SimControlMessage {
    /// The rig as of its last update; sent on connect and on every change.
    State { state: SimRigState },
    /// The full track loaded on a deck (`None` once ejected) — waveform and
    /// all, which [`SimRigState`] leaves out. Sent on connect for every deck,
    /// then whenever a deck's loaded track changes.
    ///
    /// `audio_url` is where the page fetches the track's audio from, `None`
    /// when the track has no audio file (or the deck is empty).
    Track {
        deck_id: DeckId,
        track: Option<TrackInfo>,
        audio_url: Option<String>,
    },
}

/// Parses one inbound text frame as a rig gesture.
pub fn parse_command(text: &str) -> Result<SimCommand, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_command_frame_parses() {
        let command = parse_command(r#"{"type":"load","deck":2,"trackIndex":7}"#)
            .expect("a well-formed load command parses");
        assert_eq!(
            command,
            SimCommand::Load {
                deck: 2,
                track_index: 7
            }
        );
    }

    #[test]
    fn malformed_command_frames_are_errors() {
        assert!(parse_command("not json").is_err());
        assert!(parse_command(r#"{"type":"noSuchGesture","deck":1}"#).is_err());
        assert!(parse_command(r#"{"type":"playPause"}"#).is_err());
    }

    #[test]
    fn an_empty_deck_track_message_is_tagged_and_camel_cased() {
        let message = SimControlMessage::Track {
            deck_id: 1,
            track: None,
            audio_url: None,
        };
        let json = serde_json::to_value(&message).expect("a track message serializes");
        assert_eq!(
            json,
            serde_json::json!({
                "type": "track", "deckId": 1, "track": null, "audioUrl": null
            })
        );
    }

    #[test]
    fn a_track_message_carries_its_audio_url_camel_cased() {
        let message = SimControlMessage::Track {
            deck_id: 2,
            track: None,
            audio_url: Some("/audio/123".to_string()),
        };
        let json = serde_json::to_value(&message).expect("a track message serializes");
        assert_eq!(json["audioUrl"], "/audio/123");
    }

    #[test]
    fn a_state_message_nests_the_rig_state() {
        let (_engine, control) = decktector_core::sim::SimEngine::manual(
            decktector_core::sim::SimLibrary::generated(),
            None,
        );
        let message = SimControlMessage::State {
            state: control.state(),
        };
        let json = serde_json::to_value(&message).expect("a state message serializes");
        assert_eq!(json["type"], "state");
        assert!(json["state"]["decks"].is_array());
    }
}
