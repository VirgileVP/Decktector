import { useEffect, useId, useRef, useState } from 'react'

import { countLabel } from '../lib/count-label'
import type { OpenedRekordboxLibrary, RekordboxPlaylist, RekordboxPlaylistNode } from '../model'
import { readRekordboxPlaylist } from '../state/show-import'

interface ShowImportDialogProps {
  library: OpenedRekordboxLibrary
  onImport: (playlistName: string, playlist: RekordboxPlaylist) => void
  onCancel: () => void
}

/** The chosen playlist, and what reading it gave. */
type Selection =
  | { path: number[]; name: string; state: 'loading' }
  | { path: number[]; name: string; state: 'loaded'; playlist: RekordboxPlaylist }
  | { path: number[]; name: string; state: 'failed'; error: string }

const pathKey = (path: readonly number[]): string => path.join('.')

interface TreeNodeProps {
  node: RekordboxPlaylistNode
  path: number[]
  expanded: ReadonlySet<string>
  selectedKey: string | null
  onToggle: (path: number[]) => void
  onSelect: (path: number[], name: string) => void
}

function TreeNode({
  node,
  path,
  expanded,
  selectedKey,
  onToggle,
  onSelect
}: TreeNodeProps): React.JSX.Element {
  const key = pathKey(path)
  if (node.kind === 'playlist') {
    return (
      <li>
        <button
          type="button"
          className="show-import-playlist"
          aria-pressed={selectedKey === key}
          onClick={() => onSelect(path, node.name)}
        >
          <span className="show-import-name">{node.name}</span>
          <span className="show-import-count">{countLabel(node.trackCount, 'track')}</span>
        </button>
      </li>
    )
  }

  const isOpen = expanded.has(key)
  return (
    <li>
      <button
        type="button"
        className="show-import-folder"
        aria-expanded={isOpen}
        onClick={() => onToggle(path)}
      >
        <span aria-hidden="true">{isOpen ? '▾' : '▸'}</span>
        <span className="show-import-name">{node.name}</span>
      </button>
      {isOpen && node.children.length > 0 && (
        <ul className="show-import-tree">
          {node.children.map((child, index) => (
            <TreeNode
              key={index}
              node={child}
              path={[...path, index]}
              expanded={expanded}
              selectedKey={selectedKey}
              onToggle={onToggle}
              onSelect={onSelect}
            />
          ))}
        </ul>
      )}
    </li>
  )
}

/**
 * Picks one playlist of a rekordbox export: folders open and close,
 * playlists show their track count, and once one is chosen the dialog says
 * how many of its entries the export's collection did not have.
 */
export function ShowImportDialog({
  library,
  onImport,
  onCancel
}: ShowImportDialogProps): React.JSX.Element {
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set())
  const [selection, setSelection] = useState<Selection | null>(null)
  const cancelRef = useRef<HTMLButtonElement>(null)
  const titleId = useId()

  useEffect(() => {
    cancelRef.current?.focus()
  }, [])

  const toggle = (path: number[]): void => {
    const key = pathKey(path)
    setExpanded((open) => {
      const next = new Set(open)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }

  const select = (path: number[], name: string): void => {
    setSelection({ path, name, state: 'loading' })
    readRekordboxPlaylist(library.token, path).then(
      (playlist) =>
        setSelection((current) =>
          current?.path === path ? { path, name, state: 'loaded', playlist } : current
        ),
      (error: Error) =>
        setSelection((current) =>
          current?.path === path ? { path, name, state: 'failed', error: error.message } : current
        )
    )
  }

  const handleKeyDown = (event: React.KeyboardEvent): void => {
    if (event.key !== 'Escape') return
    event.stopPropagation()
    onCancel()
  }

  return (
    <div
      className="confirm-backdrop"
      onClick={(event) => {
        event.stopPropagation()
        onCancel()
      }}
    >
      <div
        className="confirm-panel show-import-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <h3 id={titleId} className="confirm-title show-import-title">
          Import a playlist
        </h3>
        <p className="setting-hint">{library.fileName}</p>
        {library.tree.children.length === 0 ? (
          <p className="setting-hint">This export has no playlists.</p>
        ) : (
          <ul className="show-import-tree show-import-root">
            {library.tree.children.map((child, index) => (
              <TreeNode
                key={index}
                node={child}
                path={[index]}
                expanded={expanded}
                selectedKey={selection === null ? null : pathKey(selection.path)}
                onToggle={toggle}
                onSelect={select}
              />
            ))}
          </ul>
        )}
        <p className="setting-hint" role="status">
          {selection?.state === 'loading' && `Reading ${selection.name}…`}
          {selection?.state === 'loaded' &&
            `${selection.name}: ${countLabel(selection.playlist.tracks.length, 'track')}` +
              (selection.playlist.skipped > 0
                ? `, ${countLabel(selection.playlist.skipped, 'entry', 'entries')} not in the collection (skipped)`
                : '')}
          {selection?.state === 'failed' && (
            <span className="lineup-warning">{selection.error}</span>
          )}
        </p>
        <div className="confirm-actions">
          <button ref={cancelRef} type="button" className="setting-btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="setting-btn confirm-accept"
            disabled={selection?.state !== 'loaded'}
            onClick={() => {
              if (selection?.state === 'loaded') onImport(selection.name, selection.playlist)
            }}
          >
            Import
          </button>
        </div>
      </div>
    </div>
  )
}
