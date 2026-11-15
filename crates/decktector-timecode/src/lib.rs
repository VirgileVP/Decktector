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

use std::net::{Ipv4Addr, SocketAddrV4};
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
    /// `false` when the protocol is off, no source deck is picked, the
    /// source deck has no playhead yet, or the output could not be opened
    /// or written to — the clock only runs while timecode actually goes out.
    pub running: bool,
    /// `true` while the source deck is not advancing (paused, cued, or its
    /// status stopped arriving): no timecode streams then. LTC falls silent,
    /// and Art-Net and MTC send the held position once, then again only
    /// when it moves. Only meaningful while `running` is `true`.
    pub paused: bool,
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
        let total_frames = ((elapsed_ms.max(0.0) / 1000.0) * frame_rate.fps() as f64) as u64;
        Self::from_frame_count(total_frames, frame_rate)
    }

    /// The timecode `total_frames` frames after `00:00:00:00`, wrapping at
    /// 24 hours like a real timecode clock.
    fn from_frame_count(total_frames: u64, frame_rate: FrameRate) -> Self {
        let fps = frame_rate.fps();
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

    /// How many frames after `00:00:00:00` this timecode lies — the exact
    /// inverse of [`Self::from_frame_count`]. Stepping or comparing frames
    /// goes through this integer count, never through milliseconds: a frame
    /// period is not a whole number of milliseconds, and the round trip
    /// lands just short of a frame boundary often enough to repeat a frame.
    fn frame_count(self, frame_rate: FrameRate) -> u64 {
        let total_seconds =
            u64::from(self.hours) * 3600 + u64::from(self.minutes) * 60 + u64::from(self.seconds);
        total_seconds * frame_rate.fps() + u64::from(self.frames)
    }
}

