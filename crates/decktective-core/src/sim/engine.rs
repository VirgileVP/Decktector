//! Scripted engine for developing the UI without Pro DJ Link hardware:
//! decks that play, drift in pitch, chain generated tracks, and occasionally
//! behave like a DJ.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration, Instant, MissedTickBehavior};

use super::tracks::{build_sim_track, SIM_TRACKS};
use crate::engine::{Engine, EngineHandle};
use crate::model::{DeckId, DeckState, DeviceInfo, NetworkState, PlayState, TrackInfo};

const TICK: Duration = Duration::from_millis(200);
/// Time before the end of a track at which the sim deck loads the next one.
const TRACK_SWAP_MARGIN: Duration = Duration::from_secs(8);
const TRACK_LOAD_DELAY: Duration = Duration::from_millis(1500);
const CUE_TO_PLAY_DELAY: Duration = Duration::from_millis(2500);
/// Delay before the simulated network reports itself connected.
const CONNECT_DELAY: Duration = Duration::from_millis(800);
/// Decks join the mix one after another rather than all at once.
const DECK_START_STAGGER: Duration = Duration::from_secs(5);

/// Per-tick probabilities of spontaneous DJ actions on a playing deck.
const PAUSE_ODDS: f64 = 0.004;
const CUE_JUMP_ODDS: f64 = 0.006;
const SCRATCH_ODDS: f64 = 0.004;

const PAUSE_MIN: Duration = Duration::from_millis(1000);
const PAUSE_MAX: Duration = Duration::from_millis(4000);
const SCRATCH: Duration = Duration::from_millis(800);
/// How far a scratch can land either side of where it started.
const SCRATCH_SPREAD_MS: f64 = 4000.0;
/// Full period of the sinusoidal pitch drift, in seconds.
const PITCH_DRIFT_PERIOD_SEC: f64 = 30.0;
/// Amplitude of the pitch drift, in percent.
const PITCH_DRIFT_PERCENT: f64 = 3.0;
const BEATS_PER_BAR: u8 = 4;
/// Pro DJ Link networks top out at 6 players.
const MAX_DECKS: u8 = 6;
/// First track id handed out; real player track ids are unrelated small ints.
const FIRST_TRACK_ID: u32 = 100;

/// Something a deck will do once its deadline passes.
///
/// The TypeScript engine nested `setTimeout` calls for these; a deadline
/// checked on each tick keeps the whole deck lifecycle readable in one place.
#[derive(Debug, Clone, Copy)]
enum Scheduled {
    /// Resume playing after a pause.
    Resume,
    /// Land the playhead near `origin_ms` and resume, ending a scratch.
    LandScratch { origin_ms: f64 },
    /// Unload the current track and show the deck as loading.
    UnloadTrack,
    /// Load the next track and cue it.
    LoadNextTrack,
    /// Leave the cue point and play.
    StartPlaying,
}

struct SimDeck {
    deck_id: DeckId,
    play_state: PlayState,
    track: Option<TrackInfo>,
    playhead_ms: f64,
    /// Index in `SIM_TRACKS` of the currently loaded track.
    track_index: usize,
    pitch_phase: f64,
    swapping: bool,
    /// True while a random action temporarily owns the play state.
    busy: bool,
    scheduled: Option<(Instant, Scheduled)>,
}

impl SimDeck {
    fn new(deck_id: DeckId, track_index: usize, track_id: u32) -> Self {
        Self {
            deck_id,
            play_state: PlayState::Cued,
            track: Some(build_sim_track(&SIM_TRACKS[track_index], track_id)),
            playhead_ms: 0.0,
            track_index,
            pitch_phase: f64::from(deck_id) * 2.1,
            swapping: false,
            busy: false,
            scheduled: None,
        }
    }

    fn schedule(&mut self, delay: Duration, action: Scheduled) {
        self.scheduled = Some((Instant::now() + delay, action));
    }

    /// Returns the action whose deadline has passed, clearing it.
    fn take_due(&mut self, now: Instant) -> Option<Scheduled> {
        match self.scheduled {
            Some((at, action)) if now >= at => {
                self.scheduled = None;
                Some(action)
            }
            _ => None,
        }
    }

