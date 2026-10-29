//! Four simulated CDJ-3000s and the mixer between them, as pure state.
//!
//! [`SimRig`] is the manual simulation without its plumbing: commands go in
//! through [`SimRig::apply`], time through [`SimRig::advance`], and what a
//! viewer sees comes out of [`SimRig::view`]. It reads no clock and spawns
//! nothing, so the engine around it owns scheduling and tests own time.
//!
//! The rig adds to its decks what no single deck can know:
//!
//! - **Master.** Exactly one deck at a time, or none before anything has
//!   played. The first deck to start playing takes it while nobody holds it;
//!   [`SimCommand::Master`] takes it explicitly; it stays with a deck that
//!   stops or pauses, and is released only when that deck is ejected.
//! - **Sync.** A synced deck matches the master's effective BPM, following
//!   every change to its pitch, and lines its beats up with the master's
//!   when sync is switched on and whenever it starts playing.
//! - **Mixer.** One channel fader per deck; a deck is on air when its fader
//!   is open, whatever it is doing.

use std::cmp::Ordering;
use std::sync::Arc;

use super::command::SimCommand;
use super::deck::SimDeckModel;
use super::library::{SimLibrary, FIRST_TRACK_ID};
use super::rig_state::{SimDeckView, SimRigState, SimTrackSummary};
use crate::model::{DeckId, DeckState, TrackInfo};

/// The rig is a fixed four-deck setup, like a club booth.
pub(super) const DECK_COUNT: u8 = 4;

pub(super) struct SimRig {
    decks: Vec<SimDeckModel>,
    channel_faders: Vec<f64>,
    master: Option<DeckId>,
    /// Sorted by title; indices into it are what [`SimCommand::Load`] uses.
    library: Arc<[Arc<TrackInfo>]>,
    summaries: Vec<SimTrackSummary>,
    fallback_warning: Option<String>,
}

impl SimRig {
    /// The rig as it stands when the power comes on: the first tracks of the
    /// sorted library on decks 1 to 4, each cued, every fader closed.
    pub fn new(tracks: Vec<TrackInfo>, fallback_warning: Option<String>) -> Self {
        let library: Arc<[Arc<TrackInfo>]> = sorted(tracks).into_iter().map(Arc::new).collect();
        let summaries = library
            .iter()
            .enumerate()
            .map(|(index, track)| SimTrackSummary::of(index, track))
            .collect();
        let decks = (1..=DECK_COUNT)
            .map(|deck_id| {
                let mut deck = SimDeckModel::empty(deck_id);
                let index = usize::from(deck_id) - 1;
                if let Some(track) = library.get(index) {
                    deck.load(index, Arc::clone(track));
                }
                deck
            })
            .collect();

        Self {
            decks,
            channel_faders: vec![0.0; usize::from(DECK_COUNT)],
            master: None,
            library,
            summaries,
            fallback_warning,
        }
    }

    pub fn from_library(library: &SimLibrary, fallback_warning: Option<String>) -> Self {
        Self::new(library.materialize(FIRST_TRACK_ID), fallback_warning)
    }

    /// The sorted library, shared so a control handle can look tracks up
    /// without asking the engine.
    pub fn library(&self) -> Arc<[Arc<TrackInfo>]> {
        Arc::clone(&self.library)
    }

    /// The track a deck holds, in the form engines report it.
    pub fn loaded_track(&self, deck_id: DeckId) -> Option<TrackInfo> {
        self.deck(deck_id)?.track().cloned()
    }

