/**
 * The display options a panel window owns for itself, independent of the
 * main window's settings — `connectSettingsToApp('panel')` deliberately
 * stops short of applying `alwaysOnTop` or `uiScale` for exactly this
 * reason (see `settings-sync.ts`).
 */

import { isTauri } from '@tauri-apps/api/core'
import { PhysicalSize } from '@tauri-apps/api/dpi'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { UI_SCALE_RANGE } from '../state/settings'

export interface PanelWindowOptions {
  alwaysOnTop: boolean
  /** Drops the window's background so it can be keyed in a video mixer. */
  transparent: boolean
  uiScale: number
}

export const DEFAULT_PANEL_WINDOW_OPTIONS: PanelWindowOptions = {
  alwaysOnTop: false,
  transparent: false,
  uiScale: 1
}

function clampUiScale(uiScale: number): number {
  return Math.min(UI_SCALE_RANGE.max, Math.max(UI_SCALE_RANGE.min, uiScale))
}

/**
 * Windows-only Tauri quirk, no framework-level fix (tauri-apps/tauri#10318,
 * closed "not planned"): a window created `transparent: true` keeps its
 * *first paint* opaque white until an actual resize forces WebView2 to
 * recomposite. A 1-physical-pixel nudge and back is the standard
 * workaround — imperceptible, and it only runs when transparency turns on.
 */
async function nudgeRepaint(): Promise<void> {
  const currentWindow = getCurrentWindow()
  const size = await currentWindow.innerSize()
  await currentWindow.setSize(new PhysicalSize(size.width + 1, size.height))
  await currentWindow.setSize(size)
}

/**
 * Applies `options` to the current window: always-on-top through the Tauri
 * API, transparency and UI scale through the two CSS hooks the rest of the
 * app already reads — `html[data-transparent]` (see `main.css`) and the
 * `--ui-scale` custom property `settings-sync.ts` also writes for the main
 * window.
 *
 * The CSS side applies even outside Tauri (a panel window rendered in a
 * plain browser tab during development); the window-manager calls below
 * do not, since there is no window to call them on.
 */
export function applyPanelWindowOptions(options: PanelWindowOptions): void {
  document.documentElement.dataset.transparent = options.transparent ? 'on' : 'off'
  document.documentElement.style.setProperty('--ui-scale', String(clampUiScale(options.uiScale)))

  if (!isTauri()) return

  void getCurrentWindow().setAlwaysOnTop(options.alwaysOnTop)
  if (options.transparent) {
    void nudgeRepaint()
  }
}
