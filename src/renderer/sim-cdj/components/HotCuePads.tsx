import type { DeckId, TrackInfo } from '@renderer/model'

import { HOT_CUE_BUTTONS } from '../lib/constants'
import { hotCueFor } from '../lib/hot-cues'
import { sendCommand } from '../net/sim-connection'
import { HardwareButton } from './HardwareButton'

interface HotCuePadsProps {
  deckId: DeckId
  track: TrackInfo | null
  enabled: boolean
}

/** Pads A-H, each lit with its cue's colour when the track has a hot cue there. */
export function HotCuePads({ deckId, track, enabled }: HotCuePadsProps): React.JSX.Element {
  return (
    <div className="hot-cue-pads">
      {HOT_CUE_BUTTONS.map((button) => {
        const cue = track ? hotCueFor(track.cues, button) : null
        return (
          <HardwareButton
            key={button}
            shape="pad"
            label={button}
            lit={cue !== null}
            litColor={cue?.color}
            disabled={!enabled}
            title={cue?.label ?? `Hot cue ${button}`}
            onClick={() => sendCommand({ type: 'hotCue', deck: deckId, button })}
          />
        )
      })}
    </div>
  )
}
