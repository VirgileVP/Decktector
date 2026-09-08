import type { TrackCue } from '../model'

export interface CueProgress {
  /** Index in the sorted cue list of the target or active loop cue. */
  index: number
  /** Milliseconds remaining until the cue point is hit, or until loop finishes. */
  remainingMs: number
  /** Progress ratio from the previous reference point towards this cue, 0 to 1. */
  ratio: number
  /** True when playhead has passed all cues in the track. */
  allPassed: boolean
}

const KIND_LABELS: Record<TrackCue['kind'], string> = {
  memory: 'MEM',
  hotcue: '',
  loop: 'LOOP',
  hotloop: 'LOOP'
}

/** Formatted display name for a cue row or active card. */
export function cueLabel(cue: TrackCue): string {
  if (cue.label) return cue.label
  if (cue.button) {
    return cue.kind === 'hotloop' ? `Hot loop ${cue.button}` : `Hot cue ${cue.button}`
  }
  return KIND_LABELS[cue.kind]
}

/** Returns cues sorted chronologically by their offset. */
export function sortCues(cues: TrackCue[]): TrackCue[] {
  return [...cues].sort((a, b) => a.offsetMs - b.offsetMs)
}

/**
 * Locates the active cue relative to the playhead.
 *
 * Cues are expected to be sorted by `offsetMs`.
 * - If the playhead is inside a loop (`lengthMs > 0`), that loop cue is active
 *   and counts down to the loop's end.
 * - Otherwise, the immediate next cue is the target cue being approached,
 *   with its gauge filling from the previous cue's offset (or 0ms).
 * - If the playhead has passed all cues, returns the last cue with `allPassed: true`.
 */
export function cueProgressAt(cues: TrackCue[], playheadMs: number): CueProgress | null {
  if (cues.length === 0) return null

  for (let i = 0; i < cues.length; i++) {
    const cue = cues[i]
    const cueEndMs =
      cue.lengthMs !== null && cue.lengthMs > 0 ? cue.offsetMs + cue.lengthMs : cue.offsetMs

    // If inside a loop, count down until the end of the loop.
    if (
      cue.lengthMs !== null &&
      cue.lengthMs > 0 &&
      playheadMs >= cue.offsetMs &&
      playheadMs < cueEndMs
    ) {
      const elapsed = Math.max(0, playheadMs - cue.offsetMs)
      const total = cue.lengthMs
      return {
        index: i,
        remainingMs: Math.max(0, cueEndMs - playheadMs),
        ratio: Math.min(1, Math.max(0, elapsed / total)),
        allPassed: false
      }
    }

    // First cue ahead of the playhead is the upcoming target cue.
    if (playheadMs < cue.offsetMs) {
      let startMs = 0
      if (i > 0) {
        const prev = cues[i - 1]
        const prevEnd =
          prev.lengthMs !== null && prev.lengthMs > 0
            ? prev.offsetMs + prev.lengthMs
            : prev.offsetMs
        startMs = Math.min(cue.offsetMs, prevEnd)
      }

      const total = Math.max(1, cue.offsetMs - startMs)
      const elapsed = Math.max(0, playheadMs - startMs)
      return {
        index: i,
        remainingMs: Math.max(0, cue.offsetMs - playheadMs),
        ratio: Math.min(1, Math.max(0, elapsed / total)),
        allPassed: false
      }
    }
  }

  // All cues passed: stick to the last cue.
  return {
    index: cues.length - 1,
    remainingMs: 0,
    ratio: 1,
    allPassed: true
  }
}
