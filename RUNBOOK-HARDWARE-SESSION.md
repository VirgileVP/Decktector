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

- [ ] Release binaries built on the laptop that's coming to the venue:
      `cargo build --release`. No compilation on-site.
- [ ] Wireshark installed on that laptop.
- [ ] A second storage device (USB stick, external drive) to copy the
      capture off before leaving the venue.
- [ ] The laptop's Ethernet adapter identified ahead of time (name and, if
      possible, its usual IP) — one less thing to figure out under time
      pressure at the booth.

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

4. **Start `prolink-capture` before powering on any player.**

   ```
   ./prolink-capture --out ./capture
   ```

   This is the one step that cannot be redone later: powering a player on
   while recording is the only way to capture the channel-claim handshake,
   and it can't be recaptured after the fact. Confirm the tool printed
   which network interface it bound to, and that it looks like the booth
   adapter, not something else on a multi-homed laptop.

5. **Power on the players, one at a time, pausing a few seconds between
   each.** Watch the tool's live device table (printed to the terminal) —
   each player should appear as it comes up.

6. **Now pull metadata: run phase C against every media slot present.**
   USB and SD are separate exports on the same player — capture both if
   both are loaded. `prolink-capture` does this automatically ~3 seconds
   after starting (or after `--duration-sec` if that flag was given), using
   whatever it's discovered so far — but if a player isn't showing up in the
   live table, or a slot isn't being picked up automatically, read the IP
   straight off that player's own network settings screen and pass it
   explicitly:

   ```
   ./prolink-capture --out ./capture --player 192.168.1.31 --slot usb --player 192.168.1.31 --slot sd
   ```

   This override exists specifically for this moment — it works even if
   discovery or status decoding turns out to be broken on real hardware,
   because it doesn't depend on either.

7. **Only now start the Decktective app itself** and observe it against the
   live network. `prolink-capture` keeps running in the background the
   whole time — it doesn't need to be stopped for the app to work.

8. **Before disconnecting anything:** stop `prolink-capture` (Ctrl+C — it
   finishes writing `session.json` and exits cleanly), then copy the whole
   `capture/` directory *and* the Wireshark `.pcapng` to the second storage
   device. Verify the file sizes are non-zero — `export.pdb` files
   especially — while the kit is still on hand to recapture from if
   something looks wrong.

## Scenarios to play while recording

An untraversed state is an unvalidated state. With the app running (or even
without it — the capture doesn't care), work through:

- Load a track on each deck.
- Play. Cue. Pause. Change to a different track.
- Move the pitch fader across its range.
- From the DJM: toggle on-air, toggle master, toggle sync.
- Move the channel faders and the crossfader.

Write the wall-clock time and the action into `capture/capture-.../notes.md`
by hand as you go (it's created empty, ready for this) — that's what lets
the capture timestamps be correlated against what actually happened,
afterwards, with no memory of the exact sequence to rely on.

## If something looks wrong on-site

Don't stop to debug it. `prolink-capture` is built to never let a parse or
connection failure abort the run (see `HARDWARE-CAPTURE-PLAN.md`'s governing
rule) — a bad reading from one player, one failed metadata pull, a decode
error, none of it should crash the tool or stop the raw recording. If the
tool *does* crash, restart it; whatever was captured before the crash is
still safely on disk (every record is flushed as it's written). Keep going
through the scenario list. Every remaining question gets sorted out later,
offline, against the bytes this session collected.
