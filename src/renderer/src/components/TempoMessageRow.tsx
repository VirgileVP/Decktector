import type { BpmRefresh, MessageIssue, OscMessageConfig, OscTarget } from '../model'
import {
  DECK_SOURCE_IDS,
  MESSAGE_TYPE_LABELS,
  REFRESH_OPTIONS,
  messageControls,
  optionValueToSource,
  sourceToOptionValue,
  withRefresh,
  withSource,
  withTargetToggled
} from '../lib/tempo-messages'
import { TempoIssueList } from './TempoIssueList'
import { TempoMessageFormat } from './TempoMessageFormat'

interface TempoSourceSelectProps {
  message: OscMessageConfig
  connectedDeckIds: ReadonlySet<number>
  onChange: (message: OscMessageConfig) => void
}

function TempoSourceSelect({
  message,
  connectedDeckIds,
  onChange
}: TempoSourceSelectProps): React.JSX.Element {
  return (
    <select
      className="setting-text-input tempo-select"
      aria-label="Source"
      value={sourceToOptionValue(message.source)}
      onChange={(event) => onChange(withSource(message, optionValueToSource(event.target.value)))}
    >
      <option value="master">Master</option>
      {DECK_SOURCE_IDS.map((deckId) => {
        const connected = connectedDeckIds.has(deckId)
        return (
          <option
            key={deckId}
            value={sourceToOptionValue({ type: 'deck', deckId })}
            className={connected ? undefined : 'tempo-option-offline'}
          >
            {connected ? `Deck ${deckId}` : `Deck ${deckId} — not connected`}
          </option>
        )
      })}
    </select>
  )
}

interface TempoTargetChipsProps {
  message: OscMessageConfig
  targets: readonly OscTarget[]
  onChange: (message: OscMessageConfig) => void
}

function TempoTargetChips({
  message,
  targets,
  onChange
}: TempoTargetChipsProps): React.JSX.Element {
  return (
    <div className="tempo-line tempo-chips" role="group" aria-label="Targets">
      {targets.map((target) => {
        const checked = message.targetIds.includes(target.id)
        const classes = [
          'tempo-chip',
          checked ? 'is-checked' : '',
          target.enabled ? '' : 'is-muted'
        ]
        return (
          <button
            key={target.id}
            type="button"
            className={classes.filter(Boolean).join(' ')}
            aria-pressed={checked}
            onClick={() => onChange(withTargetToggled(message, target.id))}
          >
            {target.name}
          </button>
        )
      })}
      {message.targetIds.length === 0 && (
        <span className="tempo-no-target setting-error">No target</span>
      )}
    </div>
  )
}

interface TempoMessageRowProps {
  message: OscMessageConfig
  targets: readonly OscTarget[]
  issues: readonly MessageIssue[]
  connectedDeckIds: ReadonlySet<number>
  onChange: (message: OscMessageConfig) => void
  onRemove: () => void
}

/** One OSC message: what it is, where its tempo comes from, its address and its targets. */
export function TempoMessageRow({
  message,
  targets,
  issues,
  connectedDeckIds,
  onChange,
  onRemove
}: TempoMessageRowProps): React.JSX.Element {
  const typeLabel = MESSAGE_TYPE_LABELS[message.kind.type]
  const controls = messageControls(message)
  const issueMessages = issues
    .filter((issue) => issue.messageId === message.id)
    .map((issue) => issue.message)

  return (
    <li className={message.enabled ? 'tempo-item' : 'tempo-item is-disabled'}>
      <div className="tempo-line">
        <input
          type="checkbox"
          className="setting-toggle"
          aria-label={`Enable ${typeLabel} message`}
          checked={message.enabled}
          onChange={(event) => onChange({ ...message, enabled: event.target.checked })}
        />
        <span className="tempo-message-type">{typeLabel}</span>
        <TempoSourceSelect
          message={message}
          connectedDeckIds={connectedDeckIds}
          onChange={onChange}
        />
        <input
          type="text"
          className="setting-text-input tempo-address"
          aria-label={`${typeLabel} OSC address`}
          placeholder="/osc/address"
          value={message.address}
          onChange={(event) => onChange({ ...message, address: event.target.value })}
        />
        <button
          type="button"
          className="tempo-icon-btn"
          aria-label={`Delete ${typeLabel} message`}
          title="Delete message"
          onClick={onRemove}
        >
          ×
        </button>
      </div>
      {(controls.refresh || controls.format) && (
        <div className="tempo-line">
          <TempoMessageFormat message={message} onChange={onChange} />
          {controls.refresh && (
            <select
              className="setting-text-input tempo-select"
              aria-label="Refresh"
              value={refreshOf(message)}
              onChange={(event) => onChange(withRefresh(message, event.target.value as BpmRefresh))}
            >
              {REFRESH_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          )}
        </div>
      )}
      <TempoTargetChips message={message} targets={targets} onChange={onChange} />
      <TempoIssueList messages={issueMessages} />
    </li>
  )
}

/** The refresh policy of a BPM or custom message. */
function refreshOf(message: OscMessageConfig): BpmRefresh {
  const { kind } = message
  return kind.type === 'bpm' || kind.type === 'custom' ? kind.refresh : 'onChange'
}
