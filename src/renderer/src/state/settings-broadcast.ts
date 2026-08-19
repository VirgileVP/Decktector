/**
 * Propagates settings changes across every window. `useSettings` persists
 * to `localStorage`, which every webview on the origin shares — but each
 * webview has its own JS heap, so a change made in one window does not
 * reach the others until a reload. Tauri's `emit` broadcasts to every
 * webview (see `src-tauri/src/events.rs`), which closes that gap.
 */

import { emit, listen } from '@tauri-apps/api/event'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

import { useSettings, type Settings } from './settings'

/** Channel every window emits on and listens to. */
export const SETTINGS_CHANNEL = 'settings:changed'

export interface SettingsBroadcast {
  /** Label of the window that made the change, so it can ignore its own echo. */
  origin: string
  settings: Settings
}

/** Whether a received broadcast should be applied by the window `self`. */
export function shouldApplyBroadcast(message: SettingsBroadcast, self: string): boolean {
  return message.origin !== self
}

/**
 * Guards against re-emitting a broadcast this window is only applying, not
 * originating. Pulled out of `connectSettingsBroadcast` so the guard logic
 * is testable without a Tauri mock: `wrapApply` is a plain function call.
 */
export function createApplyGuard(): {
  isApplying: () => boolean
  wrapApply: (apply: () => void) => void
} {
  let applying = false
  return {
    isApplying: () => applying,
    wrapApply: (apply) => {
      applying = true
      try {
        apply()
      } finally {
        applying = false
      }
    }
  }
}

/**
 * Subscribes to `useSettings` and emits every change, and applies every
 * change emitted by another window. Returns the teardown function. A no-op
 * outside Tauri: the remote browser view has no window to broadcast to.
 */
export function connectSettingsBroadcast(): () => void {
  if (!isTauri()) return () => {}

  const origin = getCurrentWindow().label
  const guard = createApplyGuard()

  const unsubscribe = useSettings.subscribe((settings) => {
    if (guard.isApplying()) return
    void emit(SETTINGS_CHANNEL, { origin, settings } satisfies SettingsBroadcast)
  })

  let unlisten: (() => void) | null = null
  void listen<SettingsBroadcast>(SETTINGS_CHANNEL, (event) => {
    if (!shouldApplyBroadcast(event.payload, origin)) return
    guard.wrapApply(() => useSettings.setState(event.payload.settings))
  }).then((stopListening) => {
    unlisten = stopListening
  })

  return () => {
    unsubscribe()
    unlisten?.()
  }
}
