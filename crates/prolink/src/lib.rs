//! Pro DJ Link protocol client.
//!
//! Speaks the wire format of Pioneer/AlphaTheta CDJ and XDJ players: announces
//! a virtual CDJ, discovers devices, reads player status and beat packets, and
//! fetches track metadata. Nothing here knows about Decktective's domain model
//! — that mapping lives in `decktective-core`.
//!
//! The protocol is not documented by the manufacturer. This implementation
//! follows Deep Symmetry's analysis at <https://djl-analysis.deepsymmetry.org/>.
//!
//! Landing in phases: the UDP layer (announce, discovery, status, beat) comes
//! first, then metadata over NFS, then the remote database fallback.

/// UDP port players announce themselves on.
pub const ANNOUNCE_PORT: u16 = 50000;
/// UDP port carrying beat and precise-position packets.
pub const BEAT_PORT: u16 = 50001;
/// UDP port carrying player status packets.
pub const STATUS_PORT: u16 = 50002;

/// Magic header every Pro DJ Link packet starts with.
pub const PROLINK_HEADER: [u8; 10] = [0x51, 0x73, 0x70, 0x74, 0x31, 0x57, 0x6d, 0x4a, 0x4f, 0x4c];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_the_qspt1_wmjol_magic() {
        assert_eq!(&PROLINK_HEADER[..5], b"Qspt1");
    }
}
