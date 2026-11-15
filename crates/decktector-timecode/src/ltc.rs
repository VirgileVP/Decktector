//! Linear Timecode (LTC) encoding and output.
//!
//! Unlike Art-Net TC and MTC, LTC has no natural fit in the tick-loop
//! pattern used elsewhere in this crate: it is not message-based, it's a
//! continuous audio-frequency biphase-mark-encoded square wave, output
//! through a sound device at a bit rate of `fps * 80` bits/second (80 bits
//! per LTC frame, always — the frame content, not the bit count, changes
//! with frame rate).
//!
//! The audio side runs on its own dedicated OS thread owning a
//! [`cpal::Stream`] — `cpal::Stream` is `!Send` on most platforms, so it
//! cannot live inside [`crate::TimecodeService`]'s async task. That thread
//! free-runs its own frame counter, one frame per LTC frame of audio, and
//! never asks the deck for its position directly. Instead the tick loop
//! (see [`crate::TimecodeService::tick`]'s `Ltc` branch) periodically hands
//! it the deck's real extrapolated timecode through [`LtcLatch`], the only
//! thing shared between the two threads. While the deck plays, the audio
//! thread jam-syncs onto that target at its next frame boundary if the
//! free-running counter has drifted more than [`MAX_DRIFT_FRAMES`] from it;
//! while the deck is not advancing, it plays silence — see
//! [`FrameSequencer`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{FrameRate, TimeCode};

/// Every LTC frame is 80 bits long, whatever the frame rate.
const BITS_PER_FRAME: usize = 80;

/// The fixed 16-bit pattern that closes every LTC frame (bits 64-79, bit 64
/// sent first). This exact pattern never occurs elsewhere in valid
/// biphase-encoded LTC, which is what lets a receiver find frame
/// boundaries.
const SYNC_WORD_BITS: [bool; 16] = [
    false, false, true, true, true, true, true, true, true, true, true, true, true, true, false,
    true,
];

/// How far the free-running frame counter may stray from the deck's real
/// position before the audio thread jam-syncs onto it. One frame absorbs the
/// target's own rounding to whole frames without letting a real drift
/// through.
const MAX_DRIFT_FRAMES: u64 = 1;

/// A `1` bit flips mid-bit, so a bit needs at least two samples to carry it.
const MIN_SAMPLES_PER_BIT: f64 = 2.0;

/// What the device plays while the deck holds still: no LTC at all.
const SILENCE: f32 = 0.0;

/// Every audio output device this machine could send LTC to.
pub fn list_output_devices() -> anyhow::Result<Vec<String>> {
    let host = cpal::default_host();
    host.output_devices()?
        .map(|device| device.name().map_err(anyhow::Error::from))
        .collect()
}

/// Sets `width` bits starting at `start`, least-significant bit first — the
/// bit order every BCD field and binary field in the LTC frame uses.
fn set_bits(bits: &mut [bool; BITS_PER_FRAME], start: usize, width: usize, value: u8) {
    for i in 0..width {
        bits[start + i] = (value >> i) & 1 != 0;
    }
}

