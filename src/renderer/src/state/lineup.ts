import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import {
  MINUTES_PER_DAY,
  MINUTES_PER_HOUR,
  axisFromClock,
  type AxisMinutes
} from '../lib/lineup-axis'
import {
  DOORS_OPEN_ID,
  END_SHOW_ID,
  createSlot,
  generateId,
  nextSlotOffset,
  type LineupSlot
} from '../lib/lineup'

const LINEUP_STORAGE_KEY = 'decktective.lineup'

/** `[day, hour, minute, artist]` of the placeholder night shown before anything is entered. */
const DEFAULT_SET_TIMES: readonly (readonly [0 | 1, number, number, string])[] = [
  [0, 20, 0, 'Artist 1'],
  [0, 21, 0, 'Artist 2'],
  [0, 22, 30, 'Artist 3'],
  [1, 0, 0, 'Artist 4'],
  [1, 2, 0, 'Artist 5']
] as const

const DEFAULT_END_OFFSET = axisFromClock(4 * MINUTES_PER_HOUR, 1)

function defaultSlots(): LineupSlot[] {
  return DEFAULT_SET_TIMES.map(([day, hours, minutes, artist]) => ({
    ...createSlot(),
    artist,
    startOffset: axisFromClock(hours * MINUTES_PER_HOUR + minutes, day)
  }))
}

/** A single boundary dragged to a new axis position. */
export interface BoundaryPatch {
  id: string
  minutes: AxisMinutes
}

interface LineupState {
  slots: LineupSlot[]
  /** When the last set stops; null leaves the night open-ended. */
  endOffset: AxisMinutes | null
  /** When doors open, ahead of the first set; null hides the marker. */
  doorsOffset: AxisMinutes | null
}

interface LineupActions {
  addSlot(): void
  updateSlot(id: string, patch: Partial<Omit<LineupSlot, 'id'>>): void
  removeSlot(id: string): void
  setEndOffset(minutes: AxisMinutes | null): void
  setDoorsOffset(minutes: AxisMinutes | null): void
  /** Applies a batch of boundary drags in one commit, so no intermediate state is published. */
  moveBoundaries(patches: readonly BoundaryPatch[]): void
  /** Inserts a new, unnamed set at `minutes`; returns its id so the caller can focus it. */
  addSlotAt(minutes: AxisMinutes): string
  clear(): void
}

/** Version 1 closed the night with a nameless slot instead of its own field. */
interface LegacySlot {
  artist: string
  startMinutes: number
}

/** Shape shared by versions 2 and 3: wall-clock minutes with the pre-axis rollover heuristic. */
interface V3LineupState {
  slots: { id: string; artist: string; startMinutes: number | null }[]
  endMinutes: number | null
  doorsOpenMinutes: number | null
}

/**
 * Rebuilds a version 1 lineup: nameless slots were terminators, and rows had no
 * identity of their own because the editor was a single block of text.
 */
function migrateFromV1(state: { slots?: LegacySlot[] }): V3LineupState {
  const legacy = state.slots ?? []
  const terminators = legacy.filter((slot) => slot.artist.trim().length === 0)

  return {
    slots: legacy
      .filter((slot) => slot.artist.trim().length > 0)
      .map((slot) => ({ id: generateId(), artist: slot.artist, startMinutes: slot.startMinutes })),
    endMinutes: terminators.length === 0 ? null : terminators[terminators.length - 1].startMinutes,
    doorsOpenMinutes: null
  }
}

/** Version 2 had no doors-open marker at all. */
function migrateFromV2(state: V3LineupState): V3LineupState {
  return { ...state, doorsOpenMinutes: null }
}

/** Version 3's bascule: anything before 06:00 belonged to the previous night. */
const V3_ROLLOVER_MINUTES = 6 * MINUTES_PER_HOUR

/** The v3 sort key is exactly the new axis position. */
function v3ToAxis(minutes: number | null): AxisMinutes | null {
  if (minutes === null) return null
  return minutes < V3_ROLLOVER_MINUTES ? minutes + MINUTES_PER_DAY : minutes
}

