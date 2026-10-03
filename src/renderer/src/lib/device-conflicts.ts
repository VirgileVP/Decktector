import type { DeviceKind, DiscoveredDevice } from '../model'

/** One device number announced by several devices on the network. */
export interface DeviceNumberConflict {
  deviceNumber: number
  kind: DeviceKind
  /** How many devices announce `deviceNumber`. */
  holders: number
}

/**
 * The device numbers more than one device announces, ascending.
 *
 * Detection itself lives in the engine, which stamps every device with
 * `numberHolders` — this only groups the already-flagged devices so the UI
 * can show one warning per number rather than one per device.
 */
export function findNumberConflicts(devices: readonly DiscoveredDevice[]): DeviceNumberConflict[] {
  const conflicts = new Map<number, DeviceNumberConflict>()
  for (const device of devices) {
    if (device.numberHolders > 1 && !conflicts.has(device.deviceNumber)) {
      conflicts.set(device.deviceNumber, {
        deviceNumber: device.deviceNumber,
        kind: device.kind,
        holders: device.numberHolders
      })
    }
  }
  return [...conflicts.values()].sort((a, b) => a.deviceNumber - b.deviceNumber)
}

/** The warning shown for a conflict, naming what it breaks where that is known. */
export function describeNumberConflict({
  deviceNumber,
  kind,
  holders
}: DeviceNumberConflict): string {
  switch (kind) {
    case 'mixer':
      return `${holders} mixers announce device ${deviceNumber} — on-air info from the CDJs is unreliable`
    case 'player':
      return `${holders} players share number ${deviceNumber} — only one of them can be shown as a deck`
    case 'other':
      return `${holders} devices share number ${deviceNumber}`
  }
}
