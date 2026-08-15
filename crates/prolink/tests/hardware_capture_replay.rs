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

use std::io::Cursor;
use std::path::{Path, PathBuf};

use binrw::BinRead;
use prolink::beat::parse_beat_message;
use prolink::capture::{read_jsonl, UdpRecord};
use prolink::nfs::pdb;
use prolink::status::parse_status;
use prolink::{BEAT_PORT, STATUS_PORT};
use rekordcrate::anlz::{Content, ANLZ};

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
    let bytes = build_minimal_pdb_bytes();
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

/// Builds a minimal, structurally valid `export.pdb`: one page per table
/// (tracks, genres, artists, albums, labels, keys), one row each, laid out
/// by hand against `crates/prolink/src/nfs/pdb.rs`'s documented byte
/// offsets. Page indices start at 1 (page 0's file region holds the header
/// and table-of-contents instead, which this simple layout leaves unused
/// past that).
fn build_minimal_pdb_bytes() -> Vec<u8> {
    const PAGE_SIZE: u32 = 512;
    const HEADER_LEN: usize = 28;
    const TABLE_ENTRY_LEN: usize = 16;
    const PAGE_HEADER_LEN: usize = 0x28;

    // page_type codes, matching `prolink::nfs::pdb`'s private `page_type` module.
    const TRACKS: u32 = 0;
    const GENRES: u32 = 1;
    const ARTISTS: u32 = 2;
    const ALBUMS: u32 = 3;
    const LABELS: u32 = 4;
    const KEYS: u32 = 5;

    let named_tables = [(GENRES, 10u32, "Test Genre"), (ARTISTS, 20, "Test Artist"), (ALBUMS, 30, "Test Album"), (LABELS, 40, "Test Label"), (KEYS, 50, "Test Key")];
    let page_types: Vec<u32> = std::iter::once(TRACKS).chain(named_tables.iter().map(|&(page_type, ..)| page_type)).collect();
    let num_tables = page_types.len();

    let total_pages = 1 + num_tables; // Page 0 unused (see this fn's doc comment).
    let mut file = vec![0u8; total_pages * PAGE_SIZE as usize];

    // Header.
    write_u32_le(&mut file, 4, PAGE_SIZE);
    write_u32_le(&mut file, 8, num_tables as u32);

    // Table of contents + pages, one page index (1-based) per table, in order.
    for (table_index, &page_type) in page_types.iter().enumerate() {
        let page_index = (table_index + 1) as u32;
        let toc_offset = HEADER_LEN + table_index * TABLE_ENTRY_LEN;
        write_u32_le(&mut file, toc_offset, page_type);
        write_u32_le(&mut file, toc_offset + 8, page_index); // first_page
        write_u32_le(&mut file, toc_offset + 12, page_index); // last_page

        let row = if page_type == TRACKS {
            build_track_row()
        } else {
            let (_, id, name) = named_tables.iter().find(|&&(pt, ..)| pt == page_type).expect("known page type");
            build_named_row(*id, name)
        };
        write_single_row_page(&mut file, page_index, PAGE_SIZE, PAGE_HEADER_LEN, &row);
    }

    file
}

fn write_single_row_page(file: &mut [u8], page_index: u32, page_size: u32, page_header_len: usize, row: &[u8]) {
    let base = page_index as usize * page_size as usize;
    write_u32_le(file, base + 8, page_index); // next_page — unread once first_page == last_page.
    file[base + 0x12] = 1; // num_rows_small = 1.
    write_u16_le(file, base + 0x1a, 0); // num_rows_large = 0 (so num_rows_small wins).

    let heap_start = base + page_header_len;
    file[heap_start..heap_start + row.len()].copy_from_slice(row);

    const ROW_GROUP_FOOTER_LEN: usize = 16 * 2 + 4;
    let footer_start = base + page_size as usize - ROW_GROUP_FOOTER_LEN;
    write_u16_le(file, footer_start, 0); // row 0's offset, relative to heap_start.
    write_u16_le(file, footer_start + 16 * 2, 0b1); // presence bitmask: row 0 present.
}

