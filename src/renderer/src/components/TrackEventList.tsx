import { useEffect, useMemo, useRef, useState } from 'react'

import type { DeckId, TrackEvent, TrackInfo } from '../model'
import { startFrameLoop } from '../lib/frame-loop'
import { estimatePlayheadMs } from '../lib/playhead'
import {
  EVENT_CONFIDENCE_HEDGE,
  eventName,
  eventProgressAt,
  formatEventSeconds
} from '../lib/track-events'
import { useDecktectorStore } from '../state/store'

/** Gauge positions the loop distinguishes; finer than a pixel on any rail width. */
const GAUGE_STEPS = 1000

interface TrackEventListProps {
  deckId: DeckId
  track: TrackInfo | null
  width: number
}

/**
 * The analysed structure of the track, as a countdown: the section running now
 * with the seconds left on it and a gauge filling as it plays, then whatever
 * comes after it with its length.
 *
 * The gauge and the countdown are written straight to the DOM from a
 * `requestAnimationFrame` loop. Re-rendering the whole list sixty times a
 * second to move one bar would cost far more than it buys, so React only
 * re-renders when the playhead crosses into the next section — and the DOM is
 * only written when the gauge or the countdown actually moves. The loop is
 * idle while the rail is off-screen or the window is hidden.
 */
export function TrackEventList({ deckId, track, width }: TrackEventListProps): React.JSX.Element {
  // Memoized so an empty list keeps its identity: the animation loop below
  // restarts whenever this changes.
  const events = useMemo(() => track?.events ?? [], [track])
  const [activeIndex, setActiveIndex] = useState(0)
  const rootRef = useRef<HTMLElement>(null)
  const gaugeRef = useRef<HTMLDivElement>(null)
  const countdownRef = useRef<HTMLSpanElement>(null)

  useEffect(() => {
    if (events.length === 0) return undefined

    let renderedIndex = -1
    let renderedRatio = -1
    let renderedCountdown = ''

    const tick = (): void => {
      const deck = useDecktectorStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())
      const progress = playhead !== null ? eventProgressAt(events, playhead) : null
      const index = progress?.index ?? 0

      if (renderedIndex !== index) {
        renderedIndex = index
        setActiveIndex(index)
      }

      const event = events[index]
      // A thousandth of the gauge is well under a pixel wide: rounding to it
      // drops most of the writes without the bar ever visibly stepping.
      const ratio = Math.round((progress?.ratio ?? 0) * GAUGE_STEPS) / GAUGE_STEPS
      if (gaugeRef.current && ratio !== renderedRatio) {
        renderedRatio = ratio
        gaugeRef.current.style.transform = `scaleX(${ratio})`
      }

      // A deck sitting on a cue point has no progress yet: show the section's
      // full length, which is what it will count down from.
      const remaining = progress?.remainingMs ?? event.endMs - event.startMs
      const countdown = formatEventSeconds(remaining)
      if (countdownRef.current && countdown !== renderedCountdown) {
        renderedCountdown = countdown
        countdownRef.current.textContent = countdown
      }
    }

    return startFrameLoop(tick, rootRef.current)
  }, [deckId, events])

  if (events.length === 0) {
    return (
      <aside className="event-list event-list-empty" style={{ width }}>
        NO ANALYSIS
      </aside>
    )
  }

  const active = events[Math.min(activeIndex, events.length - 1)]

  return (
    <aside className="event-list" style={{ width }} ref={rootRef}>
      <div className={`event-active event-kind-${active.kind}`}>
        <div className="event-gauge" ref={gaugeRef} />
        <EventName event={active} />
        <span className="event-countdown" ref={countdownRef} />
      </div>

      {events.slice(activeIndex + 1).map((event) => (
        <div key={event.startMs} className={`event-row event-kind-${event.kind}`}>
          <span className="event-chip" />
          <EventName event={event} />
          <span className="event-length">{formatEventSeconds(event.endMs - event.startMs)}</span>
        </div>
      ))}
    </aside>
  )
}

/** Section name, marked as a guess while no cue confirms the label. */
function EventName({ event }: { event: TrackEvent }): React.JSX.Element {
  const isGuess = event.confidence < EVENT_CONFIDENCE_HEDGE
  return (
    <span className="event-name">
      {eventName(event)}
      {isGuess && (
        <span className="event-guess" title="Inferred from the waveform, no cue confirms it">
          ?
        </span>
      )}
    </span>
  )
}
