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
//! against — nothing in decktector's history exercised this exact TCP
//! byte sequence before. Treat it as the best current understanding, not
//! confirmed-correct until it's run against a real player.

use std::net::{Ipv4Addr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};

use super::message::{Field, Message};
use super::queries::{self, MenuItemType};

/// How long one message may take to arrive before the connection is
/// treated as dead. Generous next to a player's sub-second replies, short
/// enough that a booth operator doesn't think the tool has crashed.
const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The bare number field exchanged in both directions before a connection
/// will accept any message — see [`Connection::greet`].
const GREETING: u32 = 1;

/// Transaction id the setup message travels under. Real clients use this
/// fixed sentinel rather than starting the ordinary counter at zero, and it
/// is what the CDJ-3000 echoed back in the reference exchange.
const SETUP_TRANSACTION_ID: u32 = 0xffff_fffe;

/// The single packed argument every menu and asset query opens with: the
/// player being asked about, which menu it should render into, which media
/// slot to read, and the track's type, one byte each in a 4-byte number
/// field.
///
/// Not four separate byte arguments, which is what this client sent before
/// 2026-08-19 — a CDJ-3000 answers a request shaped that way with a menu of
/// zero items, if it answers at all.
fn rmst_argument(target_device_number: u8, slot: u8, track_type: u8) -> u32 {
    u32::from_be_bytes([
        target_device_number,
        queries::MENU_TARGET_MAIN,
        slot,
        track_type,
    ])
}

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
/// message.
///
/// The argument layout is fixed, and was read off a CDJ-3000 on 2026-08-19
/// (`capture/reference-20260819/`):
///
/// ```text
/// 0  parent id                 4  length of the second label
/// 1  the item's numeric value  5  second label (always empty here)
/// 2  length of the first label 6  item type
/// 3  the item's text           7  colour/flags
/// ```
///
/// Indices 1 and 3, then. This used to scan for "the first field of the
/// right type", which reliably picked up index 2 — so every numeric field
/// came back as the *length of an empty label* (2) instead of its value: a
/// 180-second track reported a duration of 2, and an 82.50 BPM track a
/// tempo of 2.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackMenuItem {
    pub item_type: MenuItemType,
    pub text: Option<String>,
    pub number: Option<u32>,
}

const MENU_ITEM_VALUE_INDEX: usize = 1;
const MENU_ITEM_TEXT_INDEX: usize = 3;
const MENU_ITEM_TYPE_INDEX: usize = 6;

