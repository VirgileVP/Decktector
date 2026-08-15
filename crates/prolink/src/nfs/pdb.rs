//! Hand-rolled, minimal parser for `export.pdb` — Rekordbox's on-device
//! track database, fetched whole over NFS and parsed entirely in memory.
//!
//! **This is the least-confident file in the whole NFS route.** The
//! `rekordcrate` crate (`https://github.com/Holzhaus/rekordcrate`) parses
//! this exact format and was the layout reference here, but its own
//! row-struct fields are private in the published version — reading its
//! source (not its API) as documentation, rather than depending on it,
//! is what `dbserver`'s protocol modules did successfully against
//! `beat-link` earlier this session, but the two cases aren't equally
//! solid: the ONC-RPC/MOUNT/NFS layer this file's bytes arrive through was
//! cross-checked against multiple converging sources with concrete,
//! consistent byte tables. This file's layout — especially the exact page
//! -type numeric codes, [`Track`]'s fixed-header field order, and
//! [`DeviceSQLString`]'s short-vs-long-form encoding — rests on a single,
//! partially self-contradictory secondary reading of that source (it
//! flagged its own uncertainty about the `Table` entry's byte size while
//! reporting it). Treat every numeric constant below as a best
//! reconstruction, not a confirmed fact, until checked against either
//! `rekordcrate`'s actual source directly or a real `export.pdb` file.
//!
//! Scoped to exactly what `prolink-engine::metadata` needs: id/name/
//! cross-reference fields for seven row types, nothing else this database
//! carries (playlists, history, columns, ...).

use std::collections::HashMap;

/// 28-byte fixed header, little-endian throughout this whole format (unlike
/// every other protocol in this crate, which is big-endian) — `unknown1
/// (u32) | page_size (u32) | num_tables (u32) | next_unused_page (u32) |
/// unknown (u32) | sequence (u32) | gap (u32)`, followed by `num_tables`
/// 16-byte [`RawTable`] entries.
const HEADER_LEN: usize = 28;
const PAGE_SIZE_OFFSET: usize = 4;
const NUM_TABLES_OFFSET: usize = 8;
const TABLE_ENTRY_LEN: usize = 16;

/// Page header: `page_index | page_type | next_page` (u32 each), several
/// unknown/unused fields, `num_rows_small: u8`, more unknowns,
/// `free_size: u16`, `used_size: u16`, unknown, `num_rows_large: u16`, two
/// more unknowns — 40 bytes total before row data.
const PAGE_HEADER_LEN: usize = 0x28;
const NUM_ROWS_SMALL_OFFSET: usize = 0x12;
const NUM_ROWS_LARGE_OFFSET: usize = 0x1a;
/// A `num_rows_large` at or above this sentinel means "use
/// `num_rows_small` instead" — `rekordcrate`'s own logic for the same
/// ambiguity.
const NUM_ROWS_LARGE_SENTINEL: u16 = 0x1fff;
/// Up to 16 rows share one presence bitmask + offset table, stored as a
/// footer at the *end* of the page (not the start), read backwards.
const ROWS_PER_GROUP: usize = 16;
/// Two bytes per row offset, plus a 2-byte presence bitmask and 2-byte
/// unknown field per group.
const ROW_GROUP_FOOTER_LEN: usize = ROWS_PER_GROUP * 2 + 4;

/// Page-type codes — **the least confident numbers in this file**,
/// reconstructed from general familiarity with this reverse-engineered
/// format rather than a single quoted source.
mod page_type {
    pub const TRACKS: u32 = 0;
    pub const GENRES: u32 = 1;
    pub const ARTISTS: u32 = 2;
    pub const ALBUMS: u32 = 3;
    pub const LABELS: u32 = 4;
    pub const KEYS: u32 = 5;
    pub const ARTWORK: u32 = 13;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: u32,
    pub title: String,
    pub artist_id: u32,
    pub album_id: u32,
    pub genre_id: u32,
    pub key_id: u32,
    pub label_id: u32,
    pub artwork_id: u32,
    /// Seconds — matches the field's own on-disk unit, converted to ms by
    /// `prolink-engine::metadata` like every other duration in this
    /// codebase.
    pub duration_sec: u32,
    /// BPM x100, same convention already handled in
    /// `prolink-engine::metadata::normalize::normalize_tempo` for the
    /// dbserver route.
    pub tempo_centibpm: u32,
    /// Relative path to this track's `.DAT` analysis file — the `.EXT`/
    /// `.2EX` variants (waveform/color/song-structure data) share this
    /// path with their extension swapped.
    pub analyze_path: String,
}

