import { useRef } from 'react'

import type { DeckId } from '@renderer/model'

import { usePlayheadFrame } from '../hooks/usePlayheadFrame'
import { useJogGestures } from '../hooks/useJogGestures'
import { platterAngleDegrees } from '../lib/jog-math'

interface JogWheelProps {
  deckId: DeckId
  enabled: boolean
  /** The top of the platter is being touched (scratching). */
  scratching: boolean
}

/**
 * The jog wheel: a touch-sensitive platter ringed by an edge that bends the
 * tempo. A marker on the platter turns with the track position.
 */
export function JogWheel({ deckId, enabled, scratching }: JogWheelProps): React.JSX.Element {
  const spinnerRef = useRef<HTMLDivElement>(null)
  const handlers = useJogGestures(deckId, enabled)

  usePlayheadFrame(deckId, (playheadMs) => {
    const spinner = spinnerRef.current
    if (!spinner) return
    spinner.style.transform = `rotate(${platterAngleDegrees(playheadMs ?? 0)}deg)`
  })

  return (
    <div
      className={`jog${enabled ? '' : ' jog-disabled'}${scratching ? ' jog-scratching' : ''}`}
      aria-label={`Deck ${deckId} jog wheel`}
      {...handlers}
    >
      <div className="jog-platter">
        <div ref={spinnerRef} className="jog-spinner">
          <span className="jog-marker" />
        </div>
      </div>
    </div>
  )
}
