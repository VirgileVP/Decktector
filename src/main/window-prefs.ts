import { BrowserWindow, ipcMain, powerSaveBlocker } from 'electron'

import type { WindowPrefs } from '../preload/api'

/** Id of the running blocker, or null when the screen is free to sleep. */
let blockerId: number | null = null

function setPreventSleep(enabled: boolean): void {
  if (enabled && blockerId === null) {
    // 'prevent-display-sleep' also implies preventing app suspension.
    blockerId = powerSaveBlocker.start('prevent-display-sleep')
  } else if (!enabled && blockerId !== null) {
    powerSaveBlocker.stop(blockerId)
    blockerId = null
  }
}

/** The payload crosses a process boundary, so nothing about it is trusted. */
function parseWindowPrefs(value: unknown): WindowPrefs {
  const prefs = value as Partial<WindowPrefs> | null
  if (
    typeof prefs !== 'object' ||
    prefs === null ||
    typeof prefs.alwaysOnTop !== 'boolean' ||
    typeof prefs.preventSleep !== 'boolean'
  ) {
    throw new Error('Invalid window preferences payload')
  }
  return { alwaysOnTop: prefs.alwaysOnTop, preventSleep: prefs.preventSleep }
}

/** Lets the renderer drive the window flags its own settings panel exposes. */
export function handleWindowPrefsOverIpc(): void {
  ipcMain.handle('window:set-prefs', (event, payload: unknown) => {
    const prefs = parseWindowPrefs(payload)
    BrowserWindow.fromWebContents(event.sender)?.setAlwaysOnTop(prefs.alwaysOnTop)
    setPreventSleep(prefs.preventSleep)
  })
}
