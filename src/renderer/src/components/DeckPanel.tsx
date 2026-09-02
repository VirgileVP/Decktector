import type { DeckId, PlayState } from '../model'
import { formatTimeMs } from '../lib/playhead'
import { DECK_RAIL_WIDTH_RANGE, useSettings } from '../state/settings'
import { useDecktectiveStore } from '../state/store'
import { selectTimecodeSource, useTimecodeStore } from '../state/timecode'
import { BeatIndicator } from './BeatIndicator'
import { CueList } from './CueList'
import { DeckStat } from './DeckStat'
import { PanelResizeHandle } from './PanelResizeHandle'
import { PopOutButton } from './PopOutButton'
import { TrackEventList } from './TrackEventList'
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
  /** `'compact'` is for a small panel window — see `PanelChrome`'s compact toggle. */
  variant?: 'full' | 'compact'
}

export function DeckPanel({
  deckId,
  accentColor,
  variant = 'full'
}: DeckPanelProps): React.JSX.Element {
  const deck = useDecktectiveStore((s) => s.decks[deckId])
  const track = useDecktectiveStore((s) => s.tracks[deckId] ?? null)
  const isTimecodeSource = useTimecodeStore((s) => s.sourceDeckId === deckId)
  const showNumber = useSettings((s) => s.showDeckNumber)
  const showTrackInfo = useSettings((s) => s.showDeckTrackInfo)
  const showTrackTags = useSettings((s) => s.showDeckTrackTags)
  const showTcButton = useSettings((s) => s.showDeckTcButton)
  const showTransport = useSettings((s) => s.showDeckTransport)
  const showSyncBadges = useSettings((s) => s.showDeckSyncBadges)
  const showTime = useSettings((s) => s.showDeckTime)
  const showPitch = useSettings((s) => s.showDeckPitch)
  const showTimeRemaining = useSettings((s) => s.showDeckTimeRemaining)
  const showBpm = useSettings((s) => s.showDeckBpm)
  const showCueList = useSettings((s) => s.showDeckCueList)
  const showEventList = useSettings((s) => s.showDeckEventList)
  const showArtwork = useSettings((s) => s.showDeckArtwork)
  const showWaveformOverview = useSettings((s) => s.showDeckWaveformOverview)
  const showWaveformDetail = useSettings((s) => s.showDeckWaveformDetail)
  const cueListWidth = useSettings((s) => s.deckCueListWidth)
  const eventListWidth = useSettings((s) => s.deckEventListWidth)
  const updateSetting = useSettings((s) => s.update)
  const showTimeline = showWaveformOverview || showWaveformDetail
  const showBar =
    showNumber ||
    showArtwork ||
    showTrackInfo ||
    showTrackTags ||
    showTcButton ||
    showTransport ||
    showSyncBadges ||
    showTime ||
    showPitch ||
    showBpm

  // The engine resolves artwork to a data: URL, so it drops straight into
  // an <img> with nothing to allocate or revoke.
  const artworkUrl = track?.artwork ?? null

  const playState = deck?.playState ?? 'empty'
  const playhead = deck?.playheadMs ?? null
  const remaining = track && playhead !== null ? track.durationMs - playhead : null
  const isOnAir = deck?.isOnAir ?? false
  const bpmValue = deck?.effectiveBpm ?? deck?.trackBpm ?? null
  const bpmText = bpmValue !== null ? bpmValue.toFixed(1) : '--.-'

  // No waveforms, no cue rail, no event rail, and the `showDeck*` settings
  // do not apply — a compact window is sized for artwork, title, artist,
  // BPM, play state and time remaining only.
  if (variant === 'compact') {
    return (
      <section
        className={`deck deck-compact ${isOnAir ? 'deck-onair' : 'deck-idle'}`}
        style={{ ['--deck-color' as string]: accentColor }}
      >
        <div className="deck-compact-art">
          {artworkUrl ? <img src={artworkUrl} alt="" /> : <span>♪</span>}
        </div>
        <div className="deck-compact-info">
          <div className="deck-compact-title">{track?.title ?? 'No track loaded'}</div>
          <div className="deck-compact-sub">{track?.artist ?? '—'}</div>
          <div className="deck-compact-meta">
            <span className={`deck-transport state-${playState}`}>
              {TRANSPORT_GLYPHS[playState]}
            </span>
            <DeckStat className="deck-compact-bpm" label="BPM" value={bpmText} />
            <span className="deck-compact-remain">
              {remaining !== null ? `-${formatTimeMs(remaining)}` : '--:--'}
            </span>
          </div>
        </div>
      </section>
    )
  }

  return (
    <section
      className={`deck deck-full poppable ${isOnAir ? 'deck-onair' : 'deck-idle'}`}
      style={{ ['--deck-color' as string]: accentColor }}
    >
      <PopOutButton
        panelId="deck"
        deckId={deckId}
        title={`Open deck ${deckId} in its own window`}
      />
      <div className="deck-stack">
        {showBar && (
          <div className="deck-bar">
            {showNumber && <div className="deck-no">{deckId}</div>}

            {showArtwork && (
              <div className="deck-art">
                {artworkUrl ? <img src={artworkUrl} alt="" /> : <span>♪</span>}
              </div>
            )}

            {showTrackInfo ? (
              <div className="deck-info">
                <div className="deck-title">{track?.title ?? 'No track loaded'}</div>
                <div className="deck-sub">
                  {track?.artist ?? '—'}
                  {track?.album ? ` · ${track.album}` : ''}
                </div>
              </div>
            ) : (
              // Keeps a flex-growing element in the bar so the rest doesn't
              // collapse leftward once the title/artist block is hidden.
              <div className="deck-info-spacer" />
            )}

            {showTrackTags && (
              <div className="deck-props">
                {track?.key && <span className="prop">{track.key}</span>}
                {track?.genre && <span className="prop prop-muted">{track.genre}</span>}
              </div>
            )}

            {showTcButton && (
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

            {showTransport && (
              <div className={`deck-transport state-${playState}`}>
                {TRANSPORT_GLYPHS[playState]}
              </div>
            )}

            {showSyncBadges && (
              <div className="deck-msync">
                <span className={isOnAir ? 'lit lit-onair' : ''}>ON AIR</span>
                <span className={deck?.isMaster ? 'lit lit-master' : ''}>MASTER</span>
                <span className={deck?.isSynced ? 'lit lit-sync' : ''}>SYNC</span>
              </div>
            )}

            <div className="deck-stats">
              {showTime && (
                <DeckStat
                  className="deck-stat-time"
                  label="TIME"
                  value={playhead !== null ? formatTimeMs(playhead) : '--:--'}
                  sub={
                    showTimeRemaining && (
                      <span className="deck-stat-remain">
                        {remaining !== null ? `-${formatTimeMs(remaining)}` : ''}
                      </span>
                    )
                  }
                />
              )}
              {showPitch && (
                <DeckStat
                  className="deck-stat-pitch"
                  label="PITCH"
                  value={deck ? `${deck.pitch >= 0 ? '+' : ''}${deck.pitch.toFixed(2)}%` : '--.--%'}
                />
              )}
              {showBpm && <DeckStat className="deck-stat-bpm" label="BPM" value={bpmText} />}
            </div>
          </div>
        )}

        <div className="deck-body">
          {showCueList && (
            <>
              <CueList deckId={deckId} track={track} width={cueListWidth} />
              {(showEventList || showTimeline) && (
                <PanelResizeHandle
                  width={cueListWidth}
                  min={DECK_RAIL_WIDTH_RANGE.min}
                  max={DECK_RAIL_WIDTH_RANGE.max}
                  onResize={(width) => updateSetting('deckCueListWidth', width)}
                />
              )}
            </>
          )}
          {showEventList && (
            <>
              <TrackEventList deckId={deckId} track={track} width={eventListWidth} />
              {showTimeline && (
                <PanelResizeHandle
                  width={eventListWidth}
                  min={DECK_RAIL_WIDTH_RANGE.min}
                  max={DECK_RAIL_WIDTH_RANGE.max}
                  onResize={(width) => updateSetting('deckEventListWidth', width)}
                />
              )}
            </>
          )}
          {showTimeline && (
            <div
              className={`deck-waves${
                showWaveformOverview && !showWaveformDetail ? ' deck-waves-overview-only' : ''
              }`}
            >
              {showWaveformOverview && (
                <WaveformOverview deckId={deckId} track={track} accentColor={accentColor} />
              )}
              {showWaveformDetail && (
                <WaveformDetail deckId={deckId} track={track} accentColor={accentColor} />
              )}
            </div>
          )}
        </div>
      </div>
      <BeatIndicator deckId={deckId} track={track} />
    </section>
  )
}
