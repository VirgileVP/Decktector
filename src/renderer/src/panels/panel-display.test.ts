import { describe, expect, it } from 'vitest'

import { DEFAULT_HEADER_BLOCK_DISPLAY } from '../components/HeaderBlockVisibility'
import { DECK_VIEW_PRESETS, type DeckViewToggles } from '../state/settings'
import { initialPanelDisplay, parseBooleanFields, resolvePanelDisplay } from './panel-display'

const MAIN_DECK_VIEW: DeckViewToggles = DECK_VIEW_PRESETS.simple.values

describe('initialPanelDisplay', () => {
  it('starts a deck window with a copy of the main deck view', () => {
    const display = initialPanelDisplay('deck', MAIN_DECK_VIEW)
    expect(display).toEqual(MAIN_DECK_VIEW)
    expect(display).not.toBe(MAIN_DECK_VIEW)
  })

  it('starts the Time window with every piece shown and the whole lineup', () => {
    expect(initialPanelDisplay('header-block', MAIN_DECK_VIEW)).toEqual(
      DEFAULT_HEADER_BLOCK_DISPLAY
    )
    expect(DEFAULT_HEADER_BLOCK_DISPLAY.lineupView).toBe('full')
  })

  it('gives panels with nothing to choose no display', () => {
    expect(initialPanelDisplay('clock', MAIN_DECK_VIEW)).toBeNull()
    expect(initialPanelDisplay('timecode', MAIN_DECK_VIEW)).toBeNull()
  })
})

describe('parseBooleanFields', () => {
  const fallback = { a: true, b: false }

  it('takes every boolean the stored value holds', () => {
    expect(parseBooleanFields({ a: false, b: true }, fallback)).toEqual({ a: false, b: true })
  })

  it('falls back field by field on a missing or malformed value', () => {
    expect(parseBooleanFields({ a: false, b: 'yes' }, fallback)).toEqual({ a: false, b: false })
    expect(parseBooleanFields({ b: true }, fallback)).toEqual({ a: true, b: true })
  })

  it('drops keys the fallback does not know', () => {
    expect(parseBooleanFields({ a: false, stale: true }, fallback)).toEqual({ a: false, b: false })
  })

  it('falls back entirely on something that is not an object', () => {
    for (const raw of [undefined, null, 'full', 3, [true, false]]) {
      expect(parseBooleanFields(raw, fallback)).toEqual(fallback)
    }
  })
})

describe('resolvePanelDisplay', () => {
  it('restores a stored deck display', () => {
    const stored = { ...MAIN_DECK_VIEW, showDeckCueList: false, showDeckPitch: true }
    expect(resolvePanelDisplay('deck', stored, MAIN_DECK_VIEW)).toEqual(stored)
  })

  it('fills a partial deck display in from the main deck view', () => {
    expect(resolvePanelDisplay('deck', { showDeckCueList: false }, MAIN_DECK_VIEW)).toEqual({
      ...MAIN_DECK_VIEW,
      showDeckCueList: false
    })
  })

  it('treats a window with no record as a new one', () => {
    expect(resolvePanelDisplay('deck', undefined, MAIN_DECK_VIEW)).toEqual(
      initialPanelDisplay('deck', MAIN_DECK_VIEW)
    )
  })

  it('restores the Time window display', () => {
    const stored = {
      clock: false,
      timecode: true,
      lineup: false,
      notes: false,
      lineupView: 'around'
    }
    expect(resolvePanelDisplay('header-block', stored, MAIN_DECK_VIEW)).toEqual(stored)
  })

  it('falls back to the whole lineup on an unknown Time window readout', () => {
    expect(
      resolvePanelDisplay('header-block', { lineupView: 'everything' }, MAIN_DECK_VIEW)
    ).toEqual(DEFAULT_HEADER_BLOCK_DISPLAY)
  })

  it('shows notes in a Time window saved before notes existed', () => {
    expect(
      resolvePanelDisplay(
        'header-block',
        { clock: false, timecode: true, lineup: true },
        MAIN_DECK_VIEW
      )
    ).toEqual({ clock: false, timecode: true, lineup: true, notes: true, lineupView: 'full' })
  })
})
