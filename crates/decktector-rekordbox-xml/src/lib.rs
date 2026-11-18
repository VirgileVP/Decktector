//! Reads a rekordbox XML export (File > Export Collection in xml format):
//! its playlist tree, and the title, artist and duration of the tracks one
//! playlist lists.
//!
//! A whole collection runs to tens of MB and tens of thousands of tracks, so
//! the document is streamed with `quick-xml`, never held as a DOM, and only
//! what a show needs is kept. Bytes in, a [`Library`] out: no file access,
//! nothing about timecode.
//!
//! The format: `DJ_PLAYLISTS` holds a `COLLECTION` of `TRACK`s (`TrackID`,
//! `Name`, `Artist`, `TotalTime` in whole seconds, `Location`) and a
//! `PLAYLISTS` tree of `NODE`s — `Type` 0 a folder, 1 a playlist — whose
//! playlist entries are `TRACK Key=…`, a `TrackID` when the playlist's
//! `KeyType` is 0, a `Location` when it is 1.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::BufRead;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// Deeper than any real playlist tree; refuses a document built to exhaust the stack.
pub const MAX_DEPTH: usize = 64;
/// Far beyond a night's playlist; a whole-collection playlist is not a show.
pub const MAX_TRACKS_PER_PLAYLIST: usize = 5_000;
/// Text attributes are cut to the show file's own limit.
pub const MAX_TEXT_CHARS: usize = 300;

const ROOT_ELEMENT: &[u8] = b"DJ_PLAYLISTS";
const COLLECTION_ELEMENT: &[u8] = b"COLLECTION";
const PLAYLISTS_ELEMENT: &[u8] = b"PLAYLISTS";
const NODE_ELEMENT: &[u8] = b"NODE";
const TRACK_ELEMENT: &[u8] = b"TRACK";
/// rekordbox writes its playlist tree under one folder of this name.
const ROOT_NODE_NAME: &str = "ROOT";
/// `NODE Type` of a playlist; every other value is a folder.
const PLAYLIST_NODE_TYPE: &str = "1";
/// `NODE KeyType` of a playlist keyed by `Location`; anything else is `TrackID`.
const LOCATION_KEY_TYPE: &str = "1";
const PERCENT_ESCAPE_LEN: usize = 3;
const HEX_RADIX: u32 = 16;

/// Why a document could not be read. Messages are meant for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum XmlError {
    #[error("The file is not valid XML ({0}).")]
    Malformed(String),
    #[error("The file ends before the export does — it may have been cut short.")]
    Truncated,
    #[error("This is not a rekordbox XML export.")]
    NotRekordbox,
    #[error("The playlist tree is nested too deeply to be a rekordbox export.")]
    TooDeep,
    #[error("That playlist is not in this export.")]
    NoSuchPlaylist,
    #[error("That playlist lists {entries} tracks; a show takes at most {max}.")]
    PlaylistTooLong { entries: usize, max: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Folder,
    Playlist,
}

/// How a playlist names its tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyType {
    TrackId,
    Location,
}

/// One folder or playlist of the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistNode {
    pub name: String,
    pub kind: NodeKind,
    pub children: Vec<PlaylistNode>,
    /// How many entries a playlist lists; 0 for a folder.
    pub track_count: usize,
    key_type: KeyType,
    /// The playlist's keys, up to [`MAX_TRACKS_PER_PLAYLIST`].
    keys: Vec<String>,
}

impl PlaylistNode {
    fn folder(name: String) -> Self {
        Self {
            name,
            kind: NodeKind::Folder,
            children: Vec::new(),
            track_count: 0,
            key_type: KeyType::TrackId,
            keys: Vec::new(),
        }
    }
}

/// A track as a show row takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTrack {
    pub title: String,
    pub artist: String,
    /// Whole seconds; 0 when the export gives none.
    pub duration_seconds: u32,
}

/// What one playlist yields: its tracks in order, and how many of its entries
/// named a track the collection does not have (left out, for the UI to say).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistTracks {
    pub tracks: Vec<ImportedTrack>,
    pub skipped: usize,
}

/// A parsed export: the collection's tracks and the playlist tree.
#[derive(Debug, Clone)]
pub struct Library {
    tracks_by_id: HashMap<String, ImportedTrack>,
    /// Percent-decoded `Location` to `TrackID`.
    ids_by_location: HashMap<String, String>,
    tree: PlaylistNode,
}

