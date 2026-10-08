import { describe, expect, it } from 'vitest'

import { DEFAULT_HEADER_BLOCK_VISIBILITY } from '../components/HeaderBlockVisibility'
import { DECK_VIEW_PRESETS, type DeckViewToggles } from '../state/settings'
import {
  DEFAULT_COMPACT_DECK_FIELDS,
  initialPanelDisplay,
  parseBooleanFields,
  resolvePanelDisplay
} from './panel-display'

const MAIN_DECK_VIEW: DeckViewToggles = DECK_VIEW_PRESETS.simple.values

describe('initialPanelDisplay', () => {
  it('starts a deck window on the full layout with a copy of the main deck view', () => {
    const display = initialPanelDisplay('deck', MAIN_DECK_VIEW)
    expect(display).toEqual({
      layout: 'full',
      full: MAIN_DECK_VIEW,
      compact: DEFAULT_COMPACT_DECK_FIELDS
    })
    expect(display.full).not.toBe(MAIN_DECK_VIEW)
  })

  it('starts the Time window with every piece shown', () => {
    expect(initialPanelDisplay('header-block', MAIN_DECK_VIEW)).toEqual(
      DEFAULT_HEADER_BLOCK_VISIBILITY
    )
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
    const stored = {
      layout: 'compact',
      full: { ...MAIN_DECK_VIEW, showDeckCueList: false },
      compact: { ...DEFAULT_COMPACT_DECK_FIELDS, pitch: true }
    }
    expect(resolvePanelDisplay('deck', stored, MAIN_DECK_VIEW)).toEqual(stored)
  })

  it('fills a partial deck display in from what a new window would get', () => {
    expect(resolvePanelDisplay('deck', { layout: 'compact' }, MAIN_DECK_VIEW)).toEqual({
      layout: 'compact',
      full: MAIN_DECK_VIEW,
      compact: DEFAULT_COMPACT_DECK_FIELDS
    })
  })

  it('falls back to the full layout on an unknown one', () => {
    expect(resolvePanelDisplay('deck', { layout: 'tiny' }, MAIN_DECK_VIEW).layout).toBe('full')
  })

  it('treats a window with no record as a new one', () => {
    expect(resolvePanelDisplay('deck', undefined, MAIN_DECK_VIEW)).toEqual(
      initialPanelDisplay('deck', MAIN_DECK_VIEW)
    )
  })

  it('restores the Time window visibility', () => {
    expect(
      resolvePanelDisplay(
        'header-block',
        { clock: false, timecode: true, lineup: false },
        MAIN_DECK_VIEW
      )
    ).toEqual({ clock: false, timecode: true, lineup: false })
  })
})
