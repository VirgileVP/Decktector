import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo } from '../../../engine/types'
import { prepareCanvas } from '../lib/canvas'
import { estimatePlayheadMs } from '../lib/playhead'
import { useDecktectiveStore } from '../state/store'

/** Seconds of audio visible across the full width of the canvas. */
const VISIBLE_WINDOW_SEC = 8
const PLAYED_DIM_ALPHA = 0.45

interface WaveformDetailProps {
  deckId: DeckId
  track: TrackInfo | null
  accentColor: string
}

/**
 * Scrolling detailed waveform: the playhead stays centered while the
 * waveform moves under it, with beat-grid ticks along the top edge.
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

      const waveform = track?.waveform
      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())

      if (!track || !waveform?.detail || playhead === null) {
        ctx.strokeStyle = 'rgba(255,255,255,0.08)'
        ctx.beginPath()
        ctx.moveTo(0, height / 2)
        ctx.lineTo(width, height / 2)
        ctx.stroke()
        return
      }

      const detail = waveform.detail
      const baseline = height - 14

      for (let x = 0; x < width; x++) {
        const tMs = playhead + (x - centerX) * msPerPx
        if (tMs < 0 || tMs >= track.durationMs) continue
        const segment = detail[Math.floor(tMs / waveform.msPerDetailSegment)]
        if (!segment) continue
        const barHeight = Math.max(1, segment.height * (baseline - 6))
        const [r, g, b] = segment.color
        ctx.globalAlpha = x < centerX ? PLAYED_DIM_ALPHA : 1
        ctx.fillStyle = `rgb(${r},${g},${b})`
        ctx.fillRect(x, baseline - barHeight, 1, barHeight)
      }
      ctx.globalAlpha = 1

      drawBeatTicks(ctx, track, playhead, centerX, msPerPx, width, baseline)

      // Centered playhead
      ctx.fillStyle = accentColor
      ctx.fillRect(centerX - 1, 0, 2, height)
    }

    raf = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(raf)
  }, [deckId, track, accentColor])

  return <canvas ref={canvasRef} className="waveform-detail" />
}

function drawBeatTicks(
  ctx: CanvasRenderingContext2D,
  track: TrackInfo,
  playhead: number,
  centerX: number,
  msPerPx: number,
  width: number,
  baseline: number
): void {
  const grid = track.beatGrid
  if (!grid || grid.length === 0) return

  const windowStartMs = playhead - centerX * msPerPx
  const windowEndMs = playhead + (width - centerX) * msPerPx

  // Binary search for the first beat inside the visible window
  let lo = 0
  let hi = grid.length - 1
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (grid[mid].offsetMs < windowStartMs) lo = mid + 1
    else hi = mid
  }

  for (let i = lo; i < grid.length && grid[i].offsetMs <= windowEndMs; i++) {
    const beat = grid[i]
    const x = centerX + (beat.offsetMs - playhead) / msPerPx
    const isDownbeat = beat.beatInMeasure === 1
    ctx.fillStyle = isDownbeat ? 'rgba(255,80,80,0.9)' : 'rgba(255,255,255,0.35)'
    ctx.fillRect(x, baseline + 4, isDownbeat ? 2 : 1, isDownbeat ? 8 : 5)
  }
}
