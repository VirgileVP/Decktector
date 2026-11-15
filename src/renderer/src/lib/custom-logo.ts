/**
 * The operator's own header logo, kept as a `data:` URL so it travels with the
 * rest of the settings (storage, the other windows, the remote view) without a
 * file on disk. `null` means the built-in NEON logo.
 */
export type CustomLogo = string | null

/** Image types the picker accepts — what every webview renders in an `<img>`. */
export const CUSTOM_LOGO_MIME_TYPES = [
  'image/png',
  'image/jpeg',
  'image/webp',
  'image/gif',
  'image/svg+xml'
] as const

/** Largest image file accepted. Settings live in `localStorage` and are
 *  re-sent to every window on each change, so a logo has to stay small. */
export const CUSTOM_LOGO_MAX_BYTES = 512 * 1024

/** Base64 grows data by 4/3; the prefix is the longest `data:<type>;base64,`. */
const BASE64_GROWTH = 4 / 3
const DATA_URL_PREFIX_ALLOWANCE = 64
const CUSTOM_LOGO_MAX_DATA_URL_LENGTH =
  Math.ceil(CUSTOM_LOGO_MAX_BYTES * BASE64_GROWTH) + DATA_URL_PREFIX_ALLOWANCE

const DATA_URL_PATTERN = /^data:([a-z0-9.+-]+\/[a-z0-9.+-]+);base64,[A-Za-z0-9+/]+=*$/

export function isAcceptedLogoType(mimeType: string): boolean {
  return (CUSTOM_LOGO_MIME_TYPES as readonly string[]).includes(mimeType)
}

/** Validates a stored logo: a base64 `data:` URL of an accepted image type,
 *  within the size cap. Anything else falls back to the built-in logo. */
export function parseCustomLogo(raw: unknown): CustomLogo {
  if (typeof raw !== 'string' || raw.length > CUSTOM_LOGO_MAX_DATA_URL_LENGTH) return null
  const match = DATA_URL_PATTERN.exec(raw)
  if (match === null || !isAcceptedLogoType(match[1])) return null
  return raw
}
