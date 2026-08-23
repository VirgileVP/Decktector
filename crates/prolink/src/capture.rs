//! Records a Pro DJ Link hardware capture session to disk: raw UDP
//! payloads, device discovery/status snapshots, and per-player NFS/dbserver
//! metadata pulls. See `HARDWARE-CAPTURE-PLAN.md` (repo root) for why this
//! exists and the full on-disk layout; [`bin/prolink-capture.rs`] is the
//! only caller.
//!
//! The governing rule: write raw bytes to disk before attempting to parse
//! them, and never let a write or parse failure abort a capture run. Every
//! record here is flushed immediately (JSONL logs) or written and fsynced
//! in one shot (standalone files) — a capture that is still buffered in
//! memory when the machine is unplugged mid-session is a capture that does
//! not exist.
//!
//! `std::fs` and `std::sync::Mutex` are used instead of `tokio::fs`/async
//! locking on purpose: this crate's tokio feature set doesn't include `fs`,
//! and the packet rate a capture session sees (roughly 20 packets/sec
//! across four decks at a 200ms status interval) makes the blocking I/O
//! cost irrelevant next to the simplicity of not needing it.
//!
//! JSONL, not a binary format: a truncated final line from a hard unplug is
//! discardable and the rest of the file still parses (see
//! [`read_jsonl`]), and it's readable with no custom tooling on-site.
//! Payloads are hex-encoded rather than base64 — a four-line loop, not a
//! new dependency.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const UDP_LOG: &str = "udp.jsonl";
const DEVICES_LOG: &str = "devices.jsonl";
const STATUS_DECODED_LOG: &str = "status-decoded.jsonl";
const SESSION_INFO_FILE: &str = "session.json";
const NOTES_FILE: &str = "notes.md";
const NFS_DIR: &str = "nfs";
const DBSERVER_DIR: &str = "dbserver";
const ANALYSIS_DIR: &str = "analysis";
const ARTWORK_DIR: &str = "artwork";
/// Suffix on the per-player directory holding files rebuilt out of a
/// recorded NFS conversation, keeping them distinct from what a live pull
/// wrote (`nfs/<ip>-<slot>/`).
const RECOVERED_SUFFIX: &str = "recovered";

const NOTES_TEMPLATE: &str = "\
# Session notes

Write the wall-clock time and the action as you go, so capture timestamps
can be correlated afterwards. See RUNBOOK-HARDWARE-SESSION.md for the
scenarios worth exercising while recording.

- HH:MM:SS —
";

/// Appends one wall-clock-stamped line to a session's `notes.md`.
///
/// Correlating the capture against what physically happened on the gear is
/// what makes the bytes interpretable afterwards, and the runbook's manual
/// version of this ("write the time and the action as you go") is the step
/// most likely to be skipped under booth pressure or reconstructed wrong
/// from memory. `prolink-session` calls this as each guided step is
/// confirmed; `notes.md` stays plain Markdown that anyone can also edit by
/// hand mid-session.
///
/// Takes the session directory rather than hanging off [`CaptureWriter`]:
/// the guided driver annotates a session owned by a separate
/// `prolink-capture` process, so it has the path but not the writer.
pub fn append_note(session_dir: &Path, text: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new().create(true).append(true).open(session_dir.join(NOTES_FILE))?;
    writeln!(file, "- {} — {}", Local::now().format("%H:%M:%S"), text)
}

/// One received UDP datagram, as recorded to `udp.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UdpRecord {
    /// Microseconds since the Unix epoch.
    pub t: i64,
    pub port: u16,
    pub src: String,
    pub len: usize,
    pub hex: String,
}

impl UdpRecord {
    pub fn new(port: u16, src: IpAddr, payload: &[u8]) -> Self {
        Self::at(now_micros(), port, src, payload)
    }

    /// Same record with the timestamp supplied rather than read from the
    /// clock — for an importer replaying datagrams out of a file, where the
    /// only meaningful time is the one the capture recorded (see
    /// `bin/prolink-pcap.rs`).
    pub fn at(time_micros: i64, port: u16, src: IpAddr, payload: &[u8]) -> Self {
        Self { t: time_micros, port, src: src.to_string(), len: payload.len(), hex: encode_hex(payload) }
    }

