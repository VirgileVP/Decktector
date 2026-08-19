//! Silent play history: appends one line per track to a log file, the first
//! time a deck actually plays it.
//!
//! Mirrors the old Electron `track-log.ts` this replaces, dropped when the
//! shell moved to Tauri. Line format is tab-separated, local time,
//! human-readable: `2026-07-29 23:44:20  deck 1  Artist — Title  128.0 BPM`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Local;
use decktective_core::model::{DeckId, PlayState, TrackInfo};
use decktective_core::{Engine, EngineEvent};
use tauri::{AppHandle, Manager};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast::error::RecvError;

const LOG_FILE_NAME: &str = "played-tracks.log";

/// Where the play history lives: an explicit `DECKTECTIVE_LOG_DIR` wins, a
/// dev run logs into `./logs` next to the project for easy access, and a
/// packaged build uses the app's own data directory.
fn resolve_log_dir(app: &AppHandle) -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("DECKTECTIVE_LOG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    if cfg!(debug_assertions) {
        return Ok(std::env::current_dir()?.join("logs"));
    }
    Ok(app.path().app_data_dir()?)
}

/// Subscribes to the engine and appends a line each time a deck starts
/// playing a track it had not yet logged. Best-effort: a write failure is
/// traced and skipped rather than surfaced, since losing a log line should
/// never interrupt the show.
pub fn attach(app: AppHandle, engine: Arc<dyn Engine>) {
    let mut events = engine.handle().subscribe();

    tauri::async_runtime::spawn(async move {
        let log_path = match resolve_log_dir(&app) {
            Ok(dir) => match fs::create_dir_all(&dir).await {
                Ok(()) => dir.join(LOG_FILE_NAME),
                Err(error) => {
                    tracing::error!(%error, "failed to create play-history log directory");
                    return;
                }
            },
            Err(error) => {
                tracing::error!(%error, "failed to resolve play-history log directory");
                return;
            }
        };

        // Tracks loaded on a deck but not yet logged as played.
        let mut pending: HashMap<DeckId, TrackInfo> = HashMap::new();

        loop {
            match events.recv().await {
                Ok(EngineEvent::DeckTrack(deck_track)) => match deck_track.track {
                    Some(track) => {
                        pending.insert(deck_track.deck_id, track);
                    }
                    None => {
                        pending.remove(&deck_track.deck_id);
                    }
                },
                Ok(EngineEvent::DeckState(deck)) if deck.play_state == PlayState::Playing => {
                    if let Some(track) = pending.remove(&deck.deck_id) {
                        if let Err(error) = append_line(&log_path, deck.deck_id, &track).await {
                            tracing::error!(%error, "failed to write play-history log");
                        }
                    }
                }
                Ok(_) => {}
                // The pending map only cares about the latest load per deck,
                // so a missed burst costs nothing worth resyncing for.
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
}

fn format_line(deck_id: DeckId, track: &TrackInfo) -> String {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let artist = track.artist.as_deref().unwrap_or("Unknown artist");
    let bpm = track
        .bpm
        .map(|bpm| format!("{bpm:.1} BPM"))
        .unwrap_or_else(|| "BPM ?".to_string());
    format!(
        "{timestamp}\tdeck {deck_id}\t{artist} — {}\t{bpm}\n",
        track.title
    )
}

async fn append_line(path: &Path, deck_id: DeckId, track: &TrackInfo) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(format_line(deck_id, track).as_bytes())
        .await
}
