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
  /**
   * Channel value the rekordbox waveform colors are pulled towards, to flatten
   * them: near-white on dark themes, near-black on light ones.
   */
  softenTarget: number
}

const SOFTEN_TARGET_VAR = '--wave-soften-target'
const DEFAULT_SOFTEN_TARGET = 235

const INK_VARS: Record<Exclude<keyof CanvasInk, 'softenTarget'>, string> = {
  empty: '--wave-empty',
  played: '--wave-played',
  unplayed: '--wave-unplayed',
  playhead: '--playhead',
  gridDownbeat: '--grid-downbeat',
  gridBeat: '--grid-beat',
  overlayText: '--overlay-text',
  flagText: '--flag-text'
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
  // An undeclared property reads back as an empty string, which Number() would
  // happily turn into 0 — i.e. black — so the emptiness is checked first.
  const rawSoftenTarget = styles.getPropertyValue(SOFTEN_TARGET_VAR).trim()
  const softenTarget = rawSoftenTarget === '' ? NaN : Number(rawSoftenTarget)
  const ink: CanvasInk = {
    ...(Object.fromEntries(entries) as Omit<CanvasInk, 'softenTarget'>),
    softenTarget: Number.isFinite(softenTarget) ? softenTarget : DEFAULT_SOFTEN_TARGET
  }

  cache = { theme, ink }
  return ink
}
