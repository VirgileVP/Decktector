import { useRef } from 'react'

import type { DeckId } from '@renderer/model'
import { isNearTrackEnd } from '@renderer/lib/deck-readouts'

import { usePlayheadFrame } from '../hooks/usePlayheadFrame'
import { formatTimeTenths } from '../lib/format'

const EMPTY_TIME = '--:--.-'

interface ScreenTimesProps {
  deckId: DeckId
  durationMs: number | null
}

/** Elapsed and remaining time, written straight to the DOM every frame. */
export function ScreenTimes({ deckId, durationMs }: ScreenTimesProps): React.JSX.Element {
  const elapsedRef = useRef<HTMLSpanElement>(null)
  const remainingRef = useRef<HTMLSpanElement>(null)

  usePlayheadFrame(deckId, (playheadMs) => {
    const elapsed = elapsedRef.current
    const remaining = remainingRef.current
    if (!elapsed || !remaining) return
    if (playheadMs === null || durationMs === null) {
      elapsed.textContent = EMPTY_TIME
      remaining.textContent = EMPTY_TIME
      return
    }
    const remainingMs = Math.max(0, durationMs - playheadMs)
    elapsed.textContent = formatTimeTenths(playheadMs)
    remaining.textContent = `-${formatTimeTenths(remainingMs)}`
    remaining.classList.toggle('screen-time-warning', isNearTrackEnd(remainingMs))
  })

  return (
    <div className="screen-times">
      <div>
        <span className="screen-caption">ELAPSED</span>
        <span ref={elapsedRef} className="screen-time">
          {EMPTY_TIME}
        </span>
      </div>
      <div>
        <span className="screen-caption">REMAIN</span>
        <span ref={remainingRef} className="screen-time">
          {EMPTY_TIME}
        </span>
      </div>
    </div>
  )
}
