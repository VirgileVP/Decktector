import { useEffect, useRef } from 'react'

import type { DeckId, TrackInfo } from '../model'
import { BEATS_PER_MEASURE, beatInMeasureAt } from '../lib/beat-grid'
import { estimatePlayheadMs } from '../lib/playhead'
import { useDecktectiveStore } from '../state/store'

const BEATS = Array.from({ length: BEATS_PER_MEASURE }, (_, index) => index + 1)

interface BeatIndicatorProps {
  deckId: DeckId
  track: TrackInfo | null
}

/**
 * The bar the deck is playing, as four bars running the full height of the
 * deck down its right edge, with the beat playing right now lit.
 *
 * The beat is read off the track's beat grid against the interpolated
 * playhead, so it lands on the beat instead of up to 200 ms behind it like the
 * `beatInMeasure` the 5 Hz status packets carry — that field is only the
 * fallback, for a track that came with no grid. Lighting is written straight
 * to the DOM from a `requestAnimationFrame` loop: re-rendering the deck on
 * every beat would cost more than it buys.
 */
export function BeatIndicator({ deckId, track }: BeatIndicatorProps): React.JSX.Element {
  const barsRef = useRef<(HTMLSpanElement | null)[]>([])

  useEffect(() => {
    let raf = 0
    /** 0 means no beat lit; skips the DOM writes while the beat holds. */
    let litBeat = -1

    const tick = (): void => {
      raf = requestAnimationFrame(tick)
      const deck = useDecktectiveStore.getState().decks[deckId]
      const playhead = estimatePlayheadMs(deck, Date.now())
      const gridBeat = playhead !== null ? beatInMeasureAt(track?.beatGrid, playhead) : null
      const beat = gridBeat ?? deck?.beatInMeasure ?? 0

      if (beat === litBeat) return
      litBeat = beat
      barsRef.current.forEach((bar, index) => {
        bar?.classList.toggle('lit', index + 1 === beat)
      })
    }

    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [deckId, track])

  return (
    <div className="deck-beats" aria-hidden="true">
      {BEATS.map((beat, index) => (
        <span
          key={beat}
          className="deck-beat"
          ref={(element) => {
            barsRef.current[index] = element
          }}
        />
      ))}
    </div>
  )
}
