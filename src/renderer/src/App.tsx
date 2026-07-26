import { useEffect, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'

import neonLogo from './assets/neon-logo.png'
import { DeckPanel } from './components/DeckPanel'
import { HeaderClock } from './components/HeaderClock'
import { connectStoreToEngine, useDecktectiveStore } from './state/store'

type DeckView = 'stack' | 'quad'

const VIEW_STORAGE_KEY = 'decktective.view'

function loadView(): DeckView {
  return localStorage.getItem(VIEW_STORAGE_KEY) === 'quad' ? 'quad' : 'stack'
}

/** Accent color per player number, applied to playheads and badges. */
const DECK_COLORS: Record<number, string> = {
  1: '#4da3ff',
  2: '#ffb84d',
  3: '#6ee7a0',
  4: '#ff6b81',
  5: '#c084fc',
  6: '#22d3ee'
}
const FALLBACK_COLOR = '#9aa4b2'

const NETWORK_LABELS: Record<string, string> = {
  offline: 'Offline',
  searching: 'Searching for devices…',
  connected: 'Connected',
  failed: 'Connection failed'
}

function App(): React.JSX.Element {
  useEffect(() => connectStoreToEngine(), [])

  const [view, setView] = useState<DeckView>(loadView)
  const selectView = (next: DeckView): void => {
    setView(next)
    localStorage.setItem(VIEW_STORAGE_KEY, next)
  }

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
        <HeaderClock />
        <div className="header-right">
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
          <div className={`network-status network-${networkState}`}>
            <span className="network-dot" />
            {NETWORK_LABELS[networkState]}
          </div>
        </div>
      </header>

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
            <DeckPanel
              key={deckId}
              deckId={deckId}
              accentColor={DECK_COLORS[deckId] ?? FALLBACK_COLOR}
            />
          ))
        )}
      </main>
    </div>
  )
}

export default App
