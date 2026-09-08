# Hardware capture plan

Implementation plan for the tooling needed before the first real Pro DJ Link
hardware session (a CDJ + DJM kit, available for a single short window).

## Status: the session happened

The first hardware session ran on **2026-08-19** against two CDJ-3000s and a
DJM-900NXS2, and the plan below did its job — every parser question it was
written to answer is now answered against real bytes. What it found, since
each item invalidates something this document takes as read:

- **A passive listener records no player status at all.** Players unicast
  status only to peers whose keep-alive they have seen, so `prolink-capture`
  now announces itself as a virtual CDJ by default. Without it there is no
  status traffic, and with no status there is no metadata either — every
  later phase keys off the currently-loaded track. The keep-alive has to
  carry the adapter's real IP _and_ its real MAC to work.
- **The dbserver framing was wrong end to end** — a missing connection
  greeting, untagged header fields, the wrong argument-type codes, and menu
  queries built from four separate byte arguments instead of one packed
  word. Real players answered by closing the connection.
- **The `export.pdb` parser found zero tracks** in a real database. Nearly
  every structural constant in it was wrong; see that file's module doc for
  the list.
- **Metadata has to be asked of the player holding the media**, which on a
  linked booth is not the deck doing the playing.
- **`rekordcrate` 0.3 cannot read a CDJ-3000 `.EXT` in one piece**, so both
  the engine and the replay harness now walk ANLZ sections individually.

Reference bytes from that session — a real `export.pdb`, the dbserver
exchanges, decoded status — are under `capture/reference-20260819/`, and
the tests that pin this behaviour quote them directly.

## Why this exists

Every metadata code path in this repo — the whole NFS route and the
`export.pdb` parser especially — has never run against real hardware. See
`README.md`'s "Porting status" table and `crates/prolink/src/nfs/pdb.rs`'s
module doc, which states plainly that its page-type codes, `Track` header
field order and `DeviceSQLString` encoding rest on a single partially
self-contradictory secondary source.

Hardware time is the scarce resource, not developer time. So the session must
not be a debugging session. It must be a **capture** session: it produces raw
bytes on disk, and every parser question gets answered afterwards, offline,
against those bytes, as many times as needed.

**The governing rule for everything below: write raw bytes to disk before
attempting to parse them, and never let a parse failure abort a capture run.**
A tool that panics on-site and leaves nothing behind has wasted the whole
session. A tool that dumps 12 MB of bytes it could not interpret has succeeded
completely.

## Scope

In scope:

1. A `prolink-capture` binary that records everything a live network offers.
2. A replay test harness that runs the existing parsers over recorded captures.
3. An on-site runbook, written to be followed off a phone with no tooling.
4. Pre-session validation of all of the above against the existing simulators.

Out of scope — do not touch these:

- Implementing the virtual-CDJ channel-claim handshake. The capture will
  supply the bytes for it; writing it comes later.
- The cues/artwork gap on the dbserver route (`README.md` phase 3).
- Anything under `src/renderer/`, or the in-flight track-structure work in
  `crates/decktective-core/src/structure/`.

## House rules for this work

- Follow the conventions already in `crates/prolink/`: module-level doc
  comments explaining _why_, manual argument parsing (no `clap` — see
  `crates/prolink/src/bin/cdj-sim.rs` for the established pattern), and
  `tracing` for diagnostics.
- Conventional Commits, one commit per task below.
- Every bug fix ships with a regression test; this crate already has a test
  setup (`crates/prolink/tests/`).
- No new workspace dependency without justifying it in the plan. Two are
  justified below; add no others.

---

## Task 1 — Capture record format and writer

**File:** `crates/prolink/src/capture.rs` (new), exported from
`crates/prolink/src/lib.rs`.

A capture session is a directory. Layout:

```
capture-YYYYMMDD-HHMMSS/
  session.json            tool args, git sha, host interfaces, start/end time
  udp.jsonl               one line per received UDP packet
  devices.jsonl           discovery snapshots, one line per change
  status-decoded.jsonl    best-effort decode; may legitimately be empty
  capture.log             full tracing output
  notes.md                empty template for hand-written timestamps
  nfs/<ip>-<slot>/
    export.pdb            raw, exactly as read off the wire
    analysis/<id>.DAT     raw
    analysis/<id>.EXT     raw
    artwork/<id>.bin      raw
    parse-report.json     what parsed, what did not, and why
  dbserver/<ip>/
    metadata-<id>.bin     raw response bytes per query
    beatgrid-<id>.bin
    cues-ext-<id>.bin
    cues-basic-<id>.bin
    artwork-<id>.bin
    parse-report.json
```

`udp.jsonl` line format — one JSON object per line, payload hex-encoded:

```json
{ "t": 1755400000123456, "port": 50002, "src": "192.168.1.31", "len": 284, "hex": "5173707431..." }
```

Requirements:

