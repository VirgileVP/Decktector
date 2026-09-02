//! The message shape carried over the remote WebSocket.
//!
//! One envelope, tagged with the channel the renderer subscribes to. The
//! channel names come from [`EngineEvent::channel`], so the engine layer
//! stays the single definition of the contract — the same strings the
//! Tauri bridge emits on.

use decktective_core::model::{EngineSnapshot, TrackInfo};
use decktective_core::EngineEvent;
use serde::{Deserialize, Serialize};

/// Channel carrying the hydration message sent right after a browser
/// connects. It has no [`EngineEvent`] counterpart because the Tauri
/// renderer hydrates through the `engine_snapshot` command instead — the
/// remote transport has no equivalent command, so the snapshot travels on
/// the socket itself, first.
pub const SNAPSHOT_CHANNEL: &str = "engine:snapshot";

/// One message on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMessage {
    pub channel: String,
    pub payload: serde_json::Value,
}

/// Serializes an engine event onto its renderer channel.
///
/// Matches every [`EngineEvent`] variant explicitly, with no `_ =>` arm, so
/// adding a variant to `EngineEvent` fails to compile here instead of
/// silently dropping it from the remote stream.
pub fn from_event(event: &EngineEvent) -> anyhow::Result<RemoteMessage> {
    let payload = match event {
        EngineEvent::NetworkState(state) => serde_json::to_value(state)?,
        EngineEvent::DeviceConnected(device) => serde_json::to_value(device)?,
        EngineEvent::DeviceDisconnected(device) => serde_json::to_value(device)?,
        EngineEvent::DeckState(deck) => serde_json::to_value(deck)?,
        EngineEvent::DeckTrack(track) => {
            let mut track = (**track).clone();
            track.track = track.track.map(strip_heavy_payload);
            serde_json::to_value(&track)?
        }
    };
    Ok(RemoteMessage {
        channel: event.channel().to_string(),
        payload,
    })
}

/// Serializes a full snapshot onto [`SNAPSHOT_CHANNEL`], with every loaded
/// track's heavy payload stripped the same way [`from_event`] strips a live
/// [`EngineEvent::DeckTrack`] — a snapshot is exactly "the current
/// `DeckTrack` for every deck, all at once", so it carries the same weight
/// and needs the same treatment. Leaving it in would undo the whole point of
/// stripping: a fresh browser hydrating from a four-deck snapshot is the
/// moment this matters most.
pub fn from_snapshot(snapshot: &EngineSnapshot) -> anyhow::Result<RemoteMessage> {
    let mut snapshot = snapshot.clone();
    for deck_track in &mut snapshot.tracks {
        deck_track.track = deck_track.track.take().map(strip_heavy_payload);
    }
    Ok(RemoteMessage {
        channel: SNAPSHOT_CHANNEL.to_string(),
        payload: serde_json::to_value(&snapshot)?,
    })
}

/// Drops the artwork and waveform from a track before it goes on the
/// WebSocket — both are fetched separately over plain HTTP
/// (`/track/{id}/artwork` and `/track/{id}/waveform`), immutable per
/// `track_id` and cacheable by the browser, instead of riding along on
/// every message. A single waveform runs into the megabytes; the desktop
/// path has no network between engine and view, so this stripping is
/// remote-only — the Tauri bridge in `events.rs` sends `EngineEvent`
/// straight through, untouched.
fn strip_heavy_payload(mut track: TrackInfo) -> TrackInfo {
    track.artwork = None;
    track.waveform = None;
    track
}

#[cfg(test)]
mod tests {
    use super::*;
    use decktective_core::model::{DeckState, DeckTrack, DeviceInfo, NetworkState, PlayState};

    fn every_event() -> Vec<EngineEvent> {
        vec![
            EngineEvent::NetworkState(NetworkState::Connected),
            EngineEvent::DeviceConnected(DeviceInfo {
                deck_id: 1,
                name: "CDJ-1".to_string(),
                ip: "192.168.1.1".to_string(),
            }),
            EngineEvent::DeviceDisconnected(DeviceInfo {
                deck_id: 1,
                name: "CDJ-1".to_string(),
                ip: "192.168.1.1".to_string(),
            }),
            EngineEvent::DeckState(DeckState {
                deck_id: 1,
                play_state: PlayState::Playing,
                is_master: false,
                is_synced: false,
                is_on_air: false,
                pitch: 0.0,
                track_bpm: None,
                effective_bpm: None,
                beat_in_measure: None,
                track_id: None,
                playhead_ms: None,
                reported_at: 0.0,
            }),
        ]
    }

    #[test]
    fn every_event_variant_keeps_its_engine_channel_name() {
        for event in every_event() {
            let message = from_event(&event).expect("event serializes");
            assert_eq!(message.channel, event.channel());
        }
    }

    #[test]
    fn deck_state_round_trips_through_the_wire_payload() {
        let deck = DeckState {
            deck_id: 4,
            play_state: PlayState::Paused,
            is_master: true,
            is_synced: true,
            is_on_air: false,
            pitch: 1.5,
            track_bpm: Some(128.0),
            effective_bpm: Some(129.9),
            beat_in_measure: Some(2),
            track_id: Some(7),
            playhead_ms: Some(12_345.0),
            reported_at: 999.0,
        };
        let event = EngineEvent::DeckState(deck.clone());

        let message = from_event(&event).expect("event serializes");
        let round_tripped: DeckState =
            serde_json::from_value(message.payload).expect("payload deserializes");

        assert_eq!(round_tripped, deck);
    }

    fn track_with_heavy_payload() -> TrackInfo {
        use decktective_core::sim::{build_sim_track, SIM_TRACKS};

        let mut track = build_sim_track(&SIM_TRACKS[0], 1);
        assert!(
            track.waveform.is_some(),
            "fixture needs a waveform to prove it gets stripped"
        );
        track.artwork = Some("data:image/jpeg;base64,/9k=".to_string());
        track
    }

    #[test]
    fn a_deck_track_event_strips_artwork_and_waveform_before_the_wire() {
        let event = EngineEvent::DeckTrack(Box::new(DeckTrack {
            deck_id: 1,
            track: Some(track_with_heavy_payload()),
        }));

        let message = from_event(&event).expect("event serializes");
        let wire_track: DeckTrack =
            serde_json::from_value(message.payload).expect("payload deserializes");

        let track = wire_track
            .track
            .expect("track is still present, just lighter");
        assert!(track.artwork.is_none());
        assert!(track.waveform.is_none());
    }

    #[test]
    fn a_snapshot_strips_artwork_and_waveform_from_every_loaded_track() {
        let snapshot = EngineSnapshot {
            network_state: NetworkState::Connected,
            devices: Vec::new(),
            decks: Vec::new(),
            tracks: vec![DeckTrack {
                deck_id: 1,
                track: Some(track_with_heavy_payload()),
            }],
        };

        let message = from_snapshot(&snapshot).expect("snapshot serializes");
        let wire_snapshot: EngineSnapshot =
            serde_json::from_value(message.payload).expect("payload deserializes");

        let track = wire_snapshot.tracks[0]
            .track
            .as_ref()
            .expect("track is still present, just lighter");
        assert!(track.artwork.is_none());
        assert!(track.waveform.is_none());
    }
}
