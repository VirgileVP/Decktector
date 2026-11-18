//! Which media a simulated deck plays its track from.
//!
//! A real deck names its track by the player holding the media, the slot and
//! the id ([`TrackRef`]), on the deck state and on the metadata alike. The sim
//! has no media, so every deck plays off its own USB stick: the deck number
//! is the source player. Deck state and metadata are both stamped from the
//! one function here, so they always agree and the metadata is never mistaken
//! for a previous track's.

use crate::model::{DeckId, TrackInfo, TrackRef};

/// The slot every simulated track is read from — the wire name a real
/// player's USB slot reports.
const SIM_MEDIA_SLOT: &str = "usb";

/// The track `deck_id` plays when it reports `track_id`.
pub(super) fn sim_track_ref(deck_id: DeckId, track_id: u32) -> TrackRef {
    TrackRef {
        source_device: deck_id,
        slot: SIM_MEDIA_SLOT.to_owned(),
        track_id,
    }
}

/// `track` as `deck_id` reports it: stamped with the media it plays from.
pub(super) fn on_deck(mut track: TrackInfo, deck_id: DeckId) -> TrackInfo {
    track.source = Some(sim_track_ref(deck_id, track.track_id));
    track
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::is_identified;
    use crate::sim::test_support::titled;

    #[test]
    fn a_track_on_a_deck_is_identified_by_that_decks_own_media() {
        let track = on_deck(titled("Strobe", Some("deadmau5")), 3);
        let loaded = sim_track_ref(3, track.track_id);
        assert!(is_identified(&loaded, Some(&track)));
        assert!(
            !is_identified(&sim_track_ref(2, track.track_id), Some(&track)),
            "the same track on another deck is read off another stick"
        );
    }
}