- **JSONL, not a binary format.** A truncated final line from a hard
  unplug is discardable and the rest of the file still parses. It is also
  readable with no custom tooling on-site, which matters when something looks
  wrong and the only question worth answering is "are we recording or not".
- **Hex, not base64.** Encoding hex is a four-line loop; base64 would mean a
  new dependency in `prolink` for no benefit here.
- Flush after every record. A capture that is buffered when the machine is
  unplugged is a capture that does not exist.
- Use `std::fs` with a `BufWriter`, not `tokio::fs` — the workspace's tokio
  feature set does not include `fs`, and the packet rate is trivial (roughly
  20 packets/sec at four decks × 200 ms status interval). Document this
  reasoning in the module doc so the blocking write is not mistaken for an
  oversight.
- `CaptureWriter::new(root: &Path)` creates the timestamped session directory
  and every subdirectory up front, so a later write never fails on a missing
  parent mid-session.

**Tests** (`crates/prolink/src/capture.rs`, `#[cfg(test)]`):

- write/read round-trip preserves payload bytes exactly
- a file with a truncated final line reads back every complete record and
  reports the truncation rather than erroring
- session directory creation is idempotent

**Commit:** `feat(capture): add a session recorder for raw Pro DJ Link traffic`

---

## Task 2 — The `prolink-capture` binary

**File:** `crates/prolink/src/bin/prolink-capture.rs` (new).

Four phases, each independent. **A failure in any phase logs and continues to
the next.** No phase may abort the process.

### Phase A — passive UDP capture

Bind `ANNOUNCE_PORT`, `BEAT_PORT`, `STATUS_PORT` via the existing
`prolink::bind_udp_socket`, and write every datagram to `udp.jsonl` with no
parsing whatsoever. This must work even if every parser in the crate is
broken, because it is the phase that captures the channel-claim handshake —
the one thing `crates/prolink/src/presence.rs` documents as missing and the
one thing that cannot be re-captured later without the hardware.

### Phase B — device inventory

Run `DeviceWatcher` and `StatusWatcher` alongside phase A. Append discovery
changes to `devices.jsonl` and decoded statuses to `status-decoded.jsonl`.
Print a live table of device number, name, IP, media slot and loaded track id
to stderr, so the operator can see at a glance that the network is being read.

Decoding is best-effort here: a `parse_status` returning `None` is recorded as
a decode failure in the JSONL and never stops phase A's raw recording.

### Phase C — NFS pull

For each `(player_ip, slot)` discovered in phase B — and for each pair passed
explicitly via `--player`/`--slot`:

1. `NfsClient::open(player_ip, bind_ip, slot)`
2. `read_file("PIONEER/rekordbox/export.pdb")` → **write the raw bytes to
   disk immediately**
3. only then `pdb::parse(&bytes)`, wrapped in `std::panic::catch_unwind`,
   recording the outcome in `parse-report.json`
4. if the parse succeeded, pull each track's `analyze_path` (the `.DAT`) _and_
   its `.EXT` sibling, plus artwork, writing each raw before any parsing.
   Cap at `--max-tracks` (default 50) so a large library cannot eat the whole
   session; prefer the currently-loaded track ids seen in phase B first.

Path and API references: `crates/prolink-engine/src/metadata/nfs.rs` has the
exact constants (`EXPORT_PDB_PATH`, `EXT_SUFFIX`) and the `.DAT` → `.EXT`
path derivation this phase should reuse rather than reinvent.

**The `--player`/`--slot` override is not optional.** It is what makes phase C
work when discovery or status parsing turns out to be broken on real
hardware — the operator reads the IP off the player's own display and passes
it directly. Without it, one bug in phase B costs the entire NFS capture.

### Phase D — dbserver pull

For each player, `Connection::open(player_ip, bind_ip, our_device_number)`,
then call each of `fetch_metadata`, `fetch_beat_grid`, `fetch_cues_extended`,
`fetch_cues_basic`, `fetch_artwork`, dumping raw response bytes per query.
Honour `DECKTECTIVE_VCDJ_ID` for the virtual device number, defaulting to 5,
and log loudly when the number is outside dbserver's accepted 1–4 range —
that is the documented reason this route can legitimately return nothing (see
`README.md`, "Metadata on a full four-player network").

### Arguments

```
prolink-capture [--out DIR] [--bind IP] [--player IP] [--slot usb|sd|cd]
                [--max-tracks N] [--duration-sec N] [--no-nfs] [--no-dbserver]
```

Default `--out` is `./capture`. Resolve `--bind` from `if-addrs` (already a
workspace dependency) when not given, and print the chosen interface — a
wrong interface on a multi-homed booth machine is a likely on-site failure
and must be visible without being asked for.

### Logging

Add `tracing-subscriber` as a dependency of `prolink`. Justification: every
protocol module in this crate already emits `tracing` events, and with no
subscriber installed the binary's log file is empty — the instrumentation
that exists specifically to explain protocol failures would be invisible
during the one session where it matters. Write to both stderr and
`capture.log`, at `DEBUG` by default for this binary.

