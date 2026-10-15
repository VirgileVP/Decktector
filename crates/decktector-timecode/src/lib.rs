//! Timecode generation for lighting/video sync.
//!
//! Reads a chosen deck's playhead from the [`Engine`] and turns it into
//! `HH:MM:SS:FF`, output as Art-Net TC (`OpTimeCode`), MIDI Time Code (MTC)
//! or Linear Timecode (LTC). The first two are discrete messages driven by
//! this module's tick loop; LTC is a continuous audio signal driven by its
//! own OS thread — see the [`ltc`] module doc for why it doesn't fit the
//! tick-loop shape the other two share.
//!
//! The renderer never talks to this crate directly — the Tauri app shell
//! wraps [`TimecodeService`] in commands and forwards [`TimecodeStatus`]
//! events, the same bridge shape as the engine layer.

pub mod artnet;
pub mod ltc;
pub mod mtc;

/// Re-exported so existing callers keep writing `decktector_timecode::network::*` —
/// the lookup itself now lives in `decktector-net`, shared with the remote service.
pub use decktector_net as network;

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use decktector_core::model::{DeckId, DeckState, PlayState};
use decktector_core::Engine;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, watch};
use ts_rs::TS;

/// How many status ticks a lagging subscriber may miss before it has to
/// re-hydrate from [`TimecodeService::status`].
const STATUS_BUFFER: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "FrameRate.ts")]
pub enum FrameRate {
    Film24,
    Ebu25,
    #[default]
    Smpte30,
}

impl FrameRate {
    fn fps(self) -> u64 {
        match self {
            Self::Film24 => 24,
            Self::Ebu25 => 25,
            Self::Smpte30 => 30,
        }
    }

    /// Art-Net's `Type` byte: 0 = Film, 1 = EBU, 2 = Drop (unsupported — see
    /// the module doc), 3 = SMPTE.
    fn art_net_type(self) -> u8 {
        match self {
            Self::Film24 => 0,
            Self::Ebu25 => 1,
            Self::Smpte30 => 3,
        }
    }

    /// MTC's 2-bit rate code, used in both the quarter-frame piece 7 message
    /// and the Full Timecode SysEx hours byte: 00 = 24fps, 01 = 25fps,
    /// 11 = 30fps (non-drop). No drop-frame rate in this codebase on
    /// purpose — see [`Self::art_net_type`], same reasoning applies here.
    fn mtc_rate_code(self) -> u8 {
        match self {
            Self::Film24 => 0b00,
            Self::Ebu25 => 0b01,
            Self::Smpte30 => 0b11,
        }
    }
}

