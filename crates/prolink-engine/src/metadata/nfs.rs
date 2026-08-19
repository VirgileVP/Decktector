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

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use binrw::BinRead;
use decktective_core::model::{BeatGridBeat, CueKind, TrackCue, TrackInfo, WaveformData, WaveformSegment};
use decktective_core::structure::{self, StructureInput};
use decktective_core::waveform::{build_preview_from_detail, PREVIEW_SEGMENT_COUNT};
use prolink::nfs::pdb::{self, Pdb, Track};
use prolink::nfs::NfsClient;
use prolink::status::MediaSlot;
use binrw::Endian;
use rekordcrate::anlz::{Content, ContentKind, Cue, CueType, ExtendedCue, Header, Section};

/// Bytes of an ANLZ section header: its kind, its own size, and the
/// section's total size.
const HEADER_LEN: usize = 12;

use super::colors;
use super::normalize::normalize_tempo;

const EXPORT_PDB_PATH: &str = "PIONEER/rekordbox/export.pdb";
/// HD color waveform + extended cues both only exist in the `.EXT`
/// variant, not the base `.DAT` every player writes.
const EXT_SUFFIX: &str = ".EXT";

/// Fetches everything about `track_id` over NFS: opens a client for `slot`,
/// downloads and parses `export.pdb`, then — if the track is actually in
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
) -> anyhow::Result<Option<TrackInfo>> {
    let mut client = NfsClient::open(player_ip, bind_ip, slot).await?;

    let pdb_bytes = client.read_file(EXPORT_PDB_PATH).await?;
    let pdb = pdb::parse(&pdb_bytes).ok_or_else(|| anyhow::anyhow!("could not parse export.pdb"))?;

    let Some(track) = pdb.tracks.get(&track_id) else {
        return Ok(None);
    };

    let (beat_grid, waveform, cues) = fetch_analysis(&mut client, &track.analyze_path).await;
    let artwork = fetch_artwork(&mut client, &pdb, track).await;

    let duration_ms = f64::from(track.duration_sec) * 1000.0;
    let bpm = normalize_tempo(f64::from(track.tempo_centibpm)).or(status_bpm);

    let events = structure::analyze(&StructureInput {
        waveform: waveform.as_ref(),
        beat_grid: beat_grid.as_deref(),
        duration_ms,
        cues: &cues,
    });

    Ok(Some(TrackInfo {
        track_id,
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
    }))
}

/// Downloads `dat_path`'s `.EXT` sibling and parses whatever sections it
/// has. Every failure (can't fetch, can't parse) degrades to "nothing
/// found" rather than failing the whole track — metadata (title/artist/
/// etc, already resolved by this point) is more valuable than analysis
/// data, and a track is still worth showing without a waveform.
async fn fetch_analysis(client: &mut NfsClient, dat_path: &str) -> (Option<Vec<BeatGridBeat>>, Option<WaveformData>, Vec<TrackCue>) {
    // Both files, because the two things this route wants live in
    // different ones: the HD colour waveform and the extended cues are only
    // in the `.EXT`, while the beat grid `rekordcrate` can read (`PQTZ`) is
    // only in the `.DAT` — the `.EXT`'s own `PQT2` beat grid is a format it
    // has no variant for. Reading only the `.EXT`, as this did before
    // 2026-08-19, means never getting a beat grid over NFS at all.
    let ext_bytes = client.read_file(&ext_variant_path(dat_path)).await.unwrap_or_default();
    let dat_bytes = client.read_file(dat_path).await.unwrap_or_default();
    parse_analysis(&ext_bytes, &dat_bytes)
}

fn ext_variant_path(dat_path: &str) -> String {
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
        let Some(remaining) = bytes.get(position..end) else { break };
        let Ok(header) = Header::read(&mut Cursor::new(remaining)) else { break };
        // A zero/short size would leave `position` stuck and loop forever.
        let Some(section_bytes) = usize::try_from(header.total_size).ok().filter(|size| *size >= HEADER_LEN).and_then(|size| remaining.get(..size)) else {
            break;
        };
        if let Ok(section) = Section::read_options(&mut Cursor::new(section_bytes), Endian::Big, ()) {
            sections.push(section);
        }
        position += section_bytes.len();
    }
    sections
}

