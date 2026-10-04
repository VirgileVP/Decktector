/**
 * The set of currently open panel windows, keyed by label, persisted so
 * they can be restored on the next launch (task 5.3).
 *
 * Only the main window writes this store — two webviews persisting to the
 * same `localStorage` key would clobber each other. A panel window only
 * *reports* its state over `PANEL_WINDOW_CHANNEL` (emitted from
 * `PanelChrome`); `connectPanelWindowRegistry`, mounted from the main
 * window only, is what actually applies those reports here.
 */

import { isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import type { WindowGeometry } from '../panels/panel-geometry'
import type { PanelWindowOptions } from '../panels/panel-window-options'
import { PANEL_WINDOW_CHANNEL, type PanelWindowReport } from '../panels/panel-window-report'
import type { PanelParams } from '../panels/params'

const PANEL_WINDOWS_STORAGE_KEY = 'decktector.panel-windows'

export interface PanelWindowRecord {
  label: string
  params: PanelParams
  options: PanelWindowOptions
  compact: boolean
  geometry: WindowGeometry | null
}

interface PanelWindowsState {
  windows: Record<string, PanelWindowRecord>
}

interface PanelWindowsActions {
  upsertWindow: (record: PanelWindowRecord) => void
  removeWindow: (label: string) => void
}

export const usePanelWindowsStore = create<PanelWindowsState & PanelWindowsActions>()(
  persist(
    (set) => ({
      windows: {},
      upsertWindow: (record) =>
        set((state) => ({ windows: { ...state.windows, [record.label]: record } })),
      removeWindow: (label) =>
        set((state) => {
          const windows = { ...state.windows }
          delete windows[label]
          return { windows }
        })
    }),
    {
      name: PANEL_WINDOWS_STORAGE_KEY,
      // Actions are not state: keeping them out avoids writing dead keys to storage.
      partialize: (state) => ({ windows: state.windows })
    }
  )
)

/**
 * A window's own record from the store, keyed by its own label — the seed
 * a panel window uses for whatever it has to know about itself before its
 * first mount-time report goes out (its display options, whether it is in
 * its compact layout). Backed by the same persisted `localStorage` the
 * main window wrote it to, since that storage — unlike JS state — is
 * shared by every webview on the origin.
 */
export function panelWindowRecordFor(label: string): PanelWindowRecord | undefined {
  return usePanelWindowsStore.getState().windows[label]
}

/**
 * `panelWindowRecordFor(getCurrentWindow().label)`, safely: `getCurrentWindow()`
 * reads `window.__TAURI_INTERNALS__.metadata`, which throws outside a real
 * Tauri webview (a panel window rendered in a plain browser tab during
 * development) — exactly the case a `useState` initializer runs into, since
 * it fires on every mount regardless of `isTauri()`. `undefined` outside
 * Tauri means "no record", which is also the correct answer there.
 */
export function currentPanelWindowRecord(): PanelWindowRecord | undefined {
  return isTauri() ? panelWindowRecordFor(getCurrentWindow().label) : undefined
}

/**
 * Set for the duration of `restorePanelWindows` (see `restore-panel-windows.ts`),
 * so the flurry of "on mount" reports from every window it recreates does not
 * get written back to the store as if the user had just opened them fresh.
 */
let restoringWindows = false

export function beginRestoringWindows(): void {
  restoringWindows = true
}

export function endRestoringWindows(): void {
  restoringWindows = false
}

/**
 * Applies every panel window report to the store. Main-window-only — see
 * the module doc comment above. Returns the teardown function.
 */
export function connectPanelWindowRegistry(): () => void {
  if (!isTauri()) return () => {}

  let unlisten: (() => void) | null = null
  void listen<PanelWindowReport>(PANEL_WINDOW_CHANNEL, (event) => {
    if (restoringWindows) return

    const report = event.payload
    const { upsertWindow, removeWindow } = usePanelWindowsStore.getState()
    if (report.type === 'closed') {
      removeWindow(report.label)
    } else {
      upsertWindow({
        label: report.label,
        params: report.params,
        options: report.options,
        compact: report.compact,
        geometry: report.geometry
      })
    }
  }).then((stopListening) => {
    unlisten = stopListening
  })

  return () => unlisten?.()
}
