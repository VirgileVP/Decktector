/**
 * Reopens every panel window left open at the end of the previous session,
 * in its saved position — clamped to whatever monitors are actually
 * connected now, since the set of monitors can change between launches.
 */

import { isTauri } from '@tauri-apps/api/core'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'
import { availableMonitors } from '@tauri-apps/api/window'

import {
  beginRestoringWindows,
  endRestoringWindows,
  usePanelWindowsStore,
  type PanelWindowRecord
} from '../state/panel-windows'
import { createPanelWebviewWindow } from './open-panel-window'
import { clampToMonitors, type MonitorBounds } from './panel-geometry'
import { PANELS } from './registry'

async function readMonitorBounds(): Promise<MonitorBounds[]> {
  const monitors = await availableMonitors()
  return monitors.map((monitor) => {
    const position = monitor.position.toLogical(monitor.scaleFactor)
    const size = monitor.size.toLogical(monitor.scaleFactor)
    return { x: position.x, y: position.y, width: size.width, height: size.height }
  })
}

async function restoreOne(
  record: PanelWindowRecord,
  monitors: readonly MonitorBounds[]
): Promise<void> {
  const webview = createPanelWebviewWindow(record.label, record.params)
  if (record.geometry === null) return

  const { minSize } = PANELS[record.params.panelId]
  const geometry = clampToMonitors(record.geometry, monitors, minSize)
  await webview.setPosition(new LogicalPosition(geometry.x, geometry.y))
  await webview.setSize(new LogicalSize(geometry.width, geometry.height))

  // Written directly rather than left to the window's own next report: the
  // window's mount-time report is suppressed for the whole restore pass
  // (see `beginRestoringWindows`), and this is main-window code anyway.
  usePanelWindowsStore.getState().upsertWindow({ ...record, geometry })
}

/**
 * Reopens every panel window recorded in the panel windows store, reusing
 * each one's saved label so its instance index survives the restart. A
 * no-op when nothing was left open.
 */
export async function restorePanelWindows(): Promise<void> {
  if (!isTauri()) return

  const records = Object.values(usePanelWindowsStore.getState().windows)
  if (records.length === 0) return

  beginRestoringWindows()
  try {
    const monitors = await readMonitorBounds()
    await Promise.all(records.map((record) => restoreOne(record, monitors)))
  } finally {
    endRestoringWindows()
  }
}
