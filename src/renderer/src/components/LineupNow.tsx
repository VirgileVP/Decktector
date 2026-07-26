import { buildLineup, formatClockMinutes, selectAroundNow, type LineupEntry } from '../lib/lineup'
import { useLineup } from '../state/lineup'
import { useSettings } from '../state/settings'
import { useClockMinutes } from '../state/wall-clock'

/**
 * The running order on one line under the header clock, either in full or
 * narrowed to the sets around now. Renders nothing outside the night, so the
 * header falls back to the clock alone.
 */
export function LineupNow(): React.JSX.Element | null {
  const slots = useLineup((s) => s.slots)
  const endMinutes = useLineup((s) => s.endMinutes)
  const lineupView = useSettings((s) => s.lineupView)
  const nowMinutes = useClockMinutes()

  const entries = buildLineup(slots, endMinutes, nowMinutes)
  const shown = lineupView === 'full' ? entries : selectAroundNow(entries)
  if (shown.length === 0) return null

  return (
    <div className="lineup-now">
      {shown.map((entry) => (
        <span key={entry.id} className={`lineup-set lineup-${entry.state}`}>
          <span className="lineup-set-time">{formatSetTimes(entry)}</span>
          <span className="lineup-set-artist" title={entry.artist}>
            {entry.artist}
          </span>
        </span>
      ))}
    </div>
  )
}

/** The set on air spells out its end; the others are bounded by their successor. */
function formatSetTimes(entry: LineupEntry): string {
  const start = formatClockMinutes(entry.startMinutes)
  if (entry.state !== 'current') return start
  const end = entry.endMinutes === null ? '' : formatClockMinutes(entry.endMinutes)
  return `${start}–${end}`
}
