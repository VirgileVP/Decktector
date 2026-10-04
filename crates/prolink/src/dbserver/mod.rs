//! The dbserver/remotedb protocol: a TCP query service every player runs,
//! used here to fetch track metadata, beat grid, cues and waveform for
//! whatever track a deck has loaded. No domain knowledge here — mapping
//! onto `decktector_core::model::TrackInfo` lives in
//! `prolink-engine::metadata`, the same split `status.rs`/`beat.rs` use.
//!
//! See [`client`]'s doc comment for the confidence level of the exact wire
//! bytes: the message envelope and query/response type ids are cross
//! -checked against Deep Symmetry's `beat-link`, but nothing in
//! decktector's history exercised this exact TCP exchange before this
//! module, unlike the UDP parsers elsewhere in this crate.

pub mod beatgrid;
pub mod client;
pub mod cues;
pub mod message;
pub mod queries;
pub mod waveform;

pub use client::{Connection, TrackMenuItem};
