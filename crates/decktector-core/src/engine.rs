//! The single contract between the engine layer and the app.
//!
//! Implementations report through [`EngineHandle`], which keeps the snapshot
//! coherent and broadcasts the matching event. The Tauri layer forwards those
//! events to the renderer verbatim.

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::model::{
    DeckId, DeckState, DeckTrack, DeviceInfo, EngineSnapshot, NetworkState, TrackInfo,
};

/// How many events the broadcast channel buffers per subscriber.
///
/// Deck state arrives at ~5 Hz per deck, so six players produce ~30 events a
/// second; this holds several seconds of backlog. A subscriber that still
/// falls behind receives [`broadcast::error::RecvError::Lagged`] and should
/// re-hydrate from [`EngineHandle::snapshot`] rather than try to catch up.
const EVENT_BUFFER: usize = 256;

/// Everything an engine reports, one variant per renderer channel.
///
/// The track payload is boxed: it carries a whole waveform, and every event
/// is cloned once per subscriber on its way out.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    NetworkState(NetworkState),
    DeviceConnected(DeviceInfo),
    DeviceDisconnected(DeviceInfo),
    DeckState(DeckState),
    DeckTrack(Box<DeckTrack>),
}

impl EngineEvent {
    /// Name of the channel this event is delivered on.
    ///
    /// These strings are the renderer's subscription keys; they match the
    /// Electron IPC channels one-for-one so the UI did not have to change.
    pub fn channel(&self) -> &'static str {
        match self {
            Self::NetworkState(_) => "engine:network:state",
            Self::DeviceConnected(_) => "engine:device:connected",
            Self::DeviceDisconnected(_) => "engine:device:disconnected",
            Self::DeckState(_) => "engine:deck:state",
            Self::DeckTrack(_) => "engine:deck:track",
        }
    }
}

/// Snapshot bookkeeping shared by all engines.
#[derive(Debug, Default)]
struct Bookkeeping {
    network_state: NetworkState,
    devices: BTreeMap<DeckId, DeviceInfo>,
    decks: BTreeMap<DeckId, DeckState>,
    tracks: BTreeMap<DeckId, Option<TrackInfo>>,
}

/// The reporting side of an engine: implementations call these methods, and
/// the handle keeps the snapshot coherent and broadcasts the event.
///
/// Cloneable and `Send + Sync`, so a running engine can hand it to as many
/// listener tasks as it needs.
#[derive(Debug)]
pub struct EngineHandle {
    state: Mutex<Bookkeeping>,
    events: broadcast::Sender<EngineEvent>,
}

impl Default for EngineHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineHandle {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(Bookkeeping::default()),
            events: broadcast::channel(EVENT_BUFFER).0,
        }
    }

    /// Subscribes to the event stream. Events emitted before this call are
    /// not replayed — pair it with [`Self::snapshot`] to hydrate.
    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    /// Current state of everything, for hydrating a fresh renderer.
    pub fn snapshot(&self) -> EngineSnapshot {
        let state = self.lock();
        EngineSnapshot {
            network_state: state.network_state,
            devices: state.devices.values().cloned().collect(),
            decks: state.decks.values().cloned().collect(),
            tracks: state
                .tracks
                .iter()
                .map(|(&deck_id, track)| DeckTrack {
                    deck_id,
                    track: track.clone(),
                })
                .collect(),
        }
    }

    pub fn set_network_state(&self, network_state: NetworkState) {
        self.lock().network_state = network_state;
        self.emit(EngineEvent::NetworkState(network_state));
    }

    pub fn device_connected(&self, device: DeviceInfo) {
        self.lock().devices.insert(device.deck_id, device.clone());
        self.emit(EngineEvent::DeviceConnected(device));
    }

    /// Drops the device along with the deck state and track it owned — a
    /// player that left the network must not linger in the UI.
    pub fn device_disconnected(&self, device: DeviceInfo) {
        {
            let mut state = self.lock();
            state.devices.remove(&device.deck_id);
            state.decks.remove(&device.deck_id);
            state.tracks.remove(&device.deck_id);
        }
        self.emit(EngineEvent::DeviceDisconnected(device));
    }

    pub fn update_deck_state(&self, deck: DeckState) {
        self.lock().decks.insert(deck.deck_id, deck.clone());
        self.emit(EngineEvent::DeckState(deck));
    }

    pub fn update_deck_track(&self, deck_id: DeckId, track: Option<TrackInfo>) {
        self.lock().tracks.insert(deck_id, track.clone());
        self.emit(EngineEvent::DeckTrack(Box::new(DeckTrack {
            deck_id,
            track,
        })));
    }

    /// Whether a track is currently held in state for `deck_id`.
    pub fn has_track(&self, deck_id: DeckId) -> bool {
        self.lock()
            .tracks
            .get(&deck_id)
            .and_then(Option::as_ref)
            .is_some()
    }

    /// A send failure only means nobody is subscribed yet, which is normal
    /// while the window is still opening.
    fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }

    /// The bookkeeping mutex is only ever held for a field update, so a
    /// panic while holding it is not recoverable state we want to preserve.
    fn lock(&self) -> std::sync::MutexGuard<'_, Bookkeeping> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A source of Pro DJ Link deck data.
