import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo, WaveformSegment } from '../model'
import { prepareCanvas } from '../lib/canvas'
import { canvasInk, type CanvasInk } from '../lib/ink'
import { estimatePlayheadMs } from '../lib/playhead'
import { useSettings, type WaveformStyle } from '../state/settings'
import { useDecktectiveStore } from '../state/store'

/** The classic CDJ mono waveform's base hue, before whiteness brightens it. */
const BLUE_STYLE_BASE: readonly [number, number, number] = [60, 130, 255]

/** Default seconds of audio visible across the full width of the canvas. */
const DEFAULT_WINDOW_SEC = 8
const MIN_WINDOW_SEC = 2
const MAX_WINDOW_SEC = 60
/** Multiplier applied per wheel notch when zooming. */
const ZOOM_STEP = 1.18
const PLAYED_DIM_ALPHA = 0.32
/** Height reserved at the bottom for the beat-grid tick strip. */
const TICK_STRIP = 12
/** Height reserved at the top for cue flags. */
const FLAG_STRIP = 14

interface WaveformDetailProps {
  deckId: DeckId
  track: TrackInfo | null
  accentColor: string
}

/**
 * Scrolling detailed waveform, mirrored around its center line: the
 * playhead stays fixed in the middle while the waveform moves under it.
 * Cue/loop markers and beat-grid ticks are drawn in the same pass.
 * Drawing happens in a rAF loop that reads the store directly, so the
 * playhead is interpolated at 60 fps between 5 Hz status packets.
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

    let raf = 0
    const draw = (): void => {
      raf = requestAnimationFrame(draw)
      const ctx = prepareCanvas(canvas)
      if (!ctx) return
      const width = canvas.clientWidth
      const height = canvas.clientHeight
      const centerX = width / 2
      const msPerPx = (windowSecRef.current * 1000) / width
      const midY = FLAG_STRIP + (height - FLAG_STRIP - TICK_STRIP) / 2
      const halfMax = (height - FLAG_STRIP - TICK_STRIP) / 2 - 2

      const waveform = track?.waveform
      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())
      const ink = canvasInk()
      // Read straight from the store: the rAF loop must not depend on a render.
      const settings = useSettings.getState()
      const wash = settings.waveformDesaturation
      const gain = settings.waveformAmplitude
      const style = settings.waveformStyle

      if (!track || !waveform?.detail || playhead === null) {
        ctx.strokeStyle = ink.empty
        ctx.beginPath()
        ctx.moveTo(0, midY)
        ctx.lineTo(width, midY)
        ctx.stroke()
        return
      }

      const windowStartMs = playhead - centerX * msPerPx
      const windowEndMs = playhead + (width - centerX) * msPerPx
      const detail = waveform.detail
      // 'blue' reads the classic mono detail when the track has one;
      // otherwise it falls back to a grayscale read of the color detail
      // rather than going blank.
      const getColumn =
        style === 'blue'
          ? waveform.monoDetail
            ? segmentAt(waveform.monoDetail)
            : grayscaleSegmentAt(detail)
          : segmentAt(detail)

      // Loop regions first, under everything else
      drawLoopRegions(ctx, track, windowStartMs, windowEndMs, msPerPx, height)

      for (let x = 0; x < width; x++) {
        const tMs = windowStartMs + x * msPerPx
        if (tMs < 0 || tMs >= track.durationMs) continue
        // Interpolate between neighboring detail columns so the envelope
        // stays smooth when zoomed in past the waveform's native resolution,
        // instead of stair-stepping between identical nearest-neighbor bars.
        const column = interpolateColumn(getColumn, waveform.msPerDetailSegment, tMs)
        if (!column) continue
        const half = Math.max(1, Math.min(halfMax, column.height * gain * halfMax))
        ctx.globalAlpha = x < centerX ? PLAYED_DIM_ALPHA : 0.95
        if (style === '3band') {
          draw3BandColumn(ctx, x, midY, half, column.color, ink, wash)
        } else if (style === 'blue') {
          drawBlueColumn(ctx, x, midY, half, column.color[0], ink, wash)
        } else {
          drawRgbColumn(ctx, x, midY, half, column.color, ink, wash)
        }
      }
      ctx.globalAlpha = 1

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
      ctx.fillText(`${windowSecRef.current.toFixed(0)}s`, width - 5, height - 3)
    }

    raf = requestAnimationFrame(draw)
    return () => {
      cancelAnimationFrame(raf)
      canvas.removeEventListener('wheel', onWheel)
      canvas.removeEventListener('dblclick', onDoubleClick)
    }
  }, [deckId, track, accentColor])

  return <canvas ref={canvasRef} className="waveform-detail" />
}

/**
 * Pulls a color channel toward the theme's target for a flatter look.
 * `wash` is how far to go: 0 leaves the rekordbox color untouched, 1 replaces it.
 */
function soften(channel: number, ink: CanvasInk, wash: number): number {
  return Math.round(channel + (ink.softenTarget - channel) * wash)
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
