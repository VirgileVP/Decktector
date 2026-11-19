import type {
  BpmRefresh,
  CustomValue,
  MessageKind,
  OscMessageConfig,
  TempoSource,
  Trigger
} from '../model'
import { MAX_PLAYER_NUMBER } from './deck-labels'
import { isRecord } from './is-record'

export type MessageType = MessageKind['type']
export type CustomValueType = CustomValue['type']
type CustomKind = Extract<MessageKind, { type: 'custom' }>

/** Address root every default address starts with. */
const DEFAULT_ADDRESS_ROOT = '/decktective'

const DEFAULT_REFRESH: BpmRefresh = 'onChangeAndEverySecond'
const DEFAULT_BPM_DECIMALS = 2
export const BPM_DECIMAL_OPTIONS: readonly number[] = [0, 1, 2]

/** Range a normalized float maps to 0..1 from, until the user picks another. */
const DEFAULT_NORMALIZED_MIN = 0
const DEFAULT_NORMALIZED_MAX = 300
const DEFAULT_TEMPLATE = '{bpm}'

export const MESSAGE_TYPE_LABELS: Record<MessageType, string> = {
  bpm: 'BPM',
  barStart: 'Bar start',
  eachBeat: 'Each beat',
  custom: 'Custom'
}

/** Entries of the "Add message" menu, in order. */
export const ADD_MESSAGE_CHOICES: readonly { type: MessageType; label: string; hint: string }[] = [
  { type: 'bpm', label: 'BPM', hint: 'The tempo, as a float' },
  { type: 'barStart', label: 'Bar start', hint: 'Beat 1 of each bar' },
  { type: 'eachBeat', label: 'Each beat', hint: 'The beat number, on every beat' },
  { type: 'custom', label: 'Custom message', hint: 'Pick the trigger and the argument' }
]

export const REFRESH_OPTIONS: readonly { value: BpmRefresh; label: string }[] = [
  { value: 'onChange', label: 'On change' },
  { value: 'onChangeAndEverySecond', label: 'On change + every second' }
]

export const TRIGGER_OPTIONS: readonly { value: Trigger; label: string }[] = [
  { value: 'bpm', label: 'BPM' },
  { value: 'barStart', label: 'Bar start' },
  { value: 'eachBeat', label: 'Each beat' }
]

export const CUSTOM_VALUE_OPTIONS: readonly { value: CustomValueType; label: string }[] = [
  { value: 'floatRaw', label: 'Float' },
  { value: 'floatNormalized', label: 'Float normalized' },
  { value: 'int', label: 'Int' },
  { value: 'template', label: 'Text template' },
  { value: 'noArgument', label: 'No argument' }
]

/** Variables a text template understands, for the hint under the field. */
export const TEMPLATE_VARIABLES = ['{bpm}', '{beat}', '{deck}'] as const

export const MASTER_SOURCE: TempoSource = { type: 'master' }

/** Player numbers a message can be bound to. */
export const DECK_SOURCE_IDS: readonly number[] = Array.from(
  { length: MAX_PLAYER_NUMBER },
  (_, index) => index + 1
)

export function createMessageKind(type: MessageType): MessageKind {
  switch (type) {
    case 'bpm':
      return { type: 'bpm', refresh: DEFAULT_REFRESH }
    case 'barStart':
      return { type: 'barStart' }
    case 'eachBeat':
      return { type: 'eachBeat' }
    case 'custom':
      return {
        type: 'custom',
        trigger: 'bpm',
        value: { type: 'floatRaw' },
        bpmDecimals: DEFAULT_BPM_DECIMALS,
        refresh: DEFAULT_REFRESH
      }
  }
}

const DEFAULT_ADDRESS_LEAVES: Record<Exclude<MessageType, 'custom'>, string> = {
  bpm: 'bpm',
  barStart: 'bar',
  eachBeat: 'beat'
}

/** The address a message of `type` gets for `source`; custom messages have none. */
export function defaultAddress(type: MessageType, source: TempoSource): string {
  if (type === 'custom') return ''
  const scope = source.type === 'master' ? 'master' : `deck/${source.deckId}`
  return `${DEFAULT_ADDRESS_ROOT}/${scope}/${DEFAULT_ADDRESS_LEAVES[type]}`
}

/** A new message: master source, enabled, default address, and no target checked. */
export function createMessage(id: string, type: MessageType): OscMessageConfig {
  return {
    id,
    enabled: true,
    source: MASTER_SOURCE,
    kind: createMessageKind(type),
    address: defaultAddress(type, MASTER_SOURCE),
    targetIds: []
  }
}

/**
 * Binds `message` to `source`. The default address follows, but only while it
 * is still the one the previous source gave — an address typed by hand stays.
 */
export function withSource(message: OscMessageConfig, source: TempoSource): OscMessageConfig {
  const followsDefault = message.address === defaultAddress(message.kind.type, message.source)
  return {
    ...message,
    source,
    address: followsDefault ? defaultAddress(message.kind.type, source) : message.address
  }
}

const DECK_OPTION_PREFIX = 'deck:'

/** Value of the source `<select>`: `master` or `deck:<n>`. */
export function sourceToOptionValue(source: TempoSource): string {
  return source.type === 'master' ? 'master' : `${DECK_OPTION_PREFIX}${source.deckId}`
}

