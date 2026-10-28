//! Turns the phrases rekordbox found in a track ([`SongStructure`]) into the
//! model's sections ([`TrackEvent`]), placed on the track's beat grid.
//!
//! Each phrase keeps rekordbox's own name as its label, and is given the
//! closest of the model's kinds so the deck colours it like an inferred
//! section of the same character. Runs of phrases with the same name (four
//! "Chorus" phrases back to back) become one section: the deck counts
//! sections down, and a countdown restarting every four bars inside one
//! chorus would tell the reader nothing.

use decktector_core::model::{BeatGridBeat, TrackEvent, TrackEventKind};
use decktector_core::structure::{self, StructureInput};
use prolink::nfs::{Phrase, PhraseKind, SongStructure};

/// Rekordbox's analysis, not an inference — as sure as a section gets.
const PHRASE_CONFIDENCE: f32 = 1.0;

/// A track's sections: rekordbox's phrases when it has them and they fit
/// the beat grid, otherwise the structure inferred from `input`.
pub fn sections(
    song_structure: Option<&SongStructure>,
    input: &StructureInput<'_>,
) -> Vec<TrackEvent> {
    let phrases = song_structure
        .zip(input.beat_grid)
        .map(|(structure, beat_grid)| track_events(structure, beat_grid, input.duration_ms))
        .unwrap_or_default();
    if phrases.is_empty() {
        structure::analyze(input)
    } else {
        phrases
    }
}

/// The sections `structure` describes, timed by `beat_grid`. Empty when
/// the beat grid does not reach every phrase, since a phrase placed at a
/// guessed time is worse than the inferred structure the caller falls back
/// to.
pub fn track_events(
    structure: &SongStructure,
    beat_grid: &[BeatGridBeat],
    duration_ms: f64,
) -> Vec<TrackEvent> {
    let beat_ms = |beat: u16| {
        usize::from(beat)
            .checked_sub(1)
            .and_then(|index| beat_grid.get(index))
            .map(|beat| beat.offset_ms)
    };
    // The track runs on past its last phrase, mostly in silence, and its
    // end beat can fall after the grid's last beat.
    let end_ms = beat_ms(structure.end_beat).unwrap_or(duration_ms);

    let Some(starts) = structure
        .phrases
        .iter()
        .map(|phrase| beat_ms(phrase.start_beat))
        .collect::<Option<Vec<f64>>>()
    else {
        return Vec::new();
    };

    let mut events: Vec<TrackEvent> = Vec::with_capacity(structure.phrases.len());
    for (index, phrase) in structure.phrases.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(end_ms);
        let label = phrase.label();
        match events.last_mut() {
            Some(previous) if previous.label.as_deref() == Some(label.as_str()) => {
                previous.end_ms = end;
            }
            _ => events.push(TrackEvent {
                kind: event_kind(phrase),
                start_ms: starts[index],
                end_ms: end,
                confidence: PHRASE_CONFIDENCE,
                label: Some(label),
            }),
        }
    }
    events.retain(|event| event.end_ms > event.start_ms);
    events
}

fn event_kind(phrase: &Phrase) -> TrackEventKind {
    match phrase.kind {
        PhraseKind::Intro => TrackEventKind::Intro,
        PhraseKind::Up => TrackEventKind::Build,
        PhraseKind::Down | PhraseKind::Bridge => TrackEventKind::Break,
        PhraseKind::Chorus => TrackEventKind::Drop,
        PhraseKind::Verse => TrackEventKind::Groove,
        PhraseKind::Outro => TrackEventKind::Outro,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prolink::nfs::PhraseMood;

    /// A beat every half second — 120 BPM — starting at 0.
    fn grid(beats: usize) -> Vec<BeatGridBeat> {
        (0..beats)
            .map(|index| BeatGridBeat {
                offset_ms: index as f64 * 500.0,
                beat_in_measure: (index % 4) as u8 + 1,
            })
            .collect()
    }

    fn phrase(start_beat: u16, kind: PhraseKind, variant: Option<u8>) -> Phrase {
        Phrase {
            start_beat,
            kind,
            variant,
        }
    }

    fn structure(end_beat: u16, phrases: Vec<Phrase>) -> SongStructure {
        SongStructure {
            mood: PhraseMood::Mid,
            end_beat,
            phrases,
        }
    }

    fn summary(events: &[TrackEvent]) -> Vec<(TrackEventKind, f64, f64, &str)> {
        events
            .iter()
            .map(|event| {
                (
                    event.kind,
                    event.start_ms,
                    event.end_ms,
                    event.label.as_deref().unwrap_or(""),
                )
            })
            .collect()
    }

    #[test]
    fn places_each_phrase_on_its_beats_with_rekordboxs_name() {
        let song = structure(
            49,
            vec![
                phrase(1, PhraseKind::Intro, None),
                phrase(17, PhraseKind::Verse, Some(2)),
                phrase(33, PhraseKind::Chorus, None),
            ],
        );

        let events = track_events(&song, &grid(64), 40_000.0);

        assert_eq!(
            summary(&events),
            vec![
                (TrackEventKind::Intro, 0.0, 8_000.0, "Intro"),
                (TrackEventKind::Groove, 8_000.0, 16_000.0, "Verse 2"),
                (TrackEventKind::Drop, 16_000.0, 24_000.0, "Chorus"),
            ]
        );
        assert!(events.iter().all(|event| event.confidence == 1.0));
    }

    #[test]
    fn phrases_with_the_same_name_back_to_back_are_one_section() {
        let song = structure(
            49,
            vec![
                phrase(1, PhraseKind::Bridge, None),
                phrase(17, PhraseKind::Bridge, None),
                phrase(33, PhraseKind::Verse, Some(4)),
            ],
        );

        let events = track_events(&song, &grid(64), 40_000.0);

        assert_eq!(
            summary(&events),
            vec![
                (TrackEventKind::Break, 0.0, 16_000.0, "Bridge"),
                (TrackEventKind::Groove, 16_000.0, 24_000.0, "Verse 4"),
            ]
        );
    }

    #[test]
    fn a_last_phrase_ending_past_the_grid_ends_with_the_track() {
        let song = structure(200, vec![phrase(1, PhraseKind::Outro, None)]);

        let events = track_events(&song, &grid(64), 40_000.0);

        assert_eq!(events[0].end_ms, 40_000.0);
    }

    #[test]
    fn a_phrase_the_grid_does_not_reach_drops_the_structure() {
        let song = structure(
            300,
            vec![
                phrase(1, PhraseKind::Intro, None),
                phrase(250, PhraseKind::Outro, None),
            ],
        );

        assert!(track_events(&song, &grid(64), 40_000.0).is_empty());
    }

    #[test]
    fn the_high_moods_phrases_take_the_nearest_kind() {
        let song = SongStructure {
            mood: PhraseMood::High,
            end_beat: 65,
            phrases: vec![
                phrase(1, PhraseKind::Up, Some(3)),
                phrase(17, PhraseKind::Chorus, Some(1)),
                phrase(33, PhraseKind::Down, None),
                phrase(49, PhraseKind::Outro, Some(2)),
            ],
        };

        let events = track_events(&song, &grid(64), 40_000.0);

        let kinds: Vec<_> = events.iter().map(|event| event.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TrackEventKind::Build,
                TrackEventKind::Drop,
                TrackEventKind::Break,
                TrackEventKind::Outro,
            ]
        );
        assert_eq!(events[0].label.as_deref(), Some("Up 3"));
    }
}