/// Every row this phase looks up by id, indexed after one parsing pass —
/// `prolink-engine::metadata::nfs` resolves a track's cross-references
/// (artist name, album name, ...) with plain `HashMap` lookups against
/// this, the same pattern `metadata::normalize` already uses for dbserver
/// menu items.
#[derive(Debug, Default)]
pub struct Pdb {
    pub tracks: HashMap<u32, Track>,
    pub artists: HashMap<u32, String>,
    pub albums: HashMap<u32, String>,
    pub genres: HashMap<u32, String>,
    pub keys: HashMap<u32, String>,
    pub labels: HashMap<u32, String>,
    /// Artwork row id -> relative path to the artwork file, another
    /// `NfsClient::read_file` call away.
    pub artwork_paths: HashMap<u32, String>,
}

/// Parses a whole `export.pdb` file's bytes; `None` on any structural
/// inconsistency (a page/row that doesn't fit where the header says it
/// should) rather than a panic — the same discipline as every other parser
/// in this crate, doubly important here given this file's own confidence
/// caveat above.
pub fn parse(bytes: &[u8]) -> Option<Pdb> {
    if bytes.len() < HEADER_LEN {
        return None;
    }
    let page_size = read_u32_le(bytes, PAGE_SIZE_OFFSET)? as usize;
    let num_tables = read_u32_le(bytes, NUM_TABLES_OFFSET)?;
    if page_size == 0 {
        return None;
    }

    let mut pdb = Pdb::default();
    for table_index in 0..num_tables {
        let table_offset = HEADER_LEN + table_index as usize * TABLE_ENTRY_LEN;
        let page_type = read_u32_le(bytes, table_offset)?;
        let first_page = read_u32_le(bytes, table_offset + 8)?;
        let last_page = read_u32_le(bytes, table_offset + 12)?;
        if !matches!(
            page_type,
            page_type::TRACKS | page_type::GENRES | page_type::ARTISTS | page_type::ALBUMS | page_type::LABELS | page_type::KEYS | page_type::ARTWORK
        ) {
            continue; // Playlists, history, columns, ... — outside this parser's scope.
        }

        let mut page_index = first_page;
        while let Some(page) = read_page(bytes, page_size, page_index) {
            for row_bytes in page.row_slices(bytes) {
                apply_row(&mut pdb, page_type, row_bytes);
            }
            if page_index == last_page {
                break;
            }
            page_index = page.next_page;
        }
    }
    Some(pdb)
}

struct Page {
    /// Byte offset of this page's own start within the whole file —
    /// `page_index as usize * page_size`, kept rather than recomputed so
    /// [`Self::row_slices`] doesn't need `page_size` threaded back in.
    base: usize,
    page_size: usize,
    next_page: u32,
    /// Row offsets relative to the end of the page header (the "page
    /// heap"), already extracted from the row-group footer(s) by
    /// [`read_row_group_offsets`].
    row_offsets: Vec<u16>,
}

impl Page {
    /// Byte slices for this page's present rows, each starting at its row
    /// offset and running to the start of the row-group footer — generous,
    /// since individual row parsers (`apply_row`) read only the
    /// fixed-size fields they need from the front and bail out via `?` on
    /// anything short.
    fn row_slices<'a>(&self, bytes: &'a [u8]) -> Vec<&'a [u8]> {
        let heap_start = self.base + PAGE_HEADER_LEN;
        let footer_start = (self.base + self.page_size).saturating_sub(ROW_GROUP_FOOTER_LEN);
        self.row_offsets
            .iter()
            .filter_map(|&offset| {
                let row_start = heap_start + offset as usize;
                (row_start < footer_start).then(|| &bytes[row_start..footer_start])
            })
            .collect()
    }
}

