import { useRef } from 'react'

import { lineupBlocks, slotDuration, sortSlots, type LineupSlot } from '../lib/lineup'
import {
  MINUTES_PER_HOUR,
  axisAfter,
  axisClock,
  axisDay,
  formatDuration,
  parseTimeInput,
  toggleAxisDay,
  toTimeInput,
  type AxisMinutes
} from '../lib/lineup-axis'
import { useLineup } from '../state/lineup'
import { LINEUP_VIEW_OPTIONS, useSettings } from '../state/settings'
import { LineupTimeline } from './LineupTimeline'
import { ChoiceRow } from './SettingControls'

/** A set stretching past this is very likely a day-inference mistake, not a real set. */
const SUSPICIOUS_BLOCK_MINUTES = 12 * MINUTES_PER_HOUR

/**
 * Mistakes the running order can hold that the inputs cannot prevent on their
 * own. Reported rather than blocked: a half-filled lineup is a normal state
 * while typing, it just must not pass unnoticed.
 */
function findProblems(
  slots: LineupSlot[],
  endOffset: AxisMinutes | null,
  doorsOffset: AxisMinutes | null
): string[] {
  const problems: string[] = []
  const timed = slots.filter((slot) => slot.startOffset !== null)

  if (timed.some((slot) => slot.artist.trim().length === 0)) {
    problems.push('A set has no artist yet.')
  }
  if (slots.some((slot) => slot.startOffset === null && slot.artist.trim().length > 0)) {
    problems.push('A set has no start time yet.')
  }
  if (new Set(timed.map((slot) => slot.startOffset)).size !== timed.length) {
    problems.push('Two sets start at the same time.')
  }

  const ordered = sortSlots(timed)
  const first = ordered[0]
  const last = ordered[timed.length - 1]

  if (endOffset !== null && last?.startOffset != null) {
    if (endOffset <= last.startOffset) {
      problems.push('The night ends before its last set starts.')
    }
  }
  if (doorsOffset !== null && first?.startOffset != null) {
    if (doorsOffset > first.startOffset) {
      problems.push('Doors open after the first set starts.')
    }
  }

  const blocks = lineupBlocks(slots, endOffset, doorsOffset)
  if (blocks.some((block) => (slotDuration(block) ?? 0) > SUSPICIOUS_BLOCK_MINUTES)) {
    problems.push('A set runs longer than 12 hours — check the +1 markers.')
  }

  return problems
}

/** The position, on the axis, that a line before `index` is already anchored to. */
function anchorBefore(
  ordered: LineupSlot[],
  index: number,
  doorsOffset: AxisMinutes | null
): AxisMinutes | null {
  for (let i = index - 1; i >= 0; i--) {
    if (ordered[i].startOffset !== null) return ordered[i].startOffset
  }
  return doorsOffset
}

interface DayBadgeProps {
  offset: AxisMinutes | null
  onToggle: (next: AxisMinutes) => void
}

/**
 * The escape hatch for the one case axis inference cannot resolve on its
 * own — typing a time meant for the show's second day into the first line.
 * Shown only when a row actually landed on day two, so it never distracts
 * from a straightforward night.
 */
function DayBadge({ offset, onToggle }: DayBadgeProps): React.JSX.Element | null {
  if (offset === null || axisDay(offset) === 0) return null
  return (
    <button
      type="button"
      className="lineup-day-badge"
      title="Landed on the show's second day — click to move it back a day"
      onClick={() => onToggle(toggleAxisDay(offset))}
    >
      +1
    </button>
  )
}

/**
 * The lineup is entered one set per row, with a native time field and a name.
 * Rows sort themselves chronologically as times are filled in, and each one
 * shows the length it works out to, so a wrong hour shows up as a wrong duration
 * instead of waiting until the night to be noticed. Under each row, an optional
 * note for the booth, which only the "Time" panel window displays. A rail above the rows shows
 * the same running order as draggable blocks on a two-day timeline.
 */
