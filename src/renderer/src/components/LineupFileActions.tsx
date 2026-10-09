import { isTauri } from '@tauri-apps/api/core'
import { useState } from 'react'

import type { LineupData } from '../lib/lineup'
import { useLineup } from '../state/lineup'
import { exportLineupToFile, importLineupFromFile } from '../state/lineup-transfer'
import { ConfirmDialog } from './ConfirmDialog'

/** File dialogs are a desktop-shell feature; the remote view has no use for them. */
const IS_TAURI = isTauri()

type Status = { kind: 'info' | 'error'; text: string } | null

function fileNameOf(path: string): string {
  return path.split(/[\\/]/).pop() ?? path
}

function hasContent({ slots, endOffset, doorsOffset }: LineupData): boolean {
  return slots.length > 0 || endOffset !== null || doorsOffset !== null
}

/**
 * Saves the lineup to a file and loads one back, so a night can be prepared
 * on one machine and played on another. Loading replaces the whole lineup, so
 * it asks first whenever there is something to lose.
 */
export function LineupFileActions(): React.JSX.Element | null {
  const [status, setStatus] = useState<Status>(null)
  const [pendingImport, setPendingImport] = useState<LineupData | null>(null)
  const [busy, setBusy] = useState(false)

  if (!IS_TAURI) return null

  const reportError = (error: unknown): void => {
    setStatus({ kind: 'error', text: error instanceof Error ? error.message : String(error) })
  }

  const applyImport = (lineup: LineupData): void => {
    useLineup.getState().replace(lineup)
    setStatus({ kind: 'info', text: 'Lineup imported.' })
  }

  const handleExport = async (): Promise<void> => {
    setBusy(true)
    try {
      const { slots, endOffset, doorsOffset } = useLineup.getState()
      const path = await exportLineupToFile({ slots, endOffset, doorsOffset })
      if (path !== null) setStatus({ kind: 'info', text: `Saved to ${fileNameOf(path)}.` })
    } catch (error) {
      reportError(error)
    } finally {
      setBusy(false)
    }
  }

  const handleImport = async (): Promise<void> => {
    setBusy(true)
    try {
      const result = await importLineupFromFile()
      if (result === null) return
      if (!result.ok) {
        setStatus({ kind: 'error', text: result.error })
        return
      }
      if (hasContent(useLineup.getState())) setPendingImport(result.lineup)
      else applyImport(result.lineup)
    } catch (error) {
      reportError(error)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="lineup-file-actions">
      <button type="button" className="lineup-file-button" onClick={handleImport} disabled={busy}>
        Import…
      </button>
      <button type="button" className="lineup-file-button" onClick={handleExport} disabled={busy}>
        Export…
      </button>
      {status !== null && (
        <span
          className={`setting-hint${status.kind === 'error' ? ' lineup-warning' : ''}`}
          role="status"
        >
          {status.text}
        </span>
      )}
      {pendingImport !== null && (
        <ConfirmDialog
          title="Replace the current lineup?"
          confirmLabel="Replace"
          onConfirm={() => {
            applyImport(pendingImport)
            setPendingImport(null)
          }}
          onCancel={() => setPendingImport(null)}
        >
          <p>
            The imported file holds {pendingImport.slots.length}{' '}
            {pendingImport.slots.length === 1 ? 'set' : 'sets'}. It replaces every set, note and
            time currently in the lineup — export it first if you want to keep it.
          </p>
        </ConfirmDialog>
      )}
    </div>
  )
}
