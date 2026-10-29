import { describe, expect, it } from 'vitest'

import {
  RECONNECT_INITIAL_DELAY_MS,
  RECONNECT_MAX_DELAY_MS,
  controlSocketUrl,
  reconnectDelayMs
} from './sim-socket'

describe('controlSocketUrl', () => {
  it('follows the page host and scheme', () => {
    expect(controlSocketUrl({ protocol: 'http:', host: '127.0.0.1:1430' })).toBe(
      'ws://127.0.0.1:1430/ws'
    )
    expect(controlSocketUrl({ protocol: 'https:', host: 'sim.local' })).toBe('wss://sim.local/ws')
  })
})

describe('reconnectDelayMs', () => {
  it('backs off from the initial delay up to the cap', () => {
    expect(reconnectDelayMs(0)).toBe(RECONNECT_INITIAL_DELAY_MS)
    expect(reconnectDelayMs(1)).toBe(RECONNECT_INITIAL_DELAY_MS * 2)
    expect(reconnectDelayMs(50)).toBe(RECONNECT_MAX_DELAY_MS)
  })
})
