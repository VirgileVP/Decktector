/** Text for the deck readouts shared by the deck bar and the compact deck row. */

/** `+1.25%` / `-0.50%`, or a placeholder of the same width before the deck reports. */
export function formatPitch(pitch: number | null): string {
  if (pitch === null) return '--.--%'
  return `${pitch >= 0 ? '+' : ''}${pitch.toFixed(2)}%`
}

/** One decimal, or a placeholder of the same width before a BPM is known. */
export function formatBpm(bpm: number | null): string {
  return bpm !== null ? bpm.toFixed(1) : '--.-'
}
