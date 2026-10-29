//! Synthetic fixtures shared by prolink's own integration tests and by
//! downstream crates' tests (`prolink-engine` enables the `test-support`
//! feature in its dev-dependencies). Not part of the protocol client: it is
//! compiled only when that feature is on.

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

    let named_tables = [
        (GENRES, 10u32, "Test Genre"),
        (ARTISTS, 20, "Test Artist"),
        (ALBUMS, 30, "Test Album"),
        (LABELS, 40, "Test Label"),
        (KEYS, 50, "Test Key"),
    ];
    let page_types: Vec<u32> = std::iter::once(TRACKS)
        .chain(named_tables.iter().map(|&(page_type, ..)| page_type))
        .collect();
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
            let (_, id, name) = named_tables
                .iter()
                .find(|&&(pt, ..)| pt == page_type)
                .expect("known page type");
            build_named_row(page_type, *id, name)
        };
        write_single_row_page(
            &mut file,
            page_index,
            page_type,
            PAGE_SIZE,
            PAGE_HEADER_LEN,
            &row,
        );
    }

    file
}

fn write_single_row_page(
    file: &mut [u8],
    page_index: u32,
    page_type: u32,
    page_size: u32,
    page_header_len: usize,
    row: &[u8],
) {
    let base = page_index as usize * page_size as usize;
    write_u32_le(file, base + 0x04, page_index);
    // A page states its own table, and the parser checks it against the
    // table directory before reading any row out of it.
    write_u32_le(file, base + 0x08, page_type);
    write_u32_le(file, base + 0x0c, page_index); // next_page — unread once first_page == last_page.
    file[base + 0x18] = 1; // num_rows_small = 1.
    file[base + 0x1b] = 0x24; // page flags: a data page (0x40 clear).
    write_u16_le(file, base + 0x22, 0); // num_rows_large = 0 (so num_rows_small wins).

    let heap_start = base + page_header_len;
    file[heap_start..heap_start + row.len()].copy_from_slice(row);

    const ROW_GROUP_FOOTER_LEN: usize = 16 * 2 + 4;
    let footer_start = base + page_size as usize - ROW_GROUP_FOOTER_LEN;
    // Row offsets are stored backwards: slot 0 is the *last* of the 16.
    write_u16_le(file, footer_start + 15 * 2, 0); // row 0's offset, relative to heap_start.
    write_u16_le(file, footer_start + 16 * 2, 0b1); // presence bitmask: row 0 present.
}

/// One name row, in whichever of the three shapes its table uses (see
/// `pdb.rs`'s `apply_row`): genres and labels put the string straight after
/// the id, keys repeat the id first, and artists/albums point at it with a
/// one-byte offset from elsewhere in the row.
fn build_named_row(page_type: u32, id: u32, name: &str) -> Vec<u8> {
    const ARTISTS: u32 = 2;
    const ALBUMS: u32 = 3;
    const KEYS: u32 = 5;

    let string = short_device_sql_string(name);
    match page_type {
        ARTISTS => {
            let name_offset = 0x0a;
            let mut row = vec![0u8; name_offset];
            write_u16_le(&mut row, 0x00, 0x0060); // subtype: one-byte name pointer.
            write_u32_le(&mut row, 0x04, id);
            row[0x09] = name_offset as u8;
            row.extend_from_slice(&string);
            row
        }
        ALBUMS => {
            let name_offset = 0x16;
            let mut row = vec![0u8; name_offset];
            write_u16_le(&mut row, 0x00, 0x0080);
            write_u32_le(&mut row, 0x0c, id);
            row[0x15] = name_offset as u8;
            row.extend_from_slice(&string);
            row
        }
        KEYS => {
            let mut row = id.to_le_bytes().to_vec();
            row.extend_from_slice(&id.to_le_bytes());
            row.extend_from_slice(&string);
            row
        }
        _ => {
            let mut row = id.to_le_bytes().to_vec();
            row.extend_from_slice(&string);
            row
        }
    }
}

/// A track row: the fixed-offset numeric fields `parse_track_row` reads,
/// plus the title (string index 17) and the `.DAT` analysis path (index
/// 14) placed after the 21-entry string-offset table. Every other string
/// slot points at the title's own offset rather than staying zeroed — a
/// zero there would point at the row's first byte, which is not a string.
fn build_track_row() -> Vec<u8> {
    const STRING_TABLE_OFFSET: usize = 0x5e;
    const STRING_COUNT: usize = 21;
    const ANALYZE_PATH_INDEX: usize = 14;

    let title = short_device_sql_string("Test Track");
    let analyze_path = short_device_sql_string("ANLZ0000.DAT");
    let title_offset = STRING_TABLE_OFFSET + STRING_COUNT * 2;
    let analyze_path_offset = title_offset + title.len();
    let mut row = vec![0u8; analyze_path_offset + analyze_path.len()];

    write_u32_le(&mut row, 0x1c, 0); // artwork_id (0 = none)
    write_u32_le(&mut row, 0x20, 50); // key_id
    write_u32_le(&mut row, 0x28, 40); // label_id
    write_u32_le(&mut row, 0x38, 12_800); // tempo_centibpm: 128.00 BPM
    write_u32_le(&mut row, 0x3c, 10); // genre_id
    write_u32_le(&mut row, 0x40, 30); // album_id
    write_u32_le(&mut row, 0x44, 20); // artist_id
    write_u32_le(&mut row, 0x48, 4207); // id
    write_u16_le(&mut row, 0x54, 240); // duration_sec

    for index in 0..STRING_COUNT {
        let offset = if index == ANALYZE_PATH_INDEX {
            analyze_path_offset
        } else {
            title_offset
        };
        write_u16_le(&mut row, STRING_TABLE_OFFSET + index * 2, offset as u16);
    }

    row[title_offset..title_offset + title.len()].copy_from_slice(&title);
    row[analyze_path_offset..analyze_path_offset + analyze_path.len()]
        .copy_from_slice(&analyze_path);
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