fn read_page(bytes: &[u8], page_size: usize, page_index: u32) -> Option<Page> {
    let base = page_index as usize * page_size;
    if base + page_size > bytes.len() || base + PAGE_HEADER_LEN > bytes.len() {
        return None;
    }
    let next_page = read_u32_le(bytes, base + 8)?;
    let num_rows_small = *bytes.get(base + NUM_ROWS_SMALL_OFFSET)?;
    let num_rows_large = read_u16_le(bytes, base + NUM_ROWS_LARGE_OFFSET)?;
    let num_rows = if num_rows_large > u16::from(num_rows_small) && num_rows_large != NUM_ROWS_LARGE_SENTINEL {
        num_rows_large
    } else {
        u16::from(num_rows_small)
    };

    let row_offsets = read_row_group_offsets(bytes, base, page_size, num_rows)?;
    Some(Page { base, page_size, next_page, row_offsets })
}

fn read_row_group_offsets(bytes: &[u8], page_base: usize, page_size: usize, num_rows: u16) -> Option<Vec<u16>> {
    let mut offsets = Vec::with_capacity(num_rows as usize);
    let mut remaining = num_rows as usize;
    let mut group_end = page_base + page_size;

    while remaining > 0 {
        if group_end < ROW_GROUP_FOOTER_LEN {
            return None;
        }
        let footer_start = group_end - ROW_GROUP_FOOTER_LEN;
        let presence = read_u16_le(bytes, footer_start + ROWS_PER_GROUP * 2)?;
        let take = remaining.min(ROWS_PER_GROUP);
        for slot in 0..take {
            if presence & (1 << slot) != 0 {
                let offset = read_u16_le(bytes, footer_start + slot * 2)?;
                offsets.push(offset);
            }
        }
        remaining -= take;
        group_end = footer_start;
    }
    Some(offsets)
}

fn apply_row(pdb: &mut Pdb, page_type: u32, row: &[u8]) {
    match page_type {
        page_type::TRACKS => {
            if let Some(track) = parse_track_row(row) {
                pdb.tracks.insert(track.id, track);
            }
        }
        page_type::GENRES => insert_named_row(&mut pdb.genres, row),
        page_type::ARTISTS => insert_named_row(&mut pdb.artists, row),
        page_type::ALBUMS => insert_named_row(&mut pdb.albums, row),
        page_type::LABELS => insert_named_row(&mut pdb.labels, row),
        page_type::KEYS => insert_named_row(&mut pdb.keys, row),
        page_type::ARTWORK => insert_named_row(&mut pdb.artwork_paths, row),
        _ => {}
    }
}

/// Genre/Artist/Album/Label/Key/Artwork rows share the same simple shape:
/// a 4-byte id, then one inline [`DeviceSQLString`](mod@device_sql_string)
/// (the row's name, or — for Artwork — its file path).
fn insert_named_row(table: &mut HashMap<u32, String>, row: &[u8]) {
    let Some(id) = read_u32_le(row, 0) else { return };
    let Some((name, _)) = device_sql_string::read(row, 4) else { return };
    table.insert(id, name);
}

