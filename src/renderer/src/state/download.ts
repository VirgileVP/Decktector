import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'

import type { DeckId } from '../model'

export interface DownloadProgress {
  deckId: DeckId
  bytesDownloaded: number
  totalBytes: number
  percent: number
}

export interface DownloadResult {
  deckId: DeckId
  filePath: string
  filename: string
}

export type DownloadState =
  | { status: 'idle' }
  | { status: 'downloading'; percent: number }
  | { status: 'success'; filename: string; filePath: string }
  | { status: 'error'; message: string }

interface TrackDownloadStore {
  downloads: Partial<Record<DeckId, DownloadState>>
  startDownload: (deckId: DeckId) => Promise<void>
}

export const useTrackDownloadStore = create<TrackDownloadStore>((set, get) => ({
  downloads: {},

  startDownload: async (deckId: DeckId) => {
    if (!isTauri()) return
    const current = get().downloads[deckId]
    if (current && current.status === 'downloading') return

    set((state) => ({
      downloads: {
        ...state.downloads,
        [deckId]: { status: 'downloading', percent: 0 }
      }
    }))

    try {
      const result = await invoke<DownloadResult>('download_deck_track', { deckId })
      set((state) => ({
        downloads: {
          ...state.downloads,
          [deckId]: {
            status: 'success',
            filename: result.filename,
            filePath: result.filePath
          }
        }
      }))
      setTimeout(() => {
        set((state) => ({
          downloads: {
            ...state.downloads,
            [deckId]: { status: 'idle' }
          }
        }))
      }, 4000)
    } catch (err) {
      const message =
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Download failed'
      set((state) => ({
        downloads: {
          ...state.downloads,
          [deckId]: {
            status: 'error',
            message
          }
        }
      }))
      setTimeout(() => {
        set((state) => ({
          downloads: {
            ...state.downloads,
            [deckId]: { status: 'idle' }
          }
        }))
      }, 5000)
    }
  }
}))

export function connectDownloadToEngine(): () => void {
  if (!isTauri()) {
    return () => {}
  }

  const unlistenPromise = listen<DownloadProgress>('download:progress', ({ payload }) => {
    const { deckId, percent } = payload
    useTrackDownloadStore.setState((state) => {
      const existing = state.downloads[deckId]
      if (existing && existing.status === 'downloading') {
        return {
          downloads: {
            ...state.downloads,
            [deckId]: { status: 'downloading', percent }
          }
        }
      }
      return state
    })
  })

  return () => {
    unlistenPromise.then((unlisten) => unlisten())
  }
}