/// The 80-bit LTC frame for `timecode`.
///
/// `frame_rate` carries no weight here: unlike MTC, LTC's 80 bits have no
/// rate field of their own (a receiver infers the rate from the bit
/// duration, i.e. from the sample generator, not from the frame content).
/// It stays a parameter for symmetry with the rest of this crate and in
/// case a future revision needs it (e.g. a drop-frame flag).
///
/// Layout (bit 0 sent first), standard SMPTE/EBU non-drop LTC:
/// - 0-3: frame units (BCD); 4-7: user bits 1
/// - 8-9: frame tens (BCD); 10: drop-frame flag (always 0 — no drop-frame
///   rate in this codebase, see [`FrameRate::art_net_type`]); 11: color-frame
///   flag (always 0)
/// - 12-15: user bits 2; 16-19: seconds units (BCD); 20-23: user bits 3
/// - 24-26: seconds tens; 27: unused; 28-31: user bits 4
/// - 32-35: minutes units (BCD); 36-39: user bits 5; 40-42: minutes tens
/// - 43: unused; 44-47: user bits 6; 48-51: hours units (BCD)
/// - 52-55: user bits 7; 56-57: hours tens; 58-59: unused; 60-63: user bits 8
/// - 64-79: sync word ([`SYNC_WORD_BITS`])
fn frame_bits(timecode: TimeCode, _frame_rate: FrameRate) -> [bool; BITS_PER_FRAME] {
    let mut bits = [false; BITS_PER_FRAME];
    set_bits(&mut bits, 0, 4, timecode.frames % 10);
    set_bits(&mut bits, 8, 2, timecode.frames / 10);
    set_bits(&mut bits, 16, 4, timecode.seconds % 10);
    set_bits(&mut bits, 24, 3, timecode.seconds / 10);
    set_bits(&mut bits, 32, 4, timecode.minutes % 10);
    set_bits(&mut bits, 40, 3, timecode.minutes / 10);
    set_bits(&mut bits, 48, 4, timecode.hours % 10);
    set_bits(&mut bits, 56, 2, timecode.hours / 10);
    bits[64..80].copy_from_slice(&SYNC_WORD_BITS);
    bits
}

/// How many audio samples one LTC bit lasts at `sample_rate` — rarely a
/// whole number, see [`BitClock`].
fn samples_per_bit(sample_rate: u32, frame_rate: FrameRate) -> f64 {
    let bits_per_second = (frame_rate.fps() * BITS_PER_FRAME as u64) as f64;
    (f64::from(sample_rate) / bits_per_second).max(MIN_SAMPLES_PER_BIT)
}

/// Splits the LTC bit stream into whole audio samples while keeping its
/// average bit duration exact.
///
/// One bit lasts `sample_rate / (fps * 80)` samples: 18.375 at 44.1 kHz and
/// 30 fps. Rounding that per bit would run the whole signal fast — 2% there,
/// 4.4% at 24 fps, close enough to the 25 fps bit rate to fool a receiver —
/// so each bit gets the samples that fall inside its exact time span instead,
/// and the fraction left over carries into the next bit.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BitClock {
    samples_per_bit: f64,
    /// How far, in samples, the current bit's exact start lies before the
    /// first sample it is given — always in `0.0..1.0`.
    lag: f64,
}

impl BitClock {
    fn new(sample_rate: u32, frame_rate: FrameRate) -> Self {
        Self::with_samples_per_bit(samples_per_bit(sample_rate, frame_rate))
    }

    fn with_samples_per_bit(samples_per_bit: f64) -> Self {
        Self {
            samples_per_bit,
            lag: 0.0,
        }
    }

    /// Follows a frame-rate change without breaking the bit stream.
    fn set_rate(&mut self, sample_rate: u32, frame_rate: FrameRate) {
        self.samples_per_bit = samples_per_bit(sample_rate, frame_rate);
    }

    /// The next bit's length in samples, and the index (within it) of the
    /// first sample of its second half — where a `1` bit flips.
    fn next_bit(&mut self) -> (usize, usize) {
        let end = self.samples_per_bit - self.lag;
        let length = end.ceil();
        let midpoint = (self.samples_per_bit / 2.0 - self.lag).ceil().max(1.0);
        self.lag = length - end;
        (length as usize, midpoint as usize)
    }

    /// The next whole frame's length in samples, for a frame that carries
    /// no bits (silence) but must keep the stream's timing.
    fn next_frame_length(&mut self) -> usize {
        (0..BITS_PER_FRAME).map(|_| self.next_bit().0).sum()
    }
}

