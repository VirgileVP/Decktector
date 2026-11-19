interface TempoIssueListProps {
  messages: readonly string[]
}

/** Problems the backend reported for one target or message, in the danger color. */
export function TempoIssueList({ messages }: TempoIssueListProps): React.JSX.Element | null {
  if (messages.length === 0) return null
  return (
    <ul className="tempo-issues" role="alert">
      {messages.map((message, index) => (
        <li key={index} className="setting-hint setting-error">
          {message}
        </li>
      ))}
    </ul>
  )
}
