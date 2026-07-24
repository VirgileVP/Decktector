import { useEffect } from 'react'
import { useShallow } from 'zustand/react/shallow'

import { DeckPanel } from './components/DeckPanel'
import { connectStoreToEngine, useDecktectiveStore } from './state/store'

/** Accent color per player number, applied to playheads and badges. */
const DECK_COLORS: Record<number, string> = {
  1: '#4da3ff',
  2: '#ffb84d',
  3: '#6ee7a0',
  4: '#ff6b81'
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
        <h1 className="app-title">
          Decktective <span className="app-title-icon">🕵️</span>
        </h1>
        <div className={`network-status network-${networkState}`}>
          <span className="network-dot" />
          {NETWORK_LABELS[networkState]}
        </div>
      </header>

      <main className="deck-grid">
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
