import { BEATS_PER_BAR } from '../lib/constants'

const BEAT_NUMBERS = Array.from({ length: BEATS_PER_BAR }, (_, index) => index + 1)

/** One box per beat of the bar, the current one lit (downbeat in the accent colour). */
export function BeatBoxes({ beatInMeasure }: { beatInMeasure: number | null }): React.JSX.Element {
  return (
    <div className="beat-boxes" aria-label="Beat in bar">
      {BEAT_NUMBERS.map((beat) => (
        <span
          key={beat}
          className={`beat-box${beat === beatInMeasure ? ' beat-box-lit' : ''}${beat === 1 ? ' beat-box-downbeat' : ''}`}
        >
          {beat}
        </span>
      ))}
    </div>
  )
}
