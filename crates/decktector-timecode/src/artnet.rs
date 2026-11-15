//! Art-Net TimeCode (`OpTimeCode`, 0x9700) packet encoding.
//!
//! <https://art-net.org.uk/how-it-works/time-code/> — a fixed 19-byte UDP
//! packet, broadcast to the chosen network interface's own subnet (e.g.
//! `2.255.255.255`), the directed broadcast the Art-Net spec asks for. The
//! limited broadcast `255.255.255.255` is not steered by the socket's bound
//! address on every OS, so it could reach the wrong network — see
//! `decktector_net::Ipv4Subnet::broadcast`.

use std::net::SocketAddrV4;

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

/// Sends one Art-Net TimeCode frame to `destination` — the subnet broadcast
/// address in practice, though a unicast node address works the same.
///
/// UDP is fire-and-forget: a failed send means one dropped frame on the
/// wire, not a broken session, so the caller only needs to log it.
pub(crate) async fn send(
    socket: &UdpSocket,
    destination: SocketAddrV4,
    timecode: TimeCode,
    frame_rate: FrameRate,
) -> anyhow::Result<()> {
    let packet = build_packet(timecode, frame_rate);
    socket.send_to(&packet, destination).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[tokio::test]
    async fn send_delivers_the_packet_to_the_given_destination() {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let destination =
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, receiver.local_addr().unwrap().port());
        let timecode = TimeCode {
            hours: 1,
            minutes: 2,
            seconds: 3,
            frames: 4,
        };

        send(&sender, destination, timecode, FrameRate::Ebu25)
            .await
            .unwrap();

        let mut received = [0u8; 64];
        let (length, _) = receiver.recv_from(&mut received).await.unwrap();
        assert_eq!(
            &received[..length],
            &build_packet(timecode, FrameRate::Ebu25)
        );
    }

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
        assert_eq!(
            &packet[10..12],
            &[0x00, 0x0e],
            "ProtVer 14, high byte first"
        );
        assert_eq!(packet[14], 4, "frames");
        assert_eq!(packet[15], 3, "seconds");
        assert_eq!(packet[16], 2, "minutes");
        assert_eq!(packet[17], 1, "hours");
        assert_eq!(packet[18], 3, "SMPTE 30fps type");
    }
}
