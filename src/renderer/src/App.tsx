import { useEffect, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'

import neonLogo from './assets/neon-logo.png'
import { DeckPanel } from './components/DeckPanel'
import { HeaderClock } from './components/HeaderClock'
import { LineupNow } from './components/LineupNow'
import { SettingsDialog } from './components/SettingsDialog'
import { TimecodeClock } from './components/TimecodeClock'
import { TimecodeSourceButtons } from './components/TimecodeSourceButtons'
import { deckColor } from './lib/deck-colors'
import { connectDevicesToEngine } from './state/devices'
import { connectRemoteToEngine } from './state/remote'
import { useSettings } from './state/settings'
import { connectSettingsToApp } from './state/settings-sync'
import { connectStoreToEngine, useDecktectiveStore } from './state/store'
import { connectTimecodeToEngine } from './state/timecode'

type DeckView = 'stack' | 'quad'

const VIEW_STORAGE_KEY = 'decktective.view'

function loadView(): DeckView {
  return localStorage.getItem(VIEW_STORAGE_KEY) === 'quad' ? 'quad' : 'stack'
}

/** Evenly spaced teeth of the settings gear icon, in degrees. */
const GEAR_TOOTH_ANGLES = [0, 45, 90, 135, 180, 225, 270, 315] as const

const NETWORK_LABELS: Record<string, string> = {
  offline: 'Offline',
  searching: 'Searching for devices…',
  connected: 'Connected',
  failed: 'Connection failed'
}

function App(): React.JSX.Element {
  useEffect(() => connectStoreToEngine(), [])
  useEffect(() => connectSettingsToApp(), [])
  useEffect(() => connectTimecodeToEngine(), [])
  useEffect(() => connectDevicesToEngine(), [])
  useEffect(() => connectRemoteToEngine(), [])

  const [view, setView] = useState<DeckView>(loadView)
  const selectView = (next: DeckView): void => {
    setView(next)
    localStorage.setItem(VIEW_STORAGE_KEY, next)
  }

  const [settingsOpen, setSettingsOpen] = useState(false)
  const deckColors = useSettings((s) => s.deckColors)

  const networkState = useDecktectiveStore((s) => s.networkState)
  const deckIds = useDecktectiveStore(
    useShallow((s) =>
      Object.keys(s.decks)
        .map(Number)
        .sort((a, b) => a - b)
    )
  )

  return (
    <div className="app">
      <header className="app-header">
        <img className="neon-logo" src={neonLogo} alt="NEON" />
        <div className="header-center">
          <div className="header-clocks">
            <HeaderClock />
            <TimecodeClock />
            <TimecodeSourceButtons />
          </div>
          <LineupNow />
        </div>
        <div className="header-right">
          <div className={`network-status network-${networkState}`}>
            <span className="network-dot" />
            {NETWORK_LABELS[networkState]}
          </div>
          <div className="view-toggle">
            <button
              type="button"
              className={view === 'stack' ? 'active' : ''}
              title="Stacked view"
              onClick={() => selectView('stack')}
            >
              <svg viewBox="0 0 14 14" width="14" height="14">
                <rect x="1" y="1.5" width="12" height="4.4" rx="1" />
                <rect x="1" y="8.1" width="12" height="4.4" rx="1" />
              </svg>
            </button>
            <button
              type="button"
              className={view === 'quad' ? 'active' : ''}
              title="Quad view (2×2)"
              onClick={() => selectView('quad')}
            >
              <svg viewBox="0 0 14 14" width="14" height="14">
                <rect x="1" y="1" width="5.4" height="5.4" rx="1" />
                <rect x="7.6" y="1" width="5.4" height="5.4" rx="1" />
                <rect x="1" y="7.6" width="5.4" height="5.4" rx="1" />
                <rect x="7.6" y="7.6" width="5.4" height="5.4" rx="1" />
              </svg>
            </button>
          </div>
          <button
            type="button"
            className="settings-button"
            title="Settings"
            aria-label="Settings"
            onClick={() => setSettingsOpen(true)}
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

      {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} />}

      <main className={`deck-grid view-${view}`}>
        {deckIds.length === 0 ? (
          <div className="empty-state">
            <p>Waiting for Pro DJ Link devices…</p>
            <p className="empty-hint">
              Connect this machine to the same network as your CDJ/XDJ gear, and make sure Rekordbox
              is not running.
            </p>
          </div>
        ) : (
          deckIds.map((deckId) => (
            <DeckPanel key={deckId} deckId={deckId} accentColor={deckColor(deckId, deckColors)} />
          ))
        )}
      </main>
    </div>
  )
}

export default App
