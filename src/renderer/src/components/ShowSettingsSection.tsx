import { useMemo } from 'react'

import { countLabel } from '../lib/count-label'
import { fallbackStartFits, frameRateMismatch, readinessLabel } from '../lib/show-readiness'
import { FALLBACK_START_TC_DEFAULT, fpsOfFrameRate, frameRateOfFps } from '../lib/show-timecode'
import { analyseShow } from '../lib/show-validation'
import { openPanelWindow } from '../panels/open-panel-window'
import { useSettings } from '../state/settings'
import { useShow } from '../state/show'
import { ToggleRow } from './SettingControls'
import { TimecodeInput } from './TimecodeInput'
import { useOnAirConfirmation } from './useOnAirConfirmation'

function openShowPanel(): void {
  // `instance` is a placeholder — `openPanelWindow` picks the real one.
  openPanelWindow({ panelId: 'show', deckId: null, instance: 0 }).catch((error: unknown) =>
    console.error('Failed to open the show panel window:', error)
  )
}

/**
 * The show as the timecode output sees it, in Settings > Timecode: which
 * show is active and whether it is ready, a note while its rate and the
 * output's disagree, and the two rules for a track the show does not know —
 * where its timeline starts, or whether it is cut. A fallback change that
 * moves a deck on air asks first, like any start edit.
 */
export function ShowSettingsSection(): React.JSX.Element {
  const show = useShow((state) => state.show)
  const name = useShow((state) => state.name)
  const outputRate = useSettings((state) => state.timecodeFrameRate)
  const analysis = useMemo(() => (show === null ? null : analyseShow(show)), [show])
  const { confirmShowChange, dialog } = useOnAirConfirmation()

  if (show === null || analysis === null) {
    return (
      <section className="settings-section">
        <h3>Show</h3>
        <p className="setting-hint">
          No show loaded — every deck runs its timecode from 00:00:00:00. Import a rekordbox
          playlist in the Show tab to give each track its own start.
        </p>
      </section>
    )
  }

  const mismatch = frameRateMismatch(show, outputRate)
  const changeFallback = (label: string | null): void => {
    if (label === null) return
    confirmShowChange({ ...show, fallbackStartTimecode: label }, () =>
      useShow.getState().setFallbackStart(label)
    )
  }

  return (
    <section className="settings-section">
      <h3>Show</h3>
      <div className="setting-row">
        <span className="setting-text">
          <span className="setting-label">{name ?? 'Show'}</span>
          <span className="setting-hint">
            {countLabel(show.rows.length, 'track')} · {show.fps} fps ·{' '}
            <span className={analysis.ready ? 'show-ready' : 'show-not-ready'}>
              {readinessLabel(analysis)}
            </span>
          </span>
        </span>
        <button type="button" className="setting-btn" onClick={openShowPanel}>
          Open the Show panel
        </button>
      </div>
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
      <div className="setting-row">
        <span className="setting-text">
          <span className="setting-label">Fallback start timecode</span>
          <span className="setting-hint">
            Where the timeline of a track the show does not know starts
          </span>
        </span>
        <TimecodeInput
          value={show.fallbackStartTimecode}
          fps={show.fps}
          allowBlank={false}
          ariaLabel="Fallback start timecode"
          onCommit={changeFallback}
        />
      </div>
      {!fallbackStartFits(show) && (
        <p className="setting-hint setting-warning" role="note">
          Its frames run past {show.fps} fps: unknown tracks start at {FALLBACK_START_TC_DEFAULT}{' '}
          until it is corrected.
        </p>
      )}
      <ToggleRow
        label="Cut timecode when a track is unknown"
        hint="Sends nothing for a track the show does not list, instead of running it from the fallback start"
        checked={show.cutWhenUnidentified}
        onChange={(cut) => useShow.getState().setCutWhenUnidentified(cut)}
      />
      {dialog}
    </section>
  )
}
