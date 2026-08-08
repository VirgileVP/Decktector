//! Bridges [`decktective_remote::RemoteService`] to the renderer: one event
//! channel for status changes, and the command the settings panel calls to
//! turn the remote view on or off.
//!
//! Mirrors `timecode.rs` — the service is the single source of truth, this
//! module only forwards it.

use std::sync::Arc;

use decktective_remote::{AssetSource, RemoteConfig, RemoteService, RemoteStatus};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast::error::RecvError;

/// Renderer's subscription key for [`RemoteStatus`] changes.
const STATUS_CHANNEL: &str = "remote:status";

/// Vite's fixed dev-server port — matches `vite.config.ts`'s `DEV_PORT` and
/// `tauri.conf.json`'s `devUrl`, which both hardcode the same value for the
/// same reason: Tauri's `devUrl` has to point at a fixed port.
const VITE_DEV_PORT: u16 = 1420;

/// Where the remote server should get the UI it hands to browsers: the
/// packaged app's bundled `web` resource (see `tauri.conf.json`'s
/// `bundle.resources`) if it exists, otherwise a redirect to the Vite dev
/// server that owns the assets during `tauri dev`.
pub fn resolve_asset_source(app: &AppHandle) -> AssetSource {
    match app.path().resource_dir() {
        Ok(resource_dir) => {
            let web_dir = resource_dir.join("web");
            if web_dir.is_dir() {
                tracing::info!(
                    path = %web_dir.display(),
                    "serving the remote view from the bundled web resource"
                );
                return AssetSource::Directory(web_dir);
            }
        }
        Err(error) => {
            tracing::warn!(%error, "failed to resolve the app's resource directory");
        }
    }
    tracing::info!(
        port = VITE_DEV_PORT,
        "no bundled web resource found; redirecting the remote view to the Vite dev server"
    );
    AssetSource::DevServer(VITE_DEV_PORT)
}

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

/// Renders the remote view URL the settings panel already built as a QR
/// code, so it drops straight into an `<img>` as a `data:` URL.
#[tauri::command]
pub fn remote_qr_svg(url: String) -> Result<String, String> {
    decktective_remote::qr::qr_svg(&url).map_err(|error| error.to_string())
}