    /// Decodes [`Self::hex`] back to raw bytes; `None` if it isn't valid hex
    /// (shouldn't happen for anything this module wrote itself, but a
    /// hand-edited or corrupted capture file is exactly the kind of input
    /// the replay harness needs to survive rather than panic on).
    pub fn payload(&self) -> Option<Vec<u8>> {
        decode_hex(&self.hex)
    }
}

fn now_micros() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_micros() as i64).unwrap_or(0)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

/// A single JSONL log: one file, appended one flushed record at a time.
/// Wrapped in a [`Mutex`] rather than requiring `&mut self` so
/// [`CaptureWriter`] can be shared across the concurrently-running capture
/// phases via `Arc` without extra synchronization at the call site.
struct JsonlWriter {
    file: Mutex<BufWriter<File>>,
}

impl JsonlWriter {
    fn create(path: &Path) -> io::Result<Self> {
        Ok(Self { file: Mutex::new(BufWriter::new(File::create(path)?)) })
    }

    /// Serializes `record`, appends it as one line, and flushes — a panic
    /// elsewhere while another thread holds this lock must not permanently
    /// stop this log from accepting further records, so a poisoned mutex is
    /// recovered from rather than propagated.
    fn write<T: Serialize>(&self, record: &T) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        serde_json::to_writer(&mut *guard, record)?;
        guard.write_all(b"\n")?;
        guard.flush()
    }
}

/// Writes `bytes` to `path` in one shot and fsyncs before returning — the
/// governing rule of this whole module: bytes are safely on disk before
/// anything downstream ever tries to interpret them.
pub fn write_raw_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Writes `value` as pretty-printed JSON to `path` — used for `session.json`
/// and every `parse-report.json`.
pub fn write_json_file(path: &Path, value: &impl Serialize) -> io::Result<()> {
    write_raw_file(path, &serde_json::to_vec_pretty(value)?)
}

/// Result of a lenient JSONL read: every complete, parseable record, plus
/// whether the file ended with a line that failed to parse (the expected
/// shape of a capture cut off mid-write — e.g. a hard unplug — rather than
/// a parse error to propagate up).
pub struct JsonlRead<T> {
    pub records: Vec<T>,
    pub truncated: bool,
}

/// Reads a JSONL file record by record. A malformed *final* line is treated
/// as truncation (recorded in [`JsonlRead::truncated`], not an error) so a
/// capture cut off mid-write still replays everything captured up to that
/// point. A malformed line anywhere *else* is a genuine data problem and
/// still errors — the leniency this function offers is specifically for the
/// one failure mode an interrupted capture actually produces.
pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> io::Result<JsonlRead<T>> {
    let lines: Vec<String> = BufReader::new(File::open(path)?).lines().collect::<io::Result<_>>()?;
    let last_index = lines.len().checked_sub(1);

    let mut records = Vec::with_capacity(lines.len());
    let mut truncated = false;
    for (index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(record) => records.push(record),
            Err(error) if Some(index) == last_index => {
                truncated = true;
                tracing::debug!(%error, line = index, path = %path.display(), "capture JSONL file ends with a truncated record");
            }
            Err(error) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("malformed JSONL line {index} in {}: {error}", path.display()),
                ));
            }
        }
    }
    Ok(JsonlRead { records, truncated })
}

/// Owns one capture session's directory and its always-present logs
/// (`udp.jsonl`, `devices.jsonl`, `status-decoded.jsonl`, `session.json`).
/// Per-player NFS/dbserver directories are created on demand by
/// [`Self::nfs_capture_dir`]/[`Self::dbserver_capture_dir`], since which
/// players exist isn't known until discovery runs.
pub struct CaptureWriter {
    session_dir: PathBuf,
    udp: JsonlWriter,
    devices: JsonlWriter,
    status_decoded: JsonlWriter,
}

