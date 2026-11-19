//! OSC 1.0 message encoding — just what tempo output needs.
//!
//! <https://opensoundcontrol.stanford.edu/spec-1_0.html> — a message is the
//! address pattern, the type tag string and the arguments, each part padded
//! with zero bytes to a multiple of 4 bytes. Only 32-bit integers and floats
//! and strings are supported: nothing else is sent, so nothing else is
//! encoded.

/// Characters the OSC spec reserves for address patterns on the receiving
/// side; a concrete address a sender emits must not contain any of them.
const RESERVED_ADDRESS_CHARS: &str = " #*,?[]{}";

/// OSC data is aligned to 4-byte boundaries.
const ALIGNMENT: usize = 4;

/// One OSC argument.
#[derive(Debug, Clone, PartialEq)]
pub enum OscArg {
    /// Type tag `i`: big-endian 32-bit signed integer.
    Int(i32),
    /// Type tag `f`: big-endian IEEE 754 32-bit float.
    Float(f32),
    /// Type tag `s`: the text, a terminating null, padded to 4 bytes.
    String(String),
}

impl OscArg {
    fn type_tag(&self) -> char {
        match self {
            Self::Int(_) => 'i',
            Self::Float(_) => 'f',
            Self::String(_) => 's',
        }
    }

    fn write_to(&self, buffer: &mut Vec<u8>) {
        match self {
            Self::Int(value) => buffer.extend_from_slice(&value.to_be_bytes()),
            Self::Float(value) => buffer.extend_from_slice(&value.to_be_bytes()),
            Self::String(text) => write_padded_string(buffer, text),
        }
    }
}

/// Whether `address` is a concrete OSC address this module can encode: it
/// starts with `/` and holds only printable ASCII without spaces or the
/// pattern characters ` #*,?[]{}`.
pub fn is_valid_address(address: &str) -> bool {
    address.starts_with('/')
        && address
            .chars()
            .all(|c| c.is_ascii_graphic() && !RESERVED_ADDRESS_CHARS.contains(c))
}

/// Encodes one OSC message. `address` is expected to satisfy
/// [`is_valid_address`]; it is written as-is.
pub fn encode_message(address: &str, args: &[OscArg]) -> Vec<u8> {
    let mut buffer = Vec::new();
    write_padded_string(&mut buffer, address);

    let type_tags: String = std::iter::once(',')
        .chain(args.iter().map(|arg| arg.type_tag()))
        .collect();
    write_padded_string(&mut buffer, &type_tags);

    for arg in args {
        arg.write_to(&mut buffer);
    }
    buffer
}