    pub fn apply(&mut self, command: SimCommand) {
        match command {
            SimCommand::Load { deck, track_index } => self.load(deck, track_index),
            SimCommand::Eject { deck } => self.eject(deck),
            SimCommand::PlayPause { deck } => self.play_pause(deck),
            SimCommand::Cue { deck } => self.with_deck(deck, SimDeckModel::cue),
            SimCommand::Seek { deck, position_ms } => {
                self.with_deck(deck, |d| d.seek(position_ms));
            }
            SimCommand::SetPitchFader { deck, position } => {
                self.with_deck(deck, |d| d.set_pitch_fader(position));
            }
            SimCommand::SetTempoRange { deck, range } => {
                self.with_deck(deck, |d| d.set_tempo_range(range));
            }
            SimCommand::JogTouch { deck, touched } => {
                self.with_deck(deck, |d| d.set_jog_touched(touched));
            }
            SimCommand::JogScratch { deck, delta_ms } => {
                self.with_deck(deck, |d| d.scratch(delta_ms));
            }
            SimCommand::JogNudge { deck, bend } => self.with_deck(deck, |d| d.set_nudge(bend)),
            SimCommand::HotCue { deck, button } => self.with_deck(deck, |d| d.hot_cue(&button)),
            SimCommand::Loop { deck, beats } => self.with_deck(deck, |d| d.start_loop(beats)),
            SimCommand::ReloopExit { deck } => self.with_deck(deck, SimDeckModel::reloop_exit),
            SimCommand::Sync { deck } => self.toggle_sync(deck),
            SimCommand::Master { deck } => self.take_master(deck),
            SimCommand::SetChannelFader { deck, level } => self.set_channel_fader(deck, level),
        }
        self.follow_master_tempo();
    }

    /// Lets `elapsed_ms` of real time pass on every deck.
    pub fn advance(&mut self, elapsed_ms: f64) {
        self.follow_master_tempo();
        for deck in &mut self.decks {
            deck.advance(elapsed_ms);
        }
    }

    pub fn view(&self, reported_at: f64) -> SimRigState {
        SimRigState {
            decks: self.deck_views(),
            channel_faders: self.channel_faders.clone(),
            library: self.summaries.clone(),
            fallback_warning: self.fallback_warning.clone(),
            reported_at,
        }
    }

    /// Every deck as the rest of the app sees it.
    pub fn deck_states(&self, reported_at: f64) -> Vec<DeckState> {
        self.deck_views()
            .iter()
            .map(|deck| deck.to_deck_state(reported_at))
            .collect()
    }

    fn deck_views(&self) -> Vec<SimDeckView> {
        self.decks
            .iter()
            .zip(&self.channel_faders)
            .map(|(deck, fader)| deck.view(self.master == Some(deck.deck_id()), *fader > 0.0))
            .collect()
    }

    // ---- commands ------------------------------------------------------

    fn load(&mut self, deck_id: DeckId, track_index: usize) {
        let Some(track) = self.library.get(track_index).map(Arc::clone) else {
            tracing::warn!(track_index, "load of a track outside the library ignored");
            return;
        };
        self.with_deck(deck_id, |deck| deck.load(track_index, track));
    }

    fn eject(&mut self, deck_id: DeckId) {
        let Some(deck) = self.deck_mut(deck_id) else {
            return;
        };
        if deck.eject() && self.master == Some(deck_id) {
            self.master = None;
        }
    }

    fn play_pause(&mut self, deck_id: DeckId) {
        let Some(deck) = self.deck_mut(deck_id) else {
            return;
        };
        if !deck.toggle_play() {
            return;
        }
        if self.master.is_none() {
            self.master = Some(deck_id);
        } else {
            self.align_to_master(deck_id);
        }
    }

    fn toggle_sync(&mut self, deck_id: DeckId) {
        let Some(deck) = self.deck_mut(deck_id) else {
            return;
        };
        let synced = !deck.is_synced();
        deck.set_synced(synced);
        if synced {
            self.align_to_master(deck_id);
        }
    }

    fn take_master(&mut self, deck_id: DeckId) {
        if self
            .deck(deck_id)
            .is_some_and(|deck| deck.track().is_some())
        {
            self.master = Some(deck_id);
        }
    }

    fn set_channel_fader(&mut self, deck_id: DeckId, level: f64) {
        if let Some(index) = deck_index(deck_id) {
            self.channel_faders[index] = level.clamp(0.0, 1.0);
        }
    }

