import type { PlayState } from '../model'

/** Single-glyph transport readout, one per play state — Denon-style. */
export const TRANSPORT_GLYPHS: Record<PlayState, string> = {
  empty: '—',
  loading: '…',
  cued: '●',
  playing: '▶',
  paused: '❙❙',
  searching: '≫',
  ended: '■'
}
