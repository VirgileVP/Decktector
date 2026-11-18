/**
 * `date`'s local calendar day as `YYYY-MM-DD`, for the names suggested when
 * a night's files are exported (`lineup-2026-10-09.json`).
 */
export function localDayStamp(date: Date): string {
  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, '0'),
    String(date.getDate()).padStart(2, '0')
  ].join('-')
}
