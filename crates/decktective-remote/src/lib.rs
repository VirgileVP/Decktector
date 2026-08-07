//! LAN web server that streams the engine's state to remote browsers.
//!
//! A read-only second screen: any browser on the local network can watch
//! the same decks the desktop app shows, fed by the same [`Engine`] handle
//! the Tauri shell uses. This crate knows nothing about Tauri — it depends
//! only on `decktective-core`, the same shape as `decktective-timecode`.
//!
//! [`Engine`]: decktective_core::Engine

pub mod assets;
pub mod hub;
pub mod server;
pub mod wire;

pub use assets::AssetSource;
pub use server::{RemoteConfig, RemoteService, RemoteStatus, DEFAULT_REMOTE_PORT};
