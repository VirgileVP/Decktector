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
> Rust backend with Tauri, keeping the same UI. The shell, domain model, simulation
> engine, the Pro DJ Link UDP layer, and both metadata routes (NFS and dbserver —
> title/artist/album/genre/key/label/bpm, beat grid, HD waveform, cues, artwork) are
> in place — `npm run dev` connects to real players and fetches real track info with
> no virtual-player-number workaround needed. **None of this has been run against
> real hardware yet** (see [Porting status](#porting-status) for exactly which parts
> carry how much confidence — the NFS transport and metadata query protocol are
> cross-checked against hardware-validated reference implementations; the PDB
> database parser is this codebase's own reconstruction and is the shakiest part).
> The Electron implementation remains on the `master` branch. See
> [Architecture](#architecture) for the plan.

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

Decktective tries NFS first, which needs no virtual player number at all — this
should just work on any network size, including four CDJs with all of 1-4 taken.
dbserver is the fallback if NFS doesn't answer (e.g. an unsupported media slot),
and *that* route only answers a virtual CDJ numbered 1-4 — Decktective defaults to
5 to stay clear of numbers real hardware self-assigns, so on a full four-player
network the dbserver fallback alone wouldn't get metadata either.

A CDJ-3000 can be renumbered to player 5 or 6, which frees a number in range, for
the dbserver fallback specifically. Set one player that way, then claim the freed
number:

```sh
DECKTECTIVE_VCDJ_ID=4 npm run dev
```

Never point it at a number a device already uses — two devices sharing one breaks the
network.

This behaviour is documented here because it is a hard-won constraint the port has
to preserve.

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

| Phase | Scope                                                                    | State |
| ----- | ------------------------------------------------------------------------- | ----- |
| 1     | App shell, domain model, simulation engine                                | done  |
| 2     | UDP layer: announce, discovery, status, beat, precise position            | done  |
| 3     | Metadata over dbserver: title/artist/album/genre/key/label/comment/bpm, beat grid, HD waveform | done (cues and artwork not implemented — see below) |
| 4     | Metadata over NFS: `export.pdb` + `.EXT` analysis files, needs no virtual player number — the primary route, dbserver is the fallback | done — **untested against real hardware, see below** |

Phase 2 listens for real player traffic passively rather than running the full
virtual-CDJ channel-claim handshake Deep Symmetry documents: the handshake's exact
byte layout isn't pinned down precisely enough to transmit safely onto a live
network yet, and passive listening is enough for status/beat traffic (see
`crates/prolink/src/presence.rs`'s module doc). Decktective still shows up as a
peer on the network — it picks a free virtual player number and broadcasts the
same settled-state keep-alive a real player does — it just never contests a number
another device claims first.

Phase 3's two gaps stay open — dbserver's cue-list wire format
(`crates/prolink/src/dbserver/cues.rs`) is variable-length and still unconfirmed
(a wrong offset would desync every cue after it, not just misread one field), and
dbserver has no artwork route wired up. Both are moot for a deck reading real media
now that phase 4 gets cues and artwork through NFS instead, which is why dbserver
was demoted to the fallback route — see phase 4 below.

**Phase 4 carries real, explicit confidence differences worth reading before
trusting it at a gig.** Cross-checked against Deep Symmetry's `crate-digger` (the
hardware-validated Java reference for exactly this NFS-based read):

- **High confidence**: the transport itself. What was assumed to be a
  "non-standard NFSv2" server turned out to be standard ONC-RPC/portmapper/NFSv2
  (RFC 1057/1094) almost everywhere — only filenames/paths are encoded UTF-16LE
  instead of ASCII, confirmed straight from `crate-digger`'s own hand-written
  protocol spec. `crates/prolink/src/nfs/{xdr,rpc,portmap,mount,procedures,client}.rs`
  implement this, tested against an in-process fake UDP server
  (`crates/prolink/tests/nfs_fake_server.rs`, not `#[ignore]`-gated) covering the
  full portmap → mount → lookup-chain → multi-chunk read flow and the
  give-up-cleanly-on-no-response path.
- **High confidence**: parsing the fetched `.EXT` analysis file (beat grid, HD
  color waveform, cues) — done with the `rekordcrate` crate
  (`crates/prolink-engine/src/metadata/nfs.rs`), whose `anlz` module has a clean,
  documented public API and whose hot-cue color table
  (`crates/prolink-engine/src/metadata/colors.rs`) is copied verbatim from its own
  doc comments.
- **Low confidence, flagged in code**: the hand-rolled `export.pdb` database
  parser (`crates/prolink/src/nfs/pdb.rs`) — `rekordcrate`'s own `pdb` module
  parses this exact format but its row fields (title, artist id, ...) are private
  in the published crate version, so this codebase reconstructs a minimal parser
  from `rekordcrate`'s source as a *layout reference* rather than depending on it.
  That reference reading was itself only partially certain (page-type numeric
  codes, `Track`'s exact field order) — this is genuinely the shakiest code in the
  whole Rust port, more so than any UDP/TCP protocol work, and the one most likely
  to need fixing once it meets a real `export.pdb`.
- **Unconfirmed, needs a real network to check**: the MOUNT path per media slot
  (`"/B/"`/`"/C/"` for SD/USB, `crates/prolink/src/nfs/client.rs`'s `slot_path`) —
  a wrong mapping fails at the very first NFS call, which at least makes it an
  obvious failure rather than a subtle one.

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
  Rekordbox analysis files (`.DAT`/`.EXT`, beat grid/waveform/cues), used as-is.
  Its own `export.pdb` parser's row fields are private in the published version,
  so `crates/prolink/src/nfs/pdb.rs` is a from-scratch minimal parser instead,
  using `rekordcrate`'s source as a layout reference — see
  [Porting status](#porting-status) for the confidence caveat that comes with
  that.
- [crate-digger](https://github.com/Deep-Symmetry/crate-digger) — Deep
  Symmetry's Java reference implementation for reading a player's database over
  NFS, the source this port's `prolink::nfs` transport layer was cross-checked
  against.
