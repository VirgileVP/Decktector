import { create, type StoreApi, type UseBoundStore } from 'zustand'
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware'

import { isRecord } from '../lib/is-record'
import { createShow, isShowData, type ShowData, type ShowRow } from '../lib/show'
import type { ShowFps } from '../lib/show-timecode'
import { fillBlanks } from '../lib/show-validation'

const SHOW_STORAGE_KEY = 'decktector.show'
const SHOW_STORE_VERSION = 1

export interface ShowState {
  /** Tonight's show; null when none is loaded, which leaves timecode as it was before shows. */
  show: ShowData | null
  /** What the show was loaded from — a file or a playlist name — for the editor's header. */
  name: string | null
}

/** The fields of a row the editor changes; its track identity comes from imports only. */
export type ShowRowPatch = Partial<Pick<ShowRow, 'startTimecode' | 'trimMs' | 'setArtist'>>

interface ShowActions {
  /** Swaps the whole show for `show`, e.g. one read from a file. */
  load(show: ShowData, name: string | null): void
  /** Puts in the rows an import produced (`replaceRows` / `mergeRows`), creating the show if there is none. */
  applyImport(rows: ShowRow[]): void
  updateRow(id: string, patch: ShowRowPatch): void
  removeRow(id: string): void
  /** `timecode` is a label already validated against the show's fps. */
  setFallbackStart(timecode: string): void
  setCutWhenUnidentified(cut: boolean): void
  /** Keeps every start label as typed; the confirmation lives in the editor. */
  setFps(fps: ShowFps): void
  fillBlanks(stepFrames: number): void
  unload(): void
}

export type ShowStore = ShowState & ShowActions

const NO_SHOW: ShowState = { show: null, name: null }

/**
 * The steps from each stored version to the next: entry `n` turns a version
 * `n + 1` store into a version `n + 2` one. Version 1 is the first release,
 * so there is none yet; the next layout change adds its step here.
 */
const MIGRATIONS: readonly ((persisted: unknown) => unknown)[] = []

function migratePersistedShow(persisted: unknown, version: number): unknown {
  return MIGRATIONS.slice(Math.max(0, version - 1)).reduce(
    (migrated, migrate) => migrate(migrated),
    persisted
  )
}

/**
 * What storage held, as a state: storage is an outside boundary that an
 * older build or a hand edit may have left in another shape, and a show
 * that does not have this one is dropped rather than half-read.
 */
function readPersistedShow(persisted: unknown): ShowState {
  if (!isRecord(persisted)) return NO_SHOW
  return {
    show: isShowData(persisted.show) ? persisted.show : null,
    name: typeof persisted.name === 'string' ? persisted.name : null
  }
}

/**
 * `edit` applied to `state`'s show, or `state` itself when no show is loaded —
 * the same reference, which tells zustand (and persist) nothing changed.
 */
function editShow(state: ShowState, edit: (show: ShowData) => ShowData): Partial<ShowState> {
  return state.show === null ? state : { show: edit(state.show) }
}

/**
 * Builds the show store over the storage `getStorage` returns. The app uses
 * {@link useShow}, over `localStorage`; tests pass an in-memory storage to
 * reload a store from what another one wrote.
 */
export function createShowStore(
  getStorage: () => StateStorage
): UseBoundStore<StoreApi<ShowStore>> {
  return create<ShowStore>()(
    persist(
      (set) => ({
        ...NO_SHOW,
        load: (show, name) => set({ show, name }),
        applyImport: (rows) =>
          set((state) => ({
            show: state.show === null ? createShow(rows) : { ...state.show, rows }
          })),
        updateRow: (id, patch) =>
          set((state) => {
            if (state.show?.rows.some((row) => row.id === id) !== true) return state
            return editShow(state, (show) => ({
              ...show,
              rows: show.rows.map((row) => (row.id === id ? { ...row, ...patch } : row))
            }))
          }),
        removeRow: (id) =>
          set((state) => {
            if (state.show?.rows.some((row) => row.id === id) !== true) return state
            return editShow(state, (show) => ({
              ...show,
              rows: show.rows.filter((row) => row.id !== id)
            }))
          }),
        setFallbackStart: (fallbackStartTimecode) =>
          set((state) => editShow(state, (show) => ({ ...show, fallbackStartTimecode }))),
        setCutWhenUnidentified: (cutWhenUnidentified) =>
          set((state) => editShow(state, (show) => ({ ...show, cutWhenUnidentified }))),
        setFps: (fps) => set((state) => editShow(state, (show) => ({ ...show, fps }))),
        fillBlanks: (stepFrames) =>
          set((state) =>
            editShow(state, (show) => ({
              ...show,
              rows: fillBlanks(show.rows, stepFrames, show.fps)
            }))
          ),
        unload: () => set(NO_SHOW)
      }),
      {
        name: SHOW_STORAGE_KEY,
        version: SHOW_STORE_VERSION,
        storage: createJSONStorage(getStorage),
        // `merge` below checks the migrated state's shape before it is used.
        migrate: (persisted, version) => migratePersistedShow(persisted, version) as ShowStore,
        // Actions are not state: keeping them out avoids writing dead keys to storage.
        partialize: ({ show, name }): ShowState => ({ show, name }),
        merge: (persisted, current) => ({ ...current, ...readPersistedShow(persisted) })
      }
    )
  )
}

/**
 * Tonight's show, persisted so it survives a restart. Kept apart from the
 * lineup and the settings: it is gig data of its own, saved to its own file,
 * and only the main window edits it and pushes it to the timecode service.
 */
export const useShow = createShowStore(() => localStorage)
