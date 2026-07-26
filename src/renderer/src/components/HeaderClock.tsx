import { useClockSeconds } from '../state/wall-clock'

/** Wall clock in the header, re-rendered on every second boundary. */
export function HeaderClock(): React.JSX.Element {
  return <time className="header-clock">{useClockSeconds()}</time>
}
