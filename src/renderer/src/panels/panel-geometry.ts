/**
 * Fits a saved window position back onto whatever monitors are actually
 * connected. Restoring a saved position blindly puts a window off-screen
 * when the monitor it lived on is gone — a real case for a laptop that ran
 * the last set on a projector.
 */

export interface WindowGeometry {
  x: number
  y: number
  width: number
  height: number
}

export interface MonitorBounds {
  x: number
  y: number
  width: number
  height: number
}

interface MinSize {
  width: number
  height: number
}

function overlapArea(geometry: WindowGeometry, monitor: MonitorBounds): number {
  const overlapWidth = Math.max(
    0,
    Math.min(geometry.x + geometry.width, monitor.x + monitor.width) -
      Math.max(geometry.x, monitor.x)
  )
  const overlapHeight = Math.max(
    0,
    Math.min(geometry.y + geometry.height, monitor.y + monitor.height) -
      Math.max(geometry.y, monitor.y)
  )
  return overlapWidth * overlapHeight
}

/** The monitor `geometry` overlaps most, falling back to the first monitor when it overlaps none. */
function bestMonitorFor(
  geometry: WindowGeometry,
  monitors: readonly MonitorBounds[]
): MonitorBounds {
  let best = monitors[0]
  let bestArea = overlapArea(geometry, best)
  for (const monitor of monitors.slice(1)) {
    const area = overlapArea(geometry, monitor)
    if (area > bestArea) {
      best = monitor
      bestArea = area
    }
  }
  return best
}

/**
 * Fits `geometry` onto whichever monitor it most overlaps, falling back to
 * `monitors[0]` when it overlaps none. Size is clamped to the monitor's
 * size and floored at `minSize`; position is clamped so the window stays
 * fully inside the chosen monitor.
 */
export function clampToMonitors(
  geometry: WindowGeometry,
  monitors: readonly MonitorBounds[],
  minSize: MinSize
): WindowGeometry {
  if (monitors.length === 0) return geometry

  const monitor = bestMonitorFor(geometry, monitors)

  const width = Math.max(minSize.width, Math.min(geometry.width, monitor.width))
  const height = Math.max(minSize.height, Math.min(geometry.height, monitor.height))

  const maxX = monitor.x + Math.max(0, monitor.width - width)
  const maxY = monitor.y + Math.max(0, monitor.height - height)
  const x = Math.min(Math.max(geometry.x, monitor.x), maxX)
  const y = Math.min(Math.max(geometry.y, monitor.y), maxY)

  return { x, y, width, height }
}
