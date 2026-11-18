//! Tauri commands for saving the lineup to a file and loading it back.
//!
//! The renderer owns the file's format and validation; [`text_file`] shows
//! the native dialog and moves the text.

use tauri::AppHandle;

use crate::text_file::{self, TextDocument};

const LINEUP: TextDocument = TextDocument {
    filter_name: "Decktector lineup",
    extensions: &["json"],
    // Far beyond any real lineup.
    max_import_bytes: 1024 * 1024,
    noun: "lineup",
};

/// Asks where to save the lineup and writes `contents` there. Returns the
/// saved path, or `None` if the user cancelled the dialog.
#[tauri::command]
pub async fn export_lineup_file(
    app: AppHandle,
    contents: String,
    suggested_name: String,
) -> Result<Option<String>, String> {
    text_file::save(&app, &LINEUP, contents, suggested_name).await
}

/// Asks which lineup file to open and returns its text, or `None` if the
/// user cancelled the dialog.
#[tauri::command]
pub async fn import_lineup_file(app: AppHandle) -> Result<Option<String>, String> {
    text_file::open(&app, &LINEUP).await
}
