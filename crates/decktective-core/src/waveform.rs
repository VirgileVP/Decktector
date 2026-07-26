//! Waveform downsampling shared by every engine.

use crate::model::WaveformSegment;

/// Column count of whole-track preview waveforms (rekordbox convention).
pub const PREVIEW_SEGMENT_COUNT: usize = 400;

/// Downsamples a detailed waveform into a fixed-size preview by keeping the
/// tallest column of each chunk, so transients stay visible in the minimap.
pub fn build_preview_from_detail(
    detail: &[WaveformSegment],
    segment_count: usize,
) -> Vec<WaveformSegment> {
    if detail.is_empty() {
        return Vec::new();
    }

    let chunk = detail.len() as f64 / segment_count as f64;
    (0..segment_count)
        .map(|i| {
            let start = (i as f64 * chunk).floor() as usize;
            let end = detail
                .len()
                .min((((i + 1) as f64 * chunk).ceil() as usize).max(start + 1));
            detail[start..end]
                .iter()
                .copied()
                .max_by(|a, b| a.height.total_cmp(&b.height))
                .unwrap_or(detail[start])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(height: f32) -> WaveformSegment {
        WaveformSegment {
            height,
            color: [0, 0, 0],
        }
    }

    #[test]
    fn empty_detail_yields_empty_preview() {
        assert!(build_preview_from_detail(&[], PREVIEW_SEGMENT_COUNT).is_empty());
    }

    #[test]
    fn keeps_the_tallest_column_of_each_chunk() {
        let detail: Vec<_> = [0.1, 0.9, 0.2, 0.3, 0.8, 0.4].map(segment).into();
        let preview = build_preview_from_detail(&detail, 3);

        assert_eq!(preview.len(), 3);
        assert_eq!(preview[0].height, 0.9);
        assert_eq!(preview[1].height, 0.3);
        assert_eq!(preview[2].height, 0.8);
    }

    #[test]
    fn upsampling_never_reads_out_of_bounds() {
        let detail: Vec<_> = [0.5, 0.6].map(segment).into();
        let preview = build_preview_from_detail(&detail, PREVIEW_SEGMENT_COUNT);

        assert_eq!(preview.len(), PREVIEW_SEGMENT_COUNT);
    }
}
