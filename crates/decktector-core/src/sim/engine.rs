//! Scripted engine for developing the UI without Pro DJ Link hardware:
//! decks that play, drift in pitch, chain generated tracks, and occasionally
//! behave like a DJ — including keeping only one or two decks playing at
//! once, as a real set would.
//!
//! That is [`SimMode::Auto`]. [`SimMode::Manual`] ([`SimEngine::manual`])
//! plays nothing by itself: four decks that do exactly what a
//! [`SimControl`] tells them, run by [`super::manual`].

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use tokio::sync::{mpsc, watch, Mutex as AsyncMutex};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration, Instant, MissedTickBehavior};

use super::beat::BEATS_PER_BAR;
use super::clock::epoch_ms;
use super::control::SimControl;
use super::library::{SimLibrary, FIRST_TRACK_ID};
use super::manual::{self, ManualSession};
use super::rig::{SimRig, DECK_COUNT};
use crate::engine::{Engine, EngineHandle};
use crate::model::{DeckId, DeckState, DeviceInfo, NetworkState, PlayState, TrackInfo};

const TICK: Duration = Duration::from_millis(200);
/// Time before the end of a track at which the sim deck loads the next one.
const TRACK_SWAP_MARGIN: Duration = Duration::from_secs(8);
const TRACK_LOAD_DELAY: Duration = Duration::from_millis(1500);
const CUE_TO_PLAY_DELAY: Duration = Duration::from_millis(2500);
/// Delay before the simulated network reports itself connected.
pub(super) const CONNECT_DELAY: Duration = Duration::from_millis(800);
/// Decks join the mix one after another rather than all at once.
const DECK_START_STAGGER: Duration = Duration::from_secs(5);

/// Per-tick probabilities of spontaneous DJ actions on a playing deck.
const PAUSE_ODDS: f64 = 0.004;
const CUE_JUMP_ODDS: f64 = 0.006;
const SCRATCH_ODDS: f64 = 0.004;
const SPIN_ODDS: f64 = 0.0015;

const PAUSE_MIN: Duration = Duration::from_millis(1000);
const PAUSE_MAX: Duration = Duration::from_millis(4000);
const SCRATCH: Duration = Duration::from_millis(800);
/// How far a scratch can land either side of where it started.
const SCRATCH_SPREAD_MS: f64 = 4000.0;
/// How long a vinyl-style spinback takes to land.
const SPIN: Duration = Duration::from_millis(900);
/// How far back a spinback throws the playhead.
const SPIN_BACK_MIN_MS: f64 = 2000.0;
const SPIN_BACK_MAX_MS: f64 = 8000.0;

/// DJs mostly run a single deck; a second is occasional, a third rare.
/// A fourth deck and beyond never gets a slot.
const SECOND_DECK_ODDS: f64 = 0.1;
const THIRD_DECK_ODDS: f64 = 0.02;
/// How long a deck waits before checking again for a free slot to play in.
const START_RETRY_DELAY: Duration = Duration::from_secs(3);
/// Full period of the sinusoidal pitch drift, in seconds.
const PITCH_DRIFT_PERIOD_SEC: f64 = 30.0;
/// Amplitude of the pitch drift, in percent.
const PITCH_DRIFT_PERCENT: f64 = 3.0;
/// Pro DJ Link networks top out at 6 players.
const MAX_DECKS: u8 = 6;

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
    /// Land the playhead at `target_ms` and resume, ending a spinback.
    LandSpin { target_ms: f64 },
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
    /// Index into the [`SimLibrary`] of the currently loaded track.
    track_index: usize,
    pitch_phase: f64,
    swapping: bool,
    /// True while a random action temporarily owns the play state.
    busy: bool,
    scheduled: Option<(Instant, Scheduled)>,
}

