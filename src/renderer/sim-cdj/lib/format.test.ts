import { describe, expect, it } from 'vitest'

import { formatDuration, formatTimeTenths } from './format'

describe('formatTimeTenths', () => {
  it('shows minutes, seconds and tenths', () => {
    expect(formatTimeTenths(0)).toBe('00:00.0')
    expect(formatTimeTenths(83_456)).toBe('01:23.4')
  })

  it('clamps negative time to zero', () => {
    expect(formatTimeTenths(-500)).toBe('00:00.0')
  })
})

describe('formatDuration', () => {
  it('rounds to whole seconds', () => {
    expect(formatDuration(215_600)).toBe('3:36')
  })
})
