import {
  PanelDisplayCheckbox,
  PanelDisplayPopover,
  type DisplayControlProps
} from '../panels/PanelDisplayPopover'
import type { HeaderBlockVisibility } from './HeaderBlockVisibility'

const VISIBILITY_ROWS: { key: keyof HeaderBlockVisibility; label: string }[] = [
  { key: 'clock', label: 'Clock' },
  { key: 'timecode', label: 'Timecode' },
  { key: 'lineup', label: 'Lineup' },
  { key: 'notes', label: 'Artist notes' }
]

/** The "Time" panel window's show/hide control for its pieces (clock, timecode, lineup, notes). */
export function HeaderBlockVisibilityToggle({
  display,
  onChange
}: DisplayControlProps<HeaderBlockVisibility>): React.JSX.Element {
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
    </PanelDisplayPopover>
  )
}
