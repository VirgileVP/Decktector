//! Where the simulation's tracks come from: generated, or recorded.
//!
//! The generated tracks ([`super::tracks`]) are plausible — a shaped energy
//! envelope, an exactly regular beat grid, cues placed by rule. They are
//! enough to build a layout against and not enough to trust it: nothing
//! generated has the silence of a real breakdown, a beat grid that drifts,
//! or a track whose waveform simply looks unlike the shape the generator
//! knows how to draw.
//!
//! So the simulation can instead run on tracks a real player served,
//! recovered from a capture by `prolink-engine`'s `sim-fixtures` binary and
//! written out as [`TrackInfo`] — the very type the live engine reports, so
//! nothing downstream can tell the two apart. Point
//! [`SIM_TRACKS_ENV`] at that file to use it.
//!
//! Fixtures are opt-in and never bundled: they hold a real DJ's library
//! metadata, which is why the captures they come from are gitignored. With
//! the variable unset — the default, and every CI run — the generator
//! behaves exactly as it always has.

use std::path::{Path, PathBuf};

use crate::model::TrackInfo;

use super::tracks::{build_sim_track, SIM_TRACKS};

/// Environment variable naming a fixture file of recorded tracks.
pub const SIM_TRACKS_ENV: &str = "DECKTECTIVE_SIM_TRACKS";

/// The set of tracks the simulated decks draw from.
pub struct SimLibrary {
    source: Source,
}

enum Source {
    /// Built on demand from [`SIM_TRACKS`].
    Generated,
    /// Read from a fixture file, in the order it listed them.
    Recorded(Vec<TrackInfo>),
}

impl SimLibrary {
    /// Recorded tracks when [`SIM_TRACKS_ENV`] names a file that reads and
    /// parses, generated ones otherwise.
    ///
    /// A named file that cannot be used is reported and then fallen back
    /// from, rather than failing startup: the simulation exists to let the
    /// UI run, and a stale path in a shell profile should not stop it.
    pub fn from_env() -> Self {
        let Some(path) = std::env::var_os(SIM_TRACKS_ENV).map(PathBuf::from) else {
            return Self::generated();
        };
        match Self::from_fixture_file(&path) {
            Ok(library) => {
                tracing::info!(path = %path.display(), tracks = library.len(), "simulation using recorded tracks");
                library
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not read {SIM_TRACKS_ENV}; using generated tracks");
                Self::generated()
            }
        }
    }

    pub fn generated() -> Self {
        Self { source: Source::Generated }
    }

    /// Reads a JSON array of [`TrackInfo`], as `sim-fixtures` writes it.
    pub fn from_fixture_file(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)?;
        let tracks: Vec<TrackInfo> = serde_json::from_slice(&bytes)?;
        if tracks.is_empty() {
            anyhow::bail!("fixture file holds no tracks");
        }
        Ok(Self { source: Source::Recorded(tracks) })
    }

    /// How many distinct tracks exist — what the engine takes its rolling
    /// index modulo, so decks pick different ones.
    pub fn len(&self) -> usize {
        match &self.source {
            Source::Generated => SIM_TRACKS.len(),
            Source::Recorded(tracks) => tracks.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The track at `index`, wrapping so no caller can index out of range.
    ///
    /// `track_id` is the id the engine hands out for this load. A generated
    /// track takes it — it has no identity of its own, and the id also
    /// seeds its waveform. A recorded track keeps the id the player it came
    /// from actually used, which is the whole point of it being recorded.
    pub fn track(&self, index: usize, track_id: u32) -> TrackInfo {
        match &self.source {
            Source::Generated => build_sim_track(&SIM_TRACKS[index % SIM_TRACKS.len()], track_id),
            Source::Recorded(tracks) => tracks[index % tracks.len()].clone(),
        }
    }
}

impl Default for SimLibrary {
    fn default() -> Self {
        Self::generated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_library_wraps_rather_than_indexing_out_of_range() {
        let library = SimLibrary::generated();
        let first = library.track(0, 100);
        let wrapped = library.track(SIM_TRACKS.len(), 100);

        assert_eq!(library.len(), SIM_TRACKS.len());
        assert_eq!(first.title, wrapped.title);
    }

    #[test]
    fn a_recorded_track_keeps_the_id_it_was_recorded_with() {
        let recorded = SimLibrary { source: Source::Recorded(vec![track_named("Recorded", 4242)]) };

        let loaded = library_track_ignoring_engine_id(&recorded);
        assert_eq!(loaded.track_id, 4242);
        assert_eq!(loaded.title, "Recorded");
    }

    /// A generated track has no identity of its own, so it takes whichever
    /// id the engine is handing out.
    #[test]
    fn a_generated_track_takes_the_engine_s_id() {
        assert_eq!(SimLibrary::generated().track(0, 777).track_id, 777);
    }

    #[test]
    fn a_fixture_file_that_does_not_exist_is_an_error_not_a_panic() {
        assert!(SimLibrary::from_fixture_file(Path::new("no/such/fixture.json")).is_err());
    }

    /// The fixture format is exactly what `TrackInfo` serialises to, so a
    /// round trip through a file is the whole contract between
    /// `sim-fixtures` and this.
    #[test]
    fn a_written_fixture_file_reads_back() {
        let path = std::env::temp_dir().join("decktective-sim-library-round-trip.json");
        let written = vec![track_named("First", 11), track_named("Second", 22)];
        std::fs::write(&path, serde_json::to_vec(&written).expect("serialises")).expect("writes");

        let library = SimLibrary::from_fixture_file(&path).expect("reads back");

        assert_eq!(library.len(), 2);
        assert_eq!(library.track(1, 0).title, "Second");
        let _ = std::fs::remove_file(&path);
    }

    /// A file that parses but lists nothing would leave the engine indexing
    /// modulo zero.
    #[test]
    fn an_empty_fixture_file_is_rejected() {
        let path = std::env::temp_dir().join("decktective-sim-library-empty.json");
        std::fs::write(&path, b"[]").expect("writes");

        assert!(SimLibrary::from_fixture_file(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    fn library_track_ignoring_engine_id(library: &SimLibrary) -> TrackInfo {
        library.track(0, 100)
    }

    fn track_named(title: &str, track_id: u32) -> TrackInfo {
        TrackInfo {
            track_id,
            title: title.to_owned(),
            artist: None,
            album: None,
            genre: None,
            key: None,
            label: None,
            comment: None,
            duration_ms: 1000.0,
            bpm: None,
            artwork: None,
            beat_grid: None,
            waveform: None,
            cues: Vec::new(),
            events: Vec::new(),
        }
    }
}
