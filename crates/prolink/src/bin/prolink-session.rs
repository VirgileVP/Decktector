//! Guided hardware-session driver: runs a whole Pro DJ Link capture session
//! end to end from one command, so the booth operator types nothing but
//! ENTER.
//!
//! `RUNBOOK-HARDWARE-SESSION.md` describes the same session as eight manual
//! steps across two terminals, with `notes.md` filled in by hand. Every one
//! of those is a chance to mis-order something under time pressure, on a
//! kit that's only available once — and the ordering genuinely matters
//! (nothing captures the channel-claim handshake except recording *before*
//! the players power on). This tool keeps the runbook's order as code:
//!
//!   1. preflight — bind interface, rekordbox, sibling binary, output dir
//!   2. start `prolink-capture` (the long raw recording), before anything else
//!   3. walk the scenario checklist one prompt at a time, stamping `notes.md`
//!      with the wall clock as each step is confirmed
//!   4. run the metadata pull at the right moment, as its own short capture
//!   5. stop the recording cleanly and verify what landed on disk, while the
//!      kit is still in the room to recapture from
//!
//! It drives `prolink-capture` as a child process rather than absorbing its
//! phases: that tool is the thing the session depends on, it is already
//! covered by `tests/capture_tool_e2e.rs`, and a guided wrapper is not a
//! reason to restructure it days before a session. Nothing here can fail in
//! a way that stops the recording — every check below is advisory, and the
//! child keeps recording regardless of what this process concludes.
//!
//! Usage:
//!   prolink-session [--out DIR] [--bind IP] [--player IP] [--slot usb|sd|cd]

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use prolink::capture::{append_note, read_jsonl};
use prolink::{ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Mirrors `prolink-capture`'s own default, so both tools write into the
/// same place when neither is given `--out` — duplicated rather than shared
/// because each binary is its own crate target and can only see the
/// library's public API (the same reason that tool re-implements its socket
/// helper locally).
const DEFAULT_OUT_DIR: &str = "./capture";
/// How long the metadata pull is given. Matches the runbook's
/// `--duration-sec 5`: long enough for a settle plus the NFS and dbserver
/// phases, short enough that nobody is left waiting at the booth.
const METADATA_PULL_SECS: u64 = 5;
/// How long to wait for the child to announce its session directory before
/// carrying on without one. Generous: a slow disk on an unfamiliar laptop
/// is not a reason to abandon a session.
const SESSION_DIR_TIMEOUT: Duration = Duration::from_secs(20);

/// What the operator does on the gear, and what gets written down when they
/// confirm it. Kept as data so the checklist reads as a checklist — the
/// runbook's "scenarios to play" section, in the order it should happen.
struct Step {
    /// Shown at the booth, one at a time.
    instruction: &'static str,
    /// Stamped into `notes.md` with the wall clock once confirmed.
    note: &'static str,
    action: Action,
}

enum Action {
    /// The operator acted on the hardware; this tool only records when.
    NoteOnly,
    /// Additionally run the metadata pull as its own short capture.
    MetadataPull,
}

/// The session, in the order the runbook requires. Powering the players on
/// comes first and cannot be moved: the channel-claim handshake only exists
/// at boot and is the one thing that can never be recaptured later.
///
/// "An untraversed state is an unvalidated state" — every entry here exists
/// because some parser branch stays guesswork without it.
const CHECKLIST: &[Step] = &[
    Step {
        instruction: "Power on the players ONE AT A TIME, a few seconds apart.\n  Watch the player count below grow as each one comes up.",
        note: "players powered on",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "Load a track on EACH deck.",
        note: "track loaded on every deck",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "Pull metadata now — leave the decks loaded and don't touch them.",
        note: "metadata pull (NFS + dbserver)",
        action: Action::MetadataPull,
    },
    Step {
        instruction: "Press PLAY on a deck.",
        note: "play",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "Press CUE on that deck.",
        note: "cue",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "PAUSE that deck.",
        note: "pause",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "Load a DIFFERENT track on that deck.",
        note: "track changed",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "Move the PITCH fader slowly across its whole range, end to end.",
        note: "pitch fader swept full range",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "On the DJM: toggle ON-AIR (move the channel fader up and down).",
        note: "on-air toggled",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "On the DJM or the deck: toggle MASTER onto another deck.",
        note: "master toggled",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "Toggle SYNC on and off.",
        note: "sync toggled",
        action: Action::NoteOnly,
    },
    Step {
        instruction: "Move the CHANNEL faders and the CROSSFADER across their range.",
        note: "channel faders and crossfader swept",
        action: Action::NoteOnly,
    },
];

struct Config {
    out_dir: PathBuf,
    bind_ip: Option<Ipv4Addr>,
    player_overrides: Vec<String>,
    slot_overrides: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::from(DEFAULT_OUT_DIR),
            bind_ip: None,
            player_overrides: Vec::new(),
            slot_overrides: Vec::new(),
        }
    }
}

