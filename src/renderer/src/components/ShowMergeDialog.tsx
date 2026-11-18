import { useEffect, useId, useRef } from 'react'

import { countLabel } from '../lib/count-label'
import type { ImportPlan } from '../lib/show-import-plan'

interface ShowMergeDialogProps {
  playlistName: string
  merge: ImportPlan
  replace: ImportPlan
  onMerge: () => void
  onReplace: () => void
  onCancel: () => void
}

/**
 * Asks how a playlist joins a show that already has rows, saying in numbers
 * what each choice does. Cancel is the focused default and changes nothing;
 * Escape and a backdrop click cancel too.
 */
export function ShowMergeDialog({
  playlistName,
  merge,
  replace,
  onMerge,
  onReplace,
  onCancel
}: ShowMergeDialogProps): React.JSX.Element {
  const cancelRef = useRef<HTMLButtonElement>(null)
  const titleId = useId()
  const bodyId = useId()

  useEffect(() => {
    cancelRef.current?.focus()
  }, [])

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
        className="confirm-panel"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        onClick={(event) => event.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <h3 id={titleId} className="confirm-title">
          Import {playlistName} into the show?
        </h3>
        <div id={bodyId} className="confirm-body">
          <p>
            <strong>Merge</strong> keeps the start timecodes and trims of the tracks the playlist
            still lists: {countLabel(merge.kept, 'row')} kept, {countLabel(merge.added, 'new row')},{' '}
            {countLabel(merge.absent, 'row')} marked absent (not deleted).
          </p>
          <p>
            <strong>Replace</strong> starts over: {countLabel(replace.dropped, 'row')} removed,{' '}
            {countLabel(replace.added, 'new row')} with blank starts.
          </p>
        </div>
        <div className="confirm-actions">
          <button ref={cancelRef} type="button" className="setting-btn" onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="setting-btn confirm-accept" onClick={onReplace}>
            Replace
          </button>
          <button type="button" className="setting-btn confirm-accept" onClick={onMerge}>
            Merge
          </button>
        </div>
      </div>
    </div>
  )
}
