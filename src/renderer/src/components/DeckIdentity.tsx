import type { DeckId } from '../model'
import { useSettings } from '../state/settings'

interface DeckIdentityProps {
  deckId: DeckId
  showDeckNumber: boolean
}

/**
 * Badge naming a deck on the waveform: the player number, and the operator's
 * label beside it when one is set. Renders nothing if both are switched off.
 */
export function DeckIdentity({
  deckId,
  showDeckNumber
}: DeckIdentityProps): React.JSX.Element | null {
  const label = useSettings((s) => s.deckLabels[deckId] ?? '')

  if (!showDeckNumber && label === '') return null
  return (
    <div className="deck-id">
      {showDeckNumber && <div className="deck-no">{deckId}</div>}
      {label !== '' && <div className="deck-no deck-no-label">{label}</div>}
    </div>
  )
}
