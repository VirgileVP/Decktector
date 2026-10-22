import { isTauri } from '@tauri-apps/api/core'
import { create } from 'zustand'

import {
  DISMISSED_UPDATE_STORAGE_KEY,
  LATEST_RELEASE_API_URL,
  parseLatestRelease
} from '../lib/update-check'

/** Gives up on a slow GitHub rather than leaving the request open for the whole session. */
const UPDATE_CHECK_TIMEOUT_MS = 10_000

/** Marks an abort caused by unmounting, which is not worth a warning. */
const CLEANUP_REASON = 'update check cancelled'

interface UpdateCheckState {
  /** Version of the latest published release; `null` until checked, or if the check failed. */
  latestVersion: string | null
  /** Newest version the user chose to skip on this device. */
  dismissedVersion: string | null
}

export const useUpdateCheckStore = create<UpdateCheckState>(() => ({
  latestVersion: null,
  dismissedVersion: localStorage.getItem(DISMISSED_UPDATE_STORAGE_KEY)
}))

/** Hides the notice for `version`; a later release is announced again. */
export function dismissUpdate(version: string): void {
  localStorage.setItem(DISMISSED_UPDATE_STORAGE_KEY, version)
  useUpdateCheckStore.setState({ dismissedVersion: version })
}

async function fetchLatestReleaseVersion(signal: AbortSignal): Promise<string | null> {
  const response = await fetch(LATEST_RELEASE_API_URL, {
    headers: { Accept: 'application/vnd.github+json' },
    signal
  })
  if (!response.ok) throw new Error(`GitHub answered ${response.status}`)
  return parseLatestRelease(await response.json())
}

/**
 * Asks GitHub once for the latest release. Desktop shell only: a remote
 * viewer is watching a set, not running an install it could update.
 * Returns a cleanup that cancels a check still in flight.
 */
export function connectUpdateCheck(): () => void {
  if (!isTauri()) return () => {}

  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), UPDATE_CHECK_TIMEOUT_MS)

  fetchLatestReleaseVersion(controller.signal)
    .then((latestVersion) => useUpdateCheckStore.setState({ latestVersion }))
    .catch((error: unknown) => {
      // Offline booths are the norm, so a failed check is logged, not shown.
      if (controller.signal.reason !== CLEANUP_REASON) {
        console.warn('Update check failed:', error)
      }
    })
    .finally(() => clearTimeout(timeout))

  return () => controller.abort(CLEANUP_REASON)
}
