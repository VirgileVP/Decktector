/**
 * Viewport <-> pixel math for the lineup rail. Pure and ignorant of slots —
 * everything here operates on bare axis minutes and pixels.
 */

import { AXIS_MINUTES, type AxisMinutes } from './lineup-axis'

export interface TimelineViewport {
  /** Axis minute drawn at pixel 0. */
  originMinutes: AxisMinutes
  pxPerMinute: number
  widthPx: number
}

/** Zoom bounds expressed in visible minutes, not px/minute — independent of width. */
export const MIN_VISIBLE_MINUTES = 60
export const MAX_VISIBLE_MINUTES = AXIS_MINUTES

/** Drag and keyboard step; hold Alt for `FINE_SNAP_MINUTES` instead. */
export const SNAP_MINUTES = 5
export const FINE_SNAP_MINUTES = 1

/** Multiplier per wheel notch, matching WaveformDetail's zoom feel. */
export const ZOOM_STEP = 1.18

export function minutesToPx(viewport: TimelineViewport, minutes: AxisMinutes): number {
  return (minutes - viewport.originMinutes) * viewport.pxPerMinute
}

export function pxToMinutes(viewport: TimelineViewport, px: number): AxisMinutes {
  return viewport.originMinutes + px / viewport.pxPerMinute
}

/** How many axis minutes the viewport currently shows. */
export function visibleSpan(viewport: TimelineViewport): number {
  return viewport.widthPx / viewport.pxPerMinute
}

function clampVisibleMinutes(visible: number): number {
  return Math.min(MAX_VISIBLE_MINUTES, Math.max(MIN_VISIBLE_MINUTES, visible))
}

/** Re-derives `pxPerMinute` from a clamped visible span, then pins the origin to the axis. */
export function clampViewport(viewport: TimelineViewport): TimelineViewport {
  const visible = clampVisibleMinutes(viewport.widthPx / viewport.pxPerMinute)
  const pxPerMinute = viewport.widthPx / visible
  const maxOrigin = Math.max(0, AXIS_MINUTES - visible)
  const originMinutes = Math.min(maxOrigin, Math.max(0, viewport.originMinutes))
  return { originMinutes, pxPerMinute, widthPx: viewport.widthPx }
}

/** Zooms by `factor`, keeping the axis minute under `anchorPx` fixed in place. */
export function zoomAt(
  viewport: TimelineViewport,
  anchorPx: number,
  factor: number
): TimelineViewport {
  const anchorMinutes = pxToMinutes(viewport, anchorPx)
  const visible = clampVisibleMinutes(viewport.widthPx / (viewport.pxPerMinute * factor))
  const pxPerMinute = viewport.widthPx / visible
  const originMinutes = anchorMinutes - anchorPx / pxPerMinute
  return clampViewport({ originMinutes, pxPerMinute, widthPx: viewport.widthPx })
}

export function panByPx(viewport: TimelineViewport, deltaPx: number): TimelineViewport {
  return clampViewport({
    ...viewport,
    originMinutes: viewport.originMinutes + deltaPx / viewport.pxPerMinute
  })
}

/** Frames `[start, end]` with `padMinutes` of breathing room on each side. */
export function fitRange(
  widthPx: number,
  start: AxisMinutes,
  end: AxisMinutes,
  padMinutes: number
): TimelineViewport {
  const visible = clampVisibleMinutes(end - start + 2 * padMinutes)
  const pxPerMinute = widthPx / visible
  const center = (start + end) / 2
  const originMinutes = center - visible / 2
  return clampViewport({ originMinutes, pxPerMinute, widthPx })
}

export function snapMinutes(minutes: number, granularity: number): number {
  return Math.round(minutes / granularity) * granularity
}

/** Coarsest-to-finest gridlines; picks the finest that keeps ~60px between ticks. */
const TICK_STEPS = [5, 15, 30, 60, 180, 360] as const
const MIN_TICK_PX = 60

export function tickStep(pxPerMinute: number): number {
  for (const step of TICK_STEPS) {
    if (step * pxPerMinute >= MIN_TICK_PX) return step
  }
  return TICK_STEPS[TICK_STEPS.length - 1]
}
