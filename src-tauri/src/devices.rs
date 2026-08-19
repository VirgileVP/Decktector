//! Bridges [`prolink::discovery::DeviceWatcher`] to the renderer's Devices
//! settings panel: one event channel for list changes, and the command that
//! hydrates a freshly opened panel.
//!
//! Mirrors `timecode.rs` — the watcher is the single source of truth, this
//! module only forwards it.

use std::sync::Arc;

use prolink::discovery::{DeviceWatcher, DiscoveredDevice};
use tauri::{AppHandle, Emitter, State};

/// Renderer's subscription key for device-list changes.
const DEVICES_CHANNEL: &str = "devices:list";

/// Subscribes to the watcher and forwards its device list for the lifetime
/// of the app.
pub fn forward_to_renderer(app: AppHandle, watcher: Arc<DeviceWatcher>) {
    let mut devices = watcher.subscribe();

    tauri::async_runtime::spawn(async move {
        // Whether the very first `changed()` resolves immediately with the
        // watcher's current value depends on watch-channel internals we
        // shouldn't lean on — the renderer hydrates explicitly via
        // `list_devices` instead, the same split `timecode.rs` uses.
        while devices.changed().await.is_ok() {
            let snapshot = devices.borrow_and_update().clone();
            if let Err(error) = app.emit(DEVICES_CHANNEL, snapshot) {
                tracing::error!(%error, "failed to emit the Pro DJ Link device list");
            }
        }
    });
}

/// Current device list, for hydrating a freshly opened settings panel.
#[tauri::command]
pub fn list_devices(watcher: State<'_, Arc<DeviceWatcher>>) -> Vec<DiscoveredDevice> {
    watcher.devices()
}
