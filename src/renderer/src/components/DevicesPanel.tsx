import type { DeviceKind } from '../model'
import { describeNumberConflict, findNumberConflicts } from '../lib/device-conflicts'
import { useDevicesStore } from '../state/devices'

const KIND_LABELS: Record<DeviceKind, string> = {
  player: 'Player',
  mixer: 'Mixer',
  other: 'Other'
}

/** Devices tab: everything currently broadcasting on the Pro DJ Link network. */
export function DevicesPanel(): React.JSX.Element {
  const devices = useDevicesStore((s) => s.devices)

  const physicalPlayers = devices.filter(
    (d) => d.name.toLowerCase() !== 'decktective' && d.kind === 'player'
  )
  const virtualDevice = devices.find((d) => d.name.toLowerCase() === 'decktective')
  const sortedDevices = [...devices].sort((a, b) => a.deviceNumber - b.deviceNumber)
  const numberConflicts = findNumberConflicts(devices)

  return (
    <section className="settings-section">
      <h3>Pro DJ Link devices</h3>
      <p className="setting-hint">
        Players and mixers currently broadcasting on the network — updates live as gear joins or
        leaves.
      </p>

      <div className="device-topology-card">
        <div className="device-topology-header">
          <div className="device-topology-title">
            <span
              className={`device-topology-dot ${
                physicalPlayers.length > 0 ? 'dot-connected' : 'dot-searching'
              }`}
            />
            <span>Network Topology</span>
          </div>
          {virtualDevice ? (
            virtualDevice.deviceNumber <= 4 ? (
              <span className="device-topology-badge badge-dual-route">
                Dual route (NFS + dbserver)
              </span>
            ) : (
              <span className="device-topology-badge badge-nfs-only">NFS route active</span>
            )
          ) : (
            <span className="device-topology-badge badge-standby">
              {physicalPlayers.length > 0 ? 'Connecting…' : 'Searching…'}
            </span>
          )}
        </div>

        <div className="device-topology-details">
          <div className="device-topology-summary">
            <span>
              <strong>Physical players:</strong>{' '}
              {physicalPlayers.length === 0
                ? 'None detected'
                : `${physicalPlayers.length} detected (${physicalPlayers
                    .map((p) => `#${p.deviceNumber}`)
                    .join(', ')})`}
            </span>
            <span>
              <strong>Decktective:</strong>{' '}
              {virtualDevice
                ? virtualDevice.deviceNumber <= 4
                  ? `Virtual ID #${virtualDevice.deviceNumber} • Double route active (NFS + repli dbserver)`
                  : `Virtual ID #${virtualDevice.deviceNumber} • Route NFS active (${physicalPlayers.length} platines physiques)`
                : physicalPlayers.length > 0
                  ? 'Claiming virtual player number…'
                  : 'En attente de connexion…'}
            </span>
          </div>
          <p className="device-topology-note">
            {virtualDevice
              ? virtualDevice.deviceNumber <= 4
                ? 'Ceinture de sécurité maximale : métadonnées accessibles via NFS et requêtes directes dbserver.'
                : 'Les slots 1 à 4 sont occupés par les platines physiques. Lecture des métadonnées assurée via NFS.'
              : 'Dès détection d’une platine sur le réseau, Decktective s’attribue un ID virtuel pour acquérir le statut et les métadonnées.'}
          </p>
        </div>
      </div>

      {numberConflicts.map((conflict) => (
        <p key={conflict.deviceNumber} className="setting-hint setting-warning">
          {describeNumberConflict(conflict)}
        </p>
      ))}

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
            {sortedDevices.map((device) => {
              const isVirtual = device.name.toLowerCase() === 'decktective'
              return (
                <tr
                  key={`${device.macAddress}@${device.ip}`}
                  className={isVirtual ? 'device-row-virtual' : undefined}
                >
                  <td>
                    {device.deviceNumber}
                    {device.numberHolders > 1 && (
                      <span
                        className="device-conflict-badge"
                        title={`${device.numberHolders} devices announce number ${device.deviceNumber}`}
                      >
                        Number conflict
                      </span>
                    )}
                  </td>
                  <td>
                    {device.name}
                    {isVirtual && <span className="device-virtual-badge">Virtual CDJ</span>}
                  </td>
                  <td>
                    <span className={`device-kind device-kind-${device.kind}`}>
                      {KIND_LABELS[device.kind]}
                    </span>
                  </td>
                  <td className="device-mono">{device.ip}</td>
                  <td className="device-mono">{device.macAddress}</td>
                  <td>{device.peerCount}</td>
                </tr>
              )
            })}
          </tbody>
        </table>
      )}
    </section>
  )
}
