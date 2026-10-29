import {
  MS_PER_SECOND,
  SCRATCH_GRAIN_LENGTH_MS,
  SCRATCH_GRAIN_OVERLAP_MS,
  SCRATCH_MAX_RATE,
  SCRATCH_MIN_DELTA_MS,
  SCRATCH_MIN_RATE
} from './audio-constants'

/** A deck position seen at a moment of the server's clock. */
export interface ScratchSample {
  positionMs: number
  atMs: number
}

/** A short slice of the track to play for one step of jog motion. */
export interface ScratchGrain {
  startMs: number
  rate: number
  durationSeconds: number
}

/**
 * The grain that voices the jog moving from `previous` to `next`. Its speed is
 * the speed of the motion. Reversed motion is voiced forward at that speed
 * too, from the earlier position: an AudioBufferSourceNode cannot play
 * backwards, and a reversed copy of the buffer is not worth its memory for a
 * test rig. Returns null when nothing moved.
 */
export function scratchGrain(
  previous: ScratchSample | null,
  next: ScratchSample
): ScratchGrain | null {
  if (previous === null) return null
  const elapsedMs = next.atMs - previous.atMs
  const deltaMs = Math.abs(next.positionMs - previous.positionMs)
  if (elapsedMs <= 0 || deltaMs < SCRATCH_MIN_DELTA_MS) return null

  const rate = Math.min(SCRATCH_MAX_RATE, Math.max(SCRATCH_MIN_RATE, deltaMs / elapsedMs))
  const durationMs = Math.min(SCRATCH_GRAIN_LENGTH_MS, elapsedMs + SCRATCH_GRAIN_OVERLAP_MS)
  return {
    startMs: Math.min(previous.positionMs, next.positionMs),
    rate,
    durationSeconds: durationMs / MS_PER_SECOND
  }
}