**Commit:** `feat(capture): add the prolink-capture hardware session tool`

---

## Task 3 — Replay harness

**File:** `crates/prolink/tests/hardware_capture_replay.rs` (new).

Reads capture sessions from `fixtures/hardware-capture/` (gitignored — real
captures may hold a DJ's library metadata and should not land in the repo).

- If that directory is absent, every test **skips cleanly** rather than
  failing, so CI stays green before any capture exists.
- Replay `udp.jsonl` through `parse_status` and the beat parsing, asserting no
  panics and reporting decode rates per port.
- Run `pdb::parse` over the real `export.pdb` and assert structural
  invariants: tables resolve, track rows carry non-empty titles, artist/album/
  genre/key/label cross-references resolve to real rows rather than dangling
  ids.
- Run the `.EXT` analysis parsing through `rekordcrate`'s `ANLZ` and assert a
  beat grid and waveform come out.

This harness is the whole point of the exercise: after Tuesday, every
remaining question about the PDB parser is answered by `cargo test` with no
hardware in the room.

**Commit:** `test(capture): replay recorded hardware sessions through the parsers`

---

## Task 4 — Validate the tooling before the session

The capture tool itself must not be first-run on-site. Everything except real
player behaviour can be exercised now:

- **Phases A and B** against `cdj-sim`
  (`crates/prolink/src/bin/cdj-sim.rs`), which broadcasts real wire-format
  keep-alives, status and beat packets. Run `cdj-sim --count 4 --mixer` and
  confirm `udp.jsonl` fills, `devices.jsonl` lists four players and a mixer,
  and the decode rate is 100%.
- **Phase C** against `crates/prolink/tests/nfs_fake_server.rs`.
- **Phase D** against `crates/prolink/tests/dbserver_fake_server.rs`.

Add an end-to-end test that runs the capture binary against the fake servers
and asserts the resulting session directory has the expected shape and
non-empty files.

Then deliberately break things and confirm the tool survives: point it at an
IP with nothing on it, feed `pdb::parse` a truncated file, and kill the
process mid-capture. In all three cases the session directory must still hold
every byte captured up to that moment.

**Commit:** `test(capture): verify the capture tool against the simulators`

---

## Task 5 — On-site runbook

**File:** `RUNBOOK-HARDWARE-SESSION.md` (new, repo root, English).

A checklist to follow off a phone with no tooling and no assistant. It must
state, in order and unambiguously:

**Before leaving:** release binaries built (`cargo build --release`, no
compilation on-site), Wireshark installed, a second storage device for
copying the capture off, the machine's Ethernet interface IP noted.

**On arrival, in this exact order:**

1. Close Rekordbox. It competes for the same virtual-player slot and will
   distort or block the capture.
2. Connect Ethernet to the booth switch.
3. Start Wireshark on that interface and leave it running for the whole
   session. It is the backstop: if `prolink-capture` has a bug, the pcap still
   contains every byte.
4. Start `prolink-capture` **before powering on any player.** Powering a
   player on while recording is the only way to capture the channel-claim
   handshake, and it cannot be recaptured afterwards.
5. Power on the players one at a time, pausing between each.
6. Only now pull metadata: run phase C against every media slot present. USB
   and SD are separate exports and both must be captured.
7. Only now start the app itself and observe.
8. Before disconnecting anything: copy the entire capture directory to the
   second storage device, and verify file sizes are non-zero while the kit is
   still on hand.

**Scenarios to play while recording** — an untraversed state is an unvalidated
state. Load a track on each deck; play; cue; pause; change track; move the
pitch fader across its range; toggle on-air, master and sync from the DJM;
move the channel faders and crossfader. Write the wall-clock time and the
action in `notes.md` by hand as you go, so the capture timestamps can be
correlated afterwards.

Steps 1 through 6 do not require the application to work at all. If the app is
broken on the day, the session is still a complete success.

**Commit:** `docs: add the hardware session runbook`

---

## Acceptance criteria

- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass.
- `prolink-capture --help` documents every flag.
- Running the tool against `cdj-sim` produces a session directory whose
  `udp.jsonl` decodes at 100% and whose `devices.jsonl` lists every simulated
  device.
- Killing the tool mid-run leaves a readable session directory.
- Pointing the tool at an unreachable IP produces a `parse-report.json`
  explaining the failure and a zero exit status — a dead player is an expected
  condition on a live network, not a tool error.
- The replay harness skips cleanly with no fixtures present and passes against
  a capture produced from the fake servers.

## Priority if time runs short

Task 1 and Phase A of Task 2 are the irreducible core: raw packet recording,
flushed to disk, with no parsing in the path. Everything else is recoverable
later; the handshake bytes and the real `export.pdb` are not.
