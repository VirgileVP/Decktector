import { describe, expect, it } from 'vitest'

import {
  DECK_LABEL_MAX_LENGTH,
  MAX_PLAYER_NUMBER,
  sanitizeDeckLabel,
  sanitizeDeckLabels,
  withDeckLabel
} from './deck-labels'

describe('sanitizeDeckLabel', () => {
  it('trims surrounding whitespace', () => {
    expect(sanitizeDeckLabel('  Master CH2 ')).toBe('Master CH2')
  })

  it('caps the length', () => {
    const label = sanitizeDeckLabel('x'.repeat(DECK_LABEL_MAX_LENGTH + 10))
    expect(label).toHaveLength(DECK_LABEL_MAX_LENGTH)
  })

  it('does not leave trailing whitespace where the cap cuts', () => {
    const raw = `${'a'.repeat(DECK_LABEL_MAX_LENGTH - 1)} tail`
    expect(sanitizeDeckLabel(raw)).toBe('a'.repeat(DECK_LABEL_MAX_LENGTH - 1))
  })

  it('treats non-strings as empty', () => {
    expect(sanitizeDeckLabel(42)).toBe('')
    expect(sanitizeDeckLabel(null)).toBe('')
    expect(sanitizeDeckLabel(undefined)).toBe('')
  })
})

describe('sanitizeDeckLabels', () => {
  it('keeps valid entries, sanitized', () => {
    expect(sanitizeDeckLabels({ 1: ' Master CH2 ', 4: 'Slave CH4' })).toEqual({
      1: 'Master CH2',
      4: 'Slave CH4'
    })
  })

  it('drops empty labels and unknown player numbers', () => {
    expect(
      sanitizeDeckLabels({
        1: '   ',
        0: 'zero',
        [MAX_PLAYER_NUMBER + 1]: 'too high',
        '2.5': 'fractional',
        abc: 'named',
        3: 'ok'
      })
    ).toEqual({ 3: 'ok' })
  })

  it('drops non-string values', () => {
    expect(sanitizeDeckLabels({ 1: 7, 2: { a: 1 }, 3: 'ok' })).toEqual({ 3: 'ok' })
  })

  it.each([null, undefined, 'text', 12, ['a', 'b']])('returns no labels for %j', (raw) => {
    expect(sanitizeDeckLabels(raw)).toEqual({})
  })
})

describe('withDeckLabel', () => {
  it('sets a label without touching the others or mutating the input', () => {
    const labels = { 1: 'A' }
    expect(withDeckLabel(labels, 2, ' B ')).toEqual({ 1: 'A', 2: 'B' })
    expect(labels).toEqual({ 1: 'A' })
  })

  it('replaces an existing label', () => {
    expect(withDeckLabel({ 1: 'A' }, 1, 'Z')).toEqual({ 1: 'Z' })
  })

  it('removes the entry when the label is blank', () => {
    expect(withDeckLabel({ 1: 'A', 2: 'B' }, 1, '   ')).toEqual({ 2: 'B' })
  })
})
