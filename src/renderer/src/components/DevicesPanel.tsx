import type { DeviceKind } from '../model'
import { useDevicesStore } from '../state/devices'

const KIND_LABELS: Record<DeviceKind, string> = {
  player: 'Player',
  mixer: 'Mixer',
  other: 'Other'
}

/** Devices tab: everything currently broadcasting on the Pro DJ Link network. */
export function DevicesPanel(): React.JSX.Element {
  const devices = useDevicesStore((s) => s.devices)

  return (
    <section className="settings-section">
      <h3>Pro DJ Link devices</h3>
      <p className="setting-hint">
        Players and mixers currently broadcasting on the network — updates live as gear joins or
        leaves.
      </p>

      {devices.length === 0 ? (
        <p className="setting-hint">
          No device found yet — connect this machine to the same network as your CDJ/XDJ/DJM gear.
        </p>
      ) : (
        <table className="device-table">
          <thead>
            <tr>
              <th>#</th>
              <th>Name</th>
              <th>Type</th>
              <th>IP address</th>
              <th>MAC address</th>
              <th>Peers</th>
            </tr>
          </thead>
          <tbody>
            {devices.map((device) => (
              <tr key={device.deviceNumber}>
                <td>{device.deviceNumber}</td>
                <td>{device.name}</td>
                <td>
                  <span className={`device-kind device-kind-${device.kind}`}>
                    {KIND_LABELS[device.kind]}
                  </span>
                </td>
                <td className="device-mono">{device.ip}</td>
                <td className="device-mono">{device.macAddress}</td>
                <td>{device.peerCount}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  )
}
