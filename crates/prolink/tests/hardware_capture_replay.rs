//! Replays recorded Pro DJ Link hardware capture sessions (see
//! `HARDWARE-CAPTURE-PLAN.md`, repo root) through this crate's parsers,
//! entirely offline — the whole point of the capture tooling: after a
//! hardware session, every remaining question about the still-unvalidated
//! parsers (`crates/prolink/src/nfs/pdb.rs` especially) is answered here,
//! with `cargo test`, no hardware in the room.
//!
//! Drop one or more `capture-YYYYMMDD-HHMMSS/` session directories, as
//! produced by the `prolink-capture` binary, under
//! `crates/prolink/fixtures/hardware-capture/` (gitignored — a real capture
//! may hold a DJ's library metadata and must never land in the repo) to
//! exercise these tests for real. With none present, every test here skips
//! cleanly rather than failing, so CI stays green before any capture
//! exists.

mod support;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use binrw::BinRead;
use prolink::beat::parse_beat_message;
use prolink::capture::{read_jsonl, UdpRecord};
use prolink::nfs::pdb;
use prolink::status::parse_status;
use prolink::{BEAT_PORT, STATUS_PORT};
use rekordcrate::anlz::{Content, ContentKind, Header, Section};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/hardware-capture")
}

/// Every `capture-*` session directory present under the fixtures dir.
/// Empty (never an error) when the directory is absent or has nothing in
/// it — the "skip cleanly with no fixtures" behavior every test here relies
/// on.
fn capture_sessions() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(fixtures_dir()) else { return Vec::new() };
    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("capture-")))
        .collect()
}

/// Every file directly under `session/nfs/*/` (one player-slot directory
/// per subfolder) whose name matches `predicate` — used to find
/// `export.pdb` and `analysis/*.EXT` files without hard-coding which
/// player-slot directories exist.
fn find_files(session: &Path, relative_glob_dir: &str, predicate: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let nfs_dir = session.join("nfs");
    let Ok(player_dirs) = std::fs::read_dir(&nfs_dir) else { return Vec::new() };

    let mut found = Vec::new();
    for player_dir in player_dirs.filter_map(|entry| entry.ok()).map(|entry| entry.path()) {
        let search_dir = if relative_glob_dir.is_empty() { player_dir } else { player_dir.join(relative_glob_dir) };
        let Ok(files) = std::fs::read_dir(&search_dir) else { continue };
        for file in files.filter_map(|entry| entry.ok()).map(|entry| entry.path()) {
            if file.is_file() && predicate(&file) {
                found.push(file);
            }
        }
    }
    found
}

#[test]
fn replays_udp_traffic_through_the_status_and_beat_parsers_without_panicking() {
    let sessions = capture_sessions();
    if sessions.is_empty() {
        eprintln!("no fixtures under {}; skipping (see this file's module doc)", fixtures_dir().display());
        return;
    }

    for session in sessions {
        let udp_path = session.join("udp.jsonl");
        let read = read_jsonl::<UdpRecord>(&udp_path).unwrap_or_else(|error| panic!("failed to read {}: {error}", udp_path.display()));
        if read.truncated {
            eprintln!("{}: ends with a truncated record (expected for a capture cut off mid-write)", udp_path.display());
        }

        let (mut status_total, mut status_decoded) = (0u32, 0u32);
        let (mut beat_total, mut beat_decoded) = (0u32, 0u32);

        for record in &read.records {
            let Some(payload) = record.payload() else {
                panic!("{}: record with port {} has hex that doesn't decode to bytes", udp_path.display(), record.port);
            };
            match record.port {
                STATUS_PORT => {
                    status_total += 1;
                    // The assertion this test makes is implicit: a panic
                    // here fails the test. `parse_status` returning `None`
                    // is a legitimate, already-covered outcome (an
                    // unrecognized packet shape), not a failure.
                    if parse_status(&payload, record.src.parse().unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))).is_some() {
                        status_decoded += 1;
                    }
                }
                BEAT_PORT => {
                    beat_total += 1;
                    if parse_beat_message(&payload).is_some() {
                        beat_decoded += 1;
                    }
                }
                _ => {}
            }
        }

        eprintln!(
            "{}: status {status_decoded}/{status_total} decoded, beat {beat_decoded}/{beat_total} decoded",
            session.file_name().and_then(|name| name.to_str()).unwrap_or("?")
        );
    }
}

