import type { WaveformSegment } from './types'

/** Column count of whole-track preview waveforms (rekordbox convention). */
export const PREVIEW_SEGMENT_COUNT = 400

/**
 * Downsamples a detailed waveform into a fixed-size preview by keeping the
 * tallest column of each chunk, so transients stay visible in the minimap.
 */
export function buildPreviewFromDetail(
  detail: WaveformSegment[],
  segmentCount: number = PREVIEW_SEGMENT_COUNT
): WaveformSegment[] {
  if (detail.length === 0) return []

  const preview: WaveformSegment[] = new Array(segmentCount)
  const chunk = detail.length / segmentCount
  for (let i = 0; i < segmentCount; i++) {
    const start = Math.floor(i * chunk)
    const end = Math.min(detail.length, Math.max(start + 1, Math.ceil((i + 1) * chunk)))
    let tallest = detail[start]
    for (let j = start + 1; j < end; j++) {
      if (detail[j].height > tallest.height) tallest = detail[j]
    }
    preview[i] = tallest
  }
  return preview
}
