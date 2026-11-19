import { describe, expect, it } from 'vitest'

import {
  createCustomValue,
  createMessage,
  defaultAddress,
  messageControls,
  optionValueToSource,
  sourceToOptionValue,
  withCustomFields,
  withRefresh,
  withSource,
  withTargetToggled,
  withoutTarget
} from './tempo-messages'

const deck3 = { type: 'deck', deckId: 3 } as const

describe('createMessage', () => {
  it('builds a master BPM message with its default address and no target', () => {
    expect(createMessage('m', 'bpm')).toEqual({
      id: 'm',
      enabled: true,
      source: { type: 'master' },
      kind: { type: 'bpm', refresh: 'onChangeAndEverySecond' },
      address: '/decktective/master/bpm',
      targetIds: []
    })
  })

  it('gives bar start and each beat their addresses', () => {
    expect(createMessage('m', 'barStart').address).toBe('/decktective/master/bar')
    expect(createMessage('m', 'eachBeat').address).toBe('/decktective/master/beat')
  })

  it('starts a custom message empty, sending a raw float on BPM', () => {
    const message = createMessage('m', 'custom')
    expect(message.address).toBe('')
    expect(message.kind).toEqual({
      type: 'custom',
      trigger: 'bpm',
      value: { type: 'floatRaw' },
      bpmDecimals: 2,
      refresh: 'onChangeAndEverySecond'
    })
  })
})

describe('defaultAddress', () => {
  it('follows the deck for deck sources', () => {
    expect(defaultAddress('bpm', deck3)).toBe('/decktective/deck/3/bpm')
    expect(defaultAddress('barStart', deck3)).toBe('/decktective/deck/3/bar')
    expect(defaultAddress('eachBeat', deck3)).toBe('/decktective/deck/3/beat')
  })
})

describe('withSource', () => {
  it('moves the default address along with the source', () => {
    const moved = withSource(createMessage('m', 'bpm'), deck3)
    expect(moved.source).toEqual(deck3)
    expect(moved.address).toBe('/decktective/deck/3/bpm')
    expect(withSource(moved, { type: 'master' }).address).toBe('/decktective/master/bpm')
  })

  it('never overwrites an address edited by hand', () => {
    const edited = { ...createMessage('m', 'bpm'), address: '/my/tempo' }
    expect(withSource(edited, deck3).address).toBe('/my/tempo')
  })

  it('leaves a custom address alone, empty or not', () => {
    expect(withSource(createMessage('m', 'custom'), deck3).address).toBe('')
    const typed = { ...createMessage('m', 'custom'), address: '/x' }
    expect(withSource(typed, deck3).address).toBe('/x')
  })
})

describe('source option values', () => {
  it('round-trips master and decks', () => {
    for (const source of [{ type: 'master' } as const, deck3]) {
      expect(optionValueToSource(sourceToOptionValue(source))).toEqual(source)
    }
  })

  it('reads anything unknown as the master', () => {
    expect(optionValueToSource('deck:x')).toEqual({ type: 'master' })
    expect(optionValueToSource('')).toEqual({ type: 'master' })
  })
})

describe('target checks', () => {
  it('toggles a target on and off', () => {
    const checked = withTargetToggled(createMessage('m', 'bpm'), 't1')
    expect(checked.targetIds).toEqual(['t1'])
    expect(withTargetToggled(checked, 't1').targetIds).toEqual([])
  })

  it('unchecks a deleted target everywhere', () => {
    const a = { ...createMessage('a', 'bpm'), targetIds: ['t1', 't2'] }
    const b = createMessage('b', 'bpm')
    const [cleanA, cleanB] = withoutTarget([a, b], 't1')
    expect(cleanA.targetIds).toEqual(['t2'])
    expect(cleanB).toBe(b)
  })
})

describe('withRefresh and withCustomFields', () => {
  it('changes the refresh of BPM and custom messages', () => {
    expect(withRefresh(createMessage('m', 'bpm'), 'onChange').kind).toEqual({
      type: 'bpm',
      refresh: 'onChange'
    })
    const custom = withRefresh(createMessage('m', 'custom'), 'onChange')
    expect(custom.kind).toMatchObject({ type: 'custom', refresh: 'onChange' })
  })

  it('leaves messages without a refresh or custom fields untouched', () => {
    const bar = createMessage('m', 'barStart')
    expect(withRefresh(bar, 'onChange')).toBe(bar)
    expect(withCustomFields(bar, { bpmDecimals: 0 })).toBe(bar)
  })

  it('patches custom fields', () => {
    const custom = withCustomFields(createMessage('m', 'custom'), { trigger: 'eachBeat' })
    expect(custom.kind).toMatchObject({ trigger: 'eachBeat', bpmDecimals: 2 })
  })
})

describe('createCustomValue', () => {
  it('gives each value its default parameters', () => {
    expect(createCustomValue('floatNormalized')).toEqual({
      type: 'floatNormalized',
      min: 0,
      max: 300
    })
    expect(createCustomValue('template')).toEqual({ type: 'template', template: '{bpm}' })
    expect(createCustomValue('noArgument')).toEqual({ type: 'noArgument' })
  })
})

describe('messageControls', () => {
  it('shows only the refresh for a BPM message', () => {
    expect(messageControls(createMessage('m', 'bpm'))).toEqual({
      refresh: true,
      format: false,
      bpmDecimals: false
    })
  })

  it('shows nothing extra for bar start and each beat', () => {
    for (const type of ['barStart', 'eachBeat'] as const) {
      expect(messageControls(createMessage('m', type))).toEqual({
        refresh: false,
        format: false,
        bpmDecimals: false
      })
    }
  })

  it('shows the format controls and, on a BPM trigger, refresh and decimals', () => {
    expect(messageControls(createMessage('m', 'custom'))).toEqual({
      refresh: true,
      format: true,
      bpmDecimals: true
    })
  })

  it('hides refresh and decimals for a beat trigger, unless the template uses {bpm}', () => {
    const beat = withCustomFields(createMessage('m', 'custom'), { trigger: 'eachBeat' })
    expect(messageControls(beat)).toEqual({ refresh: false, format: true, bpmDecimals: false })

    const withBpm = withCustomFields(beat, {
      value: { type: 'template', template: 'beat {beat} at {bpm}' }
    })
    expect(messageControls(withBpm).bpmDecimals).toBe(true)

    const withoutBpm = withCustomFields(beat, {
      value: { type: 'template', template: 'beat {beat}' }
    })
    expect(messageControls(withoutBpm).bpmDecimals).toBe(false)
  })
})
