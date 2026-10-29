import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type { RemoteStatus } from '../model'
import { REMOTE_DEFAULT_MAX_CLIENTS, REMOTE_DEFAULT_PORT } from './settings'

const IDLE_STATUS: RemoteStatus = {
  listening: false,
  port: REMOTE_DEFAULT_PORT,
  token: null,
  clientCount: 0,
  error: null,
  useTls: false,
  maxClients: REMOTE_DEFAULT_MAX_CLIENTS,
  requireToken: true
}

export const useRemoteStore = create<RemoteStatus>(() => IDLE_STATUS)

/**
 * Wires the store to the remote service's status changes and hydrates it
 * from the current status. Returns the teardown function.
 *
 * Listeners are attached before the status is requested, so nothing emitted
 * while we are hydrating is lost — same shape as `connectTimecodeToEngine`.
 */
export function connectRemoteToEngine(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  const pending = listen<RemoteStatus>('remote:status', ({ payload }) =>
    useRemoteStore.setState(payload)
  )

  pending
    .then(() => invoke<RemoteStatus>('remote_status'))
    .then((status) => useRemoteStore.setState(status))
    .catch((error) => console.error('Failed to hydrate the remote status:', error))

  return () => {
    pending.then((unlisten) => unlisten(), ignoreAttachFailure)
  }
}

function ignoreAttachFailure(): void {}
