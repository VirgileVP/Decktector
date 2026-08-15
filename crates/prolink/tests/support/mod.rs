//! Shared test-only fixture builders, used by more than one file under
//! `crates/prolink/tests/` — not a test target itself (see the `tests/`
//! module-sharing convention: files directly in `tests/` each become their
//! own binary, files nested one level under `tests/<dir>/` don't).

/// Builds a minimal, structurally valid `export.pdb`: one page per table
/// (tracks, genres, artists, albums, labels, keys), one row each, laid out
/// by hand against `crates/prolink/src/nfs/pdb.rs`'s documented byte
/// offsets. Page indices start at 1 (page 0's file region holds the header
/// and table-of-contents instead, which this simple layout leaves unused
/// past that).
pub fn build_minimal_pdb_bytes() -> Vec<u8> {
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
