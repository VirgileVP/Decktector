import type { MessageIssue, OscMessageConfig, OscTarget } from '../model'
import type { MessageType } from '../lib/tempo-messages'
import { AddTempoMessageMenu } from './AddTempoMessageMenu'
import { TempoMessageRow } from './TempoMessageRow'

interface TempoMessagesEditorProps {
  messages: readonly OscMessageConfig[]
  targets: readonly OscTarget[]
  issues: readonly MessageIssue[]
  connectedDeckIds: ReadonlySet<number>
  onAdd: (type: MessageType) => void
  onChange: (message: OscMessageConfig) => void
  onRemove: (messageId: string) => void
}

/** The OSC messages to send, each with its source, address and targets. */
export function TempoMessagesEditor({
  messages,
  targets,
  issues,
  connectedDeckIds,
  onAdd,
  onChange,
  onRemove
}: TempoMessagesEditorProps): React.JSX.Element {
  return (
    <div className="tempo-card">
      <div className="tempo-card-head">
        <span className="setting-label">Messages</span>
        <AddTempoMessageMenu onAdd={onAdd} />
      </div>
      {messages.length === 0 ? (
        <p className="setting-hint tempo-empty">
          <strong>No messages yet</strong> — add the BPM, the bar start or the beats, then check
          which targets receive each one.
        </p>
      ) : (
        <ul className="tempo-list">
          {messages.map((message) => (
            <TempoMessageRow
              key={message.id}
              message={message}
              targets={targets}
              issues={issues}
              connectedDeckIds={connectedDeckIds}
              onChange={onChange}
              onRemove={() => onRemove(message.id)}
            />
          ))}
        </ul>
      )}
    </div>
  )
}
