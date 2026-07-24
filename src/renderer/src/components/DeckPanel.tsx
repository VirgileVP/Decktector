import { useEffect, useMemo } from 'react'

import type { DeckId, PlayState } from '../../../engine/types'
import { formatTimeMs } from '../lib/playhead'
import { useDecktectiveStore } from '../state/store'
import { CueList } from './CueList'
import { WaveformDetail } from './WaveformDetail'
import { WaveformOverview } from './WaveformOverview'

const TRANSPORT_GLYPHS: Record<PlayState, string> = {
  empty: '—',
  loading: '…',
  cued: '●',
  playing: '▶',
  paused: '❙❙',
  searching: '≫',
  ended: '■'
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

  const playState = deck?.playState ?? 'empty'
  const playhead = deck?.playheadMs ?? null
  const remaining = track && playhead !== null ? track.durationMs - playhead : null

  return (
    <section
      className={`deck ${deck?.isOnAir ? 'deck-onair' : ''}`}
      style={{ ['--deck-color' as string]: accentColor }}
    >
      <div className="deck-bar">
        <div className="deck-no">{deckId}</div>

        <div className="deck-art">
          {artworkUrl ? <img src={artworkUrl} alt="" /> : <span>♪</span>}
        </div>

        <div className="deck-info">
          <div className="deck-title">{track?.title ?? 'No track loaded'}</div>
          <div className="deck-sub">
            {track?.artist ?? '—'}
            {track?.album ? ` · ${track.album}` : ''}
          </div>
        </div>

        <div className="deck-props">
          {track?.key && <span className="prop">{track.key}</span>}
          {track?.genre && <span className="prop prop-muted">{track.genre}</span>}
        </div>

        <div className={`deck-transport state-${playState}`}>{TRANSPORT_GLYPHS[playState]}</div>

        <div className="deck-msync">
          <span className={deck?.isMaster ? 'lit lit-master' : ''}>MASTER</span>
          <span className={deck?.isSynced ? 'lit lit-sync' : ''}>SYNC</span>
        </div>

        <div className="deck-time">
          <div className="time-main">
            {playhead !== null ? formatTimeMs(playhead) : '--:--'}
            <span className="time-sep"> / </span>
            {track ? formatTimeMs(track.durationMs) : '--:--'}
          </div>
          <div className="time-sub">
            <span className="time-bpm">
              {deck?.effectiveBpm ? `${deck.effectiveBpm.toFixed(1)} BPM` : '--.- BPM'}
            </span>
            <span className="time-pitch">
              {deck ? `${deck.pitch >= 0 ? '+' : ''}${deck.pitch.toFixed(2)}%` : ''}
            </span>
            <span className="time-remain">
              {remaining !== null ? `-${formatTimeMs(remaining)}` : ''}
            </span>
          </div>
        </div>
      </div>

      <div className="deck-body">
        <CueList deckId={deckId} track={track} />
        <div className="deck-waves">
          <WaveformOverview deckId={deckId} track={track} accentColor={accentColor} />
          <WaveformDetail deckId={deckId} track={track} accentColor={accentColor} />
        </div>
      </div>
    </section>
  )
}
