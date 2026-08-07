import { isTauri } from '@tauri-apps/api/core'
import { useShallow } from 'zustand/react/shallow'

import { useDecktectiveStore } from '../state/store'
import { selectTimecodeSource, useTimecodeStore } from '../state/timecode'

/** Timecode output only exists in the desktop shell — a remote browser has no generator to drive. */
const IS_TAURI = isTauri()

/**
 * One-click alternative to each deck panel's own TC badge — picks which
 * deck's playhead the timecode generator reads, right next to the clock it
 * drives. Mirrors whichever decks are actually connected, so the row
 * neither shows a pick for gear that isn't there nor hides one that is.
 */
export function TimecodeSourceButtons(): React.JSX.Element | null {
  const sourceDeckId = useTimecodeStore((s) => s.sourceDeckId)
  const deckIds = useDecktectiveStore(
    useShallow((s) =>
      Object.keys(s.decks)
        .map(Number)
        .sort((a, b) => a - b)
    )
  )

  if (!IS_TAURI) return null

  return (
    <div className="tc-source-buttons">
      {deckIds.map((deckId) => {
        const active = sourceDeckId === deckId
        return (
          <button
            key={deckId}
            type="button"
            className={active ? 'active' : ''}
            title={active ? 'Stop sending timecode from this deck' : `Send timecode from deck ${deckId}`}
            aria-pressed={active}
            onClick={() => selectTimecodeSource(active ? null : deckId)}
          >
            {deckId}
          </button>
        )
      })}
    </div>
  )
}
