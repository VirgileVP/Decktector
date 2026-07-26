import { applyTheme } from '../lib/theme'
import { useSettings, type Settings } from './settings'

/**
 * Settings the renderer cannot honour by itself; the main process owns the
 * window and the power-save blocker.
 */
function pushWindowPrefs(settings: Settings): void {
  const api = window.decktective
  if (!api) return
  api
    .setWindowPrefs({
      alwaysOnTop: settings.alwaysOnTop,
      preventSleep: settings.preventSleep
    })
    .catch((error) => console.error('Failed to apply window preferences:', error))
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
  if (previous?.uiScale !== settings.uiScale) {
    // Chromium's own zoom rather than a CSS transform: it reflows the layout
    // instead of scaling a fixed-size picture.
    window.electron?.webFrame.setZoomFactor(settings.uiScale)
  }
  if (
    previous?.alwaysOnTop !== settings.alwaysOnTop ||
    previous?.preventSleep !== settings.preventSleep
  ) {
    pushWindowPrefs(settings)
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
