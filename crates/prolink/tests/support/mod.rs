//! Shared test-only fixture builders, used by more than one file under
//! `crates/prolink/tests/` — not a test target itself (see the `tests/`
//! module-sharing convention: files directly in `tests/` each become their
//! own binary, files nested one level under `tests/<dir>/` don't).
//!
//! The builders themselves live in [`prolink::test_support`] so other
//! crates' tests can reach the same single copy; this re-export keeps the
//! `support::build_minimal_pdb_bytes()` call sites here unchanged.

pub use prolink::test_support::build_minimal_pdb_bytes;
