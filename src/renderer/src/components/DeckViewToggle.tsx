import type { JSX } from 'react'

import type { DeckView } from '../lib/deck-view'

interface DeckViewOption {
  view: DeckView
  title: string
  icon: JSX.Element
}

const DECK_VIEW_OPTIONS: readonly DeckViewOption[] = [
  {
    view: 'stack',
    title: 'Stacked view',
    icon: (
      <svg viewBox="0 0 14 14" width="14" height="14">
        <rect x="1" y="1.5" width="12" height="4.4" rx="1" />
        <rect x="1" y="8.1" width="12" height="4.4" rx="1" />
      </svg>
    )
  },
  {
    view: 'grid',
    title: 'Grid view (3 columns)',
    icon: (
      <svg viewBox="0 0 14 14" width="14" height="14">
        <rect x="1" y="1" width="3.4" height="5.4" rx="0.8" />
        <rect x="5.3" y="1" width="3.4" height="5.4" rx="0.8" />
        <rect x="9.6" y="1" width="3.4" height="5.4" rx="0.8" />
        <rect x="1" y="7.6" width="3.4" height="5.4" rx="0.8" />
        <rect x="5.3" y="7.6" width="3.4" height="5.4" rx="0.8" />
        <rect x="9.6" y="7.6" width="3.4" height="5.4" rx="0.8" />
      </svg>
    )
  },
  {
    view: 'quad',
    title: 'Grid view (2 columns)',
    icon: (
      <svg viewBox="0 0 14 14" width="14" height="14">
        <rect x="1" y="1" width="5.4" height="5.4" rx="1" />
        <rect x="7.6" y="1" width="5.4" height="5.4" rx="1" />
        <rect x="1" y="7.6" width="5.4" height="5.4" rx="1" />
        <rect x="7.6" y="7.6" width="5.4" height="5.4" rx="1" />
      </svg>
    )
  }
]

interface DeckViewToggleProps {
  view: DeckView
  onSelect: (view: DeckView) => void
}

/** Header buttons that pick how the deck panels are arranged. */
export function DeckViewToggle({ view, onSelect }: DeckViewToggleProps): React.JSX.Element {
  return (
    <div className="view-toggle">
      {DECK_VIEW_OPTIONS.map((option) => (
        <button
          key={option.view}
          type="button"
          className={view === option.view ? 'active' : ''}
          title={option.title}
          aria-pressed={view === option.view}
          onClick={() => onSelect(option.view)}
        >
          {option.icon}
        </button>
      ))}
    </div>
  )
}
