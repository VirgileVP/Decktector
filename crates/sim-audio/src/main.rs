//! Turns audio files into simulator track fixtures: real waveforms, real
//! durations, a detected tempo and grid, and whatever the tags say.
//!
//! `decktective_core::sim` generates its tracks — a shaped energy envelope,
//! a perfect beat grid, cues placed by rule. That is enough to develop a
//! layout against and not enough to trust one: a generated waveform has no
//! silence where a real track breaks down and no drop where the energy
//! actually lands. `sim-fixtures` closes that gap from a hardware capture,
//! which needs the hardware; this closes it from a folder of audio files,
//! which needs nothing.
//!
//! Output is the same `Vec<TrackInfo>` JSON `sim-fixtures` writes, read
//! back by [`decktective_core::sim::SimLibrary`] through the
//! `DECKTECTIVE_SIM_TRACKS` environment variable.
//!
//! What comes from the file and what is inferred:
//!   - waveform, duration, artwork, tags — read from the file itself.
//!   - tempo, beat grid, downbeat — detected, and regular where a real
//!     grid drifts.
//!   - cues — derived from the track's own section analysis, since no cue
//!     a DJ set is stored in an audio file.
//!
//! An audio file is someone's copyrighted work, so the folder it is read
//! from and the fixture it produces both belong in gitignored territory,
//! exactly like a capture.
//!
//! Usage:
//!   sim-audio [--input DIR|FILE] [--out FILE.json] [--max-tracks N]

mod bands;
mod columns;
mod cues;
mod decode;
mod tags;
mod tempo;
mod track;
mod waveform;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use decktective_core::model::TrackInfo;

/// Where audio files are dropped by default — gitignored, like every other
/// input that carries someone else's material.
const DEFAULT_INPUT_DIR: &str = "capture/audio";
/// Written where `sim-fixtures` writes, so either builder feeds the same
/// `DECKTECTIVE_SIM_TRACKS` path.
const DEFAULT_OUT_FILE: &str = "crates/decktective-core/fixtures/sim-tracks.json";
/// A track's HD waveform is 150 columns per second — roughly 2 MB of JSON
/// for a six-minute track. Eight is already more than the six decks a Pro
/// DJ Link network tops out at.
const DEFAULT_MAX_TRACKS: usize = 8;
/// Extensions probed. Symphonia reads more than these, but a fixture
/// folder holding a stray `.txt` should not be reported as a decode
/// failure.
const AUDIO_EXTENSIONS: [&str; 6] = ["mp3", "flac", "wav", "m4a", "aac", "ogg"];

struct Config {
    input: PathBuf,
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
            eprintln!("no audio file found under {}", config.input.display());
            ExitCode::FAILURE
        }
        Ok(count) => {
            println!("Wrote {count} tracks to {}", config.out.display());
            println!("{}", how_to_run(&config.out));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to build fixtures: {error}");
            ExitCode::FAILURE
        }
    }
}

/// How to run the app on what was just written.
///
/// The default path has an npm script pointing at it already; any other one
/// has to be named through the environment variable, and relative to the
/// directory the app is launched from — `tauri dev` starts it in
/// `src-tauri`, not at the workspace root.
fn how_to_run(out: &Path) -> String {
    if out == Path::new(DEFAULT_OUT_FILE) {
        return "Run `npm run dev:sim:recorded` to use them.".to_owned();
    }
    format!(
        "Run the app with {}={} to use them.",
        decktective_core::sim::SIM_TRACKS_ENV,
        out.display()
    )
}

fn build(config: &Config) -> anyhow::Result<usize> {
    let files = audio_files(&config.input);
    let mut tracks: Vec<TrackInfo> = Vec::new();

    for path in files.iter().take(config.max_tracks) {
        // One unreadable file should not cost the other three: report it
        // and carry on, the way the fixture builder from a capture skips a
        // track whose analysis is missing.
        match track::build(path) {
            Ok(built) => {
                println!("{}: {}", path.display(), describe(&built));
                tracks.push(built);
            }
            Err(error) => eprintln!("{}: skipped ({error:#})", path.display()),
        }
    }

    if tracks.is_empty() {
        return Ok(0);
    }

    // Compact rather than pretty: this file is read by a program, and
    // indentation on tens of thousands of waveform columns per track more
    // than doubles it for nothing.
    let json = serde_json::to_vec(&tracks)?;
    if let Some(parent) = config
        .out
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config.out, json)?;
    Ok(tracks.len())
}

/// A one-line summary of what was recovered, so a wrong tempo or a missing
/// grid is visible at the point it is built rather than in the UI.
fn describe(track: &TrackInfo) -> String {
    let bpm = track
        .bpm
        .map(|bpm| format!("{bpm:.2} BPM"))
        .unwrap_or_else(|| "no tempo".to_owned());
    let beats = track.beat_grid.as_ref().map(Vec::len).unwrap_or_default();
    format!(
        "{} — {}, {:.0}s, {bpm}, {beats} beats, {} cues, {} sections",
        track.artist.as_deref().unwrap_or("unknown artist"),
        track.title,
        track.duration_ms / 1000.0,
        track.cues.len(),
        track.events.len(),
    )
}

/// Every audio file under `input`, sorted, so a rebuild produces the same
/// order. A path to a single file is taken as itself.
fn audio_files(input: &Path) -> Vec<PathBuf> {
    if input.is_file() {
        return vec![input.to_path_buf()];
    }

    let Ok(entries) = std::fs::read_dir(input) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && has_audio_extension(path))
        .collect();
    found.sort();
    found
}

fn has_audio_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| AUDIO_EXTENSIONS.contains(&extension.to_lowercase().as_str()))
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config {
        input: PathBuf::from(DEFAULT_INPUT_DIR),
        out: PathBuf::from(DEFAULT_OUT_FILE),
        max_tracks: DEFAULT_MAX_TRACKS,
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => config.input = PathBuf::from(next_value(&mut args, "--input")?),
            "--out" => config.out = PathBuf::from(next_value(&mut args, "--out")?),
            "--max-tracks" => {
                config.max_tracks = next_value(&mut args, "--max-tracks")?
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

    Ok(config)
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn print_usage() {
    eprintln!("sim-audio — builds simulator track fixtures from audio files");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --input DIR|FILE  audio to read (default {DEFAULT_INPUT_DIR})");
    eprintln!("  --out FILE.json   where to write the fixtures (default {DEFAULT_OUT_FILE})");
    eprintln!("  --max-tracks N    how many tracks to keep (default {DEFAULT_MAX_TRACKS})");
}
