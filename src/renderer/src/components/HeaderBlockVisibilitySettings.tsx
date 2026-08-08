import { useState } from 'react'

import { isPanelWindow } from '../panels/params'
import type { HeaderBlockVisibility } from './HeaderBlockVisibility'

interface HeaderBlockVisibilitySettingsProps {
  visibility: HeaderBlockVisibility
  onChange: (next: HeaderBlockVisibility) => void
}

const VISIBILITY_ROWS: { key: keyof HeaderBlockVisibility; label: string }[] = [
  { key: 'clock', label: 'Clock' },
  { key: 'timecode', label: 'Timecode' },
  { key: 'lineup', label: 'Lineup' }
]

/**
 * Settings button + popover that shows or hides each piece of the header
 * block — clock, timecode, lineup — independently of the others. Only
 * useful once the block is popped into its own window: the main header
 * always shows all three, so this renders nothing there.
 */
export function HeaderBlockVisibilitySettings({
  visibility,
  onChange
}: HeaderBlockVisibilitySettingsProps): React.JSX.Element | null {
  const [open, setOpen] = useState(false)

  if (!isPanelWindow(window.location)) return null

  const toggle = (key: keyof HeaderBlockVisibility): void =>
    onChange({ ...visibility, [key]: !visibility[key] })

  return (
    <div className="header-block-settings">
      <button
        type="button"
        className="header-block-settings-button"
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
