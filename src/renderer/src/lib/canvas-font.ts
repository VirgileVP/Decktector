/**
 * Font family for text drawn on a canvas. Mirrors the root stack in main.css:
 * the bundled Geist face first, system fonts only as a fallback.
 */
const CANVAS_FONT_FAMILY = "'Geist Variable', system-ui, sans-serif"

/** Builds a `CanvasRenderingContext2D.font` value in the app's typeface. */
export function canvasFont(weight: number, sizePx: number): string {
  return `${weight} ${sizePx}px ${CANVAS_FONT_FAMILY}`
}

/**
 * Canvas text never triggers a web-font download by itself: until the face is
 * loaded it silently falls back. The animation loops repaint every frame, so
 * the next frame after this resolves already uses Geist; nothing has to wait
 * on it.
 */
export function preloadCanvasFont(): void {
  document.fonts.load(canvasFont(500, 9)).catch((error: unknown) => {
    console.warn('Could not preload the canvas font; text falls back to the system font.', error)
  })
}
