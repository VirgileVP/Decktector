import { describe, expect, it } from 'vitest'

import { durationLabel, rowBadges, rowInConflict, stepTrim, trimLabel } from './show-row-view'
import { MAX_TRIM_MS } from './show-timecode'

describe('trimLabel', () => {
  it('shows a signed number of frames and is stable at zero', () => {
    expect(trimLabel(80, 25)).toBe('+2f')
    expect(trimLabel(-40, 25)).toBe('−1f')
    expect(trimLabel(0, 25)).toBe('0f')
    expect(trimLabel(-0, 30)).toBe('0f')
    // Less than half a frame either way is still no trim.
    expect(trimLabel(10, 30)).toBe('0f')
  })
})

describe('stepTrim', () => {
  it('moves the trim in whole frames', () => {
    expect(stepTrim(0, 25, 1)).toBe(40)
    expect(stepTrim(40, 25, -1)).toBe(0)
    expect(stepTrim(0, 30, 1)).toBe(33)
    expect(stepTrim(33, 30, 1)).toBe(67)
    expect(trimLabel(stepTrim(67, 30, 1), 30)).toBe('+3f')
  })

  it('stops at the largest trim either way', () => {
    expect(stepTrim(MAX_TRIM_MS, 25, 1)).toBe(MAX_TRIM_MS)
    expect(stepTrim(-MAX_TRIM_MS, 25, -1)).toBe(-MAX_TRIM_MS)
  })
})

describe('durationLabel', () => {
  it('formats an unknown duration distinctly', () => {
    expect(durationLabel(637)).toBe('10:37')
    expect(durationLabel(65)).toBe('1:05')
    expect(durationLabel(0)).toBe('unknown')
  })
})

describe('rowBadges', () => {
  it('orders absent before overlap before midnight', () => {
    const badges = rowBadges([
      { kind: 'crossesMidnight' },
      { kind: 'overlap', withRowId: 'a' },
      { kind: 'absent' },
      { kind: 'overlap', withRowId: 'b' }
    ])
    expect(badges.map((badge) => [badge.kind, badge.label, badge.tone])).toEqual([
      ['absent', 'Not in playlist', 'warning'],
      ['overlap', 'Overlaps 2 rows', 'conflict'],
      ['crossesMidnight', 'Crosses midnight', 'conflict']
    ])
  })

  it('marks duplicates sharing a start as a warning, others as a conflict', () => {
    expect(rowBadges([{ kind: 'duplicate', groupSize: 2, sameStart: true }])[0].tone).toBe(
      'warning'
    )
    expect(rowBadges([{ kind: 'duplicate', groupSize: 3, sameStart: false }])[0]).toEqual({
      kind: 'duplicate',
      label: 'Listed 3×',
      tone: 'conflict'
    })
  })

  it('gives a row with no issues no badges', () => {
    expect(rowBadges([])).toEqual([])
  })
})

describe('rowInConflict', () => {
  it('is true when one badge is a conflict', () => {
    expect(
      rowInConflict(rowBadges([{ kind: 'absent' }, { kind: 'overlap', withRowId: 'a' }]))
    ).toBe(true)
  })

  it('is false for warnings only or no badge', () => {
    expect(rowInConflict(rowBadges([{ kind: 'absent' }, { kind: 'blankStart' }]))).toBe(false)
    expect(rowInConflict([])).toBe(false)
  })
})
