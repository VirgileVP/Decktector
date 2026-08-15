import type { DeckId, TrackCue, TrackInfo } from '../model'
import { formatTimeMs } from '../lib/playhead'
import { useDecktectiveStore } from '../state/store'

const KIND_LABELS: Record<TrackCue['kind'], string> = {
  memory: 'MEM',
  hotcue: '',
  loop: 'LOOP',
  hotloop: 'LOOP'
}

type CueStatus = 'passed' | 'current' | 'upcoming'

/**
 * The "current" cue is the last one the playhead has crossed (or whose
 * loop range contains it); earlier ones are passed, later ones upcoming.
 */
function cueStatus(cue: TrackCue, index: number, cues: TrackCue[], playhead: number): CueStatus {
  if (cue.offsetMs > playhead) return 'upcoming'
  const end = cue.lengthMs !== null ? cue.offsetMs + cue.lengthMs : null
  if (end !== null && playhead < end) return 'current'
  const next = cues[index + 1]
  return next === undefined || next.offsetMs > playhead ? 'current' : 'passed'
}

interface CueListProps {
  deckId: DeckId
  track: TrackInfo | null
  width: number
}

/** Left-hand rail listing the track's cues with passed/current/upcoming states. */
export function CueList({ deckId, track, width }: CueListProps): React.JSX.Element {
  const playhead = useDecktectiveStore((s) => s.decks[deckId]?.playheadMs ?? null)

  if (!track || track.cues.length === 0) {
    return (
      <aside className="cue-list cue-list-empty" style={{ width }}>
        NO CUES
      </aside>
    )
  }

  return (
    <aside className="cue-list" style={{ width }}>
      {track.cues.map((cue, index) => {
        const status = playhead !== null ? cueStatus(cue, index, track.cues, playhead) : 'upcoming'
        return (
          <div key={`${cue.kind}-${cue.offsetMs}`} className={`cue-row cue-${status}`}>
            <span className="cue-marker" style={{ background: cue.color }}>
              {cue.button ?? ''}
            </span>
            <span className="cue-name">
              {cue.label ?? (cue.button ? `Hot cue ${cue.button}` : KIND_LABELS[cue.kind])}
            </span>
            <span className="cue-time">{formatTimeMs(cue.offsetMs)}</span>
          </div>
        )
      })}
    </aside>
  )
}
