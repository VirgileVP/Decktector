import type { DeckState } from '../../../engine/types'

/**
 * Estimates the current playhead position by extrapolating from the last
 * status packet (anchor `playheadMs` at `reportedAt`) using the pitch.
 * Status packets arrive at ~5 Hz; this makes the playhead move at 60 fps.
 */
export function estimatePlayheadMs(deck: DeckState | undefined, nowMs: number): number | null {
  if (!deck || deck.playheadMs === null) return null
  if (deck.playState !== 'playing') return deck.playheadMs
  return deck.playheadMs + (nowMs - deck.reportedAt) * (1 + deck.pitch / 100)
}

export function formatTimeMs(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`
}