fn extract_menu_item(message: &Message) -> Option<TrackMenuItem> {
    let item_type = MenuItemType::from_raw(message.arguments.get(MENU_ITEM_TYPE_INDEX)?.as_u32()?);
    let text = message
        .arguments
        .get(MENU_ITEM_TEXT_INDEX)
        .and_then(Field::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let number = message
        .arguments
        .get(MENU_ITEM_VALUE_INDEX)
        .and_then(Field::as_u32);
    Some(TrackMenuItem {
        item_type,
        text,
        number,
    })
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
    pub async fn open(
        player_ip: Ipv4Addr,
        bind_ip: Ipv4Addr,
        our_device_number: u8,
    ) -> std::io::Result<Self> {
        let query_port = discover_query_port(player_ip, bind_ip).await?;
        Self::connect(player_ip, bind_ip, query_port, our_device_number).await
    }

    /// Connects directly to a known query port, skipping port discovery —
    /// for a caller that already knows it (or, in this crate's own tests,
    /// a fake server on an ephemeral port rather than the real, fixed
    /// discovery port, which would be unsafe to bind in a test running
    /// alongside anything else that might already own it).
    pub async fn connect(
        player_ip: Ipv4Addr,
        bind_ip: Ipv4Addr,
        query_port: u16,
        our_device_number: u8,
    ) -> std::io::Result<Self> {
        let socket = bound_tcp_socket(bind_ip)?;
        let stream = socket
            .connect(SocketAddr::from((player_ip, query_port)))
            .await?;

        let mut connection = Self {
            stream,
            read_buffer: Vec::new(),
            next_transaction_id: 1,
        };
        connection.greet().await?;
        connection
            .request(
                SETUP_TRANSACTION_ID,
                queries::SETUP_REQ,
                vec![Field::U32(u32::from(our_device_number))],
            )
            .await?;
        Ok(connection)
    }

    /// The bare 4-byte number field a player expects as the very first
    /// thing on a new connection, before any message: it echoes the same
    /// field straight back, and only then will it read a message. Skipping
    /// it is not a protocol nicety — a CDJ-3000 closes the connection
    /// without a word (`capture/reference-20260819/`).
    async fn greet(&mut self) -> std::io::Result<()> {
        let mut greeting = Vec::new();
        Field::U32(GREETING).encode_field(&mut greeting);
        self.stream.write_all(&greeting).await?;

        let mut echoed = [0u8; 5];
        self.stream.read_exact(&mut echoed).await?;
        match Field::decode_field(&echoed).map(|(field, _)| field.as_u32()) {
            Some(Some(GREETING)) => Ok(()),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "dbserver did not echo the connection greeting",
            )),
        }
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
            let mut args = vec![Field::U32(rmst_argument(
                target_device_number,
                slot,
                track_type,
            ))];
            args.extend(extra);
            args
        };

        let available = self
            .simple_request(queries::METADATA_REQ, rmst(vec![Field::U32(track_id)]))
            .await?;
        // The first argument echoes the request type; the item count is the
        // second. `find_map` would stop at the echo.
        let item_count = available
            .arguments
            .get(1)
            .and_then(Field::as_u32)
            .unwrap_or(0);
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
    async fn fetch_blob(
        &mut self,
        query: u16,
        args: Vec<Field>,
    ) -> std::io::Result<Option<Vec<u8>>> {
        let response = self.simple_request(query, args).await?;
        Ok(response
            .arguments
            .iter()
            .find_map(Field::as_blob)
            .map(<[u8]>::to_vec))
    }

    pub async fn fetch_beat_grid(
        &mut self,
        target_device_number: u8,
        slot: u8,
        track_type: u8,
        track_id: u32,
    ) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::BEAT_GRID_REQ,
            vec![
                Field::U32(rmst_argument(target_device_number, slot, track_type)),
                Field::U32(track_id),
            ],
        )
        .await
    }

    /// Tries the extended cue list first (color, comment, hot loops); the
    /// caller falls back to [`Self::fetch_cues_basic`] only if this comes
    /// back empty/unavailable.
    pub async fn fetch_cues_extended(
        &mut self,
        target_device_number: u8,
        slot: u8,
        track_type: u8,
        track_id: u32,
    ) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::CUE_LIST_EXT_REQ,
            vec![
                Field::U32(rmst_argument(target_device_number, slot, track_type)),
                Field::U32(track_id),
            ],
        )
        .await
    }

    pub async fn fetch_cues_basic(
        &mut self,
        target_device_number: u8,
        slot: u8,
        track_type: u8,
        track_id: u32,
    ) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::CUE_LIST_REQ,
            vec![
                Field::U32(rmst_argument(target_device_number, slot, track_type)),
                Field::U32(track_id),
            ],
        )
        .await
    }

    /// `tag_fourcc` is the 4-byte ANLZ tag magic to request, byte-reversed
    /// for the wire (see `dbserver::waveform`'s doc comment for the
    /// concrete value this codebase asks for).
    pub async fn fetch_anlz_tag(
        &mut self,
        target_device_number: u8,
        slot: u8,
        track_type: u8,
        track_id: u32,
        tag_fourcc: u32,
        file_extension: &str,
    ) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::ANLZ_TAG_REQ,
            vec![
                Field::U32(rmst_argument(target_device_number, slot, track_type)),
                Field::U32(track_id),
                Field::U32(tag_fourcc),
                Field::Str(file_extension.to_string()),
            ],
        )
        .await
    }

    pub async fn fetch_artwork(
        &mut self,
        target_device_number: u8,
        slot: u8,
        track_type: u8,
        artwork_id: u32,
    ) -> std::io::Result<Option<Vec<u8>>> {
        self.fetch_blob(
            queries::ARTWORK_REQ,
            vec![
                Field::U32(rmst_argument(target_device_number, slot, track_type)),
                Field::U32(artwork_id),
            ],
        )
        .await
    }

    async fn simple_request(
        &mut self,
        message_type: u16,
        args: Vec<Field>,
    ) -> std::io::Result<Message> {
        let transaction_id = self.take_transaction_id();
        self.request(transaction_id, message_type, args).await
    }

    async fn request(
        &mut self,
        transaction_id: u32,
        message_type: u16,
        args: Vec<Field>,
    ) -> std::io::Result<Message> {
        let bytes = Message::new(transaction_id, message_type, args).encode();
        self.stream.write_all(&bytes).await?;
        self.read_message().await
    }

    async fn send(&mut self, message_type: u16, args: Vec<Field>) -> std::io::Result<()> {
        let transaction_id = self.take_transaction_id();
        let bytes = Message::new(transaction_id, message_type, args).encode();
        self.stream.write_all(&bytes).await
    }

    fn take_transaction_id(&mut self) -> u32 {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
        transaction_id
    }

    /// Reads one complete message, accumulating across as many socket reads
    /// as it takes — a menu-item burst can arrive as several messages in a
    /// single TCP segment, or one message can be split across several
    /// reads; this handles both without assuming either. Leftover bytes
    /// past the message just decoded stay in `read_buffer` for the next
    /// call.
    ///
    /// Bounded by [`READ_TIMEOUT`], because a player that simply never
    /// answers a query is a real, observed condition (an artwork id it
    /// doesn't recognize, for one) and
    /// [`Message::decode`](super::message::Message::decode) can't tell
    /// "not a message" from "not the whole message yet" — without a
    /// deadline, one unanswered query hangs the caller forever, which for
    /// the capture tool means a session that never reaches its own
    /// shutdown.
    async fn read_message(&mut self) -> std::io::Result<Message> {
        loop {
            if let Some((message, consumed)) = Message::decode(&self.read_buffer) {
                self.read_buffer.drain(..consumed);
                return Ok(message);
            }
            let mut chunk = [0u8; 4096];
            let read = match tokio::time::timeout(READ_TIMEOUT, self.stream.read(&mut chunk)).await
            {
                Ok(result) => result?,
                Err(_) => {
                    // Quiet long enough that "still arriving" is no longer
                    // a plausible reading of a short message — so a
                    // player's deliberately truncated no-data reply gets
                    // reported as the empty answer it is, rather than as a
                    // failure (see `Message::decode_lenient`).
                    if let Some((message, consumed)) = Message::decode_lenient(&self.read_buffer) {
                        self.read_buffer.drain(..consumed);
                        return Ok(message);
                    }
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!(
                            "dbserver went quiet with {} undecodable byte(s) buffered",
                            self.read_buffer.len()
                        ),
                    ));
                }
            };
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "dbserver closed the connection",
                ));
            }
            self.read_buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

