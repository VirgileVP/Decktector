#!/usr/bin/env python3
"""Converts a Wireshark pcapng of a Pro DJ Link booth network into a
`capture-YYYYMMDD-HHMMSS/` session directory shaped exactly like the ones
`prolink-capture` writes, so `crates/prolink/tests/hardware_capture_replay.rs`
can replay a Wireshark recording through this crate's parsers offline.

Why this exists: the runbook asks for a Wireshark capture alongside every
hardware session (see `.gitignore`'s `*.pcapng` note), but a gig recorded
with Wireshark alone — no `prolink-capture` running — had no way into the
replay harness. This closes that gap. Only the UDP side is recoverable from
a pcapng: `udp.jsonl` is written, the `nfs/` and `dbserver/` trees are not,
so the `export.pdb` and ANLZ tests keep skipping on a session from here.

`udp.payload` is read rather than `data.data` because Wireshark's heuristic
dissectors claim a good fraction of Pro DJ Link datagrams (adwin_config,
kip, hcrt and a dozen others all match on these ports) and `data.data` is
empty for every one of them — a silent 3% hole in the converted session.

Usage:
  scripts/pcap-to-capture.py CAPTURE.pcapng [--out DIR] [--name NAME]
                             [--from T] [--to T] [--exclude-src IP]

  T is either seconds from the first packet (`1145`) or a wall clock
  (`00:58:30`), matching the capture's own local timezone.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone

ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT = 50000, 50001, 50002
PORTS = (ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT)

# Wireshark ships tshark inside the app bundle on macOS and does not put it
# on PATH; a Homebrew or Linux install does.
TSHARK_CANDIDATES = ["tshark", "/Applications/Wireshark.app/Contents/MacOS/tshark"]


def find_tshark():
    for candidate in TSHARK_CANDIDATES:
        found = shutil.which(candidate) or (candidate if os.path.exists(candidate) else None)
        if found:
            return found
    sys.exit("tshark not found — install Wireshark, or add tshark to PATH")


def first_packet_epoch(tshark, pcap):
    out = subprocess.run(
        [tshark, "-r", pcap, "-c", "1", "-T", "fields", "-e", "frame.time_epoch"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    if not out:
        sys.exit(f"{pcap} has no packets")
    return float(out)


def parse_time(value, base_epoch):
    """`--from`/`--to` as either seconds-from-start or a local wall clock."""
    if value is None:
        return None
    if ":" in value:
        base = datetime.fromtimestamp(base_epoch)
        parts = [int(p) for p in value.split(":")]
        while len(parts) < 3:
            parts.append(0)
        wall = base.replace(hour=parts[0], minute=parts[1], second=parts[2], microsecond=0)
        return wall.timestamp()
    return base_epoch + float(value)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("pcap")
    ap.add_argument("--out", default="crates/prolink/fixtures/hardware-capture",
                    help="parent directory for the session dir (default: the replay harness's fixtures dir)")
    ap.add_argument("--name", help="session directory name (default: capture-YYYYMMDD-HHMMSS from the first packet)")
    ap.add_argument("--from", dest="start", help="skip packets before this point")
    ap.add_argument("--to", dest="end", help="skip packets after this point")
    ap.add_argument("--exclude-src", action="append", default=[],
                    help="drop packets from this source IP; repeatable. Use it to strip the capturing "
                         "host's own virtual-CDJ keep-alives, which a replay would otherwise see as a "
                         "second decktector on the network.")
    args = ap.parse_args()

    tshark = find_tshark()
    base_epoch = first_packet_epoch(tshark, args.pcap)
    start = parse_time(args.start, base_epoch)
    end = parse_time(args.end, base_epoch)

    name = args.name or datetime.fromtimestamp(base_epoch).strftime("capture-%Y%m%d-%H%M%S")
    session_dir = os.path.join(args.out, name)
    os.makedirs(session_dir, exist_ok=True)

    # `!icmp` because a port-unreachable quotes the offending UDP header,
    # so `udp.dstport` matches inside it — and those carry no payload to
    # convert. The gig this script was written for held 1428 of them.
    display_filter = "!icmp && udp.dstport in {%s}" % " ".join(str(p) for p in PORTS)
    command = [tshark, "-r", args.pcap, "-Y", display_filter, "-T", "fields",
               "-e", "frame.time_epoch", "-e", "ip.src", "-e", "udp.dstport", "-e", "udp.payload"]

    excluded = set(args.exclude_src)
    kept = {port: 0 for port in PORTS}
    skipped_empty = skipped_window = skipped_src = 0

    udp_path = os.path.join(session_dir, "udp.jsonl")
    with subprocess.Popen(command, stdout=subprocess.PIPE, text=True, bufsize=1 << 20) as proc, \
            open(udp_path, "w") as udp_log:
        for line in proc.stdout:
            fields = line.rstrip("\n").split("\t")
            if len(fields) < 4:
                continue
            epoch_text, src, port_text, payload = fields[0], fields[1], fields[2], fields[3]
            if not payload:
                skipped_empty += 1
                continue
            epoch = float(epoch_text)
            if (start is not None and epoch < start) or (end is not None and epoch > end):
                skipped_window += 1
                continue
            if src in excluded:
                skipped_src += 1
                continue
            # A packet matching two ports (never on this filter, but tshark
            # emits comma-joined values for repeated fields in general)
            # would break the int(); take the first occurrence.
            port = int(port_text.split(",")[0])
            hexed = payload.replace(":", "")
            udp_log.write(json.dumps({
                "t": int(round(epoch * 1_000_000)),
                "port": port,
                "src": src.split(",")[0],
                "len": len(hexed) // 2,
                "hex": hexed,
            }, separators=(",", ":")) + "\n")
            kept[port] = kept.get(port, 0) + 1
        if proc.wait() != 0:
            sys.exit(f"tshark exited {proc.returncode}")

    total = sum(kept.values())
    with open(os.path.join(session_dir, "session.json"), "w") as session_log:
        json.dump({
            "tool": "pcap-to-capture.py",
            "source_pcap": os.path.abspath(args.pcap),
            "converted_at": datetime.now(timezone.utc).isoformat(),
            "window": {"from": args.start, "to": args.end},
            "excluded_sources": sorted(excluded),
            "packets": {str(port): kept.get(port, 0) for port in PORTS},
        }, session_log, indent=2)

    print(f"{session_dir}: {total} packets "
          f"(announce {kept[ANNOUNCE_PORT]}, beat {kept[BEAT_PORT]}, status {kept[STATUS_PORT]})")
    if skipped_empty:
        print(f"  {skipped_empty} packets had no UDP payload and were dropped")
    if skipped_window:
        print(f"  {skipped_window} packets fell outside --from/--to")
    if skipped_src:
        print(f"  {skipped_src} packets came from an --exclude-src address")


if __name__ == "__main__":
    main()
