import { useEffect, useRef } from 'react'

import { DECK_NUMBERS, DEFAULT_DECK_COLORS, deckColor } from '../lib/deck-colors'
import { THEMES } from '../lib/theme'
import {
  UI_SCALE_RANGE,
  WAVEFORM_DESATURATION_RANGE,
  useSettings,
  type Settings
} from '../state/settings'
import { LineupEditor } from './LineupEditor'
import { SliderRow, ToggleRow } from './SettingControls'

const asPercent = (value: number): string => `${Math.round(value * 100)} %`

interface SettingsDialogProps {
  onClose: () => void
}

/** App settings, as a modal over the deck grid. Closes on Escape or backdrop click. */
export function SettingsDialog({ onClose }: SettingsDialogProps): React.JSX.Element {
  const panelRef = useRef<HTMLDivElement>(null)
  const settings = useSettings()
  const update = <K extends keyof Settings>(key: K) => {
    return (value: Settings[K]): void => settings.update(key, value)
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    // The panel takes focus so Escape works without clicking inside first.
    panelRef.current?.focus()
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  return (
    <div className="settings-backdrop" onClick={onClose}>
      <div
        ref={panelRef}
        className="settings-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <header className="settings-head">
          <h2>Settings</h2>
          <button type="button" className="settings-close" title="Close" onClick={onClose}>
            ×
          </button>
        </header>

        <LineupEditor />

        <section className="settings-section">
          <h3>Theme</h3>
          <div className="theme-grid">
            {THEMES.map((option) => (
              <button
                key={option.id}
                type="button"
                className={`theme-card${option.id === settings.theme ? ' active' : ''}`}
                aria-pressed={option.id === settings.theme}
                onClick={() => settings.update('theme', option.id)}
              >
                {/* Palettes are scoped by attribute, so a swatch can wear another
                    theme than the app and preview it without any extra CSS. */}
                <span className="theme-swatch" data-theme={option.id}>
                  <i className="swatch-bg" />
                  <i className="swatch-panel" />
                  <i className="swatch-accent" />
                </span>
                <span className="theme-label">{option.label}</span>
                <span className="theme-description">{option.description}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="settings-section">
          <h3>Display</h3>
          <SliderRow
            label="Interface scale"
            hint="Zooms the whole window — booth screens run from 13 to 27 inches"
            value={settings.uiScale}
            range={UI_SCALE_RANGE}
            format={asPercent}
            onChange={update('uiScale')}
          />
          <SliderRow
            label="Waveform wash"
            hint="Flattens the rekordbox colors; at 0 % they are left raw"
            value={settings.waveformDesaturation}
            range={WAVEFORM_DESATURATION_RANGE}
            format={asPercent}
            onChange={update('waveformDesaturation')}
          />
          <ToggleRow
            label="Dim decks that are off air"
            hint="Fades idle decks so the live ones read at a glance"
            checked={settings.dimIdleDecks}
            onChange={update('dimIdleDecks')}
          />
        </section>

        <section className="settings-section">
          <div className="settings-section-head">
            <h3>Deck colors</h3>
            <button type="button" className="settings-reset" onClick={settings.resetDeckColors}>
              Reset
            </button>
          </div>
          <div className="deck-color-row">
            {DECK_NUMBERS.map((deckId) => (
              <label key={deckId} className="deck-color">
                <input
                  type="color"
                  value={deckColor(deckId, settings.deckColors)}
                  onChange={(event) => settings.setDeckColor(deckId, event.target.value)}
                />
                <span>{deckId}</span>
              </label>
            ))}
          </div>
          <p className="setting-hint">
            Player accents, used by the playheads and the deck badges. Defaults:{' '}
            {DECK_NUMBERS.map((deckId) => DEFAULT_DECK_COLORS[deckId]).join(', ')}.
          </p>
        </section>

        <section className="settings-section">
          <h3>Window</h3>
          <ToggleRow
            label="Always on top"
            hint="Keeps Decktective above every other application"
            checked={settings.alwaysOnTop}
            onChange={update('alwaysOnTop')}
          />
          <ToggleRow
            label="Keep the screen awake"
            hint="Holds off the screen saver while a set is running"
            checked={settings.preventSleep}
            onChange={update('preventSleep')}
          />
        </section>
      </div>
    </div>
  )
}
