import {
  ON_AIR_SOURCE_DESCRIPTIONS,
  ON_AIR_SOURCE_OPTIONS,
  type OnAirSource
} from '../lib/deck-activity'
import { useAmbiguousMixers, useMixerCount } from '../state/deck-activity'
import { ChoiceRow } from './SettingControls'

interface OnAirSourceSettingProps {
  value: OnAirSource
  onChange: (value: OnAirSource) => void
}

/** Picker for what makes a deck "live", with a warning when `auto` cannot trust the mixer. */
export function OnAirSourceSetting({
  value,
  onChange
}: OnAirSourceSettingProps): React.JSX.Element {
  const ambiguousMixers = useAmbiguousMixers()
  const mixerCount = useMixerCount()
  return (
    <>
      <ChoiceRow
        label="Live deck detection"
        hint={ON_AIR_SOURCE_DESCRIPTIONS[value]}
        value={value}
        options={ON_AIR_SOURCE_OPTIONS}
        onChange={onChange}
      />
      {ambiguousMixers && (
        <p className="setting-hint setting-warning">
          {mixerCount} mixers detected — activity follows playback.
        </p>
      )}
    </>
  )
}
