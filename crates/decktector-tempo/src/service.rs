//! The tempo output service and its run loop.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use decktector_core::{Engine, EngineEvent};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, watch};

use crate::config::TempoOutputConfig;
use crate::output::Output;
use crate::status::TempoOutputStatus;
use crate::validation::ValidTempoConfig;

/// How many status changes a lagging subscriber may miss before it has to
/// re-hydrate from [`TempoOutputService::status`].
const STATUS_BUFFER: usize = 16;

/// How often BPM messages that ask for it are re-sent unchanged, so a
/// receiver started mid-set (or one that dropped a UDP packet) catches up
/// within a second.
pub const FULL_REFRESH_PERIOD: Duration = Duration::from_secs(1);

/// Sends the decks' tempo and phase as OSC.
///
/// Construction is synchronous and does no I/O; [`Self::run`] is the async
/// worker, spawned once a Tokio runtime is available — the same shape as
/// `decktector_timecode::TimecodeService`.
pub struct TempoOutputService {
    engine: Arc<dyn Engine>,
    /// The config as the user entered it, for the settings panel to read back.
    config: Mutex<TempoOutputConfig>,
    valid_config_tx: watch::Sender<Arc<ValidTempoConfig>>,
    status_tx: broadcast::Sender<TempoOutputStatus>,
    last_status: Mutex<TempoOutputStatus>,
}

impl TempoOutputService {
    pub fn new(engine: Arc<dyn Engine>) -> Arc<Self> {
        Arc::new(Self {
            engine,
            config: Mutex::new(TempoOutputConfig::default()),
            valid_config_tx: watch::channel(Arc::new(ValidTempoConfig::default())).0,
            status_tx: broadcast::channel(STATUS_BUFFER).0,
            last_status: Mutex::new(TempoOutputStatus::default()),
        })
    }

    /// Applies `config`. Whatever is unusable in it (a bad host, an invalid
    /// address...) is left out and reported in the status, never refused.
    /// BPM messages are sent again right after a change; beats are not.
    pub fn set_config(&self, config: TempoOutputConfig) {
        let valid = Arc::new(config.validate());
        *lock(&self.config) = config;
        self.valid_config_tx.send_replace(valid);
    }

    pub fn config(&self) -> TempoOutputConfig {
        lock(&self.config).clone()
    }

    /// Subscribes to status changes. Pair with [`Self::status`] to hydrate a
    /// fresh listener — changes emitted before this call are not replayed.
    pub fn subscribe(&self) -> broadcast::Receiver<TempoOutputStatus> {
        self.status_tx.subscribe()
    }

    pub fn status(&self) -> TempoOutputStatus {
        lock(&self.last_status).clone()
    }

    /// Runs until the engine's event stream closes. Each deck status event,
    /// config change and [`FULL_REFRESH_PERIOD`] tick leads to one update:
    /// the messages whose trigger fired go out (and, on the tick, the BPM
    /// messages that ask for a refresh).
    ///
    /// The UDP socket is opened on the first packet, not here: a failure
    /// to open or send is reported in the status and retried, never fatal.
    pub async fn run(self: Arc<Self>) {
        let handle = self.engine.handle();
        let mut events = handle.subscribe();
        let mut config_rx = self.valid_config_tx.subscribe();
        let mut output = Output::default();
        output.replace_decks(handle.snapshot().decks);
        output.apply_config(&config_rx.borrow_and_update());

        let mut refresh = tokio::time::interval(FULL_REFRESH_PERIOD);
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            let refresh_due = tokio::select! {
                changed = config_rx.changed() => {
                    if changed.is_err() {
                        break; // The service was dropped.
                    }
                    output.apply_config(&config_rx.borrow_and_update());
                    false
                }
                event = events.recv() => match event {
                    Ok(EngineEvent::DeckState(deck)) => {
                        output.track(deck);
                        false
                    }
                    Ok(EngineEvent::DeviceDisconnected(device)) => {
                        output.forget(device.deck_id);
                        false
                    }
                    Ok(_) => continue,
                    Err(RecvError::Lagged(_)) => {
                        output.replace_decks(handle.snapshot().decks);
                        false
                    }
                    Err(RecvError::Closed) => break,
                },
                _ = refresh.tick() => true,
            };

            let config = config_rx.borrow().clone();
            let status = output.update(&config, refresh_due).await;
            self.publish_status(status);
        }
    }

    /// Stores `status` and broadcasts it, only if it differs from the last.
    fn publish_status(&self, status: TempoOutputStatus) {
        {
            let mut last = lock(&self.last_status);
            if *last == status {
                return;
            }
            *last = status.clone();
        }
        // No subscribers yet (window still opening) is not an error.
        let _ = self.status_tx.send(status);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
