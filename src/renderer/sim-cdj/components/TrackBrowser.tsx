import { formatBpm } from '@renderer/lib/deck-readouts'
import type { SimTrackSummary } from '@renderer/model/SimTrackSummary'

import { formatDuration } from '../lib/format'
import { sendCommand } from '../net/sim-connection'
import { closeTrackBrowser, useSimStore } from '../state/sim-store'

/** Modal list of the library (already sorted by title); a click loads the track on the deck. */
export function TrackBrowser(): React.JSX.Element | null {
  const deckId = useSimStore((state) => state.browserDeck)
  const library = useSimStore((state) => state.rig?.library)
  const loadedIndex = useSimStore(
    (state) => state.rig?.decks.find((deck) => deck.deckId === deckId)?.trackIndex ?? null
  )
  if (deckId === null) return null

  const load = (track: SimTrackSummary): void => {
    sendCommand({ type: 'load', deck: deckId, trackIndex: track.index })
    closeTrackBrowser()
  }

  return (
    <div className="browser-backdrop" onClick={closeTrackBrowser}>
      <div
        className="browser"
        role="dialog"
        aria-label={`Load a track on CDJ ${deckId}`}
        onClick={(event) => event.stopPropagation()}
      >
        <header className="browser-header">
          <h2>Load on CDJ {deckId}</h2>
          <button type="button" className="browser-close" onClick={closeTrackBrowser}>
            CLOSE
          </button>
        </header>
        <div className="browser-columns browser-head-row">
          <span>TITLE</span>
          <span>ARTIST</span>
          <span>BPM</span>
          <span>KEY</span>
          <span>TIME</span>
        </div>
        <div className="browser-list">
          {library?.length ? (
            library.map((track) => (
              <button
                key={track.index}
                type="button"
                className={`browser-columns browser-row${track.index === loadedIndex ? ' browser-row-loaded' : ''}`}
                onClick={() => load(track)}
              >
                <span className="browser-title">{track.title}</span>
                <span>{track.artist ?? ''}</span>
                <span>{formatBpm(track.bpm)}</span>
                <span>{track.key ?? ''}</span>
                <span>{formatDuration(track.durationMs)}</span>
              </button>
            ))
          ) : (
            <p className="browser-empty">The library is empty.</p>
          )}
        </div>
      </div>
    </div>
  )
}
