import { describe, expect, it } from 'vitest'

import { parseServerMessage } from './parse-message'

const RIG = { decks: [], channelFaders: [], library: [], fallbackWarning: null, reportedAt: 1 }
const TRACK = { title: 'T', durationMs: 1000, cues: [] }

describe('parseServerMessage', () => {
  it('accepts a state message', () => {
    expect(parseServerMessage(JSON.stringify({ type: 'state', state: RIG }))).toEqual({
      type: 'state',
      state: RIG
    })
  })

  it('accepts a track message and an ejected deck', () => {
    const loaded = parseServerMessage(JSON.stringify({ type: 'track', deckId: 2, track: TRACK }))
    expect(loaded?.type).toBe('track')
    expect(parseServerMessage(JSON.stringify({ type: 'track', deckId: 2, track: null }))).toEqual({
      type: 'track',
      deckId: 2,
      track: null,
      audioUrl: null
    })
  })

  it('carries the audio url of a track, or null when it has none', () => {
    const withAudio = { type: 'track', deckId: 1, track: TRACK, audioUrl: '/audio/12' }
    expect(parseServerMessage(JSON.stringify(withAudio))).toMatchObject({ audioUrl: '/audio/12' })
    const withoutAudio = { ...withAudio, audioUrl: null }
    expect(parseServerMessage(JSON.stringify(withoutAudio))).toMatchObject({ audioUrl: null })
    expect(parseServerMessage(JSON.stringify({ ...withAudio, audioUrl: 5 }))).toMatchObject({
      audioUrl: null
    })
  })

  it.each([
    ['not JSON', 'nope'],
    ['not an object', '[1]'],
    ['unknown type', JSON.stringify({ type: 'other' })],
    ['state without decks', JSON.stringify({ type: 'state', state: {} })],
    ['track without a deck id', JSON.stringify({ type: 'track', track: null })],
    ['malformed track', JSON.stringify({ type: 'track', deckId: 1, track: { title: 3 } })]
  ])('rejects %s', (_label, raw) => {
    expect(parseServerMessage(raw)).toBeNull()
  })
})
