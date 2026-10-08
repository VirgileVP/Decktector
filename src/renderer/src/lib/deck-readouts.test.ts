import { describe, expect, it } from 'vitest'

import { formatBpm, formatPitch } from './deck-readouts'

describe('formatPitch', () => {
  it('signs a positive or zero pitch with a plus', () => {
    expect(formatPitch(1.254)).toBe('+1.25%')
    expect(formatPitch(0)).toBe('+0.00%')
  })

  it('keeps the minus of a negative pitch', () => {
    expect(formatPitch(-0.5)).toBe('-0.50%')
  })

  it('shows a placeholder before the deck reports', () => {
    expect(formatPitch(null)).toBe('--.--%')
  })
})

describe('formatBpm', () => {
  it('rounds to one decimal', () => {
    expect(formatBpm(127.96)).toBe('128.0')
  })

  it('shows a placeholder when no BPM is known', () => {
    expect(formatBpm(null)).toBe('--.-')
  })
})
