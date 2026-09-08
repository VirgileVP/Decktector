import { describe, expect, it } from 'vitest'
import type { PanelParams } from './params'
import { nextInstance, panelWindowLabel, parsePanelLabel } from './window-label'

const LABEL_CHARSET = /^[A-Za-z0-9\-_/:]+$/

describe('panelWindowLabel', () => {
  const cases: PanelParams[] = [
    { panelId: 'clock', deckId: null, instance: 0 },
    { panelId: 'timecode', deckId: null, instance: 3 },
    { panelId: 'header-block', deckId: null, instance: 0 },
    { panelId: 'deck', deckId: 2, instance: 1 }
  ]

  it.each(cases)('builds a label matching the capability glob for $panelId', (params) => {
    const label = panelWindowLabel(params)
    expect(label).toMatch(LABEL_CHARSET)
    expect(label.startsWith('panel-')).toBe(true)
  })

  it('uses the placeholder segment for non-deck panels', () => {
    expect(panelWindowLabel({ panelId: 'clock', deckId: null, instance: 0 })).toBe(
      'panel-clock-x-0'
    )
  })

  it('carries the deck id for deck panels', () => {
    expect(panelWindowLabel({ panelId: 'deck', deckId: 2, instance: 1 })).toBe('panel-deck-2-1')
  })
})

describe('nextInstance', () => {
  it('returns 0 when nothing is taken', () => {
    expect(nextInstance('clock', null, [])).toBe(0)
  })

  it('fills a gap rather than always growing', () => {
    const taken = ['panel-clock-x-0', 'panel-clock-x-2']
    expect(nextInstance('clock', null, taken)).toBe(1)
  })

  it('returns the next index past a contiguous run', () => {
    const taken = ['panel-clock-x-0', 'panel-clock-x-1', 'panel-clock-x-2']
    expect(nextInstance('clock', null, taken)).toBe(3)
  })

  it('ignores labels of other panels', () => {
    const taken = ['panel-timecode-x-0', 'panel-header-block-x-0']
    expect(nextInstance('clock', null, taken)).toBe(0)
  })

  it('ignores labels of other decks', () => {
    const taken = ['panel-deck-1-0']
    expect(nextInstance('deck', 2, taken)).toBe(0)
  })

  it('tracks instances per deck independently', () => {
    const taken = ['panel-deck-1-0', 'panel-deck-2-0', 'panel-deck-2-1']
    expect(nextInstance('deck', 2, taken)).toBe(2)
    expect(nextInstance('deck', 1, taken)).toBe(1)
  })
})

describe('parsePanelLabel', () => {
  const cases: PanelParams[] = [
    { panelId: 'clock', deckId: null, instance: 0 },
    { panelId: 'timecode', deckId: null, instance: 5 },
    { panelId: 'header-block', deckId: null, instance: 2 },
    { panelId: 'deck', deckId: 4, instance: 0 }
  ]

  it.each(cases)('round-trips $panelId through panelWindowLabel', (params) => {
    expect(parsePanelLabel(panelWindowLabel(params))).toEqual(params)
  })

  it('returns null for the main window label', () => {
    expect(parsePanelLabel('main')).toBeNull()
  })

  it('returns null for malformed input', () => {
    expect(parsePanelLabel('panel-clock')).toBeNull()
    expect(parsePanelLabel('panel-nope-x-0')).toBeNull()
    expect(parsePanelLabel('panel-deck-x-0')).toBeNull()
    expect(parsePanelLabel('not-a-panel-label')).toBeNull()
  })
})
