//! TCP client for one player's dbserver/remotedb connection: port discovery,
//! the setup handshake, and the query methods `prolink-engine`'s metadata
//! fetcher calls. No timeout policy lives here — every method here runs
//! until it gets an answer or the socket errors; wrapping each call in a
//! bounded [`tokio::time::timeout`] is the caller's job (see
//! `prolink-engine::metadata::fetcher`), the same way `prolink` leaves
//! retry/backoff policy to its callers elsewhere.
//!
//! The port-discovery request byte layout (a 4-byte length prefix followed
//! by the literal ASCII string `"RemoteDBServer"` and a trailing NUL, 19
//! bytes total) and the setup handshake's exact shape are reconstructed
//! from Deep Symmetry's documentation and cross-referenced against
//! `beat-link`'s general message envelope, but — unlike this crate's UDP
//! parsers, which had a hardware-validated pre-Rust implementation to check
//! against — nothing in decktective's history exercised this exact TCP
//! byte sequence before. Treat it as the best current understanding, not
//! confirmed-correct until it's run against a real player.

use std::net::{Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};

use super::message::{Field, Message};
use super::queries::{self, MenuItemType};

/// TCP port every player's dbserver listens on for the initial port-lookup
/// query — the actual query/response port (commonly 1051) is learned from
/// the response to this.
pub const PORT_DISCOVERY_PORT: u16 = 12523;

/// `[0x00,0x00,0x00,0x0f] ++ b"RemoteDBServer" ++ [0x00]` — a 4-byte
/// big-endian length (15, matching "RemoteDBServer" plus a NUL) followed by
/// that many bytes. See this module's doc comment on confidence.
fn port_discovery_request() -> Vec<u8> {
    let mut request = vec![0x00, 0x00, 0x00, 0x0f];
    request.extend_from_slice(b"RemoteDBServer");
    request.push(0x00);
    request
}

/// One field of a track's metadata, as extracted from a single `MENU_ITEM`
/// message. The exact argument index carrying the text/numeric value beyond
/// `item_type` isn't confidently pinned down (see this module's doc
/// comment), so extraction takes the first field of the expected *type*
/// among the item's arguments rather than a fixed index — robust to the
/// unused argument slots being padded with a different index than assumed,
/// at the cost of being wrong if two same-typed fields both carry meaning
/// in the same item (not expected to happen based on the item types this
/// codebase reads).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackMenuItem {
    pub item_type: MenuItemType,
    pub text: Option<String>,
    pub number: Option<u32>,
}

