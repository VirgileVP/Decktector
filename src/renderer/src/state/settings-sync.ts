import { invoke, isTauri } from '@tauri-apps/api/core'

import { applyTheme } from '../lib/theme'
import type { RemoteConfig, TimecodeConfig } from '../model'
import { useSettings, type Settings } from './settings'

/**
 * Settings the renderer cannot honour by itself; the Rust side owns the
 * window and the sleep inhibitor.
 */
function pushWindowPrefs(settings: Settings): void {
  invoke('set_window_prefs', {
    prefs: {
      alwaysOnTop: settings.alwaysOnTop,
      preventSleep: settings.preventSleep
    }
  }).catch((error) => console.error('Failed to apply window preferences:', error))
}

/**
 * Publishes the UI scale to the stylesheet, which zooms the app shell with it.
 *
 * The webview's own `setZoom` would do the same job, but only inside the app
 * shell and only as far as the platform honours it; a custom property applies
 * everywhere the renderer runs and can be checked in the DOM.
 */
function pushUiScale(uiScale: number): void {
  document.documentElement.style.setProperty('--ui-scale', String(uiScale))
}

/** Builds the tagged config the Rust side expects from the flat settings fields. */
function toTimecodeConfig(settings: Settings): TimecodeConfig {
  const protocol: TimecodeConfig['protocol'] =
    settings.timecodeProtocol === 'artnet'
      ? {
          type: 'artNet',
          interface_name: settings.timecodeArtNetInterface,
          port: settings.timecodeArtNetPort
        }
      : settings.timecodeProtocol === 'mtc'
        ? { type: 'mtc', port_name: settings.timecodeMtcPort }
        : settings.timecodeProtocol === 'ltc'
          ? { type: 'ltc', device_name: settings.timecodeLtcDevice }
          : { type: 'off' }
  return { protocol, frameRate: settings.timecodeFrameRate }
}

function pushTimecodeConfig(settings: Settings): void {
  invoke('set_timecode_config', { config: toTimecodeConfig(settings) }).catch((error) =>
    console.error('Failed to apply the timecode config:', error)
  )
}

/** Builds the config the Rust side expects from the flat settings fields. */
function toRemoteConfig(settings: Settings): RemoteConfig {
  return {
    enabled: settings.remoteEnabled,
    port: settings.remotePort,
    interface: settings.remoteInterface,
    useTls: settings.remoteUseTls,
    maxClients: settings.remoteMaxClients
  }
}

function pushRemoteConfig(settings: Settings): void {
  invoke('set_remote_config', { config: toRemoteConfig(settings) }).catch((error) =>
    console.error('Failed to apply the remote view config:', error)
  )
}

/**
 * Which window `connectSettingsToApp` is running in. A panel window mirrors
 * one piece of the UI and must not re-apply — or fight the main window
 * over — configuration that is process-wide (the timecode generator, the
 * remote server) or owned per-window instead (`uiScale`, the window prefs;
 * see `panel-window-options.ts`). `'panel'` therefore applies only the
 * theme and `dimIdleDecks`, the two settings that are simple DOM state with
 * no ownership conflict.
 */
export type SettingsScope = 'main' | 'panel'

/**
 * Applies whatever changed. `previous` is absent on the initial pass, where
 * everything has to be pushed regardless of its value.
 */
function apply(scope: SettingsScope, settings: Settings, previous?: Settings): void {
  if (previous?.theme !== settings.theme) {
    applyTheme(settings.theme)
  }
  if (previous?.dimIdleDecks !== settings.dimIdleDecks) {
    document.documentElement.dataset.dimIdle = settings.dimIdleDecks ? 'on' : 'off'
  }

  if (scope === 'panel') return

  if (previous?.uiScale !== settings.uiScale) {
    pushUiScale(settings.uiScale)
  }

  // Everything below needs the app shell, which reaching for throws outright
  // when the renderer is served on its own. The theme, the idle dimming and
  // the UI scale are pure DOM and still apply there.
  if (!isTauri()) return

  if (
    previous?.alwaysOnTop !== settings.alwaysOnTop ||
    previous?.preventSleep !== settings.preventSleep
  ) {
    pushWindowPrefs(settings)
  }
  if (
    previous?.timecodeProtocol !== settings.timecodeProtocol ||
    previous?.timecodeFrameRate !== settings.timecodeFrameRate ||
    previous?.timecodeArtNetInterface !== settings.timecodeArtNetInterface ||
    previous?.timecodeArtNetPort !== settings.timecodeArtNetPort ||
    previous?.timecodeMtcPort !== settings.timecodeMtcPort ||
    previous?.timecodeLtcDevice !== settings.timecodeLtcDevice
  ) {
    pushTimecodeConfig(settings)
  }
  if (
    previous?.remoteEnabled !== settings.remoteEnabled ||
    previous?.remotePort !== settings.remotePort ||
    previous?.remoteInterface !== settings.remoteInterface ||
    previous?.remoteUseTls !== settings.remoteUseTls ||
    previous?.remoteMaxClients !== settings.remoteMaxClients
  ) {
    pushRemoteConfig(settings)
  }
}

/**
 * Applies the stored settings and keeps them applied as they change.
 * Returns the teardown function.
 *
 * `scope` distinguishes the main window, which owns every process-wide
 * side effect, from a panel window, which must only mirror the theme and
 * `dimIdleDecks` — see `SettingsScope`.
 */
export function connectSettingsToApp(scope: SettingsScope): () => void {
  apply(scope, useSettings.getState())
  return useSettings.subscribe((settings, previous) => apply(scope, settings, previous))
}
