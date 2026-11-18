import { useState } from 'react'

import { countLabel } from '../lib/count-label'
import type { ShowData } from '../lib/show'
import { readinessLabel } from '../lib/show-readiness'
import { summariseIssues } from '../lib/show-summary-view'
import { SHOW_FPS_OPTIONS, type ShowFps } from '../lib/show-timecode'
import {
  fpsChangeImpact,
  mergeSameStartDuplicates,
  type ShowAnalysis
} from '../lib/show-validation'
import { useShow } from '../state/show'
import { ConfirmDialog } from './ConfirmDialog'
import { FillBlanksControl } from './FillBlanksControl'

interface ShowSummaryProps {
  show: ShowData
  analysis: ShowAnalysis
  readOnly: boolean
}

/** Brings a row of the table into view and puts the caret in its start. */
function revealRow(rowId: string): void {
  const row = document.querySelector<HTMLElement>(`[data-row-id="${CSS.escape(rowId)}"]`)
  row?.scrollIntoView({ block: 'center' })
  row?.querySelector<HTMLInputElement>('input:not(:disabled)')?.focus()
}

/**
 * The strip above the table: Ready or how many conflicts stand in the way,
 * a count per kind of issue that jumps to the first offender, the show's
 * frame rate, Fill blanks, and Merge duplicates when some copies of a track
 * share a start.
 */
export function ShowSummary({ show, analysis, readOnly }: ShowSummaryProps): React.JSX.Element {
  const [pendingFps, setPendingFps] = useState<ShowFps | null>(null)
  const issues = summariseIssues(show.rows, analysis)
  const blankRows = show.rows.filter((row) => row.startTimecode === null).length
  const canMergeDuplicates = mergeSameStartDuplicates(show.rows).length < show.rows.length
  const impact = pendingFps === null ? null : fpsChangeImpact(show, pendingFps)

  const chooseFps = (fps: ShowFps): void => {
    if (fps === show.fps) return
    if (fpsChangeImpact(show, fps).rowsWithStart > 0) setPendingFps(fps)
    else useShow.getState().setFps(fps)
  }

  return (
    <div className="show-summary">
      <div className="show-summary-line">
        <span
          className={`show-readiness ${analysis.ready ? 'show-ready' : 'show-not-ready'}`}
          role="status"
        >
          {readinessLabel(analysis)}
        </span>
        {issues.map((issue) => (
          <button
            key={issue.kind}
            type="button"
            className={`show-badge show-badge-${issue.tone} show-summary-count`}
            title="Show the first one"
            onClick={() => revealRow(issue.firstRowId)}
          >
            {issue.rows} {issue.label}
          </button>
        ))}
      </div>
      {!readOnly && (
        <div className="show-summary-line">
          <span className="setting-choice" role="group" aria-label="Show frame rate">
            {SHOW_FPS_OPTIONS.map((fps) => (
              <button
                key={fps}
                type="button"
                className={fps === show.fps ? 'active' : undefined}
                aria-pressed={fps === show.fps}
                onClick={() => chooseFps(fps)}
              >
                {fps} fps
              </button>
            ))}
          </span>
          <FillBlanksControl fps={show.fps} blankRows={blankRows} />
          {canMergeDuplicates && (
            <button
              type="button"
              className="lineup-file-button"
              onClick={() => useShow.getState().mergeDuplicates()}
            >
              Merge duplicates
            </button>
          )}
        </div>
      )}
      {pendingFps !== null && impact !== null && (
        <ConfirmDialog
          title={`Run the show at ${pendingFps} fps?`}
          confirmLabel={`Switch to ${pendingFps} fps`}
          onConfirm={() => {
            useShow.getState().setFps(pendingFps)
            setPendingFps(null)
          }}
          onCancel={() => setPendingFps(null)}
        >
          <p>
            {countLabel(impact.rowsWithStart, 'start timecode')} keep their HH:MM:SS:FF labels as
            typed — nothing is converted.
          </p>
          {impact.invalidRowIds.length > 0 && (
            <p>
              {countLabel(impact.invalidRowIds.length, 'row')} would have frames too high for{' '}
              {pendingFps} fps and be flagged until corrected.
            </p>
          )}
        </ConfirmDialog>
      )}
    </div>
  )
}