impl CaptureWriter {
    /// Creates `root/capture-YYYYMMDD-HHMMSS/` (UTC) plus its `nfs/` and
    /// `dbserver/` subdirectories, and opens the three top-level JSONL logs.
    /// Every directory a later write might need already exists once this
    /// returns, so a write deep into a session never fails on a missing
    /// parent.
    pub fn new(root: &Path) -> io::Result<Self> {
        let session_dir = root.join(format!("capture-{}", timestamp_label(SystemTime::now())));
        fs::create_dir_all(&session_dir)?;
        fs::create_dir_all(session_dir.join(NFS_DIR))?;
        fs::create_dir_all(session_dir.join(DBSERVER_DIR))?;

        let notes_path = session_dir.join(NOTES_FILE);
        if !notes_path.exists() {
            fs::write(&notes_path, NOTES_TEMPLATE)?;
        }

        Ok(Self {
            udp: JsonlWriter::create(&session_dir.join(UDP_LOG))?,
            devices: JsonlWriter::create(&session_dir.join(DEVICES_LOG))?,
            status_decoded: JsonlWriter::create(&session_dir.join(STATUS_DECODED_LOG))?,
            session_dir,
        })
    }

    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    pub fn write_udp(&self, record: &UdpRecord) -> io::Result<()> {
        self.udp.write(record)
    }

    pub fn write_device(&self, snapshot: &impl Serialize) -> io::Result<()> {
        self.devices.write(snapshot)
    }

    pub fn write_status_decoded(&self, status: &impl Serialize) -> io::Result<()> {
        self.status_decoded.write(status)
    }

    pub fn write_session_info(&self, info: &impl Serialize) -> io::Result<()> {
        write_json_file(&self.session_dir.join(SESSION_INFO_FILE), info)
    }

    /// Ensures `nfs/<player_ip>-<slot_label>/`, its `analysis/` and its
    /// `artwork/` subdirectories exist, and returns the base path — call
    /// before writing anything into it.
    pub fn nfs_capture_dir(&self, player_ip: Ipv4Addr, slot_label: &str) -> io::Result<PathBuf> {
        let dir = self.session_dir.join(NFS_DIR).join(format!("{player_ip}-{slot_label}"));
        fs::create_dir_all(dir.join(ANALYSIS_DIR))?;
        fs::create_dir_all(dir.join(ARTWORK_DIR))?;
        Ok(dir)
    }

    /// Ensures the parent directories exist for a file recovered out of a
    /// recorded NFS conversation, and returns where to write it:
    /// `nfs/<player_ip>-recovered/<path>`.
    ///
    /// `path` comes off the wire — it is whatever names a capture's `LOOKUP`
    /// calls happened to carry — so it is sanitised here rather than
    /// trusted: a segment that is `..`, empty, or holds a separator or a
    /// character no filesystem accepts would otherwise let a crafted or
    /// corrupt capture write outside the session directory.
    pub fn nfs_recovered_path(&self, player_ip: Ipv4Addr, path: &str) -> io::Result<PathBuf> {
        let mut file_path = self.session_dir.join(NFS_DIR).join(format!("{player_ip}-{RECOVERED_SUFFIX}"));
        let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
        if segments.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("recovered file has no usable path: {path:?}")));
        }
        for segment in segments {
            file_path.push(sanitize_path_segment(segment));
        }
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(file_path)
    }

    /// Ensures `dbserver/<player_ip>/` exists and returns its path.
    pub fn dbserver_capture_dir(&self, player_ip: Ipv4Addr) -> io::Result<PathBuf> {
        let dir = self.session_dir.join(DBSERVER_DIR).join(player_ip.to_string());
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

/// Replaces anything a path segment read off the wire must not contain: the
/// parent-directory reference, path separators, and the characters Windows
/// rejects outright. Kept deliberately strict — a recovered file's name is
/// evidence, not a filename the code needs to round-trip.
fn sanitize_path_segment(segment: &str) -> String {
    if segment == ".." || segment == "." {
        return "_".to_string();
    }
    segment
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            control if control.is_control() => '_',
            other => other,
        })
        .collect()
}

fn timestamp_label(time: SystemTime) -> String {
    let total_secs = time.duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0);
    let (year, month, day) = civil_from_days((total_secs / 86_400) as i64);
    let secs_of_day = total_secs % 86_400;
    let (hour, minute, second) = (secs_of_day / 3600, (secs_of_day % 3600) / 60, secs_of_day % 60);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Howard Hinnant's `civil_from_days` algorithm
