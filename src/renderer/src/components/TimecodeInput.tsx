import { useState } from 'react'

import { readTimecodeLabel, type ShowFps } from '../lib/show-timecode'

interface TimecodeInputProps {
  /** The stored `HH:MM:SS:FF` label; null while blank. */
  value: string | null
  fps: ShowFps
  /** Called with a valid label — or null when `allowBlank` and the field was cleared. */
  onCommit: (label: string | null) => void
  allowBlank: boolean
  ariaLabel: string
  disabled?: boolean
}

/** Why `text` is not a start timecode at `fps`, or null when it is one. */
function rejection(text: string, fps: ShowFps, allowBlank: boolean): string | null {
  if (text === '') return allowBlank ? null : 'A timecode is needed here.'
  const fields = readTimecodeLabel(text)
  if (fields === null) return 'Use HH:MM:SS:FF, hours 00–23.'
  if (fields.frames >= fps) return `Frames run 00–${fps - 1} at ${fps} fps.`
  return null
}

/**
 * A start timecode typed as free text and committed on blur or Enter, kept as
 * the label the operator typed. Input that is not a timecode at the show's
 * fps is refused with the reason, and the stored value stays; Escape gives
 * the stored value back.
 */
export function TimecodeInput({
  value,
  fps,
  onCommit,
  allowBlank,
  ariaLabel,
  disabled = false
}: TimecodeInputProps): React.JSX.Element {
  const [draft, setDraft] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const shown = draft ?? value ?? ''

  const commit = (): void => {
    if (draft === null) return
    const text = draft.trim()
    const reason = rejection(text, fps, allowBlank)
    if (reason !== null) {
      setError(reason)
      setDraft(null)
      return
    }
    setError(null)
    setDraft(null)
    const label = text === '' ? null : text
    if (label !== value) onCommit(label)
  }

  return (
    <span className="timecode-input">
      <input
        type="text"
        className={`timecode-input-field${error === null ? '' : ' timecode-input-invalid'}`}
        value={shown}
        placeholder={allowBlank ? '--:--:--:--' : 'HH:MM:SS:FF'}
        aria-label={ariaLabel}
        aria-invalid={error !== null}
        title={error ?? undefined}
        spellCheck={false}
        disabled={disabled}
        onChange={(event) => {
          setDraft(event.target.value)
          setError(null)
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') commit()
          if (event.key === 'Escape') {
            event.stopPropagation()
            setDraft(null)
            setError(null)
          }
        }}
      />
      {error !== null && (
        <span className="timecode-input-error" role="alert">
          {error}
        </span>
      )}
    </span>
  )
}
