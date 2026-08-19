import { useState } from 'react'

import type { HeaderBlockVisibility } from './HeaderBlockVisibility'

interface HeaderBlockVisibilityToggleProps {
  visibility: HeaderBlockVisibility
  onChange: (next: HeaderBlockVisibility) => void
}

const VISIBILITY_ROWS: { key: keyof HeaderBlockVisibility; label: string }[] = [
  { key: 'clock', label: 'Clock' },
  { key: 'timecode', label: 'Timecode' },
  { key: 'lineup', label: 'Lineup' }
]

/**
 * The "Time" panel window's show/hide control for its three pieces (clock,
 * timecode, lineup) — a button styled like every other chrome control
 * (`PanelChrome`'s always-on-top/transparency/zoom), living in the same
 * toolbar rather than floating over the content it controls.
 */
export function HeaderBlockVisibilityToggle({
  visibility,
  onChange
}: HeaderBlockVisibilityToggleProps): React.JSX.Element {
  const [open, setOpen] = useState(false)

  const toggle = (key: keyof HeaderBlockVisibility): void =>
    onChange({ ...visibility, [key]: !visibility[key] })

  return (
    <div className="panel-chrome-toggle-wrap">
      <button
        type="button"
        className={`panel-chrome-toggle${open ? ' active' : ''}`}
        title="Show or hide"
        aria-label="Show or hide"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
          <circle cx="6" cy="6" r="1.6" />
          <path
            d="M6 1v1.8M6 9.2V11M1 6h1.8M9.2 6H11M2.76 2.76l1.27 1.27M7.97 7.97l1.27 1.27M2.76 9.24l1.27-1.27M7.97 4.03l1.27-1.27"
          />
        </svg>
      </button>
      {open && (
        <div className="header-block-settings-popover">
          {VISIBILITY_ROWS.map(({ key, label }) => (
            <label key={key}>
              <input type="checkbox" checked={visibility[key]} onChange={() => toggle(key)} />
              {label}
            </label>
          ))}
        </div>
      )}
    </div>
  )
}
