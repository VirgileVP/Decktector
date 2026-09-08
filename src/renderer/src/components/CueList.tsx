import { useEffect, useMemo, useRef, useState } from 'react'

import type { DeckId, TrackInfo } from '../model'
import { cueLabel, cueProgressAt, sortCues } from '../lib/cue-progress'
import { startFrameLoop } from '../lib/frame-loop'
import { estimatePlayheadMs, formatTimeMs } from '../lib/playhead'
import { formatEventSeconds } from '../lib/track-events'
import { useDecktectiveStore } from '../state/store'

/** Gauge positions the loop distinguishes; finer than a pixel on any rail width. */
const GAUGE_STEPS = 1000

interface CueListProps {
  deckId: DeckId
  track: TrackInfo | null
  width: number
}

/**
 * Left-hand rail listing the track's cues as a real-time countdown to the upcoming cue
 * (matching the track structure countdown display) so incoming cues are always visible
 * without passed cues taking up vertical space.
 *
 * The gauge and the countdown are written directly to the DOM in a `requestAnimationFrame`
 * loop. React only re-renders when the playhead crosses into the next cue, and the DOM is
 * only written when the gauge or the countdown actually moves — a style write nobody can
 * see still costs a recalc. The loop is idle while the rail is off-screen or the window
 * is hidden.
 */
export function CueList({ deckId, track, width }: CueListProps): React.JSX.Element {
  const cues = useMemo(() => sortCues(track?.cues ?? []), [track])
  const [activeIndex, setActiveIndex] = useState(0)
  const [allPassed, setAllPassed] = useState(false)
  const rootRef = useRef<HTMLElement>(null)
  const gaugeRef = useRef<HTMLDivElement>(null)
  const countdownRef = useRef<HTMLSpanElement>(null)

  useEffect(() => {
    if (cues.length === 0) return undefined

    let renderedIndex = -1
    let renderedAllPassed = false
    let renderedRatio = -1
    let renderedCountdown = ''

    const tick = (): void => {
      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())
      const progress = playhead !== null ? cueProgressAt(cues, playhead) : null
      const index = progress?.index ?? 0
      const isAllPassed = progress?.allPassed ?? false

      if (renderedIndex !== index) {
        renderedIndex = index
        setActiveIndex(index)
      }
      if (renderedAllPassed !== isAllPassed) {
        renderedAllPassed = isAllPassed
        setAllPassed(isAllPassed)
      }

      // A thousandth of the gauge is well under a pixel wide: rounding to it
      // drops most of the writes without the bar ever visibly stepping.
      const ratio = Math.round((progress?.ratio ?? 0) * GAUGE_STEPS) / GAUGE_STEPS
      if (gaugeRef.current && ratio !== renderedRatio) {
        renderedRatio = ratio
        gaugeRef.current.style.transform = `scaleX(${ratio})`
      }

      const remaining = progress?.remainingMs ?? cues[index]?.offsetMs ?? 0
      const countdown = isAllPassed ? '--' : formatEventSeconds(remaining)
      if (countdownRef.current && countdown !== renderedCountdown) {
        renderedCountdown = countdown
        countdownRef.current.textContent = countdown
      }
    }

    return startFrameLoop(tick, rootRef.current)
  }, [deckId, cues])

  if (cues.length === 0) {
    return (
      <aside className="cue-list cue-list-empty" style={{ width }}>
        NO CUES
      </aside>
    )
  }

  const active = cues[Math.min(activeIndex, cues.length - 1)]

  return (
    <aside className="cue-list" style={{ width }} ref={rootRef}>
      <div
        className={`cue-active${allPassed ? ' cue-passed' : ''}`}
        style={{ ['--cue-color' as string]: active.color }}
      >
        <div className="cue-gauge" ref={gaugeRef} />
        <span className="cue-marker" style={{ background: active.color }}>
          {active.button ?? ''}
        </span>
        <span className="cue-name">{cueLabel(active)}</span>
        <span className="cue-countdown" ref={countdownRef} />
      </div>

      {!allPassed &&
        cues.slice(activeIndex + 1).map((cue, idx) => (
          <div
            key={`${cue.kind}-${cue.offsetMs}-${cue.button ?? idx}`}
            className="cue-row"
            style={{ ['--cue-color' as string]: cue.color }}
          >
            <span className="cue-marker" style={{ background: cue.color }}>
              {cue.button ?? ''}
            </span>
            <span className="cue-name">{cueLabel(cue)}</span>
            <span className="cue-time">{formatTimeMs(cue.offsetMs)}</span>
          </div>
        ))}
    </aside>
  )
}
