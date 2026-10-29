import type { DeckId } from '@renderer/model'

import type { BufferLoader } from './buffer-loader'

/** `none`: the track has no audio file; `error`: it has one that could not be loaded. */
export type DeckAudioStatus = 'none' | 'loading' | 'ready' | 'error'

export interface DeckAudioSync {
  /** Points a deck at a track's audio (null: nothing to play); stale loads are dropped. */
  sync: (deckId: DeckId, audioUrl: string | null) => void
}

interface DeckBufferSink {
  setDeckBuffer: (deckId: DeckId, buffer: AudioBuffer | null) => void
}

/**
 * Keeps each deck's buffer in step with the URL it should play. A load that
 * finishes after the deck moved on to another URL is ignored.
 */
export function createDeckAudioSync(
  sink: DeckBufferSink,
  loader: BufferLoader,
  reportStatus: (deckId: DeckId, status: DeckAudioStatus) => void
): DeckAudioSync {
  const requestedUrls = new Map<DeckId, string | null>()

  return {
    sync: (deckId, audioUrl) => {
      if (requestedUrls.get(deckId) === audioUrl && requestedUrls.has(deckId)) return
      requestedUrls.set(deckId, audioUrl)
      sink.setDeckBuffer(deckId, null)

      if (audioUrl === null) {
        reportStatus(deckId, 'none')
        return
      }
      reportStatus(deckId, 'loading')
      loader.load(audioUrl).then(
        (buffer) => {
          if (requestedUrls.get(deckId) !== audioUrl) return
          sink.setDeckBuffer(deckId, buffer)
          reportStatus(deckId, 'ready')
        },
        (error: unknown) => {
          if (requestedUrls.get(deckId) !== audioUrl) return
          console.warn(`Could not load the audio of deck ${deckId} from ${audioUrl}.`, error)
          reportStatus(deckId, 'error')
        }
      )
    }
  }
}
