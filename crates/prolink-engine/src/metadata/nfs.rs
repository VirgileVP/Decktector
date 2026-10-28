//! Adapts the NFS route (`prolink::nfs`) into the same `TrackInfo`-shaped
//! pieces [`super::normalize`] produces from dbserver — fetches
//! `export.pdb`, looks up the loaded track, fetches its `.EXT` analysis
//! file, and assembles metadata, beat grid, HD color waveform, cues with
//! their colours and comments, and rekordbox's phrases — the last three
//! things the dbserver route cannot give in full (see
//! `prolink::nfs::pdb`'s doc comment for why `pdb`-module parsing is
//! hand-rolled rather than `rekordcrate`'s).

use std::io::Cursor;
use std::net::Ipv4Addr;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use binrw::BinRead;
use binrw::Endian;
use decktector_core::model::{
    BeatGridBeat, CueKind, TrackCue, TrackInfo, WaveformData, WaveformSegment,
};
use decktector_core::structure::StructureInput;
use decktector_core::waveform::{build_preview_from_detail, PREVIEW_SEGMENT_COUNT};
use prolink::nfs::pdb::{self, Pdb, Track};
use prolink::nfs::procedures::FileAttributes;
use prolink::nfs::{
    parse_anlz_cues, parse_export_library, parse_song_structure, NfsClient, SongStructure,
};
use prolink::status::MediaSlot;
use rekordcrate::anlz::{Content, ContentKind, Cue, CueType, ExtendedCue, Header, Section};

/// Bytes of an ANLZ section header: its kind, its own size, and the
/// section's total size.
const HEADER_LEN: usize = 12;

use super::colors;
use super::cues::{default_color, hot_cue_button_letter, track_cues};
use super::normalize::normalize_tempo;
use super::phrases;

const EXPORT_PDB_PATH: &str = "PIONEER/rekordbox/export.pdb";
const EXPORT_LIBRARY_CANDIDATE_PATHS: &[&str] = &[
    "PIONEER/rekordbox/exportLibrary.db",
    "PIONEER/DeviceLibraryPlus/exportLibrary.db",
    ".PIONEER/rekordbox/exportLibrary.db",
];
/// HD color waveform + extended cues both only exist in the `.EXT`
/// variant, not the base `.DAT` every player writes.
const EXT_SUFFIX: &str = ".EXT";

/// A parsed track database and the file it was read from.
///
/// Rekordbox track ids are only unique within one stick, and a player's
/// slot changes stick whenever a DJ swaps media — typically at a changeover,
/// a minute after the last track off the previous stick. Keeping the file's
/// attributes alongside the parsed rows is what lets a cached copy be
/// checked against what the slot holds now (see [`load_database`]) instead
/// of answering for the new stick with the old stick's tracks.
#[derive(Debug)]
pub struct TrackDatabase {
    pub pdb: Pdb,
    source: DatabaseFile,
}

