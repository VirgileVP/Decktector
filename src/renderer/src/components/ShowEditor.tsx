import { useMemo } from 'react'

import { countLabel } from '../lib/count-label'
import { analyseShow } from '../lib/show-validation'
import { isPanelWindow } from '../panels/params'
import { useShow } from '../state/show'
import { ShowImportAction } from './ShowImportAction'
import { ShowRowEditor } from './ShowRowEditor'

const NO_ISSUES = [] as const

/**
 * The show as a table: a row per track, its start timecode and trim
 * editable, its issues shown. Editable in the main window only; a Show panel
 * window mirrors it read-only, since only the main window pushes the show to
 * the timecode service.
 */
export function ShowEditor(): React.JSX.Element {
  const show = useShow((state) => state.show)
  const name = useShow((state) => state.name)
  const readOnly = isPanelWindow(window.location)
  const analysis = useMemo(() => (show === null ? null : analyseShow(show)), [show])

  if (show === null || show.rows.length === 0) {
    return (
      <div className="show-empty">
        <p className="setting-hint">No show — import a rekordbox playlist.</p>
        {!readOnly && <ShowImportAction />}
      </div>
    )
  }

  const { updateRow, removeRow } = useShow.getState()
  return (
    <div className="show-editor">
      <div className="show-editor-head">
        <span className="show-editor-name">{name ?? 'Show'}</span>
        <span className="setting-hint">
          {countLabel(show.rows.length, 'track')} · {show.fps} fps
        </span>
        {readOnly && <span className="setting-hint">Read-only here — edit in the main window</span>}
      </div>
      {!readOnly && <ShowImportAction />}
      <div className="show-table-scroll">
        <table className="show-table">
          <thead>
            <tr>
              <th scope="col" className="show-cell-position">
                #
              </th>
              <th scope="col">Title</th>
              <th scope="col">Artist</th>
              <th scope="col" className="show-cell-duration">
                Length
              </th>
              <th scope="col">Start</th>
              <th scope="col" title="+ = cues fire earlier">
                Trim
              </th>
              <th scope="col" aria-label="Issues" />
              <th scope="col" aria-label="Remove" />
            </tr>
          </thead>
          <tbody>
            {show.rows.map((row, index) => (
              <ShowRowEditor
                key={row.id}
                row={row}
                position={index + 1}
                fps={show.fps}
                issues={analysis?.byRow.get(row.id) ?? NO_ISSUES}
                readOnly={readOnly}
                onChange={(patch) => updateRow(row.id, patch)}
                onRemove={() => removeRow(row.id)}
              />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
