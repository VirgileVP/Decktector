//! Maps a raw [`DeviceStatus`] onto decktector's domain [`DeckState`].

use decktector_core::model::{DeckState, PlayState};
use prolink::status::{DeviceStatus, RawPlayState};

/// Builds the deck state a status packet implies, anchored at `now_ms`.
/// `playhead_ms` comes from [`crate::position::PositionTracker`] rather than
/// being derived here, since it also has to fold in CDJ-3000 precise
/// -position packets that arrive on a separate stream.
pub fn to_deck_state(status: &DeviceStatus, playhead_ms: Option<f64>, now_ms: f64) -> DeckState {
    let track_bpm = status.track_bpm;
    DeckState {
        deck_id: status.device_number,
        play_state: play_state(status.play_state),
        is_master: status.is_master,
        is_synced: status.is_synced,
        is_on_air: status.is_on_air,
        pitch: status.pitch_percent,
        track_bpm,
        effective_bpm: track_bpm.map(|bpm| bpm * (1.0 + status.pitch_percent / 100.0)),
        beat_in_measure: status.beat_in_bar,
        track_id: status.track_id,
        playhead_ms,
        reported_at: now_ms,
    }
}

/// Folds Pioneer's own play-mode states onto decktector's simpler model —
/// the same folding decktector's pre-Rust engine applied to the equivalent
/// states `prolink-connect` decoded (`normalize.ts`'s `PLAY_STATE_MAP`):
/// looping counts as playing, both "paused at the cue point" and "cue play
/// in progress" read as cued, and "cue scratch in progress" reads as
/// paused. A value this parser doesn't recognize falls back to `Empty`,
/// matching that same table's fallback for anything outside its map.
fn play_state(raw: RawPlayState) -> PlayState {
    match raw {
        RawPlayState::NoTrack => PlayState::Empty,
        RawPlayState::Loading => PlayState::Loading,
        RawPlayState::Playing | RawPlayState::Looping => PlayState::Playing,
        RawPlayState::PausedAway | RawPlayState::CueScratching => PlayState::Paused,
        RawPlayState::PausedAtCue | RawPlayState::CuePlaying => PlayState::Cued,
        RawPlayState::Searching | RawPlayState::SpunDown => PlayState::Searching,
        RawPlayState::Ended => PlayState::Ended,
        RawPlayState::EmergencyLoop | RawPlayState::Unknown(_) => PlayState::Empty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prolink::status::MediaSlot;

    fn status(overrides: impl FnOnce(DeviceStatus) -> DeviceStatus) -> DeviceStatus {
        overrides(DeviceStatus {
            device_number: 2,
            track_source_device: 2,
            play_state: RawPlayState::NoTrack,
            is_master: false,
            is_synced: false,
            is_on_air: false,
            media_slot: MediaSlot::Usb,
            track_type: 1,
            pitch_percent: 0.0,
            track_bpm: None,
            beat_count: None,
            beat_in_bar: None,
            track_id: None,
        })
    }

    #[test]
    fn no_track_loaded_is_empty() {
        let deck = to_deck_state(&status(|status| status), None, 1_000.0);
        assert_eq!(deck.play_state, PlayState::Empty);
    }

    #[test]
    fn playing_and_looping_both_read_as_playing() {
        for raw in [RawPlayState::Playing, RawPlayState::Looping] {
            let deck = to_deck_state(
                &status(|mut status| {
                    status.play_state = raw;
                    status
                }),
                None,
                1_000.0,
            );
            assert_eq!(
                deck.play_state,
                PlayState::Playing,
                "{raw:?} should read as Playing"
            );
        }
    }

    #[test]
    fn paused_at_cue_and_cue_playing_both_read_as_cued() {
        for raw in [RawPlayState::PausedAtCue, RawPlayState::CuePlaying] {
            let deck = to_deck_state(
                &status(|mut status| {
                    status.play_state = raw;
                    status
                }),
                None,
                1_000.0,
            );
            assert_eq!(
                deck.play_state,
                PlayState::Cued,
                "{raw:?} should read as Cued"
            );
        }
    }

    #[test]
    fn paused_away_and_cue_scratching_both_read_as_paused() {
        for raw in [RawPlayState::PausedAway, RawPlayState::CueScratching] {
            let deck = to_deck_state(
                &status(|mut status| {
                    status.play_state = raw;
                    status
                }),
                None,
                1_000.0,
            );
            assert_eq!(
                deck.play_state,
                PlayState::Paused,
                "{raw:?} should read as Paused"
            );
        }
    }

    #[test]
    fn searching_and_spun_down_both_read_as_searching() {
        for raw in [RawPlayState::Searching, RawPlayState::SpunDown] {
            let deck = to_deck_state(
                &status(|mut status| {
                    status.play_state = raw;
                    status
                }),
                None,
                1_000.0,
            );
            assert_eq!(
                deck.play_state,
                PlayState::Searching,
                "{raw:?} should read as Searching"
            );
        }
    }

    #[test]
    fn loading_and_ended_map_directly() {
        let loading = to_deck_state(
            &status(|mut status| {
                status.play_state = RawPlayState::Loading;
                status
            }),
            None,
            1_000.0,
        );
        assert_eq!(loading.play_state, PlayState::Loading);

        let ended = to_deck_state(
            &status(|mut status| {
                status.play_state = RawPlayState::Ended;
                status
            }),
            None,
            1_000.0,
        );
        assert_eq!(ended.play_state, PlayState::Ended);
    }

    #[test]
    fn an_unrecognized_play_state_falls_back_to_empty() {
        let deck = to_deck_state(
            &status(|mut status| {
                status.play_state = RawPlayState::Unknown(0x7f);
                status
            }),
            None,
            1_000.0,
        );
        assert_eq!(deck.play_state, PlayState::Empty);
    }

    #[test]
    fn effective_bpm_scales_with_pitch() {
        let deck = to_deck_state(
            &status(|mut status| {
                status.play_state = RawPlayState::Playing;
                status.track_bpm = Some(120.0);
                status.pitch_percent = 10.0;
                status
            }),
            None,
            1_000.0,
        );
        assert_eq!(deck.track_bpm, Some(120.0));
        assert_eq!(deck.effective_bpm, Some(132.0));
    }

    #[test]
    fn reports_the_deck_id_and_timestamp_it_was_given() {
        let deck = to_deck_state(&status(|status| status), Some(4_200.0), 9_999.0);
        assert_eq!(deck.deck_id, 2);
        assert_eq!(deck.playhead_ms, Some(4_200.0));
        assert_eq!(deck.reported_at, 9_999.0);
    }
}