/// Where generated timecode goes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "TimecodeProtocol.ts")]
pub enum TimecodeProtocol {
    #[default]
    Off,
    /// `interface_name` is a [`network::NetworkInterfaceInfo::name`] — the
    /// adapter to broadcast from, not a typed destination address.
    ArtNet { interface_name: String, port: u16 },
    /// `port_name` is a name as returned by [`mtc::list_output_ports`].
    Mtc { port_name: String },
    /// `device_name` is a name as returned by [`ltc::list_output_devices`].
    Ltc { device_name: String },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TimecodeConfig.ts")]
pub struct TimecodeConfig {
    pub protocol: TimecodeProtocol,
    pub frame_rate: FrameRate,
}

/// What the header clock renders, and what is actually being sent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "TimecodeStatus.ts")]
pub struct TimecodeStatus {
    /// `false` when the protocol is off, no source deck is picked, or the
    /// source deck has no playhead yet.
    pub running: bool,
    pub source_deck_id: Option<DeckId>,
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct TimeCode {
    hours: u8,
    minutes: u8,
    seconds: u8,
    frames: u8,
}

impl TimeCode {
    fn from_elapsed_ms(elapsed_ms: f64, frame_rate: FrameRate) -> Self {
        let fps = frame_rate.fps();
        let total_frames = ((elapsed_ms.max(0.0) / 1000.0) * fps as f64) as u64;
        let frames = (total_frames % fps) as u8;
        let total_seconds = total_frames / fps;
        let seconds = (total_seconds % 60) as u8;
        let total_minutes = total_seconds / 60;
        let minutes = (total_minutes % 60) as u8;
        let hours = ((total_minutes / 60) % 24) as u8;
        Self {
            hours,
            minutes,
            seconds,
            frames,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct GeneratorState {
    source_deck_id: Option<DeckId>,
    config: TimecodeConfig,
}

/// Generates timecode from a deck's playhead and sends it out.
///
/// Construction is synchronous and does no I/O; [`Self::run`] is the async
/// worker, spawned once a Tokio runtime is available (mirrors how the engine
/// itself is started from inside Tauri's `setup`, not at construction time).
pub struct TimecodeService {
    engine: Arc<dyn Engine>,
    state_tx: watch::Sender<GeneratorState>,
    state_rx: watch::Receiver<GeneratorState>,
    status_tx: broadcast::Sender<TimecodeStatus>,
    last_status: Mutex<TimecodeStatus>,
}

impl TimecodeService {
    pub fn new(engine: Arc<dyn Engine>) -> Arc<Self> {
        let (state_tx, state_rx) = watch::channel(GeneratorState::default());
        Arc::new(Self {
            engine,
            state_tx,
            state_rx,
            status_tx: broadcast::channel(STATUS_BUFFER).0,
            last_status: Mutex::new(TimecodeStatus::default()),
        })
    }

    pub fn set_source(&self, deck_id: Option<DeckId>) {
        self.state_tx
            .send_modify(|state| state.source_deck_id = deck_id);
    }

    pub fn set_config(&self, config: TimecodeConfig) {
        self.state_tx.send_modify(|state| state.config = config);
    }

    /// Subscribes to status ticks. Pair with [`Self::status`] to hydrate a
    /// fresh listener — ticks emitted before this call are not replayed.
    pub fn subscribe(&self) -> broadcast::Receiver<TimecodeStatus> {
        self.status_tx.subscribe()
    }

    pub fn status(&self) -> TimecodeStatus {
        *self
            .last_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs until the process exits. Art-Net produces one frame per tick;
    /// MTC ticks 4x as fast and produces one quarter-frame message per tick
    /// (see [`tick_period`]). LTC ticks at the Art-Net cadence too, but a
    /// tick doesn't send a message — it lazily (re)starts the dedicated LTC
    /// audio thread and hands it a jam-sync target (see the [`ltc`] module
    /// doc). A config or source change is picked up on the next tick.
    ///
    /// The socket/MIDI connection/audio stream are opened lazily on first
    /// use: a bind/connect/start failure (rare) disables sending for this
    /// run rather than crashing the app shell over a feature nobody may
    /// even enable.
    pub async fn run(self: Arc<Self>) {
        let mut output = OutputState::default();

        let mut state_rx = self.state_rx.clone();
        let mut ticker = tokio::time::interval(tick_period(&state_rx.borrow().config));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break; // The service was dropped.
                    }
                    ticker = tokio::time::interval(tick_period(&state_rx.borrow().config));
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                }
                _ = ticker.tick() => {
                    self.tick(&mut output, &state_rx).await;
                }
            }
        }
    }

    async fn tick(&self, output: &mut OutputState, state_rx: &watch::Receiver<GeneratorState>) {
        let state = state_rx.borrow().clone();

        let new_status = match self.resolve(&state) {
            Some((timecode, deck_id)) => match &state.config.protocol {
                TimecodeProtocol::ArtNet {
                    interface_name,
                    port,
                } => {
                    output.mtc_session = None; // Not running MTC this tick.
                    output.ltc_stream = None; // Not running LTC this tick — stop any audio actually playing.
                    match bound_socket(&mut output.art_net_socket, interface_name).await {
                        Ok(socket) => {
                            if let Err(error) =
                                artnet::send(socket, *port, timecode, state.config.frame_rate).await
                            {
                                tracing::warn!(%error, interface_name, port, "failed to send an Art-Net TC frame");
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, interface_name, "failed to bind the timecode socket to the chosen network interface");
                        }
                    }
                    Some(running_status(timecode, deck_id))
                }
                TimecodeProtocol::Mtc { port_name } => {
                    output.ltc_stream = None; // Not running LTC this tick — stop any audio actually playing.
                    self.tick_mtc(
                        output,
                        port_name,
                        timecode,
                        deck_id,
                        state.config.frame_rate,
                    )
                }
                TimecodeProtocol::Ltc { device_name } => {
                    output.mtc_session = None; // Not running MTC this tick.
                    self.tick_ltc(output, device_name, timecode, state.config.frame_rate);
                    Some(running_status(timecode, deck_id))
                }
                TimecodeProtocol::Off => {
                    unreachable!("resolve() returns None when the protocol is off")
                }
            },
            None => {
                output.mtc_session = None;
                output.ltc_stream = None;
                Some(TimecodeStatus {
                    running: false,
                    source_deck_id: state.source_deck_id,
                    ..TimecodeStatus::default()
                })
            }
        };

        if let Some(status) = new_status {
            *self
                .last_status
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = status;
            // No subscribers yet (window still opening) is not an error.
            let _ = self.status_tx.send(status);
        }
    }

