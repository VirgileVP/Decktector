import {
  formatDuration,
  nightOrder,
  parseTimeInput,
  resolveLineup,
  slotDuration,
  sortSlots,
  toTimeInput,
  type LineupSlot
} from '../lib/lineup'
import { useLineup } from '../state/lineup'
import { LINEUP_VIEW_OPTIONS, useSettings } from '../state/settings'
import { ChoiceRow } from './SettingControls'

/**
 * Mistakes the running order can hold that the inputs cannot prevent on their
 * own. Reported rather than blocked: a half-filled lineup is a normal state
 * while typing, it just must not pass unnoticed.
 */
function findProblems(slots: LineupSlot[], endMinutes: number | null): string[] {
  const problems: string[] = []
  const timed = slots.filter((slot) => slot.startMinutes !== null)

  if (timed.some((slot) => slot.artist.trim().length === 0)) {
    problems.push('A set has no artist yet.')
  }
  if (slots.some((slot) => slot.startMinutes === null && slot.artist.trim().length > 0)) {
    problems.push('A set has no start time yet.')
  }
  if (new Set(timed.map((slot) => slot.startMinutes)).size !== timed.length) {
    problems.push('Two sets start at the same time.')
  }

  const last = sortSlots(timed)[timed.length - 1]
  if (endMinutes !== null && last?.startMinutes != null) {
    if (nightOrder(endMinutes) <= nightOrder(last.startMinutes)) {
      problems.push('The night ends before its last set starts.')
    }
  }

  return problems
}

/**
 * The lineup is entered one set per row, with a native time field and a name.
 * Rows sort themselves chronologically as times are filled in, and each one
 * shows the length it works out to, so a wrong hour shows up as a wrong duration
 * instead of waiting until the night to be noticed.
 */
export function LineupEditor(): React.JSX.Element {
  const { slots, endMinutes, addSlot, updateSlot, removeSlot, setEndMinutes, clear } = useLineup()
  const lineupView = useSettings((s) => s.lineupView)
  const update = useSettings((s) => s.update)

  const ordered = sortSlots(slots)
  const durations = new Map(
    resolveLineup(slots, endMinutes).map((slot) => [slot.id, slotDuration(slot)])
  )
  const problems = findProblems(slots, endMinutes)

  return (
    <section className="settings-section">
      <div className="settings-section-head">
        <h3>Lineup</h3>
        <button
          type="button"
          className="settings-reset"
          onClick={clear}
          disabled={slots.length === 0 && endMinutes === null}
        >
          Clear
        </button>
      </div>

      <ChoiceRow
        label="Header readout"
        hint="How much of the running order sits under the clock"
        value={lineupView}
        options={LINEUP_VIEW_OPTIONS}
        onChange={(value) => update('lineupView', value)}
      />

      {ordered.length > 0 && (
        <div className="lineup-rows">
          {ordered.map((slot, index) => {
            const duration = durations.get(slot.id)
            return (
              <div key={slot.id} className="lineup-row">
                <input
                  type="time"
                  className="lineup-time-field"
                  value={toTimeInput(slot.startMinutes)}
                  aria-label={`Start time of set ${index + 1}`}
                  onChange={(event) =>
                    updateSlot(slot.id, { startMinutes: parseTimeInput(event.target.value) })
                  }
                />
                <input
                  type="text"
                  className="lineup-name-field"
                  value={slot.artist}
                  placeholder="Artist"
                  spellCheck={false}
                  aria-label={`Artist of set ${index + 1}`}
                  onChange={(event) => updateSlot(slot.id, { artist: event.target.value })}
                />
                <span className="lineup-duration">
                  {duration === null || duration === undefined ? '' : formatDuration(duration)}
                </span>
                <button
                  type="button"
                  className="lineup-remove"
                  title="Remove this set"
                  aria-label={`Remove set ${index + 1}`}
                  onClick={() => removeSlot(slot.id)}
                >
                  ×
                </button>
              </div>
            )
          })}
        </div>
      )}

      <button type="button" className="lineup-add" onClick={addSlot}>
        + Add a set
      </button>

      <div className="lineup-row lineup-row-end">
        <input
          type="time"
          className="lineup-time-field"
          value={toTimeInput(endMinutes)}
          aria-label="End of the night"
          onChange={(event) => setEndMinutes(parseTimeInput(event.target.value))}
        />
        <span className="lineup-end-label">End of the night</span>
      </div>

      {problems.length > 0 ? (
        <p className="setting-hint lineup-warning">{problems.join(' ')}</p>
      ) : (
        <p className="setting-hint">
          Each set runs until the next one starts. Leave the end open to keep the last set running.
        </p>
      )}
    </section>
  )
}