export function LineupEditor(): React.JSX.Element {
  const {
    slots,
    endOffset,
    doorsOffset,
    addSlot,
    updateSlot,
    removeSlot,
    setEndOffset,
    setDoorsOffset,
    clear
  } = useLineup()
  const lineupView = useSettings((s) => s.lineupView)
  const update = useSettings((s) => s.update)
  const nameInputs = useRef(new Map<string, HTMLInputElement>())

  const ordered = sortSlots(slots)
  const durations = new Map(
    lineupBlocks(slots, endOffset, doorsOffset, { includeUnnamed: true }).map((slot) => [
      slot.id,
      slotDuration(slot)
    ])
  )
  const problems = findProblems(slots, endOffset, doorsOffset)

  const focusSlot = (id: string): void => {
    requestAnimationFrame(() => nameInputs.current.get(id)?.focus())
  }

  return (
    <section className="settings-section">
      <div className="settings-section-head">
        <h3>Lineup</h3>
        <button
          type="button"
          className="settings-reset"
          onClick={clear}
          disabled={slots.length === 0 && endOffset === null && doorsOffset === null}
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

      <LineupTimeline onSlotAdded={focusSlot} />

      <div className="lineup-row lineup-row-end">
        <input
          type="time"
          className="lineup-time-field"
          value={toTimeInput(doorsOffset === null ? null : axisClock(doorsOffset))}
          aria-label="Doors opening"
          onChange={(event) => {
            const clock = parseTimeInput(event.target.value)
            setDoorsOffset(clock === null ? null : axisAfter(clock, null))
          }}
        />
        <span className="lineup-end-label">Doors opening</span>
        <DayBadge offset={doorsOffset} onToggle={setDoorsOffset} />
      </div>

      {ordered.length > 0 && (
        <div className="lineup-rows">
          {ordered.map((slot, index) => {
            const duration = durations.get(slot.id)
            const anchor = anchorBefore(ordered, index, doorsOffset)
            return (
              <div key={slot.id} className="lineup-slot">
                <div className="lineup-row">
                  <input
                    type="time"
                    className="lineup-time-field"
                    value={toTimeInput(
                      slot.startOffset === null ? null : axisClock(slot.startOffset)
                    )}
                    aria-label={`Start time of set ${index + 1}`}
                    onChange={(event) => {
                      const clock = parseTimeInput(event.target.value)
                      updateSlot(slot.id, {
                        startOffset: clock === null ? null : axisAfter(clock, anchor)
                      })
                    }}
                  />
                  <input
                    type="text"
                    className="lineup-name-field"
                    value={slot.artist}
                    placeholder="Artist"
                    spellCheck={false}
                    aria-label={`Artist of set ${index + 1}`}
                    ref={(element) => {
                      if (element) nameInputs.current.set(slot.id, element)
                      else nameInputs.current.delete(slot.id)
                    }}
                    onChange={(event) => updateSlot(slot.id, { artist: event.target.value })}
                  />
                  <span className="lineup-duration">
                    {duration === null || duration === undefined ? '' : formatDuration(duration)}
                  </span>
                  <DayBadge
                    offset={slot.startOffset}
                    onToggle={(next) => updateSlot(slot.id, { startOffset: next })}
                  />
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
                <input
                  type="text"
                  className="lineup-note-field"
                  value={slot.note}
                  placeholder="Note — shown in the Time window"
                  aria-label={`Note for set ${index + 1}`}
                  onChange={(event) => updateSlot(slot.id, { note: event.target.value })}
                />
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
          value={toTimeInput(endOffset === null ? null : axisClock(endOffset))}
          aria-label="End of the night"
          onChange={(event) => {
            const clock = parseTimeInput(event.target.value)
            const anchor = anchorBefore(ordered, ordered.length, doorsOffset)
            setEndOffset(clock === null ? null : axisAfter(clock, anchor))
          }}
        />
        <span className="lineup-end-label">End of the night</span>
        <DayBadge offset={endOffset} onToggle={setEndOffset} />
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
