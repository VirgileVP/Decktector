# Hardware session runbook

A checklist for the first real Pro DJ Link hardware session — a CDJ + DJM
kit, one short window, no do-overs. Follow it off a phone with no laptop
tooling open and no assistant available. See `HARDWARE-CAPTURE-PLAN.md` for
why this tool exists and exactly what it captures.

**The point of this session is to capture bytes, not to debug the app.**
Steps 1–6 below don't need the Decktective app to work at all. If the app is
broken on the day, the session is still a complete success — every parser
question gets answered afterwards, offline, against what got captured.

## Before leaving

- [ ] The repo checked out on the laptop coming to the venue, on the branch
      with this tool (`git fetch && git checkout feat/rust-tauri-rewrite &&
      git pull`), confirmed up to date — this file and `prolink-capture`
      only exist on that branch.
- [ ] Release binaries built: `cargo build --release`. No compilation
      on-site. This produces `target/release/prolink-capture` (add `.exe` on
      Windows) and `target/release/cdj-sim`, from the repo root.
- [ ] Wireshark installed on that laptop.
- [ ] A second storage device (USB stick, external drive) to copy the
      capture off before leaving the venue.
- [ ] The laptop's Ethernet adapter identified ahead of time (name and, if
      possible, its usual IP) — one less thing to figure out under time
      pressure at the booth.

All commands below assume the current directory is the repo root, and that
`target/release/` is on hand — either run them as
`./target/release/prolink-capture ...` (Linux/macOS) or
`.\target\release\prolink-capture.exe ...` (Windows), or `cd` into
`target/release/` first and drop that prefix.

## On arrival, in this exact order

1. **Close Rekordbox** on the laptop, if it's running. It competes for the
   same virtual-player slot on the Pro DJ Link network and will distort or
   block the capture.

2. **Connect Ethernet** from the laptop to the booth switch (or directly
   into a CDJ's LINK port if there's no switch).

3. **Start Wireshark** on that Ethernet interface and leave it running for
   the *entire* session. This is the backstop: if `prolink-capture` has a
   bug on the day, the `.pcapng` still has every byte. Save it somewhere
   before leaving (step 8 covers the capture tool's own output; do the same
   for the Wireshark file).

4. **Start `prolink-capture` before powering on any player, in its own
   terminal window that stays open for the rest of the session.**

   ```
   prolink-capture --out capture
   ```

   This is the one step that cannot be redone later: powering a player on
   while recording is the only way to capture the channel-claim handshake,
   and it can't be recaptured after the fact. Confirm the tool printed
   which network interface it bound to, and that it looks like the booth
   adapter, not something else on a multi-homed laptop. Leave this running
   — with no `--duration-sec`, it keeps recording raw traffic until Ctrl+C,
   which happens at step 8, not before.

   Note the exact timestamped folder name it prints (`capture/capture-...`)
   — every other command below writes into the *same* `--out capture`, but
   each one still creates its own new `capture-YYYYMMDD-HHMMSS/` session
   inside it, so nothing here overwrites step 4's raw recording.

5. **Power on the players, one at a time, pausing a few seconds between
   each.** Watch step 4's live device table (printed to its terminal) —
   each player should appear as it comes up.

6. **Now pull metadata, in a second terminal window:** run a *separate*
   `prolink-capture` invocation for phase C — the first one (step 4) is
   still busy doing the raw recording no other step may interrupt, so this
   is a second process, not a second action inside the first. It's fine for
   both to run at once; they don't conflict.

   ```
   prolink-capture --out capture --duration-sec 5
   ```

   USB and SD are separate exports on the same player — if a player has
   both loaded, this one pass captures both automatically. If a player
   isn't showing up in step 4's live table, or a slot isn't being picked up,
   read the IP straight off that player's own network settings screen and
   pass it explicitly instead:

   ```
   prolink-capture --out capture --duration-sec 5 --player 192.168.1.31 --slot usb --player 192.168.1.31 --slot sd
   ```

   This override exists specifically for this moment — it works even if
   discovery or status decoding turns out to be broken on real hardware,
   because it doesn't depend on either. This second process exits on its
   own after `--duration-sec`; step 4's process is untouched and keeps
   recording.

7. **Only now start the Decktective app itself** and observe it against the
   live network. Step 4's `prolink-capture` keeps running in the background
   the whole time — it doesn't need to be stopped for the app to work.

8. **Before disconnecting anything:** stop step 4's `prolink-capture`
   (Ctrl+C in its terminal — it finishes writing `session.json` and exits
   cleanly), then copy the whole `capture/` directory *and* the Wireshark
   `.pcapng` to the second storage device. Verify the file sizes are
   non-zero — `export.pdb` files especially — while the kit is still on
   hand to recapture from if something looks wrong.

## Scenarios to play while recording

An untraversed state is an unvalidated state. With the app running (or even
without it — the capture doesn't care), work through:

- Load a track on each deck.
- Play. Cue. Pause. Change to a different track.
- Move the pitch fader across its range.
- From the DJM: toggle on-air, toggle master, toggle sync.
- Move the channel faders and the crossfader.

Write the wall-clock time and the action into `notes.md`, inside **step 4's**
session folder specifically (the long-running raw-capture one, not one of
step 6's short metadata-pull ones) — it's created empty, ready for this.
That's what lets the capture timestamps be correlated against what actually
happened, afterwards, with no memory of the exact sequence to rely on.

## If something looks wrong on-site

Don't stop to debug it. `prolink-capture` is built to never let a parse or
connection failure abort the run (see `HARDWARE-CAPTURE-PLAN.md`'s governing
rule) — a bad reading from one player, one failed metadata pull, a decode
error, none of it should crash the tool or stop the raw recording. If the
tool *does* crash, restart it; whatever was captured before the crash is
still safely on disk (every record is flushed as it's written). Keep going
through the scenario list. Every remaining question gets sorted out later,
offline, against the bytes this session collected.
