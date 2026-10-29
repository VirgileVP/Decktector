//! One simulated CDJ-3000, as pure physics.
//!
//! A [`SimDeckModel`] knows what a player does when its buttons are pressed
//! and time passes, and nothing else: no clock, no I/O, no randomness. Every
//! time-dependent method takes the elapsed time as an argument, so a test
//! can play a deck for exactly 2.5 s without waiting. What needs more than
//! one deck — the master, sync — lives in [`super::rig::SimRig`], which feeds
//! this model the few facts it cannot know alone (see
//! [`SimDeckModel::set_sync_pitch`]).

use std::sync::Arc;

use super::beat::{BeatMap, BEATS_PER_BAR};
use super::command::TempoRange;
use super::rig_state::{SimDeckView, SimLoop};
use crate::model::{CueKind, DeckId, PlayState, TrackInfo};

/// Pitch bend, in percent, at full travel of the jog's edge.
const MAX_NUDGE_PERCENT: f64 = 4.0;
/// How close to the cue point a paused deck must sit to read as cued.
const CUE_MATCH_TOLERANCE_MS: f64 = 0.5;
/// The loop lengths the loop buttons offer.
const LOOP_BEAT_COUNTS: [u8; 2] = [4, 8];

struct LoadedTrack {
    /// Index into the rig's sorted library.
    index: usize,
    track: Arc<TrackInfo>,
}

pub(super) struct SimDeckModel {
    deck_id: DeckId,
    loaded: Option<LoadedTrack>,
    position_ms: f64,
    cue_point_ms: f64,
    playing: bool,
    /// The jog top is touched: the platter is held and nothing advances.
    scratching: bool,
    /// Pitch fader travel, -1 to 1.
    pitch_fader: f64,
    tempo_range: TempoRange,
    /// Jog edge drag, -1 to 1.
    nudge_bend: f64,
    synced: bool,
    /// Pitch the rig derived from the master's tempo while this deck is
    /// synced to it; replaces the fader's pitch when present.
    sync_pitch: Option<f64>,
    loop_state: Option<SimLoop>,
}

impl SimDeckModel {
    pub fn empty(deck_id: DeckId) -> Self {
        Self {
            deck_id,
            loaded: None,
            position_ms: 0.0,
            cue_point_ms: 0.0,
            playing: false,
            scratching: false,
            pitch_fader: 0.0,
            tempo_range: TempoRange::default(),
            nudge_bend: 0.0,
            synced: false,
            sync_pitch: None,
            loop_state: None,
        }
    }

    // ---- queries -------------------------------------------------------

    pub fn deck_id(&self) -> DeckId {
        self.deck_id
    }

    pub fn track(&self) -> Option<&TrackInfo> {
        self.loaded.as_ref().map(|loaded| &*loaded.track)
    }

    pub fn track_index(&self) -> Option<usize> {
        self.loaded.as_ref().map(|loaded| loaded.index)
    }

    pub fn is_synced(&self) -> bool {
        self.synced
    }

    pub fn play_state(&self) -> PlayState {
        let Some(track) = self.track() else {
            return PlayState::Empty;
        };
        if self.scratching {
            PlayState::Searching
        } else if self.position_ms >= track.duration_ms {
            PlayState::Ended
        } else if self.playing {
            PlayState::Playing
        } else if (self.position_ms - self.cue_point_ms).abs() <= CUE_MATCH_TOLERANCE_MS {
            PlayState::Cued
        } else {
            PlayState::Paused
        }
    }

    pub fn track_bpm(&self) -> Option<f64> {
        self.track().and_then(|track| track.bpm)
    }

    /// Pitch actually applied, in percent.
    pub fn effective_pitch(&self) -> f64 {
        let base = self.sync_pitch.unwrap_or_else(|| self.fader_pitch());
        base + self.nudge_percent()
    }

    pub fn effective_bpm(&self) -> Option<f64> {
        self.track_bpm()
            .map(|bpm| bpm * (1.0 + self.effective_pitch() / 100.0))
    }

