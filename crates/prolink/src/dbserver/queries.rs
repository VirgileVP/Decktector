//! Message-type and menu-item-type constants for the dbserver protocol,
//! cross-checked against Deep Symmetry's `beat-link` (`Message.KnownType`,
//! `Message.MenuItemType`) during design.

use crate::status::{TRACK_TYPE_REKORDBOX, TRACK_TYPE_STREAMING};

/// Sent once per connection, right after it opens, before any query.
pub const SETUP_REQ: u16 = 0x0000;

/// Metadata of a track listed in the media's rekordbox database, or of a
/// streamed track.
pub const METADATA_REQ: u16 = 0x2002;
/// Metadata of any other track: a file loaded from the player's file browser
/// without rekordbox analysis, or an audio CD track. A player answers
/// [`METADATA_REQ`] for such a track with an empty menu.
pub const UNANALYZED_METADATA_REQ: u16 = 0x2202;
pub const ARTWORK_REQ: u16 = 0x2003;
pub const BEAT_GRID_REQ: u16 = 0x2204;
/// Basic cue list — predates the extended format, fewer fields (no color,
/// no comment). Tried only if [`CUE_LIST_EXT_REQ`] doesn't work.
pub const CUE_LIST_REQ: u16 = 0x2104;
/// Extended cue list (Nexus2+) — carries color and comment; tried first.
pub const CUE_LIST_EXT_REQ: u16 = 0x2b04;
/// Generic "give me this raw ANLZ tag" passthrough. Only the waveform fetch
/// in this codebase uses it — beat grid and cue list are older, dedicated
/// query types that were never migrated to this generic form.
pub const ANLZ_TAG_REQ: u16 = 0x2c04;
pub const RENDER_MENU_REQ: u16 = 0x3000;

pub const MENU_AVAILABLE: u16 = 0x4000;
pub const ARTWORK_RESPONSE: u16 = 0x4002;
pub const MENU_HEADER: u16 = 0x4001;
pub const MENU_ITEM: u16 = 0x4101;
pub const MENU_FOOTER: u16 = 0x4201;
pub const BEAT_GRID_RESPONSE: u16 = 0x4602;
pub const CUE_LIST_RESPONSE: u16 = 0x4702;
pub const CUE_LIST_EXT_RESPONSE: u16 = 0x4e02;
pub const ANLZ_TAG_RESPONSE: u16 = 0x4f02;

/// The metadata request a track of `track_type` (a status packet's
/// `DeviceStatus::track_type`) has to be asked about with — the same split
/// `beat-link`'s `MetadataFinder.queryMetadata` makes.
pub fn metadata_request_for(track_type: u8) -> u16 {
    match track_type {
        TRACK_TYPE_REKORDBOX | TRACK_TYPE_STREAMING => METADATA_REQ,
        _ => UNANALYZED_METADATA_REQ,
    }
}

/// `MenuTarget.Main` — the only menu target track metadata lookups use.
pub const MENU_TARGET_MAIN: u8 = 1;

/// Discriminant carried as the 7th argument of a [`MENU_ITEM`] message,
/// naming which track field that item holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItemType {
    AlbumTitle,
    TrackTitle,
    Artist,
    Genre,
    Label,
    Key,
    Comment,
    Duration,
    Tempo,
    Unknown(u32),
}

impl MenuItemType {
    pub fn from_raw(value: u32) -> Self {
        match value {
            0x0002 => Self::AlbumTitle,
            0x0004 => Self::TrackTitle,
            0x0007 => Self::Artist,
            0x0006 => Self::Genre,
            0x000e => Self::Label,
            0x000f => Self::Key,
            0x0023 => Self::Comment,
            0x000b => Self::Duration,
            0x000d => Self::Tempo,
            other => Self::Unknown(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_every_field_this_codebase_reads() {
        assert_eq!(MenuItemType::from_raw(0x0004), MenuItemType::TrackTitle);
        assert_eq!(MenuItemType::from_raw(0x000d), MenuItemType::Tempo);
    }

    #[test]
    fn a_rekordbox_track_is_asked_about_with_the_rekordbox_metadata_request() {
        assert_eq!(metadata_request_for(TRACK_TYPE_REKORDBOX), METADATA_REQ);
    }

    /// Streamed tracks were asked about with the rekordbox request before
    /// the unanalysed split, and must stay that way.
    #[test]
    fn a_streamed_track_is_asked_about_with_the_rekordbox_metadata_request() {
        assert_eq!(metadata_request_for(TRACK_TYPE_STREAMING), METADATA_REQ);
    }

    /// An unanalysed USB file asked about with the rekordbox request got an
    /// empty menu back: the deck showed no title, no waveform, nothing.
    #[test]
    fn other_track_kinds_are_asked_about_with_the_unanalyzed_metadata_request() {
        const UNANALYSED_FILE: u8 = 0x02;
        const AUDIO_CD: u8 = 0x05;
        assert_eq!(
            metadata_request_for(UNANALYSED_FILE),
            UNANALYZED_METADATA_REQ
        );
        assert_eq!(metadata_request_for(AUDIO_CD), UNANALYZED_METADATA_REQ);
    }

    #[test]
    fn an_unrecognized_item_type_is_preserved() {
        assert_eq!(
            MenuItemType::from_raw(0x9999),
            MenuItemType::Unknown(0x9999)
        );
    }
}
