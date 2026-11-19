//! Tauri commands for saving the show to a file and loading it back.
//!
//! The renderer owns the file's format and validation (`lib/show-file.ts`);
//! [`text_file`] shows the native dialog and moves the text.

use tauri::AppHandle;

use crate::text_file::{self, TextDocument};

const SHOW: TextDocument = TextDocument {
    filter_name: "Decktector show",
    // `.dshow` for "Decktector show"; JSON inside. Shows exported as
    // `.json` before the extension existed still open.
    extensions: &["dshow", "json"],
    // 2000 rows of 300-character text stay well under it.
    max_import_bytes: 2 * 1024 * 1024,
    noun: "show",
};

/// Asks where to save the show and writes `contents` there. Returns the
/// saved path, or `None` if the user cancelled the dialog.
#[tauri::command]
pub async fn export_show_file(
    app: AppHandle,
    contents: String,
    suggested_name: String,
) -> Result<Option<String>, String> {
    text_file::save(&app, &SHOW, contents, suggested_name).await
}

/// Asks which show file to open and returns its text, or `None` if the user
/// cancelled the dialog.
#[tauri::command]
pub async fn import_show_file(app: AppHandle) -> Result<Option<String>, String> {
    text_file::open(&app, &SHOW).await
}
