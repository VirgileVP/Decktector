import { invoke, isTauri } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'

import { DECK_NUMBERS, DEFAULT_DECK_COLORS, deckColor } from '../lib/deck-colors'
import { THEMES } from '../lib/theme'
import type { NetworkInterfaceInfo } from '../model'
import { useRemoteStore } from '../state/remote'
import {
  NO_NETWORK_INTERFACE,
  TIMECODE_FRAME_RATE_OPTIONS,
  TIMECODE_PROTOCOL_OPTIONS,
  UI_SCALE_RANGE,
  WAVEFORM_DESATURATION_RANGE,
  useSettings,
  type Settings
} from '../state/settings'
import { DevicesPanel } from './DevicesPanel'
import { LineupEditor } from './LineupEditor'
import { ChoiceRow, SliderRow, TextRow, ToggleRow } from './SettingControls'

const asPercent = (value: number): string => `${Math.round(value * 100)} %`

const SETTINGS_TABS = [
  'Interface',
  'Deck view',
  'Line up',
  'Timecode',
  'Remote view',
  'Devices'
] as const
type SettingsTab = (typeof SETTINGS_TABS)[number]

/** Tabs desktop-only gear needs — a remote browser has nothing to send timecode to,
 *  and must not be able to control the very server that is showing it this screen. */
const IS_TAURI = isTauri()
const SHELL_ONLY_TABS: readonly SettingsTab[] = ['Timecode', 'Remote view']
const VISIBLE_SETTINGS_TABS = SETTINGS_TABS.filter(
  (tab) => IS_TAURI || !SHELL_ONLY_TABS.includes(tab)
)

interface SettingsDialogProps {
  onClose: () => void
}

