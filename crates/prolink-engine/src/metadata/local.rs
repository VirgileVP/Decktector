//! Loads a rekordbox USB export that has been copied onto local disk.
//!
//! The live route (`metadata::nfs`) reads `export.pdb` and a track's analysis
//! files off a player's USB slot over NFS. A stick copied to a laptop holds
//! the very same tree — `PIONEER/rekordbox/export.pdb`,
//! `PIONEER/USBANLZ/...`, artwork, the audio files — so this module is the
//! filesystem twin of that route: it finds the database with the same
//! candidate paths in the same order, parses it with the same parsers, and
//! assembles each [`TrackInfo`] through the same
//! [`build_track_info`] mapping. A second, parallel implementation would
//! only test itself.
//!
//! It exists so the simulator (and `sim-fixtures`) can play a real DJ's
//! library: real titles, beat grids, cues and waveforms, instead of the
//! generated ones.

use std::path::{Path, PathBuf};

use decktector_core::model::TrackInfo;
use prolink::nfs::devicelibraryplus::parse_export_library_file;
use prolink::nfs::pdb::{self, Pdb, Track};

use super::nfs::{
    artwork_data_url, build_track_info, ext_variant_path, parse_analysis, Analysis,
    EXPORT_LIBRARY_CANDIDATE_PATHS, EXPORT_PDB_PATH,
};

/// A track of a local export: what the player would show for it, and where
/// its audio file sits on this disk.
#[derive(Debug, Clone)]
pub struct LocalTrack {
    pub info: TrackInfo,
    /// The audio file under the export root. `None` when the database names
    /// none or the file was not copied along — a USB stick's `Contents/`
    /// folder is easily left behind when only `PIONEER/` is copied.
    pub audio_path: Option<PathBuf>,
}

/// Loads every usable track of the export under `root`, sorted by title
/// (case-insensitive) then artist.
///
/// `root` is the folder that contains the export's `PIONEER` tree. A track
/// is kept only when its analysis files are present and [`is_usable`] — a
/// track without them would carry none of the data this loader exists for.
/// Artwork is attached when its file is on disk, and so is the audio file
/// (see [`LocalTrack::audio_path`]).
pub fn load_export(root: &Path) -> anyhow::Result<Vec<LocalTrack>> {
    let pdb = read_database(root)?;

    let mut tracks: Vec<LocalTrack> = pdb
        .tracks
        .values()
        .filter_map(|track| load_track(root, &pdb, track))
        .collect();
    tracks.sort_by_cached_key(|track| {
        (
            track.info.title.to_lowercase(),
            track
                .info
                .artist
                .as_deref()
                .unwrap_or_default()
                .to_lowercase(),
        )
    });

    tracing::info!(
        root = %root.display(),
        in_database = pdb.tracks.len(),
        loaded = tracks.len(),
        "loaded local rekordbox export"
    );
    Ok(tracks)
}

/// Reads and parses the export's track database: `export.pdb` first
/// (Rekordbox 6 / legacy players), then the Rekordbox 7 `exportLibrary.db`
/// candidates, in the order the NFS route's `fetch_database` tries them.
///
/// A candidate that is missing is skipped quietly; one that is present but
/// does not parse is logged and the next is tried. No database at all is an
/// error naming `root`.
pub fn read_database(root: &Path) -> anyhow::Result<Pdb> {
    let pdb_path = root.join(EXPORT_PDB_PATH);
    match std::fs::read(&pdb_path) {
        Ok(bytes) => match pdb::parse(&bytes) {
            Some(pdb) => {
                tracing::info!(
                    path = %pdb_path.display(),
                    tracks = pdb.tracks.len(),
                    "read export.pdb"
                );
                return Ok(pdb);
            }
            None => tracing::warn!(
                path = %pdb_path.display(),
                "export.pdb did not parse; trying the exportLibrary.db candidates"
            ),
        },
        Err(error) => tracing::debug!(
            path = %pdb_path.display(),
            %error,
            "export.pdb not readable; trying the exportLibrary.db candidates"
        ),
    }

    for candidate in EXPORT_LIBRARY_CANDIDATE_PATHS {
        let path = root.join(candidate);
        if !path.is_file() {
            continue;
        }
        match parse_export_library_file(&path) {
            Ok(pdb) => {
                tracing::info!(
                    path = %path.display(),
                    tracks = pdb.tracks.len(),
                    "read exportLibrary.db"
                );
                return Ok(pdb);
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "exportLibrary.db did not parse");
            }
        }
    }

    anyhow::bail!(
        "no rekordbox database (export.pdb or exportLibrary.db) found under {}",
        root.display()
    )
}

