# Decktective 🕵️

Real-time visualizer for Pioneer **Pro DJ Link** decks (CDJ/XDJ), in the spirit of
TC Supply's BeatKontrol. Plug your Mac into the same network as your players and get,
for every deck:

- **Scrolling HD waveform** with a smoothly interpolated playhead and beat-grid ticks
- **Whole-track minimap** with played/remaining position
- **Track metadata**: title, artist, album, genre, key, label, artwork
- **Player state**: play/cue/pause, master, sync, on-air, pitch, live BPM, time remaining

Primary targets: **XDJ-XZ** and **XDJ-1000MK2** (works with 2 to 4 CDJ-2000-era players).

## Requirements

- The computer must be on the **same LAN** as the players (Ethernet into the booth
  switch, or directly into the XDJ's LINK port).
- **Rekordbox must NOT be running** on the same machine — it competes for the same
  virtual-player slot on the Pro DJ Link network.
- Tracks must come from Rekordbox-analyzed media (USB/SD) for metadata and waveforms
  to be available.
- Node must be built for the same architecture as the machine. The Rekordbox database
  downloaded from the players is read through `better-sqlite3`, a native module, and
  `postinstall` builds it for whatever architecture `node` reports: an x64 Node under
  Rosetta on an Apple Silicon Mac produces a module arm64 Electron cannot load, and
  every metadata lookup fails with a `dlopen` architecture error. Check with
  `node -p process.arch`, and rebuild after switching Node:

```sh
npx electron-builder install-app-deps --arch arm64
```

## Metadata on a full four-player network

Players only answer media-slot queries and metadata requests coming from a virtual
CDJ numbered 1-4. With four players on the network all four numbers are taken, ours
is 5, and both of those doors are closed — the track database is then read straight
off the player over NFS instead, which needs no player number (see
`src/engine/prolink/media-slot-bypass.ts`).

A CDJ-3000 can be renumbered to player 5 or 6, which frees a number in range. Set
one player that way, then claim the freed number:

```sh
DECKTECTIVE_VCDJ_ID=4 npm run dev
```

Never point it at a number a device already uses — two devices sharing one breaks the
network. Every metadata lookup is traced to `prolink-diagnostic.log` in the log
directory (`./logs` in dev, the app's user-data directory in packaged builds).

## Getting started

```sh
npm install

# Run against real hardware
npm run dev

# Run with two simulated decks (no hardware needed)
npm run dev:sim
```

## Building

```sh
npm run build:mac    # .dmg (arm64 + x64) — must run on macOS
npm run build:win    # .exe installer
```

CI (GitHub Actions) builds unsigned macOS dmg and Windows installer artifacts on
every push to master — see `.github/workflows/build.yml`. macOS builds are
unsigned for now: right-click → Open on first launch, or configure a Developer ID
certificate in CI to sign and notarize.

## Architecture

```
src/
├── engine/          # Domain layer, no Electron/UI dependency
│   ├── types.ts     #   Normalized model (DeckState, TrackInfo, WaveformData…)
│   ├── engine.ts    #   ProlinkEngine interface + snapshot bookkeeping
│   ├── prolink/     #   Real implementation on top of prolink-connect
│   └── sim/         #   Scripted simulation (npm run dev:sim)
├── main/            # Electron main: window + engine → IPC bridge
├── preload/         # contextBridge: typed window.decktective API
└── renderer/        # React UI: deck panels + canvas waveforms
```

The UI only ever consumes the normalized model, so the protocol backend can be
swapped (e.g. for a beat-link sidecar) without touching components. The same
engine layer can later feed a web server for remote viewing.

### Protocol notes

- Pro DJ Link on this hardware generation reports **no absolute playhead
  position**; Decktective anchors the playhead on the beat counter + beat grid and
  interpolates with the pitch (engine-side at 5 Hz, renderer-side at 60 fps).
- The **XDJ-XZ** exposes its two decks behind a single IP and always claims player
  numbers 1 and 2; Decktective announces itself as virtual player 5 to stay clear
  while remaining in the 1-6 range CDJs accept database queries from.

## Credits

- [prolink-connect](https://github.com/evanpurkhiser/prolink-connect) — TypeScript
  implementation of the Pro DJ Link protocol.
- [Deep Symmetry's beat-link](https://github.com/Deep-Symmetry/beat-link) and the
  [DJ Link packet analysis](https://djl-analysis.deepsymmetry.org/) — the protocol
  research that makes all of this possible.
