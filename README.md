# Decktector 🕵️

Real-time visualizer for Pioneer **Pro DJ Link** decks (CDJ/XDJ), in the spirit of
TC Supply's BeatKontrol. Plug your Mac into the same network as your players and get,
for every deck:

- **Scrolling HD waveform** with a smoothly interpolated playhead and beat-grid ticks
- **Whole-track minimap** with played/remaining position
- **Track metadata**: title, artist, album, genre, key, label, artwork
- **Player state**: play/cue/pause, master, sync, on-air, pitch, live BPM, time remaining

Primary targets: **XDJ-XZ** and **XDJ-1000MK2** (works with 2 to 4 CDJ-2000-era players).

> **Status — 1.0, the Rust rewrite.** The app now runs on a Rust backend with
> Tauri, keeping the same UI. The shell, domain model, simulation engine, the Pro
> DJ Link UDP layer and both metadata routes (NFS and dbserver —
> title/artist/album/genre/key/label/bpm, beat grid, HD waveform, artwork) are in
> place, with no virtual-player-number workaround needed.
>
> **Validated against real hardware on 2026-08-19** — two CDJ-3000s and a
> DJM-900NXS2, linked off one USB stick. Discovery, status, beat, the `export.pdb`
> parser and both metadata routes run against live players, and every fix that
> session forced is pinned by tests quoting the captured bytes. Both decks came up
> with title, artist, BPM, HD waveform, beat grid and artwork within a second of
> connecting.
>
> **Known gap**: hot cues and memory cues are not read on either route — see
> [Porting status](#porting-status). See [Architecture](#architecture) for how
> the Rust core, Tauri shell and React UI fit together.

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

Decktector tries NFS first, which needs no virtual player number at all — this
should just work on any network size, including four CDJs with all of 1-4 taken.
dbserver is the fallback if NFS doesn't answer (e.g. an unsupported media slot),
and _that_ route only answers a virtual CDJ numbered 1-4 on players older than the
CDJ-3000. Decktector claims 7 by default (the conventional virtual-CDJ number,
which no physical player can take), falling back to 6, 5, then 4 down to 1 when
those are taken — so on a full four-player network of older players the dbserver
fallback alone wouldn't get metadata either.

A CDJ-3000 can be renumbered to player 5 or 6, which frees a number in range, for
the dbserver fallback specifically. Set one player that way, then claim the freed
number:

```sh
DECKTECTOR_VCDJ_ID=4 npm run dev
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

### Simulating from a recording

The simulated decks generate their tracks by default: a shaped energy envelope,
an exactly regular beat grid, cues placed by rule. Plausible, but nothing a
real track looks like. A recording of a real booth network can replace them.

On a linked booth the players read each other's media over NFS, so a Wireshark
capture of the segment already holds `export.pdb` and the `.DAT`/`.EXT` files
carrying each track's beat grid, cues and HD waveform. Two commands turn one
into simulator input:

```sh
# Import a Wireshark recording, rebuilding the files its NFS traffic carried
cargo run -p prolink --bin prolink-pcap -- --in booth.pcapng

# Turn the recovered database and analysis files into track fixtures
cargo run -p prolink-engine --bin sim-fixtures -- --session capture/capture-YYYYMMDD-HHMMSS

# Run the simulation on them
npm run dev:sim:recorded
```

### Simulating from audio files

Without a booth to record, a folder of audio files gets most of the way there.
Drop them in `capture/audio/` and build the same fixtures:

```sh
cargo run -p sim-audio -- --input capture/audio
npm run dev:sim:recorded
```

The waveform, the duration, the artwork and the tags come from the files
themselves. The tempo, the beat grid and the downbeat are detected, so the grid
is exactly regular where a real one drifts, and a track whose tags carry no BPM
can land an octave off. The cues are derived from each track's own section
analysis, since no cue a DJ set is ever stored in an audio file.

`prolink-capture` sessions work as input to `sim-fixtures` just as well; the
`.pcapng` route exists because a Wireshark recording survives the capture tool
itself being wrong, and sees traffic no single listener socket is ever sent.

To exercise the real parsing path instead of the simulated engine, replay the
recorded packets with their original timing and run the app normally:

```sh
cargo run -p prolink --bin cdj-sim -- --replay capture/capture-YYYYMMDD-HHMMSS
```

Captures and fixtures hold a real DJ's library metadata, and audio files are
someone's work; `capture/` and the fixtures built from it are gitignored.

## Building

```sh
npm run build
```

Produces an NSIS installer on Windows, a `.dmg` on macOS and `.AppImage`/`.deb` on
Linux — each must be built on its own platform.

Without all three machines to hand, the `Build` workflow does it: run it from the
Actions tab (or push a `v*` tag) and it builds every platform on its own runner,
then attaches the installers to a GitHub release — the tag itself for a `v*` push,
a `build-<short sha>` prerelease for anything else. Download from the Releases
page rather than the run's page; these are release assets, not Actions artifacts.

The Rekordbox database `prolink` reads is SQLCipher-encrypted, so the build links a
libcrypto. Nothing to run by hand on any platform:

- **Windows** needs the OpenSSL development package, which `npm install` installs
  through winget if it is missing, then stages statically under `target/` — so the
  packaged app carries no `libcrypto` DLL alongside it. Every npm script that reaches
  Cargo goes through `scripts/with-openssl.mjs`, and `.cargo/config.toml` points a
  bare `cargo build` (and rust-analyzer) at the same staged archive.
- **Linux** links the system libcrypto; `npm install` warns if the development
  package — `libssl-dev`, `openssl-devel` or `openssl` — is missing.
- **macOS** needs nothing: SQLCipher falls back to CommonCrypto.

`scripts/openssl-linkage.mjs` holds that logic and explains the details.

### Installing without an internet connection

Windows apps render through the Edge WebView2 runtime, which the installer would
normally fetch from Microsoft on first run. `bundle.windows.webviewInstallMode` is
set to `offlineInstaller`, so the runtime is embedded instead: the resulting
`Decktector_<version>_x64-setup.exe` (~270 MB) installs on a machine that has
never been online, and needs no administrator rights — it installs per user.
Building it does require an internet connection, since Tauri downloads the runtime
at bundle time and caches it under `%LOCALAPPDATA%\tauri`.

The macOS `.dmg` and the Linux `.AppImage` are self-contained in the same way —
they render through the system WebKit and bundle what they need. The `.deb`, by
contrast, resolves its dependencies through `apt` at install time, so prefer the
`.AppImage` for offline machines.

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
└── decktector-core/   # Domain layer, no UI dependency
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

| Phase | Scope                                                                                                                                        | State                                                      |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| 1     | App shell, domain model, simulation engine                                                                                                   | done                                                       |
| 2     | UDP layer: announce, discovery, status, beat, precise position                                                                               | done                                                       |
| 3     | Metadata over dbserver: title/artist/album/genre/key/label/comment/bpm, beat grid, HD waveform                                               | done (cues and artwork not implemented — see below)        |
| 4     | Metadata over NFS: `export.pdb` + `.EXT`/`.DAT` analysis files, needs no virtual player number — the primary route, dbserver is the fallback | done — validated against a CDJ-3000's own media, see below |

Phase 2 listens for real player traffic passively rather than running the full
virtual-CDJ channel-claim handshake: its exact byte layout isn't pinned down
precisely enough to transmit safely onto a live network yet. Decktector picks a
free virtual player number and broadcasts the same settled-state keep-alive a
real player does — it just never contests a number another device claims first.
That keep-alive is not optional: players send their status packets **unicast,
only to peers whose keep-alive they have seen**, so a purely passive listener
sees discovery and beats and no status at all (see
`crates/prolink/src/presence.rs`'s module doc). It also has to announce the real
address and MAC of the adapter facing the booth, which is why picking the right
one matters on a multi-homed laptop.

Phase 3's two gaps stay open — dbserver's cue-list wire format
(`crates/prolink/src/dbserver/cues.rs`) is variable-length and still unconfirmed
(a wrong offset would desync every cue after it, not just misread one field), and
dbserver has no artwork route wired up. Artwork is moot for a deck reading real
media, since phase 4 gets it through NFS, which is part of why dbserver was
demoted to the fallback route.

**Cues are the one thing 1.0 does not read, on either route.** The NFS route
can't either: a CDJ-3000's `.EXT` cue list (`PCO2`) is one of the sections
`rekordcrate` 0.3 rejects outright, and its older `.DAT` list (`PCOB`) comes back
empty even for a track whose raw bytes plainly hold three hot cues. Closing this
means hand-rolling cue parsers for both formats, against the bytes the
2026-08-19 session captured. Until then a track arrives with no cues rather than
with wrong ones.

**Phase 4 carries real, explicit confidence differences worth reading before
trusting it at a gig:**

- **High confidence**: the transport itself. What was assumed to be a
  "non-standard NFSv2" server turned out to be standard ONC-RPC/portmapper/NFSv2
  (RFC 1057/1094) almost everywhere — only filenames/paths are encoded UTF-16LE
  instead of ASCII. `crates/prolink/src/nfs/{xdr,rpc,portmap,mount,procedures,client}.rs`
  implement this, tested against an in-process fake UDP server
  (`crates/prolink/tests/nfs_fake_server.rs`, not `#[ignore]`-gated) covering the
  full portmap → mount → lookup-chain → multi-chunk read flow and the
  give-up-cleanly-on-no-response path.
- **Validated against hardware**: parsing the fetched analysis files (beat grid,
  HD color waveform — not cues, see above) with the `rekordcrate` crate
  (`crates/prolink-engine/src/metadata/nfs.rs`), whose hot-cue color table
  (`crates/prolink-engine/src/metadata/colors.rs`) is copied verbatim from its own
  doc comments. Two things a real CDJ-3000 forced: the file is read **section by
  section**, because `ANLZ::read` abandons the whole file over one section that
  crate can't decode and a CDJ-3000's `.EXT` contains exactly that; and both the
  `.EXT` and the `.DAT` are fetched, because the beat grid `rekordcrate` reads
  (`PQTZ`) is only in the `.DAT` while the colour waveform is only in the `.EXT`.
- **Validated against hardware**: the hand-rolled `export.pdb` database parser
  (`crates/prolink/src/nfs/pdb.rs`). It was written from a reading of
  `rekordcrate`'s source — whose own row fields are private in the published
  version — and that reading got essentially every structural constant wrong: it
  found _zero_ tracks in the first real database it met. Every offset in it now
  comes from that file, cross-checked field by field against the same tracks'
  metadata as answered independently by the player's dbserver. The file's module
  doc lists what was wrong, in case any of it resurfaces.
- **Confirmed**: the MOUNT path per media slot (`"/B/"`/`"/C/"` for SD/USB,
  `crates/prolink/src/nfs/client.rs`'s `slot_path`) — `"/C/"` mounts a CDJ-3000's
  USB slot. Note that a mount failure is the expected answer from a player with an
  empty slot, which is the normal case on a linked booth: metadata is fetched from
  the player holding the media, not from the deck playing it.

### Protocol notes

- Pro DJ Link on this hardware generation reports **no absolute playhead
  position**; Decktector anchors the playhead on the beat counter + beat grid and
  interpolates with the pitch (engine-side at 5 Hz, renderer-side at 60 fps).
- The **XDJ-XZ** exposes its two decks behind a single IP and always claims player
  numbers 1 and 2; Decktector announces itself as virtual player 7 to stay clear
  of every number physical players can take (see
  [Metadata on a full four-player network](#metadata-on-a-full-four-player-network)
  for the dbserver fallback's 1-4 constraint).
- A track loaded over **link** belongs to another player: a deck's status packet
  names the player whose media it came from, and metadata has to be asked of that
  player, in that player's slot, by that player's number. Two decks fed by one USB
  stick is the ordinary booth setup, and asking the deck itself gets an empty menu
  over dbserver and a failed mount over NFS.
- A player reports itself **on air** only when its player number matches the mixer
  channel it is plugged into — the mixer announces on-air per channel, not per
  device. That flag is only trustworthy with exactly one Pro DJ Link mixer on
  the network (none: it is never set; two: both announce device 33 and the
  mapping no longer matches the cabling), so the UI decides which decks look
  live from the **Live deck detection** setting: `auto` (default) follows the
  mixer flag with one mixer and playback otherwise, `mixer` always follows the
  flag, `playback` treats a deck as live while it is playing. The remote view
  never receives the device list, so `auto` follows playback there.

## Credits

- [rekordcrate](https://github.com/Holzhaus/rekordcrate) — Rust parser for
  Rekordbox analysis files (`.DAT`/`.EXT`, beat grid/waveform/cues), used as-is.
  Its own `export.pdb` parser's row fields are private in the published version,
  so `crates/prolink/src/nfs/pdb.rs` is a from-scratch minimal parser instead,
  using `rekordcrate`'s source as a layout reference — see
  [Porting status](#porting-status) for the confidence caveat that comes with
  that.

## License

Decktector is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [LICENSE](LICENSE) for the full text.

Any distributed copy or derivative work, paid or not, must ship with its
complete source code under the same license.
