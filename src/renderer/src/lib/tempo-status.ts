import type { TempoOutputConfig, TempoOutputStatus } from '../model'

const BPM_DECIMALS = 1

/** Line saying what the output is doing, `idle` when it is simply off or waiting. */
export function describeTempoStatus(status: TempoOutputStatus, config: TempoOutputConfig): string {
  if (!config.enabled) return 'Off'
  if (status.running) {
    if (status.masterDeckId === null) return 'Sending — no tempo master'
    const bpm = status.masterBpm === null ? '' : ` · ${status.masterBpm.toFixed(BPM_DECIMALS)} BPM`
    return `Sending — master is player ${status.masterDeckId}${bpm}`
  }
  if (!config.messages.some((message) => message.enabled)) {
    return 'Not sending — no message is enabled'
  }
  if (!config.targets.some((target) => target.enabled)) {
    return 'Not sending — no target is enabled'
  }
  return 'Not sending — no enabled message has a usable target'
}
