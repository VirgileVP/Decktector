//! Decktective's application shell: opens the window, starts an engine and
//! bridges it to the renderer.
//!
//! Everything domain-related lives in `decktective-core`; this crate only
//! wires it to Tauri.

mod events;
mod window_prefs;

use std::sync::Arc;

use decktective_core::model::EngineSnapshot;
use decktective_core::sim::SimEngine;
use decktective_core::Engine;
use tauri::{Manager, State, Window};

use window_prefs::{SleepBlocker, WindowPrefs};

/// Default number of simulated decks when none is requested.
const DEFAULT_SIM_DECKS: u8 = 2;

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

/// Picks the engine backing this run.
///
/// Only the simulation is available so far — the Pro DJ Link backend lands
/// with the `prolink` crate in a later phase. Saying so out loud beats
/// letting someone plug into a booth and wonder why every deck is fictional.
fn create_engine() -> Arc<dyn Engine> {
    let deck_count = std::env::var("DECKTECTIVE_SIM_DECKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_SIM_DECKS);

    if std::env::var("DECKTECTIVE_SIM").as_deref() != Ok("1") {
        tracing::warn!(
            "the Pro DJ Link backend is not implemented yet; running the simulation instead"
        );
    }
    Arc::new(SimEngine::new(deck_count))
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "decktective=info,decktective_core=info".into()),
        )
        .init();

    let engine = create_engine();

    tauri::Builder::default()
        .manage(Arc::clone(&engine))
        .manage(SleepBlocker::default())
        .invoke_handler(tauri::generate_handler![engine_snapshot, set_window_prefs])
        .setup(move |app| {
            events::forward_to_renderer(app.handle().clone(), Arc::clone(&engine));

            let engine = Arc::clone(&engine);
            tauri::async_runtime::spawn(async move {
                if let Err(error) = engine.start().await {
                    tracing::error!(%error, "failed to start engine");
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Releasing the network sockets on the way out is the engine's
            // job; the window closing is the only signal we get for it.
            if let tauri::WindowEvent::Destroyed = event {
                let engine = window.state::<Arc<dyn Engine>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = engine.stop().await {
                        tracing::error!(%error, "failed to stop engine");
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to start Decktective");
}
