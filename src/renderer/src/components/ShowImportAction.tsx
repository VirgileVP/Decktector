import { isTauri } from '@tauri-apps/api/core'
import { useState } from 'react'

import { countLabel } from '../lib/count-label'
import { planImport, type ImportPlan } from '../lib/show-import-plan'
import type { OpenedRekordboxLibrary, RekordboxPlaylist } from '../model'
import { useShow } from '../state/show'
import { closeRekordboxXml, openRekordboxXml } from '../state/show-import'
import { ShowImportDialog } from './ShowImportDialog'
import { ShowMergeDialog } from './ShowMergeDialog'

/** File dialogs are a desktop-shell feature; the remote view has no use for them. */
const IS_TAURI = isTauri()

type Status = { kind: 'info' | 'error'; text: string } | null

/** A playlist read and waiting for the Merge / Replace choice. */
interface PendingImport {
  playlistName: string
  merge: ImportPlan
  replace: ImportPlan
}

/**
 * Imports a playlist from a rekordbox XML export into the show: open the
 * file, pick the playlist, then Merge or Replace when the show already has
 * rows. An empty show takes the playlist without asking. Cancelling at any
 * point leaves the show as it was.
 */
export function ShowImportAction(): React.JSX.Element | null {
  const [library, setLibrary] = useState<OpenedRekordboxLibrary | null>(null)
  const [pending, setPending] = useState<PendingImport | null>(null)
  const [status, setStatus] = useState<Status>(null)
  const [busy, setBusy] = useState(false)

  if (!IS_TAURI) return null

  const finish = (opened: OpenedRekordboxLibrary | null): void => {
    if (opened !== null) closeRekordboxXml(opened.token)
    setLibrary(null)
    setPending(null)
  }

  const apply = (plan: ImportPlan, playlistName: string): void => {
    useShow.getState().applyImport(plan.rows)
    setStatus({
      kind: 'info',
      text: `${playlistName} imported: ${countLabel(plan.rows.length, 'row')}.`
    })
    finish(library)
  }

  const handleOpen = async (): Promise<void> => {
    setBusy(true)
    try {
      const opened = await openRekordboxXml()
      if (opened !== null) setLibrary(opened)
    } catch (error) {
      setStatus({ kind: 'error', text: error instanceof Error ? error.message : String(error) })
    } finally {
      setBusy(false)
    }
  }

  const handlePlaylist = (playlistName: string, playlist: RekordboxPlaylist): void => {
    const existing = useShow.getState().show?.rows ?? []
    const merge = planImport(existing, playlist.tracks, 'merge')
    if (!merge.needsConfirmation) {
      apply(merge, playlistName)
      return
    }
    setPending({ playlistName, merge, replace: planImport(existing, playlist.tracks, 'replace') })
  }

  return (
    <div className="lineup-file-actions">
      <button type="button" className="lineup-file-button" onClick={handleOpen} disabled={busy}>
        Import rekordbox XML…
      </button>
      {status !== null && (
        <span
          className={`setting-hint${status.kind === 'error' ? ' lineup-warning' : ''}`}
          role="status"
        >
          {status.text}
        </span>
      )}
      {library !== null && pending === null && (
        <ShowImportDialog
          library={library}
          onImport={handlePlaylist}
          onCancel={() => finish(library)}
        />
      )}
      {pending !== null && (
        <ShowMergeDialog
          playlistName={pending.playlistName}
          merge={pending.merge}
          replace={pending.replace}
          onMerge={() => apply(pending.merge, pending.playlistName)}
          onReplace={() => apply(pending.replace, pending.playlistName)}
          onCancel={() => finish(library)}
        />
      )}
    </div>
  )
}