/// What the source deck says the timecode is at this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SourceReading {
    deck_id: DeckId,
    timecode: TimeCode,
    /// Whether that timecode is moving forward in real time — see
    /// [`Playhead::is_advancing`].
    is_advancing: bool,
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
                    // A new deck, protocol or destination has not been told
                    // where a paused deck stands yet.
                    output.held = None;
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
        let frame_rate = state.config.frame_rate;

        let new_status = match self.resolve(&state) {
            Some(reading) => match &state.config.protocol {
                TimecodeProtocol::ArtNet {
                    interface_name,
                    port,
                } => {
                    output.mtc_session = None; // Not running MTC this tick.
                    output.ltc_stream = None; // Not running LTC this tick — stop any audio actually playing.
                    if output.has_located(reading) {
                        None
                    } else {
                        let is_sent = self
                            .tick_art_net(
                                output,
                                interface_name,
                                *port,
                                reading.timecode,
                                frame_rate,
                            )
                            .await;
                        Some(output.record_send(reading, is_sent))
                    }
                }
                TimecodeProtocol::Mtc { port_name } => {
                    output.ltc_stream = None; // Not running LTC this tick — stop any audio actually playing.
                    if reading.is_advancing {
                        output.held = None;
                        self.tick_mtc(output, port_name, reading, frame_rate)
                    } else if output.has_located(reading) {
                        None
                    } else {
                        let is_sent =
                            self.locate_mtc(output, port_name, reading.timecode, frame_rate);
                        Some(output.record_send(reading, is_sent))
                    }
                }
                TimecodeProtocol::Ltc { device_name } => {
                    output.mtc_session = None; // Not running MTC this tick.

                    // No locate here: a paused deck's LTC is silence, which
                    // the audio thread produces from the jam-sync target.
                    let is_sent = self.tick_ltc(output, device_name, reading, frame_rate);
                    Some(output_status(
                        reading.timecode,
                        reading.deck_id,
                        is_sent,
                        !reading.is_advancing,
                    ))
                }
                TimecodeProtocol::Off => {
                    unreachable!("resolve() returns None when the protocol is off")
                }
            },
            None => {
                output.mtc_session = None;
                output.ltc_stream = None;
                output.held = None;
                Some(stopped_status(state.source_deck_id))
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

    /// Sends this tick's Art-Net TC frame, (re)binding the socket first when
    /// needed. Returns whether the frame actually went out; a failure is
    /// logged here, since UDP has no session for the caller to tear down.
    async fn tick_art_net(
        &self,
        output: &mut OutputState,
        interface_name: &str,
        port: u16,
        timecode: TimeCode,
        frame_rate: FrameRate,
    ) -> bool {
        let art_net = match art_net_output(&mut output.art_net, interface_name).await {
            Ok(art_net) => art_net,
            Err(error) => {
                tracing::warn!(%error, interface_name, "failed to bind the timecode socket to the chosen network interface");
                return false;
            }
        };
        let destination = SocketAddrV4::new(art_net.broadcast, port);
        match artnet::send(&art_net.socket, destination, timecode, frame_rate).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, interface_name, %destination, "failed to send an Art-Net TC frame");
                false
            }
        }
    }

    /// Sends this tick's quarter-frame message and, once every 8 ticks (one
    /// full cycle, 2 frames), the value the caller should broadcast as the
    /// new [`TimecodeStatus`] — `None` on the 7 ticks in between, since the
    /// status only actually advances at the start of each cycle. That status
    /// is stopped rather than running when the cycle's first message could
    /// not be sent.
    ///
    /// A Full Timecode SysEx is sent whenever output (re)starts or the
    /// source deck / frame rate changes, detected by comparing against the
    /// session recorded on the previous tick — so a receiver can lock
    /// immediately instead of waiting on the quarter-frame stream.
    fn tick_mtc(
        &self,
        output: &mut OutputState,
        port_name: &str,
        reading: SourceReading,
        frame_rate: FrameRate,
    ) -> Option<TimecodeStatus> {
        let session = MtcSession {
            deck_id: reading.deck_id,
            frame_rate,
        };
        let is_restart = output.mtc_session != Some(session);
        output.mtc_session = Some(session);
        if is_restart {
            output.mtc_cycle = mtc::QuarterFrameCycle::new();
        }

        let quarter_frame = output.mtc_cycle.tick(reading.timecode, frame_rate);

        let sent = connected_midi(&mut output.mtc_connection, port_name).and_then(|connection| {
            if is_restart {
                let sysex = mtc::full_timecode_sysex(quarter_frame.timecode, frame_rate);
                mtc::send(connection, &sysex)?;
            }
            mtc::send(connection, &quarter_frame.bytes)
        });
        if let Err(error) = &sent {
            tracing::warn!(%error, port_name, "failed to send MTC to the chosen MIDI output port");
        }

        quarter_frame
            .is_new_cycle
            .then(|| output_status(quarter_frame.timecode, reading.deck_id, sent.is_ok(), false))
    }

    /// Tells MTC receivers where a deck that isn't advancing stands, with a
    /// Full Timecode SysEx — the standard locate message; quarter frames
    /// would say the transport is rolling. Returns whether it went out.
    ///
    /// Ends the quarter-frame session, so the stream restarts with its own
    /// Full Timecode once the deck plays again.
    fn locate_mtc(
        &self,
        output: &mut OutputState,
        port_name: &str,
        timecode: TimeCode,
        frame_rate: FrameRate,
    ) -> bool {
        output.mtc_session = None;
        let sysex = mtc::full_timecode_sysex(timecode, frame_rate);
        let sent = connected_midi(&mut output.mtc_connection, port_name)
            .and_then(|connection| mtc::send(connection, &sysex));
        if let Err(error) = &sent {
            tracing::warn!(%error, port_name, "failed to send the held MTC position to the chosen MIDI output port");
        }
        sent.is_ok()
    }

    /// Keeps the LTC audio stream alive and jam-synced. Unlike Art-Net and
    /// MTC there is no message to send here: the tick loop only lazily
    /// (re)starts the stream on a device mismatch (mirroring
    /// [`art_net_output`]/[`connected_midi`]) and hands it the deck's current
    /// timecode so its free-running audio thread can re-latch if it has
    /// drifted — see the [`ltc`] module doc for the full picture.
    ///
    /// Returns whether LTC is actually reaching the device: `false` when the
    /// stream could not start, or its device went away since.
    fn tick_ltc(
        &self,
        output: &mut OutputState,
        device_name: &str,
        reading: SourceReading,
        frame_rate: FrameRate,
    ) -> bool {
        let target = ltc::LtcTarget {
            timecode: reading.timecode,
            frame_rate,
            is_advancing: reading.is_advancing,
        };
        match ltc_stream(&mut output.ltc_stream, device_name, target) {
            Ok(stream) => {
                stream.resync(target);
                !stream.is_device_lost()
            }
            Err(error) => {
                tracing::warn!(%error, device_name, "failed to start the LTC output stream");
                false
            }
        }
    }

    /// The timecode for this instant, if the config and source deck allow
    /// producing one at all.
    fn resolve(&self, state: &GeneratorState) -> Option<SourceReading> {
        if matches!(state.config.protocol, TimecodeProtocol::Off) {
            return None;
        }
        let deck_id = state.source_deck_id?;
        // Runs on every tick (up to ~120 a second for MTC): read the source
        // deck alone, not a whole snapshot that clones every track's waveform.
        let deck = self.engine.handle().deck_state(deck_id)?;
        let playhead = extrapolate_playhead(&deck, epoch_ms())?;
        Some(SourceReading {
            deck_id,
            timecode: TimeCode::from_elapsed_ms(playhead.position_ms, state.config.frame_rate),
            is_advancing: playhead.is_advancing,
        })
    }
}

