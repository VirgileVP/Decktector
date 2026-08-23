//! Imports a Wireshark `.pcapng` recording of a booth network into the same
//! session layout `prolink-capture` writes, so everything downstream of a
//! capture — the replay harness (`tests/hardware_capture_replay.rs`),
//! `cdj-sim --replay`, the simulator's track fixtures — works off a
//! Wireshark file with no separate code path.
//!
//! Why bother, when `prolink-capture` exists: a Wireshark recording sees the
//! whole segment, including the traffic players exchange between themselves.
//! Player status is unicast only to peers whose keep-alive a player has
//! seen, so a listener that fails to announce itself correctly records
//! nothing on [`prolink::STATUS_PORT`] — exactly what happened on
//! 2026-08-19 — while the `.pcapng` taken alongside it still holds every
//! byte. It also holds the NFS metadata transfers players perform against
//! each other on a linked booth, which is where a real track's beat grid,
//! cues and waveform come from.
//!
//! This tool only converts; it does not interpret. Parsing stays where it
//! already lives, run offline against the imported session as many times as
//! needed. Same governing rule as `prolink-capture`: write the bytes out,
//! never let one undecodable packet end the run.
//!
//! Usage:
//!   prolink-pcap --in FILE.pcapng [--out DIR] [--ports 50000,50001,50002]
//!                [--all-ports] [--from-sec N] [--to-sec N] [--max-packets N]
//!                [--no-nfs] [--dry-run]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::net::IpAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use prolink::capture::{write_raw_file, CaptureWriter, UdpRecord};
use prolink::nfs::extract::{ExtractStats, NfsExtractor};
use prolink::pcap::{Datagram, PcapngReader, PcapngStats};
use prolink::{ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT};
use serde::Serialize;

/// Ports kept by default: the three Pro DJ Link ones plus NFS, which is
/// where track analysis files cross the wire on a linked booth. Everything
/// else on a festival network (DHCP, mDNS, whatever else shares the switch)
/// is noise that would multiply the imported session's size for nothing.
const DEFAULT_PORTS: [u16; 4] = [ANNOUNCE_PORT, BEAT_PORT, STATUS_PORT, NFS_PORT];
/// ONC-RPC NFS, always on this well-known port on a player.
const NFS_PORT: u16 = 2049;

/// How often to report progress while walking a large file. A festival set
/// runs to millions of packets, and a tool that prints nothing for minutes
/// is indistinguishable from a hung one.
const PROGRESS_EVERY: u64 = 250_000;

struct Config {
    input: PathBuf,
    out_dir: PathBuf,
    /// Empty means keep every port.
    ports: Vec<u16>,
    /// Seconds from the first packet in the file; `None` means no bound.
    from_sec: Option<f64>,
    to_sec: Option<f64>,
    max_packets: Option<u64>,
    /// Rebuild the files the capture's NFS traffic carried.
    extract_nfs: bool,
    /// Walk and summarise without writing a session directory.
    dry_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            out_dir: PathBuf::from("capture"),
            ports: DEFAULT_PORTS.to_vec(),
            from_sec: None,
            to_sec: None,
            max_packets: None,
            extract_nfs: true,
            dry_run: false,
        }
    }
}

/// Written to the imported session's `session.json`, mirroring the shape
/// `prolink-capture` writes so a reader can tell the two apart by `tool`
/// alone.
#[derive(Serialize)]
struct SessionInfo {
    tool: &'static str,
    version: &'static str,
    args: Vec<String>,
    source_file: String,
    started_at_unix_ms: i64,
    ended_at_unix_ms: i64,
    imported_datagrams: u64,
    ports: Vec<u16>,
    endpoints: Vec<EndpointSummary>,
}

/// One source address seen in the capture, and what it sent.
#[derive(Serialize)]
struct EndpointSummary {
    src: String,
    datagrams: u64,
    /// Destination port to datagram count, so a player (announce/beat/status)
    /// is distinguishable from a metadata client (NFS) at a glance.
    by_port: BTreeMap<u16, u64>,
}

/// Running tally over the walk, printed at the end and stored in
/// `session.json`.
#[derive(Default)]
struct ImportSummary {
    kept: u64,
    filtered_out: u64,
    first_time_micros: Option<i64>,
    last_time_micros: i64,
    by_source: BTreeMap<String, BTreeMap<u16, u64>>,
}

impl ImportSummary {
    fn record(&mut self, datagram: &Datagram) {
        self.kept += 1;
        self.first_time_micros.get_or_insert(datagram.time_micros);
        self.last_time_micros = datagram.time_micros;
        *self
            .by_source
            .entry(datagram.src.to_string())
            .or_default()
            .entry(datagram.dst_port)
            .or_default() += 1;
    }

