import { useState } from 'react'

import { FILL_STEP_PRESETS, parseCustomStep, stepFrames } from '../lib/show-summary-view'
import type { ShowFps } from '../lib/show-timecode'
import { fillBlanks } from '../lib/show-validation'
import { useShow } from '../state/show'
import { useOnAirConfirmation } from './useOnAirConfirmation'

interface FillBlanksControlProps {
  fps: ShowFps
  /** Blank starts left; with none, there is nothing to fill. */
  blankRows: number
}

/**
 * Numbers every blank start at the previous numbered row's start plus a
 * step — a preset or a custom `HH:MM:SS`. The overlaps this may create show
 * up in the summary like any other; a deck on air waiting on a blank row is
 * named before it moves.
 */
export function FillBlanksControl({ fps, blankRows }: FillBlanksControlProps): React.JSX.Element {
  const [custom, setCustom] = useState('')
  const [error, setError] = useState<string | null>(null)
  const disabled = blankRows === 0
  const { confirmShowChange, dialog } = useOnAirConfirmation()

  const fill = (seconds: number): void => {
    const show = useShow.getState().show
    if (show === null) return
    const step = stepFrames(seconds, fps)
    const after = { ...show, rows: fillBlanks(show.rows, step, fps) }
    confirmShowChange(after, () => useShow.getState().fillBlanks(step))
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
    <div className="show-fill" role="group" aria-label="Fill blank starts">
      <span className="setting-hint">Fill blanks every</span>
      {FILL_STEP_PRESETS.map((preset) => (
        <button
          key={preset.label}
          type="button"
          className="lineup-file-button"
          disabled={disabled}
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
        disabled={disabled}
        onChange={(event) => {
          setCustom(event.target.value)
          setError(null)
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter') fillCustom()
        }}
      />
      <button
        type="button"
        className="lineup-file-button"
        disabled={disabled || custom.trim() === ''}
        onClick={fillCustom}
      >
        Fill
      </button>
      {error !== null && (
        <span className="timecode-input-error" role="alert">
          {error}
        </span>
      )}
      {dialog}
    </div>
  )
}