export function optionValueToSource(value: string): TempoSource {
  if (!value.startsWith(DECK_OPTION_PREFIX)) return MASTER_SOURCE
  const deckId = Number(value.slice(DECK_OPTION_PREFIX.length))
  return Number.isInteger(deckId) ? { type: 'deck', deckId } : MASTER_SOURCE
}

/** The message with `targetId` checked or unchecked. */
export function withTargetToggled(message: OscMessageConfig, targetId: string): OscMessageConfig {
  const checked = message.targetIds.includes(targetId)
  return {
    ...message,
    targetIds: checked
      ? message.targetIds.filter((id) => id !== targetId)
      : [...message.targetIds, targetId]
  }
}

/** `messages` with a deleted target no longer checked anywhere. */
export function withoutTarget(
  messages: readonly OscMessageConfig[],
  targetId: string
): OscMessageConfig[] {
  return messages.map((message) =>
    message.targetIds.includes(targetId)
      ? { ...message, targetIds: message.targetIds.filter((id) => id !== targetId) }
      : message
  )
}

/** The message with its custom fields patched; a message of another type is returned untouched. */
export function withCustomFields(
  message: OscMessageConfig,
  patch: Partial<Omit<CustomKind, 'type'>>
): OscMessageConfig {
  return message.kind.type === 'custom'
    ? { ...message, kind: { ...message.kind, ...patch } }
    : message
}

/** The message with its BPM refresh policy changed; only BPM and custom messages have one. */
export function withRefresh(message: OscMessageConfig, refresh: BpmRefresh): OscMessageConfig {
  if (message.kind.type === 'bpm') return { ...message, kind: { ...message.kind, refresh } }
  return withCustomFields(message, { refresh })
}

/** A value of `type` with sensible fields, e.g. the default range for a normalized float. */
export function createCustomValue(type: CustomValueType): CustomValue {
  switch (type) {
    case 'floatRaw':
      return { type: 'floatRaw' }
    case 'floatNormalized':
      return { type: 'floatNormalized', min: DEFAULT_NORMALIZED_MIN, max: DEFAULT_NORMALIZED_MAX }
    case 'int':
      return { type: 'int' }
    case 'template':
      return { type: 'template', template: DEFAULT_TEMPLATE }
    case 'noArgument':
      return { type: 'noArgument' }
  }
}

/** Which optional controls a message row shows. */
export interface MessageControls {
  refresh: boolean
  /** Trigger and argument with its parameters — custom messages only. */
  format: boolean
  bpmDecimals: boolean
}

export function messageControls(message: OscMessageConfig): MessageControls {
  const { kind } = message
  if (kind.type !== 'custom') {
    return { refresh: kind.type === 'bpm', format: false, bpmDecimals: false }
  }
  const templateUsesBpm = kind.value.type === 'template' && kind.value.template.includes('{bpm}')
  return {
    refresh: kind.trigger === 'bpm',
    format: true,
    bpmDecimals: kind.trigger === 'bpm' || templateUsesBpm
  }
}

const REFRESHES: readonly BpmRefresh[] = REFRESH_OPTIONS.map((option) => option.value)
const TRIGGERS: readonly Trigger[] = TRIGGER_OPTIONS.map((option) => option.value)

function parseRefresh(raw: unknown): BpmRefresh | null {
  return REFRESHES.find((refresh) => refresh === raw) ?? null
}

function parseTrigger(raw: unknown): Trigger | null {
  return TRIGGERS.find((trigger) => trigger === raw) ?? null
}

/** Narrows an untrusted value to a source; `null` when it is not exactly the current shape. */
export function parseTempoSource(raw: unknown): TempoSource | null {
  if (!isRecord(raw)) return null
  if (raw.type === 'master') return MASTER_SOURCE
  if (raw.type === 'deck' && typeof raw.deckId === 'number' && Number.isInteger(raw.deckId)) {
    return { type: 'deck', deckId: raw.deckId }
  }
  return null
}

function parseCustomValue(raw: unknown): CustomValue | null {
  if (!isRecord(raw)) return null
  switch (raw.type) {
    case 'floatRaw':
    case 'int':
    case 'noArgument':
      return { type: raw.type }
    case 'floatNormalized':
      return typeof raw.min === 'number' && typeof raw.max === 'number'
        ? { type: 'floatNormalized', min: raw.min, max: raw.max }
        : null
    case 'template':
      return typeof raw.template === 'string' ? { type: 'template', template: raw.template } : null
    default:
      return null
  }
}

function parseCustomKind(raw: Record<string, unknown>): CustomKind | null {
  const trigger = parseTrigger(raw.trigger)
  const refresh = parseRefresh(raw.refresh)
  const value = parseCustomValue(raw.value)
  if (
    trigger === null ||
    refresh === null ||
    value === null ||
    typeof raw.bpmDecimals !== 'number'
  ) {
    return null
  }
  return { type: 'custom', trigger, value, bpmDecimals: raw.bpmDecimals, refresh }
}

/** Narrows an untrusted value to a message kind; `null` when it is not exactly the current shape. */
export function parseMessageKind(raw: unknown): MessageKind | null {
  if (!isRecord(raw)) return null
  switch (raw.type) {
    case 'bpm': {
      const refresh = parseRefresh(raw.refresh)
      return refresh === null ? null : { type: 'bpm', refresh }
    }
    case 'barStart':
      return { type: 'barStart' }
    case 'eachBeat':
      return { type: 'eachBeat' }
    case 'custom':
      return parseCustomKind(raw)
    default:
      return null
  }
}