/// Writes `text`, a terminating null, then zeros up to the next 4-byte
/// boundary. Everything written before is already aligned, so the buffer
/// length alone says how much padding is left.
fn write_padded_string(buffer: &mut Vec<u8>, text: &str) {
    buffer.extend_from_slice(text.as_bytes());
    buffer.push(0);
    while !buffer.len().is_multiple_of(ALIGNMENT) {
        buffer.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shorter_than_a_boundary_is_padded() {
        // "/a" + null = 3 bytes, one pad byte.
        let bytes = encode_message("/a", &[]);
        assert_eq!(bytes, [b'/', b'a', 0, 0, b',', 0, 0, 0]);
    }

    #[test]
    fn address_that_fills_a_boundary_with_its_null_gets_no_extra_padding() {
        // "/ab" + null = exactly 4 bytes.
        let bytes = encode_message("/ab", &[]);
        assert_eq!(bytes, [b'/', b'a', b'b', 0, b',', 0, 0, 0]);
    }

    #[test]
    fn address_whose_length_is_a_multiple_of_four_gets_a_whole_padding_word() {
        // "/abc" is 4 bytes: the mandatory null starts a new word.
        let bytes = encode_message("/abc", &[]);
        assert_eq!(bytes, [b'/', b'a', b'b', b'c', 0, 0, 0, 0, b',', 0, 0, 0]);
    }

    #[test]
    fn int_argument_is_big_endian() {
        let bytes = encode_message("/ab", &[OscArg::Int(0x0102_0304)]);
        assert_eq!(bytes, [b'/', b'a', b'b', 0, b',', b'i', 0, 0, 1, 2, 3, 4]);
    }

    #[test]
    fn negative_int_is_twos_complement() {
        let bytes = encode_message("/ab", &[OscArg::Int(-1)]);
        assert_eq!(&bytes[8..], [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn float_argument_is_big_endian_ieee754() {
        // 128.0f32 = 0x43000000.
        let bytes = encode_message("/ab", &[OscArg::Float(128.0)]);
        assert_eq!(
            bytes,
            [b'/', b'a', b'b', 0, b',', b'f', 0, 0, 0x43, 0, 0, 0]
        );
    }

    #[test]
    fn type_tags_that_fill_a_boundary_get_a_whole_padding_word() {
        // ",iii" is 4 bytes: the null starts a new word.
        let bytes = encode_message("/ab", &[OscArg::Int(1), OscArg::Int(2), OscArg::Int(3)]);
        assert_eq!(&bytes[4..12], [b',', b'i', b'i', b'i', 0, 0, 0, 0]);
        assert_eq!(bytes.len(), 4 + 8 + 12);
    }

    #[test]
    fn mixed_arguments_keep_their_order() {
        let bytes = encode_message("/ab", &[OscArg::Int(1), OscArg::Float(1.0)]);
        assert_eq!(
            bytes,
            [b'/', b'a', b'b', 0, b',', b'i', b'f', 0, 0, 0, 0, 1, 0x3F, 0x80, 0, 0]
        );
    }

    #[test]
    fn message_without_arguments_has_an_empty_type_tag_string() {
        // "," + null, padded: the tag string is just the comma.
        let bytes = encode_message("/ab", &[]);
        assert_eq!(bytes, [b'/', b'a', b'b', 0, b',', 0, 0, 0]);
        assert_eq!(encode_message("/abc", &[]).len(), 12);
    }

    #[test]
    fn string_argument_is_null_terminated_and_padded() {
        // "hi" + null = 3 bytes, one pad byte.
        let bytes = encode_message("/ab", &[OscArg::String("hi".to_string())]);
        assert_eq!(
            bytes,
            [b'/', b'a', b'b', 0, b',', b's', 0, 0, b'h', b'i', 0, 0]
        );
    }

    #[test]
    fn string_argument_that_fills_a_boundary_gets_a_whole_padding_word() {
        let bytes = encode_message("/ab", &[OscArg::String("abcd".to_string())]);
        assert_eq!(&bytes[8..], [b'a', b'b', b'c', b'd', 0, 0, 0, 0],);
    }

    #[test]
    fn empty_string_argument_is_one_zero_word() {
        let bytes = encode_message("/ab", &[OscArg::String(String::new())]);
        assert_eq!(&bytes[8..], [0, 0, 0, 0]);
    }

    #[test]
    fn string_and_numbers_keep_their_order() {
        let bytes = encode_message(
            "/ab",
            &[
                OscArg::Int(1),
                OscArg::String("x".to_string()),
                OscArg::Float(1.0),
            ],
        );
        // ",isf" fills a boundary, so its null opens a whole padding word.
        assert_eq!(&bytes[4..12], [b',', b'i', b's', b'f', 0, 0, 0, 0]);
        assert_eq!(&bytes[12..16], [0, 0, 0, 1]);
        assert_eq!(&bytes[16..20], [b'x', 0, 0, 0]);
        assert_eq!(&bytes[20..], [0x3F, 0x80, 0, 0]);
    }

    #[test]
    fn encoded_length_is_always_a_multiple_of_four() {
        for length in 1..12 {
            let address = format!("/{}", "x".repeat(length));
            let bytes = encode_message(&address, &[OscArg::Float(1.5)]);
            assert_eq!(bytes.len() % ALIGNMENT, 0, "{address}");
        }
    }

    #[test]
    fn accepts_plain_addresses() {
        assert!(is_valid_address("/decktective/master/bpm"));
        assert!(is_valid_address("/deck/1/beat"));
        assert!(is_valid_address("/a-b_c.d"));
    }

    #[test]
    fn rejects_addresses_without_a_leading_slash() {
        assert!(!is_valid_address(""));
        assert!(!is_valid_address("decktective"));
    }

    #[test]
    fn rejects_reserved_and_non_printable_characters() {
        for bad in [
            "/a b",
            "/a#b",
            "/a*b",
            "/a,b",
            "/a?b",
            "/a[b",
            "/a]b",
            "/a{b",
            "/a}b",
            "/a\tb",
            "/a\u{e9}b",
        ] {
            assert!(!is_valid_address(bad), "{bad:?}");
        }
    }
}
