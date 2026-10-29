export const MS_PER_SECOND = 1000

/** Playback must stay within this many ms of the rig's position before it is re-seeked. */
export const DRIFT_RESEEK_THRESHOLD_MS = 40

/** Fade applied when a source starts, stops or is replaced, to avoid clicks. */
export const SEEK_FADE_SECONDS = 0.012

/** Time constants of the exponential glide towards a new playback rate or gain. */
export const RATE_TIME_CONSTANT_SECONDS = 0.015
export const FADER_TIME_CONSTANT_SECONDS = 0.02
export const MASTER_TIME_CONSTANT_SECONDS = 0.02

/** Rates closer than this to the current one are not worth touching the AudioParam. */
export const RATE_EPSILON = 0.0005

/** Longest a single scratch grain plays. */
export const SCRATCH_GRAIN_LENGTH_MS = 60

/** Extra time each grain plays past the motion it covers, so consecutive grains overlap. */
export const SCRATCH_GRAIN_OVERLAP_MS = 10

/** Fastest a scratch grain is played; faster jog motion is clamped to this. */
export const SCRATCH_MAX_RATE = 4

/** Slowest a scratch grain is played, so a barely moving jog still ticks audibly. */
export const SCRATCH_MIN_RATE = 0.05

/** Position changes smaller than this are jitter, not scratching. */
export const SCRATCH_MIN_DELTA_MS = 1

/** Fade at each end of a grain. */
export const SCRATCH_GRAIN_FADE_SECONDS = 0.005

/** Master volume the page starts with, 0..1. */
export const DEFAULT_MASTER_VOLUME = 0.8

/** Decoded buffers kept around (they are large); the least recently used go first. */
export const MAX_CACHED_BUFFERS = 8
