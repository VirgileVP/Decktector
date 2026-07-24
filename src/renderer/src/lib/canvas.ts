/**
 * Keeps a canvas' backing store in sync with its CSS size and the device
 * pixel ratio, and returns a context scaled to CSS pixels.
 * Returns null while the canvas has no layout size yet.
 */
export function prepareCanvas(canvas: HTMLCanvasElement): CanvasRenderingContext2D | null {
  const dpr = window.devicePixelRatio || 1
  const width = canvas.clientWidth
  const height = canvas.clientHeight
  if (width === 0 || height === 0) return null

  const backingWidth = Math.round(width * dpr)
  const backingHeight = Math.round(height * dpr)
  if (canvas.width !== backingWidth || canvas.height !== backingHeight) {
    canvas.width = backingWidth
    canvas.height = backingHeight
  }

  const ctx = canvas.getContext('2d')
  if (!ctx) return null
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, width, height)
  return ctx
}
