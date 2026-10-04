//! The contract between this tool and the simulation: whatever it writes,
//! `SimLibrary` reads back.
//!
//! Both inputs are gitignored — the audio is someone's copyrighted work and
//! the fixture is built from it — so the test runs against them when a
//! machine has them and passes quietly when it does not. That keeps CI
//! green while still catching the failure that actually happens here: a
//! `TrackInfo` field changing shape and the two ends drifting apart.

use std::path::Path;

use decktector_core::sim::SimLibrary;

const FIXTURE: &str = "../decktector-core/fixtures/sim-tracks.json";

#[test]
fn the_simulation_reads_back_what_the_builder_wrote() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    if !path.exists() {
        eprintln!(
            "no fixture at {} — run sim-audio to build one",
            path.display()
        );
        return;
    }

    let library = SimLibrary::from_fixture_file(&path).expect("the fixture parses");
    assert!(!library.is_empty());

    let track = library.track(0, 1);
    assert!(
        !track.title.is_empty(),
        "a track always has something to show as a title"
    );
    assert!(track.duration_ms > 0.0, "a decoded track has a duration");
    let waveform = track
        .waveform
        .expect("a track built from audio carries its waveform");
    assert!(waveform.detail.is_some_and(|detail| !detail.is_empty()));
    assert!(waveform.preview.is_some_and(|preview| !preview.is_empty()));
}
