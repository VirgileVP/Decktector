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
mod deck_matches;
pub mod identity;
pub mod ltc;
pub mod mtc;
mod show;

pub use deck_matches::DeckMatchStatus;
pub use show::{
    resolve_deck, timeline_for, DeckResolution, ShowRowEntry, TimecodeShow, UnidentifiedReason,
    PENDING_TIMEOUT_MS,
};

/// Re-exported so existing callers keep writing `decktector_timecode::network::*` —
/// the lookup itself now lives in `decktector-net`, shared with the remote service.
pub use decktector_net as network;

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use deck_matches::DeckMatches;
use decktector_core::model::{DeckId, DeckState, PlayState};
use decktector_core::Engine;
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, watch};
use ts_rs::TS;

/// How many status ticks a lagging subscriber may miss before it has to
/// re-hydrate from [`TimecodeService::status`].
const STATUS_BUFFER: usize = 16;

/// How many deck-match updates a lagging subscriber may miss before it has
/// to re-hydrate from [`TimecodeService::deck_matches`]. They only change on
/// a track load, a show push or a position-quality flip.
const DECK_MATCHES_BUFFER: usize = 16;

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

/// How many seconds a timecode clock counts before it wraps to `00:00:00:00`.
const SECONDS_PER_DAY: u64 = 24 * 3600;

/// Where a track's own timeline starts, and its manual nudge.
///
/// The default — start `00:00:00:00`, no trim — is the timeline every deck
/// ran on before tracks had their own: the timecode is the playhead itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timeline {
    /// Frames from `00:00:00:00` at the output frame rate.
    pub start_frames: u64,
    /// Signed nudge added to the playhead: positive runs the timecode ahead
    /// of the music, so cues fire earlier.
    pub trim_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct TimeCode {
    hours: u8,
    minutes: u8,
    seconds: u8,
    frames: u8,
}

