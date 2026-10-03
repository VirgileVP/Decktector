import { describe, expect, it } from 'vitest'

import type { DiscoveredDevice, PlayState } from '../model'
import {
  AMBIGUOUS_MIXER_COUNT,
  DEFAULT_ON_AIR_SOURCE,
  ON_AIR_SOURCES,
  countMixers,
  hasAmbiguousMixers,
  isDeckActive,
  parseOnAirSource,
  resolveOnAirSource
} from './deck-activity'

const PLAY_STATES: readonly PlayState[] = [
  'empty',
  'loading',
  'cued',
  'playing',
  'paused',
  'searching',
  'ended'
]

function device(overrides: Partial<DiscoveredDevice>): DiscoveredDevice {
  return {
    name: 'DJM-900NXS2',
    deviceNumber: 33,
    kind: 'mixer',
    ip: '169.254.1.33',
    macAddress: 'aa:bb:cc:00:00:33',
    peerCount: 3,
    numberHolders: 1,
    ...overrides
  }
}

describe('parseOnAirSource', () => {
  it('keeps every known source', () => {
    for (const source of ON_AIR_SOURCES) expect(parseOnAirSource(source)).toBe(source)
  })

  it('falls back to auto for anything else', () => {
    expect(DEFAULT_ON_AIR_SOURCE).toBe('auto')
    for (const raw of ['MIXER', '', 'djm', 3, null, undefined, {}, ['mixer']]) {
      expect(parseOnAirSource(raw)).toBe('auto')
    }
  })
})

describe('countMixers', () => {
  it('counts nothing on an empty network', () => {
    expect(countMixers([])).toBe(0)
  })

  it('ignores players and other gear', () => {
    const devices = [
      device({ kind: 'player', deviceNumber: 1, macAddress: 'aa:00:00:00:00:01' }),
      device({ kind: 'other', deviceNumber: 5, macAddress: 'aa:00:00:00:00:05' })
    ]
    expect(countMixers(devices)).toBe(0)
  })

  it('counts one mixer announced several times only once', () => {
    expect(countMixers([device({}), device({ ip: '169.254.1.34' }), device({})])).toBe(1)
  })

  it('compares MAC addresses without regard to case', () => {
    const devices = [device({ macAddress: 'AA:BB:CC:00:00:33' }), device({})]
    expect(countMixers(devices)).toBe(1)
  })

  it('counts two mixers announcing the same device number by MAC', () => {
    const devices = [device({}), device({ macAddress: 'aa:bb:cc:00:00:99', ip: '169.254.1.99' })]
    expect(countMixers(devices)).toBe(2)
  })

  it('falls back to the IP when a mixer has no MAC address', () => {
    const devices = [
      device({ macAddress: '', ip: '169.254.1.10' }),
      device({ macAddress: '', ip: '169.254.1.11' })
    ]
    expect(countMixers(devices)).toBe(2)
  })
})

describe('resolveOnAirSource', () => {
  it('auto follows the mixer only when exactly one is present', () => {
    expect(resolveOnAirSource('auto', 0)).toBe('playback')
    expect(resolveOnAirSource('auto', 1)).toBe('mixer')
    expect(resolveOnAirSource('auto', AMBIGUOUS_MIXER_COUNT)).toBe('playback')
    expect(resolveOnAirSource('auto', 5)).toBe('playback')
  })

  it('an explicit choice ignores the mixer count', () => {
    for (const count of [0, 1, 2, 3]) {
      expect(resolveOnAirSource('mixer', count)).toBe('mixer')
      expect(resolveOnAirSource('playback', count)).toBe('playback')
    }
  })
})

describe('hasAmbiguousMixers', () => {
  it('warns only in auto, from two mixers up', () => {
    expect(hasAmbiguousMixers('auto', 0)).toBe(false)
    expect(hasAmbiguousMixers('auto', 1)).toBe(false)
    expect(hasAmbiguousMixers('auto', 2)).toBe(true)
    expect(hasAmbiguousMixers('auto', 3)).toBe(true)
    expect(hasAmbiguousMixers('mixer', 2)).toBe(false)
    expect(hasAmbiguousMixers('playback', 2)).toBe(false)
  })
})

describe('isDeckActive', () => {
  it('never activates a deck the engine has not reported', () => {
    for (const source of ON_AIR_SOURCES) expect(isDeckActive(source, undefined, 1)).toBe(false)
  })

  it('mixer mode trusts the CDJ flag whatever the play state', () => {
    for (const playState of PLAY_STATES) {
      expect(isDeckActive('mixer', { playState, isOnAir: true }, 0)).toBe(true)
      expect(isDeckActive('mixer', { playState, isOnAir: false }, 0)).toBe(false)
    }
  })

  it('playback mode counts only a playing deck, ignoring the CDJ flag', () => {
    for (const playState of PLAY_STATES) {
      const expected = playState === 'playing'
      expect(isDeckActive('playback', { playState, isOnAir: false }, 1)).toBe(expected)
      expect(isDeckActive('playback', { playState, isOnAir: true }, 1)).toBe(expected)
    }
  })

  it('auto without a mixer follows playback', () => {
    expect(isDeckActive('auto', { playState: 'playing', isOnAir: false }, 0)).toBe(true)
    expect(isDeckActive('auto', { playState: 'paused', isOnAir: true }, 0)).toBe(false)
  })

  it('auto with one mixer follows the CDJ flag', () => {
    expect(isDeckActive('auto', { playState: 'playing', isOnAir: false }, 1)).toBe(false)
    expect(isDeckActive('auto', { playState: 'cued', isOnAir: true }, 1)).toBe(true)
  })

  it('auto with several mixers follows playback', () => {
    expect(isDeckActive('auto', { playState: 'playing', isOnAir: false }, 2)).toBe(true)
    expect(isDeckActive('auto', { playState: 'cued', isOnAir: true }, 2)).toBe(false)
  })
})