/**
 * Replaces the rollover heuristic with an explicit axis position, at the same
 * clock reading the user typed — the axis is not rebased to zero, since that
 * would change what was actually entered.
 */
function migrateFromV3(state: V3LineupState): LineupState {
  return {
    slots: state.slots.map((slot) => ({
      id: slot.id,
      artist: slot.artist,
      startOffset: v3ToAxis(slot.startMinutes)
    })),
    endOffset: v3ToAxis(state.endMinutes),
    doorsOffset: v3ToAxis(state.doorsOpenMinutes)
  }
}

/**
 * Tonight's running order. Kept apart from the settings store because it is gig
 * data, not a preference: resetting the appearance must not wipe it, and editing
 * it must not run the settings side effects.
 *
 * Starts on a placeholder night so the readout has something to show and the
 * editor something to overwrite. It does not expire on its own — last weekend's
 * lineup lights up again at its start time, and Clear empties it for good.
 */
export const useLineup = create<LineupState & LineupActions>()(
  persist(
    (set) => ({
      slots: defaultSlots(),
      endOffset: DEFAULT_END_OFFSET,
      doorsOffset: null,
      addSlot: () =>
        set((state) => {
          const startOffset = nextSlotOffset(state.slots, state.endOffset, state.doorsOffset)
          return { slots: [...state.slots, { ...createSlot(), startOffset }] }
        }),
      updateSlot: (id, patch) =>
        set((state) => ({
          slots: state.slots.map((slot) => (slot.id === id ? { ...slot, ...patch } : slot))
        })),
      removeSlot: (id) => set((state) => ({ slots: state.slots.filter((slot) => slot.id !== id) })),
      setEndOffset: (endOffset) => set({ endOffset }),
      setDoorsOffset: (doorsOffset) => set({ doorsOffset }),
      moveBoundaries: (patches) =>
        set((state) => {
          let nextSlots = state.slots
          let nextDoorsOffset = state.doorsOffset
          let nextEndOffset = state.endOffset
          let changed = false

          for (const patch of patches) {
            if (patch.id === DOORS_OPEN_ID) {
              if (patch.minutes === nextDoorsOffset) continue
              nextDoorsOffset = patch.minutes
              changed = true
            } else if (patch.id === END_SHOW_ID) {
              if (patch.minutes === nextEndOffset) continue
              nextEndOffset = patch.minutes
              changed = true
            } else {
              const index = nextSlots.findIndex((slot) => slot.id === patch.id)
              if (index === -1 || nextSlots[index].startOffset === patch.minutes) continue
              if (nextSlots === state.slots) nextSlots = [...state.slots]
              nextSlots[index] = { ...nextSlots[index], startOffset: patch.minutes }
              changed = true
            }
          }

          // Same reference back tells zustand (and persist) to skip the tick entirely.
          if (!changed) return state
          return { slots: nextSlots, doorsOffset: nextDoorsOffset, endOffset: nextEndOffset }
        }),
      addSlotAt: (minutes) => {
        const id = generateId()
        set((state) => ({ slots: [...state.slots, { id, artist: '', startOffset: minutes }] }))
        return id
      },
      clear: () => set({ slots: [], endOffset: null, doorsOffset: null })
    }),
    {
      name: LINEUP_STORAGE_KEY,
      version: 4,
      migrate: (persisted, version) => {
        let state = persisted as unknown
        if (version < 2) state = migrateFromV1(state as { slots?: LegacySlot[] })
        if (version < 3) state = migrateFromV2(state as V3LineupState)
        if (version < 4) state = migrateFromV3(state as V3LineupState)
        return state as LineupState
      },
      // Actions are not state: keeping them out avoids writing dead keys to storage.
      partialize: (state) => ({
        slots: state.slots,
        endOffset: state.endOffset,
        doorsOffset: state.doorsOffset
      })
    }
  )
)
