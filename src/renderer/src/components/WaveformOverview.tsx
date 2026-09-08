import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo } from '../model'
import { prepareCanvas } from '../lib/canvas'
import { canvasInk } from '../lib/ink'
import { estimatePlayheadMs } from '../lib/playhead'
import { useSettings } from '../state/settings'
import { useDecktectiveStore } from '../state/store'

interface WaveformOverviewProps {
  deckId: DeckId
  track: TrackInfo | null
  accentColor: string
}

/**
 * Whole-track minimap: monochrome mirrored waveform, cue ticks in their
 * cue colors, played portion dimmed, and a position marker.
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

    let raf = 0
    const draw = (): void => {
      raf = requestAnimationFrame(draw)
      const ctx = prepareCanvas(canvas)
      if (!ctx) return
      const width = canvas.clientWidth
      const height = canvas.clientHeight
      const midY = height / 2
      const ink = canvasInk()
      const mirrored = useSettings.getState().waveformMirrored

      const preview = track?.waveform?.preview
      if (!track || !preview || preview.length === 0) {
        ctx.fillStyle = ink.empty
        ctx.fillRect(0, mirrored ? midY : height - 1, width, 1)
        return
      }

      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())
      const playedX = playhead !== null ? (playhead / track.durationMs) * width : 0

      const barWidth = width / preview.length
      for (let i = 0; i < preview.length; i++) {
        const segment = preview[i]
        const x = i * barWidth
        ctx.fillStyle = x < playedX ? ink.played : ink.unplayed
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

      if (playhead !== null) {
        ctx.save()
        ctx.shadowColor = accentColor
        ctx.shadowBlur = 4
        ctx.fillStyle = ink.playhead
        ctx.fillRect(playedX - 0.75, 0, 1.5, height)
        ctx.restore()
      }
    }

    raf = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(raf)
  }, [deckId, track, accentColor])

  return <canvas ref={canvasRef} className="waveform-overview" />
}
