//! Walks the tagged sections of a rekordbox analysis file (`.DAT`, `.EXT`).
//!
//! A file is a header, then sections back to back, each opening with the
//! same twelve bytes: a four-character tag, the section header's own
//! length, and the section's total length, both big-endian. Walking by the
//! declared lengths reaches every section without decoding any of them, so
//! a section a reader cannot make sense of never costs the ones around it.

/// Bytes every section header, and the file header, starts with.
const COMMON_HEADER_LEN: usize = 12;

/// One section of an analysis file, header included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnlzSection<'a> {
    pub tag: [u8; 4],
    pub bytes: &'a [u8],
}

/// Every section of `file`, in file order. Stops at the first section whose
/// declared length is impossible, rather than reading on from a guess.
pub fn sections(file: &[u8]) -> impl Iterator<Item = AnlzSection<'_>> {
    let (mut position, end) = match (read_u32(file, 4), read_u32(file, 8)) {
        (Some(header_len), Some(total_len)) => (header_len, total_len.min(file.len())),
        _ => (0, 0),
    };

    std::iter::from_fn(move || {
        let remaining = file.get(position..end)?;
        let section_len = read_u32(remaining, 8)?;
        if section_len < COMMON_HEADER_LEN || section_len > remaining.len() {
            return None;
        }
        let bytes = &remaining[..section_len];
        position += section_len;
        Some(AnlzSection {
            tag: [bytes[0], bytes[1], bytes[2], bytes[3]],
            bytes,
        })
    })
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<usize> {
    let field = bytes.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([field[0], field[1], field[2], field[3]]) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let total = (COMMON_HEADER_LEN + body.len()) as u32;
        [
            tag.as_slice(),
            &(COMMON_HEADER_LEN as u32).to_be_bytes(),
            &total.to_be_bytes(),
            body,
        ]
        .concat()
    }

    fn file(sections: &[Vec<u8>]) -> Vec<u8> {
        let body = sections.concat();
        section(b"PMAI", &[])
            .into_iter()
            .chain(body)
            .collect::<Vec<_>>()
    }

    fn with_total_len(mut file: Vec<u8>) -> Vec<u8> {
        let total = file.len() as u32;
        file[8..12].copy_from_slice(&total.to_be_bytes());
        file
    }

    #[test]
    fn walks_every_section_in_order() {
        let bytes = with_total_len(file(&[section(b"PQTZ", &[1, 2]), section(b"PSSI", &[3])]));

        let tags: Vec<_> = sections(&bytes).map(|section| section.tag).collect();

        assert_eq!(tags, vec![*b"PQTZ", *b"PSSI"]);
    }

    #[test]
    fn stops_at_a_section_longer_than_the_file() {
        let mut broken = section(b"PCO2", &[0; 4]);
        broken[8..12].copy_from_slice(&1_000u32.to_be_bytes());
        let bytes = with_total_len(file(&[section(b"PQTZ", &[]), broken]));

        let tags: Vec<_> = sections(&bytes).map(|section| section.tag).collect();

        assert_eq!(tags, vec![*b"PQTZ"]);
    }

    #[test]
    fn a_file_too_short_for_its_header_has_no_sections() {
        assert_eq!(sections(&[0x50, 0x4d]).count(), 0);
    }
}
