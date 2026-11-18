import { describe, expect, it } from 'vitest'

import vectors from '../../../../test-fixtures/track-identity-vectors.json'
import { DURATION_TOLERANCE_SECONDS, identityMatches, normaliseText } from './show-identity'

const track = (durationSeconds: number): Parameters<typeof identityMatches>[0] => ({
  title: 'Track',
  artist: 'Artist',
  durationSeconds
})

describe('normaliseText', () => {
  it.each(vectors.normalise)('$why', ({ in: input, out }) => {
    expect(normaliseText(input)).toBe(out)
  })

  it('is idempotent', () => {
    // References are decoded once by design, so text that still spells one
    // after decoding ("&amp;lt;" gives "&lt;") would decode again.
    const stillSpellsAReference = /&(?:#\d+|#x[0-9a-f]+|amp|lt|gt|quot|apos);/i
    const outputs = vectors.normalise
      .map(({ out }) => out)
      .filter((out) => !stillSpellsAReference.test(out))
    expect(outputs.length).toBeGreaterThan(10)
    for (const out of outputs) {
      expect(normaliseText(out)).toBe(out)
    }
  })
})

describe('identityMatches', () => {
  it.each(vectors.match)('$why', ({ a, b, matches }) => {
    expect(identityMatches(a, b)).toBe(matches)
    expect(identityMatches(b, a)).toBe(matches)
  })

  it('duration tolerance is inclusive at 2s and exclusive at 3s', () => {
    expect(DURATION_TOLERANCE_SECONDS).toBe(2)
    expect(identityMatches(track(300), track(302))).toBe(true)
    expect(identityMatches(track(300), track(298))).toBe(true)
    expect(identityMatches(track(300), track(303))).toBe(false)
    expect(identityMatches(track(300), track(297))).toBe(false)
  })
})
