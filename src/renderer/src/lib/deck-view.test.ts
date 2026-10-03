import { describe, expect, it } from 'vitest'

import { DECK_VIEWS, DEFAULT_DECK_VIEW, parseDeckView } from './deck-view'

describe('parseDeckView', () => {
  it.each(DECK_VIEWS)('accepts the known view "%s"', (view) => {
    expect(parseDeckView(view)).toBe(view)
  })

  it('falls back to the default when nothing is stored', () => {
    expect(parseDeckView(null)).toBe(DEFAULT_DECK_VIEW)
  })

  it('falls back to the default for an unknown or malformed value', () => {
    expect(parseDeckView('hexagon')).toBe(DEFAULT_DECK_VIEW)
    expect(parseDeckView('')).toBe(DEFAULT_DECK_VIEW)
    expect(parseDeckView('QUAD')).toBe(DEFAULT_DECK_VIEW)
  })
})
