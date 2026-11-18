import {
  effectiveHandover,
  TIMECODE_HANDOVER_DESCRIPTIONS,
  TIMECODE_HANDOVER_OPTIONS,
  type TimecodeHandover
} from '../lib/timecode-follow'
import { useMixerCount } from '../state/deck-activity'
import { useSettings } from '../state/settings'
import { ChoiceRow } from './SettingControls'

interface TimecodeHandoverSettingProps {
  value: TimecodeHandover
  onChange: (value: TimecodeHandover) => void
}

/**
 * Picker for when following the on-air deck hands the timecode over. Handing
 * over the moment a deck goes on air needs the mixer's on-air flag; without
 * a single linked mixer the choice is held at "when the old deck leaves",
 * and the row says why.
 */
export function TimecodeHandoverSetting({
  value,
  onChange
}: TimecodeHandoverSettingProps): React.JSX.Element {
  const onAirSource = useSettings((state) => state.onAirSource)
  const mixerCount = useMixerCount()
  const forced = effectiveHandover('newOnAir', onAirSource, mixerCount) !== 'newOnAir'
  const shown = forced ? 'oldLeaves' : value
  return (
    <>
      <ChoiceRow
        label="Hand over the timecode"
        hint={TIMECODE_HANDOVER_DESCRIPTIONS[shown]}
        value={shown}
        options={TIMECODE_HANDOVER_OPTIONS}
        onChange={onChange}
        disabled={forced}
      />
      {forced && (
        <p className="setting-hint" role="note">
          Needs a single linked mixer as the on-air source; without one every playing deck counts as
          on air, so a deck in the headphones would take the timecode.
        </p>
      )}
    </>
  )
}
