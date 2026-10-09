import { formatClockMinutes } from '../lib/lineup-axis'
import { buildLineup, selectAroundNow, type LineupEntry } from '../lib/lineup'
import { useLineup } from '../state/lineup'
import { useSettings } from '../state/settings'
import { SECONDS_PER_MINUTE, useClockMinutesStepped } from '../state/wall-clock'

/** How often the progress bar moves on — finer than the minute the rest of the readout needs. */
const PROGRESS_REFRESH_SECONDS = 15

interface LineupNowProps {
  /** Draws how far the set on air has run under its name — the "Time" panel
   *  window shows it; the app header has no room to spare for it. */
  showProgress?: boolean
}

/**
 * The running order on one line under the header clock, either in full or
 * narrowed to the sets around now. Renders nothing outside the night, so the
 * header falls back to the clock alone.
 */
export function LineupNow({ showProgress = false }: LineupNowProps): React.JSX.Element | null {
  const slots = useLineup((s) => s.slots)
  const endOffset = useLineup((s) => s.endOffset)
  const doorsOffset = useLineup((s) => s.doorsOffset)
  const lineupView = useSettings((s) => s.lineupView)
  const nowMinutes = useClockMinutesStepped(
    showProgress ? PROGRESS_REFRESH_SECONDS : SECONDS_PER_MINUTE
  )

  const entries = buildLineup(slots, endOffset, nowMinutes, doorsOffset)
  const shown = lineupView === 'full' ? entries : selectAroundNow(entries)
  if (shown.length === 0) return null

  return (
    <div className="lineup-now">
      {shown.map((entry) => (
        <span key={entry.id} className={`lineup-set lineup-${entry.state}`}>
          {showProgress && entry.progress !== null ? (
            <span className="lineup-set-body">
              <span className="lineup-set-line">
                <SetLine entry={entry} />
              </span>
              <SetProgress progress={entry.progress} />
            </span>
          ) : (
            <SetLine entry={entry} />
          )}
        </span>
      ))}
    </div>
  )
}

/** A set's times and name, side by side. */
function SetLine({ entry }: { entry: LineupEntry }): React.JSX.Element {
  return (
    <>
      <span className="lineup-set-time">{formatSetTimes(entry)}</span>
      <span className="lineup-set-artist" title={entry.artist}>
        {entry.artist}
      </span>
    </>
  )
}

const PERCENT = 100

function SetProgress({ progress }: { progress: number }): React.JSX.Element {
  const percent = Math.round(progress * PERCENT)
  return (
    <span
      className="lineup-set-progress"
      role="progressbar"
      aria-label="Set progress"
      aria-valuemin={0}
      aria-valuemax={PERCENT}
      aria-valuenow={percent}
      style={{ ['--set-progress' as string]: String(progress) }}
    />
  )
}

/** The set on air spells out its end; the others are bounded by their successor. */
function formatSetTimes(entry: LineupEntry): string {
  const start = formatClockMinutes(entry.startOffset)
  if (entry.state !== 'current') return start
  const end = entry.endOffset === null ? '' : formatClockMinutes(entry.endOffset)
  return `${start}–${end}`
}
