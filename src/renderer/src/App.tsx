import { isTauri } from '@tauri-apps/api/core'
import { useEffect, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'

import neonLogo from './assets/neon-logo.png'
import { DeckPanel } from './components/DeckPanel'
import { DeckViewToggle } from './components/DeckViewToggle'
import { HeaderBlock } from './components/HeaderBlock'
import { RemoteLinkBanner } from './components/RemoteLinkBanner'
import { SettingsDialog, type SettingsTab } from './components/SettingsDialog'
import { DECK_ACCENT_COLOR } from './lib/deck-colors'
import { loadDeckView, saveDeckView, type DeckView } from './lib/deck-view'
import { remoteFocusedDeck } from './lib/remote-url'
import { connectMainWindowCloseCascade } from './panels/close-panel-windows-on-exit'
import { restorePanelWindows } from './panels/restore-panel-windows'
import { connectDevicesToEngine, useDevicesStore } from './state/devices'
import { connectDownloadToEngine } from './state/download'
import { connectPanelWindowRegistry } from './state/panel-windows'
import { connectRemoteToEngine } from './state/remote'
import { connectSettingsBroadcast } from './state/settings-broadcast'
import { useDeckViewToggles } from './state/settings'
import { connectSettingsToApp } from './state/settings-sync'
import { connectStoreToEngine, useDecktectorStore } from './state/store'
import { connectTimecodeToEngine } from './state/timecode'
import { connectTimecodeFollowOnAir } from './state/timecode-follow'
import { useClockNow } from './state/wall-clock'

/** Evenly spaced teeth of the settings gear icon, in degrees. */
const GEAR_TOOTH_ANGLES = [0, 45, 90, 135, 180, 225, 270, 315] as const

const NETWORK_LABELS: Record<string, string> = {
  offline: 'Offline',
  searching: 'Searching for devices…',
  connected: 'Connected',
  failed: 'Connection failed'
}

const MS_PER_MINUTE = 60_000
const MS_PER_HOUR = 3_600_000

/** Coarse "how long ago", client-observed — the protocol gives us no failure
 *  reason, so this is honestly just "since we last saw any device", not a
 *  diagnosis. */
function formatElapsed(ms: number): string {
  if (ms < MS_PER_MINUTE) return `${Math.max(1, Math.floor(ms / 1000))}s`
  if (ms < MS_PER_HOUR) return `${Math.floor(ms / MS_PER_MINUTE)}m`
  return `${Math.floor(ms / MS_PER_HOUR)}h`
}

function App(): React.JSX.Element {
  useEffect(() => connectStoreToEngine(), [])
  useEffect(() => connectSettingsToApp('main'), [])
  useEffect(() => connectSettingsBroadcast(), [])
  useEffect(() => connectTimecodeToEngine(), [])
  useEffect(() => connectTimecodeFollowOnAir(), [])
  useEffect(() => connectDevicesToEngine(), [])
  useEffect(() => connectDownloadToEngine(), [])
  useEffect(() => connectRemoteToEngine(), [])
  useEffect(() => connectPanelWindowRegistry(), [])
  useEffect(() => connectMainWindowCloseCascade(), [])
  useEffect(() => {
    void restorePanelWindows()
  }, [])

  const [focusedDeckId] = useState<number | null>(() => remoteFocusedDeck(window.location))
  const [view, setView] = useState<DeckView>(loadDeckView)
  const selectView = (next: DeckView): void => {
    setView(next)
    saveDeckView(next)
  }

  const [settingsTab, setSettingsTab] = useState<SettingsTab | null>(null)
  const deckViewToggles = useDeckViewToggles()

  const networkState = useDecktectorStore((s) => s.networkState)
  const lastDeviceSeenAt = useDevicesStore((s) => s.lastSeenAt)
  // Piggybacks on the app's one shared per-second tick so the "last seen"
  // label stays live without a second timer or an impure `Date.now()` read
  // in render.
  const nowMs = useClockNow()
  const networkFailed = networkState === 'failed'
  const lastSeenHint =
    networkFailed && lastDeviceSeenAt !== null
      ? ` — last seen ${formatElapsed(nowMs - lastDeviceSeenAt)} ago`
      : ''
  const deckIds = useDecktectorStore(
    useShallow((s) =>
      Object.keys(s.decks)
        .map(Number)
        .sort((a, b) => a - b)
    )
  )
  // Full-screen focus (via `?deck=N`) forces the single-column layout
  // regardless of the view toggle — a multi-column grid would otherwise
  // leave the one visible deck sharing the screen with empty cells.
  const visibleDeckIds =
    focusedDeckId !== null && deckIds.includes(focusedDeckId) ? [focusedDeckId] : deckIds
  const effectiveView: DeckView = focusedDeckId !== null ? 'stack' : view

  return (
    <div className="app">
      <RemoteLinkBanner />
      <header className="app-header">
        <img className="neon-logo" src={neonLogo} alt="NEON" />
        <HeaderBlock />
        <div className="header-right">
          {networkFailed && isTauri() ? (
            // The Devices tab is shell-only (see `SettingsDialog.tsx`), so this
            // shortcut only makes sense — and only renders as interactive — in
            // the desktop shell; a remote viewer gets the same plain, inert
            // indicator as every other network state.
            <button
              type="button"
              className="network-status network-failed network-status-action"
              title="Connection failed — open Devices to check what Decktector can see on the network"
              onClick={() => setSettingsTab('Devices')}
            >
              <span className="network-dot" />
              {NETWORK_LABELS.failed}
              {lastSeenHint}
            </button>
          ) : (
            <div className={`network-status network-${networkState}`}>
              <span className="network-dot" />
              {NETWORK_LABELS[networkState]}
            </div>
          )}
          <DeckViewToggle view={view} onSelect={selectView} />
          <button
            type="button"
            className="settings-button"
            title="Settings"
            aria-label="Settings"
            onClick={() => setSettingsTab('Interface')}
          >
            <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
              {GEAR_TOOTH_ANGLES.map((angle) => (
                <rect
                  key={angle}
                  x="10.7"
                  y="1.4"
                  width="2.6"
                  height="6"
                  rx="1.1"
                  transform={`rotate(${angle} 12 12)`}
                />
              ))}
              {/* Hub with the center punched out by the even-odd rule. */}
              <path
                fillRule="evenodd"
                d="M12 5a7 7 0 1 0 0 14 7 7 0 1 0 0-14Zm0 4a3 3 0 1 1 0 6 3 3 0 0 1 0-6Z"
              />
            </svg>
          </button>
        </div>
      </header>

      {settingsTab !== null && (
        <SettingsDialog initialTab={settingsTab} onClose={() => setSettingsTab(null)} />
      )}

      <main className={`deck-grid view-${effectiveView}`}>
        {visibleDeckIds.length === 0 ? (
          <div className="empty-state">
            <p>Waiting for Pro DJ Link devices…</p>
            <p className="empty-hint">
              Connect this machine to the same network as your CDJ/XDJ gear, and make sure Rekordbox
              is not running.
            </p>
          </div>
        ) : (
          visibleDeckIds.map((deckId) => (
            <DeckPanel
              key={deckId}
              deckId={deckId}
              accentColor={DECK_ACCENT_COLOR}
              view={deckViewToggles}
            />
          ))
        )}
      </main>
    </div>
  )
}

export default App
