//! Wire format for the dbserver/remotedb protocol: a typed-argument message
//! envelope carried over TCP, once port discovery and the setup handshake
//! (see [`super::client`]) have opened a connection.
//!
//! Every byte here is verified against a CDJ-3000 (`capture/
//! reference-20260819/dbserver-probe/`, 2026-08-19): the envelope was
//! previously written from a reading of `beat-link` that got the framing
//! wrong — header components were emitted as bare big-endian integers and
//! arguments as bare values — and a real player answered by closing the
//! connection without a word.
//!
//! What it actually wants: **every** component on the wire is a
//! self-describing field, tag byte included. A message is the field
//! sequence
//!
//! ```text
//! 11 872349ae            magic, as a 4-byte number field
//! 11 <transaction id>    4-byte number field
//! 10 <message type>      2-byte number field
//! 0f <argument count>    1-byte number field
//! 14 <len> <tags…>       blob field: one ARGUMENT-type code per argument
//! <argument fields…>     each with its own type tag, as above
//! ```
//!
//! and the codes in that blob are a *different* enumeration from the field
//! type tags — see [`Field::argument_type`]. Message boundaries are still
//! treated as unknown (a player can pack several messages into one segment
//! or split one across reads), which is why
//! [`super::client::Connection::read_message`] accumulates and re-parses.

/// Every message starts with this 4-byte value.
pub const MAGIC: u32 = 0x872349ae;

/// Smallest possible header: the magic, transaction id, message type and
/// argument count fields (tags included) plus an empty tag blob. Only used
/// to reject obviously-too-short buffers before parsing field by field.
const MIN_HEADER_LEN: usize = 5 + 5 + 3 + 2 + 5;
/// A message declares this many argument-type codes up front, even if it
/// carries fewer — the length real players send back varies (they send
/// exactly as many as they have arguments), but a full block of 12 is what
/// this codebase transmits, which a CDJ-3000 accepts.
const MAX_ARGS: usize = 12;

/// One typed argument value. The wire only ever carries these five shapes.
#[derive(Debug, Clone, PartialEq)]
pub enum Field {
    U8(u8),
    U16(u16),
    U32(u32),
    /// Length-prefixed (4-byte big-endian length) raw bytes.
    Blob(Vec<u8>),
    /// UTF-16BE, length-prefixed in *characters* (not bytes). A trailing
    /// NUL, if present, is stripped on decode rather than kept as a `\0` in
    /// the Rust `String` — real responses are believed to null-terminate
    /// text fields, but this hasn't been confirmed against real hardware,
    /// so encoding does not add one back on the way out.
    Str(String),
}

impl Field {
    fn type_tag(&self) -> u8 {
        match self {
            Self::U8(_) => 0x0f,
            Self::U16(_) => 0x10,
            Self::U32(_) => 0x11,
            Self::Blob(_) => 0x14,
            Self::Str(_) => 0x26,
        }
    }

    /// This field's code in a message's argument-type blob — a separate,
    /// much coarser enumeration than [`Self::type_tag`]: the blob only says
    /// *number*, *binary* or *string*, while the tag byte on the value
    /// itself says how wide the number is. Mixing the two up is what made
    /// the first hardware attempt fail.
    fn argument_type(&self) -> u8 {
        match self {
            Self::U8(_) | Self::U16(_) | Self::U32(_) => 0x06,
            Self::Blob(_) => 0x03,
            Self::Str(_) => 0x02,
        }
    }

    /// Writes this field the way it travels: type tag, then value. Public
    /// because a connection's opening greeting (see
    /// [`super::client::Connection`]) is a bare field, not a message.
    pub fn encode_field(&self, out: &mut Vec<u8>) {
        out.push(self.type_tag());
        self.encode_value(out);
    }

    /// Encodes just the value bytes, without the leading type tag.
    fn encode_value(&self, out: &mut Vec<u8>) {
        match self {
            Self::U8(value) => out.push(*value),
            Self::U16(value) => out.extend_from_slice(&value.to_be_bytes()),
            Self::U32(value) => out.extend_from_slice(&value.to_be_bytes()),
            Self::Blob(bytes) => {
                out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                out.extend_from_slice(bytes);
            }
            Self::Str(text) => {
                let units: Vec<u16> = text.encode_utf16().collect();
                out.extend_from_slice(&(units.len() as u32).to_be_bytes());
                for unit in units {
                    out.extend_from_slice(&unit.to_be_bytes());
                }
            }
        }
    }

