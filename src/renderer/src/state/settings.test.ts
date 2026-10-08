import { describe, expect, it } from 'vitest'

import {
  DECK_VIEW_PRESETS,
  DEFAULT_SETTINGS,
  mergePersistedSettings,
  pickDeckViewToggles
} from './settings'

describe('mergePersistedSettings', () => {
  it('defaults the on-air source to auto', () => {
    expect(DEFAULT_SETTINGS.onAirSource).toBe('auto')
    expect(mergePersistedSettings({}, DEFAULT_SETTINGS).onAirSource).toBe('auto')
    expect(mergePersistedSettings(null, DEFAULT_SETTINGS).onAirSource).toBe('auto')
  })

  it('restores a stored on-air source', () => {
    expect(mergePersistedSettings({ onAirSource: 'mixer' }, DEFAULT_SETTINGS).onAirSource).toBe(
      'mixer'
    )
    expect(mergePersistedSettings({ onAirSource: 'playback' }, DEFAULT_SETTINGS).onAirSource).toBe(
      'playback'
    )
  })

  it('falls back to auto for an unknown stored on-air source', () => {
    for (const onAirSource of ['djm', 7, null, {}]) {
      expect(mergePersistedSettings({ onAirSource }, DEFAULT_SETTINGS).onAirSource).toBe('auto')
    }
  })

  it('leaves the timecode source manual unless follow-on-air was stored', () => {
    expect(DEFAULT_SETTINGS.timecodeFollowOnAir).toBe(false)
    expect(mergePersistedSettings({}, DEFAULT_SETTINGS).timecodeFollowOnAir).toBe(false)
    expect(
      mergePersistedSettings({ timecodeFollowOnAir: true }, DEFAULT_SETTINGS).timecodeFollowOnAir
    ).toBe(true)
  })

  it('keeps the other stored settings and sanitizes deck labels', () => {
    const merged = mergePersistedSettings(
      { uiScale: 1.25, deckLabels: { 1: ' CH1 ', 9: 'nope' } },
      DEFAULT_SETTINGS
    )
    expect(merged.uiScale).toBe(1.25)
    expect(merged.deckLabels).toEqual({ 1: 'CH1' })
  })
})

describe('pickDeckViewToggles', () => {
  it('keeps exactly the Deck view toggles', () => {
    expect(pickDeckViewToggles(DEFAULT_SETTINGS)).toEqual(DECK_VIEW_PRESETS.full.values)
  })
})
