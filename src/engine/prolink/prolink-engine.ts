import {
  bringOnline,
  CDJStatus,
  DeviceType,
  type ConnectedProlinkNetwork,
  type ProlinkNetwork
} from 'prolink-connect'

import { BaseEngine } from '../engine'
import type { DeckId } from '../types'
import { diag, slotName, trackTypeName } from './diagnostics'
import { installMediaSlotBypass } from './media-slot-bypass'
import { findInterfaceForPeer, requestedVcdjId } from './network-config'
import { normalizeStatus } from './normalize'
import { PositionTracker } from './position'
import { PrecisePositionListener } from './precise-position'
import { TrackLoader } from './track-loader'

/**
 * Real Pro DJ Link engine backed by prolink-connect.
 *
 * The virtual CDJ announces itself with the default ID (5), which stays clear
 * of the numbers XDJ/CDJ hardware self-assigns (the XDJ-XZ always claims 1
 * and 2 for its own decks). Players only answer media-slot queries and
 * metadata requests from numbers 1-4, though, so with a full four-player
 * network those routes are closed to us — see {@link installMediaSlotBypass}
 * for how the database is read anyway, and DECKTECTIVE_VCDJ_ID for taking a
 * number in range when one has been freed.
 */
export class ProlinkNetworkEngine extends BaseEngine {
  private network: ProlinkNetwork | null = null
  private positions = new Map<DeckId, PositionTracker>()
  /** Track id last seen per deck, to detect load/unload transitions. */
  private knownTrackIds = new Map<DeckId, number>()
  private precisePositions = new PrecisePositionListener()
  private trackLoader = new TrackLoader()

  async start(): Promise<void> {
    this.updateNetworkState('searching')
    try {
      const network = await bringOnline()
      this.network = network

      network.deviceManager.on('connected', (device) => {
        diag('device:connected', {
          id: device.id,
          name: device.name,
          ip: device.ip.address,
          type: DeviceType[device.type] ?? device.type
        })
        if (device.type !== DeviceType.CDJ) return
        this.deviceConnected({
          deckId: device.id,
          name: device.name,
          ip: device.ip.address
        })
      })
      network.deviceManager.on('disconnected', (device) => {
        diag('device:disconnected', { id: device.id, name: device.name })
        this.trackLoader.forgetDevice(device.id)
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
      this.applyVcdjIdOverride(network)
      network.connect()

      if (!network.isConnected()) {
        this.updateNetworkState('failed')
        return
      }

      this.updateNetworkState('connected')
      diag('network:connected')
      installMediaSlotBypass(network)
      // Media-slot replies are what the local (NFS) metadata route waits on
      // before it can download a player's database, so seeing them arrive —
      // or not — tells us which metadata route can work at all.
      network.statusEmitter.on('mediaSlot', (media) => {
        diag('media:slot', {
          device: media.deviceId,
          slot: slotName(media.slot),
          name: media.name,
          tracks: media.trackCount,
          tracksType: trackTypeName(media.tracksType)
        })
      })
      network.statusEmitter.on('status', (status) => this.handleStatus(network, status))
      this.precisePositions.start((sample) =>
        this.trackerFor(sample.deckId).setPrecisePosition(sample.positionMs, Date.now())
      )
    } catch (error) {
      console.error('Failed to bring Pro DJ Link network online:', error)
      diag('network:failed', { error })
      this.updateNetworkState('failed')
    }
  }

  /**
   * Re-announces the virtual CDJ under a player number given by the
   * environment, keeping the interface autoconfiguration just picked.
   */
  private applyVcdjIdOverride(network: ProlinkNetwork): void {
    const vcdjId = requestedVcdjId()
    if (vcdjId === null) return

    const peer = [...network.deviceManager.devices.values()][0]
    const iface = peer ? findInterfaceForPeer(peer.ip.address) : null
    if (!iface) {
      diag('network:vcdj-override-failed', {
        vcdjId,
        reason: 'no interface shares a subnet with the players'
      })
      return
    }

    network.configure({ iface, vcdjId })
    diag('network:vcdj-override', { vcdjId, iface: iface.address })
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
      const track = await this.trackLoader.load(network, status)
      // The deck may have moved on to another track while we were fetching
      if (this.knownTrackIds.get(deckId) !== status.trackId) {
        diag('track:superseded', { deck: deckId, trackId: status.trackId })
        return
      }
      if (!track) diag('track:unavailable', { deck: deckId, trackId: status.trackId })
      this.updateDeckTrack(deckId, track)
      this.trackerFor(deckId).setBeatGrid(track?.beatGrid ?? null)
    } catch (error) {
      console.error(`Failed to load track ${status.trackId} for deck ${deckId}:`, error)
      diag('track:load-failed', { deck: deckId, trackId: status.trackId, error })
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
