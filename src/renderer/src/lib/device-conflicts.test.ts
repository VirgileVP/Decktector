import { describe, expect, it } from 'vitest'

import type { DiscoveredDevice } from '../model'
import { describeNumberConflict, findNumberConflicts } from './device-conflicts'

function device(overrides: Partial<DiscoveredDevice>): DiscoveredDevice {
  return {
    name: 'CDJ-3000',
    deviceNumber: 1,
    kind: 'player',
    ip: '192.168.1.10',
    macAddress: 'aa:bb:cc:dd:ee:01',
    peerCount: 1,
    numberHolders: 1,
    ...overrides
  }
}

describe('findNumberConflicts', () => {
  it('finds nothing on a correctly numbered network', () => {
    const devices = [device({ deviceNumber: 1 }), device({ deviceNumber: 33, kind: 'mixer' })]
    expect(findNumberConflicts(devices)).toEqual([])
  })

  it('reports two mixers on 33 once, not once per mixer', () => {
    const devices = [
      device({ deviceNumber: 33, kind: 'mixer', macAddress: 'a', numberHolders: 2 }),
      device({ deviceNumber: 33, kind: 'mixer', macAddress: 'b', numberHolders: 2 })
    ]
    expect(findNumberConflicts(devices)).toEqual([{ deviceNumber: 33, kind: 'mixer', holders: 2 }])
  })

  it('lists several conflicts in ascending number order', () => {
    const devices = [
      device({ deviceNumber: 33, kind: 'mixer', numberHolders: 2 }),
      device({ deviceNumber: 2, numberHolders: 2 })
    ]
    expect(findNumberConflicts(devices).map((conflict) => conflict.deviceNumber)).toEqual([2, 33])
  })
})

describe('describeNumberConflict', () => {
  it('says what two mixers on one number break', () => {
    expect(describeNumberConflict({ deviceNumber: 33, kind: 'mixer', holders: 2 })).toBe(
      '2 mixers announce device 33 — on-air info from the CDJs is unreliable'
    )
  })

  it('says players share a number', () => {
    expect(describeNumberConflict({ deviceNumber: 2, kind: 'player', holders: 2 })).toContain(
      '2 players share number 2'
    )
  })

  it('falls back to a generic message for other devices', () => {
    expect(describeNumberConflict({ deviceNumber: 9, kind: 'other', holders: 3 })).toBe(
      '3 devices share number 9'
    )
  })
})
