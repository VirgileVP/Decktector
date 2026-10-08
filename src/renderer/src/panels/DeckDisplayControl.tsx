import { DECK_VIEW_TOGGLE_GROUPS } from '../components/deck-view-toggle-rows'
import { DECK_VIEW_TOGGLE_KEYS, useDeckViewToggles, type DeckViewToggles } from '../state/settings'
import type { CompactDeckFields, DeckWindowDisplay, DeckWindowLayout } from './panel-display'
import {
  PanelDisplayCheckbox,
  PanelDisplayPopover,
  type DisplayControlProps
} from './PanelDisplayPopover'

const LAYOUT_OPTIONS: readonly { value: DeckWindowLayout; label: string; hint: string }[] = [
  {
    value: 'full',
    label: 'Full',
    hint: 'The deck as the main window shows it, waveforms included'
  },
  { value: 'compact', label: 'Compact', hint: 'A single row sized for a small window' }
]

const COMPACT_FIELD_ROWS: readonly { key: keyof CompactDeckFields; label: string }[] = [
  { key: 'artwork', label: 'Artwork' },
  { key: 'deckNumber', label: 'Deck number' },
  { key: 'label', label: 'Deck label' },
  { key: 'title', label: 'Track title' },
  { key: 'artist', label: 'Artist' },
  { key: 'trackTags', label: 'Track tags' },
  { key: 'transport', label: 'Play/pause state' },
  { key: 'time', label: 'Time' },
  { key: 'timeRemaining', label: 'Time remaining' },
  { key: 'pitch', label: 'Pitch' },
  { key: 'bpm', label: 'BPM' },
  { key: 'syncBadges', label: 'Sync badges' }
]

function sameDeckView(a: DeckViewToggles, b: DeckViewToggles): boolean {
  return DECK_VIEW_TOGGLE_KEYS.every((key) => a[key] === b[key])
}

/**
 * A deck window's display options: its layout, then what that layout shows.
 * The full layout lists the same toggles as the Deck view settings tab, but
 * they only apply to this window.
 */
export function DeckDisplayControl({
  display,
  onChange
}: DisplayControlProps<DeckWindowDisplay>): React.JSX.Element {
  const mainDeckView = useDeckViewToggles()

  const selectLayout = (layout: DeckWindowLayout): void => onChange({ ...display, layout })
  const toggleFull = (key: keyof DeckViewToggles): void =>
    onChange({ ...display, full: { ...display.full, [key]: !display.full[key] } })
  const toggleCompact = (key: keyof CompactDeckFields): void =>
    onChange({ ...display, compact: { ...display.compact, [key]: !display.compact[key] } })
  const matchMainWindow = (): void => onChange({ ...display, full: { ...mainDeckView } })

  return (
    <PanelDisplayPopover>
      <div className="setting-choice panel-display-layout" role="group" aria-label="Layout">
        {LAYOUT_OPTIONS.map(({ value, label, hint }) => (
          <button
            key={value}
            type="button"
            title={hint}
            className={display.layout === value ? 'active' : ''}
            aria-pressed={display.layout === value}
            onClick={() => selectLayout(value)}
          >
            {label}
          </button>
        ))}
      </div>

      {display.layout === 'full' ? (
        <>
          {DECK_VIEW_TOGGLE_GROUPS.map((group) => (
            <fieldset key={group.title} className="panel-display-group">
              <legend>{group.title}</legend>
              {group.rows.map((row) => (
                <PanelDisplayCheckbox
                  key={row.key}
                  label={row.label}
                  hint={row.hint}
                  checked={display.full[row.key]}
                  onToggle={() => toggleFull(row.key)}
                />
              ))}
            </fieldset>
          ))}
          <button
            type="button"
            className="panel-display-action"
            title="Copy the Deck view settings of the main window"
            disabled={sameDeckView(display.full, mainDeckView)}
            onClick={matchMainWindow}
          >
            Match main window
          </button>
        </>
      ) : (
        <fieldset className="panel-display-group">
          <legend>Show</legend>
          {COMPACT_FIELD_ROWS.map(({ key, label }) => (
            <PanelDisplayCheckbox
              key={key}
              label={label}
              checked={display.compact[key]}
              onToggle={() => toggleCompact(key)}
            />
          ))}
        </fieldset>
      )}
    </PanelDisplayPopover>
  )
}
