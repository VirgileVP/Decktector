//! Moves a document's text to or from a file the user picks in the native
//! dialog — the one way the lineup and the show reach the disk.
//!
//! The renderer owns each format and its validation; this side only shows
//! the dialog and moves text. Paths never come from the renderer, so it
//! cannot write anywhere the user did not choose.

use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};
use tokio::fs;

/// A kind of document the user can export and import as a text file.
pub struct TextDocument {
    /// Name of the dialog's file filter, e.g. "Decktector lineup".
    pub filter_name: &'static str,
    pub extensions: &'static [&'static str],
    /// Refuses an unrelated large file picked by mistake, before reading it.
    pub max_import_bytes: u64,
    /// What the document is called in an error message, e.g. "lineup".
    pub noun: &'static str,
}

fn into_path(file: FilePath) -> Result<PathBuf, String> {
    file.into_path()
        .map_err(|error| format!("Unsupported file location: {error}"))
}

/// Refuses `path` when its `len` bytes cannot be a `document`.
fn ensure_importable_size(document: &TextDocument, path: &Path, len: u64) -> Result<(), String> {
    if len > document.max_import_bytes {
        return Err(format!(
            "{} is too large to be a {} file.",
            path.display(),
            document.noun
        ));
    }
    Ok(())
}

/// Asks where to save `document` and writes `contents` there. Returns the
/// saved path, or `None` if the user cancelled the dialog.
pub async fn save(
    app: &AppHandle,
    document: &TextDocument,
    contents: String,
    suggested_name: String,
) -> Result<Option<String>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter(document.filter_name, document.extensions)
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

/// Asks which `document` file to open and returns its text, or `None` if the
/// user cancelled the dialog.
pub async fn open(app: &AppHandle, document: &TextDocument) -> Result<Option<String>, String> {
    let Some(file) = app
        .dialog()
        .file()
        .add_filter(document.filter_name, document.extensions)
        .blocking_pick_file()
    else {
        return Ok(None);
    };

    let path = into_path(file)?;
    let metadata = fs::metadata(&path)
        .await
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    ensure_importable_size(document, &path, metadata.len())?;

    fs::read_to_string(&path)
        .await
        .map(Some)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: TextDocument = TextDocument {
        filter_name: "Decktector test",
        extensions: &["json"],
        max_import_bytes: 1024,
        noun: "test",
    };

    #[test]
    fn a_file_over_the_cap_is_refused_before_it_is_read() {
        let path = Path::new("night.json");
        assert_eq!(ensure_importable_size(&DOCUMENT, path, 1024), Ok(()));
        assert_eq!(
            ensure_importable_size(&DOCUMENT, path, 1025),
            Err("night.json is too large to be a test file.".to_owned())
        );
    }
}