/// A genre/artist/album/label/key row: a 4-byte id, then a short-form
/// `DeviceSQLString` (see `pdb.rs`'s `device_sql_string` module doc).
fn build_named_row(id: u32, name: &str) -> Vec<u8> {
    let mut row = id.to_le_bytes().to_vec();
    row.extend_from_slice(&short_device_sql_string(name));
    row
}

/// A track row: the handful of fixed-offset numeric fields
/// `parse_track_row` reads, plus title (string index 0) and `analyze_path`
/// (string index 20) placed right after the 21-entry string-offset table —
/// every other string slot is left zeroed since `parse_track_row` never
/// reads them.
fn build_track_row() -> Vec<u8> {
    const FIXED_HEADER_LEN: usize = 0x5e;
    const STRING_COUNT: usize = 21;

    let title = short_device_sql_string("Test Track");
    let analyze_path = short_device_sql_string("ANLZ0000.DAT");
    let title_offset = FIXED_HEADER_LEN + STRING_COUNT * 2;
    let analyze_path_offset = title_offset + title.len();
    let mut row = vec![0u8; analyze_path_offset + analyze_path.len()];

    write_u32_le(&mut row, 0x2c, 4207); // id
    write_u32_le(&mut row, 0x18, 20); // artist_id
    write_u32_le(&mut row, 0x28, 10); // genre_id
    write_u32_le(&mut row, 0x24, 30); // album_id
    write_u32_le(&mut row, 0x0c, 40); // label_id
    write_u32_le(&mut row, 0x10, 50); // key_id
    write_u32_le(&mut row, 0x08, 0); // artwork_id (0 = none)
    write_u32_le(&mut row, 0x1c, 12_800); // tempo_centibpm: 128.00 BPM
    write_u16_le(&mut row, 0x38, 240); // duration_sec

    write_u16_le(&mut row, FIXED_HEADER_LEN, (title_offset - FIXED_HEADER_LEN) as u16); // string index 0 (title)
    write_u16_le(&mut row, FIXED_HEADER_LEN + 20 * 2, (analyze_path_offset - FIXED_HEADER_LEN) as u16); // string index 20 (file path)

    row[title_offset..title_offset + title.len()].copy_from_slice(&title);
    row[analyze_path_offset..analyze_path_offset + analyze_path.len()].copy_from_slice(&analyze_path);
    row
}

/// Short-form `DeviceSQLString` encoding: `marker = (len(text) + 1) << 1 | 1`
/// followed by the raw text bytes — mirrors `pdb.rs`'s own test helper of
/// the same shape (`device_sql_string::tests::short_string`).
fn short_device_sql_string(text: &str) -> Vec<u8> {
    let mut bytes = vec![((text.len() + 1) as u8) << 1 | 1];
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

fn write_u16_le(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_le(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn ext_analysis_fixtures_yield_a_beat_grid_and_a_waveform() {
    let sessions = capture_sessions();
    if sessions.is_empty() {
        eprintln!("no fixtures under {}; skipping (see this file's module doc)", fixtures_dir().display());
        return;
    }

    let (mut checked_any, mut saw_beat_grid, mut saw_waveform) = (false, false, false);
    for session in &sessions {
        for ext_path in find_files(session, "analysis", |file| file.extension().and_then(|e| e.to_str()) == Some("EXT")) {
            checked_any = true;
            let bytes = std::fs::read(&ext_path).unwrap_or_else(|error| panic!("failed to read {}: {error}", ext_path.display()));
            let mut cursor = Cursor::new(bytes);
            let anlz = ANLZ::read(&mut cursor).unwrap_or_else(|error| panic!("{} did not parse as ANLZ: {error}", ext_path.display()));

            for section in &anlz.sections {
                match &section.content {
                    Content::BeatGrid(_) => saw_beat_grid = true,
                    Content::WaveformColorDetail(_) => saw_waveform = true,
                    _ => {}
                }
            }
        }
    }

    if !checked_any {
        eprintln!("no analysis/*.EXT found under any session's nfs/ directory; skipping");
        return;
    }
    assert!(saw_beat_grid, "no .EXT fixture across any session yielded a beat grid section");
    assert!(saw_waveform, "no .EXT fixture across any session yielded an HD color waveform section");
}
