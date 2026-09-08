//! Hand-rolled, minimal parser for `export.pdb` — Rekordbox's on-device
//! track database, fetched whole over NFS and parsed entirely in memory.
//!
//! Every offset below was read off a real `export.pdb` — the 1 MB database
//! from a rekordbox-exported USB stick in a CDJ-3000, pulled over NFS on
//! 2026-08-19 (`capture/reference-20260819/`) — and cross-checked field by
//! field against the same tracks' metadata as answered independently by
//! that player's dbserver. The version before that was written from a
//! secondary reading of `rekordcrate`'s source and got essentially every
//! structural constant wrong: it found zero tracks in this file. What it
//! got wrong, in case any of it resurfaces:
//!
//!   * a page's fields start after a 4-byte gap — `page_type` is at
//!     `0x08`, not `0x04`, and the row counts at `0x18`/`0x22`;
//!   * pages with `0x40` set in [`PAGE_FLAGS_OFFSET`] hold no rows at all,
//!     and every table's first page is one of them;
//!   * a row group's 16 offsets are stored **backwards** — slot `n`'s
//!     offset is entry `15 - n`;
//!   * a track row's string offsets are relative to the row, not to the
//!     end of its fixed header, and the title is entry 17 of 21, not 0;
//!   * entry 14, not the last one, is the `.DAT` analysis path. The last
//!     is the audio file, which has no analysis data in it.
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

/// Page header: a 4-byte gap, then `page_index | page_type | next_page`
/// (u32 each), unknown fields, `num_rows_small: u8` at `0x18`, page flags
/// at `0x1b`, `free_size`/`used_size` (u16) at `0x1c`/`0x1e`, and
/// `num_rows_large: u16` at `0x22` — 40 bytes total before row data.
const PAGE_HEADER_LEN: usize = 0x28;
const PAGE_TYPE_OFFSET: usize = 0x08;
const NEXT_PAGE_OFFSET: usize = 0x0c;
const NUM_ROWS_SMALL_OFFSET: usize = 0x18;
const PAGE_FLAGS_OFFSET: usize = 0x1b;
const NUM_ROWS_LARGE_OFFSET: usize = 0x22;
/// Set in [`PAGE_FLAGS_OFFSET`] on a page that carries no rows — every
/// table's first page is one, so a parser that doesn't check this reads a
/// header as if it were row data and finds nothing anywhere.
const PAGE_FLAG_NOT_DATA: u8 = 0x40;
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

/// Page-type codes, confirmed against the table directory of a real
/// `export.pdb`.
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
#[derive(Debug, Default, Clone)]
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
    /// Optional cues indexed by track id (e.g. from Rekordbox 7 DeviceLibraryPlus).
    pub cues: HashMap<u32, Vec<crate::nfs::AnlzCue>>,
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

    let max_pages = bytes.len() / page_size + 1;
    let mut pdb = Pdb::default();
    for table_index in 0..num_tables {
        let table_offset = HEADER_LEN + table_index as usize * TABLE_ENTRY_LEN;
        let page_type = read_u32_le(bytes, table_offset)?;
        let first_page = read_u32_le(bytes, table_offset + 8)?;
        let last_page = read_u32_le(bytes, table_offset + 12)?;
        if !matches!(
            page_type,
            page_type::TRACKS
                | page_type::GENRES
                | page_type::ARTISTS
                | page_type::ALBUMS
                | page_type::LABELS
                | page_type::KEYS
                | page_type::ARTWORK
        ) {
            continue; // Playlists, history, columns, ... — outside this parser's scope.
        }

        let mut page_index = first_page;
        let mut pages_visited = 0usize;
        while let Some(page) = read_page(bytes, page_size, page_index) {
            if page.page_type == page_type {
                for row_bytes in page.row_slices(bytes) {
                    apply_row(&mut pdb, page_type, row_bytes);
                }
            }
            // `next_page` on the last page of a table points into the next
            // table's chain, so `last_page` is the stop signal — plus a
            // hard cap, since a corrupt file whose chain loops back on
            // itself would otherwise spin here forever.
            pages_visited += 1;
            if page_index == last_page || page.next_page == page_index || pages_visited > max_pages
            {
                break;
            }
            page_index = page.next_page;
        }
    }
    Some(pdb)
}

