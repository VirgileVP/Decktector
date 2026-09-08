//! `NfsClient`: the orchestration layer over [`super::portmap`],
//! [`super::mount`] and [`super::procedures`] — connect, mount a media
//! slot, and read whole files by path.
//!
//! No overall timeout lives here, same discipline as
//! [`crate::dbserver::client::Connection`]: [`Self::call`] bounds *one* RPC
//! exchange (retransmit at 250ms, doubling, giving up after a few tries —
//! Deep Symmetry's `crate-digger` reference implementation's own tuning,
//! whose comment reads "the players respond within a few milliseconds if
//! they are going to at all"), but [`Self::open`] and [`Self::read_file`]
//! each make several such calls in sequence with no ceiling of their own —
//! wrapping the whole sequence in a [`tokio::time::timeout`] is the
//! caller's job (`prolink-engine::metadata::nfs`), matching how
//! `dbserver::Connection`'s callers already apply `CONNECT_TIMEOUT`/
//! `METADATA_TIMEOUT` externally rather than baking a policy in here.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::timeout;

use super::mount::{self, FileHandle};
use super::{portmap, procedures};
use crate::status::MediaSlot;

/// Matches `crate-digger`'s own retransmit tuning exactly.
const INITIAL_RETRANSMIT_TIMEOUT: Duration = Duration::from_millis(250);
/// Retries beyond the first attempt; wait time doubles each time
/// (250/500/1000/2000ms — ~3.75s worst case before giving up on one call).
const MAX_RETRIES: u32 = 4;
/// Deliberately well under the protocol max ([`procedures::NFS_MAXDATA`] =
/// 8192) — `crate-digger`'s own choice, "a trade-off between reducing the
/// number of requests and reducing IP fragmentation and expensive
/// retransmissions of already-sent fragments whenever one is lost."
const READ_CHUNK_SIZE: u32 = 2048;
/// The portmapper always listens here — the one genuinely fixed port in
/// this whole flow (everything else is resolved through it).
const PORTMAP_PORT: u16 = 111;

/// A connection to one player's NFS-like file server, mounted on a specific
/// media slot.
pub struct NfsClient {
    socket: UdpSocket,
    player_ip: Ipv4Addr,
    portmap_port: u16,
    mount_port: u16,
    nfs_port: u16,
    root_handle: FileHandle,
    next_xid: u32,
}

impl NfsClient {
    /// Resolves the MOUNT and NFS daemons' ports via the portmapper, mounts
    /// `slot`, and is ready for [`Self::read_file`] once this returns.
    pub async fn open(
        player_ip: Ipv4Addr,
        bind_ip: Ipv4Addr,
        slot: MediaSlot,
    ) -> std::io::Result<Self> {
        Self::open_with_portmap_port(player_ip, bind_ip, slot, PORTMAP_PORT).await
    }

