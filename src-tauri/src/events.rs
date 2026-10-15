//! Bridges the engine to the renderer.
//!
//! Every engine event is re-emitted on its own Tauri channel, and the
//! `engine_snapshot` command lets a fresh renderer hydrate its store. The
//! channel names come from [`EngineEvent::channel`], so the engine layer
//! stays the single place that defines the contract.

use std::sync::Arc;

use decktector_core::{Engine, EngineEvent};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast::{self, error::RecvError};

/// Channel telling the renderer the bridge dropped events and its store may
/// be stale: it answers by re-fetching `engine_snapshot`. Matches
/// `RESYNC_CHANNEL` in `src/renderer/src/state/transport/tauri.ts`.
const RESYNC_CHANNEL: &str = "engine:resync";

/// What the bridge hands the renderer.
#[derive(Debug, PartialEq)]
enum Outbound {
    /// An engine event, forwarded verbatim.
    Event(EngineEvent),
    /// Events were dropped; the renderer must re-hydrate from the snapshot.
    Resync,
}

/// Subscribes to the engine and forwards its events for the lifetime of the
/// app.
pub fn forward_to_renderer(app: AppHandle, engine: Arc<dyn Engine>) {
    let events = engine.handle().subscribe();

    tauri::async_runtime::spawn(pump(events, move |outbound| match outbound {
        Outbound::Event(event) => emit(&app, event),
        Outbound::Resync => {
            if let Err(error) = app.emit(RESYNC_CHANNEL, ()) {
                tracing::error!(%error, "failed to ask the renderer to resync");
            }
        }
    }));
}

/// Drains `events` into `deliver` until the engine goes away. A lag never
/// tries to catch up on what was missed: a dropped `DeckTrack` would leave a
/// deck showing the previous track, so the renderer is told to re-hydrate
/// from the snapshot instead.
async fn pump(mut events: broadcast::Receiver<EngineEvent>, mut deliver: impl FnMut(Outbound)) {
    loop {
        match events.recv().await {
            Ok(event) => deliver(Outbound::Event(event)),
            Err(RecvError::Lagged(missed)) => {
                tracing::warn!(missed, "renderer bridge fell behind the engine, resyncing");
                deliver(Outbound::Resync);
            }
            Err(RecvError::Closed) => break,
        }
    }
}

fn emit(app: &AppHandle, event: EngineEvent) {
    let channel = event.channel();
    let result = match event {
        EngineEvent::NetworkState(state) => app.emit(channel, state),
        EngineEvent::DeviceConnected(device) => app.emit(channel, device),
        EngineEvent::DeviceDisconnected(device) => app.emit(channel, device),
        EngineEvent::DeckState(deck) => app.emit(channel, deck),
        EngineEvent::DeckTrack(track) => app.emit(channel, *track),
    };

    if let Err(error) = result {
        tracing::error!(%error, channel, "failed to emit engine event");
    }
}

#[cfg(test)]
mod tests {
    use decktector_core::model::NetworkState;

    use super::*;

    fn network_event() -> EngineEvent {
        EngineEvent::NetworkState(NetworkState::default())
    }

    #[tokio::test]
    async fn a_lag_asks_the_renderer_to_resync_then_keeps_forwarding() {
        let (sender, receiver) = broadcast::channel(1);
        // Two events into a one-slot buffer: the first is overwritten before
        // the bridge reads anything, so it lags.
        sender.send(network_event()).unwrap();
        sender.send(network_event()).unwrap();
        drop(sender);

        let mut delivered = Vec::new();
        pump(receiver, |outbound| delivered.push(outbound)).await;

        assert_eq!(
            delivered,
            vec![Outbound::Resync, Outbound::Event(network_event())]
        );
    }

    #[tokio::test]
    async fn events_without_a_lag_are_forwarded_as_they_come() {
        let (sender, receiver) = broadcast::channel(4);
        sender.send(network_event()).unwrap();
        drop(sender);

        let mut delivered = Vec::new();
        pump(receiver, |outbound| delivered.push(outbound)).await;

        assert_eq!(delivered, vec![Outbound::Event(network_event())]);
    }
}
