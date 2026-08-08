import { invoke, isTauri } from '@tauri-apps/api/core'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'

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
 * Applies the UI scale through the webview's own zoom rather than a CSS
 * transform: it reflows the layout instead of scaling a fixed-size picture.
 */
function pushUiScale(uiScale: number): void {
  getCurrentWebviewWindow()
    .setZoom(uiScale)
    .catch((error) => console.error('Failed to apply the UI scale:', error))
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
    interface: settings.remoteInterface
  }
}

function pushRemoteConfig(settings: Settings): void {
  invoke('set_remote_config', { config: toRemoteConfig(settings) }).catch((error) =>
    console.error('Failed to apply the remote view config:', error)
  )
}

/**
 * Applies whatever changed. `previous` is absent on the initial pass, where
 * everything has to be pushed regardless of its value.
 */
function apply(settings: Settings, previous?: Settings): void {
  if (previous?.theme !== settings.theme) {
    applyTheme(settings.theme)
  }
  if (previous?.dimIdleDecks !== settings.dimIdleDecks) {
    document.documentElement.dataset.dimIdle = settings.dimIdleDecks ? 'on' : 'off'
  }

  // Everything below needs the app shell, which reaching for throws outright
  // when the renderer is served on its own. The theme and the idle dimming
  // are pure DOM and still apply there.
  if (!isTauri()) return

  if (previous?.uiScale !== settings.uiScale) {
    pushUiScale(settings.uiScale)
  }
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
    previous?.remoteInterface !== settings.remoteInterface
  ) {
    pushRemoteConfig(settings)
  }
}

/**
 * Applies the stored settings and keeps them applied as they change.
 * Returns the teardown function.
 */
export function connectSettingsToApp(): () => void {
  apply(useSettings.getState())
  return useSettings.subscribe(apply)
}
