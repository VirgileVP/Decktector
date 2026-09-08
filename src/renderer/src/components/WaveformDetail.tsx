import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo, WaveformSegment } from '../model'
import { prepareCanvas } from '../lib/canvas'
import { startFrameLoop } from '../lib/frame-loop'
import { canvasInk, type CanvasInk } from '../lib/ink'
import { estimatePlayheadMs } from '../lib/playhead'
import { stripCovers, stripRangeFor, stripWidthPx, type StripRange } from '../lib/waveform-strip'
import { useSettings } from '../state/settings'
import { useDecktectiveStore } from '../state/store'

/** Default seconds of audio visible across the full width of the canvas. */
const DEFAULT_WINDOW_SEC = 8
const MIN_WINDOW_SEC = 2
const MAX_WINDOW_SEC = 60
/** Multiplier applied per wheel notch when zooming. */
const ZOOM_STEP = 1.18
const PLAYED_DIM_ALPHA = 0.32
/** Alpha the not-yet-played side of the envelope is drawn at. */
const LIVE_ALPHA = 0.95
/** Height reserved at the bottom for the beat-grid tick strip. */
const TICK_STRIP = 12
/** Height reserved at the top for cue flags. */
const FLAG_STRIP = 14

interface WaveformDetailProps {
  deckId: DeckId
  track: TrackInfo | null
  accentColor: string
}

/** Vertical layout of the envelope, derived from the canvas height. */
interface WaveGeometry {
  midY: number
  baselineY: number
  peakMax: number
  halfPeakMax: number
}

function waveGeometry(height: number): WaveGeometry {
  const peakMax = height - FLAG_STRIP - TICK_STRIP - 2
  return {
    midY: FLAG_STRIP + (height - FLAG_STRIP - TICK_STRIP) / 2,
    baselineY: height - TICK_STRIP,
    peakMax,
    halfPeakMax: peakMax / 2
  }
}

/** Everything a painted strip has to match before it can be blitted again. */
interface PaintedStrip {
  canvas: HTMLCanvasElement
  range: StripRange
  key: string
}

/**
 * Scrolling detailed waveform, mirrored around its center line: the
 * playhead stays fixed in the middle while the waveform moves under it.
 * Cue/loop markers and beat-grid ticks are drawn in the same pass.
 * Drawing happens in a rAF loop that reads the store directly, so the
 * playhead is interpolated at 60 fps between 5 Hz status packets.
 *
 * The envelope itself is not redrawn per frame. Painting it column by column
 * costs one `fillRect` and one colour parse per pixel of width, all of it
 * shipped to the WebKit GPU process; instead it is painted once into an
 * off-screen strip a few canvas widths wide and blitted from there, so a frame
 * costs two `drawImage` calls — one for the dimmed played side, one for the
 * side still to come. The strip is repainted only when the window would leave
 * it, or when zoom, size or palette change it.
 */
