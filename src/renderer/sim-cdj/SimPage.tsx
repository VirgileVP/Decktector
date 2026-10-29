import { useEffect } from 'react'

import { Banners } from './components/Banners'
import { CdjDeck } from './components/CdjDeck'
import { Mixer } from './components/Mixer'
import { TrackBrowser } from './components/TrackBrowser'
import { connectToSim } from './net/sim-connection'
import { startAudioBridge } from './state/audio-bridge'

/** Left to right as on the table: CDJ 1, CDJ 2, mixer, CDJ 3, CDJ 4. */
export function SimPage(): React.JSX.Element {
  useEffect(() => connectToSim(), [])
  useEffect(() => startAudioBridge(), [])

  return (
    <main className="sim-page">
      <Banners />
      <div className="rig">
        <CdjDeck deckId={1} />
        <CdjDeck deckId={2} />
        <Mixer />
        <CdjDeck deckId={3} />
        <CdjDeck deckId={4} />
      </div>
      <TrackBrowser />
    </main>
  )
}
