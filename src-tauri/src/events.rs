//! Bridges the engine to the renderer.
//!
//! Every engine event is re-emitted on its own Tauri channel, and the
//! `engine_snapshot` command lets a fresh renderer hydrate its store. The
//! channel names come from [`EngineEvent::channel`], so the engine layer
//! stays the single place that defines the contract.

use std::sync::Arc;

use decktective_core::{Engine, EngineEvent};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast::error::RecvError;

/// Subscribes to the engine and forwards its events for the lifetime of the
/// app.
pub fn forward_to_renderer(app: AppHandle, engine: Arc<dyn Engine>) {
    let mut events = engine.handle().subscribe();

    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => emit(&app, event),
                // The renderer re-hydrates from the snapshot on the next
                // event, so a dropped burst costs freshness, not coherence.
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "renderer bridge fell behind the engine")
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
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
