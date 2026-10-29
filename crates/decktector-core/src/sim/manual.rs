//! The run loop of the manual simulation.
//!
//! Where [`super::engine`]'s auto mode scripts a DJ, this loop only listens:
//! commands apply the instant they arrive, and a fast ticker carries the
//! rig through real time so a scratch or a nudge shows up without the
//! 200 ms lag the auto mode's tick would add. The rig itself reads no clock;
//! this loop measures the elapsed time and hands it over.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, watch, Mutex};
use tokio::time::{interval, Duration, Instant, MissedTickBehavior};

use super::clock::epoch_ms;
use super::command::SimCommand;
use super::engine::{announce_deck, CONNECT_DELAY};
use super::rig::SimRig;
use super::rig_state::SimRigState;
use crate::engine::EngineHandle;
use crate::model::{DeckId, NetworkState};

/// Short enough that a scratch tracks the hand.
const MANUAL_TICK: Duration = Duration::from_millis(40);

/// Everything a manual run owns, kept apart from the task that drives it so
/// that stopping and restarting the engine resumes the same rig and the same
/// queue of commands rather than starting over.
pub(super) struct ManualSession {
    pub rig: SimRig,
    pub commands: mpsc::UnboundedReceiver<SimCommand>,
    pub state: watch::Sender<SimRigState>,
}

/// Runs until aborted. Holds the session for as long as it lives, which also
/// keeps two runs from driving one rig.
pub(super) async fn run(session: Arc<Mutex<ManualSession>>, handle: Arc<EngineHandle>) {
    let mut session = session.lock().await;
    let mut announcements = TrackAnnouncements::default();

    tokio::time::sleep(CONNECT_DELAY).await;
    handle.set_network_state(NetworkState::Connected);
    for deck_id in 1..=super::rig::DECK_COUNT {
        announce_deck(&handle, deck_id);
    }
    publish(&mut session, &mut announcements, &handle);

    let mut ticker = interval(MANUAL_TICK);
    // A late tick must not be followed by a burst of catch-up ticks.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_tick = Instant::now();
    let mut commands_open = true;

    loop {
        tokio::select! {
            now = ticker.tick() => {
                let elapsed = now.saturating_duration_since(last_tick);
                last_tick = now;
                session.rig.advance(elapsed.as_secs_f64() * 1000.0);
            }
            command = session.commands.recv(), if commands_open => match command {
                Some(command) => session.rig.apply(command),
                None => {
                    commands_open = false;
                    continue;
                }
            },
        }
        publish(&mut session, &mut announcements, &handle);
    }
}

/// Reports the rig to everyone watching: the app through the engine handle,
/// the control page through the state channel.
fn publish(
    session: &mut ManualSession,
    announcements: &mut TrackAnnouncements,
    handle: &EngineHandle,
) {
    let reported_at = epoch_ms();
    let view = session.rig.view(reported_at);

    announcements.report_changes(&session.rig, &view, handle);
    for state in session.rig.deck_states(reported_at) {
        handle.update_deck_state(state);
    }
    session.state.send_replace(view);
}

/// Remembers which track each deck last told the engine about, so a track is
/// reported when it changes rather than on every tick.
#[derive(Default)]
struct TrackAnnouncements {
    /// Absent until the deck's first report; `None` inside means "empty".
    announced: HashMap<DeckId, Option<usize>>,
}

