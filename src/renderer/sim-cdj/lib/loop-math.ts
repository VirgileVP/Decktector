import type { SimLoop } from '@renderer/model/SimLoop'

import { LOOP_LENGTHS_BEATS } from './constants'

const MS_PER_MINUTE = 60_000

/** Beats a loop spans at the track's own tempo, rounded, or null when the tempo is unknown. */
export function loopBeatCount(loop: SimLoop, trackBpm: number | null): number | null {
  if (trackBpm === null || trackBpm <= 0) return null
  const beatMs = MS_PER_MINUTE / trackBpm
  return Math.round((loop.endMs - loop.startMs) / beatMs)
}

/** Which of the fixed loop buttons a running loop corresponds to, if any. */
export function activeLoopButton(
  loop: SimLoop | null,
  trackBpm: number | null
): (typeof LOOP_LENGTHS_BEATS)[number] | null {
  if (loop === null || !loop.active) return null
  const beats = loopBeatCount(loop, trackBpm)
  return LOOP_LENGTHS_BEATS.find((length) => length === beats) ?? null
}