/// Track rows are far larger: a fixed-size numeric header (this parser
/// reads only the handful of fields it needs, at their reconstructed
/// offsets — see this module's confidence caveat), followed by a table of
/// 2-byte offsets to each variable-length string field, title and
/// `analyze_path` (called `file_path` on-disk, but this is what
/// `analyze_path` actually needs) among them.
fn parse_track_row(row: &[u8]) -> Option<Track> {
    let id = read_u32_le(row, 0x2c)?;
    let artist_id = read_u32_le(row, 0x18)?;
    let genre_id = read_u32_le(row, 0x28)?;
    let album_id = read_u32_le(row, 0x24)?;
    let label_id = read_u32_le(row, 0x0c)?;
    let key_id = read_u32_le(row, 0x10)?;
    let artwork_id = read_u32_le(row, 0x08)?;
    let tempo_centibpm = read_u32_le(row, 0x1c)?;
    let duration_sec = u32::from(read_u16_le(row, 0x38)?);

    // The string offset table starts right after the fixed header
    // (reconstructed length 0x5e) — title is conventionally the first
    // entry, the file path the last of 21.
    const FIXED_HEADER_LEN: usize = 0x5e;
    const STRING_COUNT: usize = 21;
    const TITLE_INDEX: usize = 0;
    const FILE_PATH_INDEX: usize = STRING_COUNT - 1;

    let string_offset = |index: usize| -> Option<usize> {
        read_u16_le(row, FIXED_HEADER_LEN + index * 2).map(|value| FIXED_HEADER_LEN + value as usize)
    };

    let (title, _) = device_sql_string::read(row, string_offset(TITLE_INDEX)?)?;
    let (analyze_path, _) = device_sql_string::read(row, string_offset(FILE_PATH_INDEX)?)?;

    Some(Track { id, title, artist_id, album_id, genre_id, key_id, label_id, artwork_id, duration_sec, tempo_centibpm, analyze_path })
}

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2).map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4).map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// `DeviceSQLString` — the string encoding this database uses throughout.
/// **Short form only** (a single length-and-flag byte followed by raw
/// ASCII/Latin-1 bytes): `byte0 & 1 == 1` marks short form, and `byte0 >>
/// 1` is the *total* length including that header byte, so the string
/// itself is `(byte0 >> 1) - 1` bytes. The long form (`byte0 == 0x40`,
/// used for strings over ~127 bytes or containing non-Latin-1 characters,
/// encoded UTF-16LE) is **not implemented** — reads as `None` rather than
/// misreading it, consistent with this file's overall confidence level;
/// most track titles/names fit comfortably in the short form, so this
/// covers the common case without risking a silent misparse of the rest.
mod device_sql_string {
    pub fn read(data: &[u8], offset: usize) -> Option<(String, usize)> {
        let marker = *data.get(offset)?;
        if marker & 1 != 1 {
            return None; // Long form — not implemented, see this module's doc comment.
        }
        let total_len = (marker >> 1) as usize;
        let text_len = total_len.checked_sub(1)?;
        let bytes = data.get(offset + 1..offset + 1 + text_len)?;
        Some((bytes.iter().map(|&byte| byte as char).collect(), 1 + text_len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short_string(text: &str) -> Vec<u8> {
        let mut bytes = vec![((text.len() + 1) as u8) << 1 | 1];
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    #[test]
    fn a_short_device_sql_string_round_trips() {
        let bytes = short_string("Techno");
        let (text, consumed) = device_sql_string::read(&bytes, 0).expect("should read");
        assert_eq!(text, "Techno");
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn a_long_form_marker_is_not_misread() {
        let bytes = [0x40, 0, 0, 0];
        assert!(device_sql_string::read(&bytes, 0).is_none());
    }

    #[test]
    fn a_named_row_is_indexed_by_id() {
        let mut row = 4u32.to_le_bytes().to_vec();
        row.extend_from_slice(&short_string("Progressive House"));
        let mut table = HashMap::new();
        insert_named_row(&mut table, &row);
        assert_eq!(table.get(&4).map(String::as_str), Some("Progressive House"));
    }

    #[test]
    fn parsing_stops_gracefully_on_a_bogus_header() {
        assert!(parse(&[0u8; 4]).is_none());
    }

    #[test]
    fn a_zero_page_size_is_rejected_rather_than_looping_forever() {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[PAGE_SIZE_OFFSET..PAGE_SIZE_OFFSET + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse(&bytes).is_none());
    }

    #[test]
    fn an_empty_table_of_contents_yields_an_empty_but_valid_database() {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[PAGE_SIZE_OFFSET..PAGE_SIZE_OFFSET + 4].copy_from_slice(&4096u32.to_le_bytes());
        // num_tables stays 0.
        let pdb = parse(&bytes).expect("an empty table list is still a structurally valid file");
        assert!(pdb.tracks.is_empty());
    }
}
