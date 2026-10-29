import { CHANNEL_FADER_TRAVEL } from '../lib/fader-math'
import { DECK_COUNT } from '../lib/constants'
import { sendCommand } from '../net/sim-connection'
import { setMasterVolume, useAudioStore } from '../state/audio-store'
import { useSimStore } from '../state/sim-store'
import { VerticalFader } from './VerticalFader'

const DECK_IDS = Array.from({ length: DECK_COUNT }, (_, index) => index + 1)
const FULL_LEVEL = 1
const MASTER_VOLUME_STEP = 0.01

/** Four channel faders with an ON AIR lamp each, and the page's master volume. */
export function Mixer(): React.JSX.Element {
  const rig = useSimStore((state) => state.rig)
  const masterVolume = useAudioStore((state) => state.masterVolume)
  return (
    <section className="mixer" aria-label="Mixer">
      <div className="cdj-label">MIXER</div>
      <div className="mixer-channels">
        {DECK_IDS.map((deckId, index) => {
          const onAir = rig?.decks.find((deck) => deck.deckId === deckId)?.isOnAir === true
          return (
            <div key={deckId} className="mixer-channel">
              <span className={`onair-lamp${onAir ? ' onair-lamp-lit' : ''}`} title="ON AIR" />
              <VerticalFader
                value={rig?.channelFaders[index] ?? FULL_LEVEL}
                travel={CHANNEL_FADER_TRAVEL}
                label={`Channel ${deckId} fader`}
                disabled={!rig}
                resetValue={FULL_LEVEL}
                onChange={(level) => sendCommand({ type: 'setChannelFader', deck: deckId, level })}
              />
              <span className="hw-caption">CH {deckId}</span>
            </div>
          )
        })}
      </div>
      <label className="mixer-master">
        <span className="hw-caption">MASTER</span>
        <input
          className="mixer-master-slider"
          type="range"
          min={0}
          max={FULL_LEVEL}
          step={MASTER_VOLUME_STEP}
          value={masterVolume}
          aria-label="Master volume"
          onChange={(event) => setMasterVolume(Number(event.target.value))}
        />
      </label>
    </section>
  )
}
