/**
 * Where the detail waveform's fixed playhead sits across the canvas.
 *
 * Centred, half the width shows audio already played. Moved to the left, most
 * of the width is spent on what is coming up — what a booth operator watches
 * for (drops, breaks, the next cue).
 */

export const WAVEFORM_PLAYHEAD_POSITIONS = ['center', 'left'] as const
export type WaveformPlayheadPosition = (typeof WAVEFORM_PLAYHEAD_POSITIONS)[number]

export const DEFAULT_WAVEFORM_PLAYHEAD_POSITION: WaveformPlayheadPosition = 'center'

export const WAVEFORM_PLAYHEAD_POSITION_OPTIONS = [
  { value: 'center', label: 'Center' },
  { value: 'left', label: 'Left' }
] as const satisfies readonly { value: WaveformPlayheadPosition; label: string }[]

/** Share of the canvas width left of the playhead, for each position. */
const PLAYHEAD_WIDTH_FRACTION: Record<WaveformPlayheadPosition, number> = {
  center: 0.5,
  left: 0.2
}

/** X of the playhead on a canvas `widthPx` wide, snapped to a whole CSS pixel. */
export function playheadX(widthPx: number, position: WaveformPlayheadPosition): number {
  return Math.round(widthPx * PLAYHEAD_WIDTH_FRACTION[position])
}

/** Narrows an untrusted value (storage, another window) to a known position; anything else is the default. */
export function parseWaveformPlayheadPosition(raw: unknown): WaveformPlayheadPosition {
  return (
    WAVEFORM_PLAYHEAD_POSITIONS.find((position) => position === raw) ??
    DEFAULT_WAVEFORM_PLAYHEAD_POSITION
  )
}
