/**
 * The channel a panel window reports its own state on. Only the main
 * window listens — see `connectPanelWindowRegistry` in
 * `../state/panel-windows.ts` for why a panel window never writes that
 * store directly.
 */

import { emit } from '@tauri-apps/api/event'

import type { WindowGeometry } from './panel-geometry'
import type { PanelDisplay } from './panel-display'
import type { PanelWindowOptions } from './panel-window-options'
import type { PanelParams } from './params'

export const PANEL_WINDOW_CHANNEL = 'panel-window:report'

export type PanelWindowReport =
  | {
      type: 'update'
      label: string
      params: PanelParams
      options: PanelWindowOptions
      display: PanelDisplay
      geometry: WindowGeometry | null
    }
  | { type: 'closed'; label: string }

export function reportPanelWindow(report: PanelWindowReport): void {
  void emit(PANEL_WINDOW_CHANNEL, report)
}
