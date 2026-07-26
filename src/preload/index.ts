import { contextBridge, ipcRenderer } from 'electron'
import { electronAPI } from '@electron-toolkit/preload'

import type {
  DeckId,
  DeckState,
  DeviceInfo,
  EngineSnapshot,
  NetworkState,
  TrackInfo
} from '../engine/types'
import type { DecktectiveApi, WindowPrefs } from './api'

/** Subscribes to an engine IPC channel; returns the unsubscribe function. */
function subscribe<Args extends unknown[]>(
  channel: string,
  listener: (...args: Args) => void
): () => void {
  const wrapped = (_event: Electron.IpcRendererEvent, ...args: Args): void => listener(...args)
  ipcRenderer.on(channel, wrapped as never)
  return () => ipcRenderer.removeListener(channel, wrapped as never)
}

const decktective: DecktectiveApi = {
  getSnapshot: (): Promise<EngineSnapshot> => ipcRenderer.invoke('engine:snapshot'),
  setWindowPrefs: (prefs: WindowPrefs): Promise<void> =>
    ipcRenderer.invoke('window:set-prefs', prefs),
  onNetworkState: (listener: (state: NetworkState) => void): (() => void) =>
    subscribe('engine:network:state', listener),
  onDeviceConnected: (listener: (device: DeviceInfo) => void): (() => void) =>
    subscribe('engine:device:connected', listener),
  onDeviceDisconnected: (listener: (device: DeviceInfo) => void): (() => void) =>
    subscribe('engine:device:disconnected', listener),
  onDeckState: (listener: (state: DeckState) => void): (() => void) =>
    subscribe('engine:deck:state', listener),
  onDeckTrack: (listener: (deckId: DeckId, track: TrackInfo | null) => void): (() => void) =>
    subscribe('engine:deck:track', listener)
}

if (process.contextIsolated) {
  try {
    contextBridge.exposeInMainWorld('electron', electronAPI)
    contextBridge.exposeInMainWorld('decktective', decktective)
  } catch (error) {
    console.error(error)
  }
} else {
  // @ts-ignore (define in dts)
  window.electron = electronAPI
  // @ts-ignore (define in dts)
  window.decktective = decktective
}
