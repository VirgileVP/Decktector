import { useEffect, useMemo } from 'react'

import type { DeckId, PlayState } from '../../../engine/types'
import { formatTimeMs } from '../lib/playhead'
import { useDecktectiveStore } from '../state/store'
import { WaveformDetail } from './WaveformDetail'
import { WaveformOverview } from './WaveformOverview'

const PLAY_STATE_LABELS: Record<PlayState, string> = {
  empty: 'NO TRACK',
  loading: 'LOADING…',
  cued: 'CUE',
  playing: 'PLAY',
  paused: 'PAUSE',
  searching: 'SEARCH',
  ended: 'END'
}

interface DeckPanelProps {
  deckId: DeckId
  accentColor: string
}

export function DeckPanel({ deckId, accentColor }: DeckPanelProps): React.JSX.Element {
  const deck = useDecktectiveStore((s) => s.decks[deckId])
  const track = useDecktectiveStore((s) => s.tracks[deckId] ?? null)

  const artworkUrl = useMemo(() => {
    if (!track?.artwork) return null
    return URL.createObjectURL(new Blob([track.artwork as BlobPart]))
  }, [track])

  useEffect(() => {
    return () => {
      if (artworkUrl) URL.revokeObjectURL(artworkUrl)
    }
  }, [artworkUrl])

  const playhead = deck?.playheadMs ?? null
  const remaining = track && playhead !== null ? track.durationMs - playhead : null

  return (
    <section className="deck" style={{ ['--deck-color' as string]: accentColor }}>
      <div className="deck-head">
        <div className="deck-number">{deckId}</div>

        <div className="deck-artwork">
          {artworkUrl ? (
            <img src={artworkUrl} alt="" />
          ) : (
            <div className="deck-artwork-empty">♪</div>
          )}
        </div>

        <div className="deck-meta">
          <div className="deck-title">{track?.title ?? 'No track loaded'}</div>
          <div className="deck-artist">{track?.artist ?? '—'}</div>
          <div className="deck-tags">
            {track?.key && <span className="tag">{track.key}</span>}
            {track?.genre && <span className="tag">{track.genre}</span>}
            {track?.album && <span className="tag tag-muted">{track.album}</span>}
          </div>
        </div>

        <div className="deck-badges">
          <span className={`badge ${deck?.isOnAir ? 'badge-onair' : ''}`}>ON AIR</span>
          <span className={`badge ${deck?.isMaster ? 'badge-master' : ''}`}>MASTER</span>
          <span className={`badge ${deck?.isSynced ? 'badge-sync' : ''}`}>SYNC</span>
          <span className={`badge badge-state state-${deck?.playState ?? 'empty'}`}>
            {PLAY_STATE_LABELS[deck?.playState ?? 'empty']}
          </span>
        </div>

        <div className="deck-stats">
          <div className="stat-bpm">
            <span className="stat-value">{deck?.effectiveBpm?.toFixed(1) ?? '--.-'}</span>
            <span className="stat-label">BPM</span>
          </div>
          <div className="stat-pitch">
            {deck ? `${deck.pitch >= 0 ? '+' : ''}${deck.pitch.toFixed(2)}%` : '--'}
          </div>
          <div className="stat-time">
            <span>{playhead !== null ? formatTimeMs(playhead) : '--:--'}</span>
            <span className="stat-remain">
              {remaining !== null ? `-${formatTimeMs(remaining)}` : '--:--'}
            </span>
          </div>
        </div>
      </div>

      <WaveformDetail deckId={deckId} track={track} accentColor={accentColor} />
      <WaveformOverview deckId={deckId} track={track} accentColor={accentColor} />
    </section>
  )
}
