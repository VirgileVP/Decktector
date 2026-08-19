import { deckColor } from '../lib/deck-colors'
import { useSettings } from '../state/settings'
import { useDecktectiveStore } from '../state/store'
import { DeckPanel } from './DeckPanel'

interface DeckPanelHostProps {
  deckId: number
  variant?: 'full' | 'compact'
}

/**
 * Hosts `DeckPanel` for a panel window pinned to one deck. Reading
 * `deckColors` and `decks` needs hooks, which the panel registry's plain
 * lookup table cannot call — so the table points here instead of at
 * `DeckPanel` directly.
 *
 * Also covers the disconnected case: a window pinned to a deck that unplugs
 * must say so rather than render an empty panel.
 */
export function DeckPanelHost({ deckId, variant }: DeckPanelHostProps): React.JSX.Element {
  const deckColors = useSettings((s) => s.deckColors)
  const isConnected = useDecktectiveStore((s) => deckId in s.decks)

  if (!isConnected) {
    return <div className="panel-deck-placeholder">Deck {deckId} is not connected</div>
  }

  return <DeckPanel deckId={deckId} accentColor={deckColor(deckId, deckColors)} variant={variant} />
}