    /// Sends this tick's quarter-frame message and, once every 8 ticks (one
    /// full cycle, 2 frames), the value the caller should broadcast as the
    /// new [`TimecodeStatus`] — `None` on the 7 ticks in between, since the
    /// status only actually advances at the start of each cycle.
    ///
    /// A Full Timecode SysEx is sent whenever output (re)starts or the
    /// source deck / frame rate changes, detected by comparing against the
    /// session recorded on the previous tick — so a receiver can lock
    /// immediately instead of waiting on the quarter-frame stream.
    fn tick_mtc(
        &self,
        output: &mut OutputState,
        port_name: &str,
        timecode: TimeCode,
        deck_id: DeckId,
        frame_rate: FrameRate,
    ) -> Option<TimecodeStatus> {
        let session = MtcSession {
            deck_id,
            frame_rate,
        };
        let is_restart = output.mtc_session != Some(session);
        output.mtc_session = Some(session);
        if is_restart {
            output.mtc_cycle = mtc::QuarterFrameCycle::new();
        }

        let quarter_frame = output.mtc_cycle.tick(timecode, frame_rate);

        match connected_midi(&mut output.mtc_connection, port_name) {
            Ok(connection) => {
                if is_restart {
                    let sysex = mtc::full_timecode_sysex(quarter_frame.timecode, frame_rate);
                    mtc::send(connection, &sysex);
                }
                mtc::send(connection, &quarter_frame.bytes);
            }
            Err(error) => {
                tracing::warn!(%error, port_name, "failed to connect to the chosen MIDI output port");
            }
        }

        quarter_frame
            .is_new_cycle
            .then(|| running_status(quarter_frame.timecode, deck_id))
    }

    /// Keeps the LTC audio stream alive and jam-synced. Unlike Art-Net and
    /// MTC there is no message to send here: the tick loop only lazily
    /// (re)starts the stream on a device mismatch (mirroring
    /// [`bound_socket`]/[`connected_midi`]) and hands it the deck's current
    /// timecode so its free-running audio thread can re-latch if it has
    /// drifted — see the [`ltc`] module doc for the full picture.
    fn tick_ltc(
        &self,
        output: &mut OutputState,
        device_name: &str,
        timecode: TimeCode,
        frame_rate: FrameRate,
    ) {
        match ltc_stream(&mut output.ltc_stream, device_name, timecode, frame_rate) {
            Ok(stream) => stream.resync(timecode, frame_rate),
            Err(error) => {
                tracing::warn!(%error, device_name, "failed to start the LTC output stream");
            }
        }
    }

