import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type { DeckId, TimecodeStatus } from '../model'

const IDLE_STATUS: TimecodeStatus = {
  running: false,
  sourceDeckId: null,
  hours: 0,
  minutes: 0,
  seconds: 0,
  frames: 0
}

export const useTimecodeStore = create<TimecodeStatus>(() => IDLE_STATUS)

/**
 * Wires the store to the timecode service's status ticks and hydrates it
 * from the current status. Returns the teardown function.
 *
 * Listeners are attached before the status is requested, so nothing emitted
 * while we are hydrating is lost — same shape as `connectStoreToEngine`.
 */
export function connectTimecodeToEngine(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  const pending = listen<TimecodeStatus>('timecode:status', ({ payload }) =>
    useTimecodeStore.setState(payload)
  )

  pending
    .then(() => invoke<TimecodeStatus>('timecode_status'))
    .then((status) => useTimecodeStore.setState(status))
    .catch((error) => console.error('Failed to hydrate the timecode status:', error))

  return () => {
    pending.then((unlisten) => unlisten(), ignoreAttachFailure)
  }
}

/** Picks which deck's playhead the timecode generator reads, or none. */
export function selectTimecodeSource(deckId: DeckId | null): void {
  if (!isTauri()) return
  invoke('set_timecode_source', { deckId }).catch((error) =>
    console.error('Failed to set the timecode source deck:', error)
  )
}

function ignoreAttachFailure(): void {}
