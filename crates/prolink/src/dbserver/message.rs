//! Wire format for the dbserver/remotedb protocol: a typed-argument message
//! envelope carried over TCP, once port discovery and the setup handshake
//! (see [`super::client`]) have opened a connection.
//!
//! Verified against Deep Symmetry's `beat-link` (the actively-maintained,
//! hardware-validated Java reference implementation of this exact protocol)
//! during design: magic number, field type tags and the query/response type
//! ids in [`super::queries`] all cross-check against it. Treat this file's
//! envelope shape as solid; treat exact behavior around message boundaries
//! (how many bytes a real player sends per read) as unverified without real
//! hardware, which is why [`super::client::Connection::read_message`]
//! accumulates and re-parses rather than assuming one read equals one
//! message.

/// Every message starts with this 4-byte value.
pub const MAGIC: u32 = 0x872349ae;

/// Fixed header size: 4-byte magic, 4-byte transaction id, 2-byte message
/// type, 1-byte argument count, 12-byte argument-type-tag block.
const HEADER_LEN: usize = 4 + 4 + 2 + 1 + 12;
/// A message declares up to this many arguments' type tags up front, even
/// if it carries fewer.
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

    /// Encodes just the value bytes — the type tag itself was already
    /// written as part of the message header's tag block, not repeated
    /// per-argument.
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

    /// Decodes one value of the given type from the start of `data`;
    /// `None` if `data` doesn't yet hold enough bytes (the caller should
    /// read more and retry) — never panics on short input. Returns the
    /// field and how many bytes it consumed.
    fn decode_value(tag: u8, data: &[u8]) -> Option<(Self, usize)> {
        match tag {
            0x0f => data.first().map(|&byte| (Self::U8(byte), 1)),
            0x10 => (data.len() >= 2).then(|| (Self::U16(u16::from_be_bytes([data[0], data[1]])), 2)),
            0x11 => (data.len() >= 4)
                .then(|| (Self::U32(u32::from_be_bytes([data[0], data[1], data[2], data[3]])), 4)),
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
        Self { transaction_id, message_type, arguments }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.arguments.len() * 4);
        out.extend_from_slice(&MAGIC.to_be_bytes());
        out.extend_from_slice(&self.transaction_id.to_be_bytes());
        out.extend_from_slice(&self.message_type.to_be_bytes());
        out.push(self.arguments.len() as u8);

        let mut tags = [0u8; MAX_ARGS];
        for (slot, argument) in tags.iter_mut().zip(&self.arguments) {
            *slot = argument.type_tag();
        }
        out.extend_from_slice(&tags);

        for argument in &self.arguments {
            argument.encode_value(&mut out);
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
        if data.len() < HEADER_LEN {
            return None;
        }
        if u32::from_be_bytes([data[0], data[1], data[2], data[3]]) != MAGIC {
            return None;
        }
        let transaction_id = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let message_type = u16::from_be_bytes([data[8], data[9]]);
        let arg_count = data[10] as usize;
        if arg_count > MAX_ARGS {
            return None;
        }
        let tags = &data[11..11 + MAX_ARGS];

        let mut offset = HEADER_LEN;
        let mut arguments = Vec::with_capacity(arg_count);
        for &tag in &tags[..arg_count] {
            let (field, consumed) = Field::decode_value(tag, &data[offset..])?;
            arguments.push(field);
            offset += consumed;
        }

        Some((Self { transaction_id, message_type, arguments }, offset))
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
        bytes.extend_from_slice(&MAGIC.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&0x4101u16.to_be_bytes());
        bytes.push(1);
        let mut tags = [0u8; MAX_ARGS];
        tags[0] = 0x26;
        bytes.extend_from_slice(&tags);
        // 4 characters: 'H', 'i', NUL — length-in-characters is 3.
        bytes.extend_from_slice(&3u32.to_be_bytes());
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
        bytes[0] = 0x00;
        assert!(Message::decode(&bytes).is_none());
    }

    #[test]
    fn decode_reports_none_on_an_implausible_argument_count() {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[..4].copy_from_slice(&MAGIC.to_be_bytes());
        bytes[10] = 200; // Far more than MAX_ARGS — a desynced stream, not a real message.
        assert!(Message::decode(&bytes).is_none());
    }

    #[test]
    fn two_back_to_back_messages_decode_independently() {
        let first = Message::new(1, 0x4001, vec![Field::U32(1)]);
        let second = Message::new(2, 0x4101, vec![Field::U32(2)]);
        let mut bytes = first.encode();
        bytes.extend_from_slice(&second.encode());

        let (decoded_first, consumed) = Message::decode(&bytes).expect("first should decode");
        assert_eq!(decoded_first, first);
        let (decoded_second, _) = Message::decode(&bytes[consumed..]).expect("second should decode");
        assert_eq!(decoded_second, second);
    }
}
