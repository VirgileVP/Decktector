import type { DeckId } from '../model'
import { useDeckActive } from '../state/deck-activity'
import { DECK_RAIL_WIDTH_RANGE, useSettings, type DeckViewToggles } from '../state/settings'
import { useDecktectorStore } from '../state/store'
import { BeatIndicator } from './BeatIndicator'
import { CueList } from './CueList'
import { DeckBar } from './DeckBar'
import { deckBarFields, deckBarLayout } from './DeckBarLayout'
import { DeckIdentity } from './DeckIdentity'
import { PanelResizeHandle } from './PanelResizeHandle'
import { PopOutButton } from './PopOutButton'
import { TrackEventList } from './TrackEventList'
import { WaveformDetail } from './WaveformDetail'
import { WaveformOverview } from './WaveformOverview'

interface DeckPanelProps {
  deckId: DeckId
  accentColor: string
  /** Which pieces are shown — the Deck view settings in the main window, a window's own in a panel window. */
  view: DeckViewToggles
}

export function DeckPanel({ deckId, accentColor, view }: DeckPanelProps): React.JSX.Element {
  const track = useDecktectorStore((s) => s.tracks[deckId] ?? null)
  const barFields = deckBarFields(view)
  const {
    showDeckCueList: showCueList,
    showDeckEventList: showEventList,
    showDeckWaveformOverview: showWaveformOverview,
    showDeckWaveformDetail: showWaveformDetail
  } = view
  const cueListWidth = useSettings((s) => s.deckCueListWidth)
  const eventListWidth = useSettings((s) => s.deckEventListWidth)
  const updateSetting = useSettings((s) => s.update)
  const showTimeline = showWaveformOverview || showWaveformDetail
  const barLayout = deckBarLayout(barFields)
  const isActive = useDeckActive(deckId)

  return (
    <section
      className={`deck deck-full poppable ${isActive ? 'deck-onair' : 'deck-idle'}`}
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
              <DeckIdentity deckId={deckId} showDeckNumber={view.showDeckNumber} />
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
