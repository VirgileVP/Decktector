/**
 * Closes every panel window when the main window is asked to close.
 * Without this, closing the main window leaves the app running headless —
 * Tauri only exits once the last window is gone, and any open panel window
 * would still count.
 */

import { isTauri } from '@tauri-apps/api/core'
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { parsePanelLabel } from './window-label'

/** Mounted from the main window only. Returns the teardown function. */
export function connectMainWindowCloseCascade(): () => void {
  if (!isTauri()) return () => {}

  let unlisten: (() => void) | null = null
  void getCurrentWindow()
    .onCloseRequested(async () => {
      const windows = await getAllWebviewWindows()
      await Promise.all(
        windows
          .filter((window) => parsePanelLabel(window.label) !== null)
          .map((window) => window.close())
      )
    })
    .then((stopListening) => {
      unlisten = stopListening
    })

  return () => unlisten?.()
}
