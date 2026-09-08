//! Adapts the NFS route (`prolink::nfs`) into the same `TrackInfo`-shaped
//! pieces [`super::normalize`] produces from dbserver — fetches
//! `export.pdb`, looks up the loaded track, fetches its `.EXT` analysis
//! file, and assembles metadata, beat grid, HD color waveform and cues (the
//! two fields the dbserver route leaves stubbed, made possible here by
//! `rekordcrate`'s clean `anlz` API — see this crate's Cargo.toml comment
//! and `prolink::nfs::pdb`'s doc comment for why `pdb`-module parsing is
//! hand-rolled instead).

use std::io::Cursor;
use std::net::Ipv4Addr;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use binrw::BinRead;
use binrw::Endian;
use decktective_core::model::{
    BeatGridBeat, CueKind, TrackCue, TrackInfo, WaveformData, WaveformSegment,
};
use decktective_core::structure::{self, StructureInput};
use decktective_core::waveform::{build_preview_from_detail, PREVIEW_SEGMENT_COUNT};
use prolink::nfs::pdb::{self, Pdb, Track};
use prolink::nfs::{parse_anlz_cues, parse_export_library, AnlzCue, NfsClient};
use prolink::status::MediaSlot;
use rekordcrate::anlz::{Content, ContentKind, Cue, CueType, ExtendedCue, Header, Section};

/// Bytes of an ANLZ section header: its kind, its own size, and the
/// section's total size.
const HEADER_LEN: usize = 12;

use super::colors;
use super::normalize::normalize_tempo;

const EXPORT_PDB_PATH: &str = "PIONEER/rekordbox/export.pdb";
const EXPORT_LIBRARY_CANDIDATE_PATHS: &[&str] = &[
    "PIONEER/rekordbox/exportLibrary.db",
    "PIONEER/DeviceLibraryPlus/exportLibrary.db",
    ".PIONEER/rekordbox/exportLibrary.db",
];
/// HD color waveform + extended cues both only exist in the `.EXT`
/// variant, not the base `.DAT` every player writes.
const EXT_SUFFIX: &str = ".EXT";

/// Fetches and parses the track database: first tries `export.pdb` (Rekordbox 6 / CDJ legacy),
/// and falls back to Rekordbox 7 / DeviceLibraryPlus `exportLibrary.db` (SQLCipher) if absent or unparseable.
pub async fn fetch_pdb(client: &mut NfsClient) -> anyhow::Result<Pdb> {
    if let Ok(pdb_bytes) = client.read_file(EXPORT_PDB_PATH).await {
        if let Some(pdb) = pdb::parse(&pdb_bytes) {
            return Ok(pdb);
        }
        tracing::warn!("read export.pdb but parsing failed; checking DeviceLibraryPlus exportLibrary.db fallback");
    }

    for &db_path in EXPORT_LIBRARY_CANDIDATE_PATHS {
        if let Ok(db_bytes) = client.read_file(db_path).await {
            if !db_bytes.is_empty() {
                match parse_export_library(&db_bytes) {
                    Ok(pdb) => {
                        tracing::info!(
                            path = db_path,
                            tracks = pdb.tracks.len(),
                            "successfully loaded DeviceLibraryPlus SQLite database"
                        );
                        return Ok(pdb);
                    }
                    Err(err) => {
                        tracing::warn!(
                            path = db_path,
                            error = %err,
                            "found exportLibrary.db but parsing failed"
                        );
                    }
                }
            }
        }
    }

    anyhow::bail!("could not fetch or parse export.pdb or exportLibrary.db over NFS")
}