export function WaveformDetail({
  deckId,
  track,
  accentColor
}: WaveformDetailProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  /** Visible window in seconds; a ref so zooming never re-renders React. */
  const windowSecRef = useRef(DEFAULT_WINDOW_SEC)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return undefined

    const onWheel = (event: WheelEvent): void => {
      event.preventDefault()
      const factor = event.deltaY > 0 ? ZOOM_STEP : 1 / ZOOM_STEP
      windowSecRef.current = Math.min(
        MAX_WINDOW_SEC,
        Math.max(MIN_WINDOW_SEC, windowSecRef.current * factor)
      )
    }
    const onDoubleClick = (): void => {
      windowSecRef.current = DEFAULT_WINDOW_SEC
    }
    canvas.addEventListener('wheel', onWheel, { passive: false })
    canvas.addEventListener('dblclick', onDoubleClick)

    let strip: PaintedStrip | null = null
    /** What was last drawn; an identical frame is skipped rather than repainted. */
    let lastSignature = ''
    /** `canvasInk` hands back the same object until the theme changes. */
    let lastInk: CanvasInk | null = null

    const draw = (): void => {
      const width = canvas.clientWidth
      const height = canvas.clientHeight
      if (width === 0 || height === 0) return
      const dpr = window.devicePixelRatio || 1
      const windowSec = windowSecRef.current
      const msPerPx = (windowSec * 1000) / width

      const waveform = track?.waveform
      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())
      const ink = canvasInk()
      // Read straight from the store: the rAF loop must not depend on a render.
      const mirrored = useSettings.getState().waveformMirrored
      const smoothing = useSettings.getState().waveformSmoothing
      const saturation = useSettings.getState().waveformSaturation

      // A new palette repaints everything, including a deck sitting still that
      // would otherwise keep its signature — and its old colours — forever.
      if (ink !== lastInk) {
        lastInk = ink
        lastSignature = ''
        strip = null
      }

      const detail = waveform?.detail ?? null
      const hasWave = track !== null && waveform != null && detail !== null && playhead !== null
      // One device pixel is the finest movement the canvas can show: below that
      // the frame would come out identical to the one already on screen. Idle
      // decks settle on a fixed signature and stop drawing altogether.
      const playheadStep = hasWave ? Math.round(playhead / (msPerPx / dpr)) : 0
      const paletteKey = `${mirrored}|${smoothing}|${saturation}`
      const signature = `${width}|${height}|${dpr}|${windowSec}|${paletteKey}|${hasWave}|${playheadStep}`
      if (signature === lastSignature) return
      lastSignature = signature

      const ctx = prepareCanvas(canvas)
      if (!ctx) {
        lastSignature = ''
        return
      }

      const geometry = waveGeometry(height)

      if (!hasWave) {
        ctx.strokeStyle = ink.empty
        ctx.beginPath()
        const emptyY = mirrored ? geometry.midY : geometry.baselineY
        ctx.moveTo(0, emptyY)
        ctx.lineTo(width, emptyY)
        ctx.stroke()
        return
      }

      const centerX = width / 2
      const windowStartMs = playhead - centerX * msPerPx
      const windowEndMs = playhead + (width - centerX) * msPerPx

      // Loop regions first, under everything else
      drawLoopRegions(ctx, track, windowStartMs, windowEndMs, msPerPx, height)

      const stripKey = `${msPerPx}|${width}|${height}|${dpr}|${paletteKey}`
      if (
        !strip ||
        strip.key !== stripKey ||
        !stripCovers(strip.range, windowStartMs, windowEndMs)
      ) {
        strip = paintStrip(strip?.canvas ?? document.createElement('canvas'), stripKey, {
          playheadMs: playhead,
          widthPx: stripWidthPx(width, dpr),
          height,
          dpr,
          msPerPx,
          mirrored,
          smoothing,
          saturation,
          detail,
          msPerDetailSegment: waveform.msPerDetailSegment,
          durationMs: track.durationMs
        })
      }

      if (strip) {
        blitStrip(ctx, strip, {
          windowStartMs,
          msPerPx,
          dpr,
          width,
          height,
          centerX,
          stripWidth: stripWidthPx(width, dpr)
        })
      }

      drawBeatTicks(ctx, track, windowStartMs, windowEndMs, msPerPx, height, ink)
      drawCueMarkers(ctx, track, windowStartMs, windowEndMs, msPerPx, height, ink)

      // Fixed centered playhead
      ctx.save()
      ctx.shadowColor = accentColor
      ctx.shadowBlur = 6
      ctx.fillStyle = ink.playhead
      ctx.fillRect(centerX - 0.75, 0, 1.5, height)
      ctx.restore()

      // Current zoom window, bottom-right corner
      ctx.fillStyle = ink.overlayText
      ctx.font = '500 9px system-ui, sans-serif'
      ctx.textAlign = 'right'
      ctx.textBaseline = 'bottom'
      ctx.fillText(`${windowSec.toFixed(0)}s`, width - 5, height - 3)
    }

    const stopLoop = startFrameLoop(draw, canvas)
    return () => {
      stopLoop()
      canvas.removeEventListener('wheel', onWheel)
      canvas.removeEventListener('dblclick', onDoubleClick)
    }
  }, [deckId, track, accentColor])

  return <canvas ref={canvasRef} className="waveform-detail" />
}

interface StripPaintOptions {
  playheadMs: number
  widthPx: number
  height: number
  dpr: number
  msPerPx: number
  mirrored: boolean
  smoothing: boolean
  /** 1 = raw rekordbox colors, 0 = fully white. */
  saturation: number
  detail: WaveformSegment[]
  msPerDetailSegment: number
  durationMs: number
}

/**
 * Paints the envelope around the playhead into the off-screen strip, opaque:
 * the played/still-to-come dimming is applied when blitting, so one strip
 * serves both sides of the playhead.
 */
