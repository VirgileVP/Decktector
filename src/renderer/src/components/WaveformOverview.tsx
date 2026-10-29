import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo } from '../model'
import { prepareCanvas } from '../lib/canvas'
import { startFrameLoop } from '../lib/frame-loop'
import { canvasInk, type CanvasInk } from '../lib/ink'
import { estimatePlayheadMs } from '../lib/playhead'
import { activeLoopKey, fillLoopSpan, strokeLoopEdges } from '../lib/waveform-loop'
import { useSettings } from '../state/settings'
import { useDecktectorStore } from '../state/store'

interface WaveformOverviewProps {
  deckId: DeckId
  track: TrackInfo | null
  accentColor: string
}

/**
 * The minimap in both of its colours: nothing about it moves, so each is
 * painted once and the playhead decides where one gives way to the other.
 */
interface OverviewLayers {
  played: HTMLCanvasElement
  unplayed: HTMLCanvasElement
  key: string
}

/**
 * Whole-track minimap: monochrome mirrored waveform, cue ticks in their
 * cue colors, played portion dimmed, and a position marker.
 *
 * Every bar and cue of it is fixed for the life of the track — only where the
 * played colour gives way to the unplayed one moves. So the two colourings are
 * painted off-screen once and each frame is two `drawImage` calls plus the
 * position marker, instead of four hundred bars redrawn at 60 fps. At this
 * scale the boundary moves about one pixel per second, so the frames in
 * between are skipped outright.
 */
export function WaveformOverview({
  deckId,
  track,
  accentColor
}: WaveformOverviewProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return undefined

    let layers: OverviewLayers | null = null
    /** What was last drawn; an identical frame is skipped rather than repainted. */
    let lastSignature = ''
    /** `canvasInk` hands back the same object until the theme changes. */
    let lastInk: CanvasInk | null = null

    const draw = (): void => {
      const width = canvas.clientWidth
      const height = canvas.clientHeight
      if (width === 0 || height === 0) return
      const dpr = window.devicePixelRatio || 1
      const ink = canvasInk()
      const mirrored = useSettings.getState().waveformMirrored

      // A new palette repaints everything, including a deck sitting still that
      // would otherwise keep its signature — and its old colours — forever.
      if (ink !== lastInk) {
        lastInk = ink
        lastSignature = ''
        layers = null
      }

      const preview = track?.waveform?.preview ?? null
      const hasWave = track !== null && preview !== null && preview.length > 0
      const deck = useDecktectorStore.getState().decks[deckId]
      const playhead = hasWave ? estimatePlayheadMs(deck, Date.now()) : null
      const loopRange = deck?.activeLoop?.range ?? null
      // A playhead is only ever set when `hasWave` held, i.e. with a track in
      // hand; the fallback duration just keeps the arithmetic total.
      const durationMs = track?.durationMs ?? 1
      const playedX =
        playhead !== null ? Math.min(width, Math.max(0, (playhead / durationMs) * width)) : 0
      // The boundary only ever lands on a whole device pixel, so a frame that
      // would not move it is not worth drawing at all.
      const playedDevice = Math.round(playedX * dpr)

      const layerKey = `${width}|${height}|${dpr}|${mirrored}|${ink.played}|${ink.unplayed}`
      const signature = `${layerKey}|${hasWave}|${playhead !== null}|${playedDevice}|${activeLoopKey(deck?.activeLoop)}`
      if (signature === lastSignature) return
      lastSignature = signature

      const ctx = prepareCanvas(canvas)
      if (!ctx) {
        lastSignature = ''
        return
      }

      if (!hasWave) {
        ctx.fillStyle = ink.empty
        ctx.fillRect(0, mirrored ? height / 2 : height - 1, width, 1)
        return
      }

      if (!layers || layers.key !== layerKey) {
        layers = paintLayers(layers, layerKey, {
          track,
          preview,
          width,
          height,
          dpr,
          mirrored,
          playedColor: ink.played,
          unplayedColor: ink.unplayed
        })
      }

      if (layers) {
        const boundary = playedDevice / dpr
        const sourceHeight = height * dpr
        const smoothing = ctx.imageSmoothingEnabled
        ctx.imageSmoothingEnabled = false
        if (playedDevice > 0) {
          ctx.drawImage(layers.played, 0, 0, playedDevice, sourceHeight, 0, 0, boundary, height)
        }
        if (boundary < width) {
          ctx.drawImage(
            layers.unplayed,
            playedDevice,
            0,
            (width - boundary) * dpr,
            sourceHeight,
            boundary,
            0,
            width - boundary,
            height
          )
        }
        ctx.imageSmoothingEnabled = smoothing
      }

      if (loopRange) {
        const x0 = (loopRange.startMs / durationMs) * width
        const x1 = (loopRange.endMs / durationMs) * width
        const style = { top: 0, bottom: height, color: ink.activeLoop }
        fillLoopSpan(ctx, x0, x1, style)
        strokeLoopEdges(ctx, x0, x1, style)
      }

      if (playhead !== null) {
        ctx.save()
        ctx.shadowColor = accentColor
        ctx.shadowBlur = 4
        ctx.fillStyle = ink.playhead
        ctx.fillRect(playedX - 0.75, 0, 1.5, height)
        ctx.restore()
      }
    }

    const stopLoop = startFrameLoop(draw, canvas)
    return stopLoop
  }, [deckId, track, accentColor])

  return <canvas ref={canvasRef} className="waveform-overview" />
}

interface LayerPaintOptions {
  track: TrackInfo
  preview: NonNullable<NonNullable<TrackInfo['waveform']>['preview']>
  width: number
  height: number
  dpr: number
  mirrored: boolean
  playedColor: string
  unplayedColor: string
}

/** Paints both colourings of the minimap, reusing the canvases already allocated. */
function paintLayers(
  previous: OverviewLayers | null,
  key: string,
  options: LayerPaintOptions
): OverviewLayers | null {
  const played = paintLayer(
    previous?.played ?? document.createElement('canvas'),
    options.playedColor,
    options
  )
  const unplayed = paintLayer(
    previous?.unplayed ?? document.createElement('canvas'),
    options.unplayedColor,
    options
  )
  if (!played || !unplayed) return null
  return { played, unplayed, key }
}

/** One colouring: the bars in `barColor`, then the cue and loop markers over them. */
function paintLayer(
  canvas: HTMLCanvasElement,
  barColor: string,
  { track, preview, width, height, dpr, mirrored }: LayerPaintOptions
): HTMLCanvasElement | null {
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

  const midY = height / 2
  const barWidth = width / preview.length
  ctx.fillStyle = barColor
  for (let i = 0; i < preview.length; i++) {
    const segment = preview[i]
    const x = i * barWidth
    if (mirrored) {
      const half = Math.max(0.5, segment.height * (midY - 2))
      ctx.fillRect(x, midY - half, Math.ceil(barWidth), half * 2)
    } else {
      const peak = Math.max(1, segment.height * (height - 2))
      ctx.fillRect(x, height - peak, Math.ceil(barWidth), peak)
    }
  }

  for (const cue of track.cues) {
    const x = (cue.offsetMs / track.durationMs) * width
    ctx.fillStyle = cue.color
    if (cue.lengthMs !== null) {
      const w = (cue.lengthMs / track.durationMs) * width
      ctx.globalAlpha = 0.25
      ctx.fillRect(x, 0, Math.max(w, 2), height)
      ctx.globalAlpha = 1
    }
    ctx.fillRect(x - 0.75, 0, 1.5, height)
  }

  return canvas
}
