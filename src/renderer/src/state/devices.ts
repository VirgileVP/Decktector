import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type { DiscoveredDevice } from '../model'

interface DevicesState {
  devices: DiscoveredDevice[]
}

export const useDevicesStore = create<DevicesState>(() => ({ devices: [] }))

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

  const pending = listen<DiscoveredDevice[]>('devices:list', ({ payload }) =>
    useDevicesStore.setState({ devices: payload })
  )

  pending
    .then(() => invoke<DiscoveredDevice[]>('list_devices'))
    .then((devices) => useDevicesStore.setState({ devices }))
    .catch((error) => console.error('Failed to hydrate the Pro DJ Link device list:', error))

  return () => {
    pending.then((unlisten) => unlisten(), ignoreAttachFailure)
  }
}

function ignoreAttachFailure(): void {}
