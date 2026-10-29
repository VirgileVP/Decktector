//! Builds the simulated engine `DECKTECTOR_SIM=1` asks for, and — in manual
//! mode — the control page that drives it.
//!
//! Two modes share the same simulated decks:
//! - auto (the default, `npm run dev:sim:recorded`): the decks play and act
//!   on their own, as `decktector_core::sim` scripts them;
//! - manual (`DECKTECTOR_SIM_MODE=manual`, `npm run dev:sim`): four decks do
//!   nothing until someone drives them from the CDJ page that
//!   `decktector_sim_control` serves, with tracks from a rekordbox export.
//!
//! The export is loaded here rather than in `decktector-core`, which cannot
//! depend on the `prolink` parsers.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use decktector_core::sim::{SimControl, SimEngine, SimLibrary};
use decktector_core::Engine;
use decktector_sim_control::{AudioCatalog, SimControlServer};
use prolink_engine::metadata::local::{load_export, LocalTrack};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::remote;

/// Set to `1` to run on simulated decks instead of the network.
const SIM_ENV: &str = "DECKTECTOR_SIM";
/// How many decks auto mode simulates.
const SIM_DECKS_ENV: &str = "DECKTECTOR_SIM_DECKS";
/// `manual` for hand-driven decks; anything else, or unset, is auto mode.
const SIM_MODE_ENV: &str = "DECKTECTOR_SIM_MODE";
const MANUAL_MODE: &str = "manual";
/// Folder holding the rekordbox export manual mode plays, overriding
/// [`DEFAULT_EXPORT_DIR`]. Relative paths resolve against the workspace root.
const SIM_EXPORT_ENV: &str = "DECKTECTOR_SIM_EXPORT";
/// Default export folder, relative to the workspace root (gitignored).
const DEFAULT_EXPORT_DIR: &str = "sim-library";

/// Default number of simulated decks in auto mode when none is requested.
const DEFAULT_SIM_DECKS: u8 = 2;

/// How long an already open CDJ page gets to reconnect before a new browser
/// tab is opened — so restarting the app does not stack up tabs.
const PAGE_RECONNECT_GRACE: Duration = Duration::from_secs(4);

/// What driving the decks by hand needs beyond the engine.
pub struct ManualControl {
    pub control: SimControl,
    /// The audio files the control page may play.
    pub audio: AudioCatalog,
}

/// The engine for this run, and the hand-driving setup when the decks are
/// driven by hand.
pub struct SimSetup {
    pub engine: Arc<dyn Engine>,
    pub manual: Option<ManualControl>,
}

/// Whether the environment asks for the simulation at all.
pub fn is_requested() -> bool {
    std::env::var(SIM_ENV).as_deref() == Ok("1")
}

/// Builds the simulated engine in the mode the environment asks for.
pub fn create() -> SimSetup {
    if std::env::var(SIM_MODE_ENV).as_deref() == Ok(MANUAL_MODE) {
        let (library, audio, fallback_warning) = manual_library();
        let (engine, control) = SimEngine::manual(library, fallback_warning);
        return SimSetup {
            engine: Arc::new(engine),
            manual: Some(ManualControl { control, audio }),
        };
    }

    let deck_count = std::env::var(SIM_DECKS_ENV)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_SIM_DECKS);
    SimSetup {
        engine: Arc::new(SimEngine::new(deck_count)),
        manual: None,
    }
}

/// The export of one manual run: its library, the audio files of its tracks,
/// and the warning the CDJ page shows when it fell back to generated tracks.
type ManualLibrary = (SimLibrary, AudioCatalog, Option<String>);