function paintStrip(
  canvas: HTMLCanvasElement,
  key: string,
  options: StripPaintOptions
): PaintedStrip | null {
  const { widthPx, height, dpr, msPerPx, mirrored, smoothing, saturation, detail } = options
  const whiteMix = 1 - saturation
  const backingWidth = Math.round(widthPx * dpr)
  const backingHeight = Math.round(height * dpr)
  if (canvas.width !== backingWidth || canvas.height !== backingHeight) {
    canvas.width = backingWidth
    canvas.height = backingHeight
  }
  const ctx = canvas.getContext('2d')
  if (!ctx) return null

  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, widthPx, height)

  const range = stripRangeFor(options.playheadMs, msPerPx, widthPx)
  const { midY, baselineY, peakMax, halfPeakMax } = waveGeometry(height)
  /** Last colour handed to the context, packed; skips redundant fillStyle sets. */
  let lastPacked = -1

  for (let x = 0; x < widthPx; x++) {
    const tMs = range.startMs + x * msPerPx
    if (tMs < 0 || tMs >= options.durationMs) continue
    // Interpolate between neighboring detail columns so the envelope
    // stays smooth when zoomed in past the waveform's native resolution,
    // instead of stair-stepping between identical nearest-neighbor bars.
    const idx = tMs / options.msPerDetailSegment
    const i0 = Math.floor(idx)
    const segA = detail[i0]
    if (!segA) continue
    const segB = detail[i0 + 1] ?? segA
    const frac = idx - i0
    const segHeight = segA.height + (segB.height - segA.height) * frac
    // The rekordbox color is shown as-is — red/green/blue map to bass/mid/
    // treble energy exactly like on a CDJ's own waveform display. Only the
    // interpolation between neighboring columns (for smooth zoom), and the
    // saturation slider's blend toward white, touch it.
    const r = toWhite(segA.color[0] + (segB.color[0] - segA.color[0]) * frac, whiteMix)
    const g = toWhite(segA.color[1] + (segB.color[1] - segA.color[1]) * frac, whiteMix)
    const b = toWhite(segA.color[2] + (segB.color[2] - segA.color[2]) * frac, whiteMix)
    // Setting fillStyle re-parses a CSS colour, so neighbouring columns of the
    // same colour — most of them — are drawn without touching it again.
    const packed = (r << 16) | (g << 8) | b
    if (packed !== lastPacked) {
      lastPacked = packed
      ctx.fillStyle = `rgb(${r},${g},${b})`
    }
    if (mirrored) {
      const half = Math.max(0.5, segHeight * halfPeakMax)
      ctx.fillRect(x, midY - half, 1, half * 2)
    } else {
      const peak = Math.max(1, segHeight * peakMax)
      ctx.fillRect(x, baselineY - peak, 1, peak)
    }
  }

  if (smoothing) blendColumns(canvas, ctx, dpr)

  return { canvas, range, key }
}

/**
 * Blends a color channel toward white. `mix` is how far to go: 0 leaves the
 * rekordbox color untouched, 1 turns it fully white — a crossfade between a
 * CDJ's "Color" and "White" waveform display modes.
 */
function toWhite(channel: number, mix: number): number {
  return Math.round(channel + (255 - channel) * mix)
}

/**
 * Blurs the painted strip by roughly half a CSS pixel so neighbouring columns
 * of differently-colored rekordbox data blend into a continuous gradient
 * instead of reading as a hard-edged patchwork of solid 1px bars.
 */
function blendColumns(canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D, dpr: number): void {
  const sharp = document.createElement('canvas')
  sharp.width = canvas.width
  sharp.height = canvas.height
  const sharpCtx = sharp.getContext('2d')
  if (!sharpCtx) return
  sharpCtx.drawImage(canvas, 0, 0)

  ctx.setTransform(1, 0, 0, 1, 0, 0)
  ctx.clearRect(0, 0, canvas.width, canvas.height)
  ctx.filter = `blur(${(0.5 * dpr).toFixed(2)}px)`
  ctx.drawImage(sharp, 0, 0)
  ctx.filter = 'none'
}

interface BlitOptions {
  windowStartMs: number
  msPerPx: number
  dpr: number
  width: number
  height: number
  centerX: number
  stripWidth: number
}

/**
 * Copies the visible slice of the strip onto the canvas, 1:1 in device pixels,
 * in two passes so the played side keeps its dimmed alpha.
 */
