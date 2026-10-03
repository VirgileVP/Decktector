import type { DeckId, TrackInfo } from '../model'
import { formatTimeMs } from '../lib/playhead'
import { TRANSPORT_GLYPHS } from '../lib/transport'
import { useEffectiveOnAirSource } from '../state/deck-activity'
import type { DownloadState } from '../state/download'
import { useTrackDownloadStore } from '../state/download'
import { useSettings } from '../state/settings'
import { useDecktectiveStore } from '../state/store'
import { selectTimecodeSource, useTimecodeStore } from '../state/timecode'
import type { DeckBarFields, DeckBarLayout } from './DeckBarLayout'
import { DeckStat } from './DeckStat'

const IDLE_DOWNLOAD_STATE: DownloadState = { status: 'idle' }

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
  const allowTrackDownload = useSettings((s) => s.allowTrackDownload)
  const downloadState = useTrackDownloadStore((s) => s.downloads[deckId] ?? IDLE_DOWNLOAD_STATE)
  const startDownload = useTrackDownloadStore((s) => s.startDownload)
  // The light is the mixer's own flag: without a trusted mixer it would only
  // ever read "off air" for a deck that may well be live, so it is left out.
  const showOnAirLight = useEffectiveOnAirSource() === 'mixer'

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

      {(fields.tcButton || (allowTrackDownload && track) || fields.transport || fields.syncBadges) && (
        <div className="deck-rail-actions">
          {(fields.tcButton || (allowTrackDownload && track) || fields.transport) && (
            <div className="deck-controls">
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

              {allowTrackDownload && track && (
                <button
                  type="button"
                  className={`deck-dl-button deck-dl-${downloadState.status}`}
                  title={
                    downloadState.status === 'downloading'
                      ? `Downloading track (${Math.round(downloadState.percent)}%)`
                      : downloadState.status === 'success'
                        ? `Downloaded: ${downloadState.filename}`
                        : downloadState.status === 'error'
                          ? `Download failed: ${downloadState.message}`
                          : 'Download track to Downloads/Decktective'
                  }
                  disabled={downloadState.status === 'downloading'}
                  onClick={() => void startDownload(deckId)}
                >
                  {downloadState.status === 'downloading' ? (
                    <span className="deck-dl-progress">{Math.round(downloadState.percent)}%</span>
                  ) : downloadState.status === 'success' ? (
                    <svg
                      width="12"
                      height="12"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="3"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  ) : downloadState.status === 'error' ? (
                    <span className="deck-dl-err">!</span>
                  ) : (
                    <svg
                      width="12"
                      height="12"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2.5"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                      <polyline points="7 10 12 15 17 10" />
                      <line x1="12" y1="15" x2="12" y2="3" />
                    </svg>
                  )}
                </button>
              )}

              {fields.transport && (
                <div className={`deck-transport state-${playState}`}>{TRANSPORT_GLYPHS[playState]}</div>
              )}
            </div>
          )}

          {fields.syncBadges && (
            <div className="deck-msync">
              {showOnAirLight && (
                <span className={deck?.isOnAir ? 'lit lit-onair' : ''}>ON AIR</span>
              )}
              <span className={deck?.isMaster ? 'lit lit-master' : ''}>MASTER</span>
              <span className={deck?.isSynced ? 'lit lit-sync' : ''}>SYNC</span>
            </div>
          )}
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
