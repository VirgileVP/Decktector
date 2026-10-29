import { describe, expect, it } from 'vitest'

import { wrapIntoLoop } from './loop-range'

const LOOP = { startMs: 1000, endMs: 3000 }

describe('wrapIntoLoop', () => {
  it('leaves positions before the loop end alone', () => {
    expect(wrapIntoLoop(2500, LOOP)).toBe(2500)
    expect(wrapIntoLoop(9000, null)).toBe(9000)
  })

  it('folds positions past the end back into the loop', () => {
    expect(wrapIntoLoop(3500, LOOP)).toBe(1500)
    expect(wrapIntoLoop(7500, LOOP)).toBe(1500)
  })

  it('leaves an empty loop alone instead of dividing by zero', () => {
    expect(wrapIntoLoop(5000, { startMs: 2000, endMs: 2000 })).toBe(5000)
  })
})
