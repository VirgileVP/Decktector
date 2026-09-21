//! Links the Win32 libraries that a statically linked libcrypto needs.
//!
//! `rusqlite`'s `bundled-sqlcipher` feature links OpenSSL's libcrypto, which on
//! Windows calls into the event log, the window station and the certificate store.
//! `libsqlite3-sys` only asks the linker for `libcrypto` itself, so the import
//! libraries behind those calls have to be named here — otherwise every binary
//! that depends on this crate fails with unresolved `__imp_*` symbols.
//!
//! The dynamic OpenSSL build pulls them in through its own DLL, so this only
//! matters for the static archive `scripts/openssl-windows.mjs` stages.

/// Win32 import libraries backing libcrypto's `OPENSSL_showfatal`, `OPENSSL_isservice`,
/// entropy pool and `winstore` certificate store.
const WINDOWS_CRYPTO_DEPENDENCIES: [&str; 3] = ["advapi32", "user32", "crypt32"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for library in WINDOWS_CRYPTO_DEPENDENCIES {
            println!("cargo:rustc-link-lib=dylib={library}");
        }
    }
}