/// Fetches everything about `track_id` over NFS: opens a client for `slot`,
/// downloads and parses `export.pdb` (or `exportLibrary.db`), then — if the track is actually in
/// it — downloads and parses its `.EXT` analysis file for beat grid,
/// waveform and cues, and its artwork if it has one.
///
/// `Ok(None)` means the connection and the database both worked fine, but
/// this specific track id wasn't in it (a legitimately different
/// situation from a connection/protocol failure, which is an `Err` — see
/// `metadata::fetcher`'s failure-cache handling of the two differently).
pub async fn fetch_track_info(
    player_ip: Ipv4Addr,
    bind_ip: Ipv4Addr,
    slot: MediaSlot,
    track_id: u32,
    status_bpm: Option<f64>,
    cached_pdb: Option<Arc<Pdb>>,
) -> anyhow::Result<(Option<TrackInfo>, Option<Arc<Pdb>>)> {
    let mut client = NfsClient::open(player_ip, bind_ip, slot).await?;

    let (pdb, is_new) = match cached_pdb {
        Some(pdb) if pdb.tracks.contains_key(&track_id) => (pdb, false),
        _ => (Arc::new(fetch_pdb(&mut client).await?), true),
    };

    let Some(track) = pdb.tracks.get(&track_id) else {
        return Ok((None, is_new.then_some(pdb)));
    };

    let analysis = fetch_analysis(&mut client, &track.analyze_path).await;
    let artwork = fetch_artwork(&mut client, &pdb, track).await;

    let track_info = build_track_info(&pdb, track, analysis, artwork, status_bpm);
    Ok((Some(track_info), is_new.then_some(pdb)))
}

/// A track's beat grid, waveform and cues, as read out of its `.DAT`/`.EXT`
/// pair — the three things that come from analysis files rather than from
/// the database.
pub type Analysis = (
    Option<Vec<BeatGridBeat>>,
    Option<WaveformData>,
    Vec<TrackCue>,
);

