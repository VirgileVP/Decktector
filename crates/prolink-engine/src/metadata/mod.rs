//! Track metadata: fetches title/artist/album/genre/key/label/comment/bpm,
//! beat grid, waveform, cues and artwork for a deck's loaded track. Two
//! routes, tried in order (see [`fetcher::MetadataFetcher`]):
//!
//! - [`nfs`]: reads `export.pdb` and the track's own analysis file
//!   straight off the player over NFS (`prolink::nfs`) — needs no virtual
//!   player number, gets everything including cues and artwork, but rests
//!   on the least-verified-without-hardware protocol code in this
//!   codebase (see `prolink::nfs::pdb`'s doc comment).
//! - [`normalize`] (dbserver): a TCP query to the player's own database
//!   service (`prolink::dbserver`) — needs a virtual player number in 1-4,
//!   fully protocol-documented and fake-server-tested, but its cue-list
//!   wire format is unimplemented (see `prolink::dbserver::cues`) and it
//!   has no artwork route wired up (see `fetcher`'s doc comment on why).

mod colors;
mod fetcher;
pub mod nfs;
mod normalize;

pub use fetcher::{FetchRequest, LoadedTrack, MetadataFetcher};
