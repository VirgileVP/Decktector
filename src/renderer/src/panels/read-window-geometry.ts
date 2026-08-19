/**
 * Reads the current window's geometry in logical pixels — the unit
 * `setPosition` / `setSize` expect on restore (task 5.3) and the unit
 * `clampToMonitors` already works in.
 */

import { getCurrentWindow } from '@tauri-apps/api/window'

import type { WindowGeometry } from './panel-geometry'

export async function readWindowGeometry(): Promise<WindowGeometry> {
  const currentWindow = getCurrentWindow()
  const [position, size, scaleFactor] = await Promise.all([
    currentWindow.outerPosition(),
    currentWindow.innerSize(),
    currentWindow.scaleFactor()
  ])
  return {
    x: position.x / scaleFactor,
    y: position.y / scaleFactor,
    width: size.width / scaleFactor,
    height: size.height / scaleFactor
  }
}