impl TrackAnnouncements {
    fn report_changes(&mut self, rig: &SimRig, view: &SimRigState, handle: &EngineHandle) {
        for deck in &view.decks {
            if self.announced.get(&deck.deck_id) == Some(&deck.track_index) {
                continue;
            }
            self.announced.insert(deck.deck_id, deck.track_index);
            handle.update_deck_track(deck.deck_id, rig.loaded_track(deck.deck_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::*;
    use crate::engine::{Engine, EngineEvent};
    use crate::model::PlayState;
    use crate::sim::{SimEngine, SimLibrary, SimMode};

    /// Long enough to cover the connect delay and many ticks; the paused
    /// clock jumps through it instead of waiting.
    const OBSERVATION: Duration = Duration::from_secs(20);

    /// Reads events until `matches` picks one out, or the observation ends.
    async fn next_matching<T>(
        events: &mut broadcast::Receiver<EngineEvent>,
        mut matches: impl FnMut(&EngineEvent) -> Option<T>,
    ) -> Option<T> {
        let deadline = Instant::now() + OBSERVATION;
        while Instant::now() < deadline {
            match tokio::time::timeout_at(deadline, events.recv()).await {
                Ok(Ok(event)) => {
                    if let Some(found) = matches(&event) {
                        return Some(found);
                    }
                }
                Ok(Err(error)) => panic!("event stream ended: {error}"),
                Err(_) => return None,
            }
        }
        None
    }

    fn playing_deck_one(event: &EngineEvent) -> Option<()> {
        match event {
            EngineEvent::DeckState(state)
                if state.deck_id == 1 && state.play_state == PlayState::Playing =>
            {
                Some(())
            }
            _ => None,
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn a_play_command_makes_deck_one_report_playing_through_the_engine() {
        let (engine, control) = SimEngine::manual(SimLibrary::generated(), None);
        let mut events = engine.handle().subscribe();
        engine.start().await.expect("engine started");

        control
            .send(SimCommand::PlayPause { deck: 1 })
            .expect("sent");

        assert_eq!(engine.mode(), SimMode::Manual);
        assert!(
            next_matching(&mut events, playing_deck_one).await.is_some(),
            "deck 1 never reported Playing"
        );
        assert_eq!(control.state().decks[0].play_state, PlayState::Playing);
        assert!(control.state().decks[0].is_master);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn a_manual_rig_does_nothing_until_it_is_told_to() {
        let (engine, control) = SimEngine::manual(SimLibrary::generated(), None);
        let mut events = engine.handle().subscribe();
        engine.start().await.expect("engine started");

        let moved = next_matching(&mut events, |event| match event {
            EngineEvent::DeckState(state) if state.play_state != PlayState::Cued => Some(()),
            _ => None,
        })
        .await;

        assert!(moved.is_none(), "a deck moved without a command");
        assert!(control
            .state()
            .decks
            .iter()
            .all(|deck| deck.position_ms == deck.cue_point_ms));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn the_engine_announces_the_four_decks_and_their_preloaded_tracks() {
        let (engine, _control) = SimEngine::manual(SimLibrary::generated(), None);
        let mut events = engine.handle().subscribe();
        engine.start().await.expect("engine started");

        let mut devices = 0;
        let mut tracks = 0;
        let _ = next_matching(&mut events, |event| {
            match event {
                EngineEvent::DeviceConnected(_) => devices += 1,
                EngineEvent::DeckTrack(deck_track) if deck_track.track.is_some() => tracks += 1,
                _ => {}
            }
            None::<()>
        })
        .await;

        assert_eq!((devices, tracks), (4, 4));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn loading_a_track_announces_it_for_that_deck() {
        let (engine, control) = SimEngine::manual(SimLibrary::generated(), None);
        let mut events = engine.handle().subscribe();
        engine.start().await.expect("engine started");
        control
            .send(SimCommand::Load {
                deck: 2,
                track_index: 3,
            })
            .expect("sent");

        let loaded = next_matching(&mut events, |event| match event {
            EngineEvent::DeckTrack(deck_track)
                if deck_track.deck_id == 2
                    && deck_track.track.as_ref().map(|t| &t.title)
                        == control.track(3).as_ref().map(|t| &t.title) =>
            {
                Some(())
            }
            _ => None,
        })
        .await;

        assert!(loaded.is_some(), "deck 2 never reported the loaded track");
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn restarting_the_engine_keeps_the_rig_where_the_player_left_it() {
        let (engine, control) = SimEngine::manual(SimLibrary::generated(), None);
        let mut events = engine.handle().subscribe();
        engine.start().await.expect("engine started");
        control
            .send(SimCommand::SetPitchFader {
                deck: 3,
                position: 0.5,
            })
            .expect("sent");
        control
            .send(SimCommand::PlayPause { deck: 1 })
            .expect("sent");
        assert!(next_matching(&mut events, playing_deck_one).await.is_some());

        engine.stop().await.expect("engine stopped");
        engine.start().await.expect("engine restarted");
        control.send(SimCommand::Cue { deck: 1 }).expect("sent");
        let mut restarted = engine.handle().subscribe();
        let cued = next_matching(&mut restarted, |event| match event {
            EngineEvent::DeckState(state)
                if state.deck_id == 1 && state.play_state == PlayState::Cued =>
            {
                Some(())
            }
            _ => None,
        })
        .await;

        assert!(cued.is_some(), "the restarted rig lost its commands");
        assert_eq!(control.state().decks[2].pitch_fader, 0.5);
        assert!(control.state().decks[0].is_master);
    }

    #[test]
    fn sending_to_a_dropped_engine_is_an_error() {
        let (engine, control) = SimEngine::manual(SimLibrary::generated(), None);
        drop(engine);

        assert!(control.send(SimCommand::PlayPause { deck: 1 }).is_err());
    }
}
