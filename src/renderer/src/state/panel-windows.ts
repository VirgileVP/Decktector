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
import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import type { WindowGeometry } from '../panels/panel-geometry'
import type { PanelWindowOptions } from '../panels/panel-window-options'
import { PANEL_WINDOW_CHANNEL, type PanelWindowReport } from '../panels/panel-window-report'
import type { PanelParams } from '../panels/params'

const PANEL_WINDOWS_STORAGE_KEY = 'decktective.panel-windows'

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
