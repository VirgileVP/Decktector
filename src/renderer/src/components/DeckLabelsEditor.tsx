import { useState } from 'react'

import {
  DECK_LABEL_MAX_LENGTH,
  MAX_PLAYER_NUMBER,
  sanitizeDeckLabel,
  withDeckLabel
} from '../lib/deck-labels'
import type { DeckId } from '../model'
import { useSettings } from '../state/settings'
import { TextRow } from './SettingControls'

const PLAYER_NUMBERS: readonly DeckId[] = Array.from(
  { length: MAX_PLAYER_NUMBER },
  (_, index) => index + 1
)

interface DeckLabelRowProps {
  deckId: DeckId
}

function DeckLabelRow({ deckId }: DeckLabelRowProps): React.JSX.Element {
  const storedLabel = useSettings((s) => s.deckLabels[deckId] ?? '')
  const update = useSettings((s) => s.update)
  // The store only ever holds the trimmed label, so typing a space between two
  // words needs a draft that keeps what was typed until the field loses focus.
  const [draft, setDraft] = useState(storedLabel)

  const handleChange = (raw: string): void => {
    setDraft(raw)
    update('deckLabels', withDeckLabel(useSettings.getState().deckLabels, deckId, raw))
  }

  return (
    <TextRow
      label={`Player ${deckId}`}
      hint="Shown next to the player number, e.g. the mixer channel"
      value={draft}
      maxLength={DECK_LABEL_MAX_LENGTH}
      placeholder="No label"
      onChange={handleChange}
      onBlur={() => setDraft(sanitizeDeckLabel(draft))}
    />
  )
}

/** One free-text label field per player number. */
export function DeckLabelsEditor(): React.JSX.Element {
  return (
    <section className="settings-section">
      <h3>Deck labels</h3>
      {PLAYER_NUMBERS.map((deckId) => (
        <DeckLabelRow key={deckId} deckId={deckId} />
      ))}
    </section>
  )
}
