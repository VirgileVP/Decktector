import { isTauri } from '@tauri-apps/api/core'
import { useState } from 'react'

import { countLabel } from '../lib/count-label'
import type { ShowData } from '../lib/show'
import { useShow } from '../state/show'
import { exportShowToFile, importShowFromFile } from '../state/show-transfer'
import { ConfirmDialog } from './ConfirmDialog'
import { useOnAirConfirmation } from './useOnAirConfirmation'

/** File dialogs are a desktop-shell feature; the remote view has no use for them. */
const IS_TAURI = isTauri()

type Status = { kind: 'info' | 'error'; text: string } | null

/** A show read from a file, waiting for the operator to agree to replace the current one. */
interface PendingImport {
  show: ShowData
  name: string
}

function fileNameOf(path: string): string {
  return path.split(/[\\/]/).pop() ?? path
}

/**
 * Saves the show to a file and loads one back, to move a night between
 * machines. Loading replaces the whole show, so it asks first when there is
 * one to lose, and again when a deck on air would change timecode.
 */
export function ShowFileActions(): React.JSX.Element | null {
  const show = useShow((state) => state.show)
  const [status, setStatus] = useState<Status>(null)
  const [pendingImport, setPendingImport] = useState<PendingImport | null>(null)
  const [busy, setBusy] = useState(false)
  const { confirmShowChange, dialog } = useOnAirConfirmation()

  if (!IS_TAURI) return null

  const reportError = (error: unknown): void => {
    setStatus({ kind: 'error', text: error instanceof Error ? error.message : String(error) })
  }

  const load = ({ show: imported, name }: PendingImport): void => {
    confirmShowChange(imported, () => {
      useShow.getState().load(imported, name)
      setStatus({ kind: 'info', text: `${name} loaded.` })
    })
  }

  const handleExport = async (): Promise<void> => {
    const current = useShow.getState().show
    if (current === null) return
    setBusy(true)
    try {
      const path = await exportShowToFile(current)
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
      const result = await importShowFromFile()
      if (result === null) return
      if (!result.ok) {
        setStatus({ kind: 'error', text: result.error })
        return
      }
      const imported = { show: result.show, name: 'Imported show' }
      if ((useShow.getState().show?.rows.length ?? 0) > 0) setPendingImport(imported)
      else load(imported)
    } catch (error) {
      reportError(error)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="lineup-file-actions">
      <button type="button" className="lineup-file-button" onClick={handleImport} disabled={busy}>
        Import show…
      </button>
      <button
        type="button"
        className="lineup-file-button"
        onClick={handleExport}
        disabled={busy || show === null}
      >
        Export show…
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
          title="Replace the current show?"
          confirmLabel="Replace"
          onConfirm={() => {
            load(pendingImport)
            setPendingImport(null)
          }}
          onCancel={() => setPendingImport(null)}
        >
          <p>
            The file holds {countLabel(pendingImport.show.rows.length, 'track')} at{' '}
            {pendingImport.show.fps} fps. It replaces every row, start timecode and trim in the show
            — export it first if you want to keep it.
          </p>
        </ConfirmDialog>
      )}
      {dialog}
    </div>
  )
}