/// Encodes `bits` as biphase-mark samples, timed by `bit_clock`. `level` is
/// the polarity in effect just before this call and is updated in place to
/// the polarity in effect after the last sample — so callers can chain
/// successive frames back-to-back with a transition preserved at every bit
/// boundary, including the seam between calls.
///
/// Biphase-mark rule: there is always a transition at the start of a bit
/// period; a `1` bit has an additional transition at the bit's midpoint, a
/// `0` bit has none.
fn biphase_samples(bits: &[bool], bit_clock: &mut BitClock, level: &mut f32) -> Vec<f32> {
    let mut samples = Vec::with_capacity(bits.len() * bit_clock.samples_per_bit.ceil() as usize);
    for &bit in bits {
        let (length, midpoint) = bit_clock.next_bit();
        *level = -*level;
        samples.push(*level);
        for i in 1..length {
            if bit && i == midpoint {
                *level = -*level;
            }
            samples.push(*level);
        }
    }
    samples
}

fn next_timecode(timecode: TimeCode, frame_rate: FrameRate) -> TimeCode {
    TimeCode::from_frame_count(timecode.frame_count(frame_rate) + 1, frame_rate)
}

/// How many whole frames apart `a` and `b` are, regardless of sign.
fn frames_apart(a: TimeCode, b: TimeCode, frame_rate: FrameRate) -> u64 {
    a.frame_count(frame_rate)
        .abs_diff(b.frame_count(frame_rate))
}

/// What the LTC output should be saying, as handed over by the tick loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LtcTarget {
    pub(crate) timecode: TimeCode,
    pub(crate) frame_rate: FrameRate,
    /// Whether the deck's timecode moves forward in real time — see
    /// [`crate::Playhead::is_advancing`].
    pub(crate) is_advancing: bool,
}

/// Jam-sync target shared between the async tick loop and the audio thread:
/// the tick loop's latest [`LtcTarget`] and the instant it was handed over.
///
/// The tick loop only ever touches this through a `Mutex`, never the
/// `cpal::Stream` directly — that is what keeps everything the async side
/// holds `Send`.
#[derive(Debug, Clone, Copy)]
struct LtcLatch {
    target: LtcTarget,
    set_at: Instant,
}

impl LtcLatch {
    fn new(target: LtcTarget, set_at: Instant) -> Self {
        Self { target, set_at }
    }

    /// Where the deck should be at `now`: the target projected forward in
    /// real time while it advances, the target itself while it doesn't.
    fn timecode_at(&self, now: Instant) -> TimeCode {
        let LtcTarget {
            timecode,
            frame_rate,
            is_advancing,
        } = self.target;
        if !is_advancing {
            return timecode;
        }
        let elapsed = now.saturating_duration_since(self.set_at);
        let elapsed_frames = (elapsed.as_secs_f64() * frame_rate.fps() as f64) as u64;
        TimeCode::from_frame_count(
            timecode.frame_count(frame_rate) + elapsed_frames,
            frame_rate,
        )
    }
}

/// Decides which timecode each successive LTC frame carries, if any.
///
/// Counts on one frame at a time while the deck plays — an LTC reader
/// expects every frame in turn — and jam-syncs onto the deck's real position
/// once the count has strayed more than [`MAX_DRIFT_FRAMES`] from it. While
/// the deck isn't advancing there is no frame at all: a stopped transport
/// sends no LTC, which is how a reader knows it stopped.
#[derive(Debug, Clone, Copy, Default)]
struct FrameSequencer {
    /// The frame last sent, `None` after silence (or before the first one).
    current: Option<TimeCode>,
}

impl FrameSequencer {
    /// The timecode of the frame that starts at `now`, `None` for a frame's
    /// worth of silence.
    fn next_frame(&mut self, latch: &LtcLatch, now: Instant) -> Option<TimeCode> {
        if !latch.target.is_advancing {
            self.current = None;
            return None;
        }
        let target = latch.timecode_at(now);
        let frame_rate = latch.target.frame_rate;
        let next = match self.current {
            // Playing again (or starting): pick up exactly where the deck is.
            None => target,
            Some(previous) => {
                let counted = next_timecode(previous, frame_rate);
                if frames_apart(counted, target, frame_rate) > MAX_DRIFT_FRAMES {
                    target
                } else {
                    counted
                }
            }
        };
        self.current = Some(next);
        self.current
    }
}