#[test]
fn export_pdb_fixtures_parse_with_resolved_cross_references() {
    let sessions = capture_sessions();
    if sessions.is_empty() {
        eprintln!("no fixtures under {}; skipping (see this file's module doc)", fixtures_dir().display());
        return;
    }

    let mut checked_any = false;
    for session in &sessions {
        for pdb_path in find_files(session, "", |file| file.file_name().and_then(|n| n.to_str()) == Some("export.pdb")) {
            checked_any = true;
            let bytes = std::fs::read(&pdb_path).unwrap_or_else(|error| panic!("failed to read {}: {error}", pdb_path.display()));
            let parsed = pdb::parse(&bytes).unwrap_or_else(|| panic!("{} did not parse — tables should resolve against a real export.pdb", pdb_path.display()));

            assert!(!parsed.tracks.is_empty(), "{}: parsed zero tracks from a real export.pdb", pdb_path.display());

            for track in parsed.tracks.values() {
                assert!(!track.title.trim().is_empty(), "{}: track {} has an empty title", pdb_path.display(), track.id);

                // 0 conventionally means "no cross-reference" in this
                // format (see `prolink-engine::metadata::nfs`'s handling of
                // the same tables) — only a non-zero id that fails to
                // resolve is a real parser bug.
                let resolves = |id: u32, table: &std::collections::HashMap<u32, String>| id == 0 || table.contains_key(&id);
                assert!(resolves(track.artist_id, &parsed.artists), "{}: track {}'s artist_id {} does not resolve", pdb_path.display(), track.id, track.artist_id);
                assert!(resolves(track.album_id, &parsed.albums), "{}: track {}'s album_id {} does not resolve", pdb_path.display(), track.id, track.album_id);
                assert!(resolves(track.genre_id, &parsed.genres), "{}: track {}'s genre_id {} does not resolve", pdb_path.display(), track.id, track.genre_id);
                assert!(resolves(track.key_id, &parsed.keys), "{}: track {}'s key_id {} does not resolve", pdb_path.display(), track.id, track.key_id);
                assert!(resolves(track.label_id, &parsed.labels), "{}: track {}'s label_id {} does not resolve", pdb_path.display(), track.id, track.label_id);
            }
        }
    }

    if !checked_any {
        eprintln!("no export.pdb found under any session's nfs/ directory; skipping");
    }
}

/// Exercises the same cross-reference assertions as
/// [`export_pdb_fixtures_parse_with_resolved_cross_references`], but
/// against a hand-built, structurally valid `export.pdb` rather than a real
/// fixture — so this crate's understanding of the format (see
/// `crates/prolink/src/nfs/pdb.rs`'s own confidence caveat) is exercised by
/// every `cargo test` run, not only once real hardware fixtures exist.
#[test]
fn a_synthetic_export_pdb_parses_with_resolved_cross_references() {
    let bytes = support::build_minimal_pdb_bytes();
    let parsed = pdb::parse(&bytes).expect("a well-formed synthetic export.pdb should parse");

    assert_eq!(parsed.tracks.len(), 1);
    let track = parsed.tracks.values().next().expect("one track");
    assert_eq!(track.title, "Test Track");
    assert_eq!(track.analyze_path, "ANLZ0000.DAT");
    assert_eq!(parsed.artists.get(&track.artist_id).map(String::as_str), Some("Test Artist"));
    assert_eq!(parsed.albums.get(&track.album_id).map(String::as_str), Some("Test Album"));
    assert_eq!(parsed.genres.get(&track.genre_id).map(String::as_str), Some("Test Genre"));
    assert_eq!(parsed.keys.get(&track.key_id).map(String::as_str), Some("Test Key"));
    assert_eq!(parsed.labels.get(&track.label_id).map(String::as_str), Some("Test Label"));
}

