import { deckTimecodeView } from '../lib/deck-timecode-view'
import { fpsOfFrameRate } from '../lib/show-timecode'
import type { DeckId } from '../model'
import { useSettings } from '../state/settings'
import { useShow } from '../state/show'
import { useDeckMatches } from '../state/show-matches'
import { useTimecodeStore } from '../state/timecode'

interface DeckTimecodeLineProps {
  deckId: DeckId
  /** In the narrow rail only the tone fits; the text moves to the tooltip. */
  compact: boolean
}

/**
 * One line in a loaded deck's bar: the show row it runs on and from which
 * timecode, or why it falls back, is cut or waits. Rendered for every deck, not only the timecode source; only
 * the source announces its changes, so a screen reader is not flooded.
 */
export function DeckTimecodeLine({
  deckId,
  compact
}: DeckTimecodeLineProps): React.JSX.Element | null {
  const status = useDeckMatches((state) => state.byDeck[deckId])
  const showFps = useShow((state) => state.show?.fps)
  const outputRate = useSettings((state) => state.timecodeFrameRate)
  const isSource = useTimecodeStore((state) => state.sourceDeckId === deckId)
  if (status === undefined || showFps === undefined) return null
  const view = deckTimecodeView(status, { showFps, outputFps: fpsOfFrameRate(outputRate) })
  if (view === null) return null

  return (
    <div
      className={`deck-tc-line deck-tc-${view.tone}${compact ? ' deck-tc-line-compact' : ''}`}
      title={view.text}
      aria-live={isSource ? 'polite' : undefined}
    >
      {compact ? (
        <span className="deck-tc-dot" aria-label={view.text} role="img" />
      ) : (
        <span className="deck-tc-text">{view.text}</span>
      )}
    </div>
  )
}
