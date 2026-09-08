//! The NFS metadata route: reads `export.pdb` and its analysis files
//! straight off a player, needing no virtual-player number at all — unlike
//! [`crate::dbserver`], which only answers a virtual CDJ numbered 1-4. This
//! was the pre-Rust Electron app's primary metadata route; `dbserver`
//! shipped first in this port only because it's fully protocol-documented
//! and fake-server-testable, where this module's ONC-RPC/MOUNT/NFSv2
//! transport and hand-rolled PDB parser carry real, explicitly-flagged
//! uncertainty pending real-hardware validation — see [`pdb`]'s and
//! [`client`]'s doc comments for exactly which parts.
//!
//! No domain knowledge here — mapping onto `decktective_core::model` lives
//! in `prolink-engine::metadata::nfs`, the same split every other protocol
//! module in this crate already uses.

pub mod client;
pub mod cues;
pub mod devicelibraryplus;
pub mod extract;
pub mod mount;
pub mod pdb;
pub mod portmap;
pub mod procedures;
pub mod rpc;
pub mod xdr;

pub use client::NfsClient;
pub use cues::{parse_anlz_cues, parse_pco2, parse_pcob, AnlzCue};
pub use devicelibraryplus::parse_export_library;
