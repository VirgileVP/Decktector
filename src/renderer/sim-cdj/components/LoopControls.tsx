import type { SimDeckView } from '@renderer/model/SimDeckView'

import { LOOP_LENGTHS_BEATS } from '../lib/constants'
import { activeLoopButton } from '../lib/loop-math'
import { sendCommand } from '../net/sim-connection'
import { HardwareButton } from './HardwareButton'

/** 4 BEAT / 8 BEAT loop buttons and RELOOP/EXIT. */
export function LoopControls({
  view,
  enabled
}: {
  view: SimDeckView
  enabled: boolean
}): React.JSX.Element {
  const litLength = activeLoopButton(view.loopState, view.trackBpm)
  const loopActive = view.loopState?.active === true
  return (
    <div className="button-row">
      {LOOP_LENGTHS_BEATS.map((beats) => (
        <HardwareButton
          key={beats}
          label={`${beats} BEAT`}
          lit={litLength === beats}
          disabled={!enabled}
          onClick={() => sendCommand({ type: 'loop', deck: view.deckId, beats })}
        />
      ))}
      <HardwareButton
        label={loopActive ? 'EXIT' : 'RELOOP'}
        lit={loopActive}
        disabled={!enabled || view.loopState === null}
        onClick={() => sendCommand({ type: 'reloopExit', deck: view.deckId })}
      />
    </div>
  )
}
