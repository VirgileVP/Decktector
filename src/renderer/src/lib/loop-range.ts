import type { LoopRange } from '../model'

/**
 * Where a playhead projected past the end of `loop` really is: a looping
 * player jumps back to the loop start each time it reaches the end, so the
 * overshoot folds back in. Positions before the end, and an empty loop,
 * are left alone.
 */
export function wrapIntoLoop(positionMs: number, loop: LoopRange | null): number {
  if (loop === null || positionMs < loop.endMs) return positionMs
  const length = loop.endMs - loop.startMs
  if (length <= 0) return positionMs
  return loop.startMs + ((positionMs - loop.startMs) % length)
}
