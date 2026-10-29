import type { DeckId } from '@renderer/model'

import { useDeckTrack, useDeckView } from '../state/sim-store'
import { CdjScreen } from './CdjScreen'
import { HotCuePads } from './HotCuePads'
import { JogWheel } from './JogWheel'
import { LoopControls } from './LoopControls'
import { TempoControls } from './TempoControls'
import { TransportButtons } from './TransportButtons'

/** One simulated CDJ: screen on top, jog and tempo in the middle, buttons below. */
export function CdjDeck({ deckId }: { deckId: DeckId }): React.JSX.Element {
  const view = useDeckView(deckId)
  const track = useDeckTrack(deckId)

  if (!view) {
    return (
      <section className="cdj cdj-waiting" aria-label={`CDJ ${deckId}`}>
        <div className="cdj-label">CDJ {deckId}</div>
      </section>
    )
  }

  const loaded = view.trackIndex !== null
  return (
    <section className="cdj" aria-label={`CDJ ${deckId}`}>
      <div className="cdj-label">CDJ {deckId}</div>
      <CdjScreen view={view} track={track} />
      <HotCuePads deckId={deckId} track={track} enabled={loaded} />
      <LoopControls view={view} enabled={loaded} />
      <div className="cdj-main">
        <JogWheel deckId={deckId} enabled={loaded} scratching={view.scratching} />
        <TempoControls view={view} enabled={loaded} />
      </div>
      <TransportButtons view={view} enabled={loaded} />
    </section>
  )
}
