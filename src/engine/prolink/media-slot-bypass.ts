import {
  MediaColor,
  TrackType,
  type ConnectedProlinkNetwork,
  type Device,
  type MediaSlot,
  type MediaSlotInfo
} from 'prolink-connect'

import { diag, slotName, trackTypeName, withTimeout } from './diagnostics'

type MediaSlotOptions = Parameters<ConnectedProlinkNetwork['statusEmitter']['queryMediaSlot']>[0]

/**
 * A player that answers this query does so immediately — it is a single
 * unicast round trip on the status port.
 */
const QUERY_TIMEOUT_MS = 3_000

/**
 * Lets the Rekordbox-database route work on players that ignore media-slot
 * queries.
 *
 * Reading analyzed media is a two-part job: a media-slot query on the status
 * port tells us what is in the slot, then the database itself is downloaded
 * from the player over NFS. Only the first part is gated on our identity —
 * the virtual CDJ has to announce a player number, and with four CDJ-3000s on
 * the network every number a player will talk to (1-4) is taken, so ours is 5
 * and the query goes unanswered. NFS is a plain RPC server on the player and
 * doesn't care who asks.
 *
 * prolink-connect waits for the reply with no timeout, inside a mutex, so one
 * unanswered query stalls every later one and the decks simply stay blank.
 * This replaces the query with one that gives up, and then assumes what it
 * could not ask: Rekordbox media in the slot. Wrong assumption? The NFS
 * download of `export.pdb` fails and says so, which is a diagnosable error
 * rather than an indefinite wait.
 *
 * The real query is attempted once. If it goes unanswered its mutex is held
 * for good, so every later call is served from the assumption directly.
 */
export function installMediaSlotBypass(network: ConnectedProlinkNetwork): void {
  const emitter = network.statusEmitter
  const query = emitter.queryMediaSlot.bind(emitter)

  /**
   * The one probe that decides for all slots. All four decks load their track
   * at once, so without sharing a single verdict they would each spend the
   * timeout discovering the same thing — and pile up behind the mutex the
   * unanswered query never releases.
   */
  let probe: Promise<MediaSlotInfo | null> | null = null
  /** Slots already reported as assumed — metadata, artwork and waveforms each ask. */
  const announced = new Set<string>()

  const runProbe = async (options: MediaSlotOptions): Promise<MediaSlotInfo | null> => {
    const { device, slot } = options
    try {
      const media = await withTimeout(query(options), QUERY_TIMEOUT_MS, 'queryMediaSlot')
      diag('media:query-answered', {
        device: device.id,
        slot: slotName(slot),
        name: media.name,
        tracks: media.trackCount,
        tracksType: trackTypeName(media.tracksType)
      })
      return media
    } catch (error) {
      diag('media:query-unanswered', {
        device: device.id,
        slot: slotName(slot),
        error,
        fallback: 'assuming rekordbox media, reading the database over NFS'
      })
      return null
    }
  }

  emitter.queryMediaSlot = async (options) => {
    const { device, slot } = options
    probe ??= runProbe(options)
    const probed = await probe

    // The probe answered for one slot only; every other slot can be asked
    // normally now that we know the player replies.
    if (probed !== null) {
      return probed.deviceId === device.id && probed.slot === slot ? probed : query(options)
    }

    const key = `${device.id}-${slot}`
    if (!announced.has(key)) {
      announced.add(key)
      diag('media:assumed', { device: device.id, slot: slotName(slot) })
    }
    return assumeRekordboxMedia(device, slot)
  }
}

/**
 * The media description prolink-connect needs to go ahead with an NFS read.
 *
 * Only two things here matter to it: `tracksType`, which gates the route, and
 * the identity fields, which it hashes into the cache key for the downloaded
 * database. Those are constant per device and slot, so a media swap during
 * the session keeps serving the database of the media that was read first —
 * the tradeoff for not being able to ask what is in the slot.
 */
function assumeRekordboxMedia(device: Device, slot: MediaSlot): MediaSlotInfo {
  return {
    deviceId: device.id,
    slot,
    name: `assumed-rekordbox-${device.id}-${slot}`,
    color: MediaColor.Default,
    createdDate: new Date(0),
    freeBytes: 0n,
    totalBytes: 0n,
    tracksType: TrackType.RB,
    trackCount: 0,
    playlistCount: 0,
    hasSettings: false
  }
}
