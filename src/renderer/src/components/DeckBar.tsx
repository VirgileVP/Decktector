import type { DeckId, TrackInfo } from '../model'
import { formatTimeMs } from '../lib/playhead'
import { TRANSPORT_GLYPHS } from '../lib/transport'
import { useDecktectiveStore } from '../state/store'
import { selectTimecodeSource, useTimecodeStore } from '../state/timecode'
import type { DeckBarFields, DeckBarLayout } from './DeckBarLayout'
import { DeckStat } from './DeckStat'

interface DeckBarProps {
  deckId: DeckId
  track: TrackInfo | null
  fields: DeckBarFields
  layout: Exclude<DeckBarLayout, 'hidden'>
}

/** The deck's identity block and its TC / transport / sync / time / pitch / BPM readouts. */
export function DeckBar({ deckId, track, fields, layout }: DeckBarProps): React.JSX.Element {
  const deck = useDecktectiveStore((s) => s.decks[deckId])
  const isTimecodeSource = useTimecodeStore((s) => s.sourceDeckId === deckId)

  const playState = deck?.playState ?? 'empty'
  const playhead = deck?.playheadMs ?? null
  const remaining = track && playhead !== null ? track.durationMs - playhead : null
  const bpmValue = deck?.effectiveBpm ?? deck?.trackBpm ?? null
  const bpmText = bpmValue !== null ? bpmValue.toFixed(1) : '--.-'

  // The engine resolves artwork to a data: URL, so it drops straight into
  // an <img> with nothing to allocate or revoke.
  const artworkUrl = track?.artwork ?? null

  return (
    <div className={layout === 'rail' ? 'deck-bar deck-bar-rail' : 'deck-bar'}>
      {fields.artwork && (
        <div className="deck-art">
          {artworkUrl ? <img src={artworkUrl} alt="" /> : <span>♪</span>}
        </div>
      )}

      {fields.trackInfo ? (
        <div className="deck-info">
          <div className="deck-title">{track?.title ?? 'No track loaded'}</div>
          <div className="deck-sub">
            {track?.artist ?? '—'}
            {track?.album ? ` · ${track.album}` : ''}
          </div>
        </div>
      ) : (
        // Keeps a flex-growing element in the bar so the rest doesn't collapse
        // leftward once the title/artist block is hidden. The rail has nothing
        // to push against, so it does without.
        layout === 'bar' && <div className="deck-info-spacer" />
      )}

      {fields.trackTags && (
        <div className="deck-props">
          {track?.key && <span className="prop">{track.key}</span>}
          {track?.genre && <span className="prop prop-muted">{track.genre}</span>}
        </div>
      )}

      {fields.tcButton && (
        <button
          type="button"
          className={`deck-tc-button${isTimecodeSource ? ' active' : ''}`}
          title={
            isTimecodeSource
              ? 'Stop sending timecode from this deck'
              : 'Send timecode from this deck'
          }
          aria-pressed={isTimecodeSource}
          onClick={() => selectTimecodeSource(isTimecodeSource ? null : deckId)}
        >
          TC
        </button>
      )}

      {fields.transport && (
        <div className={`deck-transport state-${playState}`}>{TRANSPORT_GLYPHS[playState]}</div>
      )}

      {fields.syncBadges && (
        <div className="deck-msync">
          <span className={deck?.isOnAir ? 'lit lit-onair' : ''}>ON AIR</span>
          <span className={deck?.isMaster ? 'lit lit-master' : ''}>MASTER</span>
          <span className={deck?.isSynced ? 'lit lit-sync' : ''}>SYNC</span>
        </div>
      )}

      <div className="deck-stats">
        {fields.time && (
          <DeckStat
            className="deck-stat-time"
            label="TIME"
            value={playhead !== null ? formatTimeMs(playhead) : '--:--'}
            sub={
              fields.timeRemaining && (
                <span className="deck-stat-remain">
                  {remaining !== null ? `-${formatTimeMs(remaining)}` : ''}
                </span>
              )
            }
          />
        )}
        {fields.pitch && (
          <DeckStat
            className="deck-stat-pitch"
            label="PITCH"
            value={deck ? `${deck.pitch >= 0 ? '+' : ''}${deck.pitch.toFixed(2)}%` : '--.--%'}
          />
        )}
        {fields.bpm && <DeckStat className="deck-stat-bpm" label="BPM" value={bpmText} />}
      </div>
    </div>
  )
}
