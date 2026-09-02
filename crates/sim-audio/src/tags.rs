//! Where a track's words come from: its ID3 tags when it has them, its
//! file name when it does not.
//!
//! A file bought from a store or exported by rekordbox carries a title, an
//! artist, often a BPM and a key. A file that carries none is not useless
//! for the simulation — the audio still has a waveform and a tempo — so the
//! name is parsed rather than the track skipped, and the fields nothing can
//! supply stay `None` instead of being invented.

use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use symphonia::core::meta::{MetadataRevision, StandardTagKey, Value};

/// Raw ID3 frame ids read directly, for fields Symphonia does not map to a
/// [`StandardTagKey`].
const KEY_FRAMES: [&str; 2] = ["TKEY", "INITIALKEY"];

/// Everything about a track that is words rather than sound.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub comment: Option<String>,
    pub key: Option<String>,
    pub bpm: Option<f64>,
    /// Embedded cover art as a ready-to-use `data:` URL, the form
    /// [`decktective_core::model::TrackInfo::artwork`] is served in.
    pub artwork: Option<String>,
}

impl Tags {
    /// Reads what the file declares about itself.
    pub fn from_metadata(revision: &MetadataRevision) -> Self {
        let mut tags = Self::default();
        for tag in revision.tags() {
            let text = value_text(&tag.value);
            let field = match tag.std_key {
                Some(StandardTagKey::TrackTitle) => &mut tags.title,
                Some(StandardTagKey::Artist) | Some(StandardTagKey::AlbumArtist) => {
                    &mut tags.artist
                }
                Some(StandardTagKey::Album) => &mut tags.album,
                Some(StandardTagKey::Genre) => &mut tags.genre,
                Some(StandardTagKey::Comment) => &mut tags.comment,
                Some(StandardTagKey::Bpm) => {
                    tags.bpm = tags.bpm.or_else(|| text.as_deref().and_then(parse_bpm));
                    continue;
                }
                _ if KEY_FRAMES.contains(&tag.key.to_uppercase().as_str()) => &mut tags.key,
                _ => continue,
            };
            if field.is_none() {
                *field = text;
            }
        }

        tags.artwork = revision.visuals().iter().find_map(artwork_url);
        tags
    }

    /// Fills whatever the tags left empty from the file name, which by
    /// convention reads `artist-title` or `artist-title-id`.
    ///
    /// Only the title and artist can come from a name; a track with no
    /// tags keeps `None` everywhere else rather than gaining a genre
    /// nobody wrote.
    pub fn or_file_name(mut self, path: &Path) -> Self {
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            return self;
        };
        let (artist, title) = split_file_name(stem);
        self.title = self.title.or(Some(title));
        self.artist = self.artist.or(artist);
        self
    }

    /// The title to show, falling back to the file's own name so a track
    /// can never render as an empty row.
    pub fn title_or_placeholder(&self, path: &Path) -> String {
        self.title.clone().unwrap_or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }
}

fn value_text(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => text.trim().to_owned(),
        other => other.to_string().trim().to_owned(),
    };
    (!text.is_empty()).then_some(text)
}

/// Parses a BPM tag, which stores anything from `128` to `128.00` to
/// `"128 BPM"` depending on what wrote it.
fn parse_bpm(text: &str) -> Option<f64> {
    let number: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    number
        .replace(',', ".")
        .parse()
        .ok()
        .filter(|bpm: &f64| *bpm > 0.0)
}

fn artwork_url(visual: &symphonia::core::meta::Visual) -> Option<String> {
    (!visual.data.is_empty()).then(|| {
        format!(
            "data:{};base64,{}",
            visual.media_type,
            BASE64.encode(&visual.data)
        )
    })
}

/// Splits `artist-title` or `artist-title-1234`, the shapes download stores
/// name their files in. A trailing all-digit segment is the store's id and
/// is dropped.
fn split_file_name(stem: &str) -> (Option<String>, String) {
    let mut parts: Vec<&str> = stem
        .split('-')
        .filter(|part| !part.trim().is_empty())
        .collect();
    if parts.len() > 2
        && parts
            .last()
            .is_some_and(|last| last.chars().all(|c| c.is_ascii_digit()))
    {
        parts.pop();
    }

    match parts.split_first() {
        Some((artist, title)) if !title.is_empty() => {
            (Some(titleize(artist)), titleize(&title.join(" ")))
        }
        _ => (None, titleize(stem)),
    }
}

/// Turns a file-name fragment into something readable: underscores become
/// spaces, and every word gets its capital back.
fn titleize(text: &str) -> String {
    text.replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_artist_and_title_from_a_store_file_name() {
        let tags = Tags::default().or_file_name(Path::new("alec_koff-carnaval-484622.mp3"));

        assert_eq!(tags.artist.as_deref(), Some("Alec Koff"));
        assert_eq!(tags.title.as_deref(), Some("Carnaval"));
    }

    #[test]
    fn keeps_every_word_of_a_multi_part_title() {
        let tags = Tags::default()
            .or_file_name(Path::new("grand_project-wonders-of-the-earth-550792.mp3"));

        assert_eq!(tags.artist.as_deref(), Some("Grand Project"));
        assert_eq!(tags.title.as_deref(), Some("Wonders Of The Earth"));
    }

    #[test]
    fn a_name_with_no_artist_part_becomes_the_title() {
        let tags = Tags::default().or_file_name(Path::new("untitled_sketch.mp3"));

        assert_eq!(tags.artist, None);
        assert_eq!(tags.title.as_deref(), Some("Untitled Sketch"));
    }

    #[test]
    fn a_tag_wins_over_the_file_name() {
        let tagged = Tags {
            title: Some("Real Title".into()),
            ..Tags::default()
        };

        let tags = tagged.or_file_name(Path::new("artist-guessed-1.mp3"));

        assert_eq!(tags.title.as_deref(), Some("Real Title"));
        assert_eq!(tags.artist.as_deref(), Some("Artist"));
    }

    #[test]
    fn bpm_tags_parse_whatever_shape_they_were_written_in() {
        assert_eq!(parse_bpm("128"), Some(128.0));
        assert_eq!(parse_bpm("124.50"), Some(124.5));
        assert_eq!(parse_bpm("126 BPM"), Some(126.0));
        assert_eq!(parse_bpm("unknown"), None);
        assert_eq!(parse_bpm("0"), None);
    }
}
