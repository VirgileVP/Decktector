import { DECK_VIEW_TOGGLE_GROUPS } from '../components/deck-view-toggle-rows'
import {
  DECK_VIEW_PRESETS,
  DECK_VIEW_TOGGLE_KEYS,
  matchingDeckViewPreset,
  useDeckViewToggles,
  type DeckViewPreset,
  type DeckViewToggles
} from '../state/settings'
import {
  PanelDisplayCheckbox,
  PanelDisplayPopover,
  type DisplayControlProps
} from './PanelDisplayPopover'

function sameDeckView(a: DeckViewToggles, b: DeckViewToggles): boolean {
  return DECK_VIEW_TOGGLE_KEYS.every((key) => a[key] === b[key])
}

/**
 * A deck window's display options: the Deck view settings tab's presets and
 * toggles, applied to this window only.
 */
export function DeckDisplayControl({
  display,
  onChange
}: DisplayControlProps<DeckViewToggles>): React.JSX.Element {
  const mainDeckView = useDeckViewToggles()
  const activePreset = matchingDeckViewPreset(display)

  const applyPreset = (preset: DeckViewPreset): void =>
    onChange({ ...DECK_VIEW_PRESETS[preset].values })
  const toggle = (key: keyof DeckViewToggles): void =>
    onChange({ ...display, [key]: !display[key] })
  const matchMainWindow = (): void => onChange({ ...mainDeckView })

  return (
    <PanelDisplayPopover>
      <div className="setting-choice panel-display-presets" role="group" aria-label="Preset">
        {(Object.keys(DECK_VIEW_PRESETS) as DeckViewPreset[]).map((preset) => (
          <button
            key={preset}
            type="button"
            title={DECK_VIEW_PRESETS[preset].hint}
            className={activePreset === preset ? 'active' : ''}
            aria-pressed={activePreset === preset}
            onClick={() => applyPreset(preset)}
          >
            {DECK_VIEW_PRESETS[preset].label}
          </button>
        ))}
      </div>

      {DECK_VIEW_TOGGLE_GROUPS.map((group) => (
        <fieldset key={group.title} className="panel-display-group">
          <legend>{group.title}</legend>
          {group.rows.map((row) => (
            <PanelDisplayCheckbox
              key={row.key}
              label={row.label}
              hint={row.hint}
              checked={display[row.key]}
              onToggle={() => toggle(row.key)}
            />
          ))}
        </fieldset>
      ))}

      <button
        type="button"
        className="panel-display-action"
        title="Copy the Deck view settings of the main window"
        disabled={sameDeckView(display, mainDeckView)}
        onClick={matchMainWindow}
      >
        Match main window
      </button>
    </PanelDisplayPopover>
  )
}