impl SimDeck {
    fn new(library: &SimLibrary, deck_id: DeckId, track_index: usize, track_id: u32) -> Self {
        Self {
            deck_id,
            play_state: PlayState::Cued,
            track: Some(library.track(track_index, track_id)),
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

/// Who plays the simulated decks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimMode {
    /// The engine scripts a DJ: random pauses, scratches, track changes.
    Auto,
    /// Nothing happens unless a [`SimControl`] says so.
    Manual,
}

pub struct SimEngine {
    handle: Arc<EngineHandle>,
    deck_count: u8,
    library: Arc<SimLibrary>,
    /// Present in manual mode only: the rig and its command queue.
    manual: Option<Arc<AsyncMutex<ManualSession>>>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl SimEngine {
    /// Draws its tracks from [`SimLibrary::from_env`]: recorded ones when
    /// the environment names a fixture file, generated ones otherwise.
    pub fn new(deck_count: u8) -> Self {
        Self::with_library(deck_count, SimLibrary::from_env())
    }

    /// Same, with the track source given outright — for a caller that has
    /// one already, and for tests, which cannot use an environment variable
    /// without racing every other test in the process.
    pub fn with_library(deck_count: u8, library: SimLibrary) -> Self {
        Self {
            handle: Arc::new(EngineHandle::new()),
            deck_count: deck_count.clamp(1, MAX_DECKS),
            library: Arc::new(library),
            manual: None,
            task: Mutex::new(None),
        }
    }

    /// Four decks played from outside, by the returned [`SimControl`].
    ///
    /// The library is sorted by title and its first four tracks are loaded;
    /// `fallback_warning` is passed on to the control page when the library
    /// the user asked for could not be used. Commands sent before
    /// [`Engine::start`] wait in the queue.
    pub fn manual(library: SimLibrary, fallback_warning: Option<String>) -> (Self, SimControl) {
        let rig = SimRig::from_library(&library, fallback_warning);
        let (command_sender, commands) = mpsc::unbounded_channel();
        let (state, state_receiver) = watch::channel(rig.view(epoch_ms()));
        let control = SimControl::new(command_sender, state_receiver, rig.library());
        let session = ManualSession {
            rig,
            commands,
            state,
        };

        let engine = Self {
            handle: Arc::new(EngineHandle::new()),
            deck_count: DECK_COUNT,
            library: Arc::new(library),
            manual: Some(Arc::new(AsyncMutex::new(session))),
            task: Mutex::new(None),
        };
        (engine, control)
    }

    pub fn mode(&self) -> SimMode {
        if self.manual.is_some() {
            SimMode::Manual
        } else {
            SimMode::Auto
        }
    }

    fn task_slot(&self) -> std::sync::MutexGuard<'_, Option<JoinHandle<()>>> {
        self.task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl Engine for SimEngine {
    async fn start(&self) -> anyhow::Result<()> {
        self.handle.set_network_state(NetworkState::Searching);

        let handle = Arc::clone(&self.handle);
        let task = match &self.manual {
            Some(session) => tokio::spawn(manual::run(Arc::clone(session), handle)),
            None => {
                let library = Arc::clone(&self.library);
                let deck_count = self.deck_count;
                tokio::spawn(async move { run(deck_count, library, handle).await })
            }
        };
        *self.task_slot() = Some(task);

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

    async fn download_track(
        &self,
        deck_id: DeckId,
        destination_dir: std::path::PathBuf,
        progress_callback: Box<dyn Fn(u64, u64) + Send + Sync>,
    ) -> anyhow::Result<std::path::PathBuf> {
        let track = self
            .handle
            .snapshot()
            .tracks
            .into_iter()
            .find(|t| t.deck_id == deck_id)
            .and_then(|t| t.track)
            .ok_or_else(|| anyhow::anyhow!("no track loaded on deck {deck_id}"))?;

        let artist = track.artist.as_deref().unwrap_or("Unknown Artist");
        let title = &track.title;
        let clean_artist = artist.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let clean_title = title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
        let filename = format!("{clean_artist} - {clean_title}.mp3");
        let dest_file = destination_dir.join(&filename);

        tokio::fs::create_dir_all(&destination_dir).await?;

        let total_size: u64 = 4 * 1024 * 1024; // 4 MB simulated track
        let chunk_size: u64 = 512 * 1024;
        let mut downloaded: u64 = 0;

        while downloaded < total_size {
            downloaded = (downloaded + chunk_size).min(total_size);
            progress_callback(downloaded, total_size);
            tokio::time::sleep(Duration::from_millis(60)).await;
        }

        tokio::fs::write(
            &dest_file,
            b"ID3\x03\x00\x00\x00\x00\x00\x00SIMULATED_AUDIO_DECKTECTOR",
        )
        .await?;
        Ok(dest_file)
    }
}

/// Announces a simulated player on the network.
pub(super) fn announce_deck(handle: &EngineHandle, deck_id: DeckId) {
    handle.device_connected(DeviceInfo {
        deck_id,
        name: format!("SIM-XDJ deck {deck_id}"),
        ip: format!("192.168.1.{}", 10 + u16::from(deck_id)),
    });
}

/// Drives every simulated deck. The task owns the decks outright, so no deck
/// state is shared across threads — only the reports leaving through the
/// handle are.
async fn run(deck_count: u8, library: Arc<SimLibrary>, handle: Arc<EngineHandle>) {
    let mut next_track_id = FIRST_TRACK_ID;
    // Rolling index into the library so decks pick distinct tracks.
    let mut track_cursor = 0usize;
    let mut decks: Vec<SimDeck> = (1..=deck_count)
        .map(|deck_id| {
            let track_index = track_cursor;
            track_cursor += 1;
            next_track_id += 1;
            SimDeck::new(&library, deck_id, track_index, next_track_id - 1)
        })
        .collect();

    tokio::time::sleep(CONNECT_DELAY).await;
    handle.set_network_state(NetworkState::Connected);
    for deck in &decks {
        announce_deck(&handle, deck.deck_id);
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
                if matches!(action, Scheduled::StartPlaying) {
                    let other_playing = decks
                        .iter()
                        .enumerate()
                        .filter(|(i, deck)| *i != index && deck.play_state == PlayState::Playing)
                        .count();
                    if should_promote_to_playing(other_playing, &mut rng) {
                        apply_scheduled(
                            &mut decks[index],
                            action,
                            &library,
                            &mut track_cursor,
                            &mut next_track_id,
                            &handle,
                        );
                    } else {
                        decks[index].schedule(START_RETRY_DELAY, Scheduled::StartPlaying);
                    }
                } else {
                    apply_scheduled(
                        &mut decks[index],
                        action,
                        &library,
                        &mut track_cursor,
                        &mut next_track_id,
                        &handle,
                    );
                }
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
    library: &SimLibrary,
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
        Scheduled::LandSpin { target_ms } => {
            // A spinback throws the playhead well behind where it started.
            deck.playhead_ms = target_ms.max(0.0);
            deck.resume();
        }
        Scheduled::UnloadTrack => {
            deck.track_index = *track_cursor;
            *track_cursor += 1;
            deck.play_state = PlayState::Loading;
            deck.track = None;
            deck.playhead_ms = 0.0;
            handle.update_deck_track(deck.deck_id, None);
            deck.schedule(TRACK_LOAD_DELAY, Scheduled::LoadNextTrack);
        }
        Scheduled::LoadNextTrack => {
            let track = library.track(deck.track_index, *next_track_id);
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
    } else if roll < PAUSE_ODDS + CUE_JUMP_ODDS + SCRATCH_ODDS + SPIN_ODDS {
        deck.busy = true;
        deck.play_state = PlayState::Searching;
        let back = rng.random_range(SPIN_BACK_MIN_MS..SPIN_BACK_MAX_MS);
        let target_ms = deck.playhead_ms - back;
        deck.schedule(SPIN, Scheduled::LandSpin { target_ms });
    }
}

/// Decides whether a deck about to roll actually gets a slot: the first deck
/// always gets in, a second is an occasional dice roll, a third rarer still,
/// as a real DJ set would.
fn should_promote_to_playing(other_decks_playing: usize, rng: &mut SmallRng) -> bool {
    match other_decks_playing {
        0 => true,
        1 => rng.random::<f64>() < SECOND_DECK_ODDS,
        2 => rng.random::<f64>() < THIRD_DECK_ODDS,
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineEvent;
    use crate::model::TrackInfo;

    fn deck() -> SimDeck {
        SimDeck::new(&SimLibrary::generated(), 1, 0, FIRST_TRACK_ID)
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
            &SimLibrary::generated(),
            &mut 0,
            &mut 0,
            &EngineHandle::new(),
        );

        assert_eq!(deck.playhead_ms, 0.0);
        assert!(!deck.busy);
    }

    #[test]
    fn a_spin_never_lands_before_the_start_of_the_track() {
        let mut deck = deck();
        deck.busy = true;

        apply_scheduled(
            &mut deck,
            Scheduled::LandSpin { target_ms: -5000.0 },
            &SimLibrary::generated(),
            &mut 0,
            &mut 0,
            &EngineHandle::new(),
        );

        assert_eq!(deck.playhead_ms, 0.0);
        assert!(!deck.busy);
    }

    #[test]
    fn a_second_and_third_deck_rarely_get_to_play_but_a_fourth_never_does() {
        let mut rng = SmallRng::seed_from_u64(1);

        assert!(should_promote_to_playing(0, &mut rng));

        let second_allowed = (0..1000)
            .filter(|_| should_promote_to_playing(1, &mut rng))
            .count();
        assert!(second_allowed > 0, "a second deck should sometimes play");
        assert!(
            (second_allowed as f64) < 1000.0 * (SECOND_DECK_ODDS + 0.05),
            "a second deck should stay occasional, got {second_allowed}/1000"
        );

        let third_allowed = (0..1000)
            .filter(|_| should_promote_to_playing(2, &mut rng))
            .count();
        assert!(third_allowed > 0, "a third deck should sometimes play");
        assert!(
            (third_allowed as f64) < 1000.0 * (THIRD_DECK_ODDS + 0.03),
            "a third deck should stay rare, got {third_allowed}/1000"
        );
        assert!(
            third_allowed < second_allowed,
            "a third deck should be rarer than a second"
        );

        assert!(!should_promote_to_playing(3, &mut rng));
        assert!(!should_promote_to_playing(4, &mut rng));
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

    /// The point of a recorded library: what reaches the UI is the track a
    /// player actually served, waveform and all, not something reshaped on
    /// the way through.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn a_recorded_library_reaches_the_decks_untouched() {
        let recorded = TrackInfo {
            track_id: 4242,
            title: "Recorded Track".to_owned(),
            artist: Some("Recorded Artist".to_owned()),
            album: None,
            genre: None,
            key: Some("8A".to_owned()),
            label: None,
            comment: None,
            duration_ms: 300_000.0,
            bpm: Some(126.5),
            artwork: None,
            beat_grid: None,
            waveform: None,
            cues: Vec::new(),
            events: Vec::new(),
        };
        let engine = SimEngine::with_library(1, SimLibrary::from_tracks(vec![recorded.clone()]));
        let mut events = engine.handle().subscribe();
        engine.start().await.expect("engine started");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let mut loaded = None;
        while tokio::time::Instant::now() < deadline && loaded.is_none() {
            match tokio::time::timeout(Duration::from_secs(3), events.recv()).await {
                Ok(Ok(EngineEvent::DeckTrack(deck_track))) => loaded = deck_track.track.clone(),
                Ok(Ok(_)) => {}
                Ok(Err(error)) => panic!("event stream ended: {error}"),
                Err(_) => break,
            }
        }

        let loaded = loaded.expect("the deck should report a loaded track");
        assert_eq!(
            loaded, recorded,
            "a recorded track should reach the deck exactly as recorded"
        );
    }

    /// Exercises the path the renderer actually depends on: a started engine
    /// reports itself connected, announces its decks, and streams deck state.
    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn a_started_engine_reports_devices_tracks_and_deck_state() {
        let engine = SimEngine::new(2);
        let mut events = engine.handle().subscribe();
        engine.start().await.expect("engine started");

        let mut devices = 0;
        let mut tracks = 0;
        let mut deck_states = 0;
        let mut connected = false;

        // Long enough to cover the connect delay and several ticks; the
        // paused clock advances on its own, so this does not actually wait.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_secs(3), events.recv()).await {
                Ok(Ok(EngineEvent::NetworkState(NetworkState::Connected))) => connected = true,
                Ok(Ok(EngineEvent::DeviceConnected(_))) => devices += 1,
                Ok(Ok(EngineEvent::DeckTrack(_))) => tracks += 1,
                Ok(Ok(EngineEvent::DeckState(_))) => deck_states += 1,
                Ok(Ok(_)) => {}
                Ok(Err(error)) => panic!("event stream ended: {error}"),
                Err(_) => break,
            }
        }

        assert!(connected, "engine never reported itself connected");
        assert_eq!(devices, 2, "both simulated decks should announce");
        assert_eq!(tracks, 2, "both decks should report a loaded track");
        assert!(
            deck_states > 4,
            "deck state should stream, got {deck_states}"
        );

        engine.stop().await.expect("engine stopped");
        assert_eq!(
            engine.handle().snapshot().network_state,
            NetworkState::Offline
        );
    }
}
