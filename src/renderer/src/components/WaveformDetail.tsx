import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo } from '../../../engine/types'
import { prepareCanvas } from '../lib/canvas'
import { estimatePlayheadMs } from '../lib/playhead'
import { useDecktectiveStore } from '../state/store'

/** Seconds of audio visible across the full width of the canvas. */
const VISIBLE_WINDOW_SEC = 8
const PLAYED_DIM_ALPHA = 0.32
/** 0 = raw rekordbox colors, 1 = fully washed to white. */
const DESATURATION = 0.45
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

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return undefined

    let raf = 0
    const draw = (): void => {
      raf = requestAnimationFrame(draw)
      const ctx = prepareCanvas(canvas)
      if (!ctx) return
      const width = canvas.clientWidth
      const height = canvas.clientHeight
      const centerX = width / 2
      const msPerPx = (VISIBLE_WINDOW_SEC * 1000) / width
      const midY = FLAG_STRIP + (height - FLAG_STRIP - TICK_STRIP) / 2
      const halfMax = (height - FLAG_STRIP - TICK_STRIP) / 2 - 2

      const waveform = track?.waveform
      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())

      if (!track || !waveform?.detail || playhead === null) {
        ctx.strokeStyle = 'rgba(255,255,255,0.06)'
        ctx.beginPath()
        ctx.moveTo(0, midY)
        ctx.lineTo(width, midY)
        ctx.stroke()
        return
      }

      const windowStartMs = playhead - centerX * msPerPx
      const windowEndMs = playhead + (width - centerX) * msPerPx
      const detail = waveform.detail

      // Loop regions first, under everything else
      drawLoopRegions(ctx, track, windowStartMs, windowEndMs, msPerPx, height)

      for (let x = 0; x < width; x++) {
        const tMs = windowStartMs + x * msPerPx
        if (tMs < 0 || tMs >= track.durationMs) continue
        const segment = detail[Math.floor(tMs / waveform.msPerDetailSegment)]
        if (!segment) continue
        const half = Math.max(1, segment.height * halfMax)
        const [r, g, b] = segment.color
        ctx.globalAlpha = x < centerX ? PLAYED_DIM_ALPHA : 0.95
        ctx.fillStyle = `rgb(${soften(r)},${soften(g)},${soften(b)})`
        ctx.fillRect(x, midY - half, 1, half * 2)
      }
      ctx.globalAlpha = 1

      drawBeatTicks(ctx, track, windowStartMs, windowEndMs, msPerPx, height)
      drawCueMarkers(ctx, track, windowStartMs, windowEndMs, msPerPx, height)

      // Fixed centered playhead
      ctx.save()
      ctx.shadowColor = accentColor
      ctx.shadowBlur = 6
      ctx.fillStyle = '#ffffff'
      ctx.fillRect(centerX - 0.75, 0, 1.5, height)
      ctx.restore()
    }

    raf = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(raf)
  }, [deckId, track, accentColor])

  return <canvas ref={canvasRef} className="waveform-detail" />
}

/** Pulls a color channel toward white for a flatter, desaturated look. */
function soften(channel: number): number {
  return Math.round(channel + (235 - channel) * DESATURATION)
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
  height: number
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
      ctx.fillStyle = '#0a0b0d'
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
  height: number
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
    ctx.fillStyle = isDownbeat ? 'rgba(255,255,255,0.55)' : 'rgba(255,255,255,0.18)'
    ctx.fillRect(x, tickTop, 1, isDownbeat ? 7 : 4)
  }
}
