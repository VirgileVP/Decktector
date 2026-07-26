import { create } from 'zustand'
import { persist } from 'zustand/middleware'

import { MINUTES_PER_HOUR, createSlot, type LineupSlot } from '../lib/lineup'

const LINEUP_STORAGE_KEY = 'decktective.lineup'

/** `[hour, minute, artist]` of the placeholder night shown before anything is entered. */
const DEFAULT_SET_TIMES: readonly (readonly [number, number, string])[] = [
  [20, 0, 'Artist 1'],
  [21, 0, 'Artist 2'],
  [22, 30, 'Artist 3'],
  [0, 0, 'Artist 4'],
  [2, 0, 'Artist 5']
] as const

const DEFAULT_END_MINUTES = 4 * MINUTES_PER_HOUR

function defaultSlots(): LineupSlot[] {
  return DEFAULT_SET_TIMES.map(([hours, minutes, artist]) => ({
    ...createSlot(),
    artist,
    startMinutes: hours * MINUTES_PER_HOUR + minutes
  }))
}

interface LineupState {
  slots: LineupSlot[]
  /** When the last set stops; null leaves the night open-ended. */
  endMinutes: number | null
}

interface LineupActions {
  addSlot(): void
  updateSlot(id: string, patch: Partial<Omit<LineupSlot, 'id'>>): void
  removeSlot(id: string): void
  setEndMinutes(minutes: number | null): void
  clear(): void
}

/** Version 1 closed the night with a nameless slot instead of its own field. */
interface LegacySlot {
  artist: string
  startMinutes: number
}

/**
 * Rebuilds a version 1 lineup: nameless slots were terminators, and rows had no
 * identity of their own because the editor was a single block of text.
 */
function migrateFromV1(state: { slots?: LegacySlot[] }): LineupState {
  const legacy = state.slots ?? []
  const terminators = legacy.filter((slot) => slot.artist.trim().length === 0)

  return {
    slots: legacy
      .filter((slot) => slot.artist.trim().length > 0)
      .map((slot) => ({ ...createSlot(), artist: slot.artist, startMinutes: slot.startMinutes })),
    endMinutes: terminators.length === 0 ? null : terminators[terminators.length - 1].startMinutes
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
      endMinutes: DEFAULT_END_MINUTES,
      addSlot: () => set((state) => ({ slots: [...state.slots, createSlot()] })),
      updateSlot: (id, patch) =>
        set((state) => ({
          slots: state.slots.map((slot) => (slot.id === id ? { ...slot, ...patch } : slot))
        })),
      removeSlot: (id) => set((state) => ({ slots: state.slots.filter((slot) => slot.id !== id) })),
      setEndMinutes: (endMinutes) => set({ endMinutes }),
      clear: () => set({ slots: [], endMinutes: null })
    }),
    {
      name: LINEUP_STORAGE_KEY,
      version: 2,
      migrate: (persisted, version) =>
        version < 2
          ? migrateFromV1(persisted as { slots?: LegacySlot[] })
          : (persisted as LineupState),
      // Actions are not state: keeping them out avoids writing dead keys to storage.
      partialize: (state) => ({ slots: state.slots, endMinutes: state.endMinutes })
    }
  )
)
