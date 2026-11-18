//! Reads a hand-written export shaped like rekordbox's own.

use decktector_rekordbox_xml::{
    parse, ImportedTrack, Library, NodeKind, XmlError, MAX_DEPTH, MAX_TEXT_CHARS,
    MAX_TRACKS_PER_PLAYLIST,
};

const EXPORT: &str = include_str!("fixtures/export.xml");
/// Child indices of the two playlists in the fixture.
const FRIDAY: [usize; 2] = [0, 0];
const BY_LOCATION: [usize; 1] = [1];

fn library() -> Library {
    parse(EXPORT.as_bytes()).expect("the fixture parses")
}

fn titles(library: &Library, path: &[usize]) -> Vec<String> {
    library
        .playlist_tracks(path)
        .expect("a playlist")
        .tracks
        .into_iter()
        .map(|track| track.title)
        .collect()
}

/// A minimal export around `collection` and `playlists` markup.
fn export_with(collection: &str, playlists: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
  <COLLECTION>{collection}</COLLECTION>
  <PLAYLISTS><NODE Type="0" Name="ROOT">{playlists}</NODE></PLAYLISTS>
</DJ_PLAYLISTS>"#
    )
}

#[test]
fn a_key_type_0_playlist_resolves_entries_by_track_id() {
    assert_eq!(
        titles(&library(), &FRIDAY),
        ["Strobe", "Tom &amp; Jerry & Friends", "Café del Mar"]
    );
}

#[test]
fn a_key_type_1_playlist_resolves_entries_by_location() {
    // Escaped on one side and not the other, with a space and a non-ASCII name.
    assert_eq!(
        titles(&library(), &BY_LOCATION),
        ["Tom &amp; Jerry & Friends", "Café del Mar", "No Time"]
    );
}

#[test]
fn folders_nest_and_only_playlists_carry_tracks() {
    let library = library();
    let root = library.tree();
    assert_eq!(root.name, "ROOT");
    assert_eq!(root.kind, NodeKind::Folder);

    let names: Vec<(&str, NodeKind, usize)> = root
        .children
        .iter()
        .map(|node| (node.name.as_str(), node.kind, node.track_count))
        .collect();
    assert_eq!(
        names,
        [
            ("Gigs", NodeKind::Folder, 0),
            ("By location", NodeKind::Playlist, 3)
        ]
    );
    let gigs = &root.children[0];
    assert_eq!(gigs.children[0].name, "Friday");
    assert_eq!(gigs.children[0].track_count, 4);
    assert_eq!(gigs.children[1].kind, NodeKind::Folder);

    assert_eq!(library.playlist_tracks(&[0]), Err(XmlError::NoSuchPlaylist));
    assert_eq!(library.playlist_tracks(&[]), Err(XmlError::NoSuchPlaylist));
    assert_eq!(
        library.playlist_tracks(&[0, 7]),
        Err(XmlError::NoSuchPlaylist)
    );
}

#[test]
fn entities_in_names_are_decoded_by_the_parser_once() {
    let tracks = library()
        .playlist_tracks(&FRIDAY)
        .expect("a playlist")
        .tracks;
    assert_eq!(tracks[1].title, "Tom &amp; Jerry & Friends");
    assert_eq!(tracks[1].artist, "A <B>");
}

#[test]
fn an_entry_missing_from_the_collection_is_skipped_and_counted() {
    let friday = library().playlist_tracks(&FRIDAY).expect("a playlist");
    assert_eq!(friday.tracks.len(), 3);
    assert_eq!(friday.skipped, 1);
}

#[test]
fn total_time_is_whole_seconds_and_a_missing_one_is_zero() {
    let tracks = library()
        .playlist_tracks(&BY_LOCATION)
        .expect("a playlist")
        .tracks;
    assert_eq!(
        tracks[2],
        ImportedTrack {
            title: "No Time".to_owned(),
            artist: String::new(),
            duration_seconds: 0
        }
    );
    assert_eq!(tracks[1].duration_seconds, 480);
}

#[test]
fn a_document_that_is_not_rekordbox_xml_is_a_clean_error() {
    assert_eq!(
        parse("<html><body/></html>".as_bytes()).err(),
        Some(XmlError::NotRekordbox)
    );
    assert_eq!(
        parse("not xml at all".as_bytes()).err(),
        Some(XmlError::NotRekordbox)
    );
    assert!(matches!(
        parse("<DJ_PLAYLISTS><COLLECTION></PLAYLISTS></DJ_PLAYLISTS>".as_bytes()),
        Err(XmlError::Malformed(_))
    ));
}

#[test]
fn a_truncated_document_is_a_clean_error() {
    let after_collection = EXPORT
        .find("</COLLECTION>")
        .map(|index| index + "</COLLECTION>".len())
        .expect("the fixture has a collection");
    assert_eq!(
        parse(&EXPORT.as_bytes()[..after_collection]).err(),
        Some(XmlError::Truncated)
    );

    // Cut anywhere before its last tag closes, it is an error, never a panic
    // or a half library.
    for cut in (1..EXPORT.trim_end().len()).step_by(97) {
        if EXPORT.is_char_boundary(cut) {
            assert!(parse(&EXPORT.as_bytes()[..cut]).is_err(), "cut at {cut}");
        }
    }
}

#[test]
fn an_overlong_playlist_is_refused() {
    let entries = r#"<TRACK Key="1"/>"#.repeat(MAX_TRACKS_PER_PLAYLIST + 1);
    let export = export_with(
        r#"<TRACK TrackID="1" Name="Loop" Artist="" TotalTime="60"/>"#,
        &format!(r#"<NODE Type="1" Name="All" KeyType="0">{entries}</NODE>"#),
    );
    let library = parse(export.as_bytes()).expect("the export parses");
    assert_eq!(
        library.playlist_tracks(&[0]),
        Err(XmlError::PlaylistTooLong {
            entries: MAX_TRACKS_PER_PLAYLIST + 1,
            max: MAX_TRACKS_PER_PLAYLIST
        })
    );
}

#[test]
fn a_tree_nested_beyond_the_limit_is_refused() {
    let opening = r#"<NODE Type="0" Name="deep">"#.repeat(MAX_DEPTH);
    let closing = "</NODE>".repeat(MAX_DEPTH);
    let export = export_with("", &format!("{opening}{closing}"));
    assert_eq!(parse(export.as_bytes()).err(), Some(XmlError::TooDeep));
}

#[test]
fn long_text_is_cut_to_the_show_file_limit() {
    let export = export_with(
        &format!(
            r#"<TRACK TrackID="1" Name="{}" Artist="A" TotalTime="60"/>"#,
            "é".repeat(MAX_TEXT_CHARS + 50)
        ),
        r#"<NODE Type="1" Name="One" KeyType="0"><TRACK Key="1"/></NODE>"#,
    );
    let tracks = parse(export.as_bytes())
        .expect("the export parses")
        .playlist_tracks(&[0])
        .expect("a playlist")
        .tracks;
    assert_eq!(tracks[0].title.chars().count(), MAX_TEXT_CHARS);
}