impl Library {
    /// The playlist tree, rooted at rekordbox's own root folder.
    pub fn tree(&self) -> &PlaylistNode {
        &self.tree
    }

    /// The tracks of the playlist at `path` — child indices from the root.
    pub fn playlist_tracks(&self, path: &[usize]) -> Result<PlaylistTracks, XmlError> {
        let node = path
            .iter()
            .try_fold(&self.tree, |node, &index| node.children.get(index))
            .filter(|node| node.kind == NodeKind::Playlist)
            .ok_or(XmlError::NoSuchPlaylist)?;
        if node.track_count > MAX_TRACKS_PER_PLAYLIST {
            return Err(XmlError::PlaylistTooLong {
                entries: node.track_count,
                max: MAX_TRACKS_PER_PLAYLIST,
            });
        }

        let mut tracks = Vec::with_capacity(node.keys.len());
        let mut skipped = 0;
        for key in &node.keys {
            let track_id = match node.key_type {
                KeyType::TrackId => Some(key.as_str()),
                KeyType::Location => self
                    .ids_by_location
                    .get(&percent_decode(key))
                    .map(String::as_str),
            };
            match track_id.and_then(|id| self.tracks_by_id.get(id)) {
                Some(track) => tracks.push(track.clone()),
                None => skipped += 1,
            }
        }
        Ok(PlaylistTracks { tracks, skipped })
    }
}

/// Decodes `%XX` escapes once, as a `file://` URL carries them. Invalid
/// escapes are kept as written, and bytes that do not form UTF-8 are replaced.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escape = bytes
            .get(index..index + PERCENT_ESCAPE_LEN)
            .filter(|escape| escape[0] == b'%')
            .and_then(|escape| std::str::from_utf8(&escape[1..]).ok())
            .and_then(|hex| u8::from_str_radix(hex, HEX_RADIX).ok());
        match escape {
            Some(byte) => {
                decoded.push(byte);
                index += PERCENT_ESCAPE_LEN;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn truncated(text: Cow<'_, str>) -> String {
    text.chars().take(MAX_TEXT_CHARS).collect()
}

/// The attribute `name` of `element`, entities decoded once; `None` when absent.
fn attribute(element: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>, XmlError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| XmlError::Malformed(error.to_string()))?;
        if attribute.key.as_ref() == name {
            let value = attribute
                .unescape_value()
                .map_err(|error| XmlError::Malformed(error.to_string()))?;
            return Ok(Some(truncated(value)));
        }
    }
    Ok(None)
}

/// Which part of the document the reader is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Other,
    Collection,
    Playlists,
}

/// The streaming parse's state between events.
struct Parser {
    library: Library,
    section: Section,
    /// Open `NODE`s, innermost last; the synthetic container is the first.
    nodes: Vec<PlaylistNode>,
    depth: usize,
    seen_root: bool,
}

impl Parser {
    fn new() -> Self {
        Self {
            library: Library {
                tracks_by_id: HashMap::new(),
                ids_by_location: HashMap::new(),
                tree: PlaylistNode::folder(ROOT_NODE_NAME.to_owned()),
            },
            section: Section::Other,
            nodes: vec![PlaylistNode::folder(ROOT_NODE_NAME.to_owned())],
            depth: 0,
            seen_root: false,
        }
    }

    /// An opening tag; `is_empty` for a self-closing one, which gets no end event.
    fn open(&mut self, element: &BytesStart<'_>, is_empty: bool) -> Result<(), XmlError> {
        let name = element.local_name();
        if !self.seen_root {
            if name.as_ref() != ROOT_ELEMENT {
                return Err(XmlError::NotRekordbox);
            }
            self.seen_root = true;
        }
        if !is_empty {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return Err(XmlError::TooDeep);
            }
        }

