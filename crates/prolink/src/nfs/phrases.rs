//! Decodes a track's song structure: the phrases rekordbox's phrase
//! analysis found (`PSSI`, in the `.EXT` analysis file) — the same ones a
//! CDJ-3000 colours its waveform with.
//!
//! `rekordcrate` 0.3 parses this section but keeps its phrases private, so
//! the layout is read here, after Deep Symmetry's analysis
//! (<https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html#song-structure-tag>).
//! Big-endian throughout:
//!
//! ```text
//! 00  PSSI, header length (0x20), section length
//! 0c  u32  bytes per phrase entry: 24
//! 10  u16  phrase count
//! 12  u16  mood: 1 high, 2 mid, 3 low — what the phrase kinds mean
//! 1a  u16  the beat the last phrase ends on
//! 20       phrase entries, each:
//!          00 u16 index, from 1    02 u16 first beat    04 u16 kind
//!          07 u8  k1    09 u8 k2    13 u8 k3  (variant flags, high mood)
//! ```
//!
//! Since rekordbox 6 everything from the mood on is XOR-masked with a fixed
//! 19-byte key, each byte offset by the phrase count; older exports are
//! plain. Both readings are tried, unmasked first.
//!
//! A reading is accepted whole or not at all. Phrase indexes must run 1, 2,
//! 3…, beats must climb, and every kind must be one the mood defines; the
//! wrong one of the two readings fails this at once. A section neither
//! reading passes gives no structure, and the caller falls back to
//! inferring one.

use super::anlz;

const PSSI_MAGIC: [u8; 4] = *b"PSSI";
const HEADER_LEN: usize = 0x20;
const ENTRY_LEN: usize = 24;
const COUNT_OFFSET: usize = 0x10;
/// Where the masking starts: the mood, right after the phrase count.
const MASKED_FROM: usize = 0x12;
const END_BEAT_OFFSET: usize = 0x1a;
const MASK_KEY: [u8; 19] = [
    0xcb, 0xe1, 0xee, 0xfa, 0xe5, 0xee, 0xad, 0xee, 0xe9, 0xd2, 0xe9, 0xeb, 0xe1, 0xe9, 0xf3, 0xe8,
    0xe9, 0xf4, 0xe1,
];

/// The overall character rekordbox gave a track, which decides what its
/// phrase kinds are called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhraseMood {
    High,
    Mid,
    Low,
}

impl PhraseMood {
    fn from_raw(raw: u16) -> Option<Self> {
        match raw {
            1 => Some(Self::High),
            2 => Some(Self::Mid),
            3 => Some(Self::Low),
            _ => None,
        }
    }
}

/// A phrase as rekordbox names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhraseKind {
    Intro,
    /// Energy rising (high mood).
    Up,
    /// Energy falling away (high mood).
    Down,
    Verse,
    Bridge,
    Chorus,
    Outro,
}

/// One phrase, from its first beat to the next phrase's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Phrase {
    /// Beat number, counted from 1 as the beat grid counts.
    pub start_beat: u16,
    pub kind: PhraseKind,
    /// The number rekordbox shows after the name: Verse 1-6, Up 1-3,
    /// Intro/Chorus/Outro 1-2 in the high mood.
    pub variant: Option<u8>,
}

impl Phrase {
    /// The phrase's name as rekordbox shows it — "Verse 2", "Up 3", "Bridge".
    pub fn label(&self) -> String {
        let name = match self.kind {
            PhraseKind::Intro => "Intro",
            PhraseKind::Up => "Up",
            PhraseKind::Down => "Down",
            PhraseKind::Verse => "Verse",
            PhraseKind::Bridge => "Bridge",
            PhraseKind::Chorus => "Chorus",
            PhraseKind::Outro => "Outro",
        };
        match self.variant {
            Some(number) => format!("{name} {number}"),
            None => name.to_string(),
        }
    }
}

/// A track's phrases, in playing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongStructure {
    pub mood: PhraseMood,
    /// The beat the last phrase ends on. The track may run on past it,
    /// mostly in silence.
    pub end_beat: u16,
    pub phrases: Vec<Phrase>,
}

