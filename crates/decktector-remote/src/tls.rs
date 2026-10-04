//! TLS support for the remote view server.
//!
//! Generates ephemeral in-memory self-signed X.509 certificates so browsers
//! and mobile devices can connect over HTTPS and WSS with full link encryption
//! on open festival or venue networks.

use std::net::IpAddr;

use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};

/// Generates an ephemeral self-signed certificate and private key in PEM format,
/// valid for `localhost`, `127.0.0.1` and the optional bound IP address.
pub fn generate_self_signed_pem(bound_ip: Option<IpAddr>) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Decktector Remote");
    dn.push(DnType::OrganizationName, "Decktector");
    params.distinguished_name = dn;

    let mut subject_alt_names = vec![
        SanType::DnsName("localhost".to_string().try_into()?),
        SanType::IpAddress(IpAddr::from([127, 0, 0, 1])),
    ];
    if let Some(ip) = bound_ip {
        if !ip.is_loopback() && !ip.is_unspecified() {
            subject_alt_names.push(SanType::IpAddress(ip));
        }
    }
    params.subject_alt_names = subject_alt_names;

    let key_pair = KeyPair::generate().context("failed to generate TLS keypair")?;
    let cert = params
        .self_signed(&key_pair)
        .context("failed to generate self-signed TLS certificate")?;

    let cert_pem = cert.pem().into_bytes();
    let key_pem = key_pair.serialize_pem().into_bytes();

    Ok((cert_pem, key_pem))
}

/// Builds an `axum-server` [`RustlsConfig`] from PEM-encoded certificate and key bytes.
pub async fn rustls_config_from_pem(
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
) -> anyhow::Result<RustlsConfig> {
    RustlsConfig::from_pem(cert_pem, key_pem)
        .await
        .context("failed to initialize rustls TLS config")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generates_valid_self_signed_rustls_config() {
        let (cert_pem, key_pem) =
            generate_self_signed_pem(Some("192.168.1.42".parse().unwrap())).unwrap();
        assert!(!cert_pem.is_empty());
        assert!(!key_pem.is_empty());

        let config = rustls_config_from_pem(cert_pem, key_pem).await;
        assert!(config.is_ok());
    }
}
