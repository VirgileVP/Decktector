//! Decktector's application shell: opens the window, starts an engine and
//! bridges it to the renderer.
//!
//! Everything domain-related lives in `decktector-core`; this crate only
//! wires it to Tauri.

mod devices;
mod download;
mod events;
mod lineup_file;
mod remote;
mod timecode;
mod track_log;
mod window_prefs;

use std::sync::Arc;

use decktector_core::model::EngineSnapshot;
use decktector_core::sim::SimEngine;
use decktector_core::Engine;
use decktector_remote::RemoteService;
use decktector_timecode::TimecodeService;
use prolink::discovery::DeviceWatcher;
use prolink_engine::{InterfacePreference, ProlinkEngine};
use tauri::{Manager, State, Window};

use window_prefs::{SleepBlocker, WindowPrefs};

/// Default number of simulated decks when none is requested.
const DEFAULT_SIM_DECKS: u8 = 2;

/// Label of the window declared in `tauri.conf.json`. Every other window is
/// a detachable panel the renderer opens on its own.
const MAIN_WINDOW_LABEL: &str = "main";

/// Whether the window labelled `label` going away ends the app's session,
/// and with it the engine. Only the main window's does: a panel closed or
/// docked back must leave the engine running for the windows still open —
/// stopping it there silenced presence, so players stopped sending status
/// and every deck froze until the app was restarted.
fn ends_session(label: &str) -> bool {
    label == MAIN_WINDOW_LABEL
}

/// Current state of everything, for hydrating a fresh renderer.
#[tauri::command]
fn engine_snapshot(engine: State<'_, Arc<dyn Engine>>) -> EngineSnapshot {
    engine.handle().snapshot()
}

/// Applies the window preferences the settings panel owns.
#[tauri::command]
fn set_window_prefs(
    window: Window,
    blocker: State<'_, SleepBlocker>,
    prefs: WindowPrefs,
) -> Result<(), String> {
    window
        .set_always_on_top(prefs.always_on_top)
        .map_err(|error| error.to_string())?;
    blocker
        .set(prefs.prevent_sleep)
        .map_err(|error| error.to_string())
}

/// Picks the engine backing this run: the real Pro DJ Link backend by
/// default, or the scripted simulation when `DECKTECTOR_SIM=1` — set by
/// `npm run dev:sim`/`dev:sim4`, or by hand when developing the UI without
/// hardware nearby. `device_watcher` is shared with the real engine rather
/// than each owning its own — both just want the same "who's on the
/// network" read (see `prolink_engine::ProlinkEngine`'s doc comment).
/// `interface_preference` is likewise shared with the settings command that
/// sets it; the simulation has no network and ignores it.
fn create_engine(
    device_watcher: Arc<DeviceWatcher>,
    interface_preference: Arc<InterfacePreference>,
) -> Arc<dyn Engine> {
    if std::env::var("DECKTECTOR_SIM").as_deref() == Ok("1") {
        let deck_count = std::env::var("DECKTECTOR_SIM_DECKS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_SIM_DECKS);
        return Arc::new(SimEngine::new(deck_count));
    }
    Arc::new(ProlinkEngine::new(device_watcher, interface_preference))
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "decktector=info,decktector_core=info".into()),
        )
        .init();

    let device_watcher = DeviceWatcher::new();
    let interface_preference = InterfacePreference::new();
    let engine = create_engine(
        Arc::clone(&device_watcher),
        Arc::clone(&interface_preference),
    );
    let timecode = TimecodeService::new(Arc::clone(&engine));

    tauri::Builder::default()
        .manage(Arc::clone(&engine))
        .manage(Arc::clone(&timecode))
        .manage(Arc::clone(&device_watcher))
        .manage(interface_preference)
        .manage(SleepBlocker::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            engine_snapshot,
            set_window_prefs,
            timecode::timecode_status,
            timecode::set_timecode_source,
            timecode::set_timecode_config,
            timecode::list_network_interfaces,
            timecode::list_midi_output_ports,
            timecode::list_ltc_output_devices,
            remote::remote_status,
            remote::set_remote_config,
            remote::remote_qr_svg,
            remote::regenerate_remote_token,
            devices::list_devices,
            devices::set_prolink_interface,
            download::download_deck_track,
            lineup_file::export_lineup_file,
            lineup_file::import_lineup_file
        ])
        .setup(move |app| {
            // Resolving the asset source needs the resource-dir lookup,
            // which only exists once the app handle is available — this is
            // the earliest point that is true, so the service is built here
            // rather than alongside `timecode` and `device_watcher` above.
            let remote = RemoteService::new(
                Arc::clone(&engine),
                remote::resolve_asset_source(app.handle()),
            );
            app.manage(Arc::clone(&remote));

            events::forward_to_renderer(app.handle().clone(), Arc::clone(&engine));
            timecode::forward_to_renderer(app.handle().clone(), Arc::clone(&timecode));
            remote::forward_to_renderer(app.handle().clone(), Arc::clone(&remote));
            devices::forward_to_renderer(app.handle().clone(), Arc::clone(&device_watcher));
            track_log::attach(app.handle().clone(), Arc::clone(&engine));

            let engine = Arc::clone(&engine);
            tauri::async_runtime::spawn(async move {
                if let Err(error) = engine.start().await {
                    tracing::error!(%error, "failed to start engine");
                }
            });
            tauri::async_runtime::spawn(Arc::clone(&timecode).run());
            tauri::async_runtime::spawn(remote.hub().run());
            tauri::async_runtime::spawn(Arc::clone(&device_watcher).run());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Releasing the network sockets on the way out is the engine's
            // job; the main window closing is the only signal we get for it.
            if matches!(event, tauri::WindowEvent::Destroyed) && ends_session(window.label()) {
                let engine = window.state::<Arc<dyn Engine>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = engine.stop().await {
                        tracing::error!(%error, "failed to stop engine");
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to start Decktector");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_main_window_closing_ends_the_session() {
        assert!(ends_session(MAIN_WINDOW_LABEL));
        // Panel labels as the renderer builds them: `panel-<id>-<deck|x>-<n>`.
        assert!(!ends_session("panel-deck-2-0"));
        assert!(!ends_session("panel-clock-x-1"));
    }
}
