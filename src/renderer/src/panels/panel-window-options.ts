/**
 * The display options a panel window owns for itself, independent of the
 * main window's settings — `connectSettingsToApp('panel')` deliberately
 * stops short of applying `alwaysOnTop` or `uiScale` for exactly this
 * reason (see `settings-sync.ts`).
 */

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
 * Applies `options` to the current window: always-on-top through the Tauri
 * API, transparency and UI scale through the two CSS hooks the rest of the
 * app already reads — `html[data-transparent]` (see `main.css`) and the
 * `--ui-scale` custom property `settings-sync.ts` also writes for the main
 * window.
 */
export function applyPanelWindowOptions(options: PanelWindowOptions): void {
  void getCurrentWindow().setAlwaysOnTop(options.alwaysOnTop)
  document.documentElement.dataset.transparent = options.transparent ? 'on' : 'off'
  document.documentElement.style.setProperty('--ui-scale', String(clampUiScale(options.uiScale)))
}