    // ---- sync ----------------------------------------------------------

    /// The deck `follower` would sync to: the master, if it is another deck
    /// with a tempo to follow.
    fn sync_reference(&self, follower: DeckId) -> Option<&SimDeckModel> {
        self.master
            .filter(|master| *master != follower)
            .and_then(|master| self.deck(master))
            .filter(|master| master.effective_bpm().is_some())
    }

    /// Gives every synced follower the pitch that matches the master's
    /// current tempo, and everyone else their fader back.
    fn follow_master_tempo(&mut self) {
        if let Some(master) = self.master {
            self.with_deck(master, |deck| deck.set_sync_pitch(None));
        }
        let master_bpm = self
            .master
            .and_then(|master| self.deck(master))
            .and_then(SimDeckModel::effective_bpm);

        for deck in &mut self.decks {
            let is_follower = deck.is_synced() && self.master != Some(deck.deck_id());
            let pitch = master_bpm
                .zip(deck.track_bpm())
                .filter(|_| is_follower)
                .map(|(master_bpm, track_bpm)| (master_bpm / track_bpm - 1.0) * 100.0);
            deck.set_sync_pitch(pitch);
        }
    }

    /// Shifts a synced deck's beats onto the master's. Does nothing for a
    /// deck that is not synced, or when there is no master to align to.
    fn align_to_master(&mut self, deck_id: DeckId) {
        if !self.deck(deck_id).is_some_and(SimDeckModel::is_synced) {
            return;
        }
        let Some(target_phase) = self
            .sync_reference(deck_id)
            .and_then(SimDeckModel::beat_phase)
        else {
            return;
        };
        self.with_deck(deck_id, |deck| deck.align_beat_phase(target_phase));
    }

    // ---- deck lookup ---------------------------------------------------

    fn deck(&self, deck_id: DeckId) -> Option<&SimDeckModel> {
        self.decks.get(deck_index(deck_id)?)
    }

    fn deck_mut(&mut self, deck_id: DeckId) -> Option<&mut SimDeckModel> {
        let index = deck_index(deck_id)?;
        self.decks.get_mut(index)
    }

    /// Runs `action` on a deck, ignoring a command for a deck the rig does
    /// not have.
    fn with_deck(&mut self, deck_id: DeckId, action: impl FnOnce(&mut SimDeckModel)) {
        match self.deck_mut(deck_id) {
            Some(deck) => action(deck),
            None => tracing::warn!(deck_id, "command for a deck the rig does not have ignored"),
        }
    }
}

fn deck_index(deck_id: DeckId) -> Option<usize> {
    (1..=DECK_COUNT)
        .contains(&deck_id)
        .then(|| usize::from(deck_id) - 1)
}

