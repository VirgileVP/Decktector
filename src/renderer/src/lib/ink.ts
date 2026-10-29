/**
 * Canvas colors pulled from the active theme.
 *
 * The waveforms are drawn on a canvas, which CSS cannot reach, so the palette
 * has to be read back from the custom properties. Resolving them is a layout
 * query, far too costly per animation frame, so the result is cached until the
 * theme on the document root changes.
 */

export interface CanvasInk {
  /** Flat line drawn when a deck has no waveform yet. */
  empty: string
  played: string
  unplayed: string
  playhead: string
  gridDownbeat: string
  gridBeat: string
  /** Secondary text drawn over the waveform, e.g. the zoom level. */
  overlayText: string
  /** Letter inside a hot cue flag, on top of the cue color. */
  flagText: string
  /** The loop a deck is playing right now, and its LOOP badge. */
  activeLoop: string
}

const INK_VARS: Record<keyof CanvasInk, string> = {
  empty: '--wave-empty',
  played: '--wave-played',
  unplayed: '--wave-unplayed',
  playhead: '--playhead',
  gridDownbeat: '--grid-downbeat',
  gridBeat: '--grid-beat',
  overlayText: '--overlay-text',
  flagText: '--flag-text',
  activeLoop: '--loop-active'
}

let cache: { theme: string; ink: CanvasInk } | null = null

export function canvasInk(): CanvasInk {
  const theme = document.documentElement.dataset.theme ?? ''
  if (cache?.theme === theme) return cache.ink

  const styles = getComputedStyle(document.documentElement)
  const entries = Object.entries(INK_VARS).map(([key, property]) => [
    key,
    styles.getPropertyValue(property).trim()
  ])
  const ink = Object.fromEntries(entries) as CanvasInk

  cache = { theme, ink }
  return ink
}
