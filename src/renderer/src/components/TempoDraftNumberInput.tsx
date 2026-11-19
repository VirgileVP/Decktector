import { useState } from 'react'

interface TempoDraftNumberInputProps {
  value: number
  label: string
  className?: string
  placeholder?: string
  onCommit: (value: number) => void
}

/**
 * Number field that keeps what is typed ("-", "0.") until it parses, and only
 * then hands a finite number up. Losing focus restores the committed value.
 */
export function TempoDraftNumberInput({
  value,
  label,
  className = '',
  placeholder,
  onCommit
}: TempoDraftNumberInputProps): React.JSX.Element {
  const [draft, setDraft] = useState<string | null>(null)

  const handleChange = (text: string): void => {
    setDraft(text)
    const parsed = Number(text)
    if (text.trim() !== '' && Number.isFinite(parsed)) onCommit(parsed)
  }

  return (
    <input
      type="number"
      className={`setting-text-input ${className}`}
      aria-label={label}
      placeholder={placeholder}
      value={draft ?? String(value)}
      onChange={(event) => handleChange(event.target.value)}
      onBlur={() => setDraft(null)}
    />
  )
}
