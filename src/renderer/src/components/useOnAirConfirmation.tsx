import { useState } from 'react'

import type { ShowData } from '../lib/show'
import { onAirImpact, type DeckImpact } from '../lib/show-onair-impact'
import { useShow } from '../state/show'
import { currentDeckRows, currentDecksForImpact } from '../state/show-onair'
import { ConfirmDialog } from './ConfirmDialog'

interface PendingChange {
  impacts: DeckImpact[]
  apply: () => void
}

export interface OnAirConfirmation {
  /**
   * Applies a change that turns the show into `after` — at once when no deck
   * on air would move, after a confirmation naming the decks otherwise.
   */
  confirmShowChange: (after: ShowData | null, apply: () => void) => void
  /** The confirmation to render while one is open. */
  dialog: React.JSX.Element | null
}

/**
 * Guards edits that would move a deck on air to another timecode: a start
 * edit, an import, a replaced or loaded show. The console is following that
 * deck right now, so the operator sees the old and new timecode first.
 */
export function useOnAirConfirmation(): OnAirConfirmation {
  const [pending, setPending] = useState<PendingChange | null>(null)

  const confirmShowChange = (after: ShowData | null, apply: () => void): void => {
    const impacts = onAirImpact(
      useShow.getState().show,
      after,
      currentDecksForImpact(),
      currentDeckRows()
    )
    if (impacts.length === 0) apply()
    else setPending({ impacts, apply })
  }

  const dialog =
    pending === null ? null : (
      <ConfirmDialog
        title="Move a deck on air to another timecode?"
        confirmLabel="Apply"
        onConfirm={() => {
          pending.apply()
          setPending(null)
        }}
        onCancel={() => setPending(null)}
      >
        <p>The console is following these decks right now:</p>
        <ul className="show-impact-list">
          {pending.impacts.map((impact) => (
            <li key={impact.deckId}>
              Deck {impact.deckId}: {impact.before} → {impact.after}
            </li>
          ))}
        </ul>
      </ConfirmDialog>
    )

  return { confirmShowChange, dialog }
}
