import { describe, expect, it } from 'vitest'

import { createShow, type ShowData } from './show'
import { fallbackStartFits, frameRateMismatch, readinessLabel } from './show-readiness'
import type { ShowAnalysis } from './show-validation'

function analysis(conflicts: number): ShowAnalysis {
  return { byRow: new Map(), conflicts, warnings: 0, ready: conflicts === 0 }
}

describe('frameRateMismatch', () => {
  it('a matching rate is no mismatch', () => {
    expect(frameRateMismatch(createShow([], 25), 'ebu25')).toBeNull()
    expect(frameRateMismatch(createShow([], 30), 'smpte30')).toBeNull()
  })

  it('a show in 25 against 30 output is a mismatch', () => {
    expect(frameRateMismatch(createShow([], 25), 'smpte30')).toBe(25)
  })

  it('no show is never a mismatch', () => {
    expect(frameRateMismatch(null, 'film24')).toBeNull()
  })
})

describe('readinessLabel', () => {
  it('says Ready, or counts the conflicts', () => {
    expect(readinessLabel(analysis(0))).toBe('Ready')
    expect(readinessLabel(analysis(1))).toBe('Not ready — 1 conflict')
    expect(readinessLabel(analysis(3))).toBe('Not ready — 3 conflicts')
  })
})

describe('fallbackStartFits', () => {
  it('a fallback typed at 30 fps stops fitting once the show runs at 25', () => {
    const show: ShowData = { ...createShow([], 30), fallbackStartTimecode: '22:00:00:27' }
    expect(fallbackStartFits(show)).toBe(true)
    expect(fallbackStartFits({ ...show, fps: 25 })).toBe(false)
  })
})