/** App settings, as a modal over the deck grid. Closes on Escape or backdrop click. */
export function SettingsDialog({ onClose }: SettingsDialogProps): React.JSX.Element {
  const panelRef = useRef<HTMLDivElement>(null)
  const settings = useSettings()
  const remoteStatus = useRemoteStore()
  const [activeTab, setActiveTab] = useState<SettingsTab>('Interface')
  const update = <K extends keyof Settings>(key: K) => {
    return (value: Settings[K]): void => settings.update(key, value)
  }

  const [networkInterfaces, setNetworkInterfaces] = useState<NetworkInterfaceInfo[]>([])
  const [midiPorts, setMidiPorts] = useState<string[]>([])
  const [ltcDevices, setLtcDevices] = useState<string[]>([])
  const [remoteQrSvg, setRemoteQrSvg] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    invoke<NetworkInterfaceInfo[]>('list_network_interfaces')
      .then(setNetworkInterfaces)
      .catch((error) => console.error('Failed to list network interfaces:', error))
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    invoke<string[]>('list_midi_output_ports')
      .then(setMidiPorts)
      .catch((error) => console.error('Failed to list MIDI output ports:', error))
  }, [])

  useEffect(() => {
    if (!isTauri()) return
    invoke<string[]>('list_ltc_output_devices')
      .then(setLtcDevices)
      .catch((error) => console.error('Failed to list LTC output devices:', error))
  }, [])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    // The panel takes focus so Escape works without clicking inside first.
    panelRef.current?.focus()
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  // The URL to hand out: the chosen interface's address once one is picked,
  // otherwise the first broadcastable adapter as a best-effort guess — the
  // server itself is bound to every interface in that case, so any of this
  // machine's addresses would work, not just the one shown here.
  const selectedInterface = networkInterfaces.find(
    (networkInterface) => networkInterface.name === settings.remoteInterface
  )
  const remoteHost = selectedInterface?.ip ?? networkInterfaces[0]?.ip ?? 'localhost'
  const remoteUrl =
    remoteStatus.listening && remoteStatus.token
      ? `http://${remoteHost}:${remoteStatus.port}/?token=${remoteStatus.token}`
      : null

  useEffect(() => {
    // A stale SVG from a previous URL is never shown: the `<img>` below only
    // renders while `remoteUrl` is set, so there is nothing to reset here.
    if (!isTauri() || !remoteUrl) return
    invoke<string>('remote_qr_svg', { url: remoteUrl })
      .then(setRemoteQrSvg)
      .catch((error) => console.error('Failed to render the remote view QR code:', error))
  }, [remoteUrl])

  return (
    <div className="settings-backdrop" onClick={onClose}>
      <div
        ref={panelRef}
        className="settings-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
      >
        <header className="settings-head">
          <h2>Settings</h2>
          <button type="button" className="settings-close" title="Close" onClick={onClose}>
            ×
          </button>
        </header>

        <nav className="settings-tabs" role="tablist" aria-label="Settings sections">
          {VISIBLE_SETTINGS_TABS.map((tab) => (
            <button
              key={tab}
              type="button"
              role="tab"
              aria-selected={tab === activeTab}
              className={`settings-tab${tab === activeTab ? ' active' : ''}`}
              onClick={() => setActiveTab(tab)}
            >
              {tab}
            </button>
          ))}
        </nav>

        <div className="settings-tab-panel">
          {activeTab === 'Interface' && (
            <>
              <section className="settings-section">
                <h3>Theme</h3>
                <div className="theme-grid">
                  {THEMES.map((option) => (
                    <button
                      key={option.id}
                      type="button"
                      className={`theme-card${option.id === settings.theme ? ' active' : ''}`}
                      aria-pressed={option.id === settings.theme}
                      onClick={() => settings.update('theme', option.id)}
                    >
                      {/* Palettes are scoped by attribute, so a swatch can wear another
                        theme than the app and preview it without any extra CSS. */}
                      <span className="theme-swatch" data-theme={option.id}>
                        <i className="swatch-bg" />
                        <i className="swatch-panel" />
                        <i className="swatch-accent" />
                      </span>
                      <span className="theme-label">{option.label}</span>
                      <span className="theme-description">{option.description}</span>
                    </button>
                  ))}
                </div>
              </section>

              <section className="settings-section">
                <h3>Display</h3>
                <SliderRow
                  label="Interface scale"
                  hint="Zooms the whole window — booth screens run from 13 to 27 inches"
                  value={settings.uiScale}
                  range={UI_SCALE_RANGE}
                  format={asPercent}
                  onChange={update('uiScale')}
                />
                <SliderRow
                  label="Waveform wash"
                  hint="Flattens the rekordbox colors; at 0 % they are left raw"
                  value={settings.waveformDesaturation}
                  range={WAVEFORM_DESATURATION_RANGE}
                  format={asPercent}
                  onChange={update('waveformDesaturation')}
                />
                <ToggleRow
                  label="Dim decks that are off air"
                  hint="Fades idle decks so the live ones read at a glance"
                  checked={settings.dimIdleDecks}
                  onChange={update('dimIdleDecks')}
                />
              </section>

              <section className="settings-section">
                <div className="settings-section-head">
                  <h3>Deck colors</h3>
                  <button
                    type="button"
                    className="settings-reset"
                    onClick={settings.resetDeckColors}
                  >
                    Reset
                  </button>
                </div>
                <div className="deck-color-row">
                  {DECK_NUMBERS.map((deckId) => (
                    <label key={deckId} className="deck-color">
                      <input
                        type="color"
                        value={deckColor(deckId, settings.deckColors)}
                        onChange={(event) => settings.setDeckColor(deckId, event.target.value)}
                      />
                      <span>{deckId}</span>
                    </label>
                  ))}
                </div>
                <p className="setting-hint">
                  Player accents, used by the playheads and the deck badges. Defaults:{' '}
                  {DECK_NUMBERS.map((deckId) => DEFAULT_DECK_COLORS[deckId]).join(', ')}.
                </p>
              </section>

              {IS_TAURI && (
                <section className="settings-section">
                  <h3>Window</h3>
                  <ToggleRow
                    label="Always on top"
                    hint="Keeps Decktective above every other application"
                    checked={settings.alwaysOnTop}
                    onChange={update('alwaysOnTop')}
                  />
                  <ToggleRow
                    label="Keep the screen awake"
                    hint="Holds off the screen saver while a set is running"
                    checked={settings.preventSleep}
                    onChange={update('preventSleep')}
                  />
                </section>
              )}
            </>
          )}

          {activeTab === 'Deck view' && (
            <section className="settings-section">
              <h3>Deck bar</h3>
              <ToggleRow
                label="Artwork"
                hint="Album art thumbnail next to the deck number"
                checked={settings.showDeckArtwork}
                onChange={update('showDeckArtwork')}
              />
              <ToggleRow
                label="Track tags"
                hint="Key and genre badges next to the track title"
                checked={settings.showDeckTrackTags}
                onChange={update('showDeckTrackTags')}
              />
              <ToggleRow
                label="Sync badges"
                hint="ON AIR, MASTER and SYNC indicators"
                checked={settings.showDeckSyncBadges}
                onChange={update('showDeckSyncBadges')}
              />
              <ToggleRow
                label="Pitch"
                hint="Pitch percentage next to the BPM"
                checked={settings.showDeckPitch}
                onChange={update('showDeckPitch')}
              />
              <ToggleRow
                label="Time remaining"
                hint="Countdown next to the BPM and pitch"
                checked={settings.showDeckTimeRemaining}
                onChange={update('showDeckTimeRemaining')}
              />

              <h3>Deck body</h3>
              <ToggleRow
                label="Cue list"
                hint="Cue points below the track info"
                checked={settings.showDeckCueList}
                onChange={update('showDeckCueList')}
              />
              <ToggleRow
                label="Waveform overview"
                hint="Full-track waveform strip"
                checked={settings.showDeckWaveformOverview}
                onChange={update('showDeckWaveformOverview')}
              />
              <ToggleRow
                label="Waveform detail"
                hint="Zoomed-in waveform around the playhead"
                checked={settings.showDeckWaveformDetail}
                onChange={update('showDeckWaveformDetail')}
              />
            </section>
          )}

          {activeTab === 'Line up' && <LineupEditor />}

          {IS_TAURI && activeTab === 'Timecode' && (
            <section className="settings-section">
              <h3>Timecode</h3>
              <ChoiceRow
                label="Output"
                hint="Which deck to send from is picked in the header, not here"
                value={settings.timecodeProtocol}
                options={TIMECODE_PROTOCOL_OPTIONS}
                onChange={update('timecodeProtocol')}
              />
              <ChoiceRow
                label="Frame rate"
                hint="Must match the receiving lighting or video console"
                value={settings.timecodeFrameRate}
                options={TIMECODE_FRAME_RATE_OPTIONS}
                onChange={update('timecodeFrameRate')}
              />
              {settings.timecodeProtocol === 'artnet' && (
                <>
                  {networkInterfaces.length > 0 ? (
                    <ChoiceRow
                      label="Network interface"
                      hint="Adapter to broadcast Art-Net from — pick the one on the lighting network"
                      value={settings.timecodeArtNetInterface}
                      options={networkInterfaces.map((networkInterface) => ({
                        value: networkInterface.name,
                        label: `${networkInterface.name} (${networkInterface.ip})`
                      }))}
                      onChange={update('timecodeArtNetInterface')}
                    />
                  ) : (
                    <p className="setting-hint">
                      No network interface found — connect the lighting network and reopen settings.
                    </p>
                  )}
                  <TextRow
                    label="Art-Net port"
                    hint="6454 by convention — change only for a non-standard node"
                    type="number"
                    value={String(settings.timecodeArtNetPort)}
                    onChange={(value) => settings.update('timecodeArtNetPort', Number(value) || 0)}
                  />
                </>
              )}
              {settings.timecodeProtocol === 'mtc' &&
                (midiPorts.length > 0 ? (
                  <ChoiceRow
                    label="MIDI output"
                    hint="Port to send MTC quarter-frame messages to"
                    value={settings.timecodeMtcPort}
                    options={midiPorts.map((port) => ({ value: port, label: port }))}
                    onChange={update('timecodeMtcPort')}
                  />
                ) : (
                  <p className="setting-hint">
                    No MIDI output port found — Windows has no built-in virtual MIDI cable, install
                    loopMIDI (or similar) to create one, then reopen settings.
                  </p>
                ))}
              {settings.timecodeProtocol === 'ltc' &&
                (ltcDevices.length > 0 ? (
                  <ChoiceRow
                    label="Audio output"
                    hint="Device to send the LTC audio signal to — route it into the lighting or video console"
                    value={settings.timecodeLtcDevice}
                    options={ltcDevices.map((device) => ({ value: device, label: device }))}
                    onChange={update('timecodeLtcDevice')}
                  />
                ) : (
                  <p className="setting-hint">
                    No audio output device found — connect one and reopen settings.
                  </p>
                ))}
            </section>
          )}

          {IS_TAURI && activeTab === 'Remote view' && (
            <section className="settings-section">
              <h3>Remote view</h3>
              <ToggleRow
                label="Enable"
                hint="Serves a read-only copy of this screen to browsers on the local network"
                checked={settings.remoteEnabled}
                onChange={update('remoteEnabled')}
              />
              <TextRow
                label="Port"
                hint="Change only if this port is already used by something else"
                type="number"
                value={String(settings.remotePort)}
                onChange={(value) => settings.update('remotePort', Number(value) || settings.remotePort)}
              />
              <ChoiceRow
                label="Network interface"
                hint="Adapter to bind to. Every interface means reachable from every network this machine is on."
                value={settings.remoteInterface}
                options={[
                  { value: NO_NETWORK_INTERFACE, label: 'Every interface' },
                  ...networkInterfaces.map((networkInterface) => ({
                    value: networkInterface.name,
                    label: `${networkInterface.name} (${networkInterface.ip})`
                  }))
                ]}
                onChange={update('remoteInterface')}
              />
              {remoteStatus.error && <p className="setting-hint">Could not start: {remoteStatus.error}</p>}
              {remoteUrl && (
                <>
                  <p className="setting-hint">Open this on another device on the same network:</p>
                  <p className="setting-hint setting-value-selectable">{remoteUrl}</p>
                  {remoteQrSvg && (
                    <img
                      alt="QR code for the remote view URL"
                      src={`data:image/svg+xml;utf8,${encodeURIComponent(remoteQrSvg)}`}
                    />
                  )}
                  <p className="setting-hint">
                    {remoteStatus.clientCount}{' '}
                    {remoteStatus.clientCount === 1 ? 'browser' : 'browsers'} connected
                  </p>
                </>
              )}
            </section>
          )}

          {activeTab === 'Devices' && <DevicesPanel />}
        </div>
      </div>
    </div>
  )
}
