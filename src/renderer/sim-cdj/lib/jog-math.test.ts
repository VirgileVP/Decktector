import { describe, expect, it } from 'vitest'

import {
  JOG_MS_PER_REVOLUTION,
  NUDGE_FULL_BEND_REVOLUTIONS_PER_SECOND,
  angleAround,
  angleDelta,
  bendForVelocity,
  isOnJogTop,
  platterAngleDegrees,
  scratchDeltaMs
} from './jog-math'

const CENTER = { x: 100, y: 100 }

describe('angleDelta', () => {
  it('folds a move across the atan2 seam into a small step', () => {
    const justBeforeSeam = Math.PI - 0.1
    const justAfterSeam = -Math.PI + 0.1
    expect(angleDelta(justBeforeSeam, justAfterSeam)).toBeCloseTo(0.2)
    expect(angleDelta(justAfterSeam, justBeforeSeam)).toBeCloseTo(-0.2)
  })
})

describe('angleAround', () => {
  it('grows clockwise on screen, where y points down', () => {
    const right = angleAround(CENTER, { x: 200, y: 100 })
    const below = angleAround(CENTER, { x: 100, y: 200 })
    expect(angleDelta(right, below)).toBeGreaterThan(0)
  })
})

describe('scratchDeltaMs', () => {
  it('maps a full clockwise turn to one platter revolution forward', () => {
    expect(scratchDeltaMs(Math.PI * 2)).toBeCloseTo(JOG_MS_PER_REVOLUTION)
    expect(scratchDeltaMs(-Math.PI)).toBeCloseTo(-JOG_MS_PER_REVOLUTION / 2)
  })
})

describe('platterAngleDegrees', () => {
  it('turns once per revolution of track time', () => {
    expect(platterAngleDegrees(JOG_MS_PER_REVOLUTION / 4)).toBeCloseTo(90)
    expect(platterAngleDegrees(JOG_MS_PER_REVOLUTION)).toBeCloseTo(0)
  })
})

describe('bendForVelocity', () => {
  it('saturates at full bend in either direction', () => {
    const fast = Math.PI * 2 * NUDGE_FULL_BEND_REVOLUTIONS_PER_SECOND * 3
    expect(bendForVelocity(fast)).toBe(1)
    expect(bendForVelocity(-fast)).toBe(-1)
    expect(bendForVelocity(0)).toBe(0)
  })
})

describe('isOnJogTop', () => {
  it('separates the platter from the edge by distance from the centre', () => {
    expect(isOnJogTop(CENTER, 100, { x: 120, y: 100 })).toBe(true)
    expect(isOnJogTop(CENTER, 100, { x: 195, y: 100 })).toBe(false)
  })
})
