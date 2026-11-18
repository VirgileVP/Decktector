//! When a deck's track is a show row's track: the rule the timecode service
//! matches every loaded deck by.
//!
//! The renderer applies the same rule in TypeScript (`lib/show-identity.ts`)
//! to find duplicate rows and merge imports. Both sides run the shared vectors
//! in `test-fixtures/track-identity-vectors.json`, which is what keeps them in
//! agreement: change one side only together with the vectors.

use unicode_normalization::UnicodeNormalization;

/// Durations come in whole seconds on both sides and may round differently.
pub const DURATION_TOLERANCE_SECONDS: u32 = 2;

/// The five entities XML predefines; every other character comes as a
/// numeric reference.
const NAMED_ENTITIES: [(&str, char); 5] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
];

const DECIMAL_RADIX: u32 = 10;
const HEX_RADIX: u32 = 16;

/// The character a reference body (what sits between `&` and `;`) stands
/// for, if it is one: `#` + decimal digits, `#x`/`#X` + hex digits, or a
/// named entity. A reference to something that is not a Unicode scalar
/// value (a surrogate, or beyond U+10FFFF) is not decoded.
fn referenced_char(body: &str) -> Option<char> {
    if let Some(number) = body.strip_prefix('#') {
        let (digits, radix) = match number.strip_prefix(['x', 'X']) {
            Some(hex) => (hex, HEX_RADIX),
            None => (number, DECIMAL_RADIX),
        };
        if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
            return None;
        }
        return u32::from_str_radix(digits, radix)
            .ok()
            .and_then(char::from_u32);
    }
    NAMED_ENTITIES
        .iter()
        .find(|(name, _)| *name == body)
        .map(|&(_, character)| character)
}

/// Decodes XML character references and the named entities, in one pass:
/// what a decoded reference produces is never decoded again.
fn decode_xml_references(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(ampersand) = rest.find('&') {
        decoded.push_str(&rest[..ampersand]);
        let after = &rest[ampersand + 1..];
        let reference = after
            .find(';')
            .and_then(|semicolon| Some((referenced_char(&after[..semicolon])?, semicolon)));
        match reference {
            Some((character, semicolon)) => {
                decoded.push(character);
                rest = &after[semicolon + 1..];
            }
            None => {
                decoded.push('&');
                rest = after;
            }
        }
    }
    decoded.push_str(rest);
    decoded
}

/// The comparable form of a title or an artist: XML references decoded
/// once, Unicode NFC, lowercase, every run of Unicode whitespace collapsed to
/// one space and trimmed. Nothing is stripped — "feat." and remix names stay
/// part of the key.
pub fn normalise_text(text: &str) -> String {
    let lowercase = decode_xml_references(text)
        .nfc()
        .collect::<String>()
        .to_lowercase();
    lowercase.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a track is recognised by, ready to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub title_key: String,
    pub artist_key: String,
    /// Whole seconds; 0 when unknown.
    pub duration_seconds: u32,
}

impl Identity {
    pub fn new(title: &str, artist: &str, duration_seconds: u32) -> Self {
        Self {
            title_key: normalise_text(title),
            artist_key: normalise_text(artist),
            duration_seconds,
        }
    }

    /// Whether `self` and `other` are the same track. An unknown duration
    /// never matches, not even on an identical title and artist: without it
    /// a cover or an edit is indistinguishable from the original.
    pub fn matches(&self, other: &Self) -> bool {
        self.duration_seconds > 0
            && other.duration_seconds > 0
            && self.duration_seconds.abs_diff(other.duration_seconds) <= DURATION_TOLERANCE_SECONDS
            && self.title_key == other.title_key
            && self.artist_key == other.artist_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    const SHARED_VECTORS: &str = include_str!("../../../test-fixtures/track-identity-vectors.json");

    #[derive(Deserialize)]
    struct Vectors {
        normalise: Vec<NormaliseVector>,
        #[serde(rename = "match")]
        matches: Vec<MatchVector>,
    }

    #[derive(Deserialize)]
    struct NormaliseVector {
        why: String,
        #[serde(rename = "in")]
        input: String,
        out: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TrackVector {
        title: String,
        artist: String,
        duration_seconds: u32,
    }

    impl TrackVector {
        fn identity(&self) -> Identity {
            Identity::new(&self.title, &self.artist, self.duration_seconds)
        }
    }

    #[derive(Deserialize)]
    struct MatchVector {
        why: String,
        a: TrackVector,
        b: TrackVector,
        matches: bool,
    }

    fn shared_vectors() -> Vectors {
        serde_json::from_str(SHARED_VECTORS).expect("the shared vector file parses")
    }

    #[test]
    fn every_shared_normalisation_vector_agrees_with_the_typescript_side() {
        let vectors = shared_vectors().normalise;
        assert!(!vectors.is_empty());
        for vector in vectors {
            assert_eq!(normalise_text(&vector.input), vector.out, "{}", vector.why);
        }
    }

    #[test]
    fn every_shared_match_vector_agrees_with_the_typescript_side() {
        let vectors = shared_vectors().matches;
        assert!(!vectors.is_empty());
        for vector in vectors {
            let (a, b) = (vector.a.identity(), vector.b.identity());
            assert_eq!(a.matches(&b), vector.matches, "{}", vector.why);
            assert_eq!(b.matches(&a), vector.matches, "{} (reversed)", vector.why);
        }
    }

    #[test]
    fn an_unknown_duration_never_matches() {
        let known = Identity::new("Strobe", "deadmau5", 637);
        let unknown = Identity::new("Strobe", "deadmau5", 0);
        assert!(!known.matches(&unknown));
        assert!(!unknown.matches(&known));
        assert!(!unknown.matches(&unknown.clone()));
    }

    #[test]
    fn nfc_and_nfd_spellings_of_the_same_title_match() {
        let composed = Identity::new("Caf\u{e9} del Mar", "Energy 52", 480);
        let decomposed = Identity::new("Cafe\u{301} del Mar", "Energy 52", 480);
        assert!(composed.matches(&decomposed));
    }

    #[test]
    fn remix_text_is_part_of_the_key() {
        let extended = Identity::new("Strobe (Extended Mix)", "deadmau5", 637);
        let radio = Identity::new("Strobe (Radio Edit)", "deadmau5", 637);
        let bare = Identity::new("Strobe", "deadmau5", 637);
        assert!(!extended.matches(&radio));
        assert!(!extended.matches(&bare));
    }
}
