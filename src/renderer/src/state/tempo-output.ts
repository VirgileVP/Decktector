import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type { TempoOutputStatus } from '../model'

const IDLE_STATUS: TempoOutputStatus = {
  running: false,
  masterDeckId: null,
  masterBpm: null,
  targetIssues: [],
  messageIssues: []
}

interface TempoOutputState {
  status: TempoOutputStatus
}

export const useTempoOutputStore = create<TempoOutputState>(() => ({
  status: IDLE_STATUS
}))

function setStatus(status: TempoOutputStatus): void {
  useTempoOutputStore.setState({ status })
}

/**
 * Wires the store to the tempo output's status ticks and hydrates it from the
 * current status. Returns the teardown function.
 *
 * Listeners are attached before the status is requested, so nothing emitted
 * while we are hydrating is lost — same shape as `connectTimecodeToEngine`.
 */
export function connectTempoOutputStatus(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  const pending = listen<TempoOutputStatus>('tempo:status', ({ payload }) => setStatus(payload))

  pending
    .then(() => invoke<TempoOutputStatus>('tempo_output_status'))
    .then(setStatus)
    .catch((error) => console.error('Failed to hydrate the tempo output status:', error))

  return () => {
    pending.then((unlisten) => unlisten(), ignoreAttachFailure)
  }
}

function ignoreAttachFailure(): void {}