    /// Slow sinusoidal pitch drift, unique phase per deck.
    fn advance_pitch(&mut self) -> f64 {
        self.pitch_phase += TICK.as_secs_f64() / PITCH_DRIFT_PERIOD_SEC;
        self.pitch_phase.sin() * PITCH_DRIFT_PERCENT
    }

    fn resume(&mut self) {
        if !self.swapping && self.track.is_some() {
            self.play_state = PlayState::Playing;
        }
        self.busy = false;
    }

    fn to_deck_state(&self, pitch: f64, master_deck: DeckId) -> DeckState {
        let bpm = self.track.as_ref().and_then(|track| track.bpm);
        let playing = self.play_state == PlayState::Playing;
        let beat_ms = bpm.map(|bpm| 60_000.0 / bpm);

        DeckState {
            deck_id: self.deck_id,
            play_state: self.play_state,
            is_master: master_deck == self.deck_id && playing,
            is_synced: master_deck != self.deck_id && playing,
            is_on_air: playing,
            pitch,
            track_bpm: bpm,
            effective_bpm: bpm.map(|bpm| bpm * (1.0 + pitch / 100.0)),
            beat_in_measure: match beat_ms {
                Some(beat_ms) if playing => {
                    Some((self.playhead_ms / beat_ms) as u64 as u8 % BEATS_PER_BAR + 1)
                }
                _ => None,
            },
            track_id: self.track.as_ref().map(|track| track.track_id),
            playhead_ms: self.track.as_ref().map(|_| self.playhead_ms),
            reported_at: epoch_ms(),
        }
    }
}