///
/// Implementations: `prolink_engine::ProlinkEngine` (real hardware) and
/// [`crate::sim::SimEngine`] (scripted decks for development without any).
#[async_trait]
pub trait Engine: Send + Sync {
    async fn start(&self) -> anyhow::Result<()>;
    async fn stop(&self) -> anyhow::Result<()>;
    /// The handle carrying this engine's snapshot and event stream.
    fn handle(&self) -> &EngineHandle;

    /// Downloads the audio file of the track currently loaded on `deck_id` into `destination_dir`.
    /// Invokes `progress_callback(bytes_downloaded, total_bytes)` during the transfer.
    /// Returns the path of the saved file.
    async fn download_track(
        &self,
        deck_id: DeckId,
        destination_dir: std::path::PathBuf,
        progress_callback: Box<dyn Fn(u64, u64) + Send + Sync>,
    ) -> anyhow::Result<std::path::PathBuf> {
        let _ = (deck_id, destination_dir, progress_callback);
        anyhow::bail!("track download is not supported by this engine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(deck_id: DeckId) -> DeviceInfo {
        DeviceInfo {
            deck_id,
            name: format!("CDJ-{deck_id}"),
            ip: format!("192.168.1.{deck_id}"),
        }
    }

    #[test]
    fn snapshot_starts_empty_and_offline() {
        let snapshot = EngineHandle::new().snapshot();

        assert_eq!(snapshot.network_state, NetworkState::Offline);
        assert!(snapshot.devices.is_empty());
        assert!(snapshot.decks.is_empty());
    }

    #[test]
    fn disconnecting_a_device_clears_its_deck_and_track() {
        let handle = EngineHandle::new();
        handle.device_connected(device(2));
        handle.update_deck_track(2, None);

        handle.device_disconnected(device(2));

        let snapshot = handle.snapshot();
        assert!(snapshot.devices.is_empty());
        assert!(snapshot.tracks.is_empty());
    }

    #[test]
    fn devices_are_reported_in_deck_order() {
        let handle = EngineHandle::new();
        handle.device_connected(device(3));
        handle.device_connected(device(1));

        let ids: Vec<_> = handle
            .snapshot()
            .devices
            .iter()
            .map(|device| device.deck_id)
            .collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn events_carry_the_renderer_channel_names() {
        let handle = EngineHandle::new();
        let mut events = handle.subscribe();

        handle.set_network_state(NetworkState::Connected);

        let event = events.try_recv().expect("event was broadcast");
        assert_eq!(event.channel(), "engine:network:state");
    }

    #[test]
    fn has_track_reports_accurate_state() {
        let handle = EngineHandle::new();
        assert!(!handle.has_track(1));

        let track = TrackInfo {
            track_id: 1234,
            title: "Test Track".into(),
            artist: Some("Test Artist".into()),
            album: None,
            genre: None,
            key: None,
            label: None,
            comment: None,
            duration_ms: 120_000.0,
            bpm: Some(120.0),
            artwork: None,
            beat_grid: None,
            waveform: None,
            cues: vec![],
            events: vec![],
        };

        handle.update_deck_track(1, Some(track));
        assert!(handle.has_track(1));

        handle.update_deck_track(1, None);
        assert!(!handle.has_track(1));
    }
}
