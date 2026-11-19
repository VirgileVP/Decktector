import { invoke } from '@tauri-apps/api/core'

import type { TempoOutputConfig } from '../model'
import type { Settings } from './settings'

/** Builds the config the Rust side expects from the flat settings fields. */
export function toTempoOutputConfig(settings: Settings): TempoOutputConfig {
  return {
    enabled: settings.tempoOutputEnabled,
    targets: settings.tempoOutputTargets,
    messages: settings.tempoOutputMessages
  }
}

function hasChanged(settings: Settings, previous?: Settings): boolean {
  return (
    previous?.tempoOutputEnabled !== settings.tempoOutputEnabled ||
    previous?.tempoOutputTargets !== settings.tempoOutputTargets ||
    previous?.tempoOutputMessages !== settings.tempoOutputMessages
  )
}

/**
 * Pushes the tempo output settings when they changed. Whatever the form
 * holds is sent: the backend keeps what is usable and reports the rest in the
 * output status, shown next to the offending target or message.
 */
export function syncTempoOutputConfig(settings: Settings, previous?: Settings): void {
  if (!hasChanged(settings, previous)) return

  invoke('set_tempo_output_config', { config: toTempoOutputConfig(settings) }).catch((error) =>
    console.error('Failed to push the tempo output config:', error)
  )
}
