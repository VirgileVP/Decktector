//! Turns a recorded capture session into simulator track fixtures: real
//! titles, real beat grids, real cues, real waveforms.
//!
//! `decktector_core::sim` generates its tracks — a shaped energy envelope,
//! a perfect beat grid, cues placed by rule. That is enough to develop a
//! layout against and not enough to trust one: a generated waveform has no
//! silence where a real track breaks down, no beat grid that drifts, and no
//! DJ's cue two bars before the drop. Feeding the simulator what a player
//! actually served closes that gap.
//!
//! Input is a session directory (`prolink-capture`, or `prolink-pcap`
//! importing a Wireshark recording), which holds an `export.pdb` and the
//! `.DAT`/`.EXT` analysis files recovered from it. Every track whose
//! analysis files are present is written out as a `TrackInfo`, through the
//! very same mapping the live NFS route uses
//! (`metadata::nfs::build_track_info`) — a fixture that went through a
//! second, parallel implementation would be testing that implementation
//! rather than the app.
//!
//! The output holds a real DJ's library metadata, so it belongs in
//! gitignored territory exactly like the capture it came from.
//!
//! Usage:
//!   sim-fixtures --session DIR [--out FILE.json] [--max-tracks N]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use decktector_core::model::TrackInfo;
use prolink::nfs::pdb::{self, Pdb};
use prolink_engine::metadata::local::{is_usable, read_analysis};
use prolink_engine::metadata::nfs::{build_track_info, parse_analysis};

/// A track's HD waveform is 150 columns per second — around 40,000 of them
/// for a six-minute track, and roughly 2 MB of JSON each. Keeping every
/// track a festival USB carries would run to hundreds of megabytes, nearly
/// all of it columns for tracks the simulator will never reach. Eight is
/// already more than the six decks a Pro DJ Link network tops out at.
const DEFAULT_MAX_TRACKS: usize = 8;

const EXPORT_PDB_FILE: &str = "export.pdb";
const DEFAULT_OUT_FILE: &str = "sim-tracks.json";

struct Config {
    session: PathBuf,
    out: PathBuf,
    max_tracks: usize,
}

fn main() -> ExitCode {
    let config = match parse_args() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}\n");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    match build(&config) {
        Ok(0) => {
            eprintln!(
                "no track in {} had its analysis files in the capture",
                config.session.display()
            );
            ExitCode::FAILURE
        }
        Ok(count) => {
            println!("Wrote {count} tracks to {}", config.out.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to build fixtures: {error}");
            ExitCode::FAILURE
        }
    }
}

fn build(config: &Config) -> anyhow::Result<usize> {
    let nfs_dir = config.session.join("nfs");
    let databases = find_files(&nfs_dir, |path| {
        path.file_name().and_then(|name| name.to_str()) == Some(EXPORT_PDB_FILE)
    });
    if databases.is_empty() {
        anyhow::bail!("no {EXPORT_PDB_FILE} found under {}", nfs_dir.display());
    }

    // Keyed by title so two players holding the same track — routine on a
    // linked booth, where a capture sees both databases — yield one fixture.
    let mut tracks: BTreeMap<String, TrackInfo> = BTreeMap::new();
    for database in &databases {
        // The analysis files sit under the same player directory as the
        // database that references them, by the paths the player used.
        let Some(player_root) = database
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
        else {
            continue;
        };
        let bytes = std::fs::read(database)?;
        let Some(parsed) = pdb::parse(&bytes) else {
            eprintln!("{}: did not parse; skipping", database.display());
            continue;
        };
        collect_tracks(&parsed, player_root, &mut tracks);
        println!(
            "{}: {} tracks in the database",
            database.display(),
            parsed.tracks.len()
        );
    }

    let selected: Vec<&TrackInfo> = tracks.values().take(config.max_tracks).collect();
    // Compact rather than pretty: this file is read by a program, and
    // indentation on tens of thousands of waveform columns per track more
    // than doubles it for nothing.
    let json = serde_json::to_vec(&selected)?;
    if let Some(parent) = config
        .out
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config.out, json)?;
    Ok(selected.len())
}

/// Adds every track whose analysis files the capture recovered. A track
/// without them is skipped rather than written waveform-less: a fixture
/// exists precisely to carry the analysis data the generator cannot invent.
fn collect_tracks(pdb: &Pdb, player_root: &Path, tracks: &mut BTreeMap<String, TrackInfo>) {
    for track in pdb.tracks.values() {
        let Some((dat_bytes, ext_bytes)) = read_analysis(player_root, track) else {
            continue;
        };
        let analysis = parse_analysis(&ext_bytes, &dat_bytes);
        if !is_usable(&analysis, track) {
            continue;
        }
        // Artwork is left out: a player only serves it when asked, so a
        // capture rarely holds it, and the simulator has never shown it.
        let info = build_track_info(pdb, track, analysis, None, None);
        tracks.insert(
            format!("{} - {}", info.artist.as_deref().unwrap_or(""), info.title),
            info,
        );
    }
}

fn find_files(dir: &Path, predicate: impl Fn(&Path) -> bool + Copy) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for path in entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
    {
        if path.is_dir() {
            found.extend(find_files(&path, predicate));
        } else if predicate(&path) {
            found.push(path);
        }
    }
    found.sort();
    found
}

fn parse_args() -> Result<Config, String> {
    let mut session: Option<PathBuf> = None;
    let mut out = PathBuf::from(DEFAULT_OUT_FILE);
    let mut max_tracks = DEFAULT_MAX_TRACKS;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => session = Some(PathBuf::from(next_value(&mut args, "--session")?)),
            "--out" => out = PathBuf::from(next_value(&mut args, "--out")?),
            "--max-tracks" => {
                max_tracks = next_value(&mut args, "--max-tracks")?
                    .parse()
                    .map_err(|_| "--max-tracks needs a number".to_string())?
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Config {
        session: session.ok_or_else(|| "--session DIR is required".to_string())?,
        out,
        max_tracks,
    })
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn print_usage() {
    eprintln!("sim-fixtures — builds simulator track fixtures from a recorded capture session");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --session DIR    capture session to read (required)");
    eprintln!("  --out FILE.json  where to write the fixtures (default {DEFAULT_OUT_FILE})");
    eprintln!("  --max-tracks N   how many tracks to keep (default {DEFAULT_MAX_TRACKS})");
}
