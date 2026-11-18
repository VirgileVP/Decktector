//! Bridges [`decktector_timecode::TimecodeService`] to the renderer: one
//! event channel for status ticks, one for deck matches, and the commands the
//! settings panel, the deck source picker and the show editor call.
//!
//! Mirrors `events.rs` — the service is the single source of truth, this
//! module only forwards it.

use std::sync::Arc;

use decktector_core::model::DeckId;
use decktector_timecode::network::{self, NetworkInterfaceInfo};
use decktector_timecode::{
    ltc, mtc, DeckMatchStatus, TimecodeConfig, TimecodeService, TimecodeShow, TimecodeStatus,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::broadcast::{self, error::RecvError};

/// Renderer's subscription key for [`TimecodeStatus`] ticks.
const STATUS_CHANNEL: &str = "timecode:status";

/// Renderer's subscription key for every deck's [`DeckMatchStatus`].
const DECK_MATCHES_CHANNEL: &str = "timecode:deck-matches";

/// Subscribes to the service and forwards its status ticks and deck matches
/// for the lifetime of the app.
pub fn forward_to_renderer(app: AppHandle, service: Arc<TimecodeService>) {
    forward(app.clone(), service.subscribe(), STATUS_CHANNEL);
    forward(app, service.subscribe_deck_matches(), DECK_MATCHES_CHANNEL);
}

/// Emits everything `updates` carries on `channel` until the service goes away.
fn forward<T>(app: AppHandle, mut updates: broadcast::Receiver<T>, channel: &'static str)
where
    T: Clone + Serialize + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        loop {
            match updates.recv().await {
                Ok(update) => {
                    if let Err(error) = app.emit(channel, update) {
                        tracing::error!(%error, channel, "failed to emit a timecode update");
                    }
                }
                // The renderer re-hydrates from the matching command on the
                // next update, so a dropped burst costs freshness, not coherence.
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(
                        missed,
                        channel,
                        "renderer bridge fell behind the timecode service"
                    )
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

/// Current status, for hydrating a fresh renderer.
#[tauri::command]
pub fn timecode_status(service: State<'_, Arc<TimecodeService>>) -> TimecodeStatus {
    service.status()
}

/// Picks which deck's playhead the generator reads. `None` stops output
/// without touching the protocol config, so re-enabling it needs one click.
#[tauri::command]
pub fn set_timecode_source(service: State<'_, Arc<TimecodeService>>, deck_id: Option<DeckId>) {
    service.set_source(deck_id);
}

/// Applies the protocol and frame rate chosen in the settings panel.
#[tauri::command]
pub fn set_timecode_config(service: State<'_, Arc<TimecodeService>>, config: TimecodeConfig) {
    service.set_config(config);
}

/// Replaces the show table the decks are matched against; `None` unloads it.
#[tauri::command]
pub fn set_timecode_show(service: State<'_, Arc<TimecodeService>>, show: Option<TimecodeShow>) {
    service.set_show(show);
}

/// Every deck's current match, for hydrating a fresh renderer.
#[tauri::command]
pub fn timecode_deck_matches(service: State<'_, Arc<TimecodeService>>) -> Vec<DeckMatchStatus> {
    service.deck_matches()
}

/// Network adapters the settings panel can offer for Art-Net output.
#[tauri::command]
pub fn list_network_interfaces() -> Result<Vec<NetworkInterfaceInfo>, String> {
    network::list_broadcastable_interfaces().map_err(|error| error.to_string())
}

/// MIDI output ports the settings panel can offer for MTC output.
#[tauri::command]
pub fn list_midi_output_ports() -> Result<Vec<String>, String> {
    mtc::list_output_ports().map_err(|error| error.to_string())
}

/// Audio output devices the settings panel can offer for LTC output.
#[tauri::command]
pub fn list_ltc_output_devices() -> Result<Vec<String>, String> {
    ltc::list_output_devices().map_err(|error| error.to_string())
}
