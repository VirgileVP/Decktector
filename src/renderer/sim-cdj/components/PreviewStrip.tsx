import { useRef } from 'react'
import type { PointerEvent } from 'react'

import { WaveformOverview } from '@renderer/components/WaveformOverview'
import { DECK_ACCENT_COLOR } from '@renderer/lib/deck-colors'
import type { DeckId, TrackInfo } from '@renderer/model'
import type { SimLoop } from '@renderer/model/SimLoop'

import { seekPositionMs, trackFraction } from '../lib/seek-math'
import { sendCommand } from '../net/sim-connection'

/** Shortest gap between two seek commands while dragging across the strip. */
const SEEK_MIN_INTERVAL_MS = 40

interface PreviewStripProps {
  deckId: DeckId
  track: TrackInfo | null
  loop: SimLoop | null
}

/** Whole-track waveform with the playhead; click or drag to seek, loop shown as a region. */
export function PreviewStrip({ deckId, track, loop }: PreviewStripProps): React.JSX.Element {
  const draggingRef = useRef(false)
  const lastSeekAtRef = useRef(0)

  const seekTo = (event: PointerEvent<HTMLDivElement>, force: boolean): void => {
    if (!track) return
    const now = event.timeStamp
    if (!force && now - lastSeekAtRef.current < SEEK_MIN_INTERVAL_MS) return
    lastSeekAtRef.current = now
    const positionMs = seekPositionMs(
      event.clientX,
      event.currentTarget.getBoundingClientRect(),
      track.durationMs
    )
    sendCommand({ type: 'seek', deck: deckId, positionMs })
  }

  const onPointerDown = (event: PointerEvent<HTMLDivElement>): void => {
    if (!track) return
    event.currentTarget.setPointerCapture(event.pointerId)
    draggingRef.current = true
    seekTo(event, true)
  }

  const onPointerMove = (event: PointerEvent<HTMLDivElement>): void => {
    if (draggingRef.current) seekTo(event, false)
  }

  const onPointerUp = (event: PointerEvent<HTMLDivElement>): void => {
    if (!draggingRef.current) return
    draggingRef.current = false
    seekTo(event, true)
  }

  const loopStyle =
    track && loop
      ? {
          left: `${trackFraction(loop.startMs, track.durationMs) * 100}%`,
          width: `${(trackFraction(loop.endMs, track.durationMs) - trackFraction(loop.startMs, track.durationMs)) * 100}%`
        }
      : null

  return (
    <div
      className={`preview-strip${track ? '' : ' preview-strip-empty'}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        draggingRef.current = false
      }}
    >
      <WaveformOverview deckId={deckId} track={track} accentColor={DECK_ACCENT_COLOR} />
      {loopStyle && (
        <span
          className={`preview-loop${loop?.active ? ' preview-loop-active' : ''}`}
          style={loopStyle}
        />
      )}
    </div>
  )
}