impl TimeCode {
    /// `timeline.start_frames` plus `elapsed_ms` of playback (nudged by the
    /// trim), wrapped at 24 hours.
    ///
    /// The playback position is floored to whole frames *before* the start is
    /// added, and the start is added as a frame count: a start never goes
    /// through milliseconds, so it lands on exactly the frame it names (see
    /// [`Self::frame_count`]). A trim can pull the position back to the start
    /// but never before it.
    fn from_timeline(timeline: Timeline, elapsed_ms: f64, frame_rate: FrameRate) -> Self {
        let fps = frame_rate.fps();
        let effective_ms = (elapsed_ms + timeline.trim_ms as f64).max(0.0);
        let elapsed_frames = ((effective_ms / 1000.0) * fps as f64) as u64;
        let frames_per_day = SECONDS_PER_DAY * fps;
        let total_frames = (timeline.start_frames % frames_per_day
            + elapsed_frames % frames_per_day)
            % frames_per_day;
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
    /// The timeline `timecode` was placed on; a change re-latches MTC
    /// receivers (see [`MtcSession`]).
    timeline: Timeline,
}

/// Everything a tick reads, as one snapshot: the source deck, the output
/// config and the show table change together or not at all, so a tick can
/// never pair a new source deck with an old table.
#[derive(Debug, Clone, Default)]
struct GeneratorState {
    source_deck_id: Option<DeckId>,
    config: TimecodeConfig,
    /// Tonight's show; `None` leaves every deck on `00:00:00:00`, as before
    /// shows existed.
    show: Option<Arc<TimecodeShow>>,
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
    deck_matches_tx: broadcast::Sender<Vec<DeckMatchStatus>>,
    last_deck_matches: Mutex<Vec<DeckMatchStatus>>,
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
            deck_matches_tx: broadcast::channel(DECK_MATCHES_BUFFER).0,
            last_deck_matches: Mutex::new(Vec::new()),
        })
    }

    pub fn set_source(&self, deck_id: Option<DeckId>) {
        self.state_tx
            .send_modify(|state| state.source_deck_id = deck_id);
    }

    pub fn set_config(&self, config: TimecodeConfig) {
        self.state_tx.send_modify(|state| state.config = config);
    }

    /// Replaces the whole show table — or drops it, back to every deck on
    /// `00:00:00:00`. One snapshot update, like the source and the config.
    pub fn set_show(&self, show: Option<TimecodeShow>) {
        self.state_tx
            .send_modify(|state| state.show = show.map(Arc::new));
    }

    /// Subscribes to deck-match updates, sent whenever any deck's changes.
    /// Pair with [`Self::deck_matches`] to hydrate a fresh listener.
    pub fn subscribe_deck_matches(&self) -> broadcast::Receiver<Vec<DeckMatchStatus>> {
        self.deck_matches_tx.subscribe()
    }

    /// Where every deck that has reported stands against the show.
    pub fn deck_matches(&self) -> Vec<DeckMatchStatus> {
        self.last_deck_matches
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
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
        self.refresh_deck_matches(output, &state, Instant::now());

        let new_status = match self.resolve(&state, output) {
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
                    if is_sent {
                        output.last_timecode = Some(reading.timecode);
                    }
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
                // Stopped: a deck that holds later has nothing current to hold.
                output.last_timecode = None;
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
    /// source deck / frame rate / timeline changes, detected by comparing
    /// against the session recorded on the previous tick — so a receiver can
    /// lock immediately instead of waiting on the quarter-frame stream.
    fn tick_mtc(
        &self,
        output: &mut OutputState,
        port_name: &str,
        reading: SourceReading,
        frame_rate: FrameRate,
    ) -> Option<TimecodeStatus> {
        let is_restart = output.latch_mtc_session(MtcSession {
            deck_id: reading.deck_id,
            frame_rate,
            timeline: reading.timeline,
        });

        let quarter_frame = output.mtc_cycle.tick(reading.timecode, frame_rate);

        let sent = connected_midi(&mut output.mtc_connection, port_name).and_then(|connection| {
            if is_restart {
                let sysex = mtc::full_timecode_sysex(quarter_frame.timecode, frame_rate);
                mtc::send(connection, &sysex)?;
            }
            mtc::send(connection, &quarter_frame.bytes)
        });
        match &sent {
            Ok(()) => output.last_timecode = Some(quarter_frame.timecode),
            Err(error) => {
                tracing::warn!(%error, port_name, "failed to send MTC to the chosen MIDI output port")
            }
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

    /// Brings every deck's match up to date and publishes the statuses when
    /// they changed. Runs whatever the protocol: the deck bars show matches
    /// while the output is off, too.
    fn refresh_deck_matches(&self, output: &mut OutputState, state: &GeneratorState, now: Instant) {
        let handle = self.engine.handle();
        let Some(statuses) = output.deck_matches.refresh(
            handle,
            &handle.deck_states(),
            state.show.as_deref(),
            state.config.frame_rate,
            now,
        ) else {
            return;
        };
        statuses.clone_into(
            &mut self
                .last_deck_matches
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        // No subscribers yet (window still opening) is not an error.
        let _ = self.deck_matches_tx.send(statuses);
    }

    /// The timecode for this instant, if the config and source deck allow
    /// producing one at all.
    ///
    /// The source deck's match decides which timeline it runs on. A deck
    /// still identifying its track, or cut by the show, holds the last
    /// timecode sent — frozen, like a paused deck — or stops if nothing went
    /// out yet; an empty deck or a frame-rate mismatch stops the output.
    fn resolve(&self, state: &GeneratorState, output: &OutputState) -> Option<SourceReading> {
        if matches!(state.config.protocol, TimecodeProtocol::Off) {
            return None;
        }
        let deck_id = state.source_deck_id?;
        // Runs on every tick (up to ~120 a second for MTC): read the source
        // deck alone, not a whole snapshot that clones every track's waveform.
        let deck = self.engine.handle().deck_state(deck_id)?;
        let resolution = output.deck_matches.resolution(deck_id);
        match resolution {
            Some(DeckResolution::Pending | DeckResolution::Cut { .. }) => {
                return Some(SourceReading {
                    deck_id,
                    timecode: output.last_timecode?,
                    is_advancing: false,
                    timeline: Timeline::default(),
                });
            }
            Some(DeckResolution::None | DeckResolution::RateMismatch) => return None,
            _ => {}
        }
        let playhead = extrapolate_playhead(&deck, epoch_ms())?;
        let timeline = match (state.show.as_deref(), resolution) {
            (Some(show), Some(resolution)) => timeline_for(show, resolution),
            _ => Timeline::default(),
        };
        Some(SourceReading {
            deck_id,
            timecode: TimeCode::from_timeline(
                timeline,
                playhead.position_ms,
                state.config.frame_rate,
            ),
            is_advancing: playhead.is_advancing,
            timeline,
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
    /// The timecode that last actually went out, on any protocol: what a
    /// deck holds while it identifies its track. `None` once output stops.
    last_timecode: Option<TimeCode>,
    deck_matches: DeckMatches,
}

impl OutputState {
    /// Whether `reading` is a held position that already went out.
    fn has_located(&self, reading: SourceReading) -> bool {
        !reading.is_advancing
            && matches!(self.held, Some(held) if held.timecode == reading.timecode && held.is_sent)
    }

    /// Makes `session` the one the MTC stream describes and returns whether
    /// that is a restart — a different session from last tick's, or none at
    /// all — in which case the quarter-frame cycle starts over and the caller
    /// owes receivers a Full Timecode SysEx.
    fn latch_mtc_session(&mut self, session: MtcSession) -> bool {
        let is_restart = self.mtc_session != Some(session);
        self.mtc_session = Some(session);
        if is_restart {
            self.mtc_cycle = mtc::QuarterFrameCycle::new();
        }
        is_restart
    }

    /// Records what this tick sent for `reading` and returns its status.
    fn record_send(&mut self, reading: SourceReading, is_sent: bool) -> TimecodeStatus {
        self.held = (!reading.is_advancing).then_some(HeldPosition {
            timecode: reading.timecode,
            is_sent,
        });
        if is_sent {
            self.last_timecode = Some(reading.timecode);
        }
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
/// tick-to-tick to detect a source deck, frame-rate or timeline change, which
/// needs an immediate re-latch and a fresh Full Timecode SysEx. The timeline
/// is part of the key because a new track on the *same* deck jumps the
/// timecode without changing the deck or the rate.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MtcSession {
    deck_id: DeckId,
    frame_rate: FrameRate,
    timeline: Timeline,
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
            loaded_track: None,
            playhead_ms: Some(playhead_ms),
            position_quality: None,
            reported_at,
            active_loop: None,
        }
    }

    #[test]
    fn timecode_rolls_over_at_60_seconds_and_60_minutes() {
        let timecode = TimeCode::from_timeline(Timeline::default(), 61_500.0, FrameRate::Smpte30);
        assert_eq!(timecode.minutes, 1);
        assert_eq!(timecode.seconds, 1);
        assert_eq!(timecode.frames, 15);
    }

    fn timecode(hours: u8, minutes: u8, seconds: u8, frames: u8) -> TimeCode {
        TimeCode {
            hours,
            minutes,
            seconds,
            frames,
        }
    }

    fn timeline_from(start: TimeCode, trim_ms: i64, frame_rate: FrameRate) -> Timeline {
        Timeline {
            start_frames: start.frame_count(frame_rate),
            trim_ms,
        }
    }

    #[test]
    fn a_start_offset_is_added_in_frames_not_milliseconds() {
        let rate = FrameRate::Ebu25;
        let timeline = timeline_from(timecode(1, 0, 0, 0), 0, rate);
        assert_eq!(
            TimeCode::from_timeline(timeline, 1_000.0, rate),
            timecode(1, 0, 1, 0)
        );
        assert_eq!(
            TimeCode::from_timeline(timeline, 999.0, rate),
            timecode(1, 0, 0, 24),
            "a position short of a frame boundary stays on the previous frame"
        );
    }

    #[test]
    fn the_timeline_wraps_at_24_hours() {
        let rate = FrameRate::Ebu25;
        let timeline = timeline_from(timecode(23, 59, 59, 0), 0, rate);
        assert_eq!(
            TimeCode::from_timeline(timeline, 2_000.0, rate),
            timecode(0, 0, 1, 0)
        );
    }

    #[test]
    fn a_negative_trim_never_goes_below_the_start() {
        let rate = FrameRate::Ebu25;
        let timeline = timeline_from(timecode(0, 10, 0, 0), -5_000, rate);
        assert_eq!(
            TimeCode::from_timeline(timeline, 2_000.0, rate),
            timecode(0, 10, 0, 0)
        );
    }

    #[test]
    fn a_positive_trim_runs_the_timecode_ahead_of_the_music() {
        let rate = FrameRate::Ebu25;
        let timeline = timeline_from(timecode(0, 0, 0, 0), 80, rate);
        assert_eq!(
            TimeCode::from_timeline(timeline, 0.0, rate),
            timecode(0, 0, 0, 2)
        );
    }

    #[test]
    fn the_default_timeline_matches_the_old_behaviour() {
        // The pre-timeline formula, kept here as the reference it must match.
        fn from_elapsed_ms(elapsed_ms: f64, frame_rate: FrameRate) -> TimeCode {
            let total_frames = ((elapsed_ms.max(0.0) / 1000.0) * frame_rate.fps() as f64) as u64;
            TimeCode::from_frame_count(total_frames, frame_rate)
        }

        let positions_ms = [
            -250.0,
            0.0,
            33.3,
            999.0,
            1_000.0,
            61_500.0,
            3_599_999.9,
            86_399_960.0,
            86_400_000.0,
            90_061_234.5,
        ];
        for frame_rate in [FrameRate::Film24, FrameRate::Ebu25, FrameRate::Smpte30] {
            for position_ms in positions_ms {
                assert_eq!(
                    TimeCode::from_timeline(Timeline::default(), position_ms, frame_rate),
                    from_elapsed_ms(position_ms, frame_rate),
                    "{frame_rate:?} at {position_ms} ms"
                );
            }
        }
    }

    #[test]
    fn mtc_restarts_when_the_start_timecode_changes_on_the_same_deck() {
        let rate = FrameRate::Ebu25;
        let first_track = MtcSession {
            deck_id: 1,
            frame_rate: rate,
            timeline: timeline_from(timecode(0, 10, 0, 0), 0, rate),
        };
        let next_track = MtcSession {
            timeline: timeline_from(timecode(0, 20, 0, 0), 0, rate),
            ..first_track
        };
        assert_ne!(first_track, next_track);

        let mut output = OutputState::default();
        assert!(output.latch_mtc_session(first_track), "output starts");
        assert!(
            !output.latch_mtc_session(first_track),
            "the same track keeps streaming"
        );
        assert!(
            output.latch_mtc_session(next_track),
            "a new start on the same deck and rate re-latches receivers"
        );
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
            timeline: Timeline::default(),
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

    /// A service running on a simulated engine that is never started: tests
    /// report deck states and metadata through its handle themselves.
    mod with_a_show {
        use super::*;
        use decktector_core::model::{TrackInfo, TrackRef};
        use decktector_core::sim::{SimEngine, SimLibrary};
        use decktector_core::EngineHandle;

        const RATE: FrameRate = FrameRate::Ebu25;
        const FPS: u64 = 25;
        const FRAMES_PER_HOUR: u64 = 3600 * FPS;
        /// Every loaded deck sits paused two seconds into its track.
        const PLAYHEAD_MS: f64 = 2_000.0;
        const INTRO: u32 = 14;
        const STROBE: u32 = 15;
        const UNKNOWN: u32 = 99;

        fn track_ref(track_id: u32) -> TrackRef {
            TrackRef {
                source_device: 1,
                slot: "usb".to_owned(),
                track_id,
            }
        }

        fn title_of(track_id: u32) -> &'static str {
            match track_id {
                INTRO => "Intro",
                STROBE => "Strobe",
                _ => "Not in the show",
            }
        }

        fn metadata(track_id: u32) -> TrackInfo {
            TrackInfo {
                track_id,
                source: Some(track_ref(track_id)),
                title: title_of(track_id).to_owned(),
                artist: Some("Artist".to_owned()),
                album: None,
                genre: None,
                key: None,
                label: None,
                comment: None,
                duration_ms: 300_000.0,
                bpm: None,
                artwork: None,
                beat_grid: None,
                waveform: None,
                cues: Vec::new(),
                events: Vec::new(),
            }
        }

        fn row(id: &str, track_id: u32, start_hours: u64) -> ShowRowEntry {
            ShowRowEntry {
                id: id.to_owned(),
                title: title_of(track_id).to_owned(),
                artist: "Artist".to_owned(),
                duration_seconds: 300,
                start_frames: Some(start_hours * FRAMES_PER_HOUR),
                trim_ms: 0,
            }
        }

        /// Intro at 02:00:00:00, Strobe at 01:00:00:00, fallback at 23:00:00:00.
        fn table(revision: u64, cut_when_unidentified: bool) -> TimecodeShow {
            TimecodeShow {
                revision,
                frame_rate: RATE,
                fallback_start_frames: 23 * FRAMES_PER_HOUR,
                cut_when_unidentified,
                rows: vec![row("intro", INTRO, 2), row("strobe", STROBE, 1)],
            }
        }

        /// The timecode two seconds into a timeline starting at `start_hours`.
        fn two_seconds_after(start_hours: u64) -> TimeCode {
            TimeCode::from_frame_count(start_hours * FRAMES_PER_HOUR + 2 * FPS, RATE)
        }

        struct Rig {
            engine: Arc<SimEngine>,
            service: Arc<TimecodeService>,
            output: OutputState,
            started_at: Instant,
        }

        impl Rig {
            /// Art-Net output at 25 fps from deck 1, with `show` pushed.
            fn new(show: Option<TimecodeShow>) -> Self {
                let engine = Arc::new(SimEngine::with_library(1, SimLibrary::default()));
                let service = TimecodeService::new(Arc::clone(&engine) as Arc<dyn Engine>);
                service.set_config(TimecodeConfig {
                    protocol: TimecodeProtocol::ArtNet {
                        interface_name: "booth".to_owned(),
                        port: 6454,
                    },
                    frame_rate: RATE,
                });
                service.set_source(Some(1));
                service.set_show(show);
                Self {
                    engine,
                    service,
                    output: OutputState::default(),
                    started_at: Instant::now(),
                }
            }

            fn handle(&self) -> &EngineHandle {
                self.engine.handle()
            }

            /// Deck 1 reports `track_id` loaded; its metadata is left as it was.
            fn load(&self, track_id: u32) {
                self.handle().update_deck_state(DeckState {
                    track_id: Some(track_id),
                    loaded_track: Some(track_ref(track_id)),
                    ..deck(PLAYHEAD_MS, PlayState::Paused, 0.0, 0.0)
                });
            }

            /// Deck 1's metadata for `track_id` lands.
            fn identify(&self, track_id: u32) {
                self.handle().update_deck_track(1, Some(metadata(track_id)));
            }

            /// One tick `after_ms` into the test: what would go out.
            fn tick_at(&mut self, after_ms: u64) -> Option<SourceReading> {
                let state = self.service.state_rx.borrow().clone();
                let now = self.started_at + Duration::from_millis(after_ms);
                self.service
                    .refresh_deck_matches(&mut self.output, &state, now);
                self.service.resolve(&state, &self.output)
            }

            /// A tick whose timecode went out.
            fn send_at(&mut self, after_ms: u64) -> SourceReading {
                let reading = self.tick_at(after_ms).expect("a timecode to send");
                self.output.record_send(reading, true);
                reading
            }
        }

        fn held(reading: Option<SourceReading>) -> Option<TimeCode> {
            reading
                .filter(|reading| !reading.is_advancing)
                .map(|reading| reading.timecode)
        }

        #[test]
        fn no_show_keeps_the_timecode_on_the_playhead() {
            let mut rig = Rig::new(None);
            rig.load(STROBE);
            assert_eq!(rig.send_at(0).timecode, two_seconds_after(0));
        }

        #[test]
        fn a_pending_deck_holds_the_last_timecode_sent_and_then_jumps_to_the_correct_one() {
            let mut rig = Rig::new(Some(table(1, false)));
            rig.load(INTRO);
            rig.identify(INTRO);
            let intro = rig.send_at(0).timecode;
            assert_eq!(intro, two_seconds_after(2));

            // Strobe loads; the deck still shows the Intro's metadata.
            rig.load(STROBE);
            assert_eq!(held(rig.tick_at(100)), Some(intro));

            rig.identify(STROBE);
            assert_eq!(
                rig.tick_at(200).map(|reading| reading.timecode),
                Some(two_seconds_after(1))
            );
        }

        #[test]
        fn a_pending_deck_with_nothing_sent_yet_is_stopped_not_zero() {
            let mut rig = Rig::new(Some(table(1, false)));
            rig.load(STROBE);
            assert_eq!(rig.tick_at(0), None);
        }

        #[test]
        fn a_rate_mismatch_sends_no_timecode_until_the_rates_match() {
            let show_at_30 = TimecodeShow {
                frame_rate: FrameRate::Smpte30,
                ..table(1, false)
            };
            let mut rig = Rig::new(Some(show_at_30));
            rig.load(STROBE);
            rig.identify(STROBE);
            assert_eq!(rig.tick_at(0), None);

            // Read first: a borrow still alive inside `set_config` would deadlock it.
            let config = rig.service.state_rx.borrow().config.clone();
            rig.service.set_config(TimecodeConfig {
                frame_rate: FrameRate::Smpte30,
                ..config
            });
            assert!(rig.tick_at(100).is_some());
        }

        #[test]
        fn a_deck_pending_past_the_timeout_runs_from_the_fallback_start() {
            let mut rig = Rig::new(Some(table(1, false)));
            rig.load(STROBE);
            assert_eq!(rig.tick_at(0), None);
            assert_eq!(rig.tick_at(PENDING_TIMEOUT_MS - 1), None);
            assert_eq!(
                rig.tick_at(PENDING_TIMEOUT_MS)
                    .map(|reading| reading.timecode),
                Some(two_seconds_after(23))
            );
        }

        #[test]
        fn a_deck_pending_past_the_timeout_is_held_when_the_cut_switch_is_on() {
            let mut rig = Rig::new(Some(table(1, true)));
            rig.load(INTRO);
            rig.identify(INTRO);
            let intro = rig.send_at(0).timecode;

            rig.load(STROBE);
            assert_eq!(held(rig.tick_at(100)), Some(intro));
            assert_eq!(held(rig.tick_at(100 + PENDING_TIMEOUT_MS)), Some(intro));
        }

        #[test]
        fn late_metadata_after_the_timeout_jumps_once_to_the_matched_timecode() {
            let mut rig = Rig::new(Some(table(1, false)));
            rig.load(STROBE);
            rig.tick_at(0);
            let fallback = rig.send_at(PENDING_TIMEOUT_MS);
            assert_eq!(fallback.timecode, two_seconds_after(23));

            rig.identify(STROBE);
            let matched = rig.send_at(PENDING_TIMEOUT_MS + 10_000);
            assert_eq!(matched.timecode, two_seconds_after(1));
            assert_ne!(matched.timeline, fallback.timeline);
            assert_eq!(
                rig.tick_at(PENDING_TIMEOUT_MS + 10_100),
                Some(matched),
                "and it stays there"
            );
        }

        #[test]
        fn an_unidentified_deck_runs_from_the_fallback_start() {
            let mut rig = Rig::new(Some(table(1, false)));
            rig.load(UNKNOWN);
            rig.identify(UNKNOWN);
            assert_eq!(
                rig.tick_at(0).map(|reading| reading.timecode),
                Some(two_seconds_after(23))
            );
        }

        #[test]
        fn an_unidentified_deck_is_held_when_the_cut_switch_is_on() {
            let mut rig = Rig::new(Some(table(1, true)));
            rig.load(INTRO);
            rig.identify(INTRO);
            let intro = rig.send_at(0).timecode;

            rig.load(UNKNOWN);
            rig.identify(UNKNOWN);
            assert_eq!(held(rig.tick_at(100)), Some(intro));
        }

        #[test]
        fn an_ambiguous_deck_follows_the_same_fallback_or_cut_rule() {
            let twice = |cut_when_unidentified| TimecodeShow {
                rows: vec![row("strobe", STROBE, 1), row("strobe-again", STROBE, 3)],
                ..table(1, cut_when_unidentified)
            };

            let mut falling_back = Rig::new(Some(twice(false)));
            falling_back.load(STROBE);
            falling_back.identify(STROBE);
            assert_eq!(
                falling_back.tick_at(0).map(|reading| reading.timecode),
                Some(two_seconds_after(23))
            );

            let mut cut = Rig::new(Some(twice(true)));
            cut.load(STROBE);
            cut.identify(STROBE);
            assert_eq!(cut.tick_at(0), None, "cut with nothing sent yet");
        }

        #[test]
        fn loading_a_new_track_on_the_source_deck_restarts_the_mtc_session() {
            let mut rig = Rig::new(Some(table(1, false)));
            rig.load(INTRO);
            rig.identify(INTRO);
            let intro = rig.send_at(0);
            rig.load(STROBE);
            rig.identify(STROBE);
            let strobe = rig.send_at(100);

            let session = |reading: SourceReading| MtcSession {
                deck_id: reading.deck_id,
                frame_rate: RATE,
                timeline: reading.timeline,
            };
            assert!(rig.output.latch_mtc_session(session(intro)));
            assert!(rig.output.latch_mtc_session(session(strobe)));
        }

        #[test]
        fn changing_the_source_and_the_table_in_one_tick_never_mixes_old_and_new() {
            let rig = Rig::new(Some(table(1, false)));
            let before = rig.service.state_rx.borrow().clone();

            rig.service.set_source(Some(2));
            rig.service.set_show(Some(table(2, true)));
            let after = rig.service.state_rx.borrow().clone();

            assert_eq!(before.source_deck_id, Some(1));
            assert_eq!(before.show.as_deref().map(|show| show.revision), Some(1));
            assert_eq!(after.source_deck_id, Some(2));
            assert_eq!(after.show.as_deref().map(|show| show.revision), Some(2));
        }

        #[test]
        fn trim_edits_apply_on_the_next_tick_without_a_rematch() {
            let mut rig = Rig::new(Some(table(1, false)));
            rig.load(STROBE);
            rig.identify(STROBE);
            assert_eq!(rig.send_at(0).timecode, two_seconds_after(1));

            // +80 ms is two frames at 25 fps; the copy would make a fresh match ambiguous.
            let mut trimmed = table(2, false);
            trimmed.rows[1].trim_ms = 80;
            trimmed.rows.push(row("strobe-copy", STROBE, 5));
            rig.service.set_show(Some(trimmed));

            let reading = rig.tick_at(100).expect("still on its row");
            assert_eq!(
                reading.timecode,
                TimeCode::from_frame_count(FRAMES_PER_HOUR + 2 * FPS + 2, RATE)
            );
        }
    }
}