fn unexpected_message(expected: &str, got_type: u16) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("expected {expected}, got message type {got_type:#06x}"),
    )
}

fn bound_tcp_socket(bind_ip: Ipv4Addr) -> std::io::Result<TcpSocket> {
    let socket = TcpSocket::new_v4()?;
    socket.bind(SocketAddr::from((bind_ip, 0)))?;
    Ok(socket)
}

async fn discover_query_port(player_ip: Ipv4Addr, bind_ip: Ipv4Addr) -> std::io::Result<u16> {
    let socket = bound_tcp_socket(bind_ip)?;
    let mut stream = socket
        .connect(SocketAddr::from((player_ip, PORT_DISCOVERY_PORT)))
        .await?;
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
                Field::U32(1),  // parent id
                Field::U32(2),  // the item's value
                Field::U32(34), // length of the label below
                Field::Str("Test Track".to_string()),
                Field::U32(2),
                Field::Str(String::new()),
                Field::U32(0x0004), // item_type: TrackTitle
                Field::U32(0x0100_0000),
            ],
        );
        let item = extract_menu_item(&message).expect("should extract");
        assert_eq!(item.item_type, MenuItemType::TrackTitle);
        assert_eq!(item.text.as_deref(), Some("Test Track"));
    }

    /// Arguments copied from a real CDJ-3000 menu item (the tempo entry of
    /// track 2 on the 2026-08-19 rig): the value sits at index 1, and index
    /// 2 is the length of the empty label that follows — the field the old
    /// "first numeric argument" scan used to return instead.
    #[test]
    fn extracts_a_numeric_field_from_a_menu_item() {
        let message = Message::new(
            1,
            queries::MENU_ITEM,
            vec![
                Field::U32(0),
                Field::U32(8250), // 82.50 BPM
                Field::U32(2),
                Field::Str(String::new()),
                Field::U32(2),
                Field::Str(String::new()),
                Field::U32(0x000d), // item_type: Tempo
                Field::U32(0),
            ],
        );
        let item = extract_menu_item(&message).expect("should extract");
        assert_eq!(item.item_type, MenuItemType::Tempo);
        assert_eq!(item.number, Some(8250));
        assert_eq!(item.text, None);
    }

    #[test]
    fn a_menu_item_missing_the_item_type_argument_is_not_extracted() {
        let message = Message::new(1, queries::MENU_ITEM, vec![Field::U32(0), Field::U32(1)]);
        assert!(extract_menu_item(&message).is_none());
    }
}