fn parse_args() -> Config {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => {
                config.out_dir = PathBuf::from(args.next().expect("--out needs a directory"))
            }
            "--bind" => {
                config.bind_ip = Some(
                    args.next()
                        .and_then(|value| value.parse().ok())
                        .expect("--bind needs an IPv4 address"),
                )
            }
            "--player" => config
                .player_overrides
                .push(args.next().expect("--player needs an IPv4 address")),
            "--slot" => config
                .slot_overrides
                .push(args.next().expect("--slot needs usb, sd or cd")),
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}\n");
                print_usage();
                std::process::exit(1);
            }
        }
    }
    config
}

fn print_usage() {
    eprintln!("prolink-session — runs a guided Pro DJ Link hardware capture session end to end");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --out DIR     session output directory (default {DEFAULT_OUT_DIR})");
    eprintln!("  --bind IP     local adapter IP (default: auto-detected, shown in preflight)");
    eprintln!(
        "  --player IP   explicit player for the metadata pull, pairs with --slot by position"
    );
    eprintln!("  --slot SLOT   usb, sd or cd — use when a player isn't being discovered");
    eprintln!();
    eprintln!("Everything else is prompted for. See RUNBOOK-HARDWARE-SESSION.md.");
}

#[tokio::main]
async fn main() {
    let config = parse_args();

    banner("PREFLIGHT");
    let bind_ip = config.bind_ip.unwrap_or_else(resolve_bind_ip);
    run_preflight(&config, bind_ip);

    let mut prompts = BufReader::new(tokio::io::stdin()).lines();

    println!();
    prompt(
        &mut prompts,
        "Wireshark running on that interface as a backstop? ENTER when it is (or to skip)",
    )
    .await;

    banner("RECORDING");
    let mut capture = match start_capture(&config, bind_ip) {
        Ok(child) => child,
        Err(error) => {
            eprintln!("Could not start prolink-capture: {error}");
            eprintln!("Is it next to this binary? Both come from `cargo build --release`.");
            std::process::exit(1);
        }
    };

    let session_dir = match take_session_dir(&mut capture).await {
        Some(dir) => {
            println!("Recording to {}", dir.display());
            dir
        }
        None => {
            eprintln!("prolink-capture never printed a session directory.");
            eprintln!(
                "It may still be recording — leave it be and check {} by hand.",
                config.out_dir.display()
            );
            std::process::exit(1);
        }
    };
    note(&session_dir, "session started (raw capture running)");

    println!();
    println!("Recording is live. Everything from here is one prompt at a time —");
    println!("do the action on the gear, then press ENTER. Times are written down");
    println!("for you. Nothing below can stop the recording.");

    banner("CHECKLIST");
    for (index, step) in CHECKLIST.iter().enumerate() {
        println!();
        println!("[{}/{}] {}", index + 1, CHECKLIST.len(), step.instruction);
        println!("      {}", live_summary(&session_dir));
        prompt(&mut prompts, "      ENTER when done").await;
        note(&session_dir, step.note);

        if matches!(step.action, Action::MetadataPull) {
            run_metadata_pull(&config, bind_ip).await;
            note(&session_dir, "metadata pull finished");
        }
    }

    banner("STOPPING");
    note(&session_dir, "session ended");
    stop_capture(capture).await;

    banner("VERDICT");
    report(&session_dir);

    println!();
    println!("Copy this whole directory AND the Wireshark .pcapng to a second");
    println!("drive before disconnecting anything:");
    println!("  {}", session_dir.display());
}