    /// The timecode for this instant, if the config and source deck allow
    /// producing one at all.
    fn resolve(&self, state: &GeneratorState) -> Option<(TimeCode, DeckId)> {
        if matches!(state.config.protocol, TimecodeProtocol::Off) {
            return None;
        }
        let deck_id = state.source_deck_id?;
        // Runs on every tick (up to ~120 a second for MTC): read the source
        // deck alone, not a whole snapshot that clones every track's waveform.
        let deck = self.engine.handle().deck_state(deck_id)?;
        let elapsed_ms = extrapolate_playhead_ms(&deck, epoch_ms())?;
        Some((
            TimeCode::from_elapsed_ms(elapsed_ms, state.config.frame_rate),
            deck_id,
        ))
    }
}

/// The cached socket if it is already bound to `interface_name`, otherwise a
/// freshly bound replacement.
///
/// Rebinding on every mismatch (not just on the first call) is what lets a
/// mid-session interface switch in the settings panel take effect on the
/// very next tick, and what recovers if the adapter's address changed under
/// DHCP.
async fn bound_socket<'a>(
    cached: &'a mut Option<(String, UdpSocket)>,
    interface_name: &str,
) -> anyhow::Result<&'a UdpSocket> {
    let is_current = matches!(cached, Some((bound_name, _)) if bound_name == interface_name);
    if !is_current {
        *cached = Some((
            interface_name.to_string(),
            bind_socket(interface_name).await?,
        ));
    }
    Ok(&cached.as_ref().expect("just bound above").1)
}

async fn bind_socket(interface_name: &str) -> anyhow::Result<UdpSocket> {
    let ip = network::resolve_ipv4(interface_name)?;
    let socket = UdpSocket::bind((ip, 0)).await?;
    // Art-Net is conventionally broadcast on the node's subnet; unicast
    // targets still work fine with this flag set.
    socket.set_broadcast(true)?;
    Ok(socket)
}

/// The cached MIDI connection if it is already open on `port_name`,
/// otherwise a freshly opened replacement.
///
/// Mirrors [`bound_socket`]: rebuilding on every mismatch is what lets a
/// mid-session port switch in the settings panel take effect on the very
/// next tick.
fn connected_midi<'a>(
    cached: &'a mut Option<(String, midir::MidiOutputConnection)>,
    port_name: &str,
) -> anyhow::Result<&'a mut midir::MidiOutputConnection> {
    let is_current = matches!(cached, Some((connected_name, _)) if connected_name == port_name);
    if !is_current {
        *cached = Some((port_name.to_string(), mtc::connect(port_name)?));
    }
    Ok(&mut cached.as_mut().expect("just connected above").1)
}

/// The cached LTC stream if it is already bound to `device_name`, otherwise
/// a freshly started replacement.
///
/// Mirrors [`bound_socket`] and [`connected_midi`]: rebuilding on every
/// mismatch is what lets a mid-session device switch in the settings panel
/// take effect on the very next tick. Unlike those two, no wrapper tuple is
/// needed here — [`ltc::LtcStream`] already remembers its own device name.
fn ltc_stream<'a>(
    cached: &'a mut Option<ltc::LtcStream>,
    device_name: &str,
    initial_timecode: TimeCode,
    frame_rate: FrameRate,
) -> anyhow::Result<&'a ltc::LtcStream> {
    let is_current = matches!(cached, Some(stream) if stream.device_name() == device_name);
    if !is_current {
        *cached = Some(ltc::LtcStream::start(
            device_name,
            initial_timecode,
            frame_rate,
        )?);
    }
    Ok(cached.as_ref().expect("just started above"))
}

fn frame_period(frame_rate: FrameRate) -> Duration {
    Duration::from_secs_f64(1.0 / frame_rate.fps() as f64)
}

/// How often the ticker should fire for the current config: once per frame
/// for Art-Net (one full packet per tick) and for LTC (just often enough to
/// keep the audio thread jam-synced — see the [`ltc`] module doc), 4x per
/// frame for MTC (one quarter-frame message per tick — see the [`mtc`]
/// module doc).
fn tick_period(config: &TimecodeConfig) -> Duration {
    match config.protocol {
        TimecodeProtocol::Mtc { .. } => frame_period(config.frame_rate) / 4,
        TimecodeProtocol::ArtNet { .. } | TimecodeProtocol::Ltc { .. } | TimecodeProtocol::Off => {
            frame_period(config.frame_rate)
        }
    }
}

fn running_status(timecode: TimeCode, deck_id: DeckId) -> TimecodeStatus {
    TimecodeStatus {
        running: true,
        source_deck_id: Some(deck_id),
        hours: timecode.hours,
        minutes: timecode.minutes,
        seconds: timecode.seconds,
        frames: timecode.frames,
    }
}

