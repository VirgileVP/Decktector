import { describe, expect, it } from 'vitest'

import {
  MAX_STRIP_DEVICE_PX,
  STRIP_SPAN,
  stripCovers,
  stripRangeFor,
  stripWidthPx
} from './waveform-strip'

describe('stripWidthPx', () => {
  it('spans STRIP_SPAN canvas widths when the backing store allows it', () => {
    expect(stripWidthPx(1000, 2)).toBe(1000 * STRIP_SPAN)
  })

  it('clamps so the backing store stays within the device pixel budget', () => {
    expect(stripWidthPx(2000, 2)).toBe(MAX_STRIP_DEVICE_PX / 2)
  })

  it('never returns a strip narrower than the canvas itself', () => {
    expect(stripWidthPx(6000, 2)).toBe(6000)
  })
})

describe('stripRangeFor', () => {
  it('centres the strip on the given time', () => {
    const range = stripRangeFor(10_000, 10, 300)
    expect(range.startMs).toBe(8500)
    expect(range.endMs).toBe(11_500)
  })
})

describe('stripCovers', () => {
  const range = stripRangeFor(10_000, 10, 300)

  it('covers a window sitting inside it', () => {
    expect(stripCovers(range, 9500, 10_500)).toBe(true)
  })

  it('covers the window it was built around', () => {
    expect(stripCovers(range, 10_000 - 500, 10_000 + 500)).toBe(true)
  })

  it('does not cover a window running off either end', () => {
    expect(stripCovers(range, 8400, 9000)).toBe(false)
    expect(stripCovers(range, 11_000, 11_600)).toBe(false)
  })
})