struct Page {
    /// The page's own idea of which table it belongs to. Checked against
    /// the table directory's before any row is read: `next_page` on a
    /// table's last page points on into another table's chain, so a walk
    /// that trusted the chain alone would parse album rows as tracks.
    page_type: u32,
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
    let next_page = read_u32_le(bytes, base + NEXT_PAGE_OFFSET)?;

    // A non-data page still has to be walked *through* — it holds the link
    // to the rest of the table — it just has no rows to read out of it.
    let page_type = read_u32_le(bytes, base + PAGE_TYPE_OFFSET)?;
    if bytes.get(base + PAGE_FLAGS_OFFSET)? & PAGE_FLAG_NOT_DATA != 0 {
        return Some(Page {
            base,
            page_size,
            page_type,
            next_page,
            row_offsets: Vec::new(),
        });
    }

    let num_rows_small = *bytes.get(base + NUM_ROWS_SMALL_OFFSET)?;
    let num_rows_large = read_u16_le(bytes, base + NUM_ROWS_LARGE_OFFSET)?;
    let num_rows = if num_rows_large > u16::from(num_rows_small)
        && num_rows_large != NUM_ROWS_LARGE_SENTINEL
    {
        num_rows_large
    } else {
        u16::from(num_rows_small)
    };

    let row_offsets = read_row_group_offsets(bytes, base, page_size, num_rows)?;
    Some(Page {
        base,
        page_size,
        page_type,
        next_page,
        row_offsets,
    })
}

fn read_row_group_offsets(
    bytes: &[u8],
    page_base: usize,
    page_size: usize,
    num_rows: u16,
) -> Option<Vec<u16>> {
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
                // Backwards: the offsets are stored last-slot-first, so
                // slot 0 lives in the final entry of the block. Reading
                // them in order yields the zero-filled unused entries for
                // every page that isn't exactly full.
                let entry = ROWS_PER_GROUP - 1 - slot;
                let offset = read_u16_le(bytes, footer_start + entry * 2)?;
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
        page_type::GENRES => insert_inline_row(&mut pdb.genres, row, 0x04),
        page_type::LABELS => insert_inline_row(&mut pdb.labels, row, 0x04),
        page_type::ARTWORK => insert_inline_row(&mut pdb.artwork_paths, row, 0x04),
        // A key row repeats its id before the name.
        page_type::KEYS => insert_inline_row(&mut pdb.keys, row, 0x08),
        page_type::ARTISTS => insert_indirect_row(&mut pdb.artists, row, 0x04, 0x09),
        page_type::ALBUMS => insert_indirect_row(&mut pdb.albums, row, 0x0c, 0x15),
        _ => {}
    }
}

/// Genre, label, key and artwork rows: a 4-byte id at the start, then the
/// row's single [`DeviceSQLString`](mod@device_sql_string) — its name, or
/// for artwork its file path — inline at a fixed offset.
fn insert_inline_row(table: &mut HashMap<u32, String>, row: &[u8], name_offset: usize) {
    let Some(id) = read_u32_le(row, 0) else {
        return;
    };
    let Some((name, _)) = device_sql_string::read(row, name_offset) else {
        return;
    };
    table.insert(id, name);
}

/// Artist and album rows, which put their name somewhere else in the row
/// and store *where* in a single byte: `id_offset` locates the row id,
/// `name_pointer_offset` the byte holding the name's offset from the start
/// of the row.
///
/// (Both row types also carry a subtype word saying whether that pointer
/// is one byte or two. Only the one-byte form has ever been observed here,
/// and a two-byte pointer would read as a plausible-but-wrong offset
/// rather than as garbage, so the string parse below is what catches it:
/// no valid string there, no row.)
fn insert_indirect_row(
    table: &mut HashMap<u32, String>,
    row: &[u8],
    id_offset: usize,
    name_pointer_offset: usize,
) {
    let Some(id) = read_u32_le(row, id_offset) else {
        return;
    };
    let Some(&name_offset) = row.get(name_pointer_offset) else {
        return;
    };
    let Some((name, _)) = device_sql_string::read(row, usize::from(name_offset)) else {
        return;
    };
    table.insert(id, name);
}

