import type { SimDeckView } from '@renderer/model/SimDeckView'

import { openTrackBrowser } from '../state/sim-store'
import { sendCommand } from '../net/sim-connection'
import { HardwareButton } from './HardwareButton'

/** PLAY/PAUSE and CUE on the left; MASTER, SYNC, EJECT and LOAD on the right. */
export function TransportButtons({
  view,
  enabled
}: {
  view: SimDeckView
  enabled: boolean
}): React.JSX.Element {
  const deck = view.deckId
  return (
    <div className="transport">
      <div className="button-row">
        <HardwareButton
          shape="round"
          label="CUE"
          lit={view.playState === 'cued'}
          disabled={!enabled}
          onClick={() => sendCommand({ type: 'cue', deck })}
        />
        <HardwareButton
          shape="round"
          label="PLAY"
          lit={view.playState === 'playing'}
          disabled={!enabled}
          title="Play / pause"
          onClick={() => sendCommand({ type: 'playPause', deck })}
        />
      </div>
      <div className="button-row">
        <HardwareButton
          label="MASTER"
          lit={view.isMaster}
          disabled={!enabled}
          onClick={() => sendCommand({ type: 'master', deck })}
        />
        <HardwareButton
          label="SYNC"
          lit={view.isSynced}
          disabled={!enabled}
          onClick={() => sendCommand({ type: 'sync', deck })}
        />
      </div>
      <div className="button-row">
        <HardwareButton
          label="EJECT"
          disabled={!enabled}
          onClick={() => sendCommand({ type: 'eject', deck })}
        />
        <HardwareButton label="LOAD" onClick={() => openTrackBrowser(deck)} />
      </div>
    </div>
  )
}
