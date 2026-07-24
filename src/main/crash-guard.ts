/**
 * Keeps the process alive when something outside our control throws.
 *
 * The Pro DJ Link parsers in prolink-connect throw on any datagram that does
 * not start with the protocol header, directly inside the UDP `message`
 * handler — a single stray packet on ports 50000-50002 would otherwise take
 * the whole app down. A visualizer running in a booth is best-effort by
 * nature: staying up with a degraded view always beats vanishing mid-set.
 *
 * Nothing is swallowed silently — every failure is logged so it can be
 * diagnosed after the show.
 */
export function installCrashGuard(): void {
  process.on('uncaughtException', (error) => {
    console.error('Uncaught exception, kept running:', error)
  })

  process.on('unhandledRejection', (reason) => {
    console.error('Unhandled promise rejection, kept running:', reason)
  })
}