/// Reads an ANLZ file section by section, keeping the ones that parse.
///
/// Mirrors `prolink_engine::metadata::nfs`'s reader of the same shape, and
/// duplicated rather than shared because that crate depends on this one,
/// not the other way around. The reason both exist: `ANLZ::read` gives up
/// on the whole file if any single section fails, and a real CDJ-3000
/// `.EXT` contains one that `rekordcrate` 0.3 rejects (an extended cue
/// whose `cue_type` it doesn't recognize) — sitting after the waveform and
/// beat grid that make the file worth reading.
fn read_sections_leniently(bytes: &[u8]) -> Vec<Section> {
    const HEADER_LEN: usize = 12;

    let Ok(file_header) = Header::read(&mut Cursor::new(bytes)) else { return Vec::new() };
    if file_header.kind != ContentKind::File {
        return Vec::new();
    }

    let mut position = file_header.size as usize;
    let end = (file_header.total_size as usize).min(bytes.len());
    let mut sections = Vec::new();
    while position < end {
        let Some(remaining) = bytes.get(position..end) else { break };
        let Ok(header) = Header::read(&mut Cursor::new(remaining)) else { break };
        let Some(section_bytes) = usize::try_from(header.total_size).ok().filter(|size| *size >= HEADER_LEN).and_then(|size| remaining.get(..size)) else {
            break;
        };
        if let Ok(section) = Section::read_options(&mut Cursor::new(section_bytes), binrw::Endian::Big, ()) {
            sections.push(section);
        }
        position += section_bytes.len();
    }
    sections
}

/// The beat grid and the waveform come from *different* files: `PQTZ` (the
/// only beat grid `rekordcrate` reads) is in the `.DAT`, the HD colour
/// waveform only in the `.EXT`. Both are checked together, since a session
/// that pulled one always pulled the other.
#[test]
fn analysis_fixtures_yield_a_beat_grid_and_a_waveform() {
    let sessions = capture_sessions();
    if sessions.is_empty() {
        eprintln!("no fixtures under {}; skipping (see this file's module doc)", fixtures_dir().display());
        return;
    }

    let (mut checked_any, mut saw_beat_grid, mut saw_waveform, mut saw_cues) = (false, false, false, false);
    for session in &sessions {
        let is_analysis = |file: &Path| matches!(file.extension().and_then(|e| e.to_str()), Some("EXT" | "DAT"));
        for ext_path in find_files(session, "analysis", is_analysis) {
            checked_any = true;
            let bytes = std::fs::read(&ext_path).unwrap_or_else(|error| panic!("failed to read {}: {error}", ext_path.display()));
            let sections = read_sections_leniently(&bytes);
            assert!(!sections.is_empty(), "{} yielded no readable sections at all", ext_path.display());

            for section in &sections {
                match &section.content {
                    Content::BeatGrid(_) => saw_beat_grid = true,
                    Content::WaveformColorDetail(_) => saw_waveform = true,
                    Content::CueList(list) if !list.cues.is_empty() => saw_cues = true,
                    _ => {}
                }
            }
        }
    }

    if !checked_any {
        eprintln!("no analysis/*.DAT or *.EXT found under any session's nfs/ directory; skipping");
        return;
    }
    assert!(saw_beat_grid, "no .DAT fixture across any session yielded a beat grid section");
    assert!(saw_waveform, "no .EXT fixture across any session yielded an HD color waveform section");

    // Cues are deliberately *not* asserted, and `saw_cues` is tracked only
    // to document why. Neither cue format survives `rekordcrate` 0.3 on
    // this hardware: the `.EXT`'s `PCO2` is rejected outright (an extended
    // cue whose `cue_type` it doesn't know), and the `.DAT`'s older `PCOB`
    // yields nothing either, even for a track whose raw bytes plainly hold
    // three hot cues — `capture/reference-20260819/`'s `5.DAT` carries a
    // 168-byte `PCOB` with `len_cues = 3`. Reading them needs cue parsers
    // of our own, the same gap `prolink::dbserver::cues` already documents
    // on the other route. Until then a track arrives with no cues rather
    // than with wrong ones.
    if saw_cues {
        eprintln!("note: a fixture yielded cues — `rekordcrate` may have gained support; the assertion above can be tightened");
    }
}
