//! XDR (RFC 4506) primitive encoding — the byte-level rules every ONC-RPC
//! message in this module (portmapper, MOUNT, NFS) is built from.
//!
//! Distinct from [`crate::dbserver::message`]'s tagged-field format: XDR has
//! no type tags at all — both sides already know the shape from the
//! procedure being called — just a handful of primitives, all 4-byte
//! aligned:
//!
//! - Every integer is 4 bytes, big-endian, no smaller size exists on the wire.
//! - Fixed-length opaque data (e.g. a 32-byte NFS file handle) is sent as-is,
//!   no length prefix, padded with zero bytes up to the next 4-byte boundary.
//! - Variable-length opaque data is a 4-byte length prefix followed by that
//!   many bytes, same padding.
//! - Strings follow the same variable-length-opaque shape. Every string this
//!   module sends is a filename or path, which Pioneer's players encode as
//!   UTF-16LE rather than the ASCII RFC 1094 specifies — confirmed from Deep
//!   Symmetry's `crate-digger` (the hardware-validated Java reference for
//!   this exact protocol), whose own hand-written XDR spec comment says so
//!   outright. Everything else here matches the RFC exactly.

/// Builds an XDR-encoded byte sequence field by field.
#[derive(Debug, Default)]
pub struct XdrEncoder {
    buf: Vec<u8>,
}

impl XdrEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write_u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    /// A fixed-length field whose length the protocol already knows (e.g. a
    /// 32-byte file handle) — no length prefix, just the bytes and padding.
    pub fn write_opaque_fixed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
        self.pad(data.len());
    }

    /// A length-prefixed field whose length varies per call (e.g. a
    /// filename).
    pub fn write_opaque_var(&mut self, data: &[u8]) {
        self.write_u32(data.len() as u32);
        self.write_opaque_fixed(data);
    }

    /// Encodes `text` as UTF-16LE and writes it as a variable-length opaque
    /// field — the shape every filename/path argument in this module needs.
    pub fn write_string_utf16le(&mut self, text: &str) {
        let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        self.write_opaque_var(&bytes);
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    fn pad(&mut self, len: usize) {
        let padding = (4 - len % 4) % 4;
        self.buf.resize(self.buf.len() + padding, 0);
    }
}

/// Reads an XDR-encoded byte sequence field by field, advancing an internal
/// cursor. Every method returns `None` on insufficient data rather than
/// panicking — this decodes bytes a real player sent over the network, never
/// to be trusted implicitly.
pub struct XdrDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> XdrDecoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn read_u32(&mut self) -> Option<u32> {
        let bytes = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some(u32::from_be_bytes(
            bytes.try_into().expect("slice is exactly 4 bytes"),
        ))
    }

    pub fn read_opaque_fixed(&mut self, len: usize) -> Option<&'a [u8]> {
        let bytes = self.data.get(self.pos..self.pos + len)?;
        self.pos += len;
        self.skip_padding(len);
        Some(bytes)
    }

    pub fn read_opaque_var(&mut self) -> Option<&'a [u8]> {
        let len = self.read_u32()? as usize;
        self.read_opaque_fixed(len)
    }

    pub fn read_string_utf16le(&mut self) -> Option<String> {
        let bytes = self.read_opaque_var()?;
        if bytes.len() % 2 != 0 {
            return None;
        }
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        Some(String::from_utf16_lossy(&units))
    }

    /// Everything not yet consumed — procedure-specific results that follow
    /// a fixed RPC reply header read the rest of the message from here.
    pub fn remaining(&self) -> &'a [u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    fn skip_padding(&mut self, len: usize) {
        let padding = (4 - len % 4) % 4;
        self.pos = (self.pos + padding).min(self.data.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_u32_round_trips() {
        let mut encoder = XdrEncoder::new();
        encoder.write_u32(0xdead_beef);
        let bytes = encoder.into_bytes();
        assert_eq!(bytes.len(), 4);
        assert_eq!(XdrDecoder::new(&bytes).read_u32(), Some(0xdead_beef));
    }

    #[test]
    fn fixed_opaque_data_already_a_multiple_of_four_needs_no_padding() {
        let mut encoder = XdrEncoder::new();
        encoder.write_opaque_fixed(&[1, 2, 3, 4]);
        assert_eq!(encoder.into_bytes().len(), 4);
    }

    #[test]
    fn fixed_opaque_data_gets_padded_to_the_next_four_byte_boundary() {
        let mut encoder = XdrEncoder::new();
        encoder.write_opaque_fixed(&[1, 2, 3]); // 3 bytes -> 1 byte of padding.
        let bytes = encoder.into_bytes();
        assert_eq!(bytes.len(), 4);
        assert_eq!(&bytes, &[1, 2, 3, 0]);
    }

    #[test]
    fn a_thirty_two_byte_file_handle_round_trips_with_no_padding() {
        let handle: Vec<u8> = (0..32).collect();
        let mut encoder = XdrEncoder::new();
        encoder.write_opaque_fixed(&handle);
        let bytes = encoder.into_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(
            XdrDecoder::new(&bytes).read_opaque_fixed(32),
            Some(handle.as_slice())
        );
    }

    #[test]
    fn variable_opaque_data_round_trips_with_its_length_prefix() {
        let mut encoder = XdrEncoder::new();
        encoder.write_opaque_var(&[1, 2, 3, 4, 5]);
        let bytes = encoder.into_bytes();
        // 4-byte length + 5 bytes of data + 3 bytes of padding = 12.
        assert_eq!(bytes.len(), 12);
        assert_eq!(
            XdrDecoder::new(&bytes).read_opaque_var(),
            Some(&[1, 2, 3, 4, 5][..])
        );
    }

    #[test]
    fn a_utf16le_string_round_trips() {
        let mut encoder = XdrEncoder::new();
        encoder.write_string_utf16le("PIONEER");
        let bytes = encoder.into_bytes();
        assert_eq!(
            XdrDecoder::new(&bytes).read_string_utf16le(),
            Some("PIONEER".to_string())
        );
    }

    #[test]
    fn a_utf16le_string_is_genuinely_little_endian_on_the_wire() {
        let mut encoder = XdrEncoder::new();
        encoder.write_string_utf16le("A"); // U+0041 -> bytes [0x41, 0x00] little-endian.
        let bytes = encoder.into_bytes();
        assert_eq!(&bytes[4..6], &[0x41, 0x00]);
    }

    #[test]
    fn fields_chain_correctly_after_a_padded_field() {
        let mut encoder = XdrEncoder::new();
        encoder.write_opaque_var(b"abc"); // 3 bytes -> needs 1 padding byte.
        encoder.write_u32(0x1234_5678);
        let bytes = encoder.into_bytes();

        let mut decoder = XdrDecoder::new(&bytes);
        assert_eq!(decoder.read_opaque_var(), Some(&b"abc"[..]));
        assert_eq!(decoder.read_u32(), Some(0x1234_5678));
    }

    #[test]
    fn reading_past_the_end_returns_none_rather_than_panicking() {
        let mut decoder = XdrDecoder::new(&[0, 0]);
        assert_eq!(decoder.read_u32(), None);
    }

    #[test]
    fn an_odd_length_utf16_byte_string_is_rejected() {
        let mut encoder = XdrEncoder::new();
        encoder.write_opaque_var(&[0x41, 0x00, 0x42]); // 3 bytes: not a whole number of UTF-16 units.
        let bytes = encoder.into_bytes();
        assert_eq!(XdrDecoder::new(&bytes).read_string_utf16le(), None);
    }
}
