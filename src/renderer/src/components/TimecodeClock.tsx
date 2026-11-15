import { useTimecodeStore } from '../state/timecode'

function pad2(value: number): string {
  return String(value).padStart(2, '0')
}

/**
 * `HH:MM:SS:FF` readout of the timecode actually being generated, in the
 * standard fluorescent green of a TC signal — fixed regardless of theme, the
 * same way a hardware TC display never follows the room around it.
 *
 * Always present in the header, not just while running: a dash placeholder
 * when idle is what makes the feature discoverable in the first place. While
 * the source deck is paused the held position stays readable, without the
 * live glow, since no timecode streams then.
 */
export function TimecodeClock(): React.JSX.Element {
  const status = useTimecodeStore()

  if (!status.running) {
    return <time className="timecode-clock timecode-clock-idle">--:--:--:--</time>
  }

  const { hours, minutes, seconds, frames } = status
  return (
    <time className={status.paused ? 'timecode-clock timecode-clock-paused' : 'timecode-clock'}>
      {pad2(hours)}:{pad2(minutes)}:{pad2(seconds)}:{pad2(frames)}
    </time>
  )
}