    /// Fraction of the current beat the playhead has covered.
    pub fn beat_phase(&self) -> Option<f64> {
        let beat = BeatMap::of(self.track()?).beat_at(self.position_ms)?;
        Some(beat.phase_at(self.position_ms))
    }

    pub fn view(&self, is_master: bool, is_on_air: bool) -> SimDeckView {
        let play_state = self.play_state();
        SimDeckView {
            deck_id: self.deck_id,
            track_index: self.track_index(),
            track_id: self.track().map(|track| track.track_id),
            play_state,
            position_ms: self.position_ms,
            duration_ms: self.duration_ms(),
            cue_point_ms: self.cue_point_ms,
            pitch_fader: self.pitch_fader,
            tempo_range: self.tempo_range,
            effective_pitch: self.effective_pitch(),
            track_bpm: self.track_bpm(),
            effective_bpm: self.effective_bpm(),
            beat_in_measure: self.beat_in_measure_while(play_state == PlayState::Playing),
            loop_state: self.loop_state,
            is_master,
            is_synced: self.synced,
            is_on_air,
            scratching: self.scratching,
        }
    }

    // ---- transport -----------------------------------------------------

    /// Replaces whatever is loaded; the deck waits at the track's first
    /// memory cue, or its start.
    pub fn load(&mut self, index: usize, track: Arc<TrackInfo>) {
        self.cue_point_ms = first_memory_cue_ms(&track);
        self.position_ms = self.cue_point_ms;
        self.loaded = Some(LoadedTrack { index, track });
        self.reset_playback();
    }

    /// Empties the deck. Refused, returning `false`, while it plays or when
    /// there is nothing to eject.
    pub fn eject(&mut self) -> bool {
        if self.playing || self.loaded.is_none() {
            return false;
        }
        self.loaded = None;
        self.position_ms = 0.0;
        self.cue_point_ms = 0.0;
        self.reset_playback();
        true
    }

    /// Toggles play/pause, returning `true` when the deck just started
    /// playing. A deck sitting at the end of its track starts over from its
    /// cue point.
    pub fn toggle_play(&mut self) -> bool {
        let Some(duration_ms) = self.duration_ms() else {
            return false;
        };
        if self.playing {
            self.playing = false;
            return false;
        }
        if self.position_ms >= duration_ms {
            self.position_ms = if self.cue_point_ms < duration_ms {
                self.cue_point_ms
            } else {
                0.0
            };
        }
        self.playing = true;
        true
    }

    /// The CUE button: set the cue point while paused, return to it and stop
    /// while playing. There is no held-cue preview.
    pub fn cue(&mut self) {
        if self.loaded.is_none() {
            return;
        }
        if self.playing {
            self.position_ms = self.cue_point_ms;
            self.playing = false;
        } else {
            self.cue_point_ms = self.position_ms;
        }
    }

    pub fn seek(&mut self, position_ms: f64) {
        if let Some(duration_ms) = self.duration_ms() {
            self.position_ms = position_ms.clamp(0.0, duration_ms);
        }
    }

    /// Moves the playhead forward by `elapsed_ms` of real time.
    pub fn advance(&mut self, elapsed_ms: f64) {
        let Some(duration_ms) = self.duration_ms() else {
            return;
        };
        if !self.playing || self.scratching {
            return;
        }

        let next_ms = self.position_ms + elapsed_ms * self.rate();
        self.position_ms = match self.loop_state.filter(|looped| looped.active) {
            Some(looped) if self.position_ms < looped.end_ms && next_ms >= looped.end_ms => {
                wrap_into_loop(next_ms, looped)
            }
            _ => next_ms,
        };
        if self.position_ms >= duration_ms {
            self.position_ms = duration_ms;
            self.playing = false;
        }
    }

    // ---- pitch ---------------------------------------------------------

    pub fn set_pitch_fader(&mut self, position: f64) {
        self.pitch_fader = position.clamp(-1.0, 1.0);
    }

