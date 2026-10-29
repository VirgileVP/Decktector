import { isRecord } from '@renderer/lib/is-record'
import type { SimControlMessage } from '@renderer/model/SimControlMessage'
import type { SimRigState } from '@renderer/model/SimRigState'
import type { TrackInfo } from '@renderer/model/TrackInfo'

/**
 * Narrows a raw WebSocket frame to a server message. The server is our own,
 * so only the envelope and the fields the page indexes into are checked;
 * anything else is rejected as malformed.
 */
export function parseServerMessage(raw: string): SimControlMessage | null {
  let payload: unknown
  try {
    payload = JSON.parse(raw)
  } catch {
    return null
  }
  if (!isRecord(payload)) return null

  if (payload.type === 'state' && isRigState(payload.state)) {
    return { type: 'state', state: payload.state }
  }
  if (payload.type === 'track' && typeof payload.deckId === 'number') {
    // An older server sends no audio URL: that is a track without audio.
    const audioUrl = typeof payload.audioUrl === 'string' ? payload.audioUrl : null
    if (payload.track === null) {
      return { type: 'track', deckId: payload.deckId, track: null, audioUrl }
    }
    if (isTrackInfo(payload.track)) {
      return { type: 'track', deckId: payload.deckId, track: payload.track, audioUrl }
    }
  }
  return null
}

function isRigState(value: unknown): value is SimRigState {
  return (
    isRecord(value) &&
    Array.isArray(value.decks) &&
    Array.isArray(value.channelFaders) &&
    Array.isArray(value.library) &&
    typeof value.reportedAt === 'number'
  )
}

function isTrackInfo(value: unknown): value is TrackInfo {
  return (
    isRecord(value) &&
    typeof value.title === 'string' &&
    typeof value.durationMs === 'number' &&
    Array.isArray(value.cues)
  )
}