pub struct SimEngine {
    handle: Arc<EngineHandle>,
    deck_count: u8,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl SimEngine {
    pub fn new(deck_count: u8) -> Self {
        Self {
            handle: Arc::new(EngineHandle::new()),
            deck_count: deck_count.clamp(1, MAX_DECKS),
            task: Mutex::new(None),
        }
    }

    fn task_slot(&self) -> std::sync::MutexGuard<'_, Option<JoinHandle<()>>> {
        self.task.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl Engine for SimEngine {
    async fn start(&self) -> anyhow::Result<()> {
        self.handle.set_network_state(NetworkState::Searching);

        let handle = Arc::clone(&self.handle);
        let deck_count = self.deck_count;
        *self.task_slot() = Some(tokio::spawn(async move { run(deck_count, handle).await }));

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if let Some(task) = self.task_slot().take() {
            task.abort();
        }
        self.handle.set_network_state(NetworkState::Offline);
        Ok(())
    }

    fn handle(&self) -> &EngineHandle {
        &self.handle
    }
}

/// Drives every simulated deck. The task owns the decks outright, so no deck
/// state is shared across threads — only the reports leaving through the
/// handle are.
async fn run(deck_count: u8, handle: Arc<EngineHandle>) {
    let mut next_track_id = FIRST_TRACK_ID;
    // Rolling index into `SIM_TRACKS` so decks pick distinct tracks.
    let mut track_cursor = 0usize;
    let mut decks: Vec<SimDeck> = (1..=deck_count)
        .map(|deck_id| {
            let track_index = track_cursor % SIM_TRACKS.len();
            track_cursor += 1;
            next_track_id += 1;
            SimDeck::new(deck_id, track_index, next_track_id - 1)
        })
        .collect();

    tokio::time::sleep(CONNECT_DELAY).await;
    handle.set_network_state(NetworkState::Connected);
    for deck in &decks {
        handle.device_connected(DeviceInfo {
            deck_id: deck.deck_id,
            name: format!("SIM-XDJ deck {}", deck.deck_id),
            ip: format!("192.168.1.{}", 10 + u16::from(deck.deck_id)),
        });
        handle.update_deck_track(deck.deck_id, deck.track.clone());
    }
    // Deck 1 starts rolling immediately, the others join the mix staggered.
    for (index, deck) in decks.iter_mut().enumerate() {
        deck.schedule(DECK_START_STAGGER * index as u32, Scheduled::StartPlaying);
    }

    let mut master_deck: DeckId = 1;
    let mut rng = SmallRng::from_os_rng();
    let mut ticker = interval(TICK);
    // A late tick must not trigger a burst of catch-up ticks that would jump
    // every playhead forward at once.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        let now = Instant::now();

        for index in 0..decks.len() {
            if let Some(action) = decks[index].take_due(now) {
                apply_scheduled(
                    &mut decks[index],
                    action,
                    &mut track_cursor,
                    &mut next_track_id,
                    &handle,
                );
            }

            let pitch = decks[index].advance_pitch();
            if decks[index].play_state == PlayState::Playing {
                advance_playhead(&mut decks[index], pitch);
                if should_hand_over_master(&decks[index]) {
                    master_deck = next_master(&decks, index).unwrap_or(master_deck);
                    decks[index].swapping = true;
                    decks[index].schedule(TRACK_SWAP_MARGIN, Scheduled::UnloadTrack);
                }
                maybe_act(&mut decks[index], &mut rng);
            }

            handle.update_deck_state(decks[index].to_deck_state(pitch, master_deck));
        }
    }
}

fn apply_scheduled(
    deck: &mut SimDeck,
    action: Scheduled,
    track_cursor: &mut usize,
    next_track_id: &mut u32,
    handle: &EngineHandle,
) {
    match action {
        Scheduled::Resume => deck.resume(),
        Scheduled::LandScratch { origin_ms } => {
            // A scratch lands slightly off where it started.
            deck.playhead_ms = origin_ms.max(0.0);
            deck.resume();
        }
        Scheduled::UnloadTrack => {
            deck.track_index = *track_cursor % SIM_TRACKS.len();
            *track_cursor += 1;
            deck.play_state = PlayState::Loading;
            deck.track = None;
            deck.playhead_ms = 0.0;
            handle.update_deck_track(deck.deck_id, None);
            deck.schedule(TRACK_LOAD_DELAY, Scheduled::LoadNextTrack);
        }
        Scheduled::LoadNextTrack => {
            let track = build_sim_track(&SIM_TRACKS[deck.track_index], *next_track_id);
            *next_track_id += 1;
            deck.track = Some(track.clone());
            deck.play_state = PlayState::Cued;
            handle.update_deck_track(deck.deck_id, Some(track));
            deck.schedule(CUE_TO_PLAY_DELAY, Scheduled::StartPlaying);
        }
        Scheduled::StartPlaying => {
            deck.play_state = PlayState::Playing;
            deck.swapping = false;
        }
    }
}

fn advance_playhead(deck: &mut SimDeck, pitch: f64) {
    let Some(duration_ms) = deck.track.as_ref().map(|track| track.duration_ms) else {
        return;
    };

    deck.playhead_ms += TICK.as_millis() as f64 * (1.0 + pitch / 100.0);
    if deck.playhead_ms >= duration_ms {
        deck.playhead_ms = duration_ms;
        deck.play_state = PlayState::Ended;
    }
}

/// True once the deck is close enough to the end to start swapping tracks.
fn should_hand_over_master(deck: &SimDeck) -> bool {
    if deck.swapping {
        return false;
    }
    deck.track.as_ref().is_some_and(|track| {
        track.duration_ms - deck.playhead_ms <= TRACK_SWAP_MARGIN.as_millis() as f64
    })
}

/// Hands master over to another playing deck, as a DJ transition would.
fn next_master(decks: &[SimDeck], leaving: usize) -> Option<DeckId> {
    decks
        .iter()
        .enumerate()
        .find(|(index, deck)| *index != leaving && deck.play_state == PlayState::Playing)
        .map(|(_, deck)| deck.deck_id)
}

/// Occasionally behaves like a DJ: pauses, jumps to a cue, scratches.
fn maybe_act(deck: &mut SimDeck, rng: &mut SmallRng) {
    if deck.busy || deck.swapping || deck.track.is_none() {
        return;
    }

    let roll: f64 = rng.random();
    if roll < PAUSE_ODDS {
        deck.busy = true;
        deck.play_state = PlayState::Paused;
        let millis = rng.random_range(PAUSE_MIN.as_millis() as u64..PAUSE_MAX.as_millis() as u64);
        deck.schedule(Duration::from_millis(millis), Scheduled::Resume);
    } else if roll < PAUSE_ODDS + CUE_JUMP_ODDS {
        jump_to_random_cue(deck, rng);
    } else if roll < PAUSE_ODDS + CUE_JUMP_ODDS + SCRATCH_ODDS {
        deck.busy = true;
        deck.play_state = PlayState::Searching;
        let drift = (rng.random::<f64>() - 0.5) * SCRATCH_SPREAD_MS;
        let origin_ms = deck.playhead_ms + drift;
        deck.schedule(SCRATCH, Scheduled::LandScratch { origin_ms });
    }
}

/// Jumps to a random cue that is not too close to the end of the track.
fn jump_to_random_cue(deck: &mut SimDeck, rng: &mut SmallRng) {
    let Some(track) = deck.track.as_ref() else {
        return;
    };
    let limit = track.duration_ms - TRACK_SWAP_MARGIN.as_millis() as f64 * 2.0;
    let targets: Vec<f64> = track
        .cues
        .iter()
        .filter(|cue| cue.offset_ms < limit)
        .map(|cue| cue.offset_ms)
        .collect();

    if !targets.is_empty() {
        deck.playhead_ms = targets[rng.random_range(0..targets.len())];
    }
}

fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as f64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> SimDeck {
        SimDeck::new(1, 0, FIRST_TRACK_ID)
    }

