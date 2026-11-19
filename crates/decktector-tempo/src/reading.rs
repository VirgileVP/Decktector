//! What the decks say about tempo right now, reduced to what gets sent.
//!
//! Pure domain: no I/O, no clock. [`read_tempo`] is the only way a
//! [`TempoReading`] is built, so [`crate::plan`] can work without knowing
//! anything about [`DeckState`].

use decktector_core::model::{DeckId, DeckState, PlayState};

use crate::config::TempoSource;

/// One deck's tempo and phase. A deck that is not playing has no value for
/// either: receivers keep what they were last told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckTempo {
    pub deck_id: DeckId,
    /// The deck's effective BPM (track BPM adjusted by pitch), `None` when
    /// unknown or when the deck is not playing.
    pub bpm: Option<f64>,
    /// Beat within the current bar (1-4), `None` when unknown or when the
    /// deck is not playing.
    pub beat_in_measure: Option<u8>,
}

impl From<&DeckState> for DeckTempo {
    fn from(deck: &DeckState) -> Self {
        let playing = deck.play_state == PlayState::Playing;
        Self {
            deck_id: deck.deck_id,
            bpm: deck.effective_bpm.filter(|_| playing),
            beat_in_measure: deck.beat_in_measure.filter(|_| playing),
        }
    }
}

/// The tempo state of every deck at one instant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TempoReading {
    /// The tempo master's reading, `None` while no deck is master.
    pub master: Option<DeckTempo>,
    /// Every known deck, sorted by deck id.
    pub decks: Vec<DeckTempo>,
}

impl TempoReading {
    /// The deck `source` designates, `None` while there is no such deck (no
    /// master, or the deck is absent).
    pub fn source(&self, source: TempoSource) -> Option<&DeckTempo> {
        match source {
            TempoSource::Master => self.master.as_ref(),
            TempoSource::Deck { deck_id } => self.decks.iter().find(|deck| deck.deck_id == deck_id),
        }
    }
}

/// Builds the reading for `decks`. Should several decks claim to be master
/// (a hand-over in flight), the lowest deck id wins, so the result does not
/// depend on iteration order.
pub fn read_tempo<'a>(decks: impl IntoIterator<Item = &'a DeckState>) -> TempoReading {
    let mut states: Vec<&DeckState> = decks.into_iter().collect();
    states.sort_by_key(|deck| deck.deck_id);

    TempoReading {
        master: states
            .iter()
            .find(|deck| deck.is_master)
            .map(|&deck| DeckTempo::from(deck)),
        decks: states.into_iter().map(DeckTempo::from).collect(),
    }
}

#[cfg(test)]
mod tests {
    use decktector_core::model::PlayState;

    use super::*;

    fn deck(deck_id: DeckId, is_master: bool, bpm: Option<f64>, beat: Option<u8>) -> DeckState {
        DeckState {
            deck_id,
            play_state: PlayState::Playing,
            is_master,
            is_synced: false,
            is_on_air: false,
            pitch: 0.0,
            track_bpm: bpm,
            effective_bpm: bpm,
            beat_in_measure: beat,
            track_id: None,
            playhead_ms: None,
            reported_at: 0.0,
            active_loop: None,
        }
    }

    #[test]
    fn no_decks_gives_an_empty_reading() {
        assert_eq!(read_tempo([]), TempoReading::default());
    }

    #[test]
    fn decks_are_sorted_by_id() {
        let decks = [
            deck(3, false, Some(120.0), None),
            deck(1, false, Some(128.0), Some(2)),
        ];
        let reading = read_tempo(&decks);
        let ids: Vec<DeckId> = reading.decks.iter().map(|deck| deck.deck_id).collect();
        assert_eq!(ids, [1, 3]);
    }

    #[test]
    fn deck_tempo_uses_the_effective_bpm() {
        let mut state = deck(1, false, Some(120.0), Some(4));
        state.effective_bpm = Some(124.8);
        let reading = read_tempo([&state]);
        assert_eq!(
            reading.decks,
            [DeckTempo {
                deck_id: 1,
                bpm: Some(124.8),
                beat_in_measure: Some(4)
            }]
        );
    }

    #[test]
    fn a_deck_that_is_not_playing_has_no_bpm_and_no_beat() {
        for play_state in [
            PlayState::Empty,
            PlayState::Loading,
            PlayState::Cued,
            PlayState::Paused,
            PlayState::Searching,
            PlayState::Ended,
        ] {
            let mut state = deck(1, true, Some(120.0), Some(2));
            state.play_state = play_state;
            let reading = read_tempo([&state]);
            let expected = DeckTempo {
                deck_id: 1,
                bpm: None,
                beat_in_measure: None,
            };
            assert_eq!(reading.master, Some(expected), "{play_state:?}");
            assert_eq!(reading.decks, [expected], "{play_state:?}");
        }
    }

    #[test]
    fn source_master_is_the_master_and_none_without_one() {
        let decks = [
            deck(1, false, Some(120.0), None),
            deck(2, true, Some(126.0), None),
        ];
        assert_eq!(
            read_tempo(&decks)
                .source(TempoSource::Master)
                .map(|d| d.deck_id),
            Some(2)
        );
        let no_master = [deck(1, false, Some(120.0), None)];
        assert_eq!(read_tempo(&no_master).source(TempoSource::Master), None);
    }

    #[test]
    fn source_deck_finds_that_deck_only() {
        let decks = [
            deck(1, false, Some(120.0), None),
            deck(3, true, Some(126.0), None),
        ];
        let reading = read_tempo(&decks);
        assert_eq!(
            reading
                .source(TempoSource::Deck { deck_id: 1 })
                .and_then(|d| d.bpm),
            Some(120.0)
        );
        assert_eq!(reading.source(TempoSource::Deck { deck_id: 2 }), None);
    }

    #[test]
    fn master_is_the_deck_flagged_as_master() {
        let decks = [
            deck(1, false, Some(120.0), None),
            deck(2, true, Some(126.0), Some(3)),
        ];
        let reading = read_tempo(&decks);
        assert_eq!(
            reading.master,
            Some(DeckTempo {
                deck_id: 2,
                bpm: Some(126.0),
                beat_in_measure: Some(3)
            })
        );
    }

    #[test]
    fn no_master_flag_means_no_master() {
        let decks = [deck(1, false, Some(120.0), None)];
        assert_eq!(read_tempo(&decks).master, None);
    }

    #[test]
    fn lowest_id_wins_when_several_decks_claim_master() {
        let decks = [
            deck(4, true, Some(1.0), None),
            deck(2, true, Some(2.0), None),
        ];
        assert_eq!(read_tempo(&decks).master.map(|deck| deck.deck_id), Some(2));
    }

    #[test]
    fn master_without_a_bpm_is_still_the_master() {
        let decks = [deck(1, true, None, None)];
        let master = read_tempo(&decks).master.expect("a master");
        assert_eq!(master.deck_id, 1);
        assert_eq!(master.bpm, None);
    }
}