/// Owns the dedicated OS thread that free-runs LTC audio output to a chosen
/// device.
///
/// `pub(crate)`, not `pub`: like the MIDI connection [`crate::mtc::connect`]
/// opens, this is only ever built and held by `lib.rs`'s tick loop, never by
/// another crate.
pub(crate) struct LtcStream {
    latch: Arc<Mutex<LtcLatch>>,
    stop: Arc<AtomicBool>,
    device_lost: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    device_name: String,
}

impl LtcStream {
    /// Spawns the audio thread and builds/plays the `cpal::Stream` on it,
    /// blocking until the stream is confirmed playing (or has failed) so
    /// callers get the same synchronous success/failure signal as
    /// [`crate::mtc::connect`].
    pub(crate) fn start(device_name: &str, initial_target: LtcTarget) -> anyhow::Result<Self> {
        let latch = Arc::new(Mutex::new(LtcLatch::new(initial_target, Instant::now())));
        let stop = Arc::new(AtomicBool::new(false));
        let device_lost = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();

        let thread_latch = Arc::clone(&latch);
        let thread_stop = Arc::clone(&stop);
        let thread_device_lost = Arc::clone(&device_lost);
        let owned_device_name = device_name.to_string();
        let thread = thread::Builder::new()
            .name("decktector-ltc".into())
            .spawn(move || {
                run_audio_thread(
                    &owned_device_name,
                    thread_latch,
                    thread_stop,
                    thread_device_lost,
                    ready_tx,
                )
            })
            .map_err(|error| anyhow::anyhow!("failed to spawn the LTC audio thread: {error}"))?;

        ready_rx.recv().map_err(|_| {
            anyhow::anyhow!("the LTC audio thread exited before starting its stream")
        })??;

        Ok(Self {
            latch,
            stop,
            device_lost,
            thread: Some(thread),
            device_name: device_name.to_string(),
        })
    }

    pub(crate) fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Whether the device went away after the stream started: its audio no
    /// longer reaches anything, though the thread lives on until dropped.
    pub(crate) fn is_device_lost(&self) -> bool {
        self.device_lost.load(Ordering::Relaxed)
    }

    /// Updates the shared jam-sync target, picked up by the audio thread at
    /// its next frame boundary — see [`FrameSequencer`].
    ///
    /// The frame rate travels with every call (mirroring how it is passed on
    /// every Art-Net/MTC tick too) so a frame-rate change while LTC is
    /// running takes effect without rebuilding the stream.
    pub(crate) fn resync(&self, target: LtcTarget) {
        *self
            .latch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            LtcLatch::new(target, Instant::now());
    }
}

impl Drop for LtcStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

/// Body of the dedicated LTC audio thread: builds and plays the stream,
/// reports the outcome through `ready`, then parks until told to stop.
fn run_audio_thread(
    device_name: &str,
    latch: Arc<Mutex<LtcLatch>>,
    stop: Arc<AtomicBool>,
    device_lost: Arc<AtomicBool>,
    ready: mpsc::Sender<anyhow::Result<()>>,
) {
    let stream = match build_stream(device_name, &latch, device_lost) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if let Err(error) = stream.play() {
        let _ = ready.send(Err(error.into()));
        return;
    }
    let _ = ready.send(Ok(()));

    while !stop.load(Ordering::Relaxed) {
        thread::park_timeout(Duration::from_millis(100));
    }
    // `stream` drops here, stopping playback before the thread exits.
}

