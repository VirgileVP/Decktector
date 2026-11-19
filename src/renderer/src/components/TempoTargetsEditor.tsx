import type { OscTarget, TargetIssue } from '../model'
import { TempoDraftNumberInput } from './TempoDraftNumberInput'
import { TempoIssueList } from './TempoIssueList'

interface TempoTargetRowProps {
  target: OscTarget
  issues: readonly string[]
  onChange: (target: OscTarget) => void
  onRemove: () => void
}

function TempoTargetRow({
  target,
  issues,
  onChange,
  onRemove
}: TempoTargetRowProps): React.JSX.Element {
  return (
    <li className={target.enabled ? 'tempo-item' : 'tempo-item is-disabled'}>
      <div className="tempo-line">
        <input
          type="checkbox"
          className="setting-toggle"
          aria-label={`Enable ${target.name}`}
          checked={target.enabled}
          onChange={(event) => onChange({ ...target, enabled: event.target.checked })}
        />
        <input
          type="text"
          className="setting-text-input tempo-target-name"
          aria-label="Target name"
          value={target.name}
          onChange={(event) => onChange({ ...target, name: event.target.value })}
        />
        <input
          type="text"
          className="setting-text-input tempo-target-host"
          aria-label={`${target.name} IPv4 address`}
          placeholder="192.168.1.255"
          value={target.host}
          onChange={(event) => onChange({ ...target, host: event.target.value })}
        />
        <TempoDraftNumberInput
          className="tempo-target-port"
          label={`${target.name} port`}
          placeholder="8000"
          value={target.port}
          onCommit={(port) => onChange({ ...target, port })}
        />
        <button
          type="button"
          className="tempo-icon-btn"
          aria-label={`Delete ${target.name}`}
          title="Delete target"
          onClick={onRemove}
        >
          ×
        </button>
      </div>
      <TempoIssueList messages={issues} />
    </li>
  )
}

interface TempoTargetsEditorProps {
  targets: readonly OscTarget[]
  issues: readonly TargetIssue[]
  onAdd: () => void
  onChange: (target: OscTarget) => void
  onRemove: (targetId: string) => void
}

/** The receivers OSC can go to: a name, an IPv4 host and a UDP port each. */
export function TempoTargetsEditor({
  targets,
  issues,
  onAdd,
  onChange,
  onRemove
}: TempoTargetsEditorProps): React.JSX.Element {
  return (
    <div className="tempo-card">
      <div className="tempo-card-head">
        <span className="setting-label">Targets</span>
        <button type="button" className="setting-btn" onClick={onAdd}>
          Add target
        </button>
      </div>
      {targets.length === 0 ? (
        <p className="setting-hint tempo-empty">
          <strong>No targets yet</strong> — add the lighting or VJ software that should receive the
          OSC. A broadcast address such as 192.168.1.255 reaches the whole subnet.
        </p>
      ) : (
        <ul className="tempo-list">
          {targets.map((target) => (
            <TempoTargetRow
              key={target.id}
              target={target}
              issues={issues
                .filter((issue) => issue.targetId === target.id)
                .map((issue) => issue.message)}
              onChange={onChange}
              onRemove={() => onRemove(target.id)}
            />
          ))}
        </ul>
      )}
    </div>
  )
}
