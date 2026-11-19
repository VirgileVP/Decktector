import { useState } from 'react'

import { FILL_STEP_PRESETS, parseCustomStep, stepFrames } from '../lib/show-summary-view'
import type { ShowFps } from '../lib/show-timecode'
import { fillBlanks, respaceStarts } from '../lib/show-validation'
import { useShow } from '../state/show'
import { useOnAirConfirmation } from './useOnAirConfirmation'

interface FillBlanksControlProps {
  fps: ShowFps
  /** Blank starts left; with none, a step respaces every row instead. */
  blankRows: number
}

/**
 * Numbers every blank start at the previous numbered row's start plus a
 * step — a preset or a custom `HH:MM:SS`. Once no start is blank, the same
 * step respaces every row from the first row's start, so a spacing can be
 * tried again. The overlaps this may create show up in the summary like any
 * other; a deck on air whose row moves is named before it does.
 */
export function FillBlanksControl({ fps, blankRows }: FillBlanksControlProps): React.JSX.Element {
  const [custom, setCustom] = useState('')
  const [error, setError] = useState<string | null>(null)
  const respacing = blankRows === 0
  const actionTitle = respacing ? 'Overwrites every start, from the first row' : undefined
  const { confirmShowChange, dialog } = useOnAirConfirmation()

  const fill = (seconds: number): void => {
    const show = useShow.getState().show
    if (show === null) return
    const step = stepFrames(seconds, fps)
    const { fillBlanks: fillStore, respaceStarts: respaceStore } = useShow.getState()
    const rows = respacing ? respaceStarts(show.rows, step, fps) : fillBlanks(show.rows, step, fps)
    confirmShowChange({ ...show, rows }, () => (respacing ? respaceStore(step) : fillStore(step)))
    setError(null)
  }

  const fillCustom = (): void => {
    const seconds = parseCustomStep(custom)
    if (seconds === null) {
      setError('Use HH:MM:SS, longer than zero.')
      return
    }
    fill(seconds)
    setCustom('')
  }

  return (
    <div
      className="show-fill"
      role="group"
      aria-label={respacing ? 'Respace every start' : 'Fill blank starts'}
    >
      <span className="setting-hint">{respacing ? 'Respace all every' : 'Fill blanks every'}</span>
      {FILL_STEP_PRESETS.map((preset) => (
        <button
          key={preset.label}
          type="button"
          className="lineup-file-button"
          title={actionTitle}
          onClick={() => fill(preset.seconds)}
        >
          {preset.label}
        </button>
      ))}
      <input
        type="text"
        className="timecode-input-field show-fill-custom"
        value={custom}
        placeholder="HH:MM:SS"
        aria-label="Custom step"
        onChange={(event) => {
          setCustom(event.target.value)
          setError(null)
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') fillCustom()
        }}
      />
      {custom.trim() !== '' && (
        <button
          type="button"
          className="lineup-file-button"
          title={actionTitle}
          onClick={fillCustom}
        >
          {respacing ? 'Respace' : 'Fill'}
        </button>
      )}
      {error !== null && (
        <span className="timecode-input-error" role="alert">
          {error}
        </span>
      )}
      {dialog}
    </div>
  )
}
