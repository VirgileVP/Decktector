import { describe, expect, it } from 'vitest'

import { DECK_VIEW_PRESETS } from '../state/settings'
import { deckBarFields, deckBarLayout, type DeckBarFields } from './DeckBarLayout'

function allFieldsFalse(): DeckBarFields {
  return {
    artwork: false,
    trackInfo: false,
    trackTags: false,
    tcButton: false,
    transport: false,
    syncBadges: false,
    time: false,
    timeRemaining: false,
    pitch: false,
    bpm: false
  }
}

describe('deckBarLayout', () => {
  it('returns "hidden" when all fields are disabled', () => {
    expect(deckBarLayout(allFieldsFalse())).toBe('hidden')
  })

  it('returns "bar" when artwork is enabled', () => {
    expect(deckBarLayout({ ...allFieldsFalse(), artwork: true })).toBe('bar')
  })

  it('returns "bar" when trackInfo is enabled', () => {
    expect(deckBarLayout({ ...allFieldsFalse(), trackInfo: true })).toBe('bar')
  })

  it('returns "bar" when trackTags is enabled', () => {
    expect(deckBarLayout({ ...allFieldsFalse(), trackTags: true })).toBe('bar')
  })

  it('returns "rail" when identity block is hidden but readouts are enabled', () => {
    const readouts: (keyof DeckBarFields)[] = [
      'tcButton',
      'transport',
      'syncBadges',
      'time',
      'pitch',
      'bpm'
    ]

    for (const key of readouts) {
      expect(deckBarLayout({ ...allFieldsFalse(), [key]: true })).toBe('rail')
    }
  })

  it('returns "hidden" when only timeRemaining is true (since timeRemaining requires time)', () => {
    expect(deckBarLayout({ ...allFieldsFalse(), timeRemaining: true })).toBe('hidden')
  })

  it('keeps "rail" layout when readouts are enabled regardless of any non-identity field', () => {
    expect(
      deckBarLayout({
        ...allFieldsFalse(),
        bpm: true,
        time: true,
        pitch: true
      })
    ).toBe('rail')
  })
})

describe('deckBarFields', () => {
  it('maps each Deck view toggle onto its bar field', () => {
    const view = { ...DECK_VIEW_PRESETS.full.values, showDeckPitch: false, showDeckArtwork: false }
    expect(deckBarFields(view)).toEqual({
      artwork: false,
      trackInfo: true,
      trackTags: true,
      tcButton: true,
      transport: true,
      syncBadges: true,
      time: true,
      timeRemaining: true,
      pitch: false,
      bpm: true
    })
  })
})
