# Decktective 🕵️

Real-time visualizer for Pioneer **Pro DJ Link** decks (CDJ/XDJ), in the spirit of
TC Supply's BeatKontrol. Plug your Mac into the same network as your players and get,
for every deck:

- **Scrolling HD waveform** with a smoothly interpolated playhead and beat-grid ticks
- **Whole-track minimap** with played/remaining position
- **Track metadata**: title, artist, album, genre, key, label, artwork
- **Player state**: play/cue/pause, master, sync, on-air, pitch, live BPM, time remaining

Primary targets: **XDJ-XZ** and **XDJ-1000MK2** (works with 2 to 4 CDJ-2000-era players).

> **Status — Rust rewrite in progress.** The app is being moved off Electron onto a
> Rust backend with Tauri, keeping the same UI. The shell, the domain model and the
> simulation engine are in place; **the Pro DJ Link backend is not ported yet**, so
> only `npm run dev:sim` does anything useful right now. The Electron implementation
> remains on the `master` branch. See [Architecture](#architecture) for the plan.

## Requirements

- The computer must be on the **same LAN** as the players (Ethernet into the booth
  switch, or directly into the XDJ's LINK port).
- **Rekordbox must NOT be running** on the same machine — it competes for the same
  virtual-player slot on the Pro DJ Link network.
- Tracks must come from Rekordbox-analyzed media (USB/SD) for metadata and waveforms
  to be available.
- A [Rust toolchain](https://rustup.rs) and the platform's webview: WebView2 on
  Windows (preinstalled on Windows 11), WebKitGTK on Linux, WKWebView on macOS. No
  native Node modules are involved anymore, so there is nothing to rebuild per
  architecture.

## Metadata on a full four-player network

Players only answer media-slot queries and metadata requests coming from a virtual
CDJ numbered 1-4. With four players on the network all four numbers are taken, ours
is 5, and both of those doors are closed — the track database is then read straight
off the player over NFS instead, which needs no player number.

A CDJ-3000 can be renumbered to player 5 or 6, which frees a number in range. Set
one player that way, then claim the freed number:

```sh
DECKTECTIVE_VCDJ_ID=4 npm run dev
```

Never point it at a number a device already uses — two devices sharing one breaks the
network.

This behaviour is documented here because it is a hard-won constraint the port has
to preserve; it lands with phase 3.

## Getting started

```sh
npm install

# Run with two simulated decks (no hardware needed)
npm run dev:sim

# Four simulated decks
npm run dev:sim4
```

```sh
npm run test:rust    # Rust tests, and regenerates the renderer's model types
npm run lint:rust    # clippy, warnings as errors
npm run typecheck    # renderer
```

## Building

```sh
npm run build
```

Produces an NSIS installer on Windows, a `.dmg` on macOS and `.AppImage`/`.deb` on
Linux — each must be built on its own platform.

## Remote view

A second screen — a phone by the lighting desk, a laptop out front — can watch the
same decks the desktop app shows, in any browser on the local network. It is
**off by default**: turn it on from Settings → Remote view, which also shows the
URL and a QR code once it is listening.

It is **read-only**. A remote browser watches; it never controls the app — there is
no write path from a remote client back into the engine, and shell-only controls
(window behaviour, timecode output) are hidden there since they would not do
anything.

The link is gated by a token embedded in the URL, **regenerated every time the
server starts** — closing and reopening the remote view drops anyone using the old
link. This is a guard against someone on the same network stumbling onto the feed,
not hardened authentication: **a venue network is not a trusted network**, and
nothing sensitive should be exposed through this beyond what is already on screen.

This works the same in development, with no extra setup: `npm run dev` has no
bundle to serve, so the remote server proxies asset requests through to Vite.
Everything stays on the remote server's own port, exactly as in a packaged
build. Vite must be reachable for that, which is why its dev server binds every
interface (`server.host` in `vite.config.ts`).

`VITE_REMOTE_WS_PORT` overrides the port the remote WebSocket connects to. It is
not needed for the flow above; it exists for the case where the page is opened
on Vite's own port rather than the remote server's.

## Detachable panels

The clock, the timecode readout, the whole header block (clock + timecode +
lineup), and any deck can be popped into its own OS window — useful for putting
the clock on a second monitor, or feeding a deck to a video mixer. Hover a panel
to reveal a small pop-out button; each click opens another window, and several
windows of the same panel are allowed.

These windows **mirror** the main window rather than moving content out of it —
the main window's layout never changes, whatever is popped out stays visible
there too. Each panel window has its own chrome (a title bar and a close button
appear on hover; drag the bar to move it, drag an edge to resize), and its own
always-on-top, transparency and zoom controls, independent of the main window's
settings. Transparency is meant for keying the window over a video feed — pin a
deck or the clock on top, drop its background, and key out the rest. Open panel
windows are remembered across restarts, in their last position.

This is desktop-only. The [remote view](#remote-view)'s `?deck=N` focus mode is
unrelated and unaffected.

## Architecture

```
crates/
├── prolink/            # Pro DJ Link protocol, no app dependency
└── decktective-core/   # Domain layer, no UI dependency
    ├── model.rs        #   Normalized model (DeckState, TrackInfo, WaveformData…)
    ├── engine.rs       #   Engine trait + snapshot bookkeeping + event stream
    └── sim/            #   Scripted simulation (npm run dev:sim)
src-tauri/              # App shell: window, commands, engine → event bridge
src/renderer/           # React UI: deck panels + canvas waveforms
    └── src/model/      #   TypeScript generated from model.rs by `cargo test`
```

The UI only ever consumes the normalized model, and the model itself is generated
from the Rust definitions — a divergence between engine and renderer fails the
build rather than surfacing mid-set. The same engine layer also feeds the
[remote view](#remote-view) over a WebSocket, without touching the protocol.

### Porting status

| Phase | Scope                                                          | State |
| ----- | -------------------------------------------------------------- | ----- |
| 1     | App shell, domain model, simulation engine                     | done  |
| 2     | UDP layer: announce, discovery, status, beat, precise position | to do |
| 3     | Metadata over NFS, PDB/ANLZ parsing via `rekordcrate`          | to do |
| 4     | Remote database (dbserver) fallback                            | to do |

### Protocol notes

- Pro DJ Link on this hardware generation reports **no absolute playhead
  position**; Decktective anchors the playhead on the beat counter + beat grid and
  interpolates with the pitch (engine-side at 5 Hz, renderer-side at 60 fps).
- The **XDJ-XZ** exposes its two decks behind a single IP and always claims player
  numbers 1 and 2; Decktective announces itself as virtual player 5 to stay clear
  while remaining in the 1-6 range CDJs accept database queries from.

## Credits

- [Deep Symmetry's beat-link](https://github.com/Deep-Symmetry/beat-link) and the
  [DJ Link packet analysis](https://djl-analysis.deepsymmetry.org/) — the protocol
  research that makes all of this possible.
- [prolink-connect](https://github.com/evanpurkhiser/prolink-connect) — the
  TypeScript implementation this project ran on until the Rust port, and the
  reference the port follows.
- [rekordcrate](https://github.com/Holzhaus/rekordcrate) — Rust parser for
  Rekordbox device exports (PDB and analysis files).