/// The export's tracks and audio, or the generated tracks (silent, hence an
/// empty catalog) with the warning the CDJ page shows when no usable export
/// is there.
fn manual_library() -> ManualLibrary {
    let export_dir = resolve_export_dir(std::env::var_os(SIM_EXPORT_ENV), &workspace_root());
    match load_export(&export_dir) {
        Ok(tracks) if !tracks.is_empty() => {
            let (library, audio) = split_tracks(tracks);
            tracing::info!(
                path = %export_dir.display(),
                tracks = library.len(),
                with_audio = audio.len(),
                "manual simulation using the rekordbox export"
            );
            if audio.is_empty() {
                tracing::warn!(
                    path = %export_dir.display(),
                    "none of the export's tracks has an audio file, so the decks will be silent; \
                     copy the export's Contents/ folder next to PIONEER/"
                );
            }
            (library, audio, None)
        }
        Ok(_) => fallback(format!(
            "The rekordbox export in {} holds no analysed track; playing generated tracks.",
            export_dir.display()
        )),
        // The loader's error already names the folder it looked in.
        Err(error) => fallback(format!("{error}; playing generated tracks.")),
    }
}

/// Separates what the decks play from where the page gets the sound.
fn split_tracks(tracks: Vec<LocalTrack>) -> (SimLibrary, AudioCatalog) {
    let audio = AudioCatalog::new(tracks.iter().filter_map(|track| {
        track
            .audio_path
            .clone()
            .map(|path| (track.info.track_id, path))
    }));
    let library = SimLibrary::from_tracks(tracks.into_iter().map(|track| track.info).collect());
    (library, audio)
}

fn fallback(warning: String) -> ManualLibrary {
    tracing::warn!("{warning}");
    (
        SimLibrary::generated(),
        AudioCatalog::default(),
        Some(warning),
    )
}

/// The workspace root, where `sim-library/` lives. Taken from the build
/// rather than the working directory, because `tauri dev` runs the binary
/// from `src-tauri`, not from the root a relative path is written against.
fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(manifest_dir).to_path_buf()
}

/// `value` (the [`SIM_EXPORT_ENV`] override) resolved against `root`, or
/// the default folder there when unset or empty.
fn resolve_export_dir(value: Option<OsString>, root: &Path) -> PathBuf {
    match value.filter(|value| !value.is_empty()) {
        Some(value) => root.join(value),
        None => root.join(DEFAULT_EXPORT_DIR),
    }
}

/// Starts the control server and opens the CDJ page in the default
/// browser, unless a page left open from a previous run reconnects first.
/// A server that cannot start is logged: the app still runs, the decks
/// just cannot be driven.
pub fn serve_control_page(app: AppHandle, manual: ManualControl) {
    let assets = remote::resolve_asset_source(&app);
    tauri::async_runtime::spawn(async move {
        let server = match SimControlServer::start(manual.control, assets, manual.audio).await {
            Ok(server) => server,
            Err(error) => {
                tracing::error!(%error, "failed to start the simulated CDJ control server");
                return;
            }
        };
        // Dropping the server stops it, so the app keeps it for its whole
        // lifetime rather than this task holding it only until the page opens.
        app.manage(Arc::clone(&server));
        let url = server.page_url();
        tracing::info!(%url, "simulated CDJ control page ready");
        if server.wait_for_client(PAGE_RECONNECT_GRACE).await {
            return;
        }
        if let Err(error) = app.opener().open_url(&url, None::<&str>) {
            tracing::error!(%error, %url, "failed to open the simulated CDJ control page");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_defaults_to_sim_library_at_the_root() {
        let root = Path::new("repo");
        assert_eq!(
            resolve_export_dir(None, root),
            root.join(DEFAULT_EXPORT_DIR)
        );
        assert_eq!(
            resolve_export_dir(Some(OsString::new()), root),
            root.join(DEFAULT_EXPORT_DIR)
        );
    }

    #[test]
    fn a_relative_override_resolves_against_the_root() {
        let root = Path::new("repo");
        assert_eq!(
            resolve_export_dir(Some("usb/export".into()), root),
            root.join("usb").join("export")
        );
    }

    #[test]
    fn an_absolute_override_is_used_as_is() {
        let absolute = std::env::temp_dir().join("export");
        assert_eq!(
            resolve_export_dir(Some(absolute.clone().into_os_string()), Path::new("repo")),
            absolute
        );
    }

    #[test]
    fn the_workspace_root_holds_the_workspace_manifest() {
        assert!(workspace_root().join("Cargo.toml").is_file());
        assert!(workspace_root().join("src-tauri").is_dir());
    }
}
