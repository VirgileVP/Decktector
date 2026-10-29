# Decktector 🕵️

**Website: [decktector-website.vercel.app](https://decktector-website.vercel.app/)**

Real-time visualizer for Pioneer **Pro DJ Link** decks (CDJ/XDJ), in the spirit of
TC Supply's BeatKontrol. Plug a computer (Windows, macOS or Linux) into the same
network as your players and get, for every deck:

- **Scrolling HD waveform** with a smoothly interpolated playhead and beat-grid ticks
- **Whole-track minimap** with played/remaining position
- **Track metadata**: title, artist, album, genre, key, label, artwork
- **Hot cues and memory cues**, with a per-deck cue list and countdown, plus the
  track's structure
- **Player state**: play/cue/pause, master, sync, on-air, pitch, live BPM, time remaining

Around the decks:

- **Clock and lineup** — the night's sets, with a progress bar and per-artist notes
- **Timecode output** — Art-Net TC, MIDI Time Code or LTC, following a chosen deck
  or the on-air one
- **[Remote view](#remote-view)** — a read-only copy of the screen in any browser on
  the local network
- **[Detachable panels](#detachable-panels)** — decks, clock, timecode or lineup in
  their own OS windows

Primary targets: **XDJ-XZ** and **XDJ-1000MK2** (works with 2 to 4 CDJ-2000-era players).

> **Status — 1.4.** Rust backend with a Tauri shell and a React UI. The Pro DJ Link
> UDP layer and both metadata routes (NFS and dbserver) are in place, with no
> virtual-player-number workaround needed on the primary route.
>
> **Validated against real hardware on 2026-08-19** — two CDJ-3000s and a
> DJM-900NXS2, linked off one USB stick. Discovery, status, beat, the `export.pdb`
> parser and both metadata routes run against live players, and every fix that
> session forced is pinned by tests quoting the captured bytes. Both decks came up
> with title, artist, BPM, HD waveform, beat grid and artwork within a second of
> connecting.
>
> **Known gap**: the dbserver fallback reads cues without their colours or
> comments — see [Porting status](#porting-status). See
> [Architecture](#architecture) for how the Rust core, Tauri shell and React UI
> fit together.

## Requirements

- The computer must be on the **same LAN** as the players (Ethernet into the booth
  switch, or directly into the XDJ's LINK port).
- **Rekordbox must NOT be running** on the same machine — it competes for the same
  virtual-player slot on the Pro DJ Link network.
- Tracks must come from Rekordbox-analyzed media (USB/SD) for metadata and waveforms
  to be available. Both export formats are read: the legacy `export.pdb` and
  Rekordbox 7's encrypted `exportLibrary.db` (DeviceLibraryPlus), used when a stick
  carries no `export.pdb`.

To build from source:

- [Node.js](https://nodejs.org) 22 (the version CI builds with) and npm.
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

```powershell
$env:DECKTECTOR_VCDJ_ID = 4; npm run dev   # Windows PowerShell
```

Never point it at a number a device already uses — two devices sharing one breaks the
network.

This behaviour is documented here because it is a hard-won constraint any change to
the network layer has to preserve.

## Getting started

```sh
npm install

# Run against real players on the network
npm run dev

# Four simulated CDJs you drive by hand from a browser page (no hardware needed)
npm run dev:sim
```

```sh
npm run test:rust    # Rust tests, and regenerates the renderer's model types
npm run lint:rust    # clippy, warnings as errors
npm test             # renderer tests (Vitest)
npm run lint         # renderer lint (ESLint)
npm run typecheck    # renderer types
```

### Driving the simulated decks by hand

`npm run dev:sim` starts the app with four simulated CDJs and opens a control
page in your default browser (`http://127.0.0.1:1430/sim-cdj.html`, development
builds only). It looks like a row of CDJ-3000s with a mixer in the middle: load,
play/pause, cue, pitch fader with tempo range, jog wheel (top to scratch, edge
to nudge), hot cues A-H, 4/8-beat loops, sync and master, and a channel fader
per deck that puts it on air. The app reflects everything the page does.

The decks play tracks analysed by rekordbox. Copy a rekordbox USB export (the
whole drive: the `PIONEER/` tree with `rekordbox/export.pdb` or
`exportLibrary.db`, `USBANLZ/` and the artwork, plus the `Contents/` folder
holding the audio files) into `sim-library/` at the repository root — it is
gitignored, since it holds your library's metadata and music.
Point `DECKTECTOR_SIM_EXPORT` at another folder (relative to the repository
root, or absolute) to use that instead. With no usable export the decks fall
back to the generated tracks, and the page says so.

The page plays sound when the audio files are there: each deck's track is
decoded in the browser and follows the deck's position, tempo, loops and
scratching. Pitch follows tempo, as on a CDJ with Master Tempo off (no key
lock). Browsers block audio until you interact with the page, so click it once.
Tracks whose audio file was not copied stay silent and the page marks them; if
no track has audio the app logs a warning.

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

A `v*` tag's release takes its notes from that version's section of
[`CHANGELOG.md`](CHANGELOG.md), followed by the downloads table in
`.github/release-footer.md`; `node scripts/release-notes.mjs <version>` prints them
locally. The workflow fails early if the tag doesn't match the `package.json` version
or the changelog has no section for it.

Rekordbox 7's `exportLibrary.db`, which `prolink` reads when a stick has no
`export.pdb`, is SQLCipher-encrypted, so the build links a libcrypto. Nothing to
run by hand on any platform:

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
link, and **Rotate link & disconnect devices** does the same on demand. This is a
guard against someone on the same network stumbling onto the feed, not hardened
authentication: **a venue network is not a trusted network**, and nothing sensitive
should be exposed through this beyond what is already on screen.

**Require access token** (on by default) can be turned off for a private booth
network: the link becomes a plain address that stays the same across restarts, and
anyone who can reach the port sees the feed.

The same settings tab narrows who can reach it:

- **Encrypt traffic (HTTPS / WSS)** serves the page and its WebSocket over TLS, with
  a self-signed certificate generated in memory each time the server starts.
  Browsers warn about it on first visit; accept it from the warning page
  (_Advanced_ / _Show details_ on most mobile browsers) to connect.
- **Network interface** binds the server to one adapter — the booth network or a
  private hotspot — instead of every interface, so it is never exposed on public
  venue Wi-Fi.
- **Max connected devices** caps simultaneous viewers (4 by default).

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
deck or the clock on top, drop its background, and key out the rest.

Each window also chooses what it shows, from the display options button in its
chrome. A deck window offers the same **Simple view** / **Full** presets and the
same toggles as the Deck view settings tab, applied to that window only. It
starts from the main window's Deck view settings, then keeps its own: two
windows of the same deck can show different things, and changing the settings
tab does not touch a window already open (its **Match main window** button
copies them back). The Time window picks among clock, timecode and lineup the
same way. Open panel windows are remembered across restarts, in their last
position and with what they show.

This is desktop-only. The [remote view](#remote-view)'s `?deck=N` focus mode is
unrelated and unaffected.

## Architecture

```
crates/
├── prolink/                # Pro DJ Link protocol, no app dependency
│   └── src/
│       ├── nfs/            #   NFS route: export.pdb, exportLibrary.db, ANLZ cues
│       ├── dbserver/       #   dbserver route (fallback)
│       └── bin/            #   cdj-sim, prolink-capture, prolink-pcap, prolink-session
├── prolink-engine/         # Engine over real players: metadata fetch, playhead position
├── decktector-core/        # Domain layer, no UI dependency
│   └── src/
│       ├── model.rs        #   Normalized model (DeckState, TrackInfo, WaveformData…)
│       ├── engine.rs       #   Engine trait + snapshot bookkeeping + event stream
│       ├── structure/      #   Track structure analysis
│       └── sim/            #   Scripted simulation (npm run dev:sim)
├── decktector-net/         # Network interface discovery
├── decktector-remote/      # Remote view: HTTP/WebSocket server, token, TLS, QR code
├── decktector-sim-control/ # Control server for the hand-driven simulated CDJs (dev only)
├── decktector-timecode/    # Timecode output: Art-Net TC, MTC, LTC
└── sim-audio/              # Simulator fixtures built from audio files
src-tauri/                  # App shell: windows, commands, engine → event bridge
src/renderer/               # React UI: deck panels + canvas waveforms
    └── src/model/          #   TypeScript generated from model.rs by `cargo test`
```

The UI only ever consumes the normalized model, and the model itself is generated
from the Rust definitions — a divergence between engine and renderer fails the
build rather than surfacing mid-set. The same engine layer also feeds the
[remote view](#remote-view) over a WebSocket and the timecode output, without
touching the protocol.

### Porting status

| Phase | Scope                                                                                                                                                                             | State                                                      |
| ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| 1     | App shell, domain model, simulation engine                                                                                                                                        | done                                                       |
| 2     | UDP layer: announce, discovery, status, beat, precise position                                                                                                                    | done                                                       |
| 3     | Metadata over dbserver: title/artist/album/genre/key/label/comment/bpm, beat grid, HD waveform, cues                                                                              | done (no cue colours or comments, no artwork — see below)  |
| 4     | Metadata over NFS: `export.pdb` or `exportLibrary.db` + `.EXT`/`.DAT` analysis files, cues included, needs no virtual player number — the primary route, dbserver is the fallback | done — validated against a CDJ-3000's own media, see below |

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

Phase 3 reads the **basic** cue list (`crates/prolink/src/dbserver/cues.rs`):
hot cues, memory cues and loops, pinned on replies two CDJ-2000NXSs gave real
decks in [`usr-ein/prolink`](https://github.com/usr-ein/prolink)'s capture
corpus. Two gaps stay open. The **extended** cue list, which adds colours and
comments, is not decoded: no real reply to it has been captured, and its entries
vary in length, so a wrong offset would desync every cue after it. And dbserver
has no artwork route wired up. Both are moot for a deck reading real media, since
phase 4 gets them through NFS, which is part of why dbserver was demoted to the
fallback route.

**Cue colours and comments come from the NFS route only.** `rekordcrate` 0.3 can't read cues on
current hardware: a CDJ-3000's `.EXT` cue list (`PCO2`) is one of the sections it
rejects outright (real players write cue type `1`, which its enum doesn't allow),
and the older `.DAT` list (`PCOB`) comes back empty even for a track whose raw
bytes plainly hold three hot cues. Both formats are therefore hand-rolled in
`crates/prolink/src/nfs/cues.rs` — hot cues A–H, memory cues, loops, comments and
colours — pinned by tests on bytes extracted from real `.EXT` files. The `.EXT`
list is tried first, then the `.DAT` one; on a Rekordbox 7 stick, cues stored in
`exportLibrary.db` fill in when neither analysis file holds any.

**Track structure comes from rekordbox's phrase analysis when there is one.** The
`.EXT`'s `PSSI` section holds the phrases rekordbox found — masked since
rekordbox 6 — and `rekordcrate` 0.3 keeps them private, so
`crates/prolink/src/nfs/phrases.rs` reads them, after Deep Symmetry's analysis,
pinned on a rekordbox export. A track without phrase analysis, or read over the
dbserver fallback, gets a structure inferred from its waveform and beat grid
instead (`crates/decktector-core/src/structure/`), shown as a guess.

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
  HD color waveform — cues are hand-rolled, see above) with the `rekordcrate` crate
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

- Only the CDJ-3000 reports an **absolute playhead position** (its precise-position
  packets, ~every 30 ms). Every older player — the XDJ-XZ and XDJ-1000MK2 included —
  does not, so Decktector anchors the playhead on the beat counter + beat grid and
  interpolates with the pitch (engine-side at ~5 Hz, renderer-side at 60 fps); a
  CDJ-3000 falls back to the same estimate whenever its precise position goes stale
  (`crates/prolink-engine/src/position.rs`).
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
  Rekordbox analysis files (`.DAT`/`.EXT`), used as-is for the beat grid and
  waveforms. Its cue lists don't parse on current hardware, so
  `crates/prolink/src/nfs/cues.rs` reads those instead; and its own `export.pdb`
  parser's row fields are private in the published version, so
  `crates/prolink/src/nfs/pdb.rs` is a from-scratch minimal parser, using
  `rekordcrate`'s source as a layout reference — see
  [Porting status](#porting-status) for the confidence caveat that comes with
  that. The phrase-analysis tests use a `PSSI` section from one of its demo
  track exports.
- [Deep Symmetry's DJ Link analysis](https://djl-analysis.deepsymmetry.org/) —
  the reference for the phrase analysis layout, its masking and its phrase names.
- [usr-ein/prolink](https://github.com/usr-ein/prolink) — its capture corpus of
  two CDJ-2000NXSs holds the cue list replies the dbserver cue parser is pinned
  on.

## License

Decktector is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. See [LICENSE](LICENSE) for the full text.

Anyone who distributes Decktector or a work based on it, paid or not, must do so
under the same license and make the complete corresponding source code available
to its recipients.
