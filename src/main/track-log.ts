import { app } from 'electron'
import { is } from '@electron-toolkit/utils'
import { mkdirSync } from 'fs'
import { appendFile } from 'fs/promises'
import { join } from 'path'

import type { ProlinkEngine } from '../engine/engine'
import type { DeckId, TrackInfo } from '../engine/types'

/**
 * Where the play history lives: an explicit DECKTECTIVE_LOG_DIR wins,
 * dev runs log into ./logs next to the project for easy access, and
 * packaged builds use the app's user-data directory.
 */
function resolveLogDir(): string {
  if (process.env.DECKTECTIVE_LOG_DIR) return process.env.DECKTECTIVE_LOG_DIR
  return is.dev ? join(process.cwd(), 'logs') : app.getPath('userData')
}

/**
 * Silent play history: appends one line per track to a log file, the
 * first time a deck actually plays it.
 * Line format: ISO timestamp, deck, artist — title, BPM.
 */
export function attachTrackLogger(engine: ProlinkEngine): string {
  const logDir = resolveLogDir()
  mkdirSync(logDir, { recursive: true })
  const logPath = join(logDir, 'played-tracks.log')
  /** Tracks loaded on a deck but not yet logged as played. */
  const pending = new Map<DeckId, TrackInfo>()

  engine.on('deck:track', (deckId, track) => {
    if (track) pending.set(deckId, track)
    else pending.delete(deckId)
  })

  engine.on('deck:state', (state) => {
    if (state.playState !== 'playing') return
    const track = pending.get(state.deckId)
    if (!track) return
    pending.delete(state.deckId)

    const artist = track.artist ?? 'Unknown artist'
    const bpm = track.bpm !== null ? `${track.bpm.toFixed(1)} BPM` : 'BPM ?'
    const line = `${new Date().toISOString()}\tdeck ${state.deckId}\t${artist} — ${track.title}\t${bpm}\n`
    appendFile(logPath, line, 'utf8').catch((error) => {
      console.error('Failed to write played-tracks log:', error)
    })
  })

  return logPath
}
