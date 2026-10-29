import { describe, expect, it } from 'vitest'

import { seekPositionMs, trackFraction } from './seek-math'

const STRIP = { left: 100, width: 200 }

describe('seekPositionMs', () => {
  it('maps the pointer across the strip onto the track', () => {
    expect(seekPositionMs(200, STRIP, 60_000)).toBe(30_000)
  })

  it('clamps a drag that leaves the strip', () => {
    expect(seekPositionMs(0, STRIP, 60_000)).toBe(0)
    expect(seekPositionMs(900, STRIP, 60_000)).toBe(60_000)
  })
})

describe('trackFraction', () => {
  it('is zero for a track with no length', () => {
    expect(trackFraction(500, 0)).toBe(0)
  })
})
