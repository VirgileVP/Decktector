import { describe, expect, it } from 'vitest'

import { boxShape } from './useFitToBox'

describe('boxShape', () => {
  it('is wide from the single-row aspect ratio up', () => {
    expect(boxShape(1600, 300)).toBe('wide')
  })

  it('is landscape when wider than tall without room for a single row', () => {
    expect(boxShape(1400, 711)).toBe('landscape')
    expect(boxShape(500, 500)).toBe('landscape')
  })

  it('is portrait when taller than wide', () => {
    expect(boxShape(490, 1147)).toBe('portrait')
  })
})
