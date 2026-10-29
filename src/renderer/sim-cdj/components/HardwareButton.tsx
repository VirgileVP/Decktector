import type { CSSProperties } from 'react'

export type HardwareButtonShape = 'round' | 'rect' | 'pad'

interface HardwareButtonProps {
  label: string
  shape?: HardwareButtonShape
  lit?: boolean
  disabled?: boolean
  /** CSS color of the lit state; defaults to the stylesheet's accent. */
  litColor?: string
  title?: string
  onClick: () => void
}

/** A CDJ-style button: a lit state, a shape, and nothing else. */
export function HardwareButton({
  label,
  shape = 'rect',
  lit = false,
  disabled = false,
  litColor,
  title,
  onClick
}: HardwareButtonProps): React.JSX.Element {
  const style = litColor ? ({ '--lit-color': litColor } as CSSProperties) : undefined
  return (
    <button
      type="button"
      className={`hw-button hw-${shape}${lit ? ' hw-lit' : ''}`}
      style={style}
      disabled={disabled}
      title={title}
      aria-pressed={lit}
      onClick={onClick}
    >
      {label}
    </button>
  )
}