    /// The fader keeps its position; only what that position means changes.
    pub fn set_tempo_range(&mut self, range: TempoRange) {
        self.tempo_range = range;
    }

    pub fn set_nudge(&mut self, bend: f64) {
        self.nudge_bend = bend.clamp(-1.0, 1.0);
    }

    // ---- jog -----------------------------------------------------------

    pub fn set_jog_touched(&mut self, touched: bool) {
        if self.loaded.is_some() {
            self.scratching = touched;
        }
    }

    /// Moves the held platter; ignored unless the jog top is touched.
    pub fn scratch(&mut self, delta_ms: f64) {
        if self.scratching {
            self.seek(self.position_ms + delta_ms);
        }
    }

    // ---- cues and loops ------------------------------------------------

    /// Jumps to hot cue `button` without changing play/pause. A hot loop
    /// also starts looping.
    pub fn hot_cue(&mut self, button: &str) {
        let Some(track) = self.track() else {
            return;
        };
        let Some(cue) = track.cues.iter().find(|cue| {
            matches!(cue.kind, CueKind::Hotcue | CueKind::Hotloop)
                && cue
                    .button
                    .as_deref()
                    .is_some_and(|label| label.eq_ignore_ascii_case(button))
        }) else {
            return;
        };
        let (offset_ms, length_ms) = (cue.offset_ms, cue.length_ms);

        self.seek(offset_ms);
        if let Some(length_ms) = length_ms.filter(|length| *length > 0.0) {
            self.loop_state = Some(SimLoop {
                start_ms: offset_ms,
                end_ms: offset_ms + length_ms,
                active: true,
            });
        }
    }

    /// Starts a loop of `beats` beats from the beat the playhead is in.
    pub fn start_loop(&mut self, beats: u8) {
        if !LOOP_BEAT_COUNTS.contains(&beats) {
            return;
        }
        if let Some(looped) = self.snapped_loop(beats) {
            self.loop_state = Some(looped);
        }
    }

    /// Leaves an active loop, or re-enters a stored one from its start.
    pub fn reloop_exit(&mut self) {
        match self.loop_state {
            Some(looped) if looped.active => {
                self.loop_state = Some(SimLoop {
                    active: false,
                    ..looped
                });
            }
            Some(looped) => {
                self.seek(looped.start_ms);
                self.loop_state = Some(SimLoop {
                    active: true,
                    ..looped
                });
            }
            None => {}
        }
    }

    // ---- sync ----------------------------------------------------------

    pub fn set_synced(&mut self, synced: bool) {
        self.synced = synced;
    }

    /// The pitch that makes this deck's tempo match the master's, or `None`
    /// to fall back on the fader. Set by the rig, which alone sees the
    /// master.
    pub fn set_sync_pitch(&mut self, pitch: Option<f64>) {
        self.sync_pitch = pitch;
    }

    /// Slides the playhead by the least distance (at most half a beat) that
    /// puts it at `target_phase` of its beat.
    pub fn align_beat_phase(&mut self, target_phase: f64) {
        let Some(beat) = self
            .track()
            .and_then(|track| BeatMap::of(track).beat_at(self.position_ms))
        else {
            return;
        };
        let phase_error = target_phase - beat.phase_at(self.position_ms);
        let shortest_error = phase_error - phase_error.round();
        self.seek(self.position_ms + shortest_error * beat.length_ms);
    }

    // ---- internals -----------------------------------------------------

    fn duration_ms(&self) -> Option<f64> {
        self.track().map(|track| track.duration_ms)
    }

    fn fader_pitch(&self) -> f64 {
        self.pitch_fader * self.tempo_range.percent()
    }

    /// The jog edge only bends a deck that is playing.
    fn nudge_percent(&self) -> f64 {
        if self.playing {
            self.nudge_bend * MAX_NUDGE_PERCENT
        } else {
            0.0
        }
    }