fn extract_menu_item(message: &Message) -> Option<TrackMenuItem> {
    let item_type = MenuItemType::from_raw(message.arguments.get(6)?.as_u32()?);
    let text = message
        .arguments
        .iter()
        .find_map(Field::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    // Excludes index 0/1 (parent id, item id) and 6 (`item_type` itself,
    // already consumed above) so neither is mistaken for the item's actual
    // numeric value, and keeps scanning past zero-valued placeholder args
    // rather than stopping at the first one — `find_map` alone would stop
    // at the first *convertible* field even if its value is a meaningless
    // placeholder zero.
    let number = message
        .arguments
        .iter()
        .enumerate()
        .filter(|&(index, _)| !matches!(index, 0 | 1 | 6))
        .filter_map(|(_, field)| field.as_u32())
        .find(|&n| n != 0);
    Some(TrackMenuItem { item_type, text, number })
}

/// A connection to one player's dbserver, past the setup handshake and
/// ready for queries.
pub struct Connection {
    stream: TcpStream,
    read_buffer: Vec<u8>,
    next_transaction_id: u32,
}

impl Connection {
    /// Discovers the query port, connects, and completes the setup
    /// handshake — everything needed before [`Self::fetch_metadata`] and
    /// the asset fetches will work. `bind_ip` is the local adapter to
    /// originate from (see `prolink-engine::network`), so metadata traffic
    /// leaves via the same interface presence already picked on a
    /// multi-homed machine, not whatever the OS default route happens to
    /// be.
    pub async fn open(player_ip: Ipv4Addr, bind_ip: Ipv4Addr, our_device_number: u8) -> std::io::Result<Self> {
        let query_port = discover_query_port(player_ip, bind_ip).await?;
        Self::connect(player_ip, bind_ip, query_port, our_device_number).await
    }

    /// Connects directly to a known query port, skipping port discovery —
    /// for a caller that already knows it (or, in this crate's own tests,
    /// a fake server on an ephemeral port rather than the real, fixed
    /// discovery port, which would be unsafe to bind in a test running
    /// alongside anything else that might already own it).
    pub async fn connect(player_ip: Ipv4Addr, bind_ip: Ipv4Addr, query_port: u16, our_device_number: u8) -> std::io::Result<Self> {
        let socket = bound_tcp_socket(bind_ip)?;
        let stream = socket.connect(SocketAddr::from((player_ip, query_port))).await?;

        let mut connection = Self { stream, read_buffer: Vec::new(), next_transaction_id: 0 };
        connection
            .simple_request(queries::SETUP_REQ, vec![Field::U32(u32::from(our_device_number))])
            .await?;
        Ok(connection)
    }

    /// Fetches a track's metadata via the full menu-render exchange:
    /// request the field count, then render that many menu items. Returns
    /// every item the player sent back, un-interpreted — mapping items onto
    /// `TrackInfo` fields is `prolink-engine::metadata::normalize`'s job.
    ///
    /// `target_device_number` names the player *being asked about* (the one
    /// this connection is open to) — distinct from the device number we
    /// asserted as our own identity once, in [`Self::open`]'s
    /// `SETUP_REQ`. Every query on this connection repeats it because the
    /// menu-request protocol is designed to also work when browsing a
    /// *different* device's media through a relay — not a case this
    /// codebase uses, but the field is mandatory regardless.
    pub async fn fetch_metadata(
        &mut self,
        target_device_number: u8,
        slot: u8,
        track_type: u8,
        track_id: u32,
    ) -> std::io::Result<Vec<TrackMenuItem>> {
        let rmst = |extra: Vec<Field>| -> Vec<Field> {
            let mut args = vec![
                Field::U8(target_device_number),
                Field::U8(queries::MENU_TARGET_MAIN),
                Field::U8(slot),
                Field::U8(track_type),
            ];
            args.extend(extra);
            args
        };

        let available = self
            .simple_request(queries::METADATA_REQ, rmst(vec![Field::U32(track_id)]))
            .await?;
        let item_count = available.arguments.iter().find_map(Field::as_u32).unwrap_or(0);
        if item_count == 0 {
            return Ok(Vec::new());
        }

        let render_args = rmst(vec![
            Field::U32(0),          // offset
            Field::U32(item_count), // limit
            Field::U32(0),
            Field::U32(item_count), // total
            Field::U32(0),
        ]);
        self.send(queries::RENDER_MENU_REQ, render_args).await?;

        let header = self.read_message().await?;
        if header.message_type != queries::MENU_HEADER {
            return Err(unexpected_message("a menu header", header.message_type));
        }

        let mut items = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            let item = self.read_message().await?;
            if item.message_type != queries::MENU_ITEM {
                return Err(unexpected_message("a menu item", item.message_type));
            }
            if let Some(parsed) = extract_menu_item(&item) {
                items.push(parsed);
            }
        }

        let footer = self.read_message().await?;
        if footer.message_type != queries::MENU_FOOTER {
            return Err(unexpected_message("a menu footer", footer.message_type));
        }

        Ok(items)
    }

    /// Fetches one raw blob response (beat grid, cue list, or the generic
    /// ANLZ-tag passthrough used for waveforms) — every asset query beyond
    /// metadata shares this same "one request, one blob-bearing response"
    /// shape. `None` means the player answered but had nothing to offer
    /// (the documented "if the length field is 0, there is no blob" case),
    /// not a failure.
    async fn fetch_blob(&mut self, query: u16, args: Vec<Field>) -> std::io::Result<Option<Vec<u8>>> {
        let response = self.simple_request(query, args).await?;
        Ok(response.arguments.iter().find_map(Field::as_blob).map(<[u8]>::to_vec))
    }

    pub async fn fetch_beat_grid(&mut self, target_device_number: u8, slot: u8, track_type: u8, track_id: u32) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::BEAT_GRID_REQ,
            vec![Field::U8(target_device_number), Field::U8(queries::MENU_TARGET_MAIN), Field::U8(slot), Field::U8(track_type), Field::U32(track_id)],
        )
        .await
    }

    /// Tries the extended cue list first (color, comment, hot loops); the
    /// caller falls back to [`Self::fetch_cues_basic`] only if this comes
    /// back empty/unavailable.
    pub async fn fetch_cues_extended(&mut self, target_device_number: u8, slot: u8, track_type: u8, track_id: u32) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::CUE_LIST_EXT_REQ,
            vec![Field::U8(target_device_number), Field::U8(queries::MENU_TARGET_MAIN), Field::U8(slot), Field::U8(track_type), Field::U32(track_id)],
        )
        .await
    }

    pub async fn fetch_cues_basic(&mut self, target_device_number: u8, slot: u8, track_type: u8, track_id: u32) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::CUE_LIST_REQ,
            vec![Field::U8(target_device_number), Field::U8(queries::MENU_TARGET_MAIN), Field::U8(slot), Field::U8(track_type), Field::U32(track_id)],
        )
        .await
    }

    /// `tag_fourcc` is the 4-byte ANLZ tag magic to request, byte-reversed
    /// for the wire (see `dbserver::waveform`'s doc comment for the
    /// concrete value this codebase asks for).
    pub async fn fetch_anlz_tag(&mut self, target_device_number: u8, slot: u8, track_type: u8, track_id: u32, tag_fourcc: u32, file_extension: &str) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::ANLZ_TAG_REQ,
            vec![
                Field::U8(target_device_number),
                Field::U8(queries::MENU_TARGET_MAIN),
                Field::U8(slot),
                Field::U8(track_type),
                Field::U32(track_id),
                Field::U32(tag_fourcc),
                Field::Str(file_extension.to_string()),
            ],
        )
        .await
    }

    pub async fn fetch_artwork(&mut self, target_device_number: u8, slot: u8, artwork_id: u32) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::ARTWORK_REQ,
            vec![Field::U8(target_device_number), Field::U8(queries::MENU_TARGET_MAIN), Field::U8(slot), Field::U32(artwork_id)],
        )
        .await
    }

    async fn simple_request(&mut self, message_type: u16, args: Vec<Field>) -> std::io::Result<Message> {
        self.send(message_type, args).await?;
        self.read_message().await
    }

    async fn send(&mut self, message_type: u16, args: Vec<Field>) -> std::io::Result<()> {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
        let bytes = Message::new(transaction_id, message_type, args).encode();
        self.stream.write_all(&bytes).await
    }

    /// Reads one complete message, accumulating across as many socket reads
    /// as it takes — a menu-item burst can arrive as several messages in a
    /// single TCP segment, or one message can be split across several
    /// reads; this handles both without assuming either. Leftover bytes
    /// past the message just decoded stay in `read_buffer` for the next
    /// call.
    async fn read_message(&mut self) -> std::io::Result<Message> {
        loop {
            if let Some((message, consumed)) = Message::decode(&self.read_buffer) {
                self.read_buffer.drain(..consumed);
                return Ok(message);
            }
            let mut chunk = [0u8; 4096];
            let read = self.stream.read(&mut chunk).await?;
            if read == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "dbserver closed the connection"));
            }
            self.read_buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

