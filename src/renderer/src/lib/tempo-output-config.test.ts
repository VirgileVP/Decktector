import { describe, expect, it } from 'vitest'

import { createMessage } from './tempo-messages'
import {
  createTarget,
  omitLegacyTempoSettings,
  parseTempoOutputEnabled,
  sanitizeOscMessages,
  sanitizeOscTargets
} from './tempo-output-config'

describe('createTarget', () => {
  it('builds an enabled local target on port 8000', () => {
    expect(createTarget('a', [])).toEqual({
      id: 'a',
      name: 'Target 1',
      host: '127.0.0.1',
      port: 8000,
      enabled: true
    })
  })

  it('numbers after the existing targets and skips names already taken', () => {
    const first = createTarget('a', [])
    expect(createTarget('b', [first]).name).toBe('Target 2')
    expect(createTarget('c', [{ ...first, name: 'Target 2' }]).name).toBe('Target 3')
  })
})

describe('sanitizeOscTargets', () => {
  const target = createTarget('a', [])

  it('keeps well-shaped entries, wrong values included', () => {
    const typing = { ...target, id: 'b', host: 'typing', port: 0 }
    expect(sanitizeOscTargets([target, typing])).toEqual([target, typing])
  })

  it('drops old-shape and malformed entries and non-lists', () => {
    expect(
      sanitizeOscTargets([{ host: '1.1.1.1', port: 9 }, { ...target, enabled: 'yes' }, null, 'x'])
    ).toEqual([])
    for (const raw of [undefined, null, {}, 'x', 3]) {
      expect(sanitizeOscTargets(raw)).toEqual([])
    }
  })

  it('keeps the first of two entries sharing an id', () => {
    expect(sanitizeOscTargets([target, { ...target, name: 'dup' }])).toEqual([target])
  })
})

describe('sanitizeOscMessages', () => {
  const message = { ...createMessage('m', 'custom'), targetIds: ['a'] }

  it('keeps every type of message', () => {
    const messages = (['bpm', 'barStart', 'eachBeat', 'custom'] as const).map((type) =>
      createMessage(type, type)
    )
    expect(sanitizeOscMessages(messages)).toEqual(messages)
  })

  it('keeps every custom value', () => {
    const values = [
      { type: 'floatRaw' },
      { type: 'floatNormalized', min: 0, max: 1 },
      { type: 'int' },
      { type: 'template', template: '{bpm}' },
      { type: 'noArgument' }
    ]
    for (const value of values) {
      const kind = { ...message.kind, value }
      expect(sanitizeOscMessages([{ ...message, kind }])).toEqual([{ ...message, kind }])
    }
  })

  it('drops messages that are not exactly the current shape', () => {
    const broken = [
      { ...message, source: 'master' },
      { ...message, source: { type: 'deck' } },
      { ...message, kind: { type: 'bpm' } },
      { ...message, kind: { ...message.kind, value: { type: 'floatNormalized', min: 0 } } },
      { ...message, kind: { ...message.kind, trigger: 'often' } },
      { ...message, targetIds: [1] },
      { ...message, enabled: 1 },
      { id: 'old', address: '/x' },
      null
    ]
    expect(sanitizeOscMessages(broken)).toEqual([])
    for (const raw of [undefined, null, {}, 'x', 3]) {
      expect(sanitizeOscMessages(raw)).toEqual([])
    }
  })

  it('removes repeated target ids and repeated message ids', () => {
    const repeated = { ...message, targetIds: ['a', 'a', 'b'] }
    expect(sanitizeOscMessages([repeated, repeated])).toEqual([
      { ...message, targetIds: ['a', 'b'] }
    ])
  })
})

describe('parseTempoOutputEnabled', () => {
  it('keeps a stored boolean and defaults anything else to off', () => {
    expect(parseTempoOutputEnabled(true)).toBe(true)
    expect(parseTempoOutputEnabled(false)).toBe(false)
    for (const raw of ['true', 1, null, undefined]) {
      expect(parseTempoOutputEnabled(raw)).toBe(false)
    }
  })
})

describe('omitLegacyTempoSettings', () => {
  it('drops the keys of the previous output and keeps the rest', () => {
    const stored = {
      theme: 'dark',
      tempoOutputAddressPrefix: '/x',
      tempoOutputScope: 'master',
      tempoOutputSendBeat: true
    }
    expect(omitLegacyTempoSettings(stored)).toEqual({ theme: 'dark' })
  })
})
