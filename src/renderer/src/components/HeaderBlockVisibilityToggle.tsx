import {
  PanelDisplayCheckbox,
  PanelDisplayPopover,
  type DisplayControlProps
} from '../panels/PanelDisplayPopover'
import { LINEUP_VIEW_OPTIONS } from '../state/settings'
import type { HeaderBlockDisplay, HeaderBlockVisibility } from './HeaderBlockVisibility'

const VISIBILITY_ROWS: { key: keyof HeaderBlockVisibility; label: string }[] = [
  { key: 'clock', label: 'Clock' },
  { key: 'timecode', label: 'Timecode' },
  { key: 'lineup', label: 'Lineup' },
  { key: 'notes', label: 'Artist notes' }
]

/**
 * The "Time" panel window's display control: show/hide for its pieces
 * (clock, timecode, lineup, notes) and how much of the running order the
 * lineup shows.
 */
export function HeaderBlockVisibilityToggle({
  display,
  onChange
}: DisplayControlProps<HeaderBlockDisplay>): React.JSX.Element {
  const toggle = (key: keyof HeaderBlockVisibility): void =>
    onChange({ ...display, [key]: !display[key] })

  return (
    <PanelDisplayPopover>
      {VISIBILITY_ROWS.map(({ key, label }) => (
        <PanelDisplayCheckbox
          key={key}
          label={label}
          checked={display[key]}
          onToggle={() => toggle(key)}
        />
      ))}

      <fieldset className="panel-display-group" disabled={!display.lineup}>
        <legend>Lineup readout</legend>
        <div
          className="setting-choice panel-display-choice"
          role="group"
          aria-label="Lineup readout"
        >
          {LINEUP_VIEW_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              className={display.lineupView === option.value ? 'active' : ''}
              aria-pressed={display.lineupView === option.value}
              onClick={() => onChange({ ...display, lineupView: option.value })}
            >
              {option.label}
            </button>
          ))}
        </div>
      </fieldset>
    </PanelDisplayPopover>
  )
}