/// Where Art-Net TC goes out: a socket bound to the chosen interface, and
/// that interface's subnet broadcast address.
struct ArtNetOutput {
    interface_name: String,
    socket: UdpSocket,
    broadcast: Ipv4Addr,
}

impl ArtNetOutput {
    async fn bind(interface_name: &str) -> anyhow::Result<Self> {
        let subnet = network::resolve_ipv4_subnet(interface_name)?;
        let socket = UdpSocket::bind((subnet.ip, 0)).await?;
        socket.set_broadcast(true)?;
        Ok(Self {
            interface_name: interface_name.to_string(),
            socket,
            // The subnet's own broadcast address, as the Art-Net spec asks:
            // binding the socket alone doesn't stop 255.255.255.255 leaving
            // through another adapter (see `Ipv4Subnet::broadcast`). A subnet
            // too small to have one is a point-to-point link, where the
            // limited broadcast has only one way out anyway.
            broadcast: subnet.broadcast().unwrap_or(Ipv4Addr::BROADCAST),
        })
    }
}

/// The cached Art-Net output if it is already bound to `interface_name`,
/// otherwise a freshly bound replacement.
///
/// Rebinding on every mismatch (not just on the first call) is what lets a
/// mid-session interface switch in the settings panel take effect on the
/// very next tick.
async fn art_net_output<'a>(
    cached: &'a mut Option<ArtNetOutput>,
    interface_name: &str,
) -> anyhow::Result<&'a ArtNetOutput> {
    let is_current = matches!(cached, Some(output) if output.interface_name == interface_name);
    if !is_current {
        *cached = Some(ArtNetOutput::bind(interface_name).await?);
    }
    Ok(cached.as_ref().expect("just bound above"))
}

