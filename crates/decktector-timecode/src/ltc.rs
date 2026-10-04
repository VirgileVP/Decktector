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
//! free-runs its own frame counter, computed from elapsed sample count, and
//! never asks the deck for its position directly. Instead the tick loop
//! (see [`crate::TimecodeService::tick`]'s `Ltc` branch) periodically hands
//! it the deck's real extrapolated timecode through [`LtcLatch`], the only
//! thing shared between the two threads; if the free-running counter has
//! drifted more than ~1 frame from that target, the audio thread jam-syncs
//! onto it at its next frame boundary.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{FrameRate, TimeCode};

/// The fixed 16-bit pattern that closes every LTC frame (bits 64-79, bit 64
/// sent first). This exact pattern never occurs elsewhere in valid
/// biphase-encoded LTC, which is what lets a receiver find frame
/// boundaries.
const SYNC_WORD_BITS: [bool; 16] = [
    false, false, true, true, true, true, true, true, true, true, true, true, true, true, false,
    true,
];

/// Every audio output device this machine could send LTC to.
pub fn list_output_devices() -> anyhow::Result<Vec<String>> {
    let host = cpal::default_host();
    host.output_devices()?
        .map(|device| device.name().map_err(anyhow::Error::from))
        .collect()
}

/// Sets `width` bits starting at `start`, least-significant bit first — the
/// bit order every BCD field and binary field in the LTC frame uses.
fn set_bits(bits: &mut [bool; 80], start: usize, width: usize, value: u8) {
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
fn frame_bits(timecode: TimeCode, _frame_rate: FrameRate) -> [bool; 80] {
    let mut bits = [false; 80];
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

/// How many audio samples one LTC bit occupies at `sample_rate`, for a
/// frame rate whose LTC frame is always 80 bits.
fn samples_per_bit(sample_rate: u32, frame_rate: FrameRate) -> usize {
    (sample_rate as u64 / (frame_rate.fps() * 80)).max(1) as usize
}

/// Encodes `bits` as biphase-mark samples, `samples_per_bit` samples per
/// bit. `level` is the polarity in effect just before this call and is
/// updated in place to the polarity in effect after the last sample — so
/// callers can chain successive frames back-to-back with a transition
/// preserved at every bit boundary, including the seam between calls.
///
/// Biphase-mark rule: there is always a transition at the start of a bit
/// period; a `1` bit has an additional transition at the bit's midpoint, a
/// `0` bit has none.
fn biphase_samples(bits: &[bool], samples_per_bit: usize, level: &mut f32) -> Vec<f32> {
    let mut samples = Vec::with_capacity(bits.len() * samples_per_bit);
    for &bit in bits {
        *level = -*level;
        samples.push(*level);
        for i in 1..samples_per_bit {
            if bit && i == samples_per_bit / 2 {
                *level = -*level;
            }
            samples.push(*level);
        }
    }
    samples
}

/// One LTC frame's worth of biphase-mark-encoded audio samples, starting
/// from a fresh negative polarity.
fn frame_samples(bits: &[bool; 80], sample_rate: u32, frame_rate: FrameRate) -> Vec<f32> {
    let mut level = -1.0_f32;
    biphase_samples(bits, samples_per_bit(sample_rate, frame_rate), &mut level)
}

fn timecode_to_ms(timecode: TimeCode, frame_rate: FrameRate) -> f64 {
    let total_seconds =
        timecode.hours as f64 * 3600.0 + timecode.minutes as f64 * 60.0 + timecode.seconds as f64;
    (total_seconds + timecode.frames as f64 / frame_rate.fps() as f64) * 1000.0
}

fn next_timecode(timecode: TimeCode, frame_rate: FrameRate) -> TimeCode {
    let frame_ms = 1000.0 / frame_rate.fps() as f64;
    TimeCode::from_elapsed_ms(timecode_to_ms(timecode, frame_rate) + frame_ms, frame_rate)
}

/// How many whole frames apart `a` and `b` are, regardless of sign.
fn frames_apart(a: TimeCode, b: TimeCode, frame_rate: FrameRate) -> f64 {
    let frame_ms = 1000.0 / frame_rate.fps() as f64;
    (timecode_to_ms(a, frame_rate) - timecode_to_ms(b, frame_rate)).abs() / frame_ms
}

/// Where the LTC audio thread should be, extrapolated from the latch's
/// anchor to this instant.
fn timecode_at(latch: &LtcLatch) -> TimeCode {
    let elapsed_ms = latch.start_instant.elapsed().as_secs_f64() * 1000.0;
    TimeCode::from_elapsed_ms(
        timecode_to_ms(latch.start_timecode, latch.frame_rate) + elapsed_ms,
        latch.frame_rate,
    )
}

/// Jam-sync target shared between the async tick loop and the audio thread.
///
/// The tick loop only ever touches this `Mutex`, never the `cpal::Stream`
/// directly — that is what keeps everything the async side holds `Send`.
#[derive(Debug, Clone, Copy)]
struct LtcLatch {
    start_timecode: TimeCode,
    start_instant: Instant,
    frame_rate: FrameRate,
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
    thread: Option<thread::JoinHandle<()>>,
    device_name: String,
}

impl LtcStream {
    /// Spawns the audio thread and builds/plays the `cpal::Stream` on it,
    /// blocking until the stream is confirmed playing (or has failed) so
    /// callers get the same synchronous success/failure signal as
    /// [`crate::mtc::connect`].
    pub(crate) fn start(
        device_name: &str,
        initial_timecode: TimeCode,
        frame_rate: FrameRate,
    ) -> anyhow::Result<Self> {
        let latch = Arc::new(Mutex::new(LtcLatch {
            start_timecode: initial_timecode,
            start_instant: Instant::now(),
            frame_rate,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();

        let thread_latch = Arc::clone(&latch);
        let thread_stop = Arc::clone(&stop);
        let owned_device_name = device_name.to_string();
        let thread = thread::Builder::new()
            .name("decktector-ltc".into())
            .spawn(move || {
                run_audio_thread(&owned_device_name, thread_latch, thread_stop, ready_tx)
            })
            .map_err(|error| anyhow::anyhow!("failed to spawn the LTC audio thread: {error}"))?;

        ready_rx.recv().map_err(|_| {
            anyhow::anyhow!("the LTC audio thread exited before starting its stream")
        })??;

        Ok(Self {
            latch,
            stop,
            thread: Some(thread),
            device_name: device_name.to_string(),
        })
    }

    pub(crate) fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Updates the shared jam-sync target. The audio thread re-latches onto
    /// it once its free-running counter has drifted more than ~1 frame away
    /// — checked at its own frame boundaries, not on every sample.
    ///
    /// `frame_rate` travels with every call (mirroring how `frame_rate` is
    /// passed on every Art-Net/MTC tick too) so a frame-rate change while
    /// LTC is running takes effect without rebuilding the stream.
    pub(crate) fn resync(&self, timecode: TimeCode, frame_rate: FrameRate) {
        if let Ok(mut latch) = self.latch.lock() {
            latch.start_timecode = timecode;
            latch.start_instant = Instant::now();
            latch.frame_rate = frame_rate;
        }
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
    ready: mpsc::Sender<anyhow::Result<()>>,
) {
    let stream = match build_stream(device_name, &latch) {
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
fn build_stream(device_name: &str, latch: &Arc<Mutex<LtcLatch>>) -> anyhow::Result<cpal::Stream> {
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

    let initial = *latch
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut frame_rate = initial.frame_rate;
    let mut timecode = initial.start_timecode;
    // The very first buffer has no prior polarity to continue from, so it's
    // the one place a fresh `frame_samples` call (rather than `biphase_samples`
    // threading `level` in from a previous call) is the right tool.
    let mut buffer = frame_samples(&frame_bits(timecode, frame_rate), sample_rate, frame_rate);
    let mut level = *buffer.last().unwrap_or(&-1.0);
    let mut cursor = 0usize;
    let latch_for_thread = Arc::clone(latch);

    // Pulls one sample at a time, generating the next frame's buffer (and
    // jam-syncing against the latch) whenever the current one runs out.
    let generator = move || -> f32 {
        if cursor >= buffer.len() {
            timecode = next_timecode(timecode, frame_rate);
            if let Ok(guard) = latch_for_thread.lock() {
                let target = timecode_at(&guard);
                if frames_apart(timecode, target, guard.frame_rate) > 1.0 {
                    timecode = target;
                }
                frame_rate = guard.frame_rate;
            }
            buffer = biphase_samples(
                &frame_bits(timecode, frame_rate),
                samples_per_bit(sample_rate, frame_rate),
                &mut level,
            );
            cursor = 0;
        }
        let sample = buffer[cursor];
        cursor += 1;
        sample
    };

    match sample_format {
        cpal::SampleFormat::F32 => {
            build_typed_stream::<f32>(&device, &stream_config, generator, channels)
        }
        cpal::SampleFormat::I16 => {
            build_typed_stream::<i16>(&device, &stream_config, generator, channels)
        }
        cpal::SampleFormat::U16 => {
            build_typed_stream::<u16>(&device, &stream_config, generator, channels)
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
            |error| tracing::warn!(%error, "LTC output stream error"),
            None,
        )
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let samples = biphase_samples(&bits, 4, &mut level);
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
        let first = biphase_samples(&[true], 2, &mut level);
        let second = biphase_samples(&[false], 2, &mut level);
        // Whatever level `first` ends on, `second` must start by flipping
        // away from it — a biphase transition is unconditional at every bit
        // boundary, including one that falls across two separate calls.
        assert_ne!(*first.last().unwrap(), second[0]);
    }

    #[test]
    fn frame_samples_produces_one_frames_worth_of_audio() {
        let bits = frame_bits(sample_timecode(), FrameRate::Smpte30);
        let samples = frame_samples(&bits, 48_000, FrameRate::Smpte30);
        // 48 kHz / (30 fps * 80 bits) = 20 samples/bit.
        assert_eq!(samples.len(), 80 * 20);
    }
}
