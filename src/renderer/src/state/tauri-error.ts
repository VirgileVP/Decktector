/** Tauri commands reject with the Rust side's error string; this gives callers an `Error`. */
export function toError(reason: unknown): Error {
  return reason instanceof Error ? reason : new Error(String(reason))
}