    /// Playback speed relative to the track's own, never negative.
    fn rate(&self) -> f64 {
        (1.0 + self.effective_pitch() / 100.0).max(0.0)
    }

    /// What a freshly loaded or emptied deck forgets.
    fn reset_playback(&mut self) {
        self.playing = false;
        self.scratching = false;
        self.nudge_bend = 0.0;
        self.loop_state = None;
    }

    fn beat_in_measure_while(&self, playing: bool) -> Option<u8> {
        if !playing {
            return None;
        }
        let beat = BeatMap::of(self.track()?).beat_at(self.position_ms)?;
        Some(beat.beat_in_measure.clamp(1, BEATS_PER_BAR))
    }

    fn snapped_loop(&self, beats: u8) -> Option<SimLoop> {
        let track = self.track()?;
        let map = BeatMap::of(track);
        let start_ms = map.beat_at(self.position_ms)?.start_ms;
        let end_ms = map.beat_start_after(start_ms, u32::from(beats))?;
        (end_ms <= track.duration_ms).then_some(SimLoop {
            start_ms,
            end_ms,
            active: true,
        })
    }
}

fn first_memory_cue_ms(track: &TrackInfo) -> f64 {
    track
        .cues
        .iter()
        .filter(|cue| cue.kind == CueKind::Memory)
        .map(|cue| cue.offset_ms)
        .min_by(f64::total_cmp)
        .unwrap_or(0.0)
}

