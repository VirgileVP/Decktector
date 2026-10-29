import { describe, expect, it } from 'vitest'

import { SCRATCH_GRAIN_LENGTH_MS, SCRATCH_MAX_RATE, SCRATCH_MIN_RATE } from './audio-constants'
import { scratchGrain } from './scratch-math'

describe('scratchGrain', () => {
  it('needs a previous sample', () => {
    expect(scratchGrain(null, { positionMs: 100, atMs: 0 })).toBeNull()
  })

  it('is null when nothing moved or time did not advance', () => {
    const previous = { positionMs: 1000, atMs: 0 }
    expect(scratchGrain(previous, { positionMs: 1000, atMs: 40 })).toBeNull()
    expect(scratchGrain(previous, { positionMs: 1040, atMs: 0 })).toBeNull()
  })

  it('plays at the speed of the motion from where it started', () => {
    const grain = scratchGrain({ positionMs: 1000, atMs: 0 }, { positionMs: 1040, atMs: 40 })
    expect(grain?.startMs).toBe(1000)
    expect(grain?.rate).toBeCloseTo(1)
    expect(grain?.durationSeconds).toBeCloseTo(0.05)
  })

  it('plays reversed motion forward from the earlier position', () => {
    const grain = scratchGrain({ positionMs: 1080, atMs: 0 }, { positionMs: 1000, atMs: 40 })
    expect(grain?.startMs).toBe(1000)
    expect(grain?.rate).toBeCloseTo(2)
  })

  it('clamps the speed', () => {
    const fast = scratchGrain({ positionMs: 0, atMs: 0 }, { positionMs: 4000, atMs: 40 })
    const slow = scratchGrain({ positionMs: 0, atMs: 0 }, { positionMs: 2, atMs: 4000 })
    expect(fast?.rate).toBe(SCRATCH_MAX_RATE)
    expect(slow?.rate).toBe(SCRATCH_MIN_RATE)
  })

  it('caps the grain length', () => {
    const grain = scratchGrain({ positionMs: 0, atMs: 0 }, { positionMs: 300, atMs: 1000 })
    expect(grain?.durationSeconds).toBeCloseTo(SCRATCH_GRAIN_LENGTH_MS / 1000)
  })
})