    #[test]
    fn deck_count_is_clamped_to_the_pro_dj_link_ceiling() {
        assert_eq!(SimEngine::new(0).deck_count, 1);
        assert_eq!(SimEngine::new(9).deck_count, MAX_DECKS);
    }

    #[test]
    fn playhead_stops_at_the_end_of_the_track() {
        let mut deck = deck();
        deck.play_state = PlayState::Playing;
        deck.playhead_ms = deck.track.as_ref().unwrap().duration_ms - 1.0;

        advance_playhead(&mut deck, 0.0);

        assert_eq!(deck.play_state, PlayState::Ended);
        assert_eq!(deck.playhead_ms, deck.track.as_ref().unwrap().duration_ms);
    }

    #[test]
    fn a_scratch_never_lands_before_the_start_of_the_track() {
        let mut deck = deck();
        deck.busy = true;

        apply_scheduled(
            &mut deck,
            Scheduled::LandScratch { origin_ms: -2000.0 },
            &mut 0,
            &mut 0,
            &EngineHandle::new(),
        );

        assert_eq!(deck.playhead_ms, 0.0);
        assert!(!deck.busy);
    }

    #[test]
    fn swap_is_armed_only_inside_the_margin_and_only_once() {
        let mut deck = deck();
        let duration_ms = deck.track.as_ref().unwrap().duration_ms;

        deck.playhead_ms = duration_ms - TRACK_SWAP_MARGIN.as_millis() as f64 - 1.0;
        assert!(!should_hand_over_master(&deck));

        deck.playhead_ms = duration_ms - 1.0;
        assert!(should_hand_over_master(&deck));

        deck.swapping = true;
        assert!(!should_hand_over_master(&deck));
    }

    #[test]
    fn only_the_master_deck_reports_itself_as_master() {
        let mut deck = deck();
        deck.play_state = PlayState::Playing;

        assert!(deck.to_deck_state(0.0, 1).is_master);
        assert!(!deck.to_deck_state(0.0, 1).is_synced);
        assert!(!deck.to_deck_state(0.0, 2).is_master);
        assert!(deck.to_deck_state(0.0, 2).is_synced);
    }

    #[test]
    fn a_cued_deck_is_neither_master_nor_on_air() {
        let state = deck().to_deck_state(0.0, 1);

        assert!(!state.is_master);
        assert!(!state.is_on_air);
        assert_eq!(state.beat_in_measure, None);
    }
}
