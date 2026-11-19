import { describe, expect, it } from 'vitest'

import { createMessage } from '../lib/tempo-messages'
import { createTarget } from '../lib/tempo-output-config'
import { DEFAULT_SETTINGS, mergePersistedSettings } from './settings'

describe('tempo output settings', () => {
  it('defaults to disabled, with no target and no message', () => {
    expect(DEFAULT_SETTINGS.tempoOutputEnabled).toBe(false)
    expect(DEFAULT_SETTINGS.tempoOutputTargets).toEqual([])
    expect(DEFAULT_SETTINGS.tempoOutputMessages).toEqual([])
  })

  it('restores what was stored', () => {
    const target = createTarget('t', [])
    const message = { ...createMessage('m', 'bpm'), targetIds: ['t'] }
    const merged = mergePersistedSettings(
      { tempoOutputEnabled: true, tempoOutputTargets: [target], tempoOutputMessages: [message] },
      DEFAULT_SETTINGS
    )
    expect(merged.tempoOutputEnabled).toBe(true)
    expect(merged.tempoOutputTargets).toEqual([target])
    expect(merged.tempoOutputMessages).toEqual([message])
  })

  it('discards the old shape without migrating it', () => {
    const merged = mergePersistedSettings(
      {
        tempoOutputEnabled: true,
        tempoOutputTargets: [{ host: '192.168.1.255', port: 8000 }],
        tempoOutputAddressPrefix: '/show',
        tempoOutputScope: 'master',
        tempoOutputSendBeat: false
      },
      DEFAULT_SETTINGS
    )
    expect(merged.tempoOutputEnabled).toBe(true)
    expect(merged.tempoOutputTargets).toEqual([])
    expect(merged.tempoOutputMessages).toEqual([])
    expect(merged).not.toHaveProperty('tempoOutputAddressPrefix')
    expect(merged).not.toHaveProperty('tempoOutputScope')
    expect(merged).not.toHaveProperty('tempoOutputSendBeat')
  })

  it('falls back to the defaults for hand-edited storage', () => {
    const merged = mergePersistedSettings(
      { tempoOutputEnabled: 'yes', tempoOutputTargets: 'nope', tempoOutputMessages: 7 },
      DEFAULT_SETTINGS
    )
    expect(merged.tempoOutputEnabled).toBe(false)
    expect(merged.tempoOutputTargets).toEqual([])
    expect(merged.tempoOutputMessages).toEqual([])
  })
})
