/** `count` and the noun in agreement with it: "1 track", "3 tracks". */
export function countLabel(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`
}
