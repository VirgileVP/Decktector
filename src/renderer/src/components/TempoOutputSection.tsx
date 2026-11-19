import { useEffect, useMemo } from 'react'

import { createMessage, withoutTarget, type MessageType } from '../lib/tempo-messages'
import { createTarget } from '../lib/tempo-output-config'
import { describeTempoStatus } from '../lib/tempo-status'
import type { OscMessageConfig, OscTarget } from '../model'
import { useSettings } from '../state/settings'
import { useDecktectorStore } from '../state/store'
import { connectTempoOutputStatus, useTempoOutputStore } from '../state/tempo-output'
import { ToggleRow } from './SettingControls'
import { TempoMessagesEditor } from './TempoMessagesEditor'
import { TempoTargetsEditor } from './TempoTargetsEditor'

function replaceById<T extends { id: string }>(entries: readonly T[], changed: T): T[] {
  return entries.map((entry) => (entry.id === changed.id ? changed : entry))
}

/** Settings of the OSC tempo output: which tempo messages go to which receivers. */
export function TempoOutputSection(): React.JSX.Element {
  const enabled = useSettings((s) => s.tempoOutputEnabled)
  const targets = useSettings((s) => s.tempoOutputTargets)
  const messages = useSettings((s) => s.tempoOutputMessages)
  const update = useSettings((s) => s.update)
  const status = useTempoOutputStore((s) => s.status)
  const devices = useDecktectorStore((s) => s.devices)

  useEffect(() => connectTempoOutputStatus(), [])

  const connectedDeckIds = useMemo(() => new Set(devices.map((device) => device.deckId)), [devices])

  const addTarget = (): void =>
    update('tempoOutputTargets', [...targets, createTarget(crypto.randomUUID(), targets)])
  const changeTarget = (target: OscTarget): void =>
    update('tempoOutputTargets', replaceById(targets, target))
  // One store write, so the backend gets a single coherent config instead of
  // a transient one where messages still point at the deleted target.
  const removeTarget = (targetId: string): void => {
    useSettings.setState({
      tempoOutputTargets: targets.filter((target) => target.id !== targetId),
      tempoOutputMessages: withoutTarget(messages, targetId)
    })
  }

  const addMessage = (type: MessageType): void =>
    update('tempoOutputMessages', [...messages, createMessage(crypto.randomUUID(), type)])
  const changeMessage = (message: OscMessageConfig): void =>
    update('tempoOutputMessages', replaceById(messages, message))
  const removeMessage = (messageId: string): void =>
    update(
      'tempoOutputMessages',
      messages.filter((message) => message.id !== messageId)
    )

  return (
    <section className="settings-section">
      <h3>BPM output (OSC)</h3>
      <ToggleRow
        label="BPM output (OSC)"
        hint={describeTempoStatus(status, { enabled, targets, messages })}
        checked={enabled}
        onChange={(checked) => update('tempoOutputEnabled', checked)}
      />
      <TempoTargetsEditor
        targets={targets}
        issues={status.targetIssues}
        onAdd={addTarget}
        onChange={changeTarget}
        onRemove={removeTarget}
      />
      <TempoMessagesEditor
        messages={messages}
        targets={targets}
        issues={status.messageIssues}
        connectedDeckIds={connectedDeckIds}
        onAdd={addMessage}
        onChange={changeMessage}
        onRemove={removeMessage}
      />
    </section>
  )
}