/// Track rows are far larger: a fixed-size numeric header (this parser
/// reads only the handful of fields it needs, at their reconstructed
/// offsets — see this module's confidence caveat), followed by a table of
/// 2-byte offsets to each variable-length string field, title and
/// `analyze_path` (called `file_path` on-disk, but this is what
/// `analyze_path` actually needs) among them.
fn parse_track_row(row: &[u8]) -> Option<Track> {
    let artwork_id = read_u32_le(row, 0x1c)?;
    let key_id = read_u32_le(row, 0x20)?;
    let label_id = read_u32_le(row, 0x28)?;
    let tempo_centibpm = read_u32_le(row, 0x38)?;
    let genre_id = read_u32_le(row, 0x3c)?;
    let album_id = read_u32_le(row, 0x40)?;
    let artist_id = read_u32_le(row, 0x44)?;
    let id = read_u32_le(row, 0x48)?;
    let duration_sec = u32::from(read_u16_le(row, 0x54)?);

    // 21 two-byte string offsets follow the fixed header, each relative to
    // the start of the row.
    const STRING_TABLE_OFFSET: usize = 0x5e;
    const TITLE_INDEX: usize = 17;
    /// The `.DAT` this track's analysis lives in — the `.EXT`/`.2EX`
    /// variants share the path with the extension swapped. Deliberately
    /// not entry 20, which is the audio file itself.
    const ANALYZE_PATH_INDEX: usize = 14;

    let string_offset = |index: usize| -> Option<usize> {
        read_u16_le(row, STRING_TABLE_OFFSET + index * 2).map(usize::from)
    };

    // A string this parser can't read (the long form — see
    // `device_sql_string`) costs that one field, not the whole row: a track
    // with an unreadable title still has an id, a tempo and a duration
    // worth having, and the dbserver route can fill the rest in.
    let title = device_sql_string::read(row, string_offset(TITLE_INDEX)?)
        .map(|(text, _)| text)
        .unwrap_or_default();
    let analyze_path = device_sql_string::read(row, string_offset(ANALYZE_PATH_INDEX)?)
        .map(|(text, _)| text)
        .unwrap_or_default();

    Some(Track {
        id,
        title,
        artist_id,
        album_id,
        genre_id,
        key_id,
        label_id,
        artwork_id,
        duration_sec,
        tempo_centibpm,
        analyze_path,
    })
}

