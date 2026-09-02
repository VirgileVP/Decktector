//! Message-type and menu-item-type constants for the dbserver protocol,
//! cross-checked against Deep Symmetry's `beat-link` (`Message.KnownType`,
//! `Message.MenuItemType`) during design.

/// Sent once per connection, right after it opens, before any query.
pub const SETUP_REQ: u16 = 0x0000;

pub const METADATA_REQ: u16 = 0x2002;
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
    fn an_unrecognized_item_type_is_preserved() {
        assert_eq!(
            MenuItemType::from_raw(0x9999),
            MenuItemType::Unknown(0x9999)
        );
    }
}