/// Where a playhead that ran past the end of `looped` lands: the same
/// distance beyond the start, however many times it overshot.
fn wrap_into_loop(position_ms: f64, looped: SimLoop) -> f64 {
    let length_ms = looped.end_ms - looped.start_ms;
    looped.start_ms + (position_ms - looped.end_ms) % length_ms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::test_support::{hot_cue, memory_cue, track_with_grid};

    const BPM: f64 = 120.0;
    const BEAT_MS: f64 = 500.0;

    fn loaded_deck() -> SimDeckModel {
        deck_with(track_with_grid(BPM, 400))
    }

    fn deck_with(track: TrackInfo) -> SimDeckModel {
        let mut deck = SimDeckModel::empty(1);
        deck.load(0, Arc::new(track));
        deck
    }

    fn playing_deck() -> SimDeckModel {
        let mut deck = loaded_deck();
        deck.toggle_play();
        deck
    }

    #[test]
    fn a_new_deck_is_empty_and_ignores_play() {
        let mut deck = SimDeckModel::empty(1);

        assert!(!deck.toggle_play());
        assert_eq!(deck.play_state(), PlayState::Empty);
        assert_eq!(deck.view(false, false).position_ms, 0.0);
    }

    #[test]
    fn loading_waits_at_the_first_memory_cue_by_offset() {
        let mut track = track_with_grid(BPM, 400);
        track.cues = vec![memory_cue(9_000.0), memory_cue(4_000.0)];
        let deck = deck_with(track);

        assert_eq!(deck.play_state(), PlayState::Cued);
        assert_eq!(deck.view(false, false).position_ms, 4_000.0);
        assert_eq!(deck.view(false, false).cue_point_ms, 4_000.0);
    }

    #[test]
    fn loading_replaces_a_playing_track_and_stops_it() {
        let mut deck = playing_deck();
        deck.start_loop(4);

        deck.load(3, Arc::new(track_with_grid(BPM, 400)));

        assert_eq!(deck.play_state(), PlayState::Cued);
        assert_eq!(deck.track_index(), Some(3));
        assert_eq!(deck.view(false, false).loop_state, None);
    }

    #[test]
    fn eject_is_ignored_while_playing_and_empties_a_stopped_deck() {
        let mut deck = playing_deck();
        assert!(!deck.eject());
        assert_eq!(deck.play_state(), PlayState::Playing);

        deck.toggle_play();
        assert!(deck.eject());
        assert_eq!(deck.play_state(), PlayState::Empty);
    }

    #[test]
    fn play_pause_toggles_and_time_only_moves_a_playing_deck() {
        let mut deck = loaded_deck();
        deck.advance(1_000.0);
        assert_eq!(deck.view(false, false).position_ms, 0.0);

        assert!(deck.toggle_play());
        deck.advance(1_000.0);
        assert_eq!(deck.view(false, false).position_ms, 1_000.0);

        assert!(!deck.toggle_play());
        deck.advance(1_000.0);
        assert_eq!(deck.view(false, false).position_ms, 1_000.0);
        assert_eq!(deck.play_state(), PlayState::Paused);
    }

    #[test]
    fn reaching_the_end_stops_the_deck_clamped_and_ended() {
        let mut deck = playing_deck();
        let duration_ms = deck.duration_ms().unwrap();
        deck.seek(duration_ms - 10.0);

        deck.advance(1_000.0);

        assert_eq!(deck.play_state(), PlayState::Ended);
        assert_eq!(deck.view(false, false).position_ms, duration_ms);
    }

    #[test]
    fn playing_from_the_end_starts_over_at_the_cue_point() {
        let mut deck = loaded_deck();
        deck.seek(f64::MAX);

        deck.toggle_play();

        assert_eq!(deck.play_state(), PlayState::Playing);
        assert_eq!(deck.view(false, false).position_ms, 0.0);
    }

    #[test]
    fn cue_while_paused_sets_the_cue_point_and_stays_cued() {
        let mut deck = loaded_deck();
        deck.seek(7_000.0);
        assert_eq!(deck.play_state(), PlayState::Paused);

        deck.cue();

        assert_eq!(deck.view(false, false).cue_point_ms, 7_000.0);
        assert_eq!(deck.play_state(), PlayState::Cued);
    }

    #[test]
    fn cue_while_playing_returns_to_the_cue_point_and_pauses() {
        let mut deck = loaded_deck();
        deck.seek(2_000.0);
        deck.cue();
        deck.toggle_play();
        deck.advance(5_000.0);

        deck.cue();

        assert_eq!(deck.view(false, false).position_ms, 2_000.0);
        assert_eq!(deck.play_state(), PlayState::Cued);
    }

    #[test]
    fn seek_is_clamped_to_the_track() {
        let mut deck = loaded_deck();
        let duration_ms = deck.duration_ms().unwrap();

        deck.seek(-5.0);
        assert_eq!(deck.view(false, false).position_ms, 0.0);
        deck.seek(duration_ms + 5.0);
        assert_eq!(deck.view(false, false).position_ms, duration_ms);
    }

    #[test]
    fn a_hot_cue_jumps_and_keeps_the_play_state() {
        let mut track = track_with_grid(BPM, 400);
        track.cues = vec![hot_cue("B", 12_000.0, None)];
        let mut paused = deck_with(track.clone());
        let mut playing = deck_with(track);
        playing.toggle_play();

        paused.hot_cue("B");
        playing.hot_cue("b");

        assert_eq!(paused.view(false, false).position_ms, 12_000.0);
        assert_eq!(paused.play_state(), PlayState::Paused);
        assert_eq!(playing.view(false, false).position_ms, 12_000.0);
        assert_eq!(playing.play_state(), PlayState::Playing);
    }

    #[test]
    fn an_unknown_hot_cue_button_does_nothing() {
        let mut deck = loaded_deck();

        deck.hot_cue("H");

        assert_eq!(deck.view(false, false).position_ms, 0.0);
    }

    #[test]
    fn a_hot_loop_jumps_to_its_start_and_arms_the_loop() {
        let mut track = track_with_grid(BPM, 400);
        track.cues = vec![hot_cue("C", 8_000.0, Some(2_000.0))];
        let mut deck = deck_with(track);

        deck.hot_cue("C");

        assert_eq!(
            deck.view(false, false).loop_state,
            Some(SimLoop {
                start_ms: 8_000.0,
                end_ms: 10_000.0,
                active: true
            })
        );
    }

    #[test]
    fn the_tempo_range_scales_the_fader_without_moving_it() {
        let mut deck = loaded_deck();
        deck.set_pitch_fader(0.5);
        assert_eq!(deck.effective_pitch(), 5.0);

        deck.set_tempo_range(TempoRange::Sixteen);
        assert_eq!(deck.view(false, false).pitch_fader, 0.5);
        assert_eq!(deck.effective_pitch(), 8.0);

        deck.set_tempo_range(TempoRange::Wide);
        deck.set_pitch_fader(-1.0);
        assert_eq!(deck.effective_pitch(), -100.0);
    }

    #[test]
    fn the_fader_position_is_clamped() {
        let mut deck = loaded_deck();

        deck.set_pitch_fader(7.0);

        assert_eq!(deck.view(false, false).pitch_fader, 1.0);
    }

    #[test]
    fn pitch_changes_the_effective_bpm_and_the_playback_speed() {
        let mut deck = playing_deck();
        deck.set_pitch_fader(1.0);
        deck.set_tempo_range(TempoRange::Six);

        deck.advance(1_000.0);

        assert!((deck.effective_bpm().unwrap() - BPM * 1.06).abs() < 1e-9);
        assert!((deck.view(false, false).position_ms - 1_060.0).abs() < 1e-9);
    }

    #[test]
    fn a_wide_range_at_the_bottom_stops_the_deck_rather_than_reversing_it() {
        let mut deck = playing_deck();
        deck.set_tempo_range(TempoRange::Wide);
        deck.set_pitch_fader(-1.0);

        deck.advance(1_000.0);

        assert_eq!(deck.view(false, false).position_ms, 0.0);
    }

    #[test]
    fn nudging_bends_the_pitch_up_and_releasing_restores_it() {
        let mut deck = playing_deck();

        deck.set_nudge(1.0);
        assert_eq!(deck.effective_pitch(), MAX_NUDGE_PERCENT);
        deck.advance(1_000.0);
        assert!(deck.view(false, false).position_ms > 1_000.0);

        deck.set_nudge(0.0);
        assert_eq!(deck.effective_pitch(), 0.0);
    }

    #[test]
    fn nudging_a_paused_deck_changes_nothing() {
        let mut deck = loaded_deck();

        deck.set_nudge(-1.0);

        assert_eq!(deck.effective_pitch(), 0.0);
    }

    #[test]
    fn a_touched_jog_holds_the_deck_and_scratching_follows_the_deltas() {
        let mut deck = playing_deck();
        deck.advance(4_000.0);

        deck.set_jog_touched(true);
        assert_eq!(deck.play_state(), PlayState::Searching);
        deck.advance(1_000.0);
        assert_eq!(deck.view(false, false).position_ms, 4_000.0);

        deck.scratch(-300.0);
        deck.scratch(120.0);
        assert_eq!(deck.view(false, false).position_ms, 3_820.0);
        deck.scratch(-1.0e9);
        assert_eq!(deck.view(false, false).position_ms, 0.0);
    }

    #[test]
    fn releasing_the_jog_resumes_a_deck_that_was_playing() {
        let mut deck = playing_deck();
        deck.set_jog_touched(true);
        deck.scratch(500.0);

        deck.set_jog_touched(false);
        deck.advance(1_000.0);

        assert_eq!(deck.play_state(), PlayState::Playing);
        assert_eq!(deck.view(false, false).position_ms, 1_500.0);
    }

    #[test]
    fn releasing_the_jog_leaves_a_paused_deck_paused() {
        let mut deck = loaded_deck();
        deck.set_jog_touched(true);
        deck.scratch(2_000.0);

        deck.set_jog_touched(false);
        deck.advance(1_000.0);

        assert_eq!(deck.play_state(), PlayState::Paused);
        assert_eq!(deck.view(false, false).position_ms, 2_000.0);
    }

    #[test]
    fn scratching_without_touching_the_jog_does_nothing() {
        let mut deck = loaded_deck();

        deck.scratch(2_000.0);

        assert_eq!(deck.view(false, false).position_ms, 0.0);
    }

    #[test]
    fn a_loop_snaps_to_the_beat_grid_for_four_and_eight_beats() {
        let mut deck = loaded_deck();
        deck.seek(3_300.0);

        deck.start_loop(4);
        assert_eq!(
            deck.view(false, false).loop_state,
            Some(SimLoop {
                start_ms: 3_000.0,
                end_ms: 5_000.0,
                active: true
            })
        );

        deck.start_loop(8);
        assert_eq!(deck.view(false, false).loop_state.unwrap().end_ms, 7_000.0);
    }

    #[test]
    fn a_loop_of_any_other_length_is_refused() {
        let mut deck = loaded_deck();

        deck.start_loop(5);

        assert_eq!(deck.view(false, false).loop_state, None);
    }

    #[test]
    fn a_loop_without_a_beat_grid_uses_the_bpm() {
        let mut track = track_with_grid(BPM, 400);
        track.beat_grid = None;
        let mut deck = deck_with(track);
        deck.seek(1_100.0);

        deck.start_loop(4);

        let looped = deck.view(false, false).loop_state.unwrap();
        assert_eq!((looped.start_ms, looped.end_ms), (1_000.0, 3_000.0));
    }

    #[test]
    fn a_loop_without_any_tempo_information_is_refused() {
        let mut track = track_with_grid(BPM, 400);
        track.beat_grid = None;
        track.bpm = None;
        let mut deck = deck_with(track);

        deck.start_loop(4);

        assert_eq!(deck.view(false, false).loop_state, None);
    }

    #[test]
    fn an_active_loop_wraps_the_playhead_keeping_the_overshoot() {
        let mut deck = loaded_deck();
        deck.seek(2_000.0);
        deck.start_loop(4);
        deck.toggle_play();

        deck.advance(2_100.0);

        assert_eq!(deck.view(false, false).position_ms, 2_100.0);
        assert_eq!(deck.play_state(), PlayState::Playing);
    }

    #[test]
    fn exiting_a_loop_lets_playback_run_past_its_end() {
        let mut deck = loaded_deck();
        deck.start_loop(4);
        deck.toggle_play();

        deck.reloop_exit();
        deck.advance(2_500.0);

        assert_eq!(deck.view(false, false).position_ms, 2_500.0);
        assert!(!deck.view(false, false).loop_state.unwrap().active);
    }

    #[test]
    fn reloop_returns_to_the_stored_loop_start_and_keeps_the_play_state() {
        let mut deck = loaded_deck();
        deck.start_loop(4);
        deck.toggle_play();
        deck.reloop_exit();
        deck.advance(5_000.0);

        deck.reloop_exit();

        let view = deck.view(false, false);
        assert_eq!(view.position_ms, 0.0);
        assert!(view.loop_state.unwrap().active);
        assert_eq!(view.play_state, PlayState::Playing);
    }

    #[test]
    fn reloop_with_no_stored_loop_does_nothing() {
        let mut deck = playing_deck();
        deck.advance(1_000.0);

        deck.reloop_exit();

        assert_eq!(deck.view(false, false).position_ms, 1_000.0);
    }

    #[test]
    fn beat_phase_alignment_takes_the_shortest_way() {
        let mut deck = loaded_deck();
        deck.seek(1_000.0 + BEAT_MS * 0.9);

        deck.align_beat_phase(0.1);

        // 0.9 -> 0.1 is a 0.2 beat step forward, not 0.8 back.
        assert!((deck.view(false, false).position_ms - (1_500.0 + BEAT_MS * 0.1)).abs() < 1e-6);
    }

    #[test]
    fn the_beat_in_the_bar_is_reported_only_while_playing() {
        let mut deck = loaded_deck();
        deck.seek(1_600.0);
        assert_eq!(deck.view(false, false).beat_in_measure, None);

        deck.toggle_play();

        assert_eq!(deck.view(false, false).beat_in_measure, Some(4));
    }
}
