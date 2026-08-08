import { describe, expect, it } from 'vitest'
import { isPanelWindow, panelParams, panelSearch, type PanelParams } from './params'

function locationFor(search: string): Location {
  return new URL(`http://localhost${search}`) as unknown as Location
}

describe('panelParams / panelSearch', () => {
  const cases: PanelParams[] = [
    { panelId: 'clock', deckId: null, instance: 0 },
    { panelId: 'clock', deckId: null, instance: 2 },
    { panelId: 'timecode', deckId: null, instance: 0 },
    { panelId: 'header-block', deckId: null, instance: 1 },
    { panelId: 'deck', deckId: 3, instance: 0 }
  ]

  it.each(cases)('round-trips $panelId (instance $instance)', (params) => {
    const roundTripped = panelParams(locationFor(panelSearch(params)))
    expect(roundTripped).toEqual(params)
  })

  it('rejects an unknown panel id', () => {
    expect(panelParams(locationFor('?panel=nope'))).toBeNull()
  })

  it('rejects a deck panel with no deckId', () => {
    expect(panelParams(locationFor('?panel=deck'))).toBeNull()
  })

  it('rejects a deck panel with deckId=0', () => {
    expect(panelParams(locationFor('?panel=deck&deckId=0'))).toBeNull()
  })

  it('rejects a deck panel with deckId=-1', () => {
    expect(panelParams(locationFor('?panel=deck&deckId=-1'))).toBeNull()
  })

  it('rejects a deck panel with deckId=abc', () => {
    expect(panelParams(locationFor('?panel=deck&deckId=abc'))).toBeNull()
  })

  it('defaults instance to 0 when absent', () => {
    expect(panelParams(locationFor('?panel=clock'))?.instance).toBe(0)
  })

  it('returns null for an empty search', () => {
    expect(panelParams(locationFor(''))).toBeNull()
  })

  it('returns null for a remote URL, proving the two schemes do not collide', () => {
    expect(panelParams(locationFor('?token=abc&deck=2'))).toBeNull()
  })
})

describe('isPanelWindow', () => {
  it('mirrors panelParams', () => {
    expect(isPanelWindow(locationFor('?panel=clock'))).toBe(true)
    expect(isPanelWindow(locationFor(''))).toBe(false)
  })
})