    /// Same as [`Self::open`], but against a given portmapper port instead
    /// of the real, fixed, privileged one (111) — this crate's own tests
    /// use it against a fake portmapper on an ephemeral port, since binding
    /// 111 in a test would need elevated privileges and risks colliding
    /// with a real `rpcbind`/portmapper already running on the test
    /// machine.
    pub async fn open_with_portmap_port(
        player_ip: Ipv4Addr,
        bind_ip: Ipv4Addr,
        slot: MediaSlot,
        portmap_port: u16,
    ) -> std::io::Result<Self> {
        let socket = UdpSocket::bind((bind_ip, 0)).await?;
        let mut client = Self {
            socket,
            player_ip,
            portmap_port,
            mount_port: 0,
            nfs_port: 0,
            root_handle: [0; mount::FILE_HANDLE_LEN],
            next_xid: 1,
        };

        client.mount_port = client
            .get_port(mount::MOUNT_PROGRAM, mount::MOUNT_VERSION)
            .await?;

        let dir_path = slot_path(slot).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("no NFS mount path for media slot {slot:?}"),
            )
        })?;
        let xid = client.fresh_xid();
        let call = mount::build_mnt_call(xid, dir_path);
        let reply = client.call(client.mount_port, &call).await?;
        client.root_handle =
            mount::parse_mnt_reply(&reply, xid).ok_or_else(|| protocol_error("mount", dir_path))?;

        client.nfs_port = client
            .get_port(procedures::NFS_PROGRAM, procedures::NFS_VERSION)
            .await?;
        Ok(client)
    }

    /// Fetches a whole file by its path relative to the mounted slot's
    /// root (e.g. `"PIONEER/rekordbox/export.pdb"`) — one `LOOKUP` per
    /// path segment, chaining file handles, then a `READ` loop until the
    /// file size `LOOKUP` reported has been fully read.
    pub async fn read_file(&mut self, path: &str) -> std::io::Result<Vec<u8>> {
        let mut handle = self.root_handle;
        let mut size = 0u32;
        for segment in path.split('/').filter(|part| !part.is_empty()) {
            let xid = self.fresh_xid();
            let call = procedures::build_lookup_call(xid, handle, segment);
            let reply = self.call(self.nfs_port, &call).await?;
            let result = procedures::parse_lookup_reply(&reply, xid)
                .ok_or_else(|| protocol_error("lookup", segment))?;
            handle = result.handle;
            size = result.attributes.size;
        }

        let mut data = Vec::with_capacity(size as usize);
        let mut offset = 0u32;
        while offset < size {
            let count = READ_CHUNK_SIZE.min(size - offset);
            let xid = self.fresh_xid();
            let call = procedures::build_read_call(xid, handle, offset, count);
            let reply = self.call(self.nfs_port, &call).await?;
            let result = procedures::parse_read_reply(&reply, xid)
                .ok_or_else(|| protocol_error("read", path))?;
            if result.data.is_empty() {
                break; // Guards against looping forever if a server reports a size it never actually delivers.
            }
            offset += result.data.len() as u32;
            data.extend_from_slice(&result.data);
        }
        Ok(data)
    }

    /// Downloads a whole file directly to a destination file on disk, streaming in chunks
    /// rather than buffering the entire file in RAM, and invokes `on_progress(bytes_downloaded, total_bytes)`
    /// as chunks arrive.
    pub async fn download_file_to_path<F>(
        &mut self,
        path: &str,
        destination: &std::path::Path,
        mut on_progress: F,
    ) -> std::io::Result<u64>
    where
        F: FnMut(u64, u64) + Send,
    {
        use tokio::io::AsyncWriteExt;

        let mut handle = self.root_handle;
        let mut size = 0u32;
        for segment in path.split('/').filter(|part| !part.is_empty()) {
            let xid = self.fresh_xid();
            let call = procedures::build_lookup_call(xid, handle, segment);
            let reply = self.call(self.nfs_port, &call).await?;
            let result = procedures::parse_lookup_reply(&reply, xid)
                .ok_or_else(|| protocol_error("lookup", segment))?;
            handle = result.handle;
            size = result.attributes.size;
        }

        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(destination).await?;

        let mut offset = 0u32;
        on_progress(0, size as u64);

        while offset < size {
            let count = READ_CHUNK_SIZE.min(size - offset);
            let xid = self.fresh_xid();
            let call = procedures::build_read_call(xid, handle, offset, count);
            let reply = self.call(self.nfs_port, &call).await?;
            let result = procedures::parse_read_reply(&reply, xid)
                .ok_or_else(|| protocol_error("read", path))?;
            if result.data.is_empty() {
                break;
            }
            file.write_all(&result.data).await?;
            offset += result.data.len() as u32;
            on_progress(offset as u64, size as u64);
            tokio::task::yield_now().await;
        }

        file.flush().await?;
        Ok(offset as u64)
    }

    async fn get_port(&mut self, program: u32, version: u32) -> std::io::Result<u16> {
        let xid = self.fresh_xid();
        let call = portmap::build_getport_call(xid, program, version);
        let reply = self.call(self.portmap_port, &call).await?;
        portmap::parse_getport_reply(&reply, xid).ok_or_else(|| {
            protocol_error(
                "portmapper",
                &format!("program {program} version {version}"),
            )
        })
    }

    /// One RPC exchange: send, wait, retransmit with doubling backoff on
    /// timeout, give up after [`MAX_RETRIES`]. A reply from anyone other
    /// than the player we're talking to is ignored rather than accepted —
    /// this socket is unicast to one target, so that should never happen,
    /// but validating costs nothing.
    async fn call(&self, target_port: u16, request: &[u8]) -> std::io::Result<Vec<u8>> {
        let target = SocketAddr::from((self.player_ip, target_port));
        let mut wait = INITIAL_RETRANSMIT_TIMEOUT;
        let mut buf = vec![0u8; procedures::NFS_MAXDATA + 512];

        for _ in 0..=MAX_RETRIES {
            self.socket.send_to(request, target).await?;
            loop {
                match timeout(wait, self.socket.recv_from(&mut buf)).await {
                    Ok(Ok((len, from))) if from.ip() == IpAddr::V4(self.player_ip) => {
                        return Ok(buf[..len].to_vec());
                    }
                    Ok(Ok(_)) => continue, // Unrelated sender; keep waiting within this attempt.
                    Ok(Err(error)) => return Err(error),
                    Err(_elapsed) => break, // This attempt timed out; retransmit.
                }
            }
            wait *= 2;
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "no reply from the player after retrying",
        ))
    }

    fn fresh_xid(&mut self) -> u32 {
        let xid = self.next_xid;
        self.next_xid = self.next_xid.wrapping_add(1);
        xid
    }
}

/// The MOUNT export path per media slot. **Low confidence** — `"/B/"`/
/// `"/C/"` are the well-known values in this ecosystem's prior art, but
/// which slot maps to which letter (and whether a third exists) hasn't
/// been checked against real hardware; a wrong mapping fails at the very
/// first NFS call ([`NfsClient::open`]'s `MNT`), which at least makes it an
/// obvious, early failure rather than a subtle one. See the phase-4 plan's
/// Open Questions.
fn slot_path(slot: MediaSlot) -> Option<&'static str> {
    match slot {
        MediaSlot::Sd => Some("/B/"),
        MediaSlot::Usb => Some("/C/"),
        MediaSlot::Cd | MediaSlot::Rekordbox | MediaSlot::Streaming | MediaSlot::Unknown(_) => None,
    }
}

fn protocol_error(step: &str, detail: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("NFS {step} failed for {detail}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sd_and_usb_slots_have_a_mount_path() {
        assert_eq!(slot_path(MediaSlot::Sd), Some("/B/"));
        assert_eq!(slot_path(MediaSlot::Usb), Some("/C/"));
    }

    #[test]
    fn other_slots_have_no_nfs_mount_path() {
        assert_eq!(slot_path(MediaSlot::Rekordbox), None);
        assert_eq!(slot_path(MediaSlot::Cd), None);
    }
}