/// Assembles one [`TrackInfo`] from bytes already in hand.
///
/// Split from [`fetch_track_info`] so the same mapping serves a source that
/// does no I/O at all: `sim-fixtures` builds the simulator's tracks from an
/// `export.pdb` and the analysis files recovered out of a recorded capture,
/// and it must produce exactly what a live player would have, not a second
/// approximation of it.
pub fn build_track_info(
    pdb: &Pdb,
    track: &Track,
    analysis: Analysis,
    artwork: Option<String>,
    status_bpm: Option<f64>,
) -> TrackInfo {
    let (beat_grid, waveform, mut cues) = analysis;
    if cues.is_empty() {
        if let Some(db_cues) = pdb.cues.get(&track.id) {
            cues = db_cues.iter().map(map_anlz_cue).collect();
            cues.sort_by(|a, b| {
                a.offset_ms
                    .partial_cmp(&b.offset_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
    let duration_ms = f64::from(track.duration_sec) * 1000.0;
    let bpm = normalize_tempo(f64::from(track.tempo_centibpm)).or(status_bpm);

    let events = structure::analyze(&StructureInput {
        waveform: waveform.as_ref(),
        beat_grid: beat_grid.as_deref(),
        duration_ms,
        cues: &cues,
    });

    TrackInfo {
        track_id: track.id,
        title: track.title.clone(),
        artist: pdb.artists.get(&track.artist_id).cloned(),
        album: pdb.albums.get(&track.album_id).cloned(),
        genre: pdb.genres.get(&track.genre_id).cloned(),
        key: pdb.keys.get(&track.key_id).cloned(),
        label: pdb.labels.get(&track.label_id).cloned(),
        // This parser's minimal Track row (`prolink::nfs::pdb::Track`)
        // doesn't extract a comment string — it's one of 21 string-offset
        // slots and this phase only resolved title (index 0) and the file
        // path (index 20) with confidence. Left `None` rather than
        // guessing at another index.
        comment: None,
        duration_ms,
        bpm,
        artwork,
        beat_grid,
        waveform,
        cues,
        events,
    }
}

/// Downloads `dat_path`'s `.EXT` sibling and parses whatever sections it
/// has. Every failure (can't fetch, can't parse) degrades to "nothing
/// found" rather than failing the whole track — metadata (title/artist/
/// etc, already resolved by this point) is more valuable than analysis
/// data, and a track is still worth showing without a waveform.
async fn fetch_analysis(client: &mut NfsClient, dat_path: &str) -> Analysis {
    if dat_path.trim().is_empty() {
        return (None, None, Vec::new());
    }
    // Both files, because the two things this route wants live in
    // different ones: the HD colour waveform and the extended cues are only
    // in the `.EXT`, while the beat grid `rekordcrate` can read (`PQTZ`) is
    // only in the `.DAT` — the `.EXT`'s own `PQT2` beat grid is a format it
    // has no variant for. Reading only the `.EXT`, as this did before
    // 2026-08-19, means never getting a beat grid over NFS at all.
    let ext_bytes = client
        .read_file(&ext_variant_path(dat_path))
        .await
        .unwrap_or_default();
    let dat_bytes = client.read_file(dat_path).await.unwrap_or_default();
    parse_analysis(&ext_bytes, &dat_bytes)
}

pub fn ext_variant_path(dat_path: &str) -> String {
    for suffix in [".DAT", ".dat"] {
        if let Some(base) = dat_path.strip_suffix(suffix) {
            return format!("{base}{EXT_SUFFIX}");
        }
    }
    dat_path.to_string() // Not a recognized `.DAT` path; use as given.
}

/// Reads an `.EXT`/`.DAT` file's sections one at a time, skipping any that
/// don't parse instead of giving up on the file.
///
/// `ANLZ::read` is all-or-nothing: one section `rekordcrate` 0.3 can't
/// decode and the whole file is lost. A CDJ-3000's `.EXT` contains exactly
/// that — an extended cue whose `cue_type` byte the crate rejects
/// (`capture/reference-20260819/`) — and it sits *after* the waveform and
/// beat grid this route came for. Walking the section table by its own
/// declared sizes recovers everything before and after the section that
/// fails.
fn read_sections(bytes: &[u8]) -> Vec<Section> {
    let Ok(file_header) = Header::read(&mut Cursor::new(bytes)) else {
        return Vec::new();
    };
    if file_header.kind != ContentKind::File {
        return Vec::new();
    }

    // Sections run from the end of the file header block to the end of the
    // file, both as the header itself declares them.
    let mut position = file_header.size as usize;
    let end = (file_header.total_size as usize).min(bytes.len());

    let mut sections = Vec::new();
    while position < end {
        let Some(remaining) = bytes.get(position..end) else {
            break;
        };
        let Ok(header) = Header::read(&mut Cursor::new(remaining)) else {
            break;
        };
        // A zero/short size would leave `position` stuck and loop forever.
        let Some(section_bytes) = usize::try_from(header.total_size)
            .ok()
            .filter(|size| *size >= HEADER_LEN)
            .and_then(|size| remaining.get(..size))
        else {
            break;
        };
        if let Ok(section) = Section::read_options(&mut Cursor::new(section_bytes), Endian::Big, ())
        {
            sections.push(section);
        }
        position += section_bytes.len();
    }
    sections
}

pub fn parse_analysis(ext_bytes: &[u8], dat_bytes: &[u8]) -> Analysis {
    let mut sections = read_sections(ext_bytes);
    sections.extend(read_sections(dat_bytes));

    let mut beat_grid = None;
    let mut waveform = None;
    let mut extended_cues = None;
    let mut basic_cues = None;

    for section in &sections {
        match &section.content {
            Content::BeatGrid(grid) => {
                beat_grid = Some(
                    grid.beats
                        .iter()
                        .map(|beat| BeatGridBeat {
                            offset_ms: f64::from(beat.time),
                            beat_in_measure: beat.beat_number as u8,
                        })
                        .collect(),
                );
            }
            Content::WaveformColorDetail(detail) => {
                waveform = build_waveform(detail);
            }
            Content::ExtendedCueList(list) => {
                extended_cues = Some(list.cues.iter().map(map_extended_cue).collect::<Vec<_>>());
            }
            Content::CueList(list) => {
                basic_cues = Some(list.cues.iter().map(map_basic_cue).collect::<Vec<_>>());
            }
            _ => {}
        }
    }

    // Decode cues using prolink's hand-rolled PCO2/PCOB parser (recovering
    // CDJ-3000 / NXS2 cues where rekordcrate 0.3 rejected cue_type 1 or variable lengths).
    let mut custom_cues = parse_anlz_cues(ext_bytes);
    if custom_cues.is_empty() {
        custom_cues = parse_anlz_cues(dat_bytes);
    }

    let mut cues = if !custom_cues.is_empty() {
        custom_cues.iter().map(map_anlz_cue).collect::<Vec<_>>()
    } else {
        extended_cues.or(basic_cues).unwrap_or_default()
    };

    cues.sort_by(|a, b| a.offset_ms.total_cmp(&b.offset_ms));

    (beat_grid, waveform, cues)
}

fn build_waveform(detail: &rekordcrate::anlz::WaveformColorDetail) -> Option<WaveformData> {
    if detail.data.is_empty() {
        return None;
    }
    // `WaveformColorDetailColumn`'s own `.height()`/`.red()`/`.green()`/`.blue()`
    // accessors do NOT match the on-disk bit layout: `modular_bitfield`
    // defaults to packing the first-declared field into the *least*
    // significant bits, while the format (confirmed against Deep Symmetry's
    // spec and against `prolink::dbserver::waveform`'s independently
    // hand-verified, tested decode of the same 2-byte column read live off
    // a dbserver) actually stores red/green/blue/height from the *most*
    // significant bit down. Trusting the generated accessors silently
    // scrambles the channels — blue in particular reads far too low, which
    // is why the detail waveform looked red/orange/yellow-heavy with almost
    // no blue compared to a real CDJ. `into_bytes()` still round-trips the
    // original file bytes faithfully (it's the same bitfield, just
    // re-encoded), so recover those and unpack them by hand instead.
    let detail_segments: Vec<WaveformSegment> = detail.data.iter().map(decode_column).collect();

    let preview = build_preview_from_detail(&detail_segments, PREVIEW_SEGMENT_COUNT);
    Some(WaveformData {
        detail: Some(detail_segments),
        // 150 HD segments/sec — same constant already established for the
        // dbserver route's waveform (`prolink::dbserver::waveform`), and
        // confirmed again directly in `rekordcrate`'s own doc comments.
        ms_per_detail_segment: 1000.0 / 150.0,
        preview: Some(preview),
    })
}

/// `WaveformColorDetailColumn`'s own `.height()`/`.red()`/`.green()`/`.blue()`
/// accessors do NOT match the on-disk bit layout: `modular_bitfield`
/// defaults to packing the first-declared field into the *least* significant
/// bits, while the format (confirmed against Deep Symmetry's spec and against
/// `prolink::dbserver::waveform`'s independently hand-verified, tested decode
/// of the same 2-byte column read live off a dbserver) actually stores
/// red/green/blue/height from the *most* significant bit down. Trusting the
/// generated accessors silently scrambles the channels — blue in particular
/// reads far too low, which is why the detail waveform looked
/// red/orange/yellow-heavy with almost no blue compared to a real CDJ.
/// `into_bytes()` still round-trips the original file bytes faithfully (it's
/// the same bitfield, just re-encoded), so recover those and unpack them by
/// hand instead.
fn decode_column(column: &rekordcrate::anlz::WaveformColorDetailColumn) -> WaveformSegment {
    let bits = u16::from_be_bytes(column.into_bytes());
    let height = f32::from((bits >> 2) & 0x1f) / 31.0;
    let scale_channel = |shift: u16| (f32::from((bits >> shift) & 0x7) / 7.0 * 255.0).round() as u8;
    WaveformSegment {
        height,
        color: [scale_channel(13), scale_channel(10), scale_channel(7)],
    }
}

fn cue_kind(hot_cue: u32, cue_type: &CueType) -> CueKind {
    match (hot_cue != 0, *cue_type == CueType::Loop) {
        (true, true) => CueKind::Hotloop,
        (true, false) => CueKind::Hotcue,
        (false, true) => CueKind::Loop,
        (false, false) => CueKind::Memory,
    }
}

fn default_color(kind: CueKind) -> &'static str {
    match kind {
        CueKind::Memory => colors::DEFAULT_MEMORY_COLOR,
        CueKind::Hotcue => colors::DEFAULT_HOTCUE_COLOR,
        CueKind::Loop | CueKind::Hotloop => colors::DEFAULT_LOOP_COLOR,
    }
}

/// `1` -> `"A"`, `8` -> `"H"` — the letters real hardware labels hot-cue
/// pads with; anything outside that range (shouldn't happen for an actual
/// hot cue) falls back to the raw number rather than guessing at a letter.
fn hot_cue_button_letter(hot_cue: u32) -> String {
    match u8::try_from(hot_cue) {
        Ok(number @ 1..=8) => char::from(b'A' + number - 1).to_string(),
        _ => hot_cue.to_string(),
    }
}

fn map_anlz_cue(cue: &AnlzCue) -> TrackCue {
    let kind = match (cue.hot_cue.is_some(), cue.is_loop) {
        (true, true) => CueKind::Hotloop,
        (true, false) => CueKind::Hotcue,
        (false, true) => CueKind::Loop,
        (false, false) => CueKind::Memory,
    };

    let button = cue.hot_cue.map(|num| hot_cue_button_letter(u32::from(num)));

    let color = if let Some((r, g, b)) = cue.color_rgb {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else if let Some(code) = cue.color_code {
        if cue.hot_cue.is_some() {
            colors::hot_cue_color_hex(code)
                .unwrap_or_else(|| default_color(kind))
                .to_string()
        } else {
            match code {
                1 => Some("#ff2d92"), // Pink
                2 => Some("#ff453a"), // Red
                3 => Some("#ff9500"), // Orange
                4 => Some("#ffd60a"), // Yellow
                5 => Some("#00c853"), // Green
                6 => Some("#00d4ff"), // Aqua
                7 => Some("#0a84ff"), // Blue
                8 => Some("#9d5cff"), // Purple
                _ => None,
            }
            .unwrap_or_else(|| default_color(kind))
            .to_string()
        }
    } else {
        default_color(kind).to_string()
    };

    TrackCue {
        kind,
        offset_ms: f64::from(cue.time_ms),
        length_ms: cue
            .loop_time_ms
            .map(|loop_time| f64::from(loop_time.saturating_sub(cue.time_ms))),
        button,
        label: cue.comment.clone(),
        color,
    }
}

fn map_extended_cue(cue: &ExtendedCue) -> TrackCue {
    let kind = cue_kind(cue.hot_cue, &cue.cue_type);
    let is_loop = cue.cue_type == CueType::Loop;
    let color = colors::hot_cue_color_hex(cue.hot_cue_color_index)
        .or_else(|| colors::memory_cue_color_hex(cue.color.clone()))
        .unwrap_or_else(|| default_color(kind))
        .to_string();
    let comment = cue.comment.to_string();

    TrackCue {
        kind,
        offset_ms: f64::from(cue.time),
        length_ms: is_loop.then(|| f64::from(cue.loop_time) - f64::from(cue.time)),
        button: (cue.hot_cue != 0).then(|| hot_cue_button_letter(cue.hot_cue)),
        label: (!comment.is_empty()).then_some(comment),
        color,
    }
}

fn map_basic_cue(cue: &Cue) -> TrackCue {
    let kind = cue_kind(cue.hot_cue, &cue.cue_type);
    let is_loop = cue.cue_type == CueType::Loop;

    TrackCue {
        kind,
        offset_ms: f64::from(cue.time),
        length_ms: is_loop.then(|| f64::from(cue.loop_time) - f64::from(cue.time)),
        button: (cue.hot_cue != 0).then(|| hot_cue_button_letter(cue.hot_cue)),
        label: None, // The basic (pre-Nexus2) cue format carries no comment field at all.
        color: default_color(kind).to_string(),
    }
}

async fn fetch_artwork(client: &mut NfsClient, pdb: &Pdb, track: &Track) -> Option<String> {
    let path = pdb.artwork_paths.get(&track.artwork_id)?;
    let bytes = client.read_file(path).await.ok()?;
    if bytes.is_empty() {
        return None;
    }
    // Rekordbox artwork is conventionally JPEG; no format marker is
    // available to confirm this per-file.
    Some(format!("data:image/jpeg;base64,{}", BASE64.encode(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bit layout per Deep Symmetry's spec (MSB->LSB): red3 green3 blue3
    /// height5 pad2 — matches `prolink::dbserver::waveform`'s own test helper.
    fn column_bytes(height5: u16, red3: u16, green3: u16, blue3: u16) -> [u8; 2] {
        let bits = (height5 << 2) | (red3 << 13) | (green3 << 10) | (blue3 << 7);
        bits.to_be_bytes()
    }

    #[test]
    fn decode_column_reads_a_full_height_white_column() {
        let column =
            rekordcrate::anlz::WaveformColorDetailColumn::from_bytes(column_bytes(31, 7, 7, 7));
        let segment = decode_column(&column);
        assert_eq!(segment.height, 1.0);
        assert_eq!(segment.color, [255, 255, 255]);
    }

    #[test]
    fn decode_column_reads_a_low_blue_leaning_column_without_scrambling_channels() {
        let column =
            rekordcrate::anlz::WaveformColorDetailColumn::from_bytes(column_bytes(8, 0, 0, 7));
        let segment = decode_column(&column);
        assert!((segment.height - 8.0 / 31.0).abs() < 0.001);
        assert_eq!(segment.color, [0, 0, 255]);
    }

    #[test]
    fn a_dat_path_becomes_its_ext_sibling() {
        assert_eq!(
            ext_variant_path("PIONEER/USBANLZ/P001/00000001/ANLZ0000.DAT"),
            "PIONEER/USBANLZ/P001/00000001/ANLZ0000.EXT"
        );
    }

    #[test]
    fn a_path_with_an_unrecognized_extension_is_left_alone() {
        assert_eq!(
            ext_variant_path("PIONEER/whatever.txt"),
            "PIONEER/whatever.txt"
        );
    }

    #[test]
    fn hot_cue_numbers_one_through_eight_map_to_letters() {
        assert_eq!(hot_cue_button_letter(1), "A");
        assert_eq!(hot_cue_button_letter(8), "H");
    }

    #[test]
    fn an_out_of_range_hot_cue_number_falls_back_to_the_digit() {
        assert_eq!(hot_cue_button_letter(0), "0");
        assert_eq!(hot_cue_button_letter(99), "99");
    }

    #[test]
    fn cue_kind_combines_hot_cue_and_loop_flags() {
        assert_eq!(cue_kind(0, &CueType::Point), CueKind::Memory);
        assert_eq!(cue_kind(1, &CueType::Point), CueKind::Hotcue);
        assert_eq!(cue_kind(0, &CueType::Loop), CueKind::Loop);
        assert_eq!(cue_kind(1, &CueType::Loop), CueKind::Hotloop);
    }

    #[test]
    fn maps_anlz_hot_cue_with_rgb() {
        let anlz_cue = AnlzCue {
            hot_cue: Some(1),
            is_loop: false,
            time_ms: 12345,
            loop_time_ms: None,
            comment: Some("Drop 1".to_string()),
            color_code: Some(0x16),
            color_rgb: Some((255, 0, 128)),
        };
        let track_cue = map_anlz_cue(&anlz_cue);
        assert_eq!(track_cue.kind, CueKind::Hotcue);
        assert_eq!(track_cue.offset_ms, 12345.0);
        assert_eq!(track_cue.length_ms, None);
        assert_eq!(track_cue.button, Some("A".to_string()));
        assert_eq!(track_cue.label, Some("Drop 1".to_string()));
        assert_eq!(track_cue.color, "#ff0080");
    }

    #[test]
    fn maps_anlz_memory_loop_cue() {
        let anlz_cue = AnlzCue {
            hot_cue: None,
            is_loop: true,
            time_ms: 5000,
            loop_time_ms: Some(9000),
            comment: None,
            color_code: Some(5), // Green in memory cue palette
            color_rgb: None,
        };
        let track_cue = map_anlz_cue(&anlz_cue);
        assert_eq!(track_cue.kind, CueKind::Loop);
        assert_eq!(track_cue.offset_ms, 5000.0);
        assert_eq!(track_cue.length_ms, Some(4000.0));
        assert_eq!(track_cue.button, None);
        assert_eq!(track_cue.label, None);
        assert_eq!(track_cue.color, "#00c853");
    }

    #[test]
    fn parse_analysis_sorts_cues_chronologically() {
        let ext = concat!(
            "504d41490000000c00000070", // PMAI file header, hsize=12, total=112
            "50434f320000001400000064", // PCO2 section, hsize=20, total=100
            "00000001",                 // list_type = 1
            "0002",                     // num_cues = 2
            "0000",                     // unk = 0
            // Cue 1: offset 50000 ms, pad 2 (B)
            "504350320000001000000028", // PCP2, hsize=16, len=40
            "00000002",                 // hot_cue = 2
            "010003e8",                 // type = 1
            "0000c350",                 // time = 50000 ms
            "ffffffff",                 // loop_time = None
            "000000000000000000000000",
            // Cue 2: offset 10000 ms, pad 1 (A)
            "504350320000001000000028", // PCP2, hsize=16, len=40
            "00000001",                 // hot_cue = 1
            "010003e8",                 // type = 1
            "00002710",                 // time = 10000 ms
            "ffffffff",                 // loop_time = None
            "000000000000000000000000"
        );
        let bytes = (0..ext.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&ext[i..i + 2], 16).unwrap())
            .collect::<Vec<u8>>();
        let (_grid, _wave, cues) = parse_analysis(&bytes, &[]);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].offset_ms, 10000.0);
        assert_eq!(cues[0].button, Some("A".to_string()));
        assert_eq!(cues[1].offset_ms, 50000.0);
        assert_eq!(cues[1].button, Some("B".to_string()));
    }

    #[test]
    fn build_track_info_uses_devicelibraryplus_cues_when_analysis_cues_absent() {
        let mut pdb = Pdb::default();
        let track = Track {
            id: 42,
            title: "Test Track".to_string(),
            artist_id: 0,
            album_id: 0,
            genre_id: 0,
            key_id: 0,
            label_id: 0,
            artwork_id: 0,
            duration_sec: 180,
            tempo_centibpm: 12500,
            analyze_path: String::new(),
        };

        let db_cue1 = AnlzCue {
            hot_cue: Some(2),
            is_loop: false,
            time_ms: 30000,
            loop_time_ms: None,
            comment: Some("Drop".to_string()),
            color_code: None,
            color_rgb: Some((255, 0, 0)),
        };
        let db_cue2 = AnlzCue {
            hot_cue: Some(1),
            is_loop: false,
            time_ms: 10000,
            loop_time_ms: None,
            comment: Some("Intro".to_string()),
            color_code: None,
            color_rgb: Some((0, 255, 0)),
        };
        pdb.cues.insert(42, vec![db_cue1, db_cue2]);

        // Empty cues from analysis
        let analysis: Analysis = (None, None, Vec::new());
        let info = build_track_info(&pdb, &track, analysis, None, None);

        assert_eq!(info.cues.len(), 2);
        // Should be sorted chronologically
        assert_eq!(info.cues[0].offset_ms, 10000.0);
        assert_eq!(info.cues[0].button, Some("A".to_string()));
        assert_eq!(info.cues[0].label, Some("Intro".to_string()));
        assert_eq!(info.cues[0].color, "#00ff00");

        assert_eq!(info.cues[1].offset_ms, 30000.0);
        assert_eq!(info.cues[1].button, Some("B".to_string()));
        assert_eq!(info.cues[1].label, Some("Drop".to_string()));
        assert_eq!(info.cues[1].color, "#ff0000");
    }
}
