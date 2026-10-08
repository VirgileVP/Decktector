import type { DeckId } from '../model'
import { formatBpm, formatPitch } from '../lib/deck-readouts'
import { formatTimeMs } from '../lib/playhead'
import { TRANSPORT_GLYPHS } from '../lib/transport'
import type { CompactDeckFields } from '../panels/panel-display'
import { useDeckActive } from '../state/deck-activity'
import { useSettings } from '../state/settings'
import { useDecktectorStore } from '../state/store'
import { DeckStat } from './DeckStat'
import { DeckSyncBadges } from './DeckSyncBadges'

interface CompactDeckPanelProps {
  deckId: DeckId
  accentColor: string
  fields: CompactDeckFields
}

/**
 * A deck as one row — artwork beside a column of text and readouts — for a
 * panel window too small for waveforms. No waveforms, cue rail or event rail;
 * every other piece is one of `fields`, picked from the window's toolbar.
 */
export function CompactDeckPanel({
  deckId,
  accentColor,
  fields
}: CompactDeckPanelProps): React.JSX.Element {
  const deck = useDecktectorStore((s) => s.decks[deckId])
  const track = useDecktectorStore((s) => s.tracks[deckId] ?? null)
  const deckLabel = useSettings((s) => s.deckLabels[deckId] ?? '')
  const isActive = useDeckActive(deckId)

  // The engine resolves artwork to a data: URL, so it drops straight into
  // an <img> with nothing to allocate or revoke.
  const artworkUrl = track?.artwork ?? null

  const playState = deck?.playState ?? 'empty'
  const playhead = deck?.playheadMs ?? null
  const remaining = track && playhead !== null ? track.durationMs - playhead : null

  const showLabel = fields.label && deckLabel !== ''
  const showIdentity = fields.deckNumber || showLabel
  const showTags = fields.trackTags && Boolean(track?.key || track?.genre)
  const showMeta =
    fields.transport ||
    fields.syncBadges ||
    fields.time ||
    fields.timeRemaining ||
    fields.pitch ||
    fields.bpm

  return (
    <section
      className={`deck deck-compact ${isActive ? 'deck-onair' : 'deck-idle'}`}
      style={{ ['--deck-color' as string]: accentColor }}
    >
      {fields.artwork && (
        <div className="deck-compact-art">
          {artworkUrl ? <img src={artworkUrl} alt="" /> : <span>♪</span>}
        </div>
      )}
      <div className="deck-compact-info">
        {showIdentity && (
          <div className="deck-compact-label">
            {fields.deckNumber && <span className="deck-compact-no">{deckId}</span>}
            {showLabel && <span className="deck-compact-label-text">{deckLabel}</span>}
          </div>
        )}
        {fields.title && (
          <div className="deck-compact-title">{track?.title ?? 'No track loaded'}</div>
        )}
        {fields.artist && <div className="deck-compact-sub">{track?.artist ?? '—'}</div>}
        {showTags && (
          <div className="deck-props deck-compact-tags">
            {track?.key && <span className="prop">{track.key}</span>}
            {track?.genre && <span className="prop prop-muted">{track.genre}</span>}
          </div>
        )}
        {showMeta && (
          <div className="deck-compact-meta">
            {fields.transport && (
              <span className={`deck-transport state-${playState}`}>
                {TRANSPORT_GLYPHS[playState]}
              </span>
            )}
            {fields.time && (
              <span className="deck-compact-time">
                {playhead !== null ? formatTimeMs(playhead) : '--:--'}
              </span>
            )}
            {fields.timeRemaining && (
              <span className="deck-compact-remain">
                {remaining !== null ? `-${formatTimeMs(remaining)}` : '--:--'}
              </span>
            )}
            {fields.pitch && (
              <span className="deck-compact-pitch">{formatPitch(deck?.pitch ?? null)}</span>
            )}
            {fields.bpm && (
              <DeckStat
                className="deck-compact-bpm"
                label="BPM"
                value={formatBpm(deck?.effectiveBpm ?? deck?.trackBpm ?? null)}
              />
            )}
            {fields.syncBadges && <DeckSyncBadges deckId={deckId} className="deck-compact-sync" />}
          </div>
        )}
      </div>
    </section>
  )
}
