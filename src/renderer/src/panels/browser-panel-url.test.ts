import { describe, expect, it } from 'vitest'

import { browserPanelUrl } from './browser-panel-url'
import { panelParams, type PanelParams } from './params'

function locationFor(pathAndSearch: string): Location {
  return new URL(`http://192.168.1.20:8080${pathAndSearch}`) as unknown as Location
}

const DECK_TWO: PanelParams = { panelId: 'deck', deckId: 2, instance: 0 }

describe('browserPanelUrl', () => {
  it('carries the remote access token over to the panel page', () => {
    const url = browserPanelUrl(locationFor('/?token=abc123'), DECK_TWO)
    expect(new URLSearchParams(url.split('?')[1]).get('token')).toBe('abc123')
  })

  it('adds no token when the page has none (token gate off)', () => {
    const url = browserPanelUrl(locationFor('/'), DECK_TWO)
    expect(url).not.toContain('token')
  })

  it('drops the main view query (`?deck=N`) in favour of the panel params', () => {
    const url = browserPanelUrl(locationFor('/?deck=4&token=abc123'), DECK_TWO)
    expect(panelParams(locationFor(url))).toEqual(DECK_TWO)
    expect(new URLSearchParams(url.split('?')[1]).has('deck')).toBe(false)
  })

  it('stays on the page path it was opened from', () => {
    const url = browserPanelUrl(locationFor('/index.html?token=abc123'), {
      panelId: 'clock',
      deckId: null,
      instance: 0
    })
    expect(url.startsWith('/index.html?')).toBe(true)
  })
})
