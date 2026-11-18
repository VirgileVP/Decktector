import { describe, expect, it } from 'vitest'
import type { StateStorage } from 'zustand/middleware'

import type { ShowData } from '../lib/show'
import { createShowStore } from './show'
import {
  pushShowThenConfig,
  revisionCounter,
  subscribeShowPushes,
  toTimecodeShow
} from './show-sync'

const FRAMES_PER_HOUR_AT_25 = 3600 * 25

function memoryStorage(): StateStorage {
  const items = new Map<string, string>()
  return {
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
  fps: 25,
  fallbackStartTimecode: '23:00:00:00',
  cutWhenUnidentified: true,
  rows: [
    {
      id: 'timed',
      title: 'Strobe',
      artist: 'deadmau5',
      durationSeconds: 637,
      startTimecode: '01:00:00:10',
      trimMs: 80,
      setArtist: 'DJ Example',
      absent: false
    },
    {
      id: 'blank',
      title: 'Blank',
      artist: '',
      durationSeconds: 0,
      startTimecode: null,
      trimMs: 0,
      absent: true
    },
    {
      id: 'invalid',
      title: 'Left by an fps change',
      artist: '',
      durationSeconds: 300,
      startTimecode: '00:10:00:28',
      trimMs: -40,
      absent: false
    }
  ]
}

describe('toTimecodeShow', () => {
  it('converts start timecodes to frames and keeps blank blank', () => {
    const pushed = toTimecodeShow(SHOW, 7)
    expect(pushed.revision).toBe(7)
    expect(pushed.fallbackStartFrames).toBe(23 * FRAMES_PER_HOUR_AT_25)
    expect(pushed.cutWhenUnidentified).toBe(true)
    expect(pushed.rows[0]).toEqual({
      id: 'timed',
      title: 'Strobe',
      artist: 'deadmau5',
      durationSeconds: 637,
      startFrames: FRAMES_PER_HOUR_AT_25 + 10,
      trimMs: 80
    })
    expect(pushed.rows[1].startFrames).toBeNull()
  })

  it('pushes a label with invalid frames as blank', () => {
    expect(toTimecodeShow(SHOW, 1).rows[2].startFrames).toBeNull()
    expect(toTimecodeShow({ ...SHOW, fps: 30 }, 1).rows[2].startFrames).toBe(600 * 30 + 28)
  })

  it('maps fps to the rust frame rate names', () => {
    expect(toTimecodeShow({ ...SHOW, fps: 24 }, 1).frameRate).toBe('film24')
    expect(toTimecodeShow({ ...SHOW, fps: 25 }, 1).frameRate).toBe('ebu25')
    expect(toTimecodeShow({ ...SHOW, fps: 30 }, 1).frameRate).toBe('smpte30')
  })
})

describe('show pushes', () => {
  it('the revision increases with every change', () => {
    const store = createShowStore(memoryStorage)
    const next = revisionCounter(() => 1_000)
    const revisions: number[] = []
    subscribeShowPushes(store, (show) => {
      if (show !== null) revisions.push(toTimecodeShow(show, next()).revision)
    })

    store.getState().load(SHOW, null)
    store.getState().updateRow('timed', { trimMs: 120 })
    store.getState().setCutWhenUnidentified(false)
    expect(revisions).toEqual([1_000, 1_001, 1_002])
  })

  it('a reloaded renderer starts above the revisions it handed out before', () => {
    let clock = 5_000
    const beforeReload = revisionCounter(() => clock)
    expect([beforeReload(), beforeReload()]).toEqual([5_000, 5_001])

    // The window reloads a second later; the service still holds 5 001.
    clock += 1_000
    const afterReload = revisionCounter(() => clock)
    expect(afterReload()).toBe(6_000)
  })

  it('only a change of show is pushed', () => {
    const store = createShowStore(memoryStorage)
    const pushed: (ShowData | null)[] = []
    subscribeShowPushes(store, (show) => pushed.push(show))

    store.getState().load(SHOW, 'Friday')
    store.getState().updateRow('missing', { trimMs: 1 })
    store.getState().unload()
    expect(pushed).toEqual([SHOW, null])
  })

  it('the show is pushed before the protocol config on startup', async () => {
    const order: string[] = []
    let finishShowPush = (): void => {}
    const showPush = new Promise<void>((resolve) => {
      finishShowPush = resolve
    })

    const startup = pushShowThenConfig(
      () => {
        order.push('show sent')
        return showPush
      },
      () => order.push('config')
    )
    await Promise.resolve()
    expect(order).toEqual(['show sent'])

    order.push('show applied')
    finishShowPush()
    await startup
    expect(order).toEqual(['show sent', 'show applied', 'config'])
  })
})