        match (self.section, name.as_ref()) {
            (_, COLLECTION_ELEMENT) if !is_empty => self.section = Section::Collection,
            (_, PLAYLISTS_ELEMENT) if !is_empty => self.section = Section::Playlists,
            (Section::Collection, TRACK_ELEMENT) => self.add_collection_track(element)?,
            (Section::Playlists, NODE_ELEMENT) => {
                let node = Self::node(element)?;
                if is_empty {
                    self.attach(node);
                } else {
                    self.nodes.push(node);
                }
            }
            (Section::Playlists, TRACK_ELEMENT) => self.add_playlist_entry(element)?,
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, name: &[u8]) {
        self.depth = self.depth.saturating_sub(1);
        match name {
            COLLECTION_ELEMENT | PLAYLISTS_ELEMENT => self.section = Section::Other,
            NODE_ELEMENT if self.section == Section::Playlists && self.nodes.len() > 1 => {
                if let Some(node) = self.nodes.pop() {
                    self.attach(node);
                }
            }
            _ => {}
        }
    }

    fn attach(&mut self, node: PlaylistNode) {
        if let Some(parent) = self.nodes.last_mut() {
            parent.children.push(node);
        }
    }

    fn node(element: &BytesStart<'_>) -> Result<PlaylistNode, XmlError> {
        let name = attribute(element, b"Name")?.unwrap_or_default();
        let is_playlist = attribute(element, b"Type")?.as_deref() == Some(PLAYLIST_NODE_TYPE);
        let mut node = PlaylistNode::folder(name);
        if is_playlist {
            node.kind = NodeKind::Playlist;
            if attribute(element, b"KeyType")?.as_deref() == Some(LOCATION_KEY_TYPE) {
                node.key_type = KeyType::Location;
            }
        }
        Ok(node)
    }

    fn add_collection_track(&mut self, element: &BytesStart<'_>) -> Result<(), XmlError> {
        let Some(track_id) = attribute(element, b"TrackID")? else {
            return Ok(()); // A track nothing can name.
        };
        if let Some(location) = attribute(element, b"Location")? {
            self.library
                .ids_by_location
                .insert(percent_decode(&location), track_id.clone());
        }
        let track = ImportedTrack {
            title: attribute(element, b"Name")?.unwrap_or_default(),
            artist: attribute(element, b"Artist")?.unwrap_or_default(),
            duration_seconds: attribute(element, b"TotalTime")?
                .and_then(|seconds| seconds.trim().parse().ok())
                .unwrap_or_default(),
        };
        self.library.tracks_by_id.insert(track_id, track);
        Ok(())
    }

    fn add_playlist_entry(&mut self, element: &BytesStart<'_>) -> Result<(), XmlError> {
        let Some(key) = attribute(element, b"Key")? else {
            return Ok(());
        };
        if let Some(playlist) = self
            .nodes
            .last_mut()
            .filter(|node| node.kind == NodeKind::Playlist)
        {
            playlist.track_count += 1;
            if playlist.keys.len() < MAX_TRACKS_PER_PLAYLIST {
                playlist.keys.push(key);
            }
        }
        Ok(())
    }

    /// The library, once the document ended cleanly.
    fn finish(mut self) -> Result<Library, XmlError> {
        if !self.seen_root {
            return Err(XmlError::NotRekordbox);
        }
        if self.depth > 0 {
            return Err(XmlError::Truncated);
        }
        let mut container = self
            .nodes
            .pop()
            .unwrap_or_else(|| PlaylistNode::folder(ROOT_NODE_NAME.to_owned()));
        // rekordbox writes one root folder; show that one, not a wrapper around it.
        self.library.tree = match container.children.len() {
            1 if container.children[0].kind == NodeKind::Folder => container.children.remove(0),
            _ => container,
        };
        Ok(self.library)
    }
}

/// Reads a rekordbox XML export from `reader`.
pub fn parse(reader: impl BufRead) -> Result<Library, XmlError> {
    let mut reader = Reader::from_reader(reader);
    let mut parser = Parser::new();
    let mut buffer = Vec::new();
    loop {
        let event = reader.read_event_into(&mut buffer).map_err(|error| {
            XmlError::Malformed(format!("{error} at byte {}", reader.error_position()))
        })?;
        match event {
            Event::Start(element) => parser.open(&element, false)?,
            Event::Empty(element) => parser.open(&element, true)?,
            Event::End(element) => parser.close(element.local_name().as_ref()),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    parser.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_escapes_are_decoded_once() {
        assert_eq!(
            percent_decode("file://localhost/C:/Music/My%20Song.mp3"),
            "file://localhost/C:/Music/My Song.mp3"
        );
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("100%2541"), "100%41");
        assert_eq!(percent_decode("50% off %zz"), "50% off %zz");
    }
}