/// Finds `device_name` and builds a playing-ready output stream for it, in
/// whatever sample format the device natively supports.
fn build_stream(
    device_name: &str,
    latch: &Arc<Mutex<LtcLatch>>,
    device_lost: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .output_devices()?
        .find(|candidate| {
            candidate
                .name()
                .map(|name| name == device_name)
                .unwrap_or(false)
        })
        .ok_or_else(|| anyhow::anyhow!("LTC output device '{device_name}' not found"))?;

    let supported = device.default_output_config()?;
    let sample_format = supported.sample_format();
    let stream_config: cpal::StreamConfig = supported.into();
    let sample_rate = stream_config.sample_rate.0;
    let channels = stream_config.channels as usize;

    let initial_frame_rate = latch
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .target
        .frame_rate;
    let mut sequencer = FrameSequencer::default();
    let mut bit_clock = BitClock::new(sample_rate, initial_frame_rate);
    let mut level = -1.0_f32;
    // Empty, so the very first sample generates the first frame.
    let mut buffer: Vec<f32> = Vec::new();
    let mut cursor = 0usize;
    let shared_latch = Arc::clone(latch);

    // Pulls one sample at a time, generating the next frame's buffer (and
    // jam-syncing against the latch) whenever the current one runs out.
    let generator = move || -> f32 {
        if cursor >= buffer.len() {
            let latch = *shared_latch
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let frame_rate = latch.target.frame_rate;
            bit_clock.set_rate(sample_rate, frame_rate);
            buffer = match sequencer.next_frame(&latch, Instant::now()) {
                Some(timecode) => biphase_samples(
                    &frame_bits(timecode, frame_rate),
                    &mut bit_clock,
                    &mut level,
                ),
                // One frame's length of silence, then the latch is read again.
                None => vec![SILENCE; bit_clock.next_frame_length()],
            };
            cursor = 0;
        }
        let sample = buffer[cursor];
        cursor += 1;
        sample
    };

    match sample_format {
        cpal::SampleFormat::F32 => {
            build_typed_stream::<f32>(&device, &stream_config, generator, channels, device_lost)
        }
        cpal::SampleFormat::I16 => {
            build_typed_stream::<i16>(&device, &stream_config, generator, channels, device_lost)
        }
        cpal::SampleFormat::U16 => {
            build_typed_stream::<u16>(&device, &stream_config, generator, channels, device_lost)
        }
        other => Err(anyhow::anyhow!(
            "unsupported LTC output sample format: {other:?}"
        )),
    }
}

