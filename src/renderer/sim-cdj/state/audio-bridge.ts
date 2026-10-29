import { AudioEngine } from '../audio/audio-engine'
import { createBufferLoader } from '../audio/buffer-loader'
import { createDeckAudioSync, type DeckAudioSync } from '../audio/deck-audio-sync'
import { setDeckAudioStatus, setSoundState, useAudioStore } from './audio-store'
import { useSimStore } from './sim-store'

/**
 * Wires the audio engine to the stores: the rig makes the decks play, track
 * messages load their audio, the master volume drives the master gain. The
 * engine (and so the AudioContext) is only created on the first pointer
 * gesture, which is also what lets the browser start audio. Returns the teardown.
 */
export function startAudioBridge(): () => void {
  let engine: AudioEngine | null = null
  let deckSync: DeckAudioSync | null = null

  const reportContextState = (state: AudioContextState): void => {
    setSoundState(state === 'running' ? 'running' : 'suspended')
  }

  const followRig = (): void => {
    const { rig, status } = useSimStore.getState()
    if (engine === null) return
    // Without the server nothing says where the decks are: stay silent until the next state.
    if (rig === null || status !== 'connected') engine.silenceAll()
    else engine.follow(rig, Date.now())
  }

  const syncAllDecks = (): void => {
    const { audioUrls } = useSimStore.getState()
    for (const [deckId, audioUrl] of Object.entries(audioUrls)) {
      deckSync?.sync(Number(deckId), audioUrl)
    }
  }

  const createEngine = (): void => {
    let created: AudioEngine
    try {
      created = new AudioEngine(useAudioStore.getState().masterVolume, reportContextState)
    } catch (error) {
      console.warn('Web Audio is not available, the decks will stay silent.', error)
      setSoundState('unavailable')
      return
    }
    engine = created
    deckSync = createDeckAudioSync(
      created,
      createBufferLoader((data) => created.decode(data)),
      setDeckAudioStatus
    )
    syncAllDecks()
    followRig()
  }

  const onPointerDown = (): void => {
    if (useAudioStore.getState().sound === 'unavailable') return
    if (engine === null) createEngine()
    engine?.resume().then(
      () => reportContextState(engine?.state ?? 'suspended'),
      (error: unknown) => console.warn('The browser refused to start audio.', error)
    )
  }
  window.addEventListener('pointerdown', onPointerDown, { capture: true })

  const unsubscribeSim = useSimStore.subscribe((state, previous) => {
    if (state.audioUrls !== previous.audioUrls) syncAllDecks()
    if (state.rig !== previous.rig || state.status !== previous.status) followRig()
  })
  const unsubscribeAudio = useAudioStore.subscribe((state, previous) => {
    if (state.masterVolume !== previous.masterVolume) engine?.setMasterVolume(state.masterVolume)
  })

  return () => {
    window.removeEventListener('pointerdown', onPointerDown, { capture: true })
    unsubscribeSim()
    unsubscribeAudio()
    engine?.silenceAll()
  }
}