impl TrackDatabase {
    pub fn new(pdb: Pdb, path: &'static str, attributes: FileAttributes) -> Self {
        Self {
            pdb,
            source: DatabaseFile { path, attributes },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DatabaseFile {
    path: &'static str,
    attributes: FileAttributes,
}

impl DatabaseFile {
    /// Whether `current`, the same path looked up again, is still the file
    /// that was read: same size, same file id, same modification time.
    fn is_unchanged(&self, current: &FileAttributes) -> bool {
        self.attributes == *current
    }
}

/// The track database for the mounted slot: `cached` when the file it was
/// read from is still the one on the media, otherwise a fresh read. The
/// flag says whether the database is a fresh read, for the caller to cache.
pub async fn load_database(
    client: &mut NfsClient,
    cached: Option<Arc<TrackDatabase>>,
) -> anyhow::Result<(Arc<TrackDatabase>, bool)> {
    if let Some(cached) = cached {
        let path = cached.source.path;
        match client.file_attributes(path).await {
            Ok(current) if cached.source.is_unchanged(&current) => return Ok((cached, false)),
            Ok(_) => tracing::info!(
                path,
                "track database changed on the media; reading it again"
            ),
            Err(error) => {
                tracing::debug!(path, %error, "cached track database is gone from the media")
            }
        }
    }
    Ok((Arc::new(fetch_database(client).await?), true))
}

/// Fetches and parses the track database: first tries `export.pdb` (Rekordbox 6 / CDJ legacy),
/// and falls back to Rekordbox 7 / DeviceLibraryPlus `exportLibrary.db` (SQLCipher) if absent or unparseable.
async fn fetch_database(client: &mut NfsClient) -> anyhow::Result<TrackDatabase> {
    if let Some((attributes, pdb_bytes)) = read_with_attributes(client, EXPORT_PDB_PATH).await {
        if let Some(pdb) = pdb::parse(&pdb_bytes) {
            return Ok(TrackDatabase::new(pdb, EXPORT_PDB_PATH, attributes));
        }
        tracing::warn!("read export.pdb but parsing failed; checking DeviceLibraryPlus exportLibrary.db fallback");
    }

    for &db_path in EXPORT_LIBRARY_CANDIDATE_PATHS {
        if let Some((attributes, db_bytes)) = read_with_attributes(client, db_path).await {
            if !db_bytes.is_empty() {
                match parse_export_library(&db_bytes) {
                    Ok(pdb) => {
                        tracing::info!(
                            path = db_path,
                            tracks = pdb.tracks.len(),
                            "successfully loaded DeviceLibraryPlus SQLite database"
                        );
                        return Ok(TrackDatabase::new(pdb, db_path, attributes));
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

/// The file at `path` and the attributes it had when read; `None` when the
/// media has no such file or reading it failed.
async fn read_with_attributes(
    client: &mut NfsClient,
    path: &str,
) -> Option<(FileAttributes, Vec<u8>)> {
    let attributes = client.file_attributes(path).await.ok()?;
    let bytes = client.read_file(path).await.ok()?;
    Some((attributes, bytes))
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
    cached_database: Option<Arc<TrackDatabase>>,
) -> anyhow::Result<(Option<TrackInfo>, Option<Arc<TrackDatabase>>)> {
    let mut client = NfsClient::open(player_ip, bind_ip, slot).await?;

    let (database, is_new) = load_database(&mut client, cached_database).await?;
    let pdb = &database.pdb;

    let Some(track) = pdb.tracks.get(&track_id) else {
        return Ok((None, is_new.then_some(database)));
    };

    let analysis = fetch_analysis(&mut client, &track.analyze_path).await;
    let artwork = fetch_artwork(&mut client, pdb, track).await;

    let track_info = build_track_info(pdb, track, analysis, artwork, status_bpm);
    Ok((Some(track_info), is_new.then_some(database)))
}

/// What a track's `.DAT`/`.EXT` pair holds, as opposed to its database row.
#[derive(Debug, Default)]
pub struct Analysis {
    pub beat_grid: Option<Vec<BeatGridBeat>>,
    pub waveform: Option<WaveformData>,
    pub cues: Vec<TrackCue>,
    /// Rekordbox's phrase analysis, when the track has one.
    pub song_structure: Option<SongStructure>,
}

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
    let Analysis {
        beat_grid,
        waveform,
        mut cues,
        song_structure,
    } = analysis;
    if cues.is_empty() {
        if let Some(db_cues) = pdb.cues.get(&track.id) {
            cues = track_cues(db_cues);
        }
    }
    let duration_ms = f64::from(track.duration_sec) * 1000.0;
    let bpm = normalize_tempo(f64::from(track.tempo_centibpm)).or(status_bpm);

    let events = phrases::sections(
        song_structure.as_ref(),
        &StructureInput {
            waveform: waveform.as_ref(),
            beat_grid: beat_grid.as_deref(),
            duration_ms,
            cues: &cues,
        },
    );

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
        return Analysis::default();
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
        track_cues(&custom_cues)
    } else {
        extended_cues.or(basic_cues).unwrap_or_default()
    };

    cues.sort_by(|a, b| a.offset_ms.total_cmp(&b.offset_ms));

    Analysis {
        beat_grid,
        waveform,
        cues,
        // Phrase analysis is written to the `.EXT` only.
        song_structure: parse_song_structure(ext_bytes),
    }
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
    use prolink::nfs::AnlzCue;

    fn database_file(size: u32, modified_seconds: u32) -> DatabaseFile {
        DatabaseFile {
            path: EXPORT_PDB_PATH,
            attributes: FileAttributes {
                file_type: prolink::nfs::procedures::FTYPE_REGULAR_FILE,
                size,
                file_id: 9,
                modified_seconds,
            },
        }
    }

    #[test]
    fn the_same_database_file_is_reused() {
        let read = database_file(4096 * 300, 1_760_000_000);
        assert!(read.is_unchanged(&read.attributes));
    }

    /// The changeover regression: the next DJ's stick goes into the same
    /// slot within the cache's lifetime, its `export.pdb` the very same
    /// page-rounded size. Its tracks must not be answered from the previous
    /// stick's rows.
    #[test]
    fn a_swapped_stick_with_a_same_size_database_is_not_reused() {
        let previous_stick = database_file(4096 * 300, 1_760_000_000);
        let next_stick = database_file(4096 * 300, 1_760_086_400);
        assert!(!previous_stick.is_unchanged(&next_stick.attributes));
    }

    #[test]
    fn a_database_grown_by_a_new_export_is_not_reused() {
        let read = database_file(4096 * 300, 1_760_000_000);
        let reexported = database_file(4096 * 301, 1_760_000_000);
        assert!(!read.is_unchanged(&reexported.attributes));
    }

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
    fn cue_kind_combines_hot_cue_and_loop_flags() {
        assert_eq!(cue_kind(0, &CueType::Point), CueKind::Memory);
        assert_eq!(cue_kind(1, &CueType::Point), CueKind::Hotcue);
        assert_eq!(cue_kind(0, &CueType::Loop), CueKind::Loop);
        assert_eq!(cue_kind(1, &CueType::Loop), CueKind::Hotloop);
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
        let cues = parse_analysis(&bytes, &[]).cues;
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
            file_path: None,
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
        let analysis = Analysis::default();
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

    fn track_of(duration_sec: u32) -> Track {
        Track {
            id: 7,
            title: "Phrased".to_string(),
            artist_id: 0,
            album_id: 0,
            genre_id: 0,
            key_id: 0,
            label_id: 0,
            artwork_id: 0,
            duration_sec,
            tempo_centibpm: 12000,
            analyze_path: String::new(),
            file_path: None,
        }
    }

    /// 120 BPM from 0 ms.
    fn beat_grid_of(beats: usize) -> Vec<BeatGridBeat> {
        (0..beats)
            .map(|index| BeatGridBeat {
                offset_ms: index as f64 * 500.0,
                beat_in_measure: (index % 4) as u8 + 1,
            })
            .collect()
    }

    /// The track structure used to be inferred from the waveform even when
    /// rekordbox had analysed the track's phrases itself.
    #[test]
    fn a_track_with_rekordbox_phrases_shows_them_instead_of_an_inferred_structure() {
        let song_structure = SongStructure {
            mood: prolink::nfs::PhraseMood::Mid,
            end_beat: 65,
            phrases: vec![
                prolink::nfs::Phrase {
                    start_beat: 1,
                    kind: prolink::nfs::PhraseKind::Intro,
                    variant: None,
                },
                prolink::nfs::Phrase {
                    start_beat: 33,
                    kind: prolink::nfs::PhraseKind::Chorus,
                    variant: None,
                },
            ],
        };
        let analysis = Analysis {
            beat_grid: Some(beat_grid_of(80)),
            song_structure: Some(song_structure),
            ..Analysis::default()
        };

        let info = build_track_info(&Pdb::default(), &track_of(40), analysis, None, None);

        let labels: Vec<_> = info
            .events
            .iter()
            .map(|event| (event.label.as_deref(), event.start_ms, event.end_ms))
            .collect();
        assert_eq!(
            labels,
            vec![
                (Some("Intro"), 0.0, 16_000.0),
                (Some("Chorus"), 16_000.0, 32_000.0)
            ]
        );
    }

    /// An unmasked `PSSI` section in the mid mood: `(first beat, kind)`
    /// per phrase.
    fn pssi_section(end_beat: u16, phrases: &[(u16, u16)]) -> Vec<u8> {
        let mut section = vec![0u8; 0x20];
        section[..4].copy_from_slice(b"PSSI");
        section[4..8].copy_from_slice(&0x20u32.to_be_bytes());
        let total = (0x20 + 24 * phrases.len()) as u32;
        section[8..12].copy_from_slice(&total.to_be_bytes());
        section[0x0c..0x10].copy_from_slice(&24u32.to_be_bytes());
        section[0x10..0x12].copy_from_slice(&(phrases.len() as u16).to_be_bytes());
        section[0x12..0x14].copy_from_slice(&2u16.to_be_bytes());
        section[0x1a..0x1c].copy_from_slice(&end_beat.to_be_bytes());
        for (index, (beat, kind)) in phrases.iter().enumerate() {
            let mut entry = vec![0u8; 24];
            entry[0..2].copy_from_slice(&(index as u16 + 1).to_be_bytes());
            entry[2..4].copy_from_slice(&beat.to_be_bytes());
            entry[4..6].copy_from_slice(&kind.to_be_bytes());
            section.extend(entry);
        }
        section
    }

    #[test]
    fn the_song_structure_is_read_from_the_ext() {
        let section = pssi_section(65, &[(1, 1), (33, 9)]);
        let mut bytes = Vec::from(*b"PMAI");
        bytes.extend(12u32.to_be_bytes());
        bytes.extend(((12 + section.len()) as u32).to_be_bytes());
        bytes.extend(section);

        let analysis = parse_analysis(&bytes, &[]);

        let phrases = analysis.song_structure.expect("a song structure").phrases;
        assert_eq!(phrases.len(), 2);
        assert_eq!(phrases[1].label(), "Chorus");
    }
}