/// Cross-tick state threaded through [`TimecodeService::tick`]: the
/// Art-Net socket, MIDI connection and LTC audio stream, each cached and
/// only rebuilt when what they're bound to changes, plus the MTC
/// quarter-frame cycle's own progress and which deck/frame-rate session it
/// was last latched for.
#[derive(Default)]
struct OutputState {
    art_net_socket: Option<(String, UdpSocket)>,
    mtc_connection: Option<(String, midir::MidiOutputConnection)>,
    mtc_cycle: mtc::QuarterFrameCycle,
    mtc_session: Option<MtcSession>,
    ltc_stream: Option<ltc::LtcStream>,
}

/// Identifies what an MTC output stream is currently describing — compared
/// tick-to-tick to detect a source deck or frame-rate change, which needs
/// an immediate re-latch and a fresh Full Timecode SysEx.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MtcSession {
    deck_id: DeckId,
    frame_rate: FrameRate,
}

/// How far past its last status a playing deck's position is projected —
/// several missed ~200ms status packets, not the length of a network
/// outage. Mirrors `MAX_EXTRAPOLATION_MS` in the renderer's `lib/playhead.ts`.
const MAX_EXTRAPOLATION_MS: f64 = 1_500.0;

/// Mirrors the renderer's own extrapolation in `lib/playhead.ts`: status
/// packets land at ~5 Hz, so the position between them is projected forward
/// from the last anchor using the deck's pitch. Once status stops arriving
/// the projection holds at [`MAX_EXTRAPOLATION_MS`]: timecode freezes with
/// the deck instead of running on for a deck that may have stopped long ago.
fn extrapolate_playhead_ms(deck: &DeckState, now_ms: f64) -> Option<f64> {
    let playhead_ms = deck.playhead_ms?;
    if deck.play_state != PlayState::Playing {
        return Some(playhead_ms);
    }
    let elapsed_ms = (now_ms - deck.reported_at).min(MAX_EXTRAPOLATION_MS);
    Some(playhead_ms + elapsed_ms * (1.0 + deck.pitch / 100.0))
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

    fn deck(playhead_ms: f64, play_state: PlayState, pitch: f64, reported_at: f64) -> DeckState {
        DeckState {
            deck_id: 1,
            play_state,
            is_master: false,
            is_synced: false,
            is_on_air: false,
            pitch,
            track_bpm: None,
            effective_bpm: None,
            beat_in_measure: None,
            track_id: None,
            playhead_ms: Some(playhead_ms),
            reported_at,
        }
    }

    #[test]
    fn timecode_rolls_over_at_60_seconds_and_60_minutes() {
        let timecode = TimeCode::from_elapsed_ms(61_500.0, FrameRate::Smpte30);
        assert_eq!(timecode.minutes, 1);
        assert_eq!(timecode.seconds, 1);
        assert_eq!(timecode.frames, 15);
    }

    #[test]
    fn paused_deck_reports_its_playhead_without_extrapolation() {
        let deck = deck(10_000.0, PlayState::Paused, 4.0, 0.0);
        assert_eq!(extrapolate_playhead_ms(&deck, 5_000.0), Some(10_000.0));
    }

    #[test]
    fn playing_deck_extrapolates_forward_with_pitch() {
        let deck = deck(10_000.0, PlayState::Playing, 4.0, 1_000.0);
        // 500ms of wall time at +4% pitch.
        assert_eq!(extrapolate_playhead_ms(&deck, 1_500.0), Some(10_520.0));
    }

    #[test]
    fn a_deck_whose_status_stopped_arriving_holds_its_timecode() {
        let deck = deck(10_000.0, PlayState::Playing, 0.0, 1_000.0);
        let held = extrapolate_playhead_ms(&deck, 1_000.0 + MAX_EXTRAPOLATION_MS);
        // A minute without status: the deck may have stopped long ago.
        assert_eq!(extrapolate_playhead_ms(&deck, 61_000.0), held);
    }

    #[test]
    fn no_playhead_yet_means_no_timecode() {
        let mut deck = deck(0.0, PlayState::Playing, 0.0, 0.0);
        deck.playhead_ms = None;
        assert_eq!(extrapolate_playhead_ms(&deck, 1_000.0), None);
    }
}
