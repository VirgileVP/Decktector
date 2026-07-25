import dgram from 'dgram'

import type { DeckId } from '../types'

/**
 * Reads CDJ-3000 "Precise Position" packets (protocol type 0x0b, port
 * 50001/BEAT_PORT), broadcast every ~30ms whenever a track is loaded.
 * Unlike CDJ-2000-era status packets, these carry an absolute playhead in
 * milliseconds, so decks on this hardware don't need beat-grid interpolation.
 *
 * prolink-connect binds this same port itself but never reads from it (only
 * sends), and doesn't export its port/header constants, so this listens on
 * its own socket with SO_REUSEADDR to receive a parallel copy of the traffic.
 *
 * Layout verified against Deep Symmetry's beat-link reference implementation
 * (PrecisePosition.java / Util.java): 60-byte packet, device number at
 * 0x21, playback position (ms) at 0x28.
 */

const BEAT_PORT = 50001
const PROLINK_HEADER = Buffer.from([0x51, 0x73, 0x70, 0x74, 0x31, 0x57, 0x6d, 0x4a, 0x4f, 0x4c])
const PACKET_TYPE_OFFSET = 0x0a
const PRECISE_POSITION_TYPE = 0x0b
const PACKET_LENGTH = 60
const DEVICE_NUMBER_OFFSET = 0x21
const POSITION_MS_OFFSET = 0x28

export interface PrecisePositionSample {
  deckId: DeckId
  positionMs: number
}

/** Parses one datagram; returns null for anything that isn't a well-formed precise-position packet. */
export function parsePrecisePosition(packet: Buffer): PrecisePositionSample | null {
  if (packet.length !== PACKET_LENGTH) return null
  if (!packet.subarray(0, PROLINK_HEADER.length).equals(PROLINK_HEADER)) return null
  if (packet[PACKET_TYPE_OFFSET] !== PRECISE_POSITION_TYPE) return null

  return {
    deckId: packet[DEVICE_NUMBER_OFFSET],
    positionMs: packet.readUInt32BE(POSITION_MS_OFFSET)
  }
}

/** Listens for precise-position packets and reports one sample at a time as they arrive. */
export class PrecisePositionListener {
  private socket: dgram.Socket | null = null

  start(onSample: (sample: PrecisePositionSample) => void): void {
    const socket = dgram.createSocket({ type: 'udp4', reuseAddr: true })
    socket.on('error', (error) => console.error('Precise-position socket error:', error))
    socket.on('message', (packet) => {
      const sample = parsePrecisePosition(packet)
      if (sample) onSample(sample)
    })
    socket.bind(BEAT_PORT, '0.0.0.0')
    this.socket = socket
  }

  stop(): void {
    this.socket?.close()
    this.socket = null
  }
}
