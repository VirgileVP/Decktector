import { networkInterfaces, type NetworkInterfaceInfoIPv4 } from 'os'

/**
 * Player number the virtual CDJ announces itself with.
 *
 * prolink-connect's autoconfiguration picks 5, which keeps clear of the
 * numbers players self-assign — but players only answer media-slot queries
 * and metadata requests coming from a number in the 1-4 range, so with four
 * players on the network those routes are closed to us.
 *
 * A CDJ-3000 can be renumbered to player 5 or 6, which frees a number in the
 * range: set that player, then start with DECKTECTIVE_VCDJ_ID pointing at the
 * freed number. Two devices sharing a number breaks the network, so this is
 * deliberately opt-in.
 */
export function requestedVcdjId(): number | null {
  const raw = process.env.DECKTECTIVE_VCDJ_ID
  if (!raw) return null
  const id = Number(raw)
  return Number.isInteger(id) && id >= 1 && id <= 6 ? id : null
}

const toInt = (ip: string): number =>
  ip.split('.').reduce((value, octet) => (value << 8) + Number(octet), 0) >>> 0

/**
 * The local IPv4 interface sharing a subnet with the given peer — the one
 * Pro DJ Link traffic has to go out of. Link-local addresses (169.254.x.x,
 * what players self-assign without a DHCP server) are matched on their /16,
 * since each device picks its own address and advertises a /16 mask.
 */
export function findInterfaceForPeer(peerIp: string): NetworkInterfaceInfoIPv4 | null {
  const peer = toInt(peerIp)

  for (const addresses of Object.values(networkInterfaces())) {
    for (const address of addresses ?? []) {
      if (address.family !== 'IPv4' || address.internal) continue
      const mask = toInt(address.netmask)
      if ((toInt(address.address) & mask) === (peer & mask)) {
        return address as NetworkInterfaceInfoIPv4
      }
    }
  }

  return null
}
