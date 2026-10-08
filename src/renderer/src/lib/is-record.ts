/** Whether `value` is a plain key/value object — the first check on anything read back from storage. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
