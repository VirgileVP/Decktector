import { useEffect, useRef, useState } from 'react'

/** What a panel's `DisplayControl` (see `registry.tsx`) receives: the window's display and a way to replace it. */
export interface DisplayControlProps<T> {
  display: T
  onChange: (next: T) => void
}

interface PanelDisplayPopoverProps {
  children: React.ReactNode
}

/**
 * The chrome button that opens a window's display options, and the popover
 * it opens. Styled like every other chrome control and living in the same
 * toolbar, rather than floating over the content it controls. A click
 * anywhere outside it closes it — Escape is already taken: it closes the
 * window (see `PanelChrome`).
 */
export function PanelDisplayPopover({ children }: PanelDisplayPopoverProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const wrapRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const closeOnOutsidePointer = (event: PointerEvent): void => {
      if (!wrapRef.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer)
  }, [open])

  return (
    <div className="panel-chrome-toggle-wrap" ref={wrapRef}>
      <button
        type="button"
        className={`panel-chrome-toggle${open ? ' active' : ''}`}
        title="Display options"
        aria-label="Display options"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
          <path d="M1.5 3h9M1.5 6h9M1.5 9h9" />
          <circle cx="4" cy="3" r="1.1" />
          <circle cx="8" cy="6" r="1.1" />
          <circle cx="5" cy="9" r="1.1" />
        </svg>
      </button>
      {open && (
        <div className="panel-display-popover" role="dialog" aria-label="Display options">
          {children}
        </div>
      )}
    </div>
  )
}

interface PanelDisplayCheckboxProps {
  label: string
  /** Shown as a tooltip — the popover has no room for a second line. */
  hint?: string
  checked: boolean
  onToggle: () => void
}

/** One show/hide line of a display popover. */
export function PanelDisplayCheckbox({
  label,
  hint,
  checked,
  onToggle
}: PanelDisplayCheckboxProps): React.JSX.Element {
  return (
    <label className="panel-display-checkbox" title={hint}>
      <input type="checkbox" checked={checked} onChange={onToggle} />
      {label}
    </label>
  )
}
