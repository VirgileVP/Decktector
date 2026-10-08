import type { DeckId } from '../model'
import { useEffectiveOnAirSource } from '../state/deck-activity'
import { useDecktectorStore } from '../state/store'

interface DeckSyncBadgesProps {
  deckId: DeckId
  className?: string
}

/** The ON AIR / MASTER / SYNC indicators, each lit while its flag is set. */
export function DeckSyncBadges({ deckId, className }: DeckSyncBadgesProps): React.JSX.Element {
  const deck = useDecktectorStore((s) => s.decks[deckId])
  // The light is the mixer's own flag: without a trusted mixer it would only
  // ever read "off air" for a deck that may well be live, so it is left out.
  const showOnAirLight = useEffectiveOnAirSource() === 'mixer'

  return (
    <div className={className ? `deck-msync ${className}` : 'deck-msync'}>
      {showOnAirLight && <span className={deck?.isOnAir ? 'lit lit-onair' : ''}>ON AIR</span>}
      <span className={deck?.isMaster ? 'lit lit-master' : ''}>MASTER</span>
      <span className={deck?.isSynced ? 'lit lit-sync' : ''}>SYNC</span>
    </div>
  )
}
