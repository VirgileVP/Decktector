# Changelog

Notable changes to Decktector, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases before 1.3.0 are
listed on the [Releases page](https://github.com/VirgileVP/Decktector/releases).

Each section is published as is as the GitHub release notes for its tag, followed by
the downloads table from `.github/release-footer.md` — so write it for people who use
the app. Note changes under `[Unreleased]` as they land, then rename that heading to
the new version and date in the `chore(release)` commit; the `Build` workflow refuses a
`v*` tag whose version has no section here.

## [Unreleased]

## [1.5.0] - 2026-10-16

A steadier booth companion: decks keep updating through cable swaps, sleep and stick
changes, and panel windows reopen where you left them.

### Highlights

- **Welcome notice** — on first launch, Decktector tells you it is free, open-source
  (GPL-3.0) and independent from Pioneer DJ / AlphaTheta, with links to report bugs and
  contribute on GitHub.
- **About tab in Settings** — the same notice and links stay one click away after
  dismissing it.

### Fixed

- Pop-out panel windows reopen at the position and size they were saved with, instead
  of their default bounds.
- Closing or docking back a panel window no longer stops the decks updating until
  the app is restarted.
- A USB stick swapped into the same slot no longer shows the previous stick's
  tracks — title, waveform and play-history line — for up to two minutes.
- When a deck's status stops arriving, its waveform and the timecode output hold
  still after a second and a half instead of running on as if it were still playing.
- Decks resume updating on their own after the network cable is unplugged and
  plugged back, or the computer wakes from sleep, instead of needing a restart.
- A file played straight from the browser without rekordbox analysis, or a CD
  track, no longer shows an unrelated rekordbox track's title and waveform.
- A deck loading a track with the same rekordbox number as the one it showed, but
  off another stick or player, now shows the new track instead of keeping the old one.
- The network listeners restart on their own if one of them fails, instead of
  leaving the decks frozen until the app is restarted.
- A deck no longer keeps showing its previous track after the app falls behind a
  burst of player updates: the window catches up from the current state instead.
- A player answering a track lookup with a corrupt item count can no longer crash
  the app.

## [1.4.0] - 2026-10-09

Panel windows you can tailor, and a lineup ready for the booth.

### Highlights

- **Per-window display options** — each panel window now chooses what it shows from its own display options, saved with the window and restored after a restart. Deck windows offer the same _Simple view_ / _Full_ presets and toggles as the Deck view settings, with a _Match main window_ button to copy those settings back. The Time window keeps its clock / timecode / lineup choice across restarts.
- **Lineup in the Time window** — a progress bar shows how far the current set has played, and each Time window picks between the whole lineup and the previous / current / next sets.
- **Artist notes** — each set can carry an optional note for the booth, shown only in the Time window (it can be hidden from that window's display options).
- **Lineup import / export** — prepare a night on one machine and play it on another: the lineup (sets, doors opening, end of the night) is saved as a file and validated on import, with a confirmation before it replaces an existing lineup.

### Fixes

- Changes no longer bounce endlessly between two open windows, which made the whole UI flicker.
- The lineup stays in sync across windows: the Time window follows edits made in the main window's settings without being reopened.
- With waveforms hidden, the cue list or track structure now stretches over the empty deck body.

## [1.3.0] - 2026-10-07

First public-facing release of **Decktector** — the real-time visualizer for Pioneer Pro DJ Link decks (CDJ/XDJ).

### Highlights

- **New name and identity** — the app is now Decktector, with a new logo, app icon and bundled Geist / Geist Mono typefaces.
- **On-air source** — choose how the on-air deck is picked, with a playback fallback; timecode output can optionally follow the on-air deck.
- **Smarter device discovery** — players are keyed by MAC address, device number conflicts are flagged, and the Pro DJ Link network interface can be pinned in settings.
- **Layouts** — new 3-column grid view, per-deck labels, inline deck view toggles and simplified presets.
- **Windows** — the installer embeds the WebView2 runtime (works fully offline, no admin rights), and adapter MAC detection is fixed.

### License

Decktector is now licensed under **GPL-3.0-or-later**.
