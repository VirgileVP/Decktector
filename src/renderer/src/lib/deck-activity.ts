/**
 * Decides whether a deck counts as "active" (live) for the UI — the glow versus
 * dimming, and whether the ON AIR light means anything.
 *
 * The Pro DJ Link on-air flag is set by a DJM mixer, so it is only meaningful
 * when exactly one Pro DJ Link mixer is on the network. With none (an analogue
 * mixer, or DJMs left off the network) it is always false; with two, both
 * announce device 33 and the channel-to-player mapping no longer matches the
 * cabling, so the flag contradicts itself. The operator can pick which signal
 * to trust; `auto` picks for them from the mixer count.
 */

import type { DeckState, DiscoveredDevice, PlayState } from '../model'

export const ON_AIR_SOURCES = ['auto', 'mixer', 'playback'] as const
/** What the operator picked: follow the mixer, follow playback, or let the app decide. */
export type OnAirSource = (typeof ON_AIR_SOURCES)[number]

/** What is actually trusted once `auto` has been resolved. */
export type EffectiveOnAirSource = Exclude<OnAirSource, 'auto'>

export const DEFAULT_ON_AIR_SOURCE: OnAirSource = 'auto'

export const ON_AIR_SOURCE_OPTIONS = [
  { value: 'auto', label: 'Automatic' },
  { value: 'mixer', label: 'Mixer' },
  { value: 'playback', label: 'Playback' }
] as const satisfies readonly { value: OnAirSource; label: string }[]

/** One line per mode, shown under the picker so the choice explains itself. */
export const ON_AIR_SOURCE_DESCRIPTIONS: Record<OnAirSource, string> = {
  auto: 'One linked mixer: its on-air flag. No mixer, or several: whether the deck is playing.',
  mixer: 'Always trust the on-air flag the CDJs report from a linked DJM mixer.',
  playback: 'A deck is live whenever it is playing, whatever the mixer says.'
}

/** From this many distinct mixers on the network the CDJ on-air flag stops being reliable. */
export const AMBIGUOUS_MIXER_COUNT = 2

/** Narrows an untrusted value (storage, another window) to a known source; anything else is `auto`. */
export function parseOnAirSource(raw: unknown): OnAirSource {
  return ON_AIR_SOURCES.find((source) => source === raw) ?? DEFAULT_ON_AIR_SOURCE
}

/**
 * Play states that make a deck audible on its own, and so "active" under
 * `playback`. Only `playing` qualifies — the engine already folds looping into
 * it. The rest are left out on purpose:
 * - `cued`: paused at the cue point, or cue-previewing (CUE held), which is
 *   normally monitored in the headphones, not sent to the floor;
 * - `searching`: fast-seeking or spun down, not programme material;
 * - `paused`, `ended`, `loading`, `empty`: silent or nothing loaded.
 */
const ACTIVE_PLAY_STATES: ReadonlySet<PlayState> = new Set<PlayState>(['playing'])

/**
 * Number of distinct mixers among `devices`. Counted by MAC address because two
 * mixers announce the same device number, and one box can show up under several
 * announcements; a device with no MAC falls back to its IP.
 */
export function countMixers(devices: readonly DiscoveredDevice[]): number {
  const identities = new Set<string>()
  for (const device of devices) {
    if (device.kind !== 'mixer') continue
    const mac = device.macAddress.trim().toLowerCase()
    identities.add(mac !== '' ? mac : device.ip)
  }
  return identities.size
}

/** Which signal the UI follows, given the operator's pick and how many mixers are around. */
export function resolveOnAirSource(source: OnAirSource, mixerCount: number): EffectiveOnAirSource {
  if (source !== 'auto') return source
  return mixerCount === 1 ? 'mixer' : 'playback'
}

/** Whether `auto` fell back to playback because several mixers make the CDJ flag unreliable. */
export function hasAmbiguousMixers(source: OnAirSource, mixerCount: number): boolean {
  return source === 'auto' && mixerCount >= AMBIGUOUS_MIXER_COUNT
}

/** Whether the deck counts as live. A deck the engine has not reported yet never does. */
export function isDeckActive(
  source: OnAirSource,
  deck: Pick<DeckState, 'playState' | 'isOnAir'> | undefined,
  mixerCount: number
): boolean {
  if (!deck) return false
  return resolveOnAirSource(source, mixerCount) === 'mixer'
    ? deck.isOnAir
    : ACTIVE_PLAY_STATES.has(deck.playState)
}
