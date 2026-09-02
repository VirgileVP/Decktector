import type { BeatGridBeat } from '../model'

/** Beats per bar in the 4/4 grid Rekordbox exports; a beat's position in its
 *  bar runs from 1 to this. */
export const BEATS_PER_MEASURE = 4

/**
 * Position within the bar of the beat the playhead is sitting on, 1-4, or
 * null when there is no grid or the playhead is still ahead of its first beat.
 *
 * Binary search rather than a scan: this runs once per animation frame, on a
 * grid holding one entry per beat (~640 for five minutes at 128 BPM).
 */
export function beatInMeasureAt(
  grid: BeatGridBeat[] | null | undefined,
  playheadMs: number
): number | null {
  if (!grid || grid.length === 0 || playheadMs < grid[0].offsetMs) return null

  // Last beat whose offset has been reached.
  let lo = 0
  let hi = grid.length - 1
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (grid[mid].offsetMs <= playheadMs) lo = mid
    else hi = mid - 1
  }

  return grid[lo].beatInMeasure
}
