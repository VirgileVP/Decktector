//! Fans engine events out to every connected remote browser.
//!
//! Subscribes to the [`EngineHandle`] once and serializes each event to
//! text a single time, then broadcasts that text to as many WebSocket
//! connections as are open. Serializing once instead of once per client is
//! the point: a [`TrackInfo`] carries a whole waveform, and a busy night can
//! have several browsers watching at once.
//!
//! [`TrackInfo`]: decktective_core::model::TrackInfo

use std::sync::Arc;

use decktective_core::Engine;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::wire;

/// How many serialized messages a slow browser may fall behind before it is
/// re-hydrated from a fresh snapshot instead of trying to catch up.
const CLIENT_BUFFER: usize = 64;

/// Pumps engine events into a broadcast channel every WebSocket connection
/// subscribes to.
pub struct RemoteHub {
    engine: Arc<dyn Engine>,
    tx: broadcast::Sender<Arc<str>>,
}

impl RemoteHub {
    pub fn new(engine: Arc<dyn Engine>) -> Arc<Self> {
        Arc::new(Self {
            engine,
            tx: broadcast::channel(CLIENT_BUFFER).0,
        })
    }

    /// Pumps engine events into the broadcast channel. Runs for the lifetime
    /// of the app; returns when the engine's event channel closes.
    pub async fn run(self: Arc<Self>) {
        let mut events = self.engine.handle().subscribe();

        loop {
            match events.recv().await {
                Ok(event) => match wire::from_event(&event) {
                    Ok(message) => self.broadcast(&message),
                    Err(error) => {
                        tracing::error!(%error, "failed to serialize an engine event for remote browsers")
                    }
                },
                // A lagging hub costs freshness, not coherence: a connection
                // that misses a burst re-hydrates from a fresh snapshot on
                // its own lag, the same way the Tauri bridge just logs it.
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "remote hub fell behind the engine")
                }
                Err(RecvError::Closed) => break,
            }
        }
    }

    /// A new subscriber, plus the snapshot text it must be sent first so it
    /// starts from a coherent state instead of a live event mid-stream.
    pub fn subscribe(&self) -> anyhow::Result<(Arc<str>, broadcast::Receiver<Arc<str>>)> {
        // Subscribe before reading the snapshot so nothing emitted in
        // between is lost — the same ordering the Tauri renderer follows.
        let receiver = self.tx.subscribe();
        let snapshot = self.snapshot_text()?;
        Ok((snapshot, receiver))
    }

    /// Current snapshot, serialized — used on connect and after a lag.
    pub fn snapshot_text(&self) -> anyhow::Result<Arc<str>> {
        let message = wire::from_snapshot(&self.engine.handle().snapshot())?;
        Ok(serde_json::to_string(&message)?.into())
    }

    fn broadcast(&self, message: &wire::RemoteMessage) {
        let text = match serde_json::to_string(message) {
            Ok(text) => text,
            Err(error) => {
                tracing::error!(%error, channel = message.channel, "failed to encode a remote message");
                return;
            }
        };
        // A send failure only means no browser is connected yet, which is
        // normal when the remote server is enabled but nobody has opened it.
        let _ = self.tx.send(text.into());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use decktective_core::model::{DeckState, PlayState};
    use decktective_core::sim::SimEngine;

    use super::*;

    fn deck(deck_id: decktective_core::model::DeckId) -> DeckState {
        DeckState {
            deck_id,
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
        }
    }

    #[tokio::test]
    async fn subscriber_receives_a_deck_state_event_within_a_bounded_wait() {
        let engine: Arc<dyn Engine> = Arc::new(SimEngine::new(2));
        let hub = RemoteHub::new(engine.clone());
        let hub_task = tokio::spawn(hub.clone().run());
        // Let `run()` reach its `recv().await` — and so subscribe to the
        // engine — before the event below is emitted, otherwise the hub
        // would never see it (a `broadcast` receiver only gets events sent
        // after it subscribes).
        tokio::task::yield_now().await;

        let (_snapshot, mut receiver) = hub.subscribe().expect("hub has a snapshot to send");

        engine.handle().update_deck_state(deck(1));

        let text = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect("a message arrived before the timeout")
            .expect("the broadcast channel did not close");

        let message: wire::RemoteMessage = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(message.channel, "engine:deck:state");

        hub_task.abort();
    }

    #[test]
    fn snapshot_text_parses_back_into_an_engine_snapshot() {
        let engine: Arc<dyn Engine> = Arc::new(SimEngine::new(2));
        let hub = RemoteHub::new(engine);

        let text = hub.snapshot_text().expect("snapshot serializes");
        let message: wire::RemoteMessage = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(message.channel, wire::SNAPSHOT_CHANNEL);

        let _snapshot: decktective_core::model::EngineSnapshot =
            serde_json::from_value(message.payload).expect("payload is a valid snapshot");
    }
}
