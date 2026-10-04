//! Bridges [`decktector_timecode::TimecodeService`] to the renderer: one
//! event channel for status ticks, and the commands the settings panel and
//! the deck source picker call.
//!
//! Mirrors `events.rs` — the service is the single source of truth, this
//! module only forwards it.

use std::sync::Arc;

use decktector_core::model::DeckId;
use decktector_timecode::network::{self, NetworkInterfaceInfo};
use decktector_timecode::{ltc, mtc, TimecodeConfig, TimecodeService, TimecodeStatus};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::broadcast::error::RecvError;

/// Renderer's subscription key for [`TimecodeStatus`] ticks.
const STATUS_CHANNEL: &str = "timecode:status";

/// Subscribes to the service and forwards its status ticks for the lifetime
/// of the app.
pub fn forward_to_renderer(app: AppHandle, service: Arc<TimecodeService>) {
    let mut statuses = service.subscribe();

    tauri::async_runtime::spawn(async move {
        loop {
            match statuses.recv().await {
                Ok(status) => {
                    if let Err(error) = app.emit(STATUS_CHANNEL, status) {
                        tracing::error!(%error, "failed to emit a timecode status tick");
                    }
                }
                // The renderer re-hydrates from `timecode_status` on the next
                // tick, so a dropped burst costs freshness, not coherence.
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "renderer bridge fell behind the timecode service")
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
