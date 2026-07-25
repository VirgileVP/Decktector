import type { CDJStatus } from 'prolink-connect'

import type { BeatGridBeat } from '../types'
import { isAdvancing } from './normalize'

/**
 * Estimates the playhead position of one deck.
 *
 * Pro DJ Link on CDJ-2000/XDJ era hardware never reports an absolute
 * position; status packets only carry the beat counter (1-based number of
 * beats since track start). The tracker anchors the playhead on the beat
 * grid every time the counter changes, and advances it with the pitch
 * between beats so downstream consumers get a monotonic position.
 */
/**
 * CDJ-3000 precise-position packets arrive roughly every 30ms; treat one as
 * stale after a few missed beats so a deck that stops sending them (older
 * hardware, or the packet listener never having seen one) falls back to
 * beat-grid interpolation instead of freezing on a stale value.
 */
const PRECISE_POSITION_STALE_MS = 150

export class PositionTracker {
  private grid: BeatGridBeat[] | null = null
  private positionMs: number | null = null
  private lastBeat: number | null = null
  private updatedAt = 0
  private precisePositionMs: number | null = null
  private precisePositionAt = 0

  reset(): void {
    this.grid = null
    this.positionMs = null
    this.lastBeat = null
    this.precisePositionMs = null
    this.precisePositionAt = 0
  }

  setBeatGrid(grid: BeatGridBeat[] | null): void {
    this.grid = grid
  }

  /** Feeds a precise-position packet (CDJ-3000+ only). Takes priority over beat-grid interpolation while fresh. */
  setPrecisePosition(positionMs: number, nowMs: number): void {
    this.precisePositionMs = positionMs
    this.precisePositionAt = nowMs
  }

  /** Feeds one status packet; returns the estimated position in ms. */
  update(state: CDJStatus.State, nowMs: number): number | null {
    if (
      this.precisePositionMs !== null &&
      nowMs - this.precisePositionAt <= PRECISE_POSITION_STALE_MS
    ) {
      return this.precisePositionMs
    }

    if (!this.grid || this.grid.length === 0) return null

    if (state.beat !== null && state.beat !== this.lastBeat) {
      // New beat: snap to the beat grid, the authoritative reference
      this.lastBeat = state.beat
      const index = Math.min(Math.max(state.beat - 1, 0), this.grid.length - 1)
      this.positionMs = this.grid[index].offsetMs
    } else if (this.positionMs !== null && isAdvancing(state)) {
      // Same beat as last packet: advance with the elapsed time and pitch
      this.positionMs += (nowMs - this.updatedAt) * (1 + state.sliderPitch / 100)
    }

    this.updatedAt = nowMs
    return this.positionMs
  }
}
