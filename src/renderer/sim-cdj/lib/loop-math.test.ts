import { describe, expect, it } from 'vitest'

import { activeLoopButton, loopBeatCount } from './loop-math'

/** 120 BPM: one beat is 500 ms. */
const BPM = 120

describe('loopBeatCount', () => {
  it('counts beats at the track tempo', () => {
    expect(loopBeatCount({ startMs: 1000, endMs: 3000, active: true }, BPM)).toBe(4)
  })

  it('is unknown without a tempo', () => {
    expect(loopBeatCount({ startMs: 0, endMs: 2000, active: true }, null)).toBeNull()
  })
})

describe('activeLoopButton', () => {
  it('lights the button matching a running loop', () => {
    expect(activeLoopButton({ startMs: 0, endMs: 4000, active: true }, BPM)).toBe(8)
  })

  it('lights nothing for an inactive or odd-sized loop', () => {
    expect(activeLoopButton({ startMs: 0, endMs: 4000, active: false }, BPM)).toBeNull()
    expect(activeLoopButton({ startMs: 0, endMs: 1500, active: true }, BPM)).toBeNull()
    expect(activeLoopButton(null, BPM)).toBeNull()
  })
})
