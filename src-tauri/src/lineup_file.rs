//! Tauri commands for saving the lineup to a file and loading it back.
//!
//! The renderer owns the file's format and validation; this side only shows
//! the native file dialog and moves text to or from the file the user picked.
//! Paths never come from the renderer, so it cannot write anywhere the user
//! did not choose.

use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};
use tokio::fs;

const FILE_FILTER_NAME: &str = "Decktector lineup";
const FILE_EXTENSIONS: &[&str] = &["json"];

/// Far beyond any real lineup; refuses to load an unrelated large file the
/// user picked by mistake.
const MAX_IMPORT_BYTES: u64 = 1024 * 1024;

fn into_path(file: FilePath) -> Result<PathBuf, String> {
    file.into_path()
        .map_err(|error| format!("Unsupported file location: {error}"))
}

/// Asks where to save the lineup and writes `contents` there. Returns the
/// saved path, or `None` if the user cancelled the dialog.
#[tauri::command]
pub async fn export_lineup_file(
    app: AppHandle,
    contents: String,
    suggested_name: String,
) -> Result<Option<String>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter(FILE_FILTER_NAME, FILE_EXTENSIONS)
        .set_file_name(suggested_name)
        .blocking_save_file()
    else {
        return Ok(None);
    };

    let path = into_path(file)?;
    fs::write(&path, contents)
        .await
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Asks which lineup file to open and returns its text, or `None` if the
/// user cancelled the dialog.
#[tauri::command]
pub async fn import_lineup_file(app: AppHandle) -> Result<Option<String>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter(FILE_FILTER_NAME, FILE_EXTENSIONS)
        .blocking_pick_file()
    else {
        return Ok(None);
    };

    let path = into_path(file)?;
    let metadata = fs::metadata(&path)
        .await
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(format!(
            "{} is too large to be a lineup file.",
            path.display()
        ));
    }

    fs::read_to_string(&path)
        .await
        .map(Some)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))
}