fn banner(title: &str) {
    println!();
    println!("== {title} ==");
}

/// Advisory checks only — each prints a line and never blocks the session.
/// A booth is the wrong place to be argued with by a tool.
fn run_preflight(config: &Config, bind_ip: Ipv4Addr) {
    check(
        "bind interface",
        true,
        &format!("{bind_ip} — must be the booth adapter, not Wi-Fi"),
    );

    let capture_binary = capture_binary_path();
    check(
        "prolink-capture",
        capture_binary.exists(),
        &capture_binary.display().to_string(),
    );

    match std::fs::create_dir_all(&config.out_dir) {
        Ok(()) => check(
            "output directory",
            true,
            &config.out_dir.display().to_string(),
        ),
        Err(error) => check(
            "output directory",
            false,
            &format!("{}: {error}", config.out_dir.display()),
        ),
    }

    match competing_apps_running() {
        Some(apps) if apps.is_empty() => check(
            "competing apps",
            true,
            "no rekordbox or Decktective app running",
        ),
        Some(apps) => check(
            "competing apps",
            false,
            &format!("{} RUNNING — quit it before recording", apps.join(", ")),
        ),
        None => check(
            "competing apps",
            true,
            "could not check — confirm by hand that rekordbox and the Decktective app are closed",
        ),
    }
}

fn check(label: &str, ok: bool, detail: &str) {
    let mark = if ok { "OK  " } else { "WARN" };
    println!("[{mark}] {label:<20} {detail}");
}

/// Other Pro DJ Link consumers running on this machine, which have to be
/// closed before recording.
///
/// rekordbox competes for the virtual-player slot. The Decktective app
/// itself is just as damaging and far easier to leave running by accident
/// (a `tauri dev` window behind the terminal): it binds the same three
/// ports with `SO_REUSEPORT`, and while broadcasts reach every listener,
/// **status packets are unicast** — the kernel hands each player's stream
/// to exactly one of the two processes, so the capture silently ends up
/// with some players' status and not others'.
///
/// `None` when the platform's process lister couldn't be consulted at all —
/// reported as an unknown rather than quietly passing as "nothing running".
///
/// Matching is on the executable's file name, never the full command line:
/// this repository's own directory is called `decktective`, so every path
/// in `ps` output — including this tool's own — contains that word.
fn competing_apps_running() -> Option<Vec<String>> {
    let output = if cfg!(windows) {
        std::process::Command::new("tasklist").output().ok()?
    } else {
        std::process::Command::new("ps")
            .args(["-A", "-o", "comm="])
            .output()
            .ok()?
    };
    if !output.status.success() {
        return None;
    }

    let listing = String::from_utf8_lossy(&output.stdout).to_lowercase();
    let mut found: Vec<String> = Vec::new();
    for line in listing.lines() {
        // `tasklist` prints an image name followed by other columns; `ps
        // -o comm=` prints one path, which on macOS can itself contain
        // spaces ("…/MacOS/Google Chrome").
        let command = if cfg!(windows) {
            line.split_whitespace().next().unwrap_or("")
        } else {
            line.trim()
        };
        let executable = command
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .trim_end_matches(".exe");
        let app = match executable {
            name if name.contains("rekordbox") => "rekordbox",
            "decktective" => "the Decktective app",
            _ => continue,
        };
        if !found.iter().any(|seen| seen == app) {
            found.push(app.to_string());
        }
    }
    Some(found)
}

/// Mirrors `prolink-capture`'s `resolve_bind_ip` — duplicated for the same
/// reason its socket helper is: separate crate targets can only share
/// through the library's public API, and this is four lines.
fn resolve_bind_ip() -> Ipv4Addr {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .find_map(|interface| match interface.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
            _ => None,
        })
        .unwrap_or(Ipv4Addr::LOCALHOST)
}

/// `prolink-capture` as built alongside this binary — both land in the same
/// `target/<profile>/` directory, so resolve it relative to this executable
/// rather than trusting `PATH`, which at a booth is whatever the shell
/// happens to have.
fn capture_binary_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap_or_default();
    path.pop();
    path.join(format!("prolink-capture{}", std::env::consts::EXE_SUFFIX))
}

