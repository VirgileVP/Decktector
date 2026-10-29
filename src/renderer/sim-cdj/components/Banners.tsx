import { useAudioStore } from '../state/audio-store'
import { useSimStore } from '../state/sim-store'

/** Top-of-page notices: the library fallback, sound still locked and a lost control connection. */
export function Banners(): React.JSX.Element {
  const status = useSimStore((state) => state.status)
  const sound = useAudioStore((state) => state.sound)
  const fallbackWarning = useSimStore((state) => state.rig?.fallbackWarning ?? null)
  return (
    <div className="banners">
      {fallbackWarning !== null && (
        <div className="banner banner-fallback" role="alert">
          {fallbackWarning}
        </div>
      )}
      {sound === 'suspended' && (
        <div className="banner banner-sound" role="status">
          Click anywhere to enable sound
        </div>
      )}
      {sound === 'unavailable' && (
        <div className="banner banner-fallback" role="alert">
          Web Audio is not available in this browser: the decks will stay silent.
        </div>
      )}
      {status !== 'connected' && (
        <div className="banner banner-disconnected" role="status">
          {status === 'connecting'
            ? 'Connecting to the simulator...'
            : 'Disconnected from the simulator. Retrying...'}
        </div>
      )}
    </div>
  )
}
