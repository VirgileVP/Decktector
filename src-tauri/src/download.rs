//! Tauri command for downloading the currently playing track from a deck.

use std::sync::Arc;

use decktective_core::model::DeckId;
use decktective_core::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressPayload {
    pub deck_id: DeckId,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub deck_id: DeckId,
    pub file_path: String,
    pub filename: String,
}

const PROGRESS_EVENT: &str = "download:progress";

#[tauri::command]
pub async fn download_deck_track(
    app: AppHandle,
    engine: State<'_, Arc<dyn Engine>>,
    deck_id: DeckId,
) -> Result<DownloadResult, String> {
    let download_dir = app
        .path()
        .download_dir()
        .map_err(|e| format!("Failed to resolve download directory: {e}"))?
        .join("Decktective");

    let app_handle = app.clone();
    let progress_callback = Box::new(move |bytes_downloaded: u64, total_bytes: u64| {
        let percent = if total_bytes > 0 {
            (bytes_downloaded as f32 / total_bytes as f32) * 100.0
        } else {
            0.0
        };
        let _ = app_handle.emit(
            PROGRESS_EVENT,
            DownloadProgressPayload {
                deck_id,
                bytes_downloaded,
                total_bytes,
                percent,
            },
        );
    });

    let saved_path = engine
        .download_track(deck_id, download_dir, progress_callback)
        .await
        .map_err(|e| e.to_string())?;

    let filename = saved_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("track.mp3")
        .to_string();

    Ok(DownloadResult {
        deck_id,
        file_path: saved_path.to_string_lossy().to_string(),
        filename,
    })
}