function blitStrip(
  ctx: CanvasRenderingContext2D,
  strip: PaintedStrip,
  { windowStartMs, msPerPx, dpr, width, height, centerX, stripWidth }: BlitOptions
): void {
  const offset = ((windowStartMs - strip.range.startMs) / msPerPx) * dpr
  // Snapped to whole device pixels so the copy stays a straight blit, and
  // clamped so rounding can never run past the end of the strip.
  const sx = Math.min(Math.max(0, Math.round(offset)), Math.round((stripWidth - width) * dpr))
  const sourceHeight = height * dpr
  const smoothing = ctx.imageSmoothingEnabled
  ctx.imageSmoothingEnabled = false

  ctx.globalAlpha = PLAYED_DIM_ALPHA
  ctx.drawImage(strip.canvas, sx, 0, centerX * dpr, sourceHeight, 0, 0, centerX, height)
  ctx.globalAlpha = LIVE_ALPHA
  const rightWidth = width - centerX
  ctx.drawImage(
    strip.canvas,
    sx + centerX * dpr,
    0,
    rightWidth * dpr,
    sourceHeight,
    centerX,
    0,
    rightWidth,
    height
  )

  ctx.globalAlpha = 1
  ctx.imageSmoothingEnabled = smoothing
}

function drawLoopRegions(
  ctx: CanvasRenderingContext2D,
  track: TrackInfo,
  windowStartMs: number,
  windowEndMs: number,
  msPerPx: number,
  height: number
): void {
  for (const cue of track.cues) {
    if (cue.lengthMs === null) continue
    const endMs = cue.offsetMs + cue.lengthMs
    if (endMs < windowStartMs || cue.offsetMs > windowEndMs) continue
    const x0 = (cue.offsetMs - windowStartMs) / msPerPx
    const x1 = (endMs - windowStartMs) / msPerPx
    ctx.fillStyle = cue.color
    ctx.globalAlpha = 0.12
    ctx.fillRect(x0, FLAG_STRIP, x1 - x0, height - FLAG_STRIP - TICK_STRIP)
    ctx.globalAlpha = 1
  }
}

function drawCueMarkers(
  ctx: CanvasRenderingContext2D,
  track: TrackInfo,
  windowStartMs: number,
  windowEndMs: number,
  msPerPx: number,
  height: number,
  ink: CanvasInk
): void {
  for (const cue of track.cues) {
    if (cue.offsetMs < windowStartMs || cue.offsetMs > windowEndMs) continue
    const x = (cue.offsetMs - windowStartMs) / msPerPx

    ctx.fillStyle = cue.color
    ctx.globalAlpha = 0.85
    ctx.fillRect(x - 0.5, FLAG_STRIP - 2, 1, height - FLAG_STRIP - TICK_STRIP + 2)
    ctx.globalAlpha = 1

    if (cue.button) {
      // Hot cue: lettered square flag
      ctx.fillRect(x - 6, 0, 12, 12)
      ctx.fillStyle = ink.flagText
      ctx.font = '700 8px system-ui, sans-serif'
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      ctx.fillText(cue.button, x, 6.5)
    } else {
      // Memory cue / loop: small triangle flag
      ctx.beginPath()
      ctx.moveTo(x - 5, 0)
      ctx.lineTo(x + 5, 0)
      ctx.lineTo(x, 8)
      ctx.closePath()
      ctx.fill()
    }
  }
}

function drawBeatTicks(
  ctx: CanvasRenderingContext2D,
  track: TrackInfo,
  windowStartMs: number,
  windowEndMs: number,
  msPerPx: number,
  height: number,
  ink: CanvasInk
): void {
  const grid = track.beatGrid
  if (!grid || grid.length === 0) return

  // Binary search for the first beat inside the visible window
  let lo = 0
  let hi = grid.length - 1
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (grid[mid].offsetMs < windowStartMs) lo = mid + 1
    else hi = mid
  }

  const tickTop = height - TICK_STRIP + 3
  for (let i = lo; i < grid.length && grid[i].offsetMs <= windowEndMs; i++) {
    const beat = grid[i]
    const x = (beat.offsetMs - windowStartMs) / msPerPx
    const isDownbeat = beat.beatInMeasure === 1
    ctx.fillStyle = isDownbeat ? ink.gridDownbeat : ink.gridBeat
    ctx.fillRect(x, tickTop, 1, isDownbeat ? 7 : 4)
  }
}