/// The cached MIDI connection if it is already open on `port_name`,
/// otherwise a freshly opened replacement.
///
/// Mirrors [`art_net_output`]: rebuilding on every mismatch is what lets a
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
/// Mirrors [`art_net_output`] and [`connected_midi`]: rebuilding on every
/// mismatch is what lets a mid-session device switch in the settings panel
/// take effect on the very next tick. Like [`ArtNetOutput`], and unlike the
/// MIDI connection, no wrapper tuple is needed here — [`ltc::LtcStream`]
/// already remembers its own device name.
fn ltc_stream<'a>(
    cached: &'a mut Option<ltc::LtcStream>,
    device_name: &str,
    initial_target: ltc::LtcTarget,
) -> anyhow::Result<&'a ltc::LtcStream> {
    let is_current = matches!(cached, Some(stream) if stream.device_name() == device_name);
    if !is_current {
        *cached = Some(ltc::LtcStream::start(device_name, initial_target)?);
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

/// The status for a tick that produced `timecode` from `deck_id`: running
/// only if that timecode actually went out, paused while the deck holds
/// still.
fn output_status(
    timecode: TimeCode,
    deck_id: DeckId,
    is_sent: bool,
    is_paused: bool,
) -> TimecodeStatus {
    if !is_sent {
        return stopped_status(Some(deck_id));
    }
    TimecodeStatus {
        running: true,
        paused: is_paused,
        source_deck_id: Some(deck_id),
        hours: timecode.hours,
        minutes: timecode.minutes,
        seconds: timecode.seconds,
        frames: timecode.frames,
    }
}

fn stopped_status(source_deck_id: Option<DeckId>) -> TimecodeStatus {
    TimecodeStatus {
        running: false,
        source_deck_id,
        ..TimecodeStatus::default()
    }
}

/// Cross-tick state threaded through [`TimecodeService::tick`]: the
/// Art-Net output, MIDI connection and LTC audio stream, each cached and
/// only rebuilt when what they're bound to changes, plus the MTC
/// quarter-frame cycle's own progress and which deck/frame-rate session it
/// was last latched for.
#[derive(Default)]
struct OutputState {
    art_net: Option<ArtNetOutput>,
    mtc_connection: Option<(String, midir::MidiOutputConnection)>,
    mtc_cycle: mtc::QuarterFrameCycle,
    mtc_session: Option<MtcSession>,
    ltc_stream: Option<ltc::LtcStream>,
    /// The position last sent for a deck that isn't advancing: Art-Net and
    /// MTC send it once instead of streaming it, then again only when it
    /// moves (a jump to the cue point while paused). `None` while the deck
    /// advances, and after any change to the source or config.
    held: Option<HeldPosition>,
}

impl OutputState {
    /// Whether `reading` is a held position that already went out.
    fn has_located(&self, reading: SourceReading) -> bool {
        !reading.is_advancing
            && matches!(self.held, Some(held) if held.timecode == reading.timecode && held.is_sent)
    }

    /// Records what this tick sent for `reading` and returns its status.
    fn record_send(&mut self, reading: SourceReading, is_sent: bool) -> TimecodeStatus {
        self.held = (!reading.is_advancing).then_some(HeldPosition {
            timecode: reading.timecode,
            is_sent,
        });
        output_status(
            reading.timecode,
            reading.deck_id,
            is_sent,
            !reading.is_advancing,
        )
    }
}

/// What went out for a deck holding still — see [`OutputState::held`]. A
/// failed send is kept too, so the next tick retries it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct HeldPosition {
    timecode: TimeCode,
    is_sent: bool,
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

/// A deck's playhead at one instant, as the generator reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Playhead {
    position_ms: f64,
    /// Whether the position moves forward in real time: the deck is playing
    /// and its status is recent enough to project from. `false` while it is
    /// paused or cued, and once its status stopped arriving and the position
    /// holds — no timecode streams then (see [`TimecodeStatus::paused`]).
    is_advancing: bool,
}

