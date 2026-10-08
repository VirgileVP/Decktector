import { DECK_ACCENT_COLOR } from '../lib/deck-colors'
import type { DeckWindowDisplay } from '../panels/panel-display'
import { useDecktectorStore } from '../state/store'
import { CompactDeckPanel } from './CompactDeckPanel'
import { DeckPanel } from './DeckPanel'

interface DeckPanelHostProps {
  deckId: number
  /** The window's own choice of layout and of what each layout shows. */
  display: DeckWindowDisplay
}

/**
 * Hosts a deck for a panel window pinned to one deck. Reading `decks`
 * needs a hook, which the panel registry's plain lookup table cannot call —
 * so the table points here instead of at the deck components directly.
 *
 * Also covers the disconnected case: a window pinned to a deck that unplugs
 * must say so rather than render an empty panel.
 */
export function DeckPanelHost({ deckId, display }: DeckPanelHostProps): React.JSX.Element {
  const isConnected = useDecktectorStore((s) => deckId in s.decks)

  if (!isConnected) {
    return <div className="panel-deck-placeholder">Deck {deckId} is not connected</div>
  }

  return display.layout === 'compact' ? (
    <CompactDeckPanel deckId={deckId} accentColor={DECK_ACCENT_COLOR} fields={display.compact} />
  ) : (
    <DeckPanel deckId={deckId} accentColor={DECK_ACCENT_COLOR} view={display.full} />
  )
}
