import { useEffect, useState } from 'react'

const MS_PER_SECOND = 1000

function pad2(value: number): string {
  return String(value).padStart(2, '0')
}

function formatClock(date: Date): string {
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`
}

/** Wall clock in the header, re-rendered on every second boundary. */
export function HeaderClock(): React.JSX.Element {
  const [now, setNow] = useState(() => new Date())

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout>

    /** Re-arms itself on the next whole second so the display never drifts. */
    const tick = (): void => {
      const current = new Date()
      setNow(current)
      timer = setTimeout(tick, MS_PER_SECOND - (current.getTime() % MS_PER_SECOND))
    }

    tick()
    return () => clearTimeout(timer)
  }, [])

  return <time className="header-clock">{formatClock(now)}</time>
}
