import type { DeckId, PlayState } from '../model'
import { formatTimeMs } from '../lib/playhead'
import { DECK_RAIL_WIDTH_RANGE, useSettings } from '../state/settings'
import { useDecktectiveStore } from '../state/store'
import { selectTimecodeSource, useTimecodeStore } from '../state/timecode'
import { CueList } from './CueList'
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
}

export function DeckPanel({ deckId, accentColor }: DeckPanelProps): React.JSX.Element {
  const deck = useDecktectiveStore((s) => s.decks[deckId])
  const track = useDecktectiveStore((s) => s.tracks[deckId] ?? null)
  const isTimecodeSource = useTimecodeStore((s) => s.sourceDeckId === deckId)
  const showTrackTags = useSettings((s) => s.showDeckTrackTags)
  const showSyncBadges = useSettings((s) => s.showDeckSyncBadges)
  const showPitch = useSettings((s) => s.showDeckPitch)
  const showTimeRemaining = useSettings((s) => s.showDeckTimeRemaining)
  const showCueList = useSettings((s) => s.showDeckCueList)
  const showEventList = useSettings((s) => s.showDeckEventList)
  const showArtwork = useSettings((s) => s.showDeckArtwork)
  const showWaveformOverview = useSettings((s) => s.showDeckWaveformOverview)
  const showWaveformDetail = useSettings((s) => s.showDeckWaveformDetail)
  const cueListWidth = useSettings((s) => s.deckCueListWidth)
  const eventListWidth = useSettings((s) => s.deckEventListWidth)
  const updateSetting = useSettings((s) => s.update)
  const showTimeline = showWaveformOverview || showWaveformDetail

  // The engine resolves artwork to a data: URL, so it drops straight into
  // an <img> with nothing to allocate or revoke.
  const artworkUrl = track?.artwork ?? null

  const playState = deck?.playState ?? 'empty'
  const playhead = deck?.playheadMs ?? null
  const remaining = track && playhead !== null ? track.durationMs - playhead : null
  const isOnAir = deck?.isOnAir ?? false

  return (
    <section
      className={`deck ${isOnAir ? 'deck-onair' : 'deck-idle'}`}
      style={{ ['--deck-color' as string]: accentColor }}
    >
      <div className="deck-bar poppable">
        <div className="deck-no">{deckId}</div>
        <PopOutButton panelId="deck" deckId={deckId} title={`Open deck ${deckId} in its own window`} />

        {showArtwork && (
          <div className="deck-art">
            {artworkUrl ? <img src={artworkUrl} alt="" /> : <span>♪</span>}
          </div>
        )}

        <div className="deck-info">
          <div className="deck-title">{track?.title ?? 'No track loaded'}</div>
          <div className="deck-sub">
            {track?.artist ?? '—'}
            {track?.album ? ` · ${track.album}` : ''}
          </div>
        </div>

        {showTrackTags && (
          <div className="deck-props">
            {track?.key && <span className="prop">{track.key}</span>}
            {track?.genre && <span className="prop prop-muted">{track.genre}</span>}
          </div>
        )}

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

        <div className={`deck-transport state-${playState}`}>{TRANSPORT_GLYPHS[playState]}</div>

        {showSyncBadges && (
          <div className="deck-msync">
            <span className={isOnAir ? 'lit lit-onair' : ''}>ON AIR</span>
            <span className={deck?.isMaster ? 'lit lit-master' : ''}>MASTER</span>
            <span className={deck?.isSynced ? 'lit lit-sync' : ''}>SYNC</span>
          </div>
        )}

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
            {showPitch && (
              <span className="time-pitch">
                {deck ? `${deck.pitch >= 0 ? '+' : ''}${deck.pitch.toFixed(2)}%` : ''}
              </span>
            )}
            {showTimeRemaining && (
              <span className="time-remain">
                {remaining !== null ? `-${formatTimeMs(remaining)}` : ''}
              </span>
            )}
          </div>
        </div>
      </div>

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
    </section>
  )
}