    fn endpoints(&self) -> Vec<EndpointSummary> {
        self.by_source
            .iter()
            .map(|(src, by_port)| EndpointSummary {
                src: src.clone(),
                datagrams: by_port.values().sum(),
                by_port: by_port.clone(),
            })
            .collect()
    }

    fn duration_sec(&self) -> f64 {
        match self.first_time_micros {
            Some(first) => (self.last_time_micros - first) as f64 / 1e6,
            None => 0.0,
        }
    }
}

fn main() -> ExitCode {
    let config = match parse_args() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}\n");
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    let file = match File::open(&config.input) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("cannot open {}: {error}", config.input.display());
            return ExitCode::FAILURE;
        }
    };

    match import(config, file) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("import failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn import(config: Config, file: File) -> anyhow::Result<()> {
    // A session directory is only created once the file has been proven
    // openable, so a mistyped path doesn't litter `capture/` with empty
    // sessions.
    let writer = match config.dry_run {
        true => None,
        false => Some(CaptureWriter::new(&config.out_dir)?),
    };
    if let Some(writer) = &writer {
        println!("Importing into {}", writer.session_dir().display());
    } else {
        println!("Dry run: walking {} without writing a session", config.input.display());
    }

    let mut reader = PcapngReader::new(BufReader::new(file));
    let mut summary = ImportSummary::default();
    let mut nfs = config.extract_nfs.then(NfsExtractor::new);
    // Bounds are given relative to the first packet in the file, which is
    // only known once it has been read.
    let mut capture_start_micros: Option<i64> = None;

    while let Some(datagram) = reader.next_datagram()? {
        let start = *capture_start_micros.get_or_insert(datagram.time_micros);
        let offset_sec = (datagram.time_micros - start) as f64 / 1e6;

        if config.to_sec.is_some_and(|to| offset_sec > to) {
            break;
        }
        if config.from_sec.is_some_and(|from| offset_sec < from) || !keeps(&config, &datagram) {
            summary.filtered_out += 1;
            continue;
        }

        summary.record(&datagram);
        if let Some(nfs) = &mut nfs {
            feed_nfs(nfs, &datagram);
        }
        if let Some(writer) = &writer {
            let record = UdpRecord::at(datagram.time_micros, datagram.dst_port, IpAddr::V4(datagram.src), &datagram.payload);
            // One unwritable record must not end an import that may have
            // hours of traffic left; the count in the summary is what tells
            // the operator something went wrong.
            if let Err(error) = writer.write_udp(&record) {
                eprintln!("failed to write a udp.jsonl record: {error}");
            }
        }
        if summary.kept % PROGRESS_EVERY == 0 {
            eprintln!("  {} datagrams imported ({:.0}s of capture)", summary.kept, offset_sec);
        }
        if config.max_packets.is_some_and(|max| summary.kept >= max) {
            break;
        }
    }

    let stats = reader.stats();
    if let Some(writer) = &writer {
        writer.write_session_info(&session_info(&config, &summary))?;
    }
    print_summary(&summary, stats);
    if let Some(nfs) = nfs {
        write_recovered_files(nfs, writer.as_ref());
    }
    Ok(())
}

/// Both ports are tested, not just the destination: a server answers from
/// its well-known port to an ephemeral one, so filtering on the destination
/// alone would keep every NFS call and drop every reply — the half that
/// carries the bytes.
fn keeps(config: &Config, datagram: &Datagram) -> bool {
    config.ports.is_empty() || config.ports.contains(&datagram.dst_port) || config.ports.contains(&datagram.src_port)
}

/// Routes one datagram to the extractor by direction: anything addressed to
/// the NFS port is a client's call, anything sent *from* it is a server's
/// reply — and the server's own address is what the recovered files are
/// filed under.
fn feed_nfs(extractor: &mut NfsExtractor, datagram: &Datagram) {
    if datagram.dst_port == NFS_PORT {
        extractor.observe_call(&datagram.payload);
    } else if datagram.src_port == NFS_PORT {
        extractor.observe_reply(datagram.src, &datagram.payload);
    }
}

/// Writes every file the NFS traffic carried, and reports what came back
/// short. An incomplete file is written anyway — partial analysis bytes are
/// still worth parsing offline — but never silently: a file handed to a
/// parser as if it were whole is exactly the mistake this reports against.
fn write_recovered_files(extractor: NfsExtractor, writer: Option<&CaptureWriter>) {
    let stats = extractor.stats();
    let files = extractor.into_files();
    print_nfs_summary(stats, files.len());
    let Some(writer) = writer else {
        for file in &files {
            println!("  {} {} ({} bytes{})", file.server, file.path, file.bytes.len(), completeness(file.is_complete()));
        }
        return;
    };
    for file in &files {
        match writer.nfs_recovered_path(file.server, &file.path).and_then(|path| {
            write_raw_file(&path, &file.bytes)?;
            Ok(path)
        }) {
            Ok(path) => println!("  {} ({} bytes{})", path.display(), file.bytes.len(), completeness(file.is_complete())),
            Err(error) => eprintln!("  failed to write {} from {}: {error}", file.path, file.server),
        }
    }
}

