interface DeckStatProps {
  label: string
  value: string
  /** Optional line rendered below the value, e.g. time remaining. */
  sub?: React.ReactNode
  className?: string
}

/** A labelled stat cell: small caps label above a bold value, Denon-style. */
export function DeckStat({ label, value, sub, className }: DeckStatProps): React.JSX.Element {
  return (
    <div className={className ? `deck-stat ${className}` : 'deck-stat'}>
      <span className="deck-stat-label">{label}</span>
      <span className="deck-stat-value">{value}</span>
      {sub}
    </div>
  )
}
