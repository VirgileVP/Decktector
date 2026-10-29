//! Which loaded track each deck was last announced to one client with.

use std::collections::HashMap;

use decktector_core::model::DeckId;
use decktector_core::sim::SimDeckView;

/// Per-client memory of the track index announced for each deck, so a
/// [`SimControlMessage::Track`] goes out only when a deck's track changes.
///
/// [`SimControlMessage::Track`]: crate::wire::SimControlMessage::Track
#[derive(Debug, Default)]
pub(crate) struct TrackAnnouncements {
    announced: HashMap<DeckId, Option<usize>>,
}

impl TrackAnnouncements {
    /// The decks whose loaded track differs from what this client was last
    /// told — every deck on the first call, since nothing was announced yet —
    /// recorded as announced.
    pub(crate) fn changed(&mut self, decks: &[SimDeckView]) -> Vec<(DeckId, Option<usize>)> {
        decks
            .iter()
            .filter_map(|deck| {
                let previous = self.announced.insert(deck.deck_id, deck.track_index);
                (previous != Some(deck.track_index)).then_some((deck.deck_id, deck.track_index))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decktector_core::sim::{SimEngine, SimLibrary};

    /// Four decks from a real manual rig, with the given track indices.
    fn decks(track_indices: [Option<usize>; 4]) -> Vec<SimDeckView> {
        let (_engine, control) = SimEngine::manual(SimLibrary::generated(), None);
        let mut decks = control.state().decks;
        for (deck, track_index) in decks.iter_mut().zip(track_indices) {
            deck.track_index = track_index;
        }
        decks
    }

    #[test]
    fn the_first_call_announces_every_deck_including_empty_ones() {
        let mut announcements = TrackAnnouncements::default();
        let changed = announcements.changed(&decks([Some(3), None, None, Some(0)]));
        assert_eq!(changed.len(), 4);
        assert!(changed.contains(&(1, Some(3))));
        assert!(changed.contains(&(2, None)));
    }

    #[test]
    fn an_unchanged_rig_announces_nothing() {
        let mut announcements = TrackAnnouncements::default();
        let rig = decks([Some(1), None, None, None]);
        announcements.changed(&rig);
        assert!(announcements.changed(&rig).is_empty());
    }

    #[test]
    fn only_the_decks_whose_track_changed_are_announced() {
        let mut announcements = TrackAnnouncements::default();
        announcements.changed(&decks([Some(1), Some(2), None, None]));
        let changed = announcements.changed(&decks([Some(1), None, Some(5), None]));
        assert_eq!(changed.len(), 2);
        assert!(changed.contains(&(2, None)));
        assert!(changed.contains(&(3, Some(5))));
    }
}
