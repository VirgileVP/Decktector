import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

/**
 * Toggles the current window in and out of fullscreen: the native window in
 * the app shell, the page through the Fullscreen API in a remote browser.
 * A user action with nobody waiting on it, so a failure is logged, not thrown.
 */
export function toggleFullscreen(): void {
  switchFullscreen().catch((error) => console.error('Failed to toggle fullscreen:', error))
}

async function switchFullscreen(): Promise<void> {
  if (isTauri()) {
    const currentWindow = getCurrentWindow()
    await currentWindow.setFullscreen(!(await currentWindow.isFullscreen()))
    return
  }
  if (document.fullscreenElement) await document.exitFullscreen()
  else await document.documentElement.requestFullscreen()
}
