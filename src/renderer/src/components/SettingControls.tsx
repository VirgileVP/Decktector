interface ToggleRowProps {
  label: string
  hint: string
  checked: boolean
  onChange: (checked: boolean) => void
}

/** Labelled on/off switch, one per line. */
export function ToggleRow({ label, hint, checked, onChange }: ToggleRowProps): React.JSX.Element {
  return (
    <label className="setting-row">
      <span className="setting-text">
        <span className="setting-label">{label}</span>
        <span className="setting-hint">{hint}</span>
      </span>
      <input
        type="checkbox"
        className="setting-toggle"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
    </label>
  )
}

interface ChoiceRowProps<T extends string> {
  label: string
  hint: string
  value: T
  options: readonly { value: T; label: string }[]
  onChange: (value: T) => void
}

/** Segmented picker for a handful of named alternatives, one per line. */
export function ChoiceRow<T extends string>({
  label,
  hint,
  value,
  options,
  onChange
}: ChoiceRowProps<T>): React.JSX.Element {
  return (
    <div className="setting-row">
      <span className="setting-text">
        <span className="setting-label">{label}</span>
        <span className="setting-hint">{hint}</span>
      </span>
      <div className="setting-choice">
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            className={option.value === value ? 'active' : ''}
            aria-pressed={option.value === value}
            onClick={() => onChange(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  )
}

interface TextRowProps {
  label: string
  hint: string
  value: string
  type?: 'text' | 'number'
  onChange: (value: string) => void
}

/** Single free-text or numeric field, one per line. */
export function TextRow({
  label,
  hint,
  value,
  type = 'text',
  onChange
}: TextRowProps): React.JSX.Element {
  return (
    <label className="setting-row">
      <span className="setting-text">
        <span className="setting-label">{label}</span>
        <span className="setting-hint">{hint}</span>
      </span>
      <input
        type={type}
        className="setting-text-input"
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  )
}

interface SliderRowProps {
  label: string
  hint: string
  value: number
  range: { min: number; max: number; step: number }
  /** Rendered next to the label, e.g. "120 %". */
  format: (value: number) => string
  onChange: (value: number) => void
}

export function SliderRow({
  label,
  hint,
  value,
  range,
  format,
  onChange
}: SliderRowProps): React.JSX.Element {
  return (
    <label className="setting-row setting-row-slider">
      <span className="setting-text">
        <span className="setting-label">
          {label}
          <span className="setting-value">{format(value)}</span>
        </span>
        <span className="setting-hint">{hint}</span>
      </span>
      <input
        type="range"
        className="setting-slider"
        min={range.min}
        max={range.max}
        step={range.step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  )
}
