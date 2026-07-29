//! Art-Net TimeCode (`OpTimeCode`, 0x9700) packet encoding.
//!
//! <https://art-net.org.uk/how-it-works/time-code/> — a fixed 19-byte UDP
//! packet. Always sent as a limited broadcast: the socket is bound to the
//! chosen network interface's own address, which is what actually steers
//! the packet onto the right subnet — the destination address only needs to
//! mean "everyone on this wire".

use std::net::Ipv4Addr;

use tokio::net::UdpSocket;

use crate::{FrameRate, TimeCode};

/// Port every Art-Net node listens on by convention.
pub const DEFAULT_PORT: u16 = 6454;

const ART_NET_ID: &[u8; 8] = b"Art-Net\0";
const OP_TIME_CODE: u16 = 0x9700;
const PROTOCOL_VERSION: u16 = 14;

fn build_packet(timecode: TimeCode, frame_rate: FrameRate) -> [u8; 19] {
    let mut packet = [0u8; 19];
    packet[0..8].copy_from_slice(ART_NET_ID);
    // OpCode and ProtVer are transmitted low byte first.
    packet[8..10].copy_from_slice(&OP_TIME_CODE.to_le_bytes());
    packet[10..12].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    // packet[12..14] Filler1/Filler2 stay zero.
    packet[14] = timecode.frames;
    packet[15] = timecode.seconds;
    packet[16] = timecode.minutes;
    packet[17] = timecode.hours;
    packet[18] = frame_rate.art_net_type();
    packet
}

/// Sends one Art-Net TimeCode frame as a limited broadcast on `port`.
///
/// UDP is fire-and-forget: a failed send means one dropped frame on the
/// wire, not a broken session, so the caller only needs to log it.
pub(crate) async fn send(
    socket: &UdpSocket,
    port: u16,
    timecode: TimeCode,
    frame_rate: FrameRate,
) -> anyhow::Result<()> {
    let packet = build_packet(timecode, frame_rate);
    socket.send_to(&packet, (Ipv4Addr::BROADCAST, port)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_carries_the_art_net_header_and_timecode_fields() {
        let timecode = TimeCode {
            hours: 1,
            minutes: 2,
            seconds: 3,
            frames: 4,
        };
        let packet = build_packet(timecode, FrameRate::Smpte30);

        assert_eq!(&packet[0..8], ART_NET_ID);
        assert_eq!(&packet[8..10], &[0x00, 0x97], "OpTimeCode, low byte first");
        assert_eq!(&packet[10..12], &[0x00, 0x0e], "ProtVer 14, high byte first");
        assert_eq!(packet[14], 4, "frames");
        assert_eq!(packet[15], 3, "seconds");
        assert_eq!(packet[16], 2, "minutes");
        assert_eq!(packet[17], 1, "hours");
        assert_eq!(packet[18], 3, "SMPTE 30fps type");
    }
}
