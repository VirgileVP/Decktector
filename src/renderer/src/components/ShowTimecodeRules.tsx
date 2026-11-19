import { fallbackStartFits, frameRateMismatch } from '../lib/show-readiness'
import { FALLBACK_START_TC_DEFAULT, fpsOfFrameRate, frameRateOfFps } from '../lib/show-timecode'
import { useSettings } from '../state/settings'
import { useShow } from '../state/show'
import { ToggleRow } from './SettingControls'
import { TimecodeInput } from './TimecodeInput'
import { useOnAirConfirmation } from './useOnAirConfirmation'

/**
 * The show's timecode rules, under its table in Settings > Show: a note
 * while its rate and the output's disagree, and the two rules for a track
 * the show does not know — whether it is cut, or else where its timeline
 * starts; the fallback start is greyed out while it is cut.
 * A fallback change that moves a deck on air asks first, like any start
 * edit. With no show there are no rules to set.
 */
export function ShowTimecodeRules(): React.JSX.Element | null {
  const show = useShow((state) => state.show)
  const outputRate = useSettings((state) => state.timecodeFrameRate)
  const { confirmShowChange, dialog } = useOnAirConfirmation()

  if (show === null) return null
  const cut = show.cutWhenUnidentified

  const mismatch = frameRateMismatch(show, outputRate)
  const changeFallback = (label: string | null): void => {
    if (label === null) return
    confirmShowChange({ ...show, fallbackStartTimecode: label }, () =>
      useShow.getState().setFallbackStart(label)
    )
  }

  return (
    <section className="settings-section">
      <h3>Timecode</h3>
      {mismatch !== null && (
        <p className="setting-hint setting-warning" role="note">
          The show runs at {mismatch} fps and the output at {fpsOfFrameRate(outputRate)} fps:
          nothing is sent until they match.{' '}
          <button
            type="button"
            className="setting-btn"
            onClick={() =>
              useSettings.getState().update('timecodeFrameRate', frameRateOfFps(mismatch))
            }
          >
            Switch output to {mismatch} fps
          </button>
        </p>
      )}
      <ToggleRow
        label="Cut timecode when a track is unknown"
        hint="Sends nothing for a track the show does not list, instead of running it from the fallback start"
        checked={cut}
        onChange={(enabled) => useShow.getState().setCutWhenUnidentified(enabled)}
      />
      <div className={`setting-row${cut ? ' setting-row-disabled' : ''}`}>
        <span className="setting-text">
          <span className="setting-label">Fallback start timecode</span>
          <span className="setting-hint">
            {cut
              ? 'Unused while the timecode is cut for unknown tracks'
              : 'Where the timeline of a track the show does not know starts'}
          </span>
        </span>
        <TimecodeInput
          value={show.fallbackStartTimecode}
          fps={show.fps}
          allowBlank={false}
          ariaLabel="Fallback start timecode"
          disabled={cut}
          onCommit={changeFallback}
        />
      </div>
      {!cut && !fallbackStartFits(show) && (
        <p className="setting-hint setting-warning" role="note">
          Its frames run past {show.fps} fps: unknown tracks start at {FALLBACK_START_TC_DEFAULT}{' '}
          until it is corrected.
        </p>
      )}
      {dialog}
    </section>
  )
}