/// Starts the long raw recording: no `--duration-sec`, so it runs until
/// [`stop_capture`] closes its stdin. Both pipes are captured so a chatty
/// child can never fill a pipe buffer and stall its own recording — they
/// are drained by [`take_session_dir`] and discarded, since this tool
/// prints the operator's view itself.
fn start_capture(config: &Config, bind_ip: Ipv4Addr) -> std::io::Result<Child> {
    Command::new(capture_binary_path())
        .arg("--out")
        .arg(&config.out_dir)
        .arg("--bind")
        .arg(bind_ip.to_string())
        // Phases C/D belong to the metadata-pull step, which runs as its own
        // short capture at the point in the checklist where the decks are
        // loaded — not at startup, when nothing is loaded yet.
        .arg("--no-nfs")
        .arg("--no-dbserver")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}

/// Reads the child's stdout until it announces its session directory, then
/// leaves both pipes being drained in the background for the rest of the
/// run.
async fn take_session_dir(capture: &mut Child) -> Option<PathBuf> {
    const MARKER: &str = "Session directory: ";

    if let Some(stderr) = capture.stderr.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(_)) = lines.next_line().await {}
        });
    }

    let stdout = capture.stdout.take()?;
    let mut lines = BufReader::new(stdout).lines();
    let found = tokio::time::timeout(SESSION_DIR_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(dir) = line.strip_prefix(MARKER) {
                return Some(PathBuf::from(dir.trim()));
            }
        }
        None
    })
    .await
    .ok()
    .flatten();

    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    found
}

/// The metadata pull, as its own short `prolink-capture` run. Runs
/// concurrently with the long recording exactly as the runbook's second
/// terminal does — they don't conflict, and this one exits on its own.
async fn run_metadata_pull(config: &Config, bind_ip: Ipv4Addr) {
    println!("      pulling metadata ({METADATA_PULL_SECS}s) — leave the decks alone...");

    let mut command = Command::new(capture_binary_path());
    command
        .arg("--out")
        .arg(&config.out_dir)
        .arg("--bind")
        .arg(bind_ip.to_string())
        .arg("--duration-sec")
        .arg(METADATA_PULL_SECS.to_string());
    for player in &config.player_overrides {
        command.arg("--player").arg(player);
    }
    for slot in &config.slot_overrides {
        command.arg("--slot").arg(slot);
    }

    match command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
    {
        Ok(status) if status.success() => println!("      metadata pull done"),
        Ok(status) => {
            println!("      metadata pull exited with {status} — the raw recording is unaffected")
        }
        Err(error) => {
            println!("      metadata pull could not run: {error} — the raw recording is unaffected")
        }
    }
}

/// Closes the child's stdin, which [`prolink-capture`'s `wait_for_stop`]
/// treats as "the parent is done" — it then writes `session.json`'s end
/// timestamp and exits on its own. Killing it instead would skip that.
async fn stop_capture(mut capture: Child) {
    drop(capture.stdin.take());
    match tokio::time::timeout(Duration::from_secs(15), capture.wait()).await {
        Ok(Ok(_)) => println!("Recording stopped cleanly."),
        Ok(Err(error)) => println!("Recording stopped, but waiting on it failed: {error}"),
        Err(_) => {
            println!(
                "Recording did not stop in time — killing it. Bytes already written are safe."
            );
            let _ = capture.kill().await;
        }
    }
}

fn note(session_dir: &Path, text: &str) {
    if let Err(error) = append_note(session_dir, text) {
        eprintln!("(could not write to notes.md: {error})");
    }
}

