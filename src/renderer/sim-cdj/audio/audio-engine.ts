import type { DeckId } from '@renderer/model'
import type { SimRigState } from '@renderer/model/SimRigState'

import { DECK_COUNT } from '../lib/constants'
import { MASTER_TIME_CONSTANT_SECONDS } from './audio-constants'
import { DeckVoice } from './deck-voice'
import { planPlayback } from './follow-math'

/**
 * The page's whole Web Audio graph: one context, a master gain and a voice per
 * deck. It is driven from the outside (see the audio bridge); nothing here
 * knows about React or the stores.
 */
export class AudioEngine {
  private readonly context: AudioContext
  private readonly master: GainNode
  private readonly voices = new Map<DeckId, DeckVoice>()

  constructor(initialVolume: number, onStateChange: (state: AudioContextState) => void) {
    this.context = new AudioContext()
    this.context.onstatechange = () => onStateChange(this.context.state)
    this.master = this.context.createGain()
    this.master.gain.value = initialVolume
    this.master.connect(this.context.destination)
    for (let deckId = 1; deckId <= DECK_COUNT; deckId += 1) {
      this.voices.set(deckId, new DeckVoice(this.context, this.master))
    }
  }

  get state(): AudioContextState {
    return this.context.state
  }

  /** Needs a user gesture the first time (autoplay policy). */
  resume(): Promise<void> {
    return this.context.resume()
  }

  decode(data: ArrayBuffer): Promise<AudioBuffer> {
    return this.context.decodeAudioData(data)
  }

  setMasterVolume(volume: number): void {
    this.master.gain.setTargetAtTime(volume, this.context.currentTime, MASTER_TIME_CONSTANT_SECONDS)
  }

  setDeckBuffer(deckId: DeckId, buffer: AudioBuffer | null): void {
    this.voices.get(deckId)?.setBuffer(buffer)
  }

  /** Makes every deck follow the rig: faders first, then what each deck plays. */
  follow(rig: SimRigState, nowMs: number): void {
    rig.decks.forEach((view) => {
      const voice = this.voices.get(view.deckId)
      if (voice === undefined) return
      voice.setLevel(rig.channelFaders[view.deckId - 1] ?? 1)
      voice.follow(planPlayback(view, rig.reportedAt, nowMs))
    })
  }

  silenceAll(): void {
    this.voices.forEach((voice) => voice.silence())
  }
}
