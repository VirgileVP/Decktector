//! Bridges [`decktector_tempo::TempoOutputService`] to the renderer: one
//! event channel for status ticks, and the commands the settings panel calls.
//!
//! Mirrors `timecode.rs` — the service is the single source of truth, this
//! module only forwards it.

use std::sync::Arc;

use decktector_tempo::{TempoOutputConfig, TempoOutputService, TempoOutputStatus};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::broadcast::error::RecvError;

/// Renderer's subscription key for [`TempoOutputStatus`] ticks.
const STATUS_CHANNEL: &str = "tempo:status";

/// Subscribes to the service and forwards its status ticks for the lifetime
/// of the app.
pub fn forward_to_renderer(app: AppHandle, service: Arc<TempoOutputService>) {
    let mut statuses = service.subscribe();

    tauri::async_runtime::spawn(async move {
        loop {
            match statuses.recv().await {
                Ok(status) => {
                    if let Err(error) = app.emit(STATUS_CHANNEL, status) {
                        tracing::error!(%error, "failed to emit a tempo output status tick");
                    }
                }
                // The renderer re-hydrates from `tempo_output_status` on the
                // next tick, so a dropped burst costs freshness, not coherence.
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(
                        missed,
                        "renderer bridge fell behind the tempo output service"
                    )
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

/// Current status, for hydrating a fresh renderer.
#[tauri::command]
pub fn tempo_output_status(service: State<'_, Arc<TempoOutputService>>) -> TempoOutputStatus {
    service.status()
}

/// Config currently applied, for a renderer that wants to show what the
/// backend actually holds.
#[tauri::command]
pub fn tempo_output_config(service: State<'_, Arc<TempoOutputService>>) -> TempoOutputConfig {
    service.config()
}

/// Applies the routing table chosen in the settings panel. Never refused:
/// what is unusable in it is left out and reported through the status.
#[tauri::command]
pub fn set_tempo_output_config(
    service: State<'_, Arc<TempoOutputService>>,
    config: TempoOutputConfig,
) {
    service.set_config(config);
}
