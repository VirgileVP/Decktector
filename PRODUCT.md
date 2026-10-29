# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Primary users are not the DJ at the decks, but the people around the booth reading the decks from outside it:

- **VJ / video operator** — pulls a deck or the header block out into its own panel window, placed wherever the show needs it (a second screen, beside other software, over video); needs a clean view with no visual noise.
- **Lighting / front-of-house operator** — watches deck state from a distance, often through the read-only remote view on a tablet or laptop, to cue lights off track/BPM/on-air changes.
- **Event technician** — verifies Pro DJ Link discovery, player state, and connection health; diagnostic use rather than expressive.

## Product Purpose

Real-time visualizer for Pioneer Pro DJ Link decks (CDJ/XDJ), in the spirit of TC Supply's BeatKontrol. Surfaces, per deck, live: scrolling HD waveform with interpolated playhead and beat-grid ticks, a whole-track minimap, track metadata (title/artist/album/genre/key/label/artwork), and player state (play/cue/pause, master, sync, on-air, pitch, live BPM, time remaining). Success is someone outside the booth being able to read deck state correctly at a glance, without asking the DJ.

## Positioning

Reads the Pro DJ Link network directly (no DJ software integration needed beyond Rekordbox-analyzed media) and exposes that state to people who are not the DJ — via detachable panel windows that put any view (a deck, the clock, the timecode, the lineup) wherever it is needed, and a token-gated read-only remote view for a second screen on the LAN. A neighboring product built only for the DJ's own booth screen could not truthfully claim the detached-view/remote/multi-operator angle.

## Operating Context

Live, in-venue use: club or event, booth network, decks running from a Pioneer Pro DJ Link network (XDJ-XZ, XDJ-1000MK2, or 2–4 CDJ-2000-era players). Three viewing contexts carry equal weight and none should be designed as secondary:

- close-up on a screen in a dim/dark venue,
- distant on a lighting-desk or front-of-house screen,
- composited transparent over video in a mixer (panel windows, background dropped for keying).

Desktop app (Tauri) plus a browser-based remote view. Detachable panel windows mirror the main window and carry their own zoom/transparency/always-on-top state, independent of it. The remote view can detach panels too, as plain browser windows (zoom only: a browser cannot make a window transparent or keep it on top).

## Capabilities and Constraints

- Rust-derived domain model (`decktector-core`) is the single source of truth; the TypeScript model is generated from it, so engine/UI divergence fails the build rather than surfacing live.
- No absolute playhead position from the protocol on this hardware generation — playhead is anchored on beat counter + beat grid and interpolated (engine 5 Hz, renderer 60 fps). Any waveform/position UI must tolerate this being an approximation, not ground truth.
- Remote view is read-only by design (no write path back into the engine); token regenerates every server start. Not hardened auth — a venue network is not trusted, so nothing beyond what's already on screen should ever be exposed there.
- Virtual player numbering constraints (1–6 range, XDJ-XZ claims 1+2, app defaults to 5) are hard protocol constraints, not UI concerns, but any diagnostics/device UI must reflect real player-number state accurately.
- Rust rewrite in progress (Electron → Tauri); UI is being kept the same through the port. All four protocol-backend phases are ported: live network presence and deck state (phase 2), and track metadata — title/artist/album/genre/key/label/bpm, beat grid, HD waveform, cues, artwork — via NFS with a dbserver fallback (phases 3–4). None of the metadata code has run against real Pro DJ Link hardware yet; confidence varies by piece (see README.md's "Porting status" table) and the hand-rolled `export.pdb` parser specifically is the least-verified part of the whole backend. Simulated decks remain available via `npm run dev:sim`.

## Brand Commitments

- Name: Decktector. Positioned explicitly "in the spirit of TC Supply's BeatKontrol" — same category, not a mandate to copy its look.
- User explicitly wants a modern, professional, legible identity — not a strict continuation of BeatKontrol's visual language.

## Evidence on Hand

- Existing themes/tokens: `src/renderer/src/assets/themes.css`, `src/renderer/src/assets/main.css`.
- Existing component set covers deck panels, waveform canvases, header block (clock, timecode, lineup), settings, remote-link banner, device discovery panel, detachable panel chrome.
- No user testing, screenshots, or third-party references on hand beyond BeatKontrol as a category reference (not a visual mandate).

## Product Principles

1. Design for the person outside the booth, not the DJ at the decks — legibility and state-accuracy from a distance, on a keyed overlay, or on a second screen all matter equally.
2. Never let a UI element assert more certainty than the protocol gives (e.g. interpolated playhead) — approximate data must read as current best estimate, not fact.
3. Panel windows (detached views, optionally transparent) are a first-class surface, not an afterthought of the main window — they need their own visual cleanliness with zero incidental chrome, whether they sit on a second screen or over video.
4. Read-only and diagnostic surfaces (remote view, device panel) should look distinctly lower-stakes than the live performance surfaces, since they can't cause harm but also shouldn't imply control they don't have.
5. Modern, professional, legible over nostalgic/retro — BeatKontrol is a category reference, not a style mandate.

## Accessibility & Inclusion

No formal accessibility requirement established. User explicitly did not flag colorblind-safety for on-air/master/sync states as a constraint — but per Product Principle 1 (legible from a distance, at a glance, in a dark venue), state indicators should not rely on color alone as good practice regardless.
