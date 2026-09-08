/**
 * Geometry for the detailed waveform's off-screen strip.
 *
 * Redrawing the envelope column by column on every frame means one `fillRect`
 * and one CSS colour parse per pixel of width, sixty times a second, per deck.
 * Instead the envelope is painted once into a strip several canvas widths wide
 * and blitted from there, so a frame costs two `drawImage` calls. The strip is
 * repainted only when the visible window would leave it — with the window
 * centred, that is after the playhead has travelled the slack on one side.
 */

/** How many canvas widths the strip spans. */
export const STRIP_SPAN = 3
/** Widest strip backing store, in device pixels, to stay well inside WebKit's limits. */
export const MAX_STRIP_DEVICE_PX = 8192

export interface StripRange {
  /** Time at the strip's left edge. */
  startMs: number
  /** Time one pixel past its right edge. */
  endMs: number
}

/**
 * Strip width in CSS pixels for a canvas `widthPx` wide: `STRIP_SPAN` times the
 * canvas, clamped so the backing store stays within `MAX_STRIP_DEVICE_PX`.
 * Never narrower than the canvas itself, however large the canvas gets.
 */
export function stripWidthPx(widthPx: number, dpr: number): number {
  return Math.max(widthPx, Math.min(widthPx * STRIP_SPAN, MAX_STRIP_DEVICE_PX / dpr))
}

/** The strip to paint for a playhead, centred so it has the same slack either way. */
export function stripRangeFor(playheadMs: number, msPerPx: number, stripWidth: number): StripRange {
  const startMs = playheadMs - (stripWidth / 2) * msPerPx
  return { startMs, endMs: startMs + stripWidth * msPerPx }
}

/** Whether a painted strip still covers the whole visible window. */
export function stripCovers(
  range: StripRange,
  windowStartMs: number,
  windowEndMs: number
): boolean {
  return range.startMs <= windowStartMs && range.endMs >= windowEndMs
}