/// The song structure in an analysis file, if it has a valid one.
pub fn parse_song_structure(file: &[u8]) -> Option<SongStructure> {
    anlz::sections(file)
        .find(|section| section.tag == PSSI_MAGIC)
        .and_then(|section| parse_pssi(section.bytes))
}

/// Decodes one `PSSI` section, header included.
pub fn parse_pssi(section: &[u8]) -> Option<SongStructure> {
    if section.get(..4)? != PSSI_MAGIC
        || read_u32(section, 4)? as usize != HEADER_LEN
        || read_u32(section, 0x0c)? as usize != ENTRY_LEN
    {
        return None;
    }
    let count = read_u16(section, COUNT_OFFSET)?;
    if section.len() != HEADER_LEN + ENTRY_LEN * usize::from(count) {
        return None;
    }

    decode(&unmask(section, count), count).or_else(|| decode(section, count))
}

fn decode(section: &[u8], count: u16) -> Option<SongStructure> {
    let mood = PhraseMood::from_raw(read_u16(section, MASKED_FROM)?)?;
    let end_beat = read_u16(section, END_BEAT_OFFSET)?;

    let phrases = section[HEADER_LEN..]
        .as_chunks::<ENTRY_LEN>()
        .0
        .iter()
        .enumerate()
        .map(|(position, entry)| {
            let index = read_u16(entry, 0)?;
            (usize::from(index) == position + 1).then(|| decode_phrase(entry, mood))?
        })
        .collect::<Option<Vec<_>>>()?;

    let beats_climb = phrases
        .windows(2)
        .all(|pair| pair[0].start_beat < pair[1].start_beat);
    let ends_after_last = phrases
        .last()
        .is_none_or(|last| end_beat >= last.start_beat);
    (usize::from(count) == phrases.len() && beats_climb && ends_after_last).then_some(
        SongStructure {
            mood,
            end_beat,
            phrases,
        },
    )
}

fn decode_phrase(entry: &[u8; ENTRY_LEN], mood: PhraseMood) -> Option<Phrase> {
    let start_beat = read_u16(entry, 2)?;
    let kind = read_u16(entry, 4)?;
    let flags = VariantFlags {
        k1: entry[0x07],
        k2: entry[0x09],
        k3: entry[0x13],
    };
    let (kind, variant) = match mood {
        PhraseMood::High => high_phrase(kind, flags)?,
        PhraseMood::Mid => mid_phrase(kind)?,
        PhraseMood::Low => low_phrase(kind)?,
    };
    Some(Phrase {
        start_beat,
        kind,
        variant,
    })
}

#[derive(Debug, Clone, Copy)]
struct VariantFlags {
    k1: u8,
    k2: u8,
    k3: u8,
}

/// Deep Symmetry's "High mood phrase variants" table.
fn high_phrase(kind: u16, flags: VariantFlags) -> Option<(PhraseKind, Option<u8>)> {
    let one_or_two = match flags.k1 {
        1 => Some(1),
        0 => Some(2),
        _ => None,
    };
    match kind {
        1 => Some((PhraseKind::Intro, one_or_two)),
        2 => {
            let variant = match (flags.k2, flags.k3) {
                (0, 0) => Some(1),
                (0, 1) => Some(2),
                (1, 0) => Some(3),
                _ => None,
            };
            Some((PhraseKind::Up, variant))
        }
        3 => Some((PhraseKind::Down, None)),
        5 => Some((PhraseKind::Chorus, one_or_two)),
        6 => Some((PhraseKind::Outro, one_or_two)),
        _ => None,
    }
}

fn mid_phrase(kind: u16) -> Option<(PhraseKind, Option<u8>)> {
    match kind {
        1 => Some((PhraseKind::Intro, None)),
        2..=7 => Some((PhraseKind::Verse, u8::try_from(kind - 1).ok())),
        8 => Some((PhraseKind::Bridge, None)),
        9 => Some((PhraseKind::Chorus, None)),
        10 => Some((PhraseKind::Outro, None)),
        _ => None,
    }
}

