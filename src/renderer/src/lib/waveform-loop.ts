/**
 * How a deck's running loop is drawn over its waveforms: the looped stretch
 * tinted between two edge lines, and a LOOP badge so a loop reads at a glance
 * even when the player doesn't say where it is.
 */

import type { ActiveLoop } from '../model'
import { canvasFont } from './canvas-font'

/** Tint over the looped stretch. */
const LOOP_FILL_ALPHA = 0.22
/** Width of the lines marking the loop in and out points. */
const LOOP_EDGE_WIDTH = 1.5
const LOOP_BADGE_LABEL = 'LOOP'
const LOOP_BADGE_FONT_SIZE = 8
const LOOP_BADGE_HEIGHT = 12
const LOOP_BADGE_PADDING_X = 4
/** Gap between the badge and the canvas edges. */
const LOOP_BADGE_MARGIN = 3
const LOOP_BADGE_RADIUS = 2

/**
 * Changes whenever what is drawn for `loop` would change, so a canvas that
 * skips identical frames still redraws when a loop starts, moves or ends.
 */
export function activeLoopKey(loop: ActiveLoop | null | undefined): string {
  if (!loop) return 'none'
  return loop.range ? `${loop.range.startMs}-${loop.range.endMs}` : 'unbounded'
}

/** Vertical extent and color of a looped stretch. */
export interface LoopSpanStyle {
  top: number
  bottom: number
  color: string
}

/** Tints the stretch between canvas x `x0` and `x1`. */
export function fillLoopSpan(
  ctx: CanvasRenderingContext2D,
  x0: number,
  x1: number,
  { top, bottom, color }: LoopSpanStyle
): void {
  ctx.save()
  ctx.fillStyle = color
  ctx.globalAlpha = LOOP_FILL_ALPHA
  ctx.fillRect(x0, top, x1 - x0, bottom - top)
  ctx.restore()
}

/** Draws the in and out lines at canvas x `x0` and `x1`. */
export function strokeLoopEdges(
  ctx: CanvasRenderingContext2D,
  x0: number,
  x1: number,
  { top, bottom, color }: LoopSpanStyle
): void {
  ctx.save()
  ctx.fillStyle = color
  for (const x of [x0, x1]) {
    ctx.fillRect(x - LOOP_EDGE_WIDTH / 2, top, LOOP_EDGE_WIDTH, bottom - top)
  }
  ctx.restore()
}

/** Draws the LOOP badge in the top-right corner of a canvas `width` wide. */
export function drawLoopBadge(
  ctx: CanvasRenderingContext2D,
  width: number,
  badgeColor: string,
  textColor: string
): void {
  ctx.save()
  ctx.font = canvasFont(700, LOOP_BADGE_FONT_SIZE)
  const badgeWidth = ctx.measureText(LOOP_BADGE_LABEL).width + LOOP_BADGE_PADDING_X * 2
  const x = width - LOOP_BADGE_MARGIN - badgeWidth
  const y = LOOP_BADGE_MARGIN
  ctx.fillStyle = badgeColor
  ctx.beginPath()
  ctx.roundRect(x, y, badgeWidth, LOOP_BADGE_HEIGHT, LOOP_BADGE_RADIUS)
  ctx.fill()
  ctx.fillStyle = textColor
  ctx.textAlign = 'center'
  ctx.textBaseline = 'middle'
  ctx.fillText(LOOP_BADGE_LABEL, x + badgeWidth / 2, y + LOOP_BADGE_HEIGHT / 2 + 0.5)
  ctx.restore()
}
