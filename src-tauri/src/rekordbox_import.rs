//! Tauri commands for importing a playlist from a rekordbox XML export.
//!
//! The export is read here, not in the renderer: a whole collection runs to
//! tens of MB, which would freeze the UI thread in a DOM parser. Same safety
//! shape as the lineup and show files — native dialog, path never from the
//! renderer, size checked before reading. The parsed library stays in this
//! process under an opaque token, so the renderer only ever receives the
//! playlist tree and the one playlist it asks for.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use decktector_rekordbox_xml::{Library, NodeKind, PlaylistNode, PlaylistTracks};
use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use crate::text_file::{self, TextDocument};

const REKORDBOX_XML: TextDocument = TextDocument {
    filter_name: "rekordbox XML",
    extensions: &["xml"],
    // A very large collection exports to well under it.
    max_import_bytes: 256 * 1024 * 1024,
    noun: "rekordbox XML",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RekordboxNodeKind.ts")]
pub enum RekordboxNodeKind {
    Folder,
    Playlist,
}

/// One folder or playlist of an export's tree.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RekordboxPlaylistNode.ts")]
pub struct RekordboxPlaylistNode {
    pub name: String,
    pub kind: RekordboxNodeKind,
    /// How many entries a playlist lists; 0 for a folder.
    pub track_count: usize,
    pub children: Vec<RekordboxPlaylistNode>,
}

impl From<&PlaylistNode> for RekordboxPlaylistNode {
    fn from(node: &PlaylistNode) -> Self {
        Self {
            name: node.name.clone(),
            kind: match node.kind {
                NodeKind::Folder => RekordboxNodeKind::Folder,
                NodeKind::Playlist => RekordboxNodeKind::Playlist,
            },
            track_count: node.track_count,
            children: node.children.iter().map(Self::from).collect(),
        }
    }
}

/// An export the user opened, ready to pick a playlist from.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "OpenedRekordboxLibrary.ts")]
pub struct OpenedRekordboxLibrary {
    /// Names the library in the other commands; only the latest one is valid.
    pub token: String,
    pub file_name: String,
    pub tree: RekordboxPlaylistNode,
}

/// A track of the chosen playlist, as a show row takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RekordboxTrack.ts")]
pub struct RekordboxTrack {
    pub title: String,
    pub artist: String,
    /// Whole seconds; 0 when the export gives none.
    pub duration_seconds: u32,
}

/// The chosen playlist's tracks in order, and how many of its entries named
/// a track the export's collection does not have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "RekordboxPlaylist.ts")]
pub struct RekordboxPlaylist {
    pub tracks: Vec<RekordboxTrack>,
    pub skipped: usize,
}

impl From<PlaylistTracks> for RekordboxPlaylist {
    fn from(playlist: PlaylistTracks) -> Self {
        Self {
            tracks: playlist
                .tracks
                .into_iter()
                .map(|track| RekordboxTrack {
                    title: track.title,
                    artist: track.artist,
                    duration_seconds: track.duration_seconds,
                })
                .collect(),
            skipped: playlist.skipped,
        }
    }
}

const UNKNOWN_TOKEN: &str = "That rekordbox export is no longer open; open it again.";

/// The one export held open, if any, and the token it was issued under.
#[derive(Default)]
pub struct RekordboxImport {
    open: Mutex<Option<(String, Library)>>,
    issued: AtomicU64,
}

impl RekordboxImport {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<(String, Library)>> {
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Holds `library`, dropping whichever was held before, and returns its token.
    fn hold(&self, library: Library) -> String {
        let token = format!(
            "rekordbox-{}",
            self.issued.fetch_add(1, Ordering::Relaxed) + 1
        );
        *self.slot() = Some((token.clone(), library));
        token
    }

    /// The tracks of the playlist at `path` in the library `token` names.
    fn playlist(&self, token: &str, path: &[usize]) -> Result<RekordboxPlaylist, String> {
        let slot = self.slot();
        let library = slot
            .as_ref()
            .filter(|(held, _)| held == token)
            .map(|(_, library)| library)
            .ok_or_else(|| UNKNOWN_TOKEN.to_owned())?;
        library
            .playlist_tracks(path)
            .map(RekordboxPlaylist::from)
            .map_err(|error| error.to_string())
    }

    /// Lets go of the library `token` names; any other token changes nothing.
    fn close(&self, token: &str) {
        let mut slot = self.slot();
        if slot.as_ref().is_some_and(|(held, _)| held == token) {
            *slot = None;
        }
    }
}

fn parse_file(path: &Path) -> Result<Library, String> {
    let file =
        File::open(path).map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    decktector_rekordbox_xml::parse(BufReader::new(file)).map_err(|error| error.to_string())
}

/// Asks which rekordbox export to open, reads it off the UI thread and holds
/// it. Returns its playlist tree, or `None` if the user cancelled the dialog.
#[tauri::command]
pub async fn open_rekordbox_xml(
    app: AppHandle,
    state: State<'_, RekordboxImport>,
) -> Result<Option<OpenedRekordboxLibrary>, String> {
    let Some(path) = text_file::pick(&app, &REKORDBOX_XML).await? else {
        return Ok(None);
    };
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let library = tauri::async_runtime::spawn_blocking(move || parse_file(&path))
        .await
        .map_err(|error| format!("Reading the rekordbox export stopped: {error}"))??;

    let tree = RekordboxPlaylistNode::from(library.tree());
    Ok(Some(OpenedRekordboxLibrary {
        token: state.hold(library),
        file_name,
        tree,
    }))
}

/// The tracks of the playlist at `path` (child indices from the root) in the
/// export `token` names.
#[tauri::command]
pub fn read_rekordbox_playlist(
    state: State<'_, RekordboxImport>,
    token: String,
    path: Vec<usize>,
) -> Result<RekordboxPlaylist, String> {
    state.playlist(&token, &path)
}

/// Lets go of the export `token` names once the import is done or cancelled.
#[tauri::command]
pub fn close_rekordbox_xml(state: State<'_, RekordboxImport>, token: String) {
    state.close(&token);
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPORT: &str = r#"<DJ_PLAYLISTS Version="1.0.0">
  <COLLECTION>
    <TRACK TrackID="1" Name="Strobe" Artist="deadmau5" TotalTime="637"/>
  </COLLECTION>
  <PLAYLISTS>
    <NODE Type="0" Name="ROOT">
      <NODE Type="1" Name="Friday" KeyType="0"><TRACK Key="1"/><TRACK Key="2"/></NODE>
    </NODE>
  </PLAYLISTS>
</DJ_PLAYLISTS>"#;

    fn library() -> Library {
        decktector_rekordbox_xml::parse(EXPORT.as_bytes()).expect("the export parses")
    }

    #[test]
    fn a_held_library_gives_its_playlist_and_the_skipped_count() {
        let import = RekordboxImport::default();
        let token = import.hold(library());
        assert_eq!(
            import.playlist(&token, &[0]),
            Ok(RekordboxPlaylist {
                tracks: vec![RekordboxTrack {
                    title: "Strobe".to_owned(),
                    artist: "deadmau5".to_owned(),
                    duration_seconds: 637,
                }],
                skipped: 1,
            })
        );
    }

    #[test]
    fn a_token_that_was_never_issued_is_refused() {
        let import = RekordboxImport::default();
        import.hold(library());
        assert_eq!(
            import.playlist("rekordbox-forged", &[0]),
            Err(UNKNOWN_TOKEN.to_owned())
        );
    }

    #[test]
    fn opening_a_second_library_drops_the_first() {
        let import = RekordboxImport::default();
        let first = import.hold(library());
        let second = import.hold(library());
        assert_ne!(first, second);
        assert!(import.playlist(&first, &[0]).is_err());
        assert!(import.playlist(&second, &[0]).is_ok());

        import.close(&first);
        assert!(
            import.playlist(&second, &[0]).is_ok(),
            "a stale close leaves the current one"
        );
        import.close(&second);
        assert!(import.playlist(&second, &[0]).is_err());
    }

    #[test]
    fn a_path_outside_the_tree_is_refused() {
        let import = RekordboxImport::default();
        let token = import.hold(library());
        for path in [&[][..], &[3][..], &[0, 0][..]] {
            assert!(import.playlist(&token, path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn a_file_over_the_cap_is_refused_before_it_is_read() {
        let path = Path::new("collection.xml");
        let cap = REKORDBOX_XML.max_import_bytes;
        assert_eq!(
            text_file::ensure_importable_size(&REKORDBOX_XML, path, cap),
            Ok(())
        );
        assert!(text_file::ensure_importable_size(&REKORDBOX_XML, path, cap + 1).is_err());
    }

    #[test]
    fn the_tree_keeps_folders_playlists_and_counts() {
        let tree = RekordboxPlaylistNode::from(library().tree());
        assert_eq!(tree.name, "ROOT");
        assert_eq!(tree.kind, RekordboxNodeKind::Folder);
        assert_eq!(tree.children[0].kind, RekordboxNodeKind::Playlist);
        assert_eq!(tree.children[0].track_count, 2);
    }
}