/// One track, `None` when its analysis files are missing or it is not
/// [`is_usable`].
fn load_track(root: &Path, pdb: &Pdb, track: &Track) -> Option<LocalTrack> {
    let (dat_bytes, ext_bytes) = read_analysis(root, track)?;
    let analysis = parse_analysis(&ext_bytes, &dat_bytes);
    if !is_usable(&analysis, track) {
        return None;
    }
    let info = build_track_info(pdb, track, analysis, read_artwork(root, pdb, track), None);
    Some(LocalTrack {
        info,
        audio_path: existing_audio_path(root, track),
    })
}

/// The track's audio file under `root`, `None` when the database names none
/// or the file is not on disk.
fn existing_audio_path(root: &Path, track: &Track) -> Option<PathBuf> {
    let path = local_path(root, track.file_path.as_deref()?);
    if path.is_file() {
        Some(path)
    } else {
        tracing::debug!(
            track_id = track.id,
            path = %path.display(),
            "audio file of a local export track is missing"
        );
        None
    }
}

/// The track's artwork as a `data:` URL, `None` when it has none, the file
/// is missing, or the file is empty — a track without a cover is still a
/// track.
fn read_artwork(root: &Path, pdb: &Pdb, track: &Track) -> Option<String> {
    let device_path = pdb.artwork_paths.get(&track.artwork_id)?;
    let bytes = std::fs::read(local_path(root, device_path)).ok()?;
    artwork_data_url(&bytes)
}

/// Whether a track can drive a simulated deck convincingly. A capture
/// contains more than music — jingles and one-shot samples turn up with no
/// tempo and no beat grid, and a deck loading one would play seven seconds
/// of it and report no beat at all.
pub fn is_usable(analysis: &Analysis, track: &Track) -> bool {
    analysis.waveform.is_some()
        && analysis
            .beat_grid
            .as_ref()
            .is_some_and(|beats| !beats.is_empty())
        && track.tempo_centibpm > 0
}

/// Reads a track's `.DAT` and `.EXT` out of the tree under `root`, `None` if
/// the `.DAT` is missing. An absent `.EXT` reads as empty, exactly as the
/// live route treats a failed fetch.
pub fn read_analysis(root: &Path, track: &Track) -> Option<(Vec<u8>, Vec<u8>)> {
    let dat_path = local_path(root, &track.analyze_path);
    let ext_path = local_path(root, &ext_variant_path(&track.analyze_path));
    let dat_bytes = std::fs::read(dat_path).ok()?;
    Some((dat_bytes, std::fs::read(ext_path).unwrap_or_default()))
}

/// Maps a path as the player expressed it onto the tree under `root`, which
/// mirrors those same paths.
pub fn local_path(root: &Path, device_path: &str) -> PathBuf {
    device_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .fold(root.to_path_buf(), |path, segment| path.join(segment))
}

#[cfg(test)]
mod tests {
    use super::*;

    use prolink::test_support::build_minimal_pdb_bytes;

    /// Title, artist and tempo of the single track `build_minimal_pdb_bytes`
    /// writes.
    const FIXTURE_TITLE: &str = "Test Track";
    const FIXTURE_ARTIST: &str = "Test Artist";
    const FIXTURE_TEMPO_CENTIBPM: u32 = 12_800;

