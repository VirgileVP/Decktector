import type { OscMessageConfig, OscTarget } from '../model'
import { isRecord } from './is-record'
import { parseMessageKind, parseTempoSource } from './tempo-messages'

/** Where a freshly added target points: this machine, on the port MadMapper listens on by default. */
export const NEW_TARGET_HOST = '127.0.0.1'
export const NEW_TARGET_PORT = 8000

/** The output ships off. */
export const DEFAULT_TEMPO_OUTPUT_ENABLED = false

/** A new enabled target named "Target N", N being the first number no other target's name uses. */
export function createTarget(id: string, existing: readonly OscTarget[]): OscTarget {
  const names = new Set(existing.map((target) => target.name))
  let number = existing.length + 1
  while (names.has(`Target ${number}`)) number += 1
  return {
    id,
    name: `Target ${number}`,
    host: NEW_TARGET_HOST,
    port: NEW_TARGET_PORT,
    enabled: true
  }
}

function parseTarget(raw: unknown): OscTarget | null {
  if (
    !isRecord(raw) ||
    typeof raw.id !== 'string' ||
    typeof raw.name !== 'string' ||
    typeof raw.host !== 'string' ||
    typeof raw.port !== 'number' ||
    typeof raw.enabled !== 'boolean'
  ) {
    return null
  }
  return { id: raw.id, name: raw.name, host: raw.host, port: raw.port, enabled: raw.enabled }
}

function parseMessage(raw: unknown): OscMessageConfig | null {
  if (
    !isRecord(raw) ||
    typeof raw.id !== 'string' ||
    typeof raw.enabled !== 'boolean' ||
    typeof raw.address !== 'string' ||
    !Array.isArray(raw.targetIds) ||
    !raw.targetIds.every((id) => typeof id === 'string')
  ) {
    return null
  }
  const source = parseTempoSource(raw.source)
  const kind = parseMessageKind(raw.kind)
  if (source === null || kind === null) return null
  return {
    id: raw.id,
    enabled: raw.enabled,
    source,
    kind,
    address: raw.address,
    targetIds: [...new Set<string>(raw.targetIds)]
  }
}

/** Keeps the first entry of each id: ids are what messages and issues refer to. */
function uniqueById<T extends { id: string }>(entries: readonly T[]): T[] {
  const seen = new Set<string>()
  return entries.filter((entry) => {
    if (seen.has(entry.id)) return false
    seen.add(entry.id)
    return true
  })
}

/**
 * Rebuilds a target list from untrusted input (persisted storage, another
 * window's broadcast). Anything that is not exactly the current shape is
 * dropped; wrong values the user may be half-way through typing are kept.
 */
export function sanitizeOscTargets(raw: unknown): OscTarget[] {
  if (!Array.isArray(raw)) return []
  return uniqueById(raw.flatMap((entry) => parseTarget(entry) ?? []))
}

/** Same as {@link sanitizeOscTargets}, for messages; checked ids of missing targets are left for the backend to ignore. */
export function sanitizeOscMessages(raw: unknown): OscMessageConfig[] {
  if (!Array.isArray(raw)) return []
  return uniqueById(raw.flatMap((entry) => parseMessage(entry) ?? []))
}

/** The stored flag, or the default when storage held anything but a boolean. */
export function parseTempoOutputEnabled(raw: unknown): boolean {
  return typeof raw === 'boolean' ? raw : DEFAULT_TEMPO_OUTPUT_ENABLED
}

/** Settings keys of the previous tempo output, which are dropped rather than migrated. */
const LEGACY_TEMPO_SETTING_KEYS = [
  'tempoOutputAddressPrefix',
  'tempoOutputScope',
  'tempoOutputSendBeat'
] as const

/** `stored` without the keys the tempo output no longer has. */
export function omitLegacyTempoSettings<T extends object>(stored: T): T {
  const kept = { ...stored } as Record<string, unknown>
  for (const key of LEGACY_TEMPO_SETTING_KEYS) delete kept[key]
  return kept as T
}