    /// Decodes one complete field — tag byte and value — from the start of
    /// `data`; `None` if it isn't a known tag or `data` is still short.
    pub fn decode_field(data: &[u8]) -> Option<(Self, usize)> {
        let (&tag, rest) = data.split_first()?;
        let (field, consumed) = Self::decode_value(tag, rest)?;
        Some((field, consumed + 1))
    }

    /// Decodes one value of the given type from the start of `data`;
    /// `None` if `data` doesn't yet hold enough bytes (the caller should
    /// read more and retry) — never panics on short input. Returns the
    /// field and how many bytes it consumed.
    fn decode_value(tag: u8, data: &[u8]) -> Option<(Self, usize)> {
        match tag {
            0x0f => data.first().map(|&byte| (Self::U8(byte), 1)),
            0x10 => {
                (data.len() >= 2).then(|| (Self::U16(u16::from_be_bytes([data[0], data[1]])), 2))
            }
            0x11 => (data.len() >= 4).then(|| {
                (
                    Self::U32(u32::from_be_bytes([data[0], data[1], data[2], data[3]])),
                    4,
                )
            }),
            0x14 => {
                if data.len() < 4 {
                    return None;
                }
                let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
                let total = 4 + len;
                (data.len() >= total).then(|| (Self::Blob(data[4..total].to_vec()), total))
            }
            0x26 => {
                if data.len() < 4 {
                    return None;
                }
                let char_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
                let total = 4 + char_len * 2;
                if data.len() < total {
                    return None;
                }
                let mut units: Vec<u16> = (0..char_len)
                    .map(|i| {
                        let offset = 4 + i * 2;
                        u16::from_be_bytes([data[offset], data[offset + 1]])
                    })
                    .collect();
                while units.last() == Some(&0) {
                    units.pop();
                }
                Some((Self::Str(String::from_utf16_lossy(&units)), total))
            }
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::U8(value) => Some(u32::from(*value)),
            Self::U16(value) => Some(u32::from(*value)),
            Self::U32(value) => Some(*value),
            Self::Blob(_) | Self::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Self::Blob(bytes) => Some(bytes),
            _ => None,
        }
    }
}

/// One request or response on a dbserver connection.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub transaction_id: u32,
    pub message_type: u16,
    pub arguments: Vec<Field>,
}

