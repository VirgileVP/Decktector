import type { ShowRow } from '../lib/show'
import { durationLabel, rowBadges, stepTrim, trimLabel } from '../lib/show-row-view'
import type { ShowFps } from '../lib/show-timecode'
import type { RowIssue } from '../lib/show-validation'
import type { DeckId } from '../model'
import type { ShowRowPatch } from '../state/show'
import { TimecodeInput } from './TimecodeInput'

interface ShowRowEditorProps {
  row: ShowRow
  /** 1-based position in the show, as the operator counts rows. */
  position: number
  fps: ShowFps
  issues: readonly RowIssue[]
  /** The decks playing this row now, in deck order; empty when none is. */
  playingDecks: readonly DeckId[]
  /** A mirror (a Show panel window) shows the row without letting it change. */
  readOnly: boolean
  onChange: (patch: ShowRowPatch) => void
  onRemove: () => void
}

const TRIM_HINT = '+ = cues fire earlier'

/**
 * One track of the show: its identity from the import (read-only), the start
 * timecode typed as a label, the trim stepped one frame at a time, and the
 * row's issues as badges. A row a deck is playing is marked with that deck.
 */
export function ShowRowEditor({
  row,
  position,
  fps,
  issues,
  playingDecks,
  readOnly,
  onChange,
  onRemove
}: ShowRowEditorProps): React.JSX.Element {
  const label = `${row.title}${row.artist === '' ? '' : ` — ${row.artist}`}`
  const playing = playingDecks.length > 0
  const rowClass = `show-row${row.absent ? ' show-row-absent' : ''}${playing ? ' show-row-playing' : ''}`

  return (
    <tr className={rowClass} data-row-id={row.id}>
      <td className="show-cell-position">
        {position}
        {playing && (
          <span className="show-row-deck" title={`Playing on deck ${playingDecks.join(', ')}`}>
            ▶ {playingDecks.join(' ')}
          </span>
        )}
      </td>
      <td className="show-cell-text" title={row.title}>
        {row.title}
      </td>
      <td className="show-cell-text show-cell-muted" title={row.artist}>
        {row.artist}
      </td>
      <td className="show-cell-duration">{durationLabel(row.durationSeconds)}</td>
      <td>
        <TimecodeInput
          value={row.startTimecode}
          fps={fps}
          allowBlank
          ariaLabel={`Start timecode of ${label}`}
          disabled={readOnly}
          onCommit={(startTimecode) => onChange({ startTimecode })}
        />
      </td>
      <td>
        <span className="show-trim" title={TRIM_HINT}>
          <button
            type="button"
            className="show-trim-step"
            aria-label={`One frame later for ${label}`}
            disabled={readOnly}
            onClick={() => onChange({ trimMs: stepTrim(row.trimMs, fps, -1) })}
          >
            −
          </button>
          <span className="show-trim-value">{trimLabel(row.trimMs, fps)}</span>
          <button
            type="button"
            className="show-trim-step"
            aria-label={`One frame earlier for ${label}`}
            disabled={readOnly}
            onClick={() => onChange({ trimMs: stepTrim(row.trimMs, fps, 1) })}
          >
            +
          </button>
        </span>
      </td>
      <td>
        <span className="show-badges">
          {rowBadges(issues).map((badge) => (
            <span key={badge.kind} className={`show-badge show-badge-${badge.tone}`}>
              {badge.label}
            </span>
          ))}
        </span>
      </td>
      <td>
        {!readOnly && (
          <button
            type="button"
            className="lineup-remove"
            aria-label={`Remove ${label}`}
            title="Remove from the show"
            onClick={onRemove}
          >
            ×
          </button>
        )}
      </td>
    </tr>
  )
}