fn completeness(complete: bool) -> &'static str {
    if complete {
        ""
    } else {
        ", INCOMPLETE"
    }
}

fn print_nfs_summary(stats: ExtractStats, file_count: usize) {
    println!();
    println!("NFS traffic: {} lookups, {} reads, {} read replies", stats.lookup_calls, stats.read_calls, stats.read_replies);
    if stats.replies_without_a_call > 0 {
        println!("  replies with no matching call in the capture: {}", stats.replies_without_a_call);
    }
    if stats.undecodable > 0 {
        println!("  undecodable RPC datagrams: {}", stats.undecodable);
    }
    println!("Recovered {file_count} files:");
}

fn session_info(config: &Config, summary: &ImportSummary) -> SessionInfo {
    SessionInfo {
        tool: "prolink-pcap",
        version: env!("CARGO_PKG_VERSION"),
        args: std::env::args().collect(),
        source_file: config.input.display().to_string(),
        started_at_unix_ms: summary.first_time_micros.unwrap_or(0) / 1_000,
        ended_at_unix_ms: summary.last_time_micros / 1_000,
        imported_datagrams: summary.kept,
        ports: config.ports.clone(),
        endpoints: summary.endpoints(),
    }
}

fn print_summary(summary: &ImportSummary, stats: PcapngStats) {
    println!();
    println!("Imported {} datagrams over {:.1}s of capture", summary.kept, summary.duration_sec());
    println!("  packets in file:      {}", stats.packets_seen);
    println!("  not UDP/IPv4:         {}", stats.packets_skipped);
    if stats.packets_unsupported_link > 0 {
        println!("  unsupported link type: {} — this file is not an Ethernet capture", stats.packets_unsupported_link);
    }
    println!("  filtered out by port/time: {}", summary.filtered_out);
    if stats.incomplete_datagrams > 0 {
        println!("  fragmented datagrams never completed: {}", stats.incomplete_datagrams);
    }
    println!();
    println!("Endpoints:");
    for endpoint in summary.endpoints() {
        let ports: Vec<String> = endpoint.by_port.iter().map(|(port, count)| format!("{port}:{count}")).collect();
        println!("  {:<16} {:>9} datagrams  [{}]", endpoint.src, endpoint.datagrams, ports.join(" "));
    }
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config::default();
    let mut input: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--in" => input = Some(PathBuf::from(next_value(&mut args, "--in")?)),
            "--out" => config.out_dir = PathBuf::from(next_value(&mut args, "--out")?),
            "--ports" => config.ports = parse_ports(&next_value(&mut args, "--ports")?)?,
            "--all-ports" => config.ports.clear(),
            "--from-sec" => config.from_sec = Some(parse_number(&mut args, "--from-sec")?),
            "--to-sec" => config.to_sec = Some(parse_number(&mut args, "--to-sec")?),
            "--max-packets" => config.max_packets = Some(parse_number(&mut args, "--max-packets")?),
            "--no-nfs" => config.extract_nfs = false,
            "--dry-run" => config.dry_run = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    config.input = input.ok_or_else(|| "--in FILE.pcapng is required".to_string())?;
    if let (Some(from), Some(to)) = (config.from_sec, config.to_sec) {
        if to < from {
            return Err(format!("--to-sec {to} is before --from-sec {from}"));
        }
    }
    Ok(config)
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_number<T: std::str::FromStr>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<T, String> {
    next_value(args, flag)?.parse().map_err(|_| format!("{flag} needs a number"))
}

fn parse_ports(value: &str) -> Result<Vec<u16>, String> {
    value
        .split(',')
        .map(|port| port.trim().parse().map_err(|_| format!("not a port number: {port}")))
        .collect()
}

fn print_usage() {
    eprintln!("prolink-pcap — imports a Wireshark .pcapng into a prolink capture session");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --in FILE.pcapng   Wireshark recording to import (required)");
    eprintln!("  --out DIR          where the session directory is created (default capture/)");
    eprintln!("  --ports A,B,C      destination ports to keep (default 50000,50001,50002,2049)");
    eprintln!("  --all-ports        keep every UDP port in the file");
    eprintln!("  --from-sec N       skip the first N seconds of the capture");
    eprintln!("  --to-sec N         stop after N seconds of the capture");
    eprintln!("  --max-packets N    stop after importing N datagrams");
    eprintln!("  --no-nfs           skip rebuilding the files the NFS traffic carried");
    eprintln!("  --dry-run          walk and summarise without writing anything");
}
