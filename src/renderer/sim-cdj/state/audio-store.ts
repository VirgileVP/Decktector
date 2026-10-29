import { create } from 'zustand'

import type { DeckId } from '@renderer/model'

import { DEFAULT_MASTER_VOLUME } from '../audio/audio-constants'
import type { DeckAudioStatus } from '../audio/deck-audio-sync'

/** `suspended` until the first click lets the browser start audio. */
export type SoundState = 'suspended' | 'running' | 'unavailable'

interface AudioStore {
  sound: SoundState
  masterVolume: number
  deckAudio: Record<DeckId, DeckAudioStatus>
}

export const useAudioStore = create<AudioStore>(() => ({
  sound: 'suspended',
  masterVolume: DEFAULT_MASTER_VOLUME,
  deckAudio: {}
}))

export function setSoundState(sound: SoundState): void {
  useAudioStore.setState({ sound })
}

export function setMasterVolume(masterVolume: number): void {
  useAudioStore.setState({ masterVolume })
}

export function setDeckAudioStatus(deckId: DeckId, status: DeckAudioStatus): void {
  useAudioStore.setState((state) => ({ deckAudio: { ...state.deckAudio, [deckId]: status } }))
}

export function useDeckAudioStatus(deckId: DeckId): DeckAudioStatus {
  return useAudioStore((state) => state.deckAudio[deckId] ?? 'none')
}
