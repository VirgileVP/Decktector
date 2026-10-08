import { DECK_ACCENT_COLOR } from '../lib/deck-colors'
import type { DeckViewToggles } from '../state/settings'
import { useDecktectorStore } from '../state/store'
import { DeckPanel } from './DeckPanel'

interface DeckPanelHostProps {
  deckId: number
  /** The window's own Deck view toggles. */
  view: DeckViewToggles
}

/**
 * Hosts `DeckPanel` for a panel window pinned to one deck. Reading `decks`
 * needs a hook, which the panel registry's plain lookup table cannot call —
 * so the table points here instead of at `DeckPanel` directly.
 *
 * Also covers the disconnected case: a window pinned to a deck that unplugs
 * must say so rather than render an empty panel.
 */
export function DeckPanelHost({ deckId, view }: DeckPanelHostProps): React.JSX.Element {
  const isConnected = useDecktectorStore((s) => deckId in s.decks)

  if (!isConnected) {
    return <div className="panel-deck-placeholder">Deck {deckId} is not connected</div>
  }

  return <DeckPanel deckId={deckId} accentColor={DECK_ACCENT_COLOR} view={view} />
}