    fn export_with_pdb() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("temp dir");
        let pdb_path = root.path().join(EXPORT_PDB_PATH);
        std::fs::create_dir_all(pdb_path.parent().unwrap()).unwrap();
        std::fs::write(pdb_path, build_minimal_pdb_bytes()).unwrap();
        root
    }

    #[test]
    fn read_database_parses_the_export_pdb_under_the_root() {
        let root = export_with_pdb();

        let pdb = read_database(root.path()).expect("database should be found");

        assert_eq!(pdb.tracks.len(), 1);
        let track = pdb.tracks.values().next().unwrap();
        assert_eq!(track.title, FIXTURE_TITLE);
        assert_eq!(track.tempo_centibpm, FIXTURE_TEMPO_CENTIBPM);
        assert_eq!(
            pdb.artists.values().next().map(String::as_str),
            Some(FIXTURE_ARTIST)
        );
    }

    #[test]
    fn read_database_names_the_root_when_there_is_no_database() {
        let root = tempfile::tempdir().unwrap();

        let error = read_database(root.path()).unwrap_err().to_string();

        assert!(
            error.contains(&root.path().display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn read_database_rejects_an_export_pdb_that_does_not_parse() {
        let root = tempfile::tempdir().unwrap();
        let pdb_path = root.path().join(EXPORT_PDB_PATH);
        std::fs::create_dir_all(pdb_path.parent().unwrap()).unwrap();
        std::fs::write(pdb_path, b"not a database").unwrap();

        assert!(read_database(root.path()).is_err());
    }

    #[test]
    fn load_export_fails_without_a_database() {
        let root = tempfile::tempdir().unwrap();

        assert!(load_export(root.path()).is_err());
    }

    #[test]
    fn load_export_skips_tracks_without_analysis_files() {
        let root = export_with_pdb();

        let tracks = load_export(root.path()).expect("database is present");

        assert!(tracks.is_empty());
    }

    #[test]
    fn the_audio_path_is_kept_only_when_the_file_exists() {
        let root = export_with_pdb();
        let pdb = read_database(root.path()).unwrap();
        let mut track = pdb.tracks.values().next().unwrap().clone();
        let device_path = "/Contents/Test Artist/Test Track.mp3";
        track.file_path = Some(device_path.to_string());
        assert_eq!(existing_audio_path(root.path(), &track), None);

        let audio_path = local_path(root.path(), device_path);
        std::fs::create_dir_all(audio_path.parent().unwrap()).unwrap();
        std::fs::write(&audio_path, b"audio").unwrap();
        assert_eq!(existing_audio_path(root.path(), &track), Some(audio_path));

        track.file_path = None;
        assert_eq!(existing_audio_path(root.path(), &track), None);
    }

    #[test]
    fn local_path_maps_a_device_path_under_the_root() {
        let root = Path::new("export-root");

        assert_eq!(
            local_path(root, "/PIONEER/USBANLZ/P000/0001/ANLZ0000.DAT"),
            root.join("PIONEER")
                .join("USBANLZ")
                .join("P000")
                .join("0001")
                .join("ANLZ0000.DAT")
        );
    }

    #[test]
    fn read_analysis_is_none_without_a_dat_file_and_tolerates_a_missing_ext() {
        let root = export_with_pdb();
        let pdb = read_database(root.path()).unwrap();
        let track = pdb.tracks.values().next().unwrap();
        assert!(read_analysis(root.path(), track).is_none());

        let dat_path = local_path(root.path(), &track.analyze_path);
        std::fs::create_dir_all(dat_path.parent().unwrap()).unwrap();
        std::fs::write(dat_path, b"dat").unwrap();

        let (dat, ext) = read_analysis(root.path(), track).expect(".DAT is present");
        assert_eq!(dat, b"dat");
        assert!(ext.is_empty());
    }

    #[test]
    fn artwork_data_url_is_none_for_an_empty_file() {
        assert_eq!(artwork_data_url(&[]), None);
    }

    #[test]
    fn artwork_data_url_embeds_the_bytes_as_base64_jpeg() {
        assert_eq!(
            artwork_data_url(&[0xff, 0xd8]).as_deref(),
            Some("data:image/jpeg;base64,/9g=")
        );
    }
}