/// Mirrors the renderer's own extrapolation in `lib/playhead.ts`: status
/// packets land at ~5 Hz, so the position between them is projected forward
/// from the last anchor using the deck's pitch. Once status stops arriving
/// the projection holds at [`MAX_EXTRAPOLATION_MS`]: timecode freezes with
/// the deck instead of running on for a deck that may have stopped long ago.
fn extrapolate_playhead(deck: &DeckState, now_ms: f64) -> Option<Playhead> {
    let playhead_ms = deck.playhead_ms?;
    if deck.play_state != PlayState::Playing {
        return Some(Playhead {
            position_ms: playhead_ms,
            is_advancing: false,
        });
    }
    let since_status_ms = now_ms - deck.reported_at;
    let speed = 1.0 + deck.pitch / 100.0;
    Some(Playhead {
        position_ms: playhead_ms + since_status_ms.min(MAX_EXTRAPOLATION_MS) * speed,
        is_advancing: since_status_ms < MAX_EXTRAPOLATION_MS && speed > 0.0,
    })
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
            active_loop: None,
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
    fn frame_count_round_trips_every_frame_of_an_hour_at_every_rate() {
        for frame_rate in [FrameRate::Film24, FrameRate::Ebu25, FrameRate::Smpte30] {
            for total_frames in 0..frame_rate.fps() * 3600 {
                let timecode = TimeCode::from_frame_count(total_frames, frame_rate);
                assert_eq!(
                    timecode.frame_count(frame_rate),
                    total_frames,
                    "{frame_rate:?}"
                );
            }
        }
    }

    #[test]
    fn frame_count_wraps_at_24_hours() {
        let day = FrameRate::Ebu25.fps() * 24 * 3600;
        assert_eq!(
            TimeCode::from_frame_count(day + 3, FrameRate::Ebu25),
            TimeCode::from_frame_count(3, FrameRate::Ebu25)
        );
    }

    #[test]
    fn paused_deck_reports_its_playhead_without_extrapolation() {
        let deck = deck(10_000.0, PlayState::Paused, 4.0, 0.0);
        assert_eq!(
            extrapolate_playhead(&deck, 5_000.0),
            Some(Playhead {
                position_ms: 10_000.0,
                is_advancing: false
            })
        );
    }

    #[test]
    fn playing_deck_extrapolates_forward_with_pitch() {
        let deck = deck(10_000.0, PlayState::Playing, 4.0, 1_000.0);
        // 500ms of wall time at +4% pitch.
        assert_eq!(
            extrapolate_playhead(&deck, 1_500.0),
            Some(Playhead {
                position_ms: 10_520.0,
                is_advancing: true
            })
        );
    }

    #[test]
    fn a_deck_whose_status_stopped_arriving_holds_its_timecode() {
        let deck = deck(10_000.0, PlayState::Playing, 0.0, 1_000.0);
        let held = extrapolate_playhead(&deck, 1_000.0 + MAX_EXTRAPOLATION_MS)
            .expect("the deck has a playhead");
        assert!(!held.is_advancing);
        // A minute without status: the deck may have stopped long ago.
        assert_eq!(extrapolate_playhead(&deck, 61_000.0), Some(held));
    }

    #[test]
    fn a_deck_pitched_all_the_way_down_does_not_advance() {
        let deck = deck(10_000.0, PlayState::Playing, -100.0, 1_000.0);
        let playhead = extrapolate_playhead(&deck, 1_500.0).expect("the deck has a playhead");
        assert!(!playhead.is_advancing);
    }

    #[test]
    fn no_playhead_yet_means_no_timecode() {
        let mut deck = deck(0.0, PlayState::Playing, 0.0, 0.0);
        deck.playhead_ms = None;
        assert_eq!(extrapolate_playhead(&deck, 1_000.0), None);
    }

    #[test]
    fn a_tick_that_could_not_send_reports_the_clock_stopped() {
        let timecode = TimeCode::from_frame_count(1234, FrameRate::Smpte30);
        let status = output_status(timecode, 2, false, false);
        assert!(!status.running);
        assert_eq!(status.source_deck_id, Some(2));
        assert_eq!(status, stopped_status(Some(2)));
        assert!(output_status(timecode, 2, true, false).running);
    }

    fn reading(total_frames: u64, is_advancing: bool) -> SourceReading {
        SourceReading {
            deck_id: 1,
            timecode: TimeCode::from_frame_count(total_frames, FrameRate::Smpte30),
            is_advancing,
        }
    }

    #[test]
    fn a_paused_deck_sends_its_position_once_instead_of_streaming_it() {
        let mut output = OutputState::default();
        let paused = reading(900, false);
        assert!(!output.has_located(paused));

        let status = output.record_send(paused, true);
        assert!(status.running && status.paused);
        assert!(output.has_located(paused), "the next ticks send nothing");

        // A jump to the cue point while paused is a new position to send.
        assert!(!output.has_located(reading(300, false)));
    }

    #[test]
    fn a_held_position_that_failed_to_go_out_is_sent_again() {
        let mut output = OutputState::default();
        let paused = reading(900, false);
        output.record_send(paused, false);
        assert!(!output.has_located(paused));
    }

    #[test]
    fn a_playing_deck_streams_every_tick() {
        let mut output = OutputState::default();
        output.record_send(reading(900, false), true);

        let playing = reading(900, true);
        assert!(!output.has_located(playing));
        let status = output.record_send(playing, true);
        assert!(status.running && !status.paused);
        assert_eq!(output.held, None);
    }
}
