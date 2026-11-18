/**
 * When two descriptions name the same track: the rule a show row is matched
 * by, against a deck's track and against the show's other rows.
 *
 * The key is the normalised title and artist plus the duration within
 * {@link DURATION_TOLERANCE_SECONDS}. The timecode service applies the same
 * rule in Rust (`decktector-timecode`); both sides run the shared vectors in
 * `test-fixtures/track-identity-vectors.json`, which is what keeps them in
 * agreement. Change one side only together with the vectors.
 */

/** Durations come in whole seconds on both sides and may round differently. */
export const DURATION_TOLERANCE_SECONDS = 2

/** The five entities XML predefines; every other character comes as a numeric reference. */
const NAMED_ENTITIES: Readonly<Record<string, string>> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'"
}

const CHARACTER_REFERENCE = /&(?:#(\d+)|#[xX]([0-9a-fA-F]+)|(amp|lt|gt|quot|apos));/g

const MAX_CODE_POINT = 0x10ffff
const SURROGATE_FIRST = 0xd800
const SURROGATE_LAST = 0xdfff

/**
 * Unicode `White_Space`, the set Rust's `char::is_whitespace` uses. JavaScript's
 * own `\s` and `trim()` differ from it (U+FEFF in, U+0085 out), so neither is
 * used here.
 */
const WHITESPACE_RUN = /\p{White_Space}+/gu
const SPACE = ' '
/** Once runs are collapsed, at most one space is left at either end. */
const EDGE_SPACE = /^ | $/g

function decodeCodePoint(reference: string, digits: string, radix: number): string {
  const codePoint = Number.parseInt(digits, radix)
  const isScalarValue =
    codePoint <= MAX_CODE_POINT && (codePoint < SURROGATE_FIRST || codePoint > SURROGATE_LAST)
  return isScalarValue ? String.fromCodePoint(codePoint) : reference
}

/** Decodes XML character references and the named entities, in one pass. */
function decodeXmlReferences(text: string): string {
  return text.replace(
    CHARACTER_REFERENCE,
    (reference, decimal: string | undefined, hex: string | undefined, name: string | undefined) => {
      if (decimal !== undefined) return decodeCodePoint(reference, decimal, 10)
      if (hex !== undefined) return decodeCodePoint(reference, hex, 16)
      return name === undefined ? reference : NAMED_ENTITIES[name]
    }
  )
}

/**
 * The comparable form of a title or an artist: XML references decoded once,
 * Unicode NFC, lowercase, whitespace runs collapsed to one space and trimmed.
 * Nothing is stripped — "feat." and remix names stay part of the key.
 */
export function normaliseText(text: string): string {
  return decodeXmlReferences(text)
    .normalize('NFC')
    .toLowerCase()
    .replace(WHITESPACE_RUN, SPACE)
    .replace(EDGE_SPACE, '')
}

/** What a track is recognised by. Durations are whole seconds; 0 means unknown. */
export interface TrackIdentity {
  title: string
  artist: string
  durationSeconds: number
}

/** Separates title from artist in {@link identityKey}: no title or artist holds it. */
const KEY_SEPARATOR = '\u0000'

/**
 * The normalised title and artist of `track` as one string. Tracks that can
 * match share it, so it buckets candidates before durations are compared.
 */
export function identityKey(track: Pick<TrackIdentity, 'title' | 'artist'>): string {
  return `${normaliseText(track.title)}${KEY_SEPARATOR}${normaliseText(track.artist)}`
}

/**
 * Whether `a` and `b` describe the same track. An unknown duration never
 * matches, not even on an identical title and artist: without it a cover or
 * an edit is indistinguishable from the original.
 */
export function identityMatches(a: TrackIdentity, b: TrackIdentity): boolean {
  if (a.durationSeconds <= 0 || b.durationSeconds <= 0) return false
  return (
    Math.abs(a.durationSeconds - b.durationSeconds) <= DURATION_TOLERANCE_SECONDS &&
    normaliseText(a.title) === normaliseText(b.title) &&
    normaliseText(a.artist) === normaliseText(b.artist)
  )
}
