import {
  bringOnline,
  CDJStatus,
  DeviceType,
  type ConnectedProlinkNetwork,
  type ProlinkNetwork
} from 'prolink-connect'

import { BaseEngine } from '../engine'
import type { DeckId } from '../types'
import { normalizeStatus } from './normalize'
import { PositionTracker } from './position'
import { PrecisePositionListener } from './precise-position'
import { loadTrack } from './track-loader'

/**
 * Real Pro DJ Link engine backed by prolink-connect.
 *
 * The virtual CDJ announces itself with the default ID (5), which stays
 * clear of the slots XDJ/CDJ hardware self-assigns (the XDJ-XZ always
 * claims 1 and 2 for its own decks) while remaining inside the 1-6 range
 * CDJs accept metadata queries from.
 */
export class ProlinkNetworkEngine extends BaseEngine {
  private network: ProlinkNetwork | null = null
  private positions = new Map<DeckId, PositionTracker>()
  /** Track id last seen per deck, to detect load/unload transitions. */
  private knownTrackIds = new Map<DeckId, number>()
  private precisePositions = new PrecisePositionListener()

  async start(): Promise<void> {
    this.updateNetworkState('searching')
    try {
      const network = await bringOnline()
      this.network = network

      network.deviceManager.on('connected', (device) => {
        if (device.type !== DeviceType.CDJ) return
        this.deviceConnected({
          deckId: device.id,
          name: device.name,
          ip: device.ip.address
        })
      })
      network.deviceManager.on('disconnected', (device) => {
        if (device.type !== DeviceType.CDJ) return
        this.positions.delete(device.id)
        this.knownTrackIds.delete(device.id)
        this.deviceDisconnected({
          deckId: device.id,
          name: device.name,
          ip: device.ip.address
        })
      })

      // Waits for the first device to appear to pick the right interface
      await network.autoconfigFromPeers()
      network.connect()

      if (!network.isConnected()) {
        this.updateNetworkState('failed')
        return
      }

      this.updateNetworkState('connected')
      network.statusEmitter.on('status', (status) => this.handleStatus(network, status))
      this.precisePositions.start((sample) =>
        this.trackerFor(sample.deckId).setPrecisePosition(sample.positionMs, Date.now())
      )
    } catch (error) {
      console.error('Failed to bring Pro DJ Link network online:', error)
      this.updateNetworkState('failed')
    }
  }

  async stop(): Promise<void> {
    this.precisePositions.stop()
    await this.network?.disconnect()
    this.network = null
    this.updateNetworkState('offline')
  }

  private handleStatus(network: ConnectedProlinkNetwork, status: CDJStatus.State): void {
    const deckId = status.deviceId
    const tracker = this.trackerFor(deckId)

    if (this.knownTrackIds.get(deckId) !== status.trackId) {
      this.knownTrackIds.set(deckId, status.trackId)
      tracker.reset()
      if (status.trackId === 0) {
        this.updateDeckTrack(deckId, null)
      } else {
        void this.fetchTrack(network, deckId, status)
      }
    }

    const now = Date.now()
    this.updateDeckState(normalizeStatus(status, tracker.update(status, now), now))
  }

  private async fetchTrack(
    network: ConnectedProlinkNetwork,
    deckId: DeckId,
    status: CDJStatus.State
  ): Promise<void> {
    // Show the deck as loading while metadata is fetched
    this.updateDeckTrack(deckId, null)
    try {
      const track = await loadTrack(network, status)
      // The deck may have moved on to another track while we were fetching
      if (this.knownTrackIds.get(deckId) !== status.trackId) return
      this.updateDeckTrack(deckId, track)
      this.trackerFor(deckId).setBeatGrid(track?.beatGrid ?? null)
    } catch (error) {
      console.error(`Failed to load track ${status.trackId} for deck ${deckId}:`, error)
    }
  }

  private trackerFor(deckId: DeckId): PositionTracker {
    let tracker = this.positions.get(deckId)
    if (!tracker) {
      tracker = new PositionTracker()
      this.positions.set(deckId, tracker)
    }
    return tracker
  }
}
