import { describe, expect, it } from 'vitest'

import type { OscMessageConfig, OscTarget, TempoOutputConfig, TempoOutputStatus } from '../model'
import { createMessage } from './tempo-messages'
import { createTarget } from './tempo-output-config'
import { describeTempoStatus } from './tempo-status'

const idle: TempoOutputStatus = {
  running: false,
  masterDeckId: null,
  masterBpm: null,
  targetIssues: [],
  messageIssues: []
}
const target: OscTarget = createTarget('t', [])
const message: OscMessageConfig = createMessage('m', 'bpm')

function config(overrides: Partial<TempoOutputConfig>): TempoOutputConfig {
  return { enabled: true, targets: [target], messages: [message], ...overrides }
}

describe('describeTempoStatus', () => {
  it('says Off while disabled, whatever the backend reports', () => {
    expect(describeTempoStatus({ ...idle, running: true }, config({ enabled: false }))).toBe('Off')
  })

  it('names the master and its BPM while running', () => {
    const status = { ...idle, running: true, masterDeckId: 2, masterBpm: 128.04 }
    expect(describeTempoStatus(status, config({}))).toBe('Sending — master is player 2 · 128.0 BPM')
  })

  it('says so when running with no master or no known BPM', () => {
    expect(describeTempoStatus({ ...idle, running: true }, config({}))).toBe(
      'Sending — no tempo master'
    )
    expect(describeTempoStatus({ ...idle, running: true, masterDeckId: 1 }, config({}))).toBe(
      'Sending — master is player 1'
    )
  })

  it('explains why nothing is sent', () => {
    expect(describeTempoStatus(idle, config({ messages: [] }))).toContain('no message is enabled')
    expect(
      describeTempoStatus(idle, config({ targets: [{ ...target, enabled: false }] }))
    ).toContain('no target is enabled')
    expect(describeTempoStatus(idle, config({}))).toContain('usable target')
  })
})