fn unexpected_message(expected: &str, got_type: u16) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("expected {expected}, got message type {got_type:#06x}"))
}

fn bound_tcp_socket(bind_ip: Ipv4Addr) -> std::io::Result<TcpSocket> {
    let socket = TcpSocket::new_v4()?;
    socket.bind(SocketAddr::from((bind_ip, 0)))?;
    Ok(socket)
}

async fn discover_query_port(player_ip: Ipv4Addr, bind_ip: Ipv4Addr) -> std::io::Result<u16> {
    let socket = bound_tcp_socket(bind_ip)?;
    let mut stream = socket.connect(SocketAddr::from((player_ip, PORT_DISCOVERY_PORT))).await?;
    stream.write_all(&port_discovery_request()).await?;

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await?;
    Ok(u16::from_be_bytes(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_discovery_request_is_the_length_prefixed_remotedbserver_string() {
        let request = port_discovery_request();
        assert_eq!(request.len(), 19);
        assert_eq!(&request[..4], &[0x00, 0x00, 0x00, 0x0f]);
        assert_eq!(&request[4..18], b"RemoteDBServer");
        assert_eq!(request[18], 0x00);
    }

    #[test]
    fn extracts_a_string_field_from_a_menu_item() {
        let message = Message::new(
            1,
            queries::MENU_ITEM,
            vec![
                Field::U32(0), // parent id
                Field::U32(1), // item id
                Field::U32(0),
                Field::Str("Test Track".to_string()),
                Field::U32(0),
                Field::Str(String::new()),
                Field::U32(0x0004), // item_type: TrackTitle
                Field::U32(0),
                Field::U32(0),
                Field::U32(0),
            ],
        );
        let item = extract_menu_item(&message).expect("should extract");
        assert_eq!(item.item_type, MenuItemType::TrackTitle);
        assert_eq!(item.text.as_deref(), Some("Test Track"));
    }

    #[test]
    fn extracts_a_numeric_field_from_a_menu_item() {
        let message = Message::new(
            1,
            queries::MENU_ITEM,
            vec![
                Field::U32(0),
                Field::U32(1),
                Field::U32(0),
                Field::Str(String::new()),
                Field::U32(0),
                Field::Str(String::new()),
                Field::U32(0x000d), // item_type: Tempo
                Field::U32(12800),  // 128.00 BPM
                Field::U32(0),
                Field::U32(0),
            ],
        );
        let item = extract_menu_item(&message).expect("should extract");
        assert_eq!(item.item_type, MenuItemType::Tempo);
        assert_eq!(item.number, Some(12800));
    }

    #[test]
    fn a_menu_item_missing_the_item_type_argument_is_not_extracted() {
        let message = Message::new(1, queries::MENU_ITEM, vec![Field::U32(0), Field::U32(1)]);
        assert!(extract_menu_item(&message).is_none());
    }
}
