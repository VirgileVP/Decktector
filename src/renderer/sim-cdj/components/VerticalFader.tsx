import { useRef, useState } from 'react'
import type { PointerEvent } from 'react'

import { fractionFromTop, valueFromFraction, type FaderTravel } from '../lib/fader-math'

/** Values closer than this to the last one sent are not worth another message. */
const MIN_FADER_STEP = 0.002

interface VerticalFaderProps {
  value: number
  travel: FaderTravel
  label: string
  disabled?: boolean
  /** Value a double click snaps back to, e.g. the pitch fader's centre. */
  resetValue?: number
  /** Draws a tick at the middle of the travel. */
  centerMark?: boolean
  onChange: (value: number) => void
}

/**
 * A vertical slider driven by the pointer. While it is dragged it shows the
 * pointer's value rather than the server's, so the cap does not stutter
 * behind the echo of its own commands.
 */
export function VerticalFader({
  value,
  travel,
  label,
  disabled = false,
  resetValue,
  centerMark = false,
  onChange
}: VerticalFaderProps): React.JSX.Element {
  const trackRef = useRef<HTMLDivElement>(null)
  const lastSentRef = useRef<number | null>(null)
  const [dragValue, setDragValue] = useState<number | null>(null)

  const valueAt = (clientY: number): number => {
    const rect = trackRef.current?.getBoundingClientRect()
    if (!rect || rect.height <= 0) return value
    return valueFromFraction((clientY - rect.top) / rect.height, travel)
  }

  const emit = (next: number): void => {
    setDragValue(next)
    const last = lastSentRef.current
    if (last !== null && Math.abs(next - last) < MIN_FADER_STEP) return
    lastSentRef.current = next
    onChange(next)
  }

  const onPointerDown = (event: PointerEvent<HTMLDivElement>): void => {
    if (disabled) return
    event.currentTarget.setPointerCapture(event.pointerId)
    lastSentRef.current = null
    emit(valueAt(event.clientY))
  }

  const onPointerMove = (event: PointerEvent<HTMLDivElement>): void => {
    if (dragValue === null) return
    emit(valueAt(event.clientY))
  }

  const endDrag = (): void => {
    setDragValue(null)
    lastSentRef.current = null
  }

  const onDoubleClick = (): void => {
    if (disabled || resetValue === undefined) return
    onChange(resetValue)
  }

  const shown = dragValue ?? value
  const topPercent = fractionFromTop(shown, travel) * 100

  return (
    <div
      className={`fader${disabled ? ' fader-disabled' : ''}`}
      role="slider"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={travel.min}
      aria-valuemax={travel.max}
      aria-valuenow={shown}
      aria-disabled={disabled}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onDoubleClick={onDoubleClick}
    >
      <div ref={trackRef} className="fader-track">
        {centerMark && <span className="fader-center-mark" />}
        <span className="fader-cap" style={{ top: `${topPercent}%` }} />
      </div>
    </div>
  )
}
