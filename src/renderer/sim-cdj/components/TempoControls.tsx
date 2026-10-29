import type { SimDeckView } from '@renderer/model/SimDeckView'

import { TEMPO_RANGE_OPTIONS } from '../lib/constants'
import { PITCH_FADER_TRAVEL } from '../lib/fader-math'
import { sendCommand } from '../net/sim-connection'
import { HardwareButton } from './HardwareButton'
import { VerticalFader } from './VerticalFader'

const PITCH_FADER_CENTER = 0

/**
 * The TEMPO column: range selector and the pitch fader. As on a CDJ, the
 * fader's + end is toward the DJ (bottom), so pulling it down speeds up.
 */
export function TempoControls({
  view,
  enabled
}: {
  view: SimDeckView
  enabled: boolean
}): React.JSX.Element {
  return (
    <div className="tempo">
      <span className="hw-caption">TEMPO</span>
      <div className="tempo-ranges">
        {TEMPO_RANGE_OPTIONS.map((option) => (
          <HardwareButton
            key={option.range}
            label={option.label}
            lit={view.tempoRange === option.range}
            disabled={!enabled}
            onClick={() =>
              sendCommand({ type: 'setTempoRange', deck: view.deckId, range: option.range })
            }
          />
        ))}
      </div>
      <div className="tempo-fader">
        <span className="hw-caption">&minus;</span>
        <VerticalFader
          value={view.pitchFader}
          travel={PITCH_FADER_TRAVEL}
          label={`Deck ${view.deckId} pitch fader`}
          disabled={!enabled}
          resetValue={PITCH_FADER_CENTER}
          centerMark
          onChange={(position) =>
            sendCommand({ type: 'setPitchFader', deck: view.deckId, position })
          }
        />
        <span className="hw-caption">+</span>
      </div>
    </div>
  )
}
