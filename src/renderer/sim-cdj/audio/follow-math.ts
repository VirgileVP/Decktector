import { wrapIntoLoop } from '@renderer/lib/loop-range'
import { MAX_EXTRAPOLATION_MS } from '@renderer/lib/playhead'
import type { LoopRange } from '@renderer/model/LoopRange'
import type { SimDeckView } from '@renderer/model/SimDeckView'

import { MS_PER_SECOND } from './audio-constants'

const PERCENT = 100

/** A loop to repeat, in track milliseconds. */
export type AudioLoop = LoopRange

/** What a deck's audio should be doing right now. */
export type PlaybackTarget =
  | { kind: 'silent' }
  | { kind: 'play'; positionMs: number; rate: number; loop: AudioLoop | null }
  | { kind: 'scratch'; positionMs: number; atMs: number }

/** Where playback was last (re)anchored, on the audio clock. */
export interface PlaybackAnchor {
  contextTimeSeconds: number
  positionMs: number
  rate: number
}

/** Playback rate for a pitch in percent. Pitch follows tempo: there is no key lock. */
export function playbackRate(effectivePitch: number): number {
  return 1 + effectivePitch / PERCENT
}

/** Position of an anchored playback at an audio clock time. */
export function positionAt(
  anchor: PlaybackAnchor,
  contextTimeSeconds: number,
  loop: AudioLoop | null
): number {
  const elapsedMs = (contextTimeSeconds - anchor.contextTimeSeconds) * MS_PER_SECOND
  return wrapIntoLoop(anchor.positionMs + elapsedMs * anchor.rate, loop)
}

/**
 * Signed distance from the expected position to the actual one. Inside a loop
 * the two may sit either side of the wrap point, so the shorter way round wins.
 */
export function driftMs(expectedMs: number, actualMs: number, loop: AudioLoop | null): number {
  const drift = actualMs - expectedMs
  if (loop === null) return drift
  const length = loop.endMs - loop.startMs
  return ((((drift + length / 2) % length) + length) % length) - length / 2
}

/**
 * Turns one deck of a rig state into what its audio should do. `reportedAt`
 * and `nowMs` share the server's clock (same machine), so the position is
 * extrapolated over the state's age.
 */
export function planPlayback(view: SimDeckView, reportedAt: number, nowMs: number): PlaybackTarget {
  if (view.trackIndex === null || view.playState === 'empty' || view.playState === 'loading') {
    return { kind: 'silent' }
  }
  if (view.scratching) return { kind: 'scratch', positionMs: view.positionMs, atMs: reportedAt }
  if (view.playState !== 'playing') return { kind: 'silent' }

  // Wide range at its bottom stops the deck: nothing to play at a rate of 0.
  const rate = playbackRate(view.effectivePitch)
  if (rate <= 0) return { kind: 'silent' }

  const loop =
    view.loopState?.active === true
      ? { startMs: view.loopState.startMs, endMs: view.loopState.endMs }
      : null
  const ageMs = Math.min(Math.max(nowMs - reportedAt, 0), MAX_EXTRAPOLATION_MS)
  return {
    kind: 'play',
    positionMs: wrapIntoLoop(view.positionMs + ageMs * rate, loop),
    rate,
    loop
  }
}
