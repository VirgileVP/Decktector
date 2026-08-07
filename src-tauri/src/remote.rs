//! Bridges [`decktective_remote::RemoteService`] to the renderer: one event
//! channel for status changes, and the command the settings panel calls to
//! turn the remote view on or off.
//!
//! Mirrors `timecode.rs` — the service is the single source of truth, this
//! module only forwards it.

use std::sync::Arc;

use decktective_remote::{RemoteConfig, RemoteService, RemoteStatus};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::broadcast::error::RecvError;

/// Renderer's subscription key for [`RemoteStatus`] changes.
const STATUS_CHANNEL: &str = "remote:status";

/// Subscribes to the service and forwards its status changes for the
/// lifetime of the app.
pub fn forward_to_renderer(app: AppHandle, service: Arc<RemoteService>) {
    let mut statuses = service.subscribe();

    tauri::async_runtime::spawn(async move {
        loop {
            match statuses.recv().await {
                Ok(status) => {
                    if let Err(error) = app.emit(STATUS_CHANNEL, status) {
                        tracing::error!(%error, "failed to emit a remote status change");
                    }
                }
                // The renderer re-hydrates from `remote_status` on the next
                // change, so a dropped burst costs freshness, not coherence.
                Err(RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "renderer bridge fell behind the remote service")
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

/// Current status, for hydrating a fresh renderer.
#[tauri::command]
pub fn remote_status(service: State<'_, Arc<RemoteService>>) -> RemoteStatus {
    service.status()
}

/// Applies the remote view settings chosen in the settings panel: turns the
/// server on or off, or moves it to a different port.
#[tauri::command]
pub async fn set_remote_config(
    service: State<'_, Arc<RemoteService>>,
    config: RemoteConfig,
) -> Result<(), String> {
    service
        .inner()
        .clone()
        .set_config(config)
        .await
        .map_err(|error| error.to_string())
}
