/**
 * The normalized domain model, as the renderer sees it.
 *
 * Every sibling module in this directory is generated from the Rust
 * definitions in `crates/decktector-core/src/model.rs` by `cargo test` —
 * do not edit them. This barrel is the one hand-written file here, so the UI
 * can import the whole model from a single path.
 */

export type { BeatGridBeat } from './BeatGridBeat'
export type { CueKind } from './CueKind'
export type { DeckState } from './DeckState'
export type { DeckTrack } from './DeckTrack'
export type { DeviceInfo } from './DeviceInfo'
export type { DeviceKind } from './DeviceKind'
export type { DiscoveredDevice } from './DiscoveredDevice'
export type { EngineSnapshot } from './EngineSnapshot'
export type { FrameRate } from './FrameRate'
export type { NetworkInterfaceInfo } from './NetworkInterfaceInfo'
export type { NetworkState } from './NetworkState'
export type { PlayState } from './PlayState'
export type { RemoteConfig } from './RemoteConfig'
export type { RemoteStatus } from './RemoteStatus'
export type { TimecodeConfig } from './TimecodeConfig'
export type { TimecodeProtocol } from './TimecodeProtocol'
export type { TimecodeStatus } from './TimecodeStatus'
export type { TrackCue } from './TrackCue'
export type { TrackEvent } from './TrackEvent'
export type { TrackEventKind } from './TrackEventKind'
export type { TrackInfo } from './TrackInfo'
export type { WaveformData } from './WaveformData'
export type { WaveformSegment } from './WaveformSegment'

/** Player number on the Pro DJ Link network (1-6). */
export type DeckId = number
