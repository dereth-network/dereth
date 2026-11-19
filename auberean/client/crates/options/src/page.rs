//! What an options page asks of the client's own preferences, and how they are listed for it.
//!
//! A page attaches the sections it shows and is listed each one's preferences ([`Listing`]), and
//! again whenever they change; what the player sets is tried at once ([`Request::Change`]); a
//! section's preferences are submitted, applied, kept as they are or put back; the preferences are
//! kept in their file ([`Request::Save`]).

use crate::preference;

/// The client's preferences' sections.
pub mod section {
    pub const GRAPHICS: u32 = 0x4000_0004;
    pub const SOUND: u32 = 0x4000_0007;
    pub const CAMERA: u32 = 0x4000_0008;
    pub const EFFECTS: u32 = 0x4000_0009;
    pub const RENDERING: u32 = 0x4000_000B;
    pub const OTHER: u32 = 0x4000_000C;
    pub const AVATARS: u32 = 0x4000_000D;
    pub const INPUT: u32 = 0x4000_000E;
    /// The interface: this client's own section, which the data's enumeration of sections doesn't
    /// name.
    pub const UI: u32 = 0x4100_7E02;
    /// The orbit camera's: this client's own section, listed by an interface that offers the
    /// orbit camera.
    pub const ORBIT: u32 = 0x4100_7E05;
}

/// The string table the client's preference labels are in.
pub const PREFERENCE_STRINGS: u32 = 0x4000_0008;

/// A preference the player set on a row, before it is applied.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Flag(preference::Flag),
    Number(preference::Number),
    Choice(preference::Choice),
}

/// What an options page asks of the client's preferences.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// The sections the page shows: list each one's preferences now and whenever they change.
    Attach(Vec<u32>),
    /// The page goes: no longer.
    Detach(Vec<u32>),
    /// Try a preference the player set.
    Change(Change),
    /// A section's preferences taken into effect (the rendering's submitted, to be kept or put
    /// back), applied, kept as they are now (synchronized) or put back (recalled).
    Submit(u32),
    Apply(u32),
    Synchronize(u32),
    Recall(u32),
    /// The preferences kept in the preferences' file.
    Save,
}

/// The client's preferences as listed for a page: each section's, in its order, and a count that
/// grows with every listing (the page takes a listing in once).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    pub serial: u32,
    pub sections: Vec<(u32, Vec<Change>)>,
}
