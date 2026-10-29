import { describe, expect, it } from 'vitest'

import {
  CHANNEL_FADER_TRAVEL,
  PITCH_FADER_TRAVEL,
  fractionFromTop,
  valueFromFraction
} from './fader-math'

describe('pitch fader', () => {
  it('is faster (+1) at the bottom, as on a CDJ', () => {
    expect(valueFromFraction(1, PITCH_FADER_TRAVEL)).toBe(1)
    expect(valueFromFraction(0, PITCH_FADER_TRAVEL)).toBe(-1)
    expect(valueFromFraction(0.5, PITCH_FADER_TRAVEL)).toBe(0)
  })

  it('draws a positive pitch below the middle', () => {
    expect(fractionFromTop(0.5, PITCH_FADER_TRAVEL)).toBeCloseTo(0.75)
  })
})

describe('channel fader', () => {
  it('is full at the top', () => {
    expect(valueFromFraction(0, CHANNEL_FADER_TRAVEL)).toBe(1)
    expect(fractionFromTop(1, CHANNEL_FADER_TRAVEL)).toBe(0)
  })

  it('clamps pointers beyond the travel', () => {
    expect(valueFromFraction(-3, CHANNEL_FADER_TRAVEL)).toBe(1)
    expect(valueFromFraction(7, CHANNEL_FADER_TRAVEL)).toBe(0)
  })
})