/// (<http://howardhinnant.github.io/date_algorithms.html>), which converts a
/// day count since the Unix epoch into a proleptic-Gregorian (year, month,
/// day). Hand-rolled specifically so this directory-naming timestamp
/// doesn't need a date/time crate dependency this module otherwise has no
/// use for.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_reads_back_the_unix_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn civil_from_days_reads_a_known_recent_date() {
        // 2024-01-01 is 19723 days after the Unix epoch.
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }

    #[test]
    fn hex_round_trips_arbitrary_bytes() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(decode_hex(&encode_hex(&bytes)), Some(bytes));
    }

    #[test]
    fn udp_record_round_trip_preserves_payload_bytes_exactly() {
        let payload = vec![0x51, 0x73, 0x70, 0x74, 0x31, 0x00, 0xff, 0x10];
        let record = UdpRecord::new(50002, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 31)), &payload);
        assert_eq!(record.payload(), Some(payload));
    }

    #[test]
    fn writer_and_reader_round_trip_through_a_real_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let writer = CaptureWriter::new(dir.path()).expect("create capture writer");

        let record = UdpRecord::new(50000, IpAddr::V4(Ipv4Addr::LOCALHOST), &[1, 2, 3]);
        writer.write_udp(&record).expect("write udp record");
        writer.write_udp(&record).expect("write udp record");

        let read: JsonlRead<UdpRecord> = read_jsonl(&writer.session_dir().join(UDP_LOG)).expect("read back");
        assert_eq!(read.records, vec![record.clone(), record]);
        assert!(!read.truncated);
    }

    #[test]
    fn a_truncated_final_line_is_reported_not_errored() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("truncated.jsonl");
        let complete = UdpRecord::new(50002, IpAddr::V4(Ipv4Addr::LOCALHOST), &[9, 9]);
        let mut contents = serde_json::to_string(&complete).expect("serialize");
        contents.push('\n');
        contents.push_str(r#"{"t":123,"port":5000"#); // Cut off mid-object, no closing brace.
        fs::write(&path, contents).expect("write truncated file");

        let read: JsonlRead<UdpRecord> = read_jsonl(&path).expect("should read the complete records");
        assert_eq!(read.records, vec![complete]);
        assert!(read.truncated);
    }

    #[test]
    fn a_malformed_line_in_the_middle_of_the_file_is_a_real_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("corrupt.jsonl");
        let complete = UdpRecord::new(50002, IpAddr::V4(Ipv4Addr::LOCALHOST), &[9, 9]);
        let contents = format!("not valid json\n{}\n", serde_json::to_string(&complete).expect("serialize"));
        fs::write(&path, contents).expect("write corrupt file");

        let result: io::Result<JsonlRead<UdpRecord>> = read_jsonl(&path);
        assert!(result.is_err(), "a corrupt line that isn't the last line should not be silently skipped");
    }

    #[test]
    fn session_directory_creation_is_idempotent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = CaptureWriter::new(dir.path()).expect("first creation");
        // Calling into the same already-existing session directory a second
        // time (e.g. a retry after a transient error) must not fail.
        fs::create_dir_all(first.session_dir()).expect("re-creating an existing session dir is a no-op, not an error");
        fs::create_dir_all(first.session_dir().join(NFS_DIR)).expect("re-creating nfs/ is a no-op");
    }

    #[test]
    fn nfs_and_dbserver_capture_dirs_are_created_on_demand() {
        let dir = tempfile::tempdir().expect("temp dir");
        let writer = CaptureWriter::new(dir.path()).expect("create capture writer");

        let nfs_dir = writer.nfs_capture_dir(Ipv4Addr::new(192, 168, 1, 31), "usb").expect("create nfs dir");
        assert!(nfs_dir.join(ANALYSIS_DIR).is_dir());
        assert!(nfs_dir.join(ARTWORK_DIR).is_dir());

        let db_dir = writer.dbserver_capture_dir(Ipv4Addr::new(192, 168, 1, 31)).expect("create dbserver dir");
        assert!(db_dir.is_dir());
    }

    #[test]
    fn write_raw_file_then_read_back_preserves_bytes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("export.pdb");
        let bytes = vec![0xde, 0xad, 0xbe, 0xef];
        write_raw_file(&path, &bytes).expect("write raw file");
        assert_eq!(fs::read(&path).expect("read back"), bytes);
    }
}
