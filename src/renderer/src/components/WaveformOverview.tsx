import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo } from '../../../engine/types'
import { prepareCanvas } from '../lib/canvas'
import { estimatePlayheadMs } from '../lib/playhead'
import { useDecktectiveStore } from '../state/store'

interface WaveformOverviewProps {
  deckId: DeckId
  track: TrackInfo | null
  accentColor: string
}

/** Whole-track minimap with the played portion dimmed and a position marker. */
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

      const preview = track?.waveform?.preview
      if (!track || !preview || preview.length === 0) {
        ctx.fillStyle = 'rgba(255,255,255,0.05)'
        ctx.fillRect(0, height / 2, width, 1)
        return
      }

      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())
      const playedX = playhead !== null ? (playhead / track.durationMs) * width : 0

      const barWidth = width / preview.length
      for (let i = 0; i < preview.length; i++) {
        const segment = preview[i]
        const x = i * barWidth
        const barHeight = Math.max(1, segment.height * (height - 4))
        const [r, g, b] = segment.color
        ctx.globalAlpha = x < playedX ? 0.35 : 0.9
        ctx.fillStyle = `rgb(${r},${g},${b})`
        ctx.fillRect(x, height - barHeight, Math.ceil(barWidth), barHeight)
      }
      ctx.globalAlpha = 1

      if (playhead !== null) {
        ctx.fillStyle = accentColor
        ctx.fillRect(playedX - 1, 0, 2, height)
      }
    }

    raf = requestAnimationFrame(draw)
    return () => cancelAnimationFrame(raf)
  }, [deckId, track, accentColor])

  return <canvas ref={canvasRef} className="waveform-overview" />
}
