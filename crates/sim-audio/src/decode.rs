//! Reading an audio file: its samples, mixed to mono, and its tags.
//!
//! Mono because everything downstream measures energy — a waveform column,
//! an onset, a band split — and none of it is stereo-aware. Mixing once
//! here keeps that assumption in a single place.

use std::path::Path;

use anyhow::Context as _;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::tags::Tags;

/// A decoded file, ready to analyse.
pub struct DecodedAudio {
    /// Mono samples, nominally -1.0 to 1.0.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub tags: Tags,
}

impl DecodedAudio {
    pub fn duration_ms(&self) -> f64 {
        self.samples.len() as f64 / self.sample_rate as f64 * 1000.0
    }
}

/// Decodes `path` in full.
///
/// Corrupt packets are skipped rather than fatal: a track whose last frame
/// is truncated — routine in files that went through a lossy chain — is
/// still worth every second that did decode.
pub fn decode(path: &Path) -> anyhow::Result<DecodedAudio> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .with_context(|| format!("{} is not an audio format we can read", path.display()))?;

    let tags = read_tags(&mut probed).or_file_name(path);
    let mut format = probed.format;
    let (track_id, codec_params) = {
        let track = format
            .tracks()
            .iter()
            .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
            .context("file holds no audio track")?;
        (track.id, track.codec_params.clone())
    };

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .context("no decoder for this codec")?;

    let mut samples = Vec::new();
    let mut sample_rate = codec_params.sample_rate.unwrap_or_default();
    let mut buffer: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // Both are how a format reader reports "that was the last of
            // it", not failures.
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(Error::ResetRequired) => break,
            Err(error) => return Err(error).context("reading a packet"),
        };
        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(Error::DecodeError(reason)) => {
                eprintln!("{}: skipping a corrupt packet ({reason})", path.display());
                continue;
            }
            Err(error) => return Err(error).context("decoding a packet"),
        };

        let spec = *decoded.spec();
        sample_rate = spec.rate;
        let interleaved =
            buffer.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        interleaved.copy_interleaved_ref(decoded);
        samples.extend(to_mono(interleaved.samples(), spec.channels.count()));
    }

    anyhow::ensure!(
        !samples.is_empty(),
        "{} decoded to no audio",
        path.display()
    );
    anyhow::ensure!(
        sample_rate > 0,
        "{} declares no sample rate",
        path.display()
    );

    Ok(DecodedAudio {
        samples,
        sample_rate,
        tags,
    })
}

/// Tags live either in the container the probe peeked at or in the format
/// reader itself, depending on where the file keeps them.
fn read_tags(probed: &mut symphonia::core::probe::ProbeResult) -> Tags {
    if let Some(revision) = probed.format.metadata().current() {
        return Tags::from_metadata(revision);
    }
    probed
        .metadata
        .get()
        .as_ref()
        .and_then(|metadata| metadata.current().map(Tags::from_metadata))
        .unwrap_or_default()
}

fn to_mono(interleaved: &[f32], channels: usize) -> impl Iterator<Item = f32> + '_ {
    let channels = channels.max(1);
    interleaved
        .chunks(channels)
        .map(move |frame| frame.iter().sum::<f32>() / channels as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_frame_averages_into_one_sample() {
        let mono: Vec<f32> = to_mono(&[1.0, 0.0, -1.0, 1.0], 2).collect();

        assert_eq!(mono, vec![0.5, 0.0]);
    }

    #[test]
    fn a_mono_stream_passes_through_untouched() {
        let mono: Vec<f32> = to_mono(&[0.25, -0.5], 1).collect();

        assert_eq!(mono, vec![0.25, -0.5]);
    }
}
