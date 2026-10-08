import { describe, expect, it } from 'vitest'

import { migratePanelWindowsState, PANEL_WINDOWS_STORE_VERSION } from './panel-windows-migration'
import { DECK_VIEW_PRESETS } from './settings'

const PARAMS = { panelId: 'deck', deckId: 2, instance: 0 }

describe('migratePanelWindowsState', () => {
  it('turns a compact window into the Simple view preset', () => {
    const migrated = migratePanelWindowsState(
      { windows: { 'panel-deck-2-0': { label: 'panel-deck-2-0', params: PARAMS, compact: true } } },
      0
    )
    expect(migrated).toEqual({
      windows: {
        'panel-deck-2-0': {
          label: 'panel-deck-2-0',
          params: PARAMS,
          display: DECK_VIEW_PRESETS.simple.values
        }
      }
    })
  })

  it('drops the flag of a window that was not compact, leaving its display to the default', () => {
    const migrated = migratePanelWindowsState(
      {
        windows: { 'panel-deck-2-0': { label: 'panel-deck-2-0', params: PARAMS, compact: false } }
      },
      0
    )
    expect(migrated).toEqual({
      windows: { 'panel-deck-2-0': { label: 'panel-deck-2-0', params: PARAMS } }
    })
  })

  it('leaves state already at the current version untouched', () => {
    const current = { windows: { a: { compact: true } } }
    expect(migratePanelWindowsState(current, PANEL_WINDOWS_STORE_VERSION)).toBe(current)
  })

  it('passes malformed state through for the store to discard', () => {
    expect(migratePanelWindowsState(null, 0)).toBeNull()
    expect(migratePanelWindowsState({ windows: 'nope' }, 0)).toEqual({ windows: 'nope' })
  })
})
