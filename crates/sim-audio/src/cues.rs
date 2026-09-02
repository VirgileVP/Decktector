//! Cues, laid out over the sections the track's own structure analysis
//! found.
//!
//! An MP3 carries no cues: rekordbox keeps a DJ's in its database, not in
//! the file. So these are the cues a DJ would plausibly have set — one per
//! section boundary, hot cues while the eight buttons last — and they are
//! honest about being derived: they sit exactly where the section analysis
//! put a boundary, not two bars before the drop the way a DJ's ear does.

use decktective_core::model::{CueKind, TrackCue, TrackEvent, TrackEventKind};

/// Hot cue buttons a CDJ offers. Boundaries past the eighth become memory
/// cues, the way a DJ who has run out of buttons would store them.
const HOT_CUE_BUTTONS: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];
/// Marker colours, matching the generated simulation tracks
/// (`decktective_core::sim`) so both libraries read the same on screen.
const DROP_COLOR: &str = "#0a84ff";
const BREAK_COLOR: &str = "#ffd60a";
const BUILD_COLOR: &str = "#12d17e";
const SECTION_COLOR: &str = "#ff9f2e";

/// One cue per section start, skipping the section that starts the track:
/// nobody sets a cue on 0:00.
pub fn build(events: &[TrackEvent]) -> Vec<TrackCue> {
    let mut button = 0usize;
    events
        .iter()
        .filter(|event| event.start_ms > 0.0)
        .map(|event| {
            let hot = HOT_CUE_BUTTONS
                .get(button)
                .filter(|_| deserves_a_button(event.kind));
            if hot.is_some() {
                button += 1;
            }
            TrackCue {
                kind: if hot.is_some() {
                    CueKind::Hotcue
                } else {
                    CueKind::Memory
                },
                offset_ms: event.start_ms,
                length_ms: None,
                button: hot.map(|button| (*button).to_owned()),
                label: Some(label_of(event.kind).to_owned()),
                color: color_of(event.kind).to_owned(),
            }
        })
        .collect()
}

/// The sections a DJ actually jumps to mid-set get a button; the ones they
/// only need to see get a memory cue.
fn deserves_a_button(kind: TrackEventKind) -> bool {
    matches!(
        kind,
        TrackEventKind::Drop | TrackEventKind::Break | TrackEventKind::Build
    )
}

fn color_of(kind: TrackEventKind) -> &'static str {
    match kind {
        TrackEventKind::Drop => DROP_COLOR,
        TrackEventKind::Break => BREAK_COLOR,
        TrackEventKind::Build => BUILD_COLOR,
        _ => SECTION_COLOR,
    }
}

fn label_of(kind: TrackEventKind) -> &'static str {
    match kind {
        TrackEventKind::Intro => "Intro",
        TrackEventKind::Build => "Build",
        TrackEventKind::Drop => "Drop",
        TrackEventKind::Break => "Break",
        TrackEventKind::Groove => "Groove",
        TrackEventKind::Outro => "Outro",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: TrackEventKind, start_ms: f64) -> TrackEvent {
        TrackEvent {
            kind,
            start_ms,
            end_ms: start_ms + 1_000.0,
            confidence: 0.5,
        }
    }

    #[test]
    fn the_section_starting_the_track_gets_no_cue() {
        let cues = build(&[
            event(TrackEventKind::Intro, 0.0),
            event(TrackEventKind::Drop, 8_000.0),
        ]);

        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].offset_ms, 8_000.0);
    }

    #[test]
    fn drops_and_breaks_take_buttons_in_order() {
        let cues = build(&[
            event(TrackEventKind::Drop, 8_000.0),
            event(TrackEventKind::Break, 16_000.0),
            event(TrackEventKind::Outro, 24_000.0),
        ]);

        assert_eq!(cues[0].button.as_deref(), Some("A"));
        assert_eq!(cues[0].kind, CueKind::Hotcue);
        assert_eq!(cues[1].button.as_deref(), Some("B"));
        assert_eq!(cues[2].button, None);
        assert_eq!(cues[2].kind, CueKind::Memory);
    }

    #[test]
    fn a_ninth_boundary_becomes_a_memory_cue() {
        let events: Vec<TrackEvent> = (1..=9)
            .map(|bar| event(TrackEventKind::Drop, f64::from(bar) * 8_000.0))
            .collect();

        let cues = build(&events);

        assert_eq!(cues[7].button.as_deref(), Some("H"));
        assert_eq!(cues[8].button, None);
        assert_eq!(cues[8].kind, CueKind::Memory);
    }
}