/// A one-line "is this working?" for the prompt display, derived from
/// `status-decoded.jsonl` — i.e. from phase A, which receives on its own
/// socket and so reports correctly on every platform, unlike phase B's
/// `devices.jsonl`, whose second socket on the same port is not delivered
/// to on Windows.
fn live_summary(session_dir: &Path) -> String {
    let mut players: Vec<u64> = read_json_lines(&session_dir.join("status-decoded.jsonl"))
        .iter()
        .filter_map(|record| {
            record
                .get("device_number")
                .and_then(serde_json::Value::as_u64)
        })
        .collect();
    players.sort_unstable();
    players.dedup();

    if players.is_empty() {
        "(no players seen yet)".to_string()
    } else {
        format!(
            "players seen: {}",
            players
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Reads a JSONL file into loosely-typed records, tolerating a missing or
/// unreadable file by reporting nothing. Every caller here is producing an
/// on-site summary — and reads these files *while* the capture process is
/// still appending to them, so the trailing partial line `read_jsonl`
/// forgives is the normal case here, not an anomaly.
fn read_json_lines(path: &Path) -> Vec<serde_json::Value> {
    read_jsonl::<serde_json::Value>(path)
        .map(|read| read.records)
        .unwrap_or_default()
}

/// The point of the whole exercise: say what landed on disk, in terms of
/// what it does and doesn't let anyone validate afterwards, while the kit
/// is still in the room.
fn report(session_dir: &Path) {
    let udp = read_json_lines(&session_dir.join("udp.jsonl"));
    let mut announce = 0usize;
    let mut beat = 0usize;
    let mut status = 0usize;
    for record in &udp {
        match record.get("port").and_then(serde_json::Value::as_u64) {
            Some(port) if port == u64::from(ANNOUNCE_PORT) => announce += 1,
            Some(port) if port == u64::from(BEAT_PORT) => beat += 1,
            Some(port) if port == u64::from(STATUS_PORT) => status += 1,
            _ => {}
        }
    }

    let decoded = read_json_lines(&session_dir.join("status-decoded.jsonl"));
    let failed = decoded
        .iter()
        .filter(|record| {
            record.get("outcome").and_then(serde_json::Value::as_str) != Some("decoded")
        })
        .count();

    check(
        "keep-alives (50000)",
        announce > 0,
        &format!("{announce} packets"),
    );
    check("beats (50001)", beat > 0, &format!("{beat} packets"));
    check("status (50002)", status > 0, &format!("{status} packets"));
    check(
        "status decoding",
        !decoded.is_empty() && failed == 0,
        &format!("{} decoded, {failed} failed", decoded.len() - failed),
    );
    check("players seen", true, &live_summary(session_dir));

    let exports = find_exports(session_dir);
    let empty = exports.iter().filter(|(_, size)| *size == 0).count();
    if exports.is_empty() {
        check(
            "export.pdb",
            false,
            "NONE — pdb.rs stays unvalidated; recapture while the kit is here",
        );
    } else {
        check(
            "export.pdb",
            empty == 0,
            &format!("{} file(s), {empty} empty", exports.len()),
        );
        for (path, size) in &exports {
            println!("       {} ({size} bytes)", path.display());
        }
    }
}

/// Every `export.pdb` the metadata pull landed, with its size — the one
/// artifact whose absence silently costs the session its main prize, since
/// `nfs/pdb.rs` is the least validated parser in the crate.
fn find_exports(session_dir: &Path) -> Vec<(PathBuf, u64)> {
    let mut found = Vec::new();
    let mut pending = vec![session_dir.join("nfs")];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("export.pdb"))
            {
                let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                found.push((path, size));
            }
        }
    }
    found
}

/// Reads the operator's ENTERs. Built once and threaded through every
/// prompt: a fresh `BufReader` per prompt would discard whatever it had
/// already buffered past the line it returned, which is invisible when
/// someone types one line at a time but silently eats input the moment it
/// arrives in a block.
type Prompts = tokio::io::Lines<BufReader<tokio::io::Stdin>>;

/// Waits for ENTER. End-of-input means stdin is gone (closed pipe, no
/// terminal), which would otherwise spin the checklist forever — treated as
/// "carry on" so an unattended run still finishes and stops the recording
/// rather than hanging with it open.
async fn prompt(prompts: &mut Prompts, message: &str) {
    use std::io::Write;

    print!("{message}: ");
    let _ = std::io::stdout().flush();

    if matches!(prompts.next_line().await, Ok(None) | Err(_)) {
        println!("(stdin closed — continuing)");
    }
}