/// Three kinds each mean Verse 1 and Verse 2; rekordbox shows no difference.
fn low_phrase(kind: u16) -> Option<(PhraseKind, Option<u8>)> {
    match kind {
        1 => Some((PhraseKind::Intro, None)),
        2..=4 => Some((PhraseKind::Verse, Some(1))),
        5..=7 => Some((PhraseKind::Verse, Some(2))),
        8 => Some((PhraseKind::Bridge, None)),
        9 => Some((PhraseKind::Chorus, None)),
        10 => Some((PhraseKind::Outro, None)),
        _ => None,
    }
}

/// The section with its masked part XORed with the key, which both masks
/// and unmasks.
fn unmask(section: &[u8], count: u16) -> Vec<u8> {
    let offset = count.to_le_bytes()[0];
    let mut bytes = section.to_vec();
    for (byte, key) in bytes[MASKED_FROM..].iter_mut().zip(MASK_KEY.iter().cycle()) {
        *byte ^= key.wrapping_add(offset);
    }
    bytes
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let field = bytes.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([field[0], field[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let field = bytes.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([field[0], field[1], field[2], field[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    /// The `PSSI` section of a rekordbox demo track's `.EXT`, masked, as
    /// rekordbox exported it (`rekordcrate` 0.3's test data,
    /// `demo_tracks/PIONEER/USBANLZ/P053/0001D21F/ANLZ0000.EXT`).
    const DEMO_TRACK_PSSI: &str = concat!(
        "50535349000000200000012800000018000bd6eef905f0f9b8f9f5dcf5f6ecf4fef2f4feecd7ecf905f0f9b8f9f4ddf4",
        "f6ecf4fef3f4ffecd6eef924f0fbb8f9f4ddf4f6ecf4fef3f4ffecd6ecf905f0f9bbf9c9ddf7f6ecf4fef3f4ffecd6ec",
        "f905f0f9b8f9f4ddf4f2eca9fef0f4ffecd6ecf905f0f9b8f9f4ddf4f6ecf4fef3f1ff81d6e8f905f0f9b8f9f4ddf4f6",
        "ecf4fef3f4ffecd6ecff058df9b0f9f4ddf4f6ecf4fef3f4ffecd6ecf905f0f9b8fef440f4feecf4fef3f4ffecd6ecf9",
        "05f0f9b8f9f4ddf4f6e4f453f3f1ffecd6ecf905f0f9b8f9f4ddf4f6ecf4fef3f4f6ec6becff05f0f9b8f9f4ddf4f6ec",
        "f4fef3f4ffecd6ecf90ff034b8fcf4ddf4f6ecf4fef3f4ffecd6ecf905f0f9b8f9ffdd29f6e6f4fef3f4ffecd6ecf905",
        "f0f9b8f9f4ddf4f6",
    );

    /// A plain (unmasked) section, as rekordbox 5 wrote them.
    fn plain_section(mood: u16, end_beat: u16, entries: &[(u16, u16, [u8; 3])]) -> Vec<u8> {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[..4].copy_from_slice(&PSSI_MAGIC);
        bytes[4..8].copy_from_slice(&(HEADER_LEN as u32).to_be_bytes());
        let total = (HEADER_LEN + ENTRY_LEN * entries.len()) as u32;
        bytes[8..12].copy_from_slice(&total.to_be_bytes());
        bytes[0x0c..0x10].copy_from_slice(&(ENTRY_LEN as u32).to_be_bytes());
        bytes[0x10..0x12].copy_from_slice(&(entries.len() as u16).to_be_bytes());
        bytes[0x12..0x14].copy_from_slice(&mood.to_be_bytes());
        bytes[0x1a..0x1c].copy_from_slice(&end_beat.to_be_bytes());
        for (index, (beat, kind, [k1, k2, k3])) in entries.iter().enumerate() {
            let mut entry = vec![0u8; ENTRY_LEN];
            entry[0..2].copy_from_slice(&(index as u16 + 1).to_be_bytes());
            entry[2..4].copy_from_slice(&beat.to_be_bytes());
            entry[4..6].copy_from_slice(&kind.to_be_bytes());
            entry[0x07] = *k1;
            entry[0x09] = *k2;
            entry[0x13] = *k3;
            bytes.extend(entry);
        }
        bytes
    }

    fn masked(section: Vec<u8>) -> Vec<u8> {
        let count = read_u16(&section, COUNT_OFFSET).expect("a phrase count");
        unmask(&section, count)
    }

    fn labels(structure: &SongStructure) -> Vec<(u16, String)> {
        structure
            .phrases
            .iter()
            .map(|phrase| (phrase.start_beat, phrase.label()))
            .collect()
    }

    #[test]
    fn decodes_a_masked_rekordbox_export() {
        let structure = parse_pssi(&decode_hex(DEMO_TRACK_PSSI)).expect("a song structure");

        assert_eq!(structure.mood, PhraseMood::Mid);
        assert_eq!(structure.end_beat, 257);
        assert_eq!(
            labels(&structure),
            vec![
                (1, "Intro".to_string()),
                (33, "Verse 1".to_string()),
                (61, "Verse 2".to_string()),
                (93, "Verse 2".to_string()),
                (109, "Verse 3".to_string()),
                (125, "Bridge".to_string()),
                (157, "Bridge".to_string()),
                (173, "Verse 4".to_string()),
                (189, "Verse 5".to_string()),
                (205, "Verse 4".to_string()),
                (221, "Outro".to_string()),
            ]
        );
    }

    #[test]
    fn decodes_an_unmasked_section() {
        let section = plain_section(3, 65, &[(1, 1, [0; 3]), (17, 6, [0; 3]), (49, 10, [0; 3])]);

        let structure = parse_pssi(&section).expect("a song structure");

        assert_eq!(structure.mood, PhraseMood::Low);
        assert_eq!(
            labels(&structure),
            vec![
                (1, "Intro".to_string()),
                (17, "Verse 2".to_string()),
                (49, "Outro".to_string()),
            ]
        );
    }

    #[test]
    fn the_high_mood_numbers_its_phrases_from_the_variant_flags() {
        let section = masked(plain_section(
            1,
            200,
            &[
                (1, 1, [1, 0, 0]),
                (17, 2, [0, 0, 0]),
                (33, 2, [0, 0, 1]),
                (49, 2, [0, 1, 0]),
                (65, 5, [0, 0, 0]),
                (97, 3, [0, 0, 0]),
                (129, 6, [1, 0, 0]),
            ],
        ));

        let structure = parse_pssi(&section).expect("a song structure");

        assert_eq!(structure.mood, PhraseMood::High);
        assert_eq!(
            labels(&structure),
            vec![
                (1, "Intro 1".to_string()),
                (17, "Up 1".to_string()),
                (33, "Up 2".to_string()),
                (49, "Up 3".to_string()),
                (65, "Chorus 2".to_string()),
                (97, "Down".to_string()),
                (129, "Outro 1".to_string()),
            ]
        );
    }

    #[test]
    fn a_kind_the_mood_does_not_define_rejects_the_section() {
        let section = plain_section(1, 100, &[(1, 1, [1, 0, 0]), (33, 9, [0; 3])]);
        assert_eq!(parse_pssi(&section), None);
    }

    #[test]
    fn beats_that_do_not_climb_reject_the_section() {
        let section = plain_section(2, 100, &[(1, 1, [0; 3]), (33, 2, [0; 3]), (17, 9, [0; 3])]);
        assert_eq!(parse_pssi(&section), None);
    }

    #[test]
    fn a_truncated_section_is_rejected() {
        let mut section = decode_hex(DEMO_TRACK_PSSI);
        section.truncate(section.len() - ENTRY_LEN);
        assert_eq!(parse_pssi(&section), None);
    }

    #[test]
    fn finds_the_section_inside_an_analysis_file() {
        let pssi = decode_hex(DEMO_TRACK_PSSI);
        let mut file = vec![0u8; 12];
        file[..4].copy_from_slice(b"PMAI");
        file[4..8].copy_from_slice(&12u32.to_be_bytes());
        let total = (file.len() + pssi.len()) as u32;
        file[8..12].copy_from_slice(&total.to_be_bytes());
        file.extend(&pssi);

        let structure = parse_song_structure(&file).expect("a song structure");

        assert_eq!(structure.phrases.len(), 11);
    }
}