fn read_u16_le(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// `DeviceSQLString` — the string encoding this database uses throughout,
/// in the three forms a real library turns out to contain.
///
/// The **short form** is marked by `byte0 & 1 == 1`, where `byte0 >> 1` is
/// the *total* length including that header byte, so the text is
/// `(byte0 >> 1) - 1` bytes of Latin-1.
///
/// The two **long forms** share a four-byte header — the marker, a
/// little-endian total length covering the header itself, and one byte
/// whose meaning is unknown — followed by `length - 4` bytes of text:
/// Latin-1 under marker `0x40`, UTF-16**LE** under `0x90`. The endianness
/// is worth stating because Deep Symmetry's `crate-digger` names the same
/// case `utf16be`: against the two real `export.pdb` databases recovered
/// from the 2026-08-19 Cabaret Vert capture, every long string decodes as
/// little-endian and none as big-endian, matching the UTF-16LE filenames
/// this crate's [`super::xdr`] already documents Pioneer using elsewhere.
///
/// Both long forms were previously read as `None`. That left 50 of the
/// 1,864 tracks across those two databases — long titles, and any title
/// carrying a character outside Latin-1 — with an empty title, which is
/// what surfaced them.
mod device_sql_string {
    /// Long form holding Latin-1 text: used once a string outgrows the
    /// seven-bit length the short form's single header byte can express.
    const KIND_LONG_LATIN1: u8 = 0x40;
    /// Long form holding UTF-16LE text: used for anything Latin-1 cannot
    /// represent, whatever its length.
    const KIND_LONG_UTF16LE: u8 = 0x90;
    /// Marker, little-endian total length, one unknown byte.
    const LONG_HEADER_LEN: usize = 4;

    pub fn read(data: &[u8], offset: usize) -> Option<(String, usize)> {
        let marker = *data.get(offset)?;
        if marker & 1 == 1 {
            return read_short(data, offset, marker);
        }
        match marker {
            KIND_LONG_LATIN1 | KIND_LONG_UTF16LE => read_long(data, offset, marker),
            // Anything else is not a string this format defines; reading on
            // would silently misalign every field after it.
            _ => None,
        }
    }

    fn read_short(data: &[u8], offset: usize, marker: u8) -> Option<(String, usize)> {
        let text_len = usize::from(marker >> 1).checked_sub(1)?;
        let bytes = data.get(offset + 1..offset + 1 + text_len)?;
        Some((latin1(bytes), 1 + text_len))
    }

    fn read_long(data: &[u8], offset: usize, marker: u8) -> Option<(String, usize)> {
        let length = usize::from(u16::from_le_bytes([
            *data.get(offset + 1)?,
            *data.get(offset + 2)?,
        ]));
        let text_len = length.checked_sub(LONG_HEADER_LEN)?;
        let bytes = data.get(offset + LONG_HEADER_LEN..offset + LONG_HEADER_LEN + text_len)?;
        let text = match marker {
            KIND_LONG_UTF16LE => utf16le(bytes)?,
            _ => latin1(bytes),
        };
        Some((text, length))
    }

    fn latin1(bytes: &[u8]) -> String {
        bytes.iter().map(|&byte| byte as char).collect()
    }

    /// `None` on an odd byte count: that is a misread offset rather than a
    /// string, and half a code unit would decode to nonsense.
    fn utf16le(bytes: &[u8]) -> Option<String> {
        if !bytes.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        Some(String::from_utf16_lossy(&units))
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
    fn a_long_latin1_device_sql_string_round_trips() {
        // Marker, little-endian total length (header included), one unknown
        // byte, then the text — the shape every long string in a real
        // `export.pdb` has.
        let text = "House, Techno (Peak Time / Driving), Trance (Main Floor)";
        let mut bytes = vec![0x40];
        bytes.extend_from_slice(&((text.len() + 4) as u16).to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(text.as_bytes());

        let (parsed, consumed) = device_sql_string::read(&bytes, 0).expect("should parse");
        assert_eq!(parsed, text);
        assert_eq!(
            consumed,
            bytes.len(),
            "a long string consumes its whole declared length"
        );
    }

    /// A title carrying anything outside Latin-1 switches to UTF-16, which
    /// this database writes little-endian.
    #[test]
    fn a_long_utf16_device_sql_string_round_trips() {
        let text = "Gabriel Muñoz – Lost";
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut bytes = vec![0x90];
        bytes.extend_from_slice(&((encoded.len() + 4) as u16).to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&encoded);

        let (parsed, consumed) = device_sql_string::read(&bytes, 0).expect("should parse");
        assert_eq!(parsed, text);
        assert_eq!(consumed, bytes.len());
    }

    /// A marker this format does not define means the offset is wrong;
    /// reading on from it would misalign every field that follows.
    #[test]
    fn an_unknown_marker_is_rejected() {
        assert!(device_sql_string::read(&[0x42, 0, 0, 0], 0).is_none());
    }

    /// Half a UTF-16 code unit is a misread offset, not a string.
    #[test]
    fn an_odd_length_utf16_string_is_rejected() {
        let bytes = [0x90, 7, 0, 0, b'a', b'b', b'c'];
        assert!(device_sql_string::read(&bytes, 0).is_none());
    }

    #[test]
    fn an_inline_named_row_is_indexed_by_id() {
        let mut row = 4u32.to_le_bytes().to_vec();
        row.extend_from_slice(&short_string("Progressive House"));
        let mut table = HashMap::new();
        insert_inline_row(&mut table, &row, 0x04);
        assert_eq!(table.get(&4).map(String::as_str), Some("Progressive House"));
    }

    /// An artist row, in the shape a CDJ-3000's `export.pdb` writes: the id
    /// at `0x04` and, at `0x09`, the offset of the name rather than the
    /// name itself.
    #[test]
    fn an_indirect_named_row_follows_its_name_pointer() {
        let mut row = vec![0u8; 0x0a];
        row[0x00] = 0x60; // subtype
        row[0x04] = 7; // id
        row[0x09] = 0x0a; // where the name starts
        row.extend_from_slice(&short_string("Trinix"));

        let mut table = HashMap::new();
        insert_indirect_row(&mut table, &row, 0x04, 0x09);
        assert_eq!(table.get(&7).map(String::as_str), Some("Trinix"));
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
