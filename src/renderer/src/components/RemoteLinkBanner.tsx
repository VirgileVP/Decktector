import { isTauri } from '@tauri-apps/api/core'

import { useRemoteLinkStore } from '../state/remote-link'

/** The banner only means anything for the WebSocket transport — the Tauri app shell has no link to lose. */
const IS_TAURI = isTauri()

/**
 * Warns that the remote link is down, so a frozen remote screen is never
 * mistaken for a frozen booth. Renders nothing under Tauri, where
 * `state/remote-link.ts` is never touched and would otherwise sit at its
 * "disconnected" default forever.
 */
export function RemoteLinkBanner(): React.JSX.Element | null {
  const connected = useRemoteLinkStore((s) => s.connected)
  const attempts = useRemoteLinkStore((s) => s.attempts)

  if (IS_TAURI || connected) return null

  return (
    <div className="remote-link-banner" role="status">
      Reconnecting to Decktector{attempts > 1 ? ` (attempt ${attempts})` : ''}…
    </div>
  )
}
