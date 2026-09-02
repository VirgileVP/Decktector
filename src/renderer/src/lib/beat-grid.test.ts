import { describe, expect, it } from 'vitest'

import type { BeatGridBeat } from '../model'
import { beatInMeasureAt } from './beat-grid'

/** Two bars of 4/4 at 120 BPM, i.e. a beat every 500 ms. */
const GRID: BeatGridBeat[] = Array.from({ length: 8 }, (_, index) => ({
  offsetMs: 1000 + index * 500,
  beatInMeasure: (index % 4) + 1
}))

describe('beatInMeasureAt', () => {
  it('returns the beat the playhead is sitting on', () => {
    expect(beatInMeasureAt(GRID, 1000)).toBe(1)
    expect(beatInMeasureAt(GRID, 1499)).toBe(1)
    expect(beatInMeasureAt(GRID, 1500)).toBe(2)
    expect(beatInMeasureAt(GRID, 3000)).toBe(1)
  })

  it('holds the last beat once the grid runs out', () => {
    expect(beatInMeasureAt(GRID, 999_999)).toBe(4)
  })

  it('returns null before the first beat and without a grid', () => {
    expect(beatInMeasureAt(GRID, 999)).toBeNull()
    expect(beatInMeasureAt([], 5000)).toBeNull()
    expect(beatInMeasureAt(null, 5000)).toBeNull()
  })
})