fn parse_analysis(ext_bytes: &[u8], dat_bytes: &[u8]) -> (Option<Vec<BeatGridBeat>>, Option<WaveformData>, Vec<TrackCue>) {
    let mut sections = read_sections(ext_bytes);
    sections.extend(read_sections(dat_bytes));

    let mut beat_grid = None;
    let mut color_detail = None;
    let mut mono_detail = None;
    let mut extended_cues = None;
    let mut basic_cues = None;

    for section in &sections {
        match &section.content {
            Content::BeatGrid(grid) => {
                beat_grid = Some(
                    grid.beats
                        .iter()
                        .map(|beat| BeatGridBeat { offset_ms: f64::from(beat.time), beat_in_measure: beat.beat_number as u8 })
                        .collect(),
                );
            }
            Content::WaveformColorDetail(detail) => {
                color_detail = build_color_detail(detail);
            }
            Content::WaveformDetail(detail) => {
                mono_detail = build_mono_detail(detail);
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

    let cues = extended_cues.or(basic_cues).unwrap_or_default();
    (beat_grid, build_waveform(color_detail, mono_detail), cues)
}

/// 150 HD segments/sec — same constant already established for the
/// dbserver route's waveform (`prolink::dbserver::waveform`), and confirmed
/// again directly in `rekordcrate`'s own doc comments. Shared by the color
/// and mono detail waveforms: both are the same "detail" resolution, only
/// the column encoding differs.
const MS_PER_DETAIL_SEGMENT: f64 = 1000.0 / 150.0;

/// Combines whichever detail waveform(s) parsed into one `WaveformData`.
/// The minimap preview always prefers the color detail, falling back to
/// mono so it still has something to show when only the classic waveform
/// analyzed.
fn build_waveform(color_detail: Option<Vec<WaveformSegment>>, mono_detail: Option<Vec<WaveformSegment>>) -> Option<WaveformData> {
    let preview_source = color_detail.as_deref().or(mono_detail.as_deref())?;
    let preview = build_preview_from_detail(preview_source, PREVIEW_SEGMENT_COUNT);
    Some(WaveformData {
        detail: color_detail,
        mono_detail,
        ms_per_detail_segment: MS_PER_DETAIL_SEGMENT,
        preview: Some(preview),
    })
}

fn build_color_detail(detail: &rekordcrate::anlz::WaveformColorDetail) -> Option<Vec<WaveformSegment>> {
    if detail.data.is_empty() {
        return None;
    }
    // `.height()`/`.red()`/`.green()`/`.blue()` are `modular_bitfield`
    // -generated accessors on `rekordcrate`'s `WaveformColorDetailColumn` —
    // deliberately not hand-unpacking these bits ourselves (unlike
    // `prolink::dbserver::waveform`, which has to, since it reads the same
    // physical format off a raw dbserver blob with no such accessor
    // available): trusting the dependency's own generated bit-extraction
    // is strictly safer than re-deriving the bit order by hand a second
    // time.
    let scale_3bit = |value: u8| (f32::from(value) / 7.0 * 255.0).round() as u8;
    Some(
        detail
            .data
            .iter()
            .map(|column| WaveformSegment {
                height: f32::from(column.height()) / 31.0,
                color: [scale_3bit(column.red()), scale_3bit(column.green()), scale_3bit(column.blue())],
            })
            .collect(),
    )
}

/// Same accessor-trusting approach as [`build_color_detail`], for the
/// classic pre-color "Blue" waveform (`WaveformDetail`'s columns share
/// their `WaveformPreviewColumn` type with the whole-track preview).
fn build_mono_detail(detail: &rekordcrate::anlz::WaveformDetail) -> Option<Vec<WaveformSegment>> {
    if detail.data.is_empty() {
        return None;
    }
    let scale_3bit = |value: u8| (f32::from(value) / 7.0 * 255.0).round() as u8;
    Some(
        detail
            .data
            .iter()
            .map(|column| {
                let gray = scale_3bit(column.whiteness());
                WaveformSegment { height: f32::from(column.height()) / 31.0, color: [gray, gray, gray] }
            })
            .collect(),
    )
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

    #[test]
    fn a_dat_path_becomes_its_ext_sibling() {
        assert_eq!(ext_variant_path("PIONEER/USBANLZ/P001/00000001/ANLZ0000.DAT"), "PIONEER/USBANLZ/P001/00000001/ANLZ0000.EXT");
    }

    #[test]
    fn a_path_with_an_unrecognized_extension_is_left_alone() {
        assert_eq!(ext_variant_path("PIONEER/whatever.txt"), "PIONEER/whatever.txt");
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
}
