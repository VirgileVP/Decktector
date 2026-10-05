/**
 * Geometry of the zoom-lens mark, drawn in a SIZE×SIZE frame. Every dimension derives from the
 * inner bar module: outer bars are the inner ones divided by the magnification, so the lens
 * literally zooms the signal.
 */

export const SIZE = 1080
export const CENTER = SIZE / 2

const MAGNIFICATION = 1.75
const INNER = { width: 30, pitch: 52, halves: [44, 86, 122, 74, 40] }
const OUTER = {
  width: INNER.width / MAGNIFICATION,
  pitch: INNER.pitch / MAGNIFICATION,
  halves: [52, 34, 44, 20]
}
const LENS = { radius: 184, stroke: 48, knockoutGap: 30 }
const HANDLE = { gap: 22, length: 150, stroke: 56, angle: 45 }

const lensOuter = LENS.radius + LENS.stroke / 2
export const LENS_INNER_RADIUS = LENS.radius - LENS.stroke / 2
const firstOuterOffset = lensOuter + LENS.knockoutGap + OUTER.width / 2
const handleStart = lensOuter + HANDLE.gap + HANDLE.stroke / 2
const handleEnd = handleStart + HANDLE.length

/** Horizontal extent of the mark on either side of the lens center. */
export const MARK_HALF_WIDTH =
  firstOuterOffset + (OUTER.halves.length - 1) * OUTER.pitch + OUTER.width / 2

const radians = (degrees) => (degrees * Math.PI) / 180

/** Rounded bar whose total visible height, caps included, is 2 × visibleHalf. */
const bar = (x, cy, visibleHalf, width, color) => {
  const half = Math.max(0, visibleHalf - width / 2)
  return `<line x1="${x}" y1="${cy - half}" x2="${x}" y2="${cy + half}" stroke="${color}" stroke-width="${width}" stroke-linecap="round"/>`
}

/**
 * SVG elements of the mark. Brand rule: the lens circle is the anchor and sits on (cx, cy),
 * the exact center of the frame by default; the handle hangs off it.
 *
 * @param {{ ink: string, accent: string }} colors
 */
export function markBody(colors, cx = CENTER, cy = CENTER) {
  const innerBars = INNER.halves
    .map((half, i) => {
      const x = cx + (i - (INNER.halves.length - 1) / 2) * INNER.pitch
      return bar(x, cy, half, INNER.width, colors.accent)
    })
    .join('')
  const outerBars = OUTER.halves
    .flatMap((half, i) => {
      const dx = firstOuterOffset + i * OUTER.pitch
      return [
        bar(cx - dx, cy, half, OUTER.width, colors.ink),
        bar(cx + dx, cy, half, OUTER.width, colors.ink)
      ]
    })
    .join('')
  const [ux, uy] = [Math.cos(radians(HANDLE.angle)), Math.sin(radians(HANDLE.angle))]
  const lens = `<circle cx="${cx}" cy="${cy}" r="${LENS.radius}" fill="none" stroke="${colors.ink}" stroke-width="${LENS.stroke}"/>`
  const handle = `<line x1="${cx + ux * handleStart}" y1="${cy + uy * handleStart}" x2="${cx + ux * handleEnd}" y2="${cy + uy * handleEnd}" stroke="${colors.ink}" stroke-width="${HANDLE.stroke}" stroke-linecap="round"/>`
  return outerBars + innerBars + lens + handle
}
