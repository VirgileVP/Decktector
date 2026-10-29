import { WaveformDetail } from '@renderer/components/WaveformDetail'
import { formatBpm, formatPitch } from '@renderer/lib/deck-readouts'
import { DECK_ACCENT_COLOR } from '@renderer/lib/deck-colors'
import type { TrackInfo } from '@renderer/model'
import type { SimDeckView } from '@renderer/model/SimDeckView'

import { BeatBoxes } from './BeatBoxes'
import { PreviewStrip } from './PreviewStrip'
import { ScreenBadges } from './ScreenBadges'
import { ScreenTimes } from './ScreenTimes'

interface CdjScreenProps {
  view: SimDeckView
  track: TrackInfo | null
}

/** The deck's display: track identity, tempo, times, beat, waveforms. */
export function CdjScreen({ view, track }: CdjScreenProps): React.JSX.Element {
  const loaded = view.trackIndex !== null
  return (
    <div className="screen">
      <ScreenBadges view={view} />
      <div className="screen-info">
        {track?.artwork ? (
          <img className="screen-artwork" src={track.artwork} alt="" />
        ) : (
          <div className="screen-artwork screen-artwork-empty" />
        )}
        <div className="screen-track">
          {loaded ? (
            <>
              <div className="screen-title">{track?.title ?? '...'}</div>
              <div className="screen-artist">{track?.artist ?? ''}</div>
              <div className="screen-key">{track?.key ? `KEY ${track.key}` : ''}</div>
            </>
          ) : (
            <div className="screen-no-track">NO TRACK</div>
          )}
        </div>
        <div className="screen-tempo">
          <div className="screen-bpm">{formatBpm(loaded ? view.effectiveBpm : null)}</div>
          <div className="screen-caption">BPM</div>
          <div className="screen-pitch">{formatPitch(loaded ? view.effectivePitch : null)}</div>
        </div>
      </div>
      <div className="screen-readouts">
        <ScreenTimes deckId={view.deckId} durationMs={view.durationMs} />
        <BeatBoxes beatInMeasure={loaded ? view.beatInMeasure : null} />
      </div>
      <div className="screen-waves">
        <WaveformDetail deckId={view.deckId} track={track} accentColor={DECK_ACCENT_COLOR} />
        <PreviewStrip deckId={view.deckId} track={track} loop={view.loopState} />
      </div>
    </div>
  )
}
