import { describe, expect, it } from 'vitest'
import type { StateStorage } from 'zustand/middleware'

import type { ShowData } from '../lib/show'
import { createShowStore } from './show'

const STORAGE_KEY = 'decktector.show'

/** Stands in for `localStorage`: shared by stores built over it, like windows of one app. */
function memoryStorage(): StateStorage & { items: Map<string, string> } {
  const items = new Map<string, string>()
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => {
      items.set(key, value)
    },
    removeItem: (key) => {
      items.delete(key)
    }
  }
}

const SHOW: ShowData = {
  fps: 30,
  fallbackStartTimecode: '23:00:00:00',
  cutWhenUnidentified: false,
  rows: [
    {
      id: 'row-1',
      title: 'Strobe',
      artist: 'deadmau5',
      durationSeconds: 637,
      startTimecode: '00:10:00:28',
      trimMs: 33,
      setArtist: 'DJ Example',
      absent: false
    },
    {
      id: 'row-2',
      title: 'Blank',
      artist: '',
      durationSeconds: 0,
      startTimecode: null,
      trimMs: 0,
      absent: true
    }
  ]
}

describe('useShow', () => {
  it('starts with no show', () => {
    const store = createShowStore(memoryStorage)
    expect(store.getState().show).toBeNull()
    expect(store.getState().name).toBeNull()
  })

  it('a stored show survives a reload', () => {
    const storage = memoryStorage()
    createShowStore(() => storage)
      .getState()
      .load(SHOW, 'Friday')

    const reloaded = createShowStore(() => storage).getState()
    expect(reloaded.show).toEqual(SHOW)
    expect(reloaded.name).toBe('Friday')
  })

  it('setFps keeps the stored hh mm ss ff labels unchanged', () => {
    const store = createShowStore(memoryStorage)
    store.getState().load(SHOW, null)
    store.getState().setFps(25)

    const show = store.getState().show
    expect(show?.fps).toBe(25)
    expect(show?.rows.map((row) => row.startTimecode)).toEqual(['00:10:00:28', null])
  })

  it('a start timecode label survives a persisted reload verbatim', () => {
    const storage = memoryStorage()
    const store = createShowStore(() => storage)
    store.getState().load(SHOW, null)
    store.getState().setFps(25)
    store.getState().updateRow('row-2', { startTimecode: '01:02:03:04' })

    expect(storage.items.get(STORAGE_KEY)).toContain('"00:10:00:28"')
    const reloaded = createShowStore(() => storage).getState().show
    expect(reloaded?.rows.map((row) => row.startTimecode)).toEqual(['00:10:00:28', '01:02:03:04'])
  })

  it('updateRow on a missing id is a no op returning the same state reference', () => {
    const store = createShowStore(memoryStorage)
    store.getState().load(SHOW, null)
    const before = store.getState()

    store.getState().updateRow('missing', { trimMs: 40 })
    store.getState().removeRow('missing')
    expect(store.getState()).toBe(before)

    store.getState().unload()
    const empty = store.getState()
    store.getState().updateRow('row-1', { trimMs: 40 })
    store.getState().setFps(24)
    expect(store.getState()).toBe(empty)
  })

  it('updateRow and removeRow change only the row they name', () => {
    const store = createShowStore(memoryStorage)
    store.getState().load(SHOW, null)
    store.getState().updateRow('row-1', { trimMs: -80, setArtist: 'Closer' })
    store.getState().removeRow('row-2')

    expect(store.getState().show?.rows).toEqual([
      { ...SHOW.rows[0], trimMs: -80, setArtist: 'Closer' }
    ])
  })

  it('unload returns to no show', () => {
    const storage = memoryStorage()
    const store = createShowStore(() => storage)
    store.getState().load(SHOW, 'Friday')
    store.getState().unload()

    expect(store.getState().show).toBeNull()
    expect(store.getState().name).toBeNull()
    expect(createShowStore(() => storage).getState().show).toBeNull()
  })

  it('a first import creates the show and a later one replaces only the rows', () => {
    const store = createShowStore(memoryStorage)
    store.getState().applyImport(SHOW.rows)
    expect(store.getState().show).toMatchObject({ fps: 30, cutWhenUnidentified: false })

    store.getState().setCutWhenUnidentified(true)
    store.getState().applyImport([SHOW.rows[1]])
    expect(store.getState().show).toMatchObject({ cutWhenUnidentified: true, rows: [SHOW.rows[1]] })
  })

  it('fillBlanks numbers the blank rows at the show fps', () => {
    const store = createShowStore(memoryStorage)
    store
      .getState()
      .load({ ...SHOW, fps: 25, rows: [{ ...SHOW.rows[1], startTimecode: null }] }, null)
    store.getState().fillBlanks(25 * 60)
    expect(store.getState().show?.rows[0].startTimecode).toBe('00:00:00:00')
  })

  it('mergeDuplicates folds copies of a track at one start into the first', () => {
    const store = createShowStore(memoryStorage)
    const copy = { ...SHOW.rows[0], id: 'row-1-copy', trimMs: 0 }
    store.getState().load({ ...SHOW, rows: [SHOW.rows[0], copy, SHOW.rows[1]] }, null)
    store.getState().mergeDuplicates()
    expect(store.getState().show?.rows.map((row) => row.id)).toEqual(['row-1', 'row-2'])
  })

  it('a stored show of another shape is dropped, not half read', () => {
    const storage = memoryStorage()
    storage.setItem(
      STORAGE_KEY,
      JSON.stringify({ state: { show: { fps: 29.97, rows: 'none' }, name: 'Old' }, version: 1 })
    )
    const state = createShowStore(() => storage).getState()
    expect(state.show).toBeNull()
    expect(state.name).toBe('Old')
  })
})
