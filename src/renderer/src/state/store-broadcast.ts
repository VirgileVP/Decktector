/**
 * Propagates a persisted store's changes across every window. Stores persist
 * to `localStorage`, which every webview on the origin shares — but each
 * webview has its own JS heap, so a change made in one window does not
 * reach the others until a reload. Tauri's `emit` broadcasts to every
 * webview (see `src-tauri/src/events.rs`), which closes that gap.
 */

import { emit, listen } from '@tauri-apps/api/event'
import { isTauri } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { StoreApi } from 'zustand'

export interface StoreBroadcast<T> {
  /** Label of the window that made the change, so it can ignore its own echo. */
  origin: string
  state: T
}

/** Whether a received broadcast should be applied by the window `self`. */
export function shouldApplyBroadcast(message: StoreBroadcast<unknown>, self: string): boolean {
  return message.origin !== self
}

/**
 * Guards against re-emitting a broadcast this window is only applying, not
 * originating. Pulled out of `bridgeStoreBroadcast` so the guard logic is
 * testable without a Tauri mock: `wrapApply` is a plain function call.
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

/** The cross-window event bus a broadcast runs over — Tauri's in the app, a fake in tests. */
export interface BroadcastTransport {
  /** Label of this window. */
  origin: string
  emit: (channel: string, payload: unknown) => Promise<void>
  /** Resolves, possibly after the caller has already torn down, to the function that stops listening. */
  listen: <T>(channel: string, handler: (payload: T) => void) => Promise<() => void>
}

/**
 * `connectStoreBroadcast` over any transport. Teardown must hold even when
 * it runs before `listen` has resolved — React's StrictMode does exactly
 * that on mount. A listener left behind then would apply other windows'
 * changes outside this connection's guard, so the live subscription would
 * re-emit them, and two windows doing that bounce every change between
 * them forever.
 */
export function bridgeStoreBroadcast<S, T extends Partial<S>>(
  store: StoreApi<S>,
  channel: string,
  pickShared: (state: S) => T,
  transport: BroadcastTransport
): () => void {
  const { origin } = transport
  const guard = createApplyGuard()
  let disposed = false
  let unlisten: (() => void) | null = null

  const unsubscribe = store.subscribe((state) => {
    if (guard.isApplying()) return
    transport
      .emit(channel, { origin, state: pickShared(state) } satisfies StoreBroadcast<T>)
      .catch((error) => console.error(`Failed to broadcast on ${channel}:`, error))
  })

  transport
    .listen<StoreBroadcast<T>>(channel, (payload) => {
      if (disposed || !shouldApplyBroadcast(payload, origin)) return
      guard.wrapApply(() => store.setState(payload.state))
    })
    .then((stopListening) => {
      if (disposed) stopListening()
      else unlisten = stopListening
    })
    .catch((error) => console.error(`Failed to listen on ${channel}:`, error))

  return () => {
    disposed = true
    unsubscribe()
    unlisten?.()
  }
}

function tauriTransport(): BroadcastTransport {
  return {
    origin: getCurrentWindow().label,
    emit: (channel, payload) => emit(channel, payload),
    listen: <T>(channel: string, handler: (payload: T) => void) =>
      listen<T>(channel, (event) => handler(event.payload))
  }
}

/**
 * Subscribes to `store` and emits the slice `pickShared` keeps of every
 * change on `channel`, and applies every slice emitted there by another
 * window. Returns the teardown function. A no-op outside Tauri: the remote
 * browser view has no window to broadcast to.
 */
export function connectStoreBroadcast<S, T extends Partial<S>>(
  store: StoreApi<S>,
  channel: string,
  pickShared: (state: S) => T
): () => void {
  if (!isTauri()) return () => {}
  return bridgeStoreBroadcast(store, channel, pickShared, tauriTransport())
}