impl Message {
    pub fn new(transaction_id: u32, message_type: u16, arguments: Vec<Field>) -> Self {
        Self {
            transaction_id,
            message_type,
            arguments,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(MIN_HEADER_LEN + MAX_ARGS + self.arguments.len() * 5);
        Field::U32(MAGIC).encode_field(&mut out);
        Field::U32(self.transaction_id).encode_field(&mut out);
        Field::U16(self.message_type).encode_field(&mut out);
        Field::U8(self.arguments.len() as u8).encode_field(&mut out);

        let mut tags = [0u8; MAX_ARGS];
        for (slot, argument) in tags.iter_mut().zip(&self.arguments) {
            *slot = argument.argument_type();
        }
        Field::Blob(tags.to_vec()).encode_field(&mut out);

        for argument in &self.arguments {
            argument.encode_field(&mut out);
        }
        out
    }

    /// Decodes one message from the start of `data`. `None` means either the
    /// header doesn't match (foreign/desynced stream — a real protocol
    /// error, not just "not enough data") or there isn't yet enough data
    /// for a complete message; the caller (see
    /// [`super::client::Connection::read_message`]) can't tell those apart
    /// from this alone and treats both as "keep reading." Returns the
    /// message and how many bytes of `data` it consumed, so a caller
    /// reading from a stream can leave any trailing bytes (the start of the
    /// next message) in its buffer.
    pub fn decode(data: &[u8]) -> Option<(Self, usize)> {
        Self::decode_inner(data, false)
    }

    /// [`Self::decode`], but accepting a message that stops short of the
    /// argument count its own header declares.
    ///
    /// Real players do this. A CDJ-3000 asked for the beat grid of a track
    /// it has no analysis for answers `4602` with a header declaring five
    /// arguments, sends four (the fourth a zero *number* where the header's
    /// tag blob promised a blob), and then goes silent
    /// (`capture/reference-20260819/`). Strictly, that message never ends.
    ///
    /// Only for use once the connection has gone quiet — see
    /// [`super::client::Connection::read_message`]. On a live stream a
    /// short read is far more likely to be a message still arriving, and
    /// accepting it early would desynchronize everything after it.
    pub fn decode_lenient(data: &[u8]) -> Option<(Self, usize)> {
        Self::decode_inner(data, true)
    }

    fn decode_inner(data: &[u8], allow_missing_arguments: bool) -> Option<(Self, usize)> {
        if data.len() < MIN_HEADER_LEN {
            return None;
        }

        let mut offset = 0;
        let next = |offset: &mut usize| -> Option<Field> {
            let (field, consumed) = Field::decode_field(data.get(*offset..)?)?;
            *offset += consumed;
            Some(field)
        };

        if next(&mut offset)?.as_u32()? != MAGIC {
            return None;
        }
        let transaction_id = next(&mut offset)?.as_u32()?;
        let message_type = u16::try_from(next(&mut offset)?.as_u32()?).ok()?;
        let arg_count = next(&mut offset)?.as_u32()? as usize;
        if arg_count > MAX_ARGS {
            return None;
        }
        // The tag blob is read for its length only: each argument's own tag
        // byte says how to decode it, and a player that disagrees with its
        // own blob would still be decoded correctly this way.
        next(&mut offset)?.as_blob()?;

        let mut arguments = Vec::with_capacity(arg_count);
        for _ in 0..arg_count {
            match next(&mut offset) {
                Some(argument) => arguments.push(argument),
                None if allow_missing_arguments => break,
                None => return None,
            }
        }

        Some((
            Self {
                transaction_id,
                message_type,
                arguments,
            },
            offset,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_with_integer_arguments_round_trips() {
        let message = Message::new(7, 0x2002, vec![Field::U8(2), Field::U32(4207)]);
        let bytes = message.encode();
        let (decoded, consumed) = Message::decode(&bytes).expect("should decode");
        assert_eq!(decoded, message);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn a_message_with_a_blob_argument_round_trips() {
        let message = Message::new(1, 0x4f02, vec![Field::Blob(vec![1, 2, 3, 4, 5])]);
        let bytes = message.encode();
        let (decoded, _) = Message::decode(&bytes).expect("should decode");
        assert_eq!(decoded.arguments[0].as_blob(), Some(&[1, 2, 3, 4, 5][..]));
    }

    #[test]
    fn a_message_with_a_string_argument_round_trips() {
        let message = Message::new(1, 0x4101, vec![Field::Str("Test Track".to_string())]);
        let bytes = message.encode();
        let (decoded, _) = Message::decode(&bytes).expect("should decode");
        assert_eq!(decoded.arguments[0].as_str(), Some("Test Track"));
    }

    #[test]
    fn a_trailing_null_terminator_is_stripped_from_a_decoded_string() {
        let mut bytes = Vec::new();
        Field::U32(MAGIC).encode_field(&mut bytes);
        Field::U32(1).encode_field(&mut bytes);
        Field::U16(0x4101).encode_field(&mut bytes);
        Field::U8(1).encode_field(&mut bytes);
        let mut tags = [0u8; MAX_ARGS];
        tags[0] = 0x02; // One string argument.
        Field::Blob(tags.to_vec()).encode_field(&mut bytes);
        // Hand-built rather than encoded: the point is a NUL a *player*
        // sends, which this crate never writes itself.
        bytes.push(0x26);
        bytes.extend_from_slice(&3u32.to_be_bytes()); // 'H', 'i', NUL.
        for unit in ['H' as u16, 'i' as u16, 0] {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }

        let (decoded, _) = Message::decode(&bytes).expect("should decode");
        assert_eq!(decoded.arguments[0].as_str(), Some("Hi"));
    }

    #[test]
    fn decode_reports_none_on_a_truncated_message() {
        let message = Message::new(1, 0x2002, vec![Field::Blob(vec![1, 2, 3])]);
        let bytes = message.encode();
        assert!(Message::decode(&bytes[..bytes.len() - 1]).is_none());
    }

    #[test]
    fn decode_reports_none_on_a_foreign_magic() {
        let message = Message::new(1, 0x2002, vec![Field::U8(1)]);
        let mut bytes = message.encode();
        bytes[1] = 0x00; // First byte of the magic's value, past its type tag.
        assert!(Message::decode(&bytes).is_none());
    }

    #[test]
    fn decode_reports_none_on_an_implausible_argument_count() {
        let mut bytes = Vec::new();
        Field::U32(MAGIC).encode_field(&mut bytes);
        Field::U32(1).encode_field(&mut bytes);
        Field::U16(0x2002).encode_field(&mut bytes);
        Field::U8(200).encode_field(&mut bytes); // Far more than MAX_ARGS — a desynced stream.
        Field::Blob(vec![0x06; 12]).encode_field(&mut bytes);
        assert!(Message::decode(&bytes).is_none());
    }

    /// The other half of the hardware ground truth: what a CDJ-3000 sends
    /// when asked for the beat grid of a track it has no analysis for. The
    /// header promises five arguments, four arrive, and the player then
    /// says nothing more — so the strict decoder must refuse it (it cannot
    /// know more bytes aren't coming) and the lenient one must accept it.
    #[test]
    fn a_players_short_no_data_response_decodes_only_leniently() {
        let reply = "11872349ae11000000031046020f05140000000506060603061100002204110000003211000000001100000000";
        let bytes: Vec<u8> = (0..reply.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&reply[i..i + 2], 16).expect("valid hex"))
            .collect();

        assert!(Message::decode(&bytes).is_none());

        let (message, consumed) = Message::decode_lenient(&bytes).expect("should decode leniently");
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            message.message_type,
            super::super::queries::BEAT_GRID_RESPONSE
        );
        // Four of the five: the request echo, a non-zero status code, and
        // two zeros where a real answer carries a length and its blob.
        assert_eq!(
            message.arguments,
            vec![
                Field::U32(0x2204),
                Field::U32(50),
                Field::U32(0),
                Field::U32(0)
            ]
        );
        assert!(
            message
                .arguments
                .iter()
                .all(|argument| argument.as_blob().is_none()),
            "no blob means no data, which is what the caller reports"
        );
    }

    /// The exact bytes a CDJ-3000 accepted as a connection's setup message
    /// on 2026-08-19, and the exact bytes it answered with (`capture/
    /// reference-20260819/dbserver-probe/`). This is the framing's ground
    /// truth: the previous encoding round-tripped through these same tests
    /// happily and was rejected by the hardware without a reply.
    #[test]
    fn the_setup_exchange_matches_the_bytes_a_cdj_3000_accepted() {
        let sent = Message::new(
            0xffff_fffe,
            super::super::queries::SETUP_REQ,
            vec![Field::U32(5)],
        );
        let hex: String = sent
            .encode()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            hex,
            "11872349ae11fffffffe1000000f01140000000c0600000000000000000000001100000005"
        );

        let reply = "11872349ae11fffffffe1040000f021400000002060611000000001100000003";
        let bytes: Vec<u8> = (0..reply.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&reply[i..i + 2], 16).expect("valid hex"))
            .collect();
        let (message, consumed) =
            Message::decode(&bytes).expect("the player's reply should decode");
        assert_eq!(consumed, bytes.len());
        assert_eq!(message.transaction_id, 0xffff_fffe);
        assert_eq!(message.message_type, 0x4000);
        // The player answers with its own device number, 3 on that rig.
        assert_eq!(message.arguments, vec![Field::U32(0), Field::U32(3)]);
    }

    #[test]
    fn two_back_to_back_messages_decode_independently() {
        let first = Message::new(1, 0x4001, vec![Field::U32(1)]);
        let second = Message::new(2, 0x4101, vec![Field::U32(2)]);
        let mut bytes = first.encode();
        bytes.extend_from_slice(&second.encode());

        let (decoded_first, consumed) = Message::decode(&bytes).expect("first should decode");
        assert_eq!(decoded_first, first);
        let (decoded_second, _) =
            Message::decode(&bytes[consumed..]).expect("second should decode");
        assert_eq!(decoded_second, second);
    }
}
