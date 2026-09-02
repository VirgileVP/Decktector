//! One audio file to one [`TrackInfo`] — the same type the live engine
//! reports, so nothing downstream can tell a fixture from a player.

use std::path::Path;

use decktective_core::model::{BeatGridBeat, TrackCue, TrackEvent, TrackInfo, WaveformData};
use decktective_core::structure::{self, StructureInput};

use crate::columns::{self, Column};
use crate::cues;
use crate::decode;
use crate::tempo::{self, Tempo};
use crate::waveform;

/// Builds the track an audio file describes.
pub fn build(path: &Path) -> anyhow::Result<TrackInfo> {
    let audio = decode::decode(path)?;
    let duration_ms = audio.duration_ms();
    let columns = columns::analyze(&audio.samples, audio.sample_rate);

    let waveform = waveform::build(&columns);
    let tempo = detect_tempo(&columns, audio.tags.bpm);
    let beat_grid = tempo.map(|tempo| tempo::build_beat_grid(&tempo, duration_ms));

    // Analysed twice on purpose: the section search takes cues as evidence
    // of where a boundary is, and the cues here are themselves derived from
    // sections. The first pass finds the sections, the second re-runs with
    // the cues those sections produced, which is what raises a confirmed
    // section's confidence.
    let sections = analyze(&waveform, beat_grid.as_deref(), duration_ms, &[]);
    let cues = cues::build(&sections);
    let events = analyze(&waveform, beat_grid.as_deref(), duration_ms, &cues);

    Ok(TrackInfo {
        track_id: track_id_for(path),
        title: audio.tags.title_or_placeholder(path),
        artist: audio.tags.artist,
        album: audio.tags.album,
        genre: audio.tags.genre,
        key: audio.tags.key,
        // Neither a label nor a rating exists anywhere in an audio file;
        // left empty rather than filled with something plausible.
        label: None,
        comment: audio.tags.comment,
        duration_ms,
        bpm: tempo.map(|tempo| tempo.bpm),
        artwork: audio.tags.artwork,
        beat_grid,
        waveform,
        cues,
        events,
    })
}

/// The tempo, preferring what the file declares over what the audio
/// suggests: a BPM tag was written by whoever made or analysed the track,
/// and beats detection can still be an octave off.
///
/// The phase always comes from detection — no tag records where the first
/// beat falls.
fn detect_tempo(columns: &[Column], tagged_bpm: Option<f64>) -> Option<Tempo> {
    let envelope = columns::onset_envelope(columns);
    let detected = tempo::detect(&envelope, columns)?;

    Some(Tempo {
        bpm: tagged_bpm.unwrap_or(detected.bpm),
        ..detected
    })
}

fn analyze(
    waveform: &Option<WaveformData>,
    beat_grid: Option<&[BeatGridBeat]>,
    duration_ms: f64,
    cues: &[TrackCue],
) -> Vec<TrackEvent> {
    structure::analyze(&StructureInput {
        waveform: waveform.as_ref(),
        beat_grid,
        duration_ms,
        cues,
    })
}

/// A stable id per file, so a track keeps the same identity across rebuilds
/// of the fixture — decks, history and the UI all key on it.
///
/// FNV-1a over the file name: no collision matters at four tracks, and a
/// hash keeps the id from depending on the order files were read in.
fn track_id_for(path: &Path) -> u32 {
    const OFFSET_BASIS: u32 = 2_166_136_261;
    const PRIME: u32 = 16_777_619;

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let hash = name.bytes().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(PRIME)
    });
    // 0 is what a player reports for "no track loaded".
    hash.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_file_always_takes_the_same_id() {
        let first = track_id_for(Path::new("capture/audio/carnaval.mp3"));
        let again = track_id_for(Path::new("elsewhere/carnaval.mp3"));

        assert_eq!(first, again);
        assert_ne!(first, track_id_for(Path::new("capture/audio/dark.mp3")));
    }

    #[test]
    fn an_id_is_never_the_no_track_loaded_value() {
        assert!(track_id_for(Path::new("")) > 0);
    }
}
