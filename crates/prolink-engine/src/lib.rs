//! Real Pro DJ Link `Engine`, backing decktective with actual CDJ/XDJ
//! hardware instead of `decktective_core::sim::SimEngine`'s scripted decks.
//!
//! This phase covers live network presence and deck state — play/pause,
//! pitch, tempo, master/sync/on-air, and an approximate playhead. It does
//! **not** yet fetch track metadata (title/artist/album/artwork/waveform):
//! every deck's `track_id` is populated, but there is no matching
//! `TrackInfo` until a later phase adds the NFS/dbserver metadata routes
//! (see the project's README porting table).
//!
//! The renderer never talks to this crate directly — the Tauri app shell
//! only needs [`ProlinkEngine`], constructed once and handed to the same
//! `decktective_core::Engine`-shaped plumbing `SimEngine` already uses.

mod engine;
pub mod metadata;
mod network;
mod normalize;
mod position;

pub use engine::ProlinkEngine;
