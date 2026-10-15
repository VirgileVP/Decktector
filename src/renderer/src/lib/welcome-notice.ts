/**
 * Whether the first-launch welcome notice has been acknowledged. Like the deck
 * view, this is a per-device flag kept in `localStorage` on its own key.
 *
 * The stored value is the notice's revision rather than a boolean, so a later
 * rewrite of the notice can be shown once more by bumping the revision.
 */

export const WELCOME_NOTICE_REVISION = '1'

export const WELCOME_NOTICE_STORAGE_KEY = 'decktector.welcomeNotice'

/** True unless the stored value says the current revision was already acknowledged. */
export function shouldShowWelcomeNotice(stored: string | null): boolean {
  return stored !== WELCOME_NOTICE_REVISION
}

export function loadShouldShowWelcomeNotice(): boolean {
  return shouldShowWelcomeNotice(localStorage.getItem(WELCOME_NOTICE_STORAGE_KEY))
}

export function acknowledgeWelcomeNotice(): void {
  localStorage.setItem(WELCOME_NOTICE_STORAGE_KEY, WELCOME_NOTICE_REVISION)
}