fn build_typed_stream<T>(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    mut generator: impl FnMut() -> f32 + Send + 'static,
    channels: usize,
    device_lost: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    device
        .build_output_stream(
            stream_config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for out_frame in data.chunks_mut(channels) {
                    let sample = T::from_sample(generator());
                    for out in out_frame.iter_mut() {
                        *out = sample;
                    }
                }
            },
            move |error| {
                // Backend-specific errors can be transient; only a vanished
                // device means the stream is gone for good.
                if matches!(error, cpal::StreamError::DeviceNotAvailable) {
                    device_lost.store(true, Ordering::Relaxed);
                }
                tracing::warn!(%error, "LTC output stream error");
            },
            None,
        )
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_FRAME_RATE: [FrameRate; 3] =
        [FrameRate::Film24, FrameRate::Ebu25, FrameRate::Smpte30];

    fn sample_timecode() -> TimeCode {
        TimeCode {
            hours: 13,
            minutes: 45,
            seconds: 52,
            frames: 21,
        }
    }

    fn bits_to_value(bits: &[bool], start: usize, width: usize) -> u8 {
        (0..width).fold(0u8, |value, i| value | ((bits[start + i] as u8) << i))
    }

    fn frame_duration(frame_rate: FrameRate) -> Duration {
        Duration::from_secs_f64(1.0 / frame_rate.fps() as f64)
    }

    fn latch(timecode: TimeCode, is_advancing: bool, set_at: Instant) -> LtcLatch {
        LtcLatch::new(
            LtcTarget {
                timecode,
                frame_rate: FrameRate::Smpte30,
                is_advancing,
            },
            set_at,
        )
    }

    #[test]
    fn frame_bits_encode_every_field_of_a_known_timecode() {
        let bits = frame_bits(sample_timecode(), FrameRate::Smpte30);

        assert_eq!(bits_to_value(&bits, 0, 4), 1, "frame units");
        assert_eq!(bits_to_value(&bits, 8, 2), 2, "frame tens");
        assert_eq!(bits_to_value(&bits, 16, 4), 2, "seconds units");
        assert_eq!(bits_to_value(&bits, 24, 3), 5, "seconds tens");
        assert_eq!(bits_to_value(&bits, 32, 4), 5, "minutes units");
        assert_eq!(bits_to_value(&bits, 40, 3), 4, "minutes tens");
        assert_eq!(bits_to_value(&bits, 48, 4), 3, "hours units");
        assert_eq!(bits_to_value(&bits, 56, 2), 1, "hours tens");

        for &flag_bit in &[10usize, 11, 27, 43, 58, 59] {
            assert!(!bits[flag_bit], "bit {flag_bit} should be zero");
        }
        for &user_bits_start in &[4usize, 12, 20, 28, 36, 44, 52, 60] {
            for i in 0..4 {
                assert!(
                    !bits[user_bits_start + i],
                    "user bit {}",
                    user_bits_start + i
                );
            }
        }

        assert_eq!(&bits[64..80], SYNC_WORD_BITS.as_slice(), "sync word");
    }

    #[test]
    fn frame_bits_layout_is_stable_across_the_frame_rate_choice() {
        // LTC's 80 bits carry no rate field of their own (unlike MTC) — the
        // rate only affects the sample generator's bit duration.
        assert_eq!(
            frame_bits(sample_timecode(), FrameRate::Film24),
            frame_bits(sample_timecode(), FrameRate::Ebu25)
        );
    }

    #[test]
    fn biphase_samples_transition_at_every_bit_start_and_mid_one_bit() {
        let bits = [false, true, false];
        let mut level = -1.0_f32;
        let samples = biphase_samples(&bits, &mut BitClock::with_samples_per_bit(4.0), &mut level);
        assert_eq!(
            samples,
            vec![1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0]
        );
        assert_eq!(
            level, -1.0,
            "the returned level matches the last sample pushed"
        );
    }

    #[test]
    fn biphase_samples_chain_across_calls_with_a_transition_at_the_seam() {
        let mut level = -1.0_f32;
        let mut bit_clock = BitClock::with_samples_per_bit(2.0);
        let first = biphase_samples(&[true], &mut bit_clock, &mut level);
        let second = biphase_samples(&[false], &mut bit_clock, &mut level);
        // Whatever level `first` ends on, `second` must start by flipping
        // away from it — a biphase transition is unconditional at every bit
        // boundary, including one that falls across two separate calls.
        assert_ne!(*first.last().unwrap(), second[0]);
    }

    #[test]
    fn one_frame_of_audio_lasts_exactly_one_frame_at_48_khz() {
        let bits = frame_bits(sample_timecode(), FrameRate::Smpte30);
        let mut level = -1.0_f32;
        let samples = biphase_samples(
            &bits,
            &mut BitClock::new(48_000, FrameRate::Smpte30),
            &mut level,
        );
        // 48 kHz / (30 fps * 80 bits) = 20 samples/bit.
        assert_eq!(samples.len(), 80 * 20);
    }

    #[test]
    fn one_second_of_ltc_lasts_one_second_at_any_sample_rate() {
        // 44.1 kHz is the case that ran fast: 18.375 samples per bit at
        // 30 fps, which rounding down turned into 18.
        for sample_rate in [44_100u32, 48_000, 88_200, 96_000] {
            for frame_rate in EVERY_FRAME_RATE {
                let mut bit_clock = BitClock::new(sample_rate, frame_rate);
                let bits_per_second = frame_rate.fps() as usize * BITS_PER_FRAME;
                let samples: usize = (0..bits_per_second).map(|_| bit_clock.next_bit().0).sum();
                assert!(
                    samples.abs_diff(sample_rate as usize) <= 1,
                    "{sample_rate} Hz at {frame_rate:?}: {samples} samples"
                );
            }
        }
    }

    #[test]
    fn a_fractional_bit_still_flips_a_one_bit_in_its_middle() {
        let mut level = -1.0_f32;
        let mut bit_clock = BitClock::new(44_100, FrameRate::Smpte30);
        let samples = biphase_samples(&[true; 8], &mut bit_clock, &mut level);
        // Every `1` bit flips at its start and in its middle; only the very
        // first start transition falls outside `samples`, against `level`.
        let transitions = samples.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert_eq!(transitions, 8 * 2 - 1);
    }

    #[test]
    fn next_timecode_steps_one_frame_at_a_time_through_an_hour() {
        // The old millisecond round trip landed just short of a frame boundary
        // on 13% of frames at 30 fps and repeated them.
        for frame_rate in EVERY_FRAME_RATE {
            for total_frames in 0..frame_rate.fps() * 3600 {
                let timecode = TimeCode::from_frame_count(total_frames, frame_rate);
                assert_eq!(
                    next_timecode(timecode, frame_rate),
                    TimeCode::from_frame_count(total_frames + 1, frame_rate),
                    "{frame_rate:?} after {timecode:?}"
                );
            }
        }
    }

    #[test]
    fn an_advancing_latch_projects_its_target_forward_in_real_time() {
        let set_at = Instant::now();
        let latch = latch(sample_timecode(), true, set_at);
        let one_second_later = latch.timecode_at(set_at + Duration::from_secs(1));
        assert_eq!(
            frames_apart(one_second_later, sample_timecode(), FrameRate::Smpte30),
            30
        );
    }

    #[test]
    fn a_playing_deck_gets_every_frame_in_turn() {
        let set_at = Instant::now();
        let latch = latch(sample_timecode(), true, set_at);
        let mut sequencer = FrameSequencer::default();
        assert_eq!(
            sequencer.next_frame(&latch, set_at),
            Some(sample_timecode())
        );
        let base = sample_timecode().frame_count(FrameRate::Smpte30);
        for step in 1..=300u32 {
            let now = set_at + frame_duration(FrameRate::Smpte30) * step;
            let frame = sequencer.next_frame(&latch, now).expect("the deck plays");
            assert_eq!(
                frame.frame_count(FrameRate::Smpte30),
                base + u64::from(step)
            );
        }
    }

    #[test]
    fn a_paused_deck_sends_no_ltc() {
        let set_at = Instant::now();
        let latch = latch(sample_timecode(), false, set_at);
        let mut sequencer = FrameSequencer::default();
        for step in 1..=10u32 {
            let now = set_at + frame_duration(FrameRate::Smpte30) * step;
            assert_eq!(sequencer.next_frame(&latch, now), None);
        }
    }

    #[test]
    fn playing_again_picks_up_exactly_where_the_deck_is() {
        let set_at = Instant::now();
        let mut sequencer = FrameSequencer::default();
        let playing = latch(sample_timecode(), true, set_at);
        sequencer.next_frame(&playing, set_at);
        sequencer.next_frame(&latch(sample_timecode(), false, set_at), set_at);

        let resumed_at = TimeCode::from_frame_count(
            sample_timecode().frame_count(FrameRate::Smpte30) - 100,
            FrameRate::Smpte30,
        );
        let resumed = latch(resumed_at, true, set_at);
        assert_eq!(sequencer.next_frame(&resumed, set_at), Some(resumed_at));
    }

    #[test]
    fn a_drifted_count_jam_syncs_onto_the_deck() {
        let set_at = Instant::now();
        let mut sequencer = FrameSequencer::default();
        sequencer.next_frame(&latch(sample_timecode(), true, set_at), set_at);
        let ahead = TimeCode::from_frame_count(
            sample_timecode().frame_count(FrameRate::Smpte30) + 10,
            FrameRate::Smpte30,
        );
        let latch = latch(ahead, true, set_at);
        assert_eq!(sequencer.next_frame(&latch, set_at), Some(ahead));
    }

    #[test]
    fn a_silent_frame_lasts_as_long_as_a_sounding_one() {
        let mut silent = BitClock::new(44_100, FrameRate::Ebu25);
        let mut sounding = silent;
        let mut level = -1.0_f32;
        let sounding_length = biphase_samples(
            &frame_bits(sample_timecode(), FrameRate::Ebu25),
            &mut sounding,
            &mut level,
        )
        .len();
        assert_eq!(silent.next_frame_length(), sounding_length);
        assert_eq!(
            silent, sounding,
            "both leave the bit clock in the same place"
        );
    }
}
