import { describe, expect, it } from 'vitest'

import type { TrackCue } from '../model'
import { cueLabel, cueProgressAt, sortCues } from './cue-progress'

function makeCue(partial: Partial<TrackCue> & { offsetMs: number }): TrackCue {
  return {
    kind: 'hotcue',
    lengthMs: null,
    button: null,
    label: null,
    color: '#305aff',
    ...partial
  }
}

describe('sortCues', () => {
  it('sorts cues chronologically by offsetMs', () => {
    const cues: TrackCue[] = [
      makeCue({ offsetMs: 60000, button: 'B' }),
      makeCue({ offsetMs: 15000, button: 'A' }),
      makeCue({ offsetMs: 90000, button: 'C' })
    ]
    const sorted = sortCues(cues)
    expect(sorted.map((c) => c.button)).toEqual(['A', 'B', 'C'])
  })
})

describe('cueLabel', () => {
  it('prefers cue label when present', () => {
    expect(makeCue({ offsetMs: 0, label: 'Drop 1', button: 'A' })).toHaveProperty('label', 'Drop 1')
    expect(cueLabel(makeCue({ offsetMs: 0, label: 'Drop 1', button: 'A' }))).toBe('Drop 1')
  })

  it('formats hot cues with button name', () => {
    expect(cueLabel(makeCue({ offsetMs: 0, kind: 'hotcue', button: 'A' }))).toBe('Hot cue A')
  })

  it('formats hot loops with button name', () => {
    expect(cueLabel(makeCue({ offsetMs: 0, kind: 'hotloop', button: 'C' }))).toBe('Hot loop C')
  })

  it('formats memory cues as MEM when unlabeled', () => {
    expect(cueLabel(makeCue({ offsetMs: 0, kind: 'memory' }))).toBe('MEM')
  })

  it('formats plain loops as LOOP when unlabeled', () => {
    expect(cueLabel(makeCue({ offsetMs: 0, kind: 'loop' }))).toBe('LOOP')
  })
})

describe('cueProgressAt', () => {
  it('returns null for an empty cue list', () => {
    expect(cueProgressAt([], 5000)).toBeNull()
  })

  it('approaches the first cue starting from 0ms', () => {
    const cues = [
      makeCue({ offsetMs: 30000, button: 'A' }),
      makeCue({ offsetMs: 60000, button: 'B' })
    ]
    const progress = cueProgressAt(cues, 10000)
    expect(progress).toEqual({
      index: 0,
      remainingMs: 20000,
      ratio: 10000 / 30000,
      allPassed: false
    })
  })

  it('advances to next cue with previous cue as start reference', () => {
    const cues = [
      makeCue({ offsetMs: 30000, button: 'A' }),
      makeCue({ offsetMs: 60000, button: 'B' })
    ]
    const progress = cueProgressAt(cues, 45000)
    expect(progress).toEqual({
      index: 1,
      remainingMs: 15000,
      ratio: 0.5,
      allPassed: false
    })
  })

  it('counts down active loop when playhead is inside loop range', () => {
    const cues = [
      makeCue({ offsetMs: 20000, lengthMs: 10000, kind: 'hotloop', button: 'A' }),
      makeCue({ offsetMs: 60000, button: 'B' })
    ]
    const inLoop = cueProgressAt(cues, 25000)
    expect(inLoop).toEqual({
      index: 0,
      remainingMs: 5000,
      ratio: 0.5,
      allPassed: false
    })
  })

  it('uses loop end as start reference for subsequent cue after loop completes', () => {
    const cues = [
      makeCue({ offsetMs: 20000, lengthMs: 10000, kind: 'hotloop', button: 'A' }),
      makeCue({ offsetMs: 60000, button: 'B' })
    ]
    // Playhead at 45s: loop ended at 30s. Target cue is at 60s.
    // Interval: 30s -> 60s (30s length). Elapsed: 45s - 30s = 15s.
    const afterLoop = cueProgressAt(cues, 45000)
    expect(afterLoop).toEqual({
      index: 1,
      remainingMs: 15000,
      ratio: 0.5,
      allPassed: false
    })
  })

  it('marks allPassed when playhead is past all cues', () => {
    const cues = [
      makeCue({ offsetMs: 30000, button: 'A' }),
      makeCue({ offsetMs: 60000, button: 'B' })
    ]
    const progress = cueProgressAt(cues, 75000)
    expect(progress).toEqual({
      index: 1,
      remainingMs: 0,
      ratio: 1,
      allPassed: true
    })
  })
})
