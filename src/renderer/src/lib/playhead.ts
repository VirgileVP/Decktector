import type { DeckState } from '../model'
import { wrapIntoLoop } from './loop-range'

/**
 * How far past its last status a playing deck's position is projected:
 * several missed ~200ms status packets, not the length of a network outage.
 * Mirrors `MAX_EXTRAPOLATION_MS` in the timecode generator.
 */
export const MAX_EXTRAPOLATION_MS = 1500

/**
 * Estimates the current playhead position by extrapolating from the last
 * status packet (anchor `playheadMs` at `reportedAt`) using the pitch.
 * Status packets arrive at ~5 Hz; this makes the playhead move at 60 fps.
 * Once status stops arriving the playhead holds after
 * {@link MAX_EXTRAPOLATION_MS} rather than scrolling on for a deck that may
 * have stopped long ago. A deck in a loop whose bounds are known jumps back
 * to the loop start at its end, as the player does.
 */
export function estimatePlayheadMs(deck: DeckState | undefined, nowMs: number): number | null {
  if (!deck || deck.playheadMs === null) return null
  if (deck.playState !== 'playing') return deck.playheadMs
  const elapsedMs = Math.min(nowMs - deck.reportedAt, MAX_EXTRAPOLATION_MS)
  const projectedMs = deck.playheadMs + elapsedMs * (1 + deck.pitch / 100)
  return wrapIntoLoop(projectedMs, deck.activeLoop?.range ?? null)
}

export function formatTimeMs(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`
}
