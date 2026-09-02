import type { DeckId } from '../model'
import { formatTimeMs } from '../lib/playhead'
import { TRANSPORT_GLYPHS } from '../lib/transport'
import { DECK_RAIL_WIDTH_RANGE, useSettings } from '../state/settings'
import { useDecktectiveStore } from '../state/store'
import { BeatIndicator } from './BeatIndicator'
import { CueList } from './CueList'
import { DeckBar } from './DeckBar'
import { deckBarLayout, useDeckBarFields } from './DeckBarLayout'
import { DeckStat } from './DeckStat'
import { PanelResizeHandle } from './PanelResizeHandle'
import { PopOutButton } from './PopOutButton'
import { TrackEventList } from './TrackEventList'
import { WaveformDetail } from './WaveformDetail'
import { WaveformOverview } from './WaveformOverview'

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
  const barFields = useDeckBarFields()
  const showCueList = useSettings((s) => s.showDeckCueList)
  const showEventList = useSettings((s) => s.showDeckEventList)
  const showWaveformOverview = useSettings((s) => s.showDeckWaveformOverview)
  const showWaveformDetail = useSettings((s) => s.showDeckWaveformDetail)
  const cueListWidth = useSettings((s) => s.deckCueListWidth)
  const eventListWidth = useSettings((s) => s.deckEventListWidth)
  const updateSetting = useSettings((s) => s.update)
  const showTimeline = showWaveformOverview || showWaveformDetail
  const barLayout = deckBarLayout(barFields)

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
        {barLayout === 'bar' && (
          <DeckBar deckId={deckId} track={track} fields={barFields} layout="bar" />
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
      {barLayout === 'rail' && (
        <DeckBar deckId={deckId} track={track} fields={barFields} layout="rail" />
      )}
      <BeatIndicator deckId={deckId} track={track} />
    </section>
  )
}
