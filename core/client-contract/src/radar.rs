//! The radar's two bit vocabularies and its range.
//!
//! `dereth_ui_screens::mapradar::radar` draws the radar; these three items are what the *world* half
//! of the client has to agree with it about, and `dereth_client::{hud, interaction}` read all three
//! (the masks are taken from the radar module rather than restated, so the seam and the rules
//! that consume it can never disagree about a mask). They are plain
//! numbers and one branch, so they come down here and the agreement is kept without
//! `dereth-client` naming a crate that draws.
//!
//! `dereth_ui_screens::mapradar::radar::{bitfield, radar_enum, radar_range}` resolve through
//! `pub use` lines.

/// The object-description bitfield values read by the radar.
///
/// Their exact meanings remain unresolved;
/// the radar is one of the few readers of them, and it only needs the numbers.
pub mod bitfield {
    /// `ObjectDescriptionFlag::UiHidden` — the flag all three radar entry points test before they
    /// will look at an object at all.
    ///
    /// **This is `0x80`, not `0x8000_0000`,** although it is sometimes called "the high bit". All
    /// three call sites truncate to a **`char`** before testing the sign — the radar's add-object,
    /// blip-colour and blip-shape entry points all test the sign of the bitfield's low byte — so
    /// what is tested is bit 7 of the low byte. The public-description flags name
    /// bit 7 `UiHidden` ("hidden") and define **no** flag at bit 31 at all, so a bit-31 mask could
    /// never fire on real traffic. `\[verified\]`
    ///
    /// It is *not* `RadarEnum::ShowNever`: that lives in `_radar_enum` and is tested separately, by
    /// `inq_showable_on_radar`.
    pub const HIDDEN: u32 = 0x0000_0080;
    /// `ObjectDescriptionFlag::Attackable`. The blip colour's creature branch needs this bit
    /// **and** whether the object is a creature (the creature bit of its item type); these are
    /// different words that happen to share the mask `0x10`.
    pub const ATTACKABLE: u32 = 0x0000_0010;
    /// An older name for [`ATTACKABLE`], kept so nothing that reads it has to change.
    pub const CREATURE: u32 = 0x0000_0010;
    /// `ObjectDescriptionFlag::Player` — the client's is-player test is
    /// `_bitfield >> 3 & 1`. `\[verified\]`
    pub const PLAYER: u32 = 0x0000_0008;
    /// `ObjectDescriptionFlag::PlayerKiller` — the client's is-PK test is
    /// `_bitfield >> 5 & 1`. `\[verified\]`
    pub const PK: u32 = 0x0000_0020;
    /// `ObjectDescriptionFlag::PkLiteStatus` — the client's is-PK-lite test is
    /// `_bitfield >> 0x19 & 1`. `\[verified\]`
    pub const PK_LITE: u32 = 0x0200_0000;
    /// Cleared alongside `0x100000` to make an Admin blip.
    pub const NOT_ADMIN: u32 = 0x0000_0040;
    /// Vendor.
    pub const VENDOR: u32 = 0x0000_0200;
    /// Portal.
    pub const PORTAL: u32 = 0x0004_0000;
    /// Admin, when `NOT_ADMIN` is clear.
    pub const ADMIN: u32 = 0x0010_0000;
    /// The player-side "creature" bit that makes a player blip gold.
    pub const PLAYER_CREATURE: u32 = 0x0020_0000;
}

/// The radar visibility values carried by an object description.
pub mod radar_enum {
    /// 0 — the value an object gets when the server sent no `RADAR_ENUM` field at all. **Not
    /// shown.**
    pub const UNDEF: u8 = 0;
    /// 1 — explicitly suppressed. Not shown.
    pub const SHOW_NEVER: u8 = 1;
    /// 2. Shown.
    pub const SHOW_MOVEMENT: u8 = 2;
    /// 3. Shown.
    pub const SHOW_ATTACKING: u8 = 3;
    /// 4. Shown.
    pub const SHOW_ALWAYS: u8 = 4;
}

/// The radar's world range: 75 units outdoors, 25 indoors. The pixel radius does
/// not change, only the scale.
#[must_use]
pub fn radar_range(outside: bool) -> f32 {
    if outside {
        75.0
    } else {
        25.0
    }
}
