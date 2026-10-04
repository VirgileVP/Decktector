# Hardware session runbook

A checklist for the first real Pro DJ Link hardware session — a CDJ + DJM
kit, one short window, no do-overs. Follow it off a phone with no laptop
tooling open and no assistant available. See `HARDWARE-CAPTURE-PLAN.md` for
why this tooling exists and exactly what it captures.

**The point of this session is to capture bytes, not to debug the app.**
None of this needs the Decktector app to work at all. If the app is broken
on the day, the session is still a complete success — every parser question
gets answered afterwards, offline, against what got captured.

`prolink-session` runs the whole thing as one guided command: it starts the
recording in the right order, prompts for one action at a time, writes the
timestamps down for you, pulls the metadata at the right moment, stops
cleanly and tells you what landed on disk. The manual path it replaces is
still documented at the bottom, for when something needs to be done by hand.

## Before leaving

- [ ] The repo checked out on the laptop coming to the venue, on the branch
      with this tooling (`git fetch && git checkout feat/rust-tauri-rewrite
  && git pull`), confirmed up to date — these tools only exist on that
      branch.
- [ ] Release binaries built: `cargo build --release`. No compilation
      on-site. This produces `prolink-session`, `prolink-capture` and
      `cdj-sim` under `target/release/` (add `.exe` on Windows), from the
      repo root.
- [ ] A dry run done at home, with no hardware: start
      `./target/release/cdj-sim` in one terminal, then run the real command
      below in another. It should walk the whole checklist and finish with a
      verdict. Ten minutes now, versus finding out at the booth.
- [ ] Wireshark installed on that laptop.
- [ ] A second storage device (USB stick, external drive) to copy the
      capture off before leaving the venue.
- [ ] The laptop's Ethernet adapter identified ahead of time (name and, if
      possible, its usual IP) — the preflight prints what it picked, and you
      want to recognize a wrong answer.

## On arrival

1. **Close Rekordbox _and the Decktector app_** on the laptop. Both
   compete for the same virtual-player slot, and the app is the easier one
   to leave running by accident — a `tauri dev` window behind the terminal
   is enough. It binds the same three UDP ports, and because player status
   arrives **unicast**, the operating system hands each player's stream to
   exactly one process: the capture then records beats and discovery
   normally while silently missing status for some or all players. The
   preflight warns about both.

2. **Connect Ethernet** from the laptop to the booth switch (or directly
   into a CDJ's LINK port if there's no switch).

3. **Start Wireshark** on that Ethernet interface and leave it running for
   the _entire_ session. This is the backstop: if the capture tooling has a
   bug on the day, the `.pcapng` still has every byte. Save it alongside the
   capture directory before leaving.

4. **Run the session, before powering on any player:**

   ```
   ./target/release/prolink-session --out capture --bind <adapter IP>
   ```

   (`.\target\release\prolink-session.exe --out capture` on Windows.)

   **Pass `--bind` explicitly.** Auto-detection picks the first non-loopback
   adapter, which on a laptop with a phone plugged in, a VM, or a VPN is not
   the booth's. The keep-alive we broadcast carries that address, and it is
   where players send their status back — the wrong one means discovery and
   beats still arrive and nothing else does. The preflight prints the
   address it will use; if it is not the Ethernet adapter's, stop and pass
   the right one.

   Everything else is prompts. Do what each one says on the gear, press
   ENTER, and it records when it happened. It starts by showing which
   network interface it bound to — check that it's the booth adapter and not
   Wi-Fi before going further.

   Starting it before the players come up is what captures the channel-claim
   handshake, which cannot be recaptured after the fact. This is the one
   ordering that matters, and it's the reason the tool starts recording
   before its first prompt.

   Each prompt shows how many players it can currently see, so powering them
   on one at a time gives you live confirmation that they're being picked
   up.

5. **Before disconnecting anything:** the tool prints the session directory
   at the end. Copy that whole directory _and_ the Wireshark `.pcapng` to
   the second storage device. The final verdict lists what it captured and
   flags what's missing — read it while the kit is still on hand to
   recapture from. In particular, `export.pdb` missing means `nfs/pdb.rs`
   stays unvalidated, which is most of the point of the session.

Once home, drop the `capture-YYYYMMDD-HHMMSS/` directories under
`crates/prolink/fixtures/hardware-capture/` (gitignored — a real capture may
hold a DJ's library metadata and must never land in the repo) and run
`cargo test`. `tests/hardware_capture_replay.rs` stops skipping and starts
answering.

## What the guided checklist covers

An untraversed state is an unvalidated state. The prompts walk through:
powering the players up one at a time, loading a track on each deck, the
metadata pull, play/cue/pause, changing track, the pitch fader across its
full range, on-air, master, sync, and the channel faders plus crossfader.

Every entry exists because some parser branch stays guesswork without it. If
the kit can do something the prompts don't mention, do it anyway and say so
in `notes.md` — the file is plain Markdown and stays editable by hand
throughout.

Two of these need the rig set up a particular way, or they capture nothing
even though the session looks fine:

- **On-air** is reported by the mixer per _channel_, and a player only
  lights up as on-air when its **player number matches the mixer channel
  it is plugged into**. The 2026-08-19 session ran two CDJ-3000s numbered 3
  and 4 through channels 1 and 2: the mixer dutifully announced channels 1
  and 2 as live, no player ever set its on-air flag, and the whole capture
  contains not one on-air status. Renumber the players to match their
  channels before this step.
- **Metadata** is only interesting if it comes off more than one stick. Two
  decks linked to the same USB is the normal booth setup and is worth
  capturing, but it exercises exactly one player's database — put a second
  stick in the other player for at least part of the session.

## If something looks wrong on-site

Don't stop to debug it. The tooling is built to never let a parse or
connection failure abort a run (see `HARDWARE-CAPTURE-PLAN.md`'s governing
rule) — a bad reading from one player, a failed metadata pull, a decode
error, none of it stops the raw recording, and every check the session
driver prints is advisory.

If the live table shows players but the packet counter next to `status=`
stays at zero, the players are not seeing our keep-alive: check `--bind` is
the booth adapter and that nothing else on the laptop is holding the Pro DJ
Link ports (see "On arrival"). Everything downstream — the whole metadata
pull included — depends on status traffic.

If a player never appears in the prompt's player count, read its IP straight
off its own network settings screen and pass it explicitly, which works even
if discovery or status decoding turns out to be broken on real hardware:

```
./target/release/prolink-session --out capture --player 192.168.1.31 --slot usb
```

If `prolink-session` itself crashes, whatever was captured before is still
on disk — every record is flushed as it's written — and the manual path
below still works.

## Manual fallback

`prolink-session` drives `prolink-capture`, which can be run directly. Two
terminals:

```
prolink-capture --out capture
```

Start that first, before powering on any player, and leave it running until
the very end (Ctrl+C stops it and finishes `session.json`). Then, once the
decks are loaded, pull metadata from a second terminal — a separate
invocation, not an action inside the first, and the two don't conflict:

```
prolink-capture --out capture --duration-sec 5
```

Add `--player IP --slot usb` pairs if a player isn't being discovered. Write
the wall-clock time and each action into `notes.md` inside the _first_
session's directory as you go; that correlation is what
`prolink-session` otherwise does for you, and reconstructing it from memory
afterwards does not work.
