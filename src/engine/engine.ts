import { EventEmitter } from 'events'

import type {
  DeckId,
  DeckState,
  DeviceInfo,
  EngineSnapshot,
  NetworkState,
  TrackInfo
} from './types'

/**
 * Events emitted by every engine implementation (real or simulated).
 * This map is the single contract between the engine layer and the app;
 * the IPC bridge forwards these events verbatim to the renderer.
 */
export interface EngineEvents {
  'network:state': [state: NetworkState]
  'device:connected': [device: DeviceInfo]
  'device:disconnected': [device: DeviceInfo]
  /** Live deck state, emitted on every status packet (~5 Hz) */
  'deck:state': [state: DeckState]
  /** Emitted once a newly loaded track is fully fetched (null = unloaded) */
  'deck:track': [deckId: DeckId, track: TrackInfo | null]
}

export const ENGINE_EVENTS = [
  'network:state',
  'device:connected',
  'device:disconnected',
  'deck:state',
  'deck:track'
] as const satisfies readonly (keyof EngineEvents)[]

/**
 * A source of Pro DJ Link deck data.
 *
 * Implementations: `prolink/ProlinkNetworkEngine` (real hardware) and
 * `sim/SimEngine` (scripted decks for development without hardware).
 */
export interface ProlinkEngine extends EventEmitter<EngineEvents> {
  start(): Promise<void>
  stop(): Promise<void>
  /** Current state of everything, for hydrating a fresh renderer. */
  getSnapshot(): EngineSnapshot
}

/**
 * Base class handling the snapshot bookkeeping shared by all engines:
 * implementations report through the `update*` helpers, which keep the
 * snapshot coherent and emit the matching event.
 */
export abstract class BaseEngine extends EventEmitter<EngineEvents> implements ProlinkEngine {
  private networkState: NetworkState = 'offline'
  private devices = new Map<DeckId, DeviceInfo>()
  private decks = new Map<DeckId, DeckState>()
  private tracks = new Map<DeckId, TrackInfo | null>()

  abstract start(): Promise<void>
  abstract stop(): Promise<void>

  getSnapshot(): EngineSnapshot {
    return {
      networkState: this.networkState,
      devices: [...this.devices.values()],
      decks: [...this.decks.values()],
      tracks: [...this.tracks.entries()].map(([deckId, track]) => ({ deckId, track }))
    }
  }

  protected updateNetworkState(state: NetworkState): void {
    this.networkState = state
    this.emit('network:state', state)
  }

  protected deviceConnected(device: DeviceInfo): void {
    this.devices.set(device.deckId, device)
    this.emit('device:connected', device)
  }

  protected deviceDisconnected(device: DeviceInfo): void {
    this.devices.delete(device.deckId)
    this.decks.delete(device.deckId)
    this.tracks.delete(device.deckId)
    this.emit('device:disconnected', device)
  }

  protected updateDeckState(state: DeckState): void {
    this.decks.set(state.deckId, state)
    this.emit('deck:state', state)
  }

  protected updateDeckTrack(deckId: DeckId, track: TrackInfo | null): void {
    this.tracks.set(deckId, track)
    this.emit('deck:track', deckId, track)
  }
}
