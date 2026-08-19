import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type { DiscoveredDevice } from '../model'

interface DevicesState {
  devices: DiscoveredDevice[]
  /** Wall-clock time of the last non-empty device list, kept across a drop to
   *  zero so a "connection failed" moment can say when gear was last seen
   *  instead of just that it is gone now. Client-observed, not protocol data
   *  — the backend does not track this. `null` until the first device ever
   *  appears this session. */
  lastSeenAt: number | null
}

export const useDevicesStore = create<DevicesState>(() => ({ devices: [], lastSeenAt: null }))

function setDevices(devices: DiscoveredDevice[]): void {
  useDevicesStore.setState((state) => ({
    devices,
    lastSeenAt: devices.length > 0 ? Date.now() : state.lastSeenAt
  }))
}

/**
 * Wires the store to the Pro DJ Link device watcher's list changes and
 * hydrates it from the devices currently known. Returns the teardown
 * function.
 *
 * Listeners are attached before the list is requested, so nothing emitted
 * while we are hydrating is lost — same shape as `connectTimecodeToEngine`.
 */
export function connectDevicesToEngine(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  const pending = listen<DiscoveredDevice[]>('devices:list', ({ payload }) => setDevices(payload))

  pending
    .then(() => invoke<DiscoveredDevice[]>('list_devices'))
    .then(setDevices)
    .catch((error) => console.error('Failed to hydrate the Pro DJ Link device list:', error))

  return () => {
    pending.then((unlisten) => unlisten(), ignoreAttachFailure)
  }
}

function ignoreAttachFailure(): void {}