/// Title order, ignoring case, with the artist breaking ties.
fn sorted(mut tracks: Vec<TrackInfo>) -> Vec<TrackInfo> {
    tracks.sort_by(|a, b| {
        let by_title = a.title.to_lowercase().cmp(&b.title.to_lowercase());
        if by_title != Ordering::Equal {
            return by_title;
        }
        let artist = |track: &TrackInfo| track.artist.as_deref().map(str::to_lowercase);
        artist(a).cmp(&artist(b))
    });
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PlayState;
    use crate::sim::command::TempoRange;
    use crate::sim::test_support::{memory_cue, titled, track_with_grid};

    const REPORTED_AT: f64 = 1_000.0;

    fn named_track(title: &str, bpm: f64) -> TrackInfo {
        TrackInfo {
            title: title.to_owned(),
            ..track_with_grid(bpm, 600)
        }
    }

    /// Five tracks, handed over out of order so that the sort shows.
    fn rig() -> SimRig {
        SimRig::new(
            vec![
                named_track("e", 126.0),
                named_track("B", 128.0),
                named_track("a", 120.0),
                named_track("D", 124.0),
                named_track("c", 122.0),
            ],
            None,
        )
    }

    fn state_of(rig: &SimRig, deck: DeckId) -> SimDeckView {
        rig.view(REPORTED_AT).decks[usize::from(deck) - 1].clone()
    }

    fn master_of(rig: &SimRig) -> Option<DeckId> {
        rig.view(REPORTED_AT)
            .decks
            .iter()
            .find(|deck| deck.is_master)
            .map(|deck| deck.deck_id)
    }

    fn press(rig: &mut SimRig, commands: impl IntoIterator<Item = SimCommand>) {
        for command in commands {
            rig.apply(command);
        }
    }

    // ---- startup -------------------------------------------------------

    #[test]
    fn the_library_is_sorted_by_title_ignoring_case() {
        let titles: Vec<String> = rig()
            .view(REPORTED_AT)
            .library
            .into_iter()
            .map(|t| t.title)
            .collect();

        assert_eq!(titles, ["a", "B", "c", "D", "e"]);
    }

    #[test]
    fn tracks_with_the_same_title_are_ordered_by_artist() {
        let rig = SimRig::new(
            vec![
                titled("Same", Some("zed")),
                titled("Same", Some("Alpha")),
                titled("Same", None),
            ],
            None,
        );

        let artists: Vec<Option<String>> = rig
            .view(REPORTED_AT)
            .library
            .into_iter()
            .map(|t| t.artist)
            .collect();

        assert_eq!(
            artists,
            [None, Some("Alpha".to_owned()), Some("zed".to_owned())]
        );
    }

    #[test]
    fn startup_preloads_the_first_four_sorted_tracks_cued_with_closed_faders() {
        let view = rig().view(REPORTED_AT);

        assert_eq!(view.channel_faders, [0.0; 4]);
        for (index, deck) in view.decks.iter().enumerate() {
            assert_eq!(deck.track_index, Some(index));
            assert_eq!(deck.play_state, PlayState::Cued);
            assert!(!deck.is_on_air);
            assert!(!deck.is_master);
        }
    }

    #[test]
    fn startup_cues_each_deck_at_its_first_memory_cue() {
        let mut track = named_track("cued", 120.0);
        track.cues = vec![memory_cue(8_000.0)];
        let rig = SimRig::new(vec![track], None);

        let deck = state_of(&rig, 1);

        assert_eq!(deck.position_ms, 8_000.0);
        assert_eq!(deck.cue_point_ms, 8_000.0);
        assert_eq!(deck.play_state, PlayState::Cued);
    }

    #[test]
    fn a_library_of_fewer_than_four_tracks_leaves_the_other_decks_empty() {
        let rig = SimRig::new(
            vec![named_track("only", 120.0), named_track("two", 120.0)],
            None,
        );

        let states: Vec<PlayState> = rig
            .view(REPORTED_AT)
            .decks
            .iter()
            .map(|d| d.play_state)
            .collect();

        assert_eq!(
            states,
            [
                PlayState::Cued,
                PlayState::Cued,
                PlayState::Empty,
                PlayState::Empty
            ]
        );
        assert_eq!(rig.deck_states(REPORTED_AT)[3].playhead_ms, None);
    }

    #[test]
    fn the_fallback_warning_reaches_the_view() {
        let rig = SimRig::new(Vec::new(), Some("no USB".to_owned()));

        assert_eq!(
            rig.view(REPORTED_AT).fallback_warning.as_deref(),
            Some("no USB")
        );
    }

    // ---- loading -------------------------------------------------------

    #[test]
    fn loading_swaps_the_track_even_on_a_playing_deck() {
        let mut rig = rig();
        press(&mut rig, [SimCommand::PlayPause { deck: 1 }]);

        press(
            &mut rig,
            [SimCommand::Load {
                deck: 1,
                track_index: 4,
            }],
        );

        let deck = state_of(&rig, 1);
        assert_eq!(deck.track_index, Some(4));
        assert_eq!(deck.play_state, PlayState::Cued);
    }

    #[test]
    fn loading_a_track_outside_the_library_changes_nothing() {
        let mut rig = rig();

        press(
            &mut rig,
            [SimCommand::Load {
                deck: 1,
                track_index: 99,
            }],
        );

        assert_eq!(state_of(&rig, 1).track_index, Some(0));
    }

    #[test]
    fn a_command_for_a_deck_the_rig_lacks_is_ignored() {
        let mut rig = rig();

        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 9 },
                SimCommand::PlayPause { deck: 0 },
            ],
        );

        assert_eq!(master_of(&rig), None);
    }

    #[test]
    fn eject_is_ignored_while_playing_and_empties_a_stopped_deck() {
        let mut rig = rig();
        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 2 },
                SimCommand::Eject { deck: 2 },
            ],
        );
        assert_eq!(state_of(&rig, 2).play_state, PlayState::Playing);

        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 2 },
                SimCommand::Eject { deck: 2 },
            ],
        );

        let deck = state_of(&rig, 2);
        assert_eq!(deck.play_state, PlayState::Empty);
        assert_eq!(deck.track_index, None);
    }

    // ---- master --------------------------------------------------------

    #[test]
    fn nobody_is_master_until_a_deck_plays() {
        assert_eq!(master_of(&rig()), None);
    }

    #[test]
    fn the_first_deck_to_play_becomes_master_and_a_later_one_does_not() {
        let mut rig = rig();

        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 3 },
                SimCommand::PlayPause { deck: 1 },
            ],
        );

        assert_eq!(master_of(&rig), Some(3));
    }

    #[test]
    fn master_can_be_taken_explicitly() {
        let mut rig = rig();
        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 1 },
                SimCommand::Master { deck: 2 },
            ],
        );

        assert_eq!(master_of(&rig), Some(2));
    }

    #[test]
    fn an_empty_deck_cannot_take_master() {
        let mut rig = SimRig::new(vec![named_track("only", 120.0)], None);

        press(&mut rig, [SimCommand::Master { deck: 4 }]);

        assert_eq!(master_of(&rig), None);
    }

    #[test]
    fn a_stopped_master_stays_master_until_another_deck_takes_over() {
        let mut rig = rig();
        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 1 },
                SimCommand::PlayPause { deck: 1 },
                SimCommand::PlayPause { deck: 2 },
            ],
        );
        assert_eq!(master_of(&rig), Some(1));

        press(&mut rig, [SimCommand::Master { deck: 2 }]);
        assert_eq!(master_of(&rig), Some(2));
    }

    #[test]
    fn ejecting_the_master_clears_it_and_the_next_deck_to_play_takes_it() {
        let mut rig = rig();
        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 1 },
                SimCommand::PlayPause { deck: 1 },
                SimCommand::Eject { deck: 1 },
            ],
        );
        assert_eq!(master_of(&rig), None);

        press(&mut rig, [SimCommand::PlayPause { deck: 4 }]);
        assert_eq!(master_of(&rig), Some(4));
    }

    // ---- mixer ---------------------------------------------------------

    #[test]
    fn a_deck_is_on_air_whenever_its_fader_is_open_playing_or_not() {
        let mut rig = rig();

        press(
            &mut rig,
            [SimCommand::SetChannelFader {
                deck: 2,
                level: 0.3,
            }],
        );

        assert!(state_of(&rig, 2).is_on_air);
        assert_eq!(state_of(&rig, 2).play_state, PlayState::Cued);
        assert!(!state_of(&rig, 1).is_on_air);

        press(
            &mut rig,
            [SimCommand::SetChannelFader {
                deck: 2,
                level: 0.0,
            }],
        );
        assert!(!state_of(&rig, 2).is_on_air);
    }

    #[test]
    fn the_channel_fader_is_clamped() {
        let mut rig = rig();

        press(
            &mut rig,
            [
                SimCommand::SetChannelFader {
                    deck: 1,
                    level: 4.0,
                },
                SimCommand::SetChannelFader {
                    deck: 2,
                    level: -4.0,
                },
            ],
        );

        assert_eq!(rig.view(REPORTED_AT).channel_faders[..2], [1.0, 0.0]);
    }

    // ---- sync ----------------------------------------------------------

    fn rig_with_deck_one_playing_for(elapsed_ms: f64) -> SimRig {
        let mut rig = rig();
        press(&mut rig, [SimCommand::PlayPause { deck: 1 }]);
        rig.advance(elapsed_ms);
        rig
    }

    #[test]
    fn a_synced_deck_matches_the_master_s_effective_bpm() {
        let mut rig = rig_with_deck_one_playing_for(1_234.0);

        press(&mut rig, [SimCommand::Sync { deck: 2 }]);

        let master = state_of(&rig, 1);
        let follower = state_of(&rig, 2);
        assert!(follower.is_synced);
        assert_eq!(master.effective_bpm, Some(120.0));
        assert!((follower.effective_bpm.unwrap() - 120.0).abs() < 1e-9);
        assert_eq!(follower.track_bpm, Some(128.0));
    }

    #[test]
    fn a_synced_deck_follows_every_change_of_the_master_s_pitch() {
        let mut rig = rig_with_deck_one_playing_for(500.0);
        press(&mut rig, [SimCommand::Sync { deck: 2 }]);

        press(
            &mut rig,
            [
                SimCommand::SetTempoRange {
                    deck: 1,
                    range: TempoRange::Ten,
                },
                SimCommand::SetPitchFader {
                    deck: 1,
                    position: 0.5,
                },
            ],
        );

        assert!((state_of(&rig, 2).effective_bpm.unwrap() - 126.0).abs() < 1e-9);
        assert!((state_of(&rig, 2).effective_pitch - (126.0 / 128.0 - 1.0) * 100.0).abs() < 1e-9);
    }

    #[test]
    fn enabling_sync_lines_the_beats_up_with_the_master() {
        let mut rig = rig_with_deck_one_playing_for(1_234.0);
        let master_phase = rig.decks[0].beat_phase().unwrap();

        press(&mut rig, [SimCommand::Sync { deck: 2 }]);

        let follower_phase = rig.decks[1].beat_phase().unwrap();
        assert!((follower_phase - master_phase).abs() < 1e-6);
    }

    #[test]
    fn beats_stay_aligned_while_both_decks_play() {
        let mut rig = rig_with_deck_one_playing_for(1_234.0);
        press(
            &mut rig,
            [
                SimCommand::Sync { deck: 2 },
                SimCommand::PlayPause { deck: 2 },
            ],
        );

        for _ in 0..50 {
            rig.advance(40.0);
        }

        let gap = (rig.decks[0].beat_phase().unwrap() - rig.decks[1].beat_phase().unwrap()).abs();
        assert!(
            gap < 1e-6 || (1.0 - gap) < 1e-6,
            "phases drifted apart by {gap}"
        );
    }

    #[test]
    fn a_synced_deck_realigns_when_it_starts_playing() {
        let mut rig = rig_with_deck_one_playing_for(1_000.0);
        press(
            &mut rig,
            [
                SimCommand::Sync { deck: 2 },
                SimCommand::Seek {
                    deck: 2,
                    position_ms: 10_100.0,
                },
            ],
        );
        rig.advance(173.0);

        press(&mut rig, [SimCommand::PlayPause { deck: 2 }]);

        let gap = (rig.decks[0].beat_phase().unwrap() - rig.decks[1].beat_phase().unwrap()).abs();
        assert!(gap < 1e-6 || (1.0 - gap) < 1e-6, "phases differ by {gap}");
    }

    #[test]
    fn phase_alignment_moves_the_deck_by_no_more_than_half_a_beat() {
        let mut rig = rig_with_deck_one_playing_for(1_234.0);
        let before = state_of(&rig, 2).position_ms;

        press(&mut rig, [SimCommand::Sync { deck: 2 }]);

        let beat_ms = 60_000.0 / 128.0;
        assert!((state_of(&rig, 2).position_ms - before).abs() <= beat_ms / 2.0 + 1e-6);
    }

    #[test]
    fn turning_sync_off_returns_the_deck_to_its_fader_pitch() {
        let mut rig = rig_with_deck_one_playing_for(500.0);
        press(
            &mut rig,
            [
                SimCommand::SetPitchFader {
                    deck: 2,
                    position: 0.2,
                },
                SimCommand::Sync { deck: 2 },
            ],
        );
        assert!((state_of(&rig, 2).effective_bpm.unwrap() - 120.0).abs() < 1e-9);

        press(&mut rig, [SimCommand::Sync { deck: 2 }]);

        let deck = state_of(&rig, 2);
        assert!(!deck.is_synced);
        assert!((deck.effective_pitch - 2.0).abs() < 1e-9);
        assert_eq!(deck.pitch_fader, 0.2);
    }

    #[test]
    fn a_synced_deck_keeps_its_fader_position_but_ignores_it() {
        let mut rig = rig_with_deck_one_playing_for(500.0);
        press(
            &mut rig,
            [
                SimCommand::Sync { deck: 2 },
                SimCommand::SetPitchFader {
                    deck: 2,
                    position: -1.0,
                },
            ],
        );

        let deck = state_of(&rig, 2);

        assert_eq!(deck.pitch_fader, -1.0);
        assert!((deck.effective_bpm.unwrap() - 120.0).abs() < 1e-9);
    }

    #[test]
    fn sync_without_a_master_leaves_the_deck_on_its_fader() {
        let mut rig = rig();

        press(
            &mut rig,
            [
                SimCommand::Sync { deck: 2 },
                SimCommand::SetPitchFader {
                    deck: 2,
                    position: 0.5,
                },
            ],
        );

        let deck = state_of(&rig, 2);
        assert!(deck.is_synced);
        assert!((deck.effective_pitch - 5.0).abs() < 1e-9);
    }

    #[test]
    fn the_master_itself_keeps_its_own_fader_even_with_sync_on() {
        let mut rig = rig_with_deck_one_playing_for(500.0);

        press(
            &mut rig,
            [
                SimCommand::Sync { deck: 1 },
                SimCommand::SetPitchFader {
                    deck: 1,
                    position: 0.5,
                },
            ],
        );

        assert!((state_of(&rig, 1).effective_pitch - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_deck_that_was_synced_becomes_master_on_its_own_tempo() {
        let mut rig = rig_with_deck_one_playing_for(500.0);
        press(
            &mut rig,
            [SimCommand::Sync { deck: 2 }, SimCommand::Master { deck: 2 }],
        );

        let deck = state_of(&rig, 2);

        assert!(deck.is_master);
        assert!((deck.effective_bpm.unwrap() - 128.0).abs() < 1e-9);
    }

    // ---- app-facing state ------------------------------------------------

    #[test]
    fn deck_states_describe_each_deck_for_the_rest_of_the_app() {
        let mut rig = rig();
        press(
            &mut rig,
            [
                SimCommand::PlayPause { deck: 1 },
                SimCommand::SetChannelFader {
                    deck: 1,
                    level: 1.0,
                },
                SimCommand::SetPitchFader {
                    deck: 1,
                    position: 1.0,
                },
            ],
        );
        rig.advance(1_000.0);

        let states = rig.deck_states(REPORTED_AT);

        assert_eq!(states.len(), 4);
        let first = &states[0];
        assert_eq!(first.deck_id, 1);
        assert_eq!(first.play_state, PlayState::Playing);
        assert!(first.is_master && first.is_on_air && !first.is_synced);
        assert!((first.pitch - 10.0).abs() < 1e-9);
        assert_eq!(first.track_bpm, Some(120.0));
        assert!((first.effective_bpm.unwrap() - 132.0).abs() < 1e-9);
        assert!(first.beat_in_measure.is_some());
        assert!((first.playhead_ms.unwrap() - 1_100.0).abs() < 1e-6);
        assert_eq!(first.reported_at, REPORTED_AT);
        assert_eq!(states[1].beat_in_measure, None);
    }
}
