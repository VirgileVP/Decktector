import { useEffect, useRef, useState } from 'react'

import { ADD_MESSAGE_CHOICES, type MessageType } from '../lib/tempo-messages'

interface AddTempoMessageMenuProps {
  onAdd: (type: MessageType) => void
}

/** "Add message" button opening a small menu of the message types. */
export function AddTempoMessageMenu({ onAdd }: AddTempoMessageMenuProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return undefined
    const closeOnOutsideClick = (event: MouseEvent): void => {
      if (!containerRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const closeOnEscape = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', closeOnOutsideClick)
    document.addEventListener('keydown', closeOnEscape)
    return () => {
      document.removeEventListener('mousedown', closeOnOutsideClick)
      document.removeEventListener('keydown', closeOnEscape)
    }
  }, [open])

  return (
    <div className="tempo-menu" ref={containerRef}>
      <button
        type="button"
        className="setting-btn"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((wasOpen) => !wasOpen)}
      >
        Add message
      </button>
      {open && (
        <div className="tempo-menu-list" role="menu">
          {ADD_MESSAGE_CHOICES.map((choice) => (
            <button
              key={choice.type}
              type="button"
              role="menuitem"
              className="tempo-menu-item"
              onClick={() => {
                setOpen(false)
                onAdd(choice.type)
              }}
            >
              <span>{choice.label}</span>
              <span className="setting-hint">{choice.hint}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
