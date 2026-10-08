//! Shared radar visibility, semantic roles, projection and world range.

use crate::{options::interface::Interface, view::RadarEntry};
use dereth_primitives::num::{math, to_i32};

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
    /// never fire on real traffic.
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
    /// `_bitfield >> 3 & 1`.
    pub const PLAYER: u32 = 0x0000_0008;
    /// `ObjectDescriptionFlag::PlayerKiller` — the client's is-PK test is
    /// `_bitfield >> 5 & 1`.
    pub const PK: u32 = 0x0000_0020;
    /// `ObjectDescriptionFlag::PkLiteStatus` — the client's is-PK-lite test is
    /// `_bitfield >> 0x19 & 1`.
    pub const PK_LITE: u32 = 0x0200_0000;
    /// Cleared alongside `0x100000` to make an Admin blip.
    pub const NOT_ADMIN: u32 = 0x0000_0040;
    /// Life stone, used by the Classic color policy.
    pub const LIFE_STONE: u32 = 0x0000_4000;
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

/// The meaning of a blip's color, before the interface chooses its palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRole {
    Default,
    Override(u8),
    Portal,
    LifeStone,
    Vendor,
    Creature,
    Admin,
    PlayerKiller,
    PkLite,
    Fellowship,
}
impl ColorRole {
    /// The shared numbered palette vocabulary. Unknown explicit indices remain explicit.
    #[must_use]
    pub const fn index(self) -> u8 {
        match self {
            Self::Default => 3,
            Self::Override(index) => index,
            Self::Portal => 4,
            Self::LifeStone => 1,
            Self::Vendor => 8,
            Self::Creature => 2,
            Self::Admin => 9,
            Self::PlayerKiller => 5,
            Self::PkLite => 6,
            Self::Fellowship => 10,
        }
    }
}

/// Explicit colors precede object roles; fellowship overlays the player's base role.
#[must_use]
pub fn color_role(entry: Option<&RadarEntry>, interface: Interface) -> ColorRole {
    let Some(o) = entry else {
        return ColorRole::Default;
    };
    if o.bitfield & bitfield::HIDDEN != 0 {
        return ColorRole::Default;
    }
    if o.blip_color != 0 {
        return ColorRole::Override(o.blip_color);
    }
    if o.bitfield & bitfield::PORTAL != 0 {
        return ColorRole::Portal;
    }
    if interface == Interface::Classic && o.bitfield & bitfield::LIFE_STONE != 0 {
        return ColorRole::LifeStone;
    }
    if o.bitfield & bitfield::VENDOR != 0 {
        return ColorRole::Vendor;
    }
    if o.bitfield & bitfield::ATTACKABLE != 0 && o.is_attackable && !o.is_player {
        return ColorRole::Creature;
    }
    if !o.is_player {
        return ColorRole::Default;
    }
    if o.is_fellow || o.is_fellowship_leader {
        return ColorRole::Fellowship;
    }
    if o.bitfield & bitfield::ADMIN != 0 && o.bitfield & bitfield::NOT_ADMIN == 0 {
        ColorRole::Admin
    } else if o.is_pk {
        ColorRole::PlayerKiller
    } else if o.is_pk_lite {
        ColorRole::PkLite
    } else if o.bitfield & bitfield::PLAYER_CREATURE != 0 {
        ColorRole::Creature
    } else {
        ColorRole::Default
    }
}

/// The relationship represented by a shape; its pixels belong to the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeRole {
    Hidden,
    Ordinary,
    Allegiance,
    Threat,
    FellowshipLeader,
    Fellowship,
}

/// The local player's two threat flags, absent before its description is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub pk: bool,
    pub pk_lite: bool,
}

#[must_use]
pub fn shape_role(
    entry: Option<&RadarEntry>,
    viewer: Option<Viewer>,
    interface: Interface,
) -> ShapeRole {
    let Some(o) = entry else {
        return ShapeRole::Hidden;
    };
    if o.bitfield & bitfield::HIDDEN != 0 {
        return ShapeRole::Hidden;
    }
    if interface == Interface::Classic && !o.is_player {
        return ShapeRole::Ordinary;
    }
    if o.is_fellow {
        return if o.is_fellowship_leader {
            ShapeRole::FellowshipLeader
        } else {
            ShapeRole::Fellowship
        };
    }
    if (interface == Interface::Classic || viewer.is_some()) && o.is_allegiance_member {
        return ShapeRole::Allegiance;
    }
    if viewer.is_some_and(|p| (o.is_pk && p.pk) || (o.is_pk_lite && p.pk_lite)) {
        return ShapeRole::Threat;
    }
    ShapeRole::Ordinary
}

/// Physics presence and the server's visibility enumeration are independent of hidden/self flags.
#[must_use]
pub fn showable(entry: &RadarEntry) -> bool {
    entry.in_world
        && matches!(
            entry.radar_enum,
            radar_enum::SHOW_MOVEMENT | radar_enum::SHOW_ATTACKING | radar_enum::SHOW_ALWAYS
        )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub radius: i32,
    pub center: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Projection {
    pub x: i32,
    pub y: i32,
    pub bright: bool,
}

pub const DIM_HEIGHT: f32 = 5.0;
pub const DIM_FACTOR: f32 = 0.65;

/// Filter and project one candidate. Both drawing and picking consume these surviving pixels.
#[must_use]
#[allow(clippy::cast_precision_loss)] // Pixel dimensions are exactly representable.
pub fn project(
    entry: &RadarEntry,
    geometry: Geometry,
    range: f32,
    interface: Interface,
) -> Option<Projection> {
    if entry.is_self || !showable(entry) || entry.bitfield & bitfield::HIDDEN != 0 {
        return None;
    }
    let (px, py, pz) = entry.player_space;
    let (x, y) = match interface {
        Interface::Classic => {
            if !px.is_finite() || !py.is_finite() || math::hypotf(px, py) >= range - 1.0 {
                return None;
            }
            (
                to_i32(geometry.center.0) + to_i32(px * geometry.radius as f32 / range),
                to_i32(geometry.center.1) - to_i32(py * geometry.radius as f32 / range),
            )
        }
        Interface::Modern | Interface::Horizon => {
            let range_sq = (range - 1.0) * (range - 1.0);
            if px * px + py * py >= range_sq {
                return None;
            }
            let scale = geometry.radius as f32 / range;
            let x = to_i32(px * scale + geometry.center.0);
            let y = to_i32(geometry.center.1 - py * scale);
            let cx = to_i32(geometry.center.0);
            let cy = to_i32(geometry.center.1);
            if x < cx - geometry.radius
                || x > cx + geometry.radius
                || y < cy - geometry.radius
                || y > cy + geometry.radius
            {
                return None;
            }
            (x, y)
        }
    };
    Some(Projection {
        x,
        y,
        bright: pz.abs() < DIM_HEIGHT,
    })
}

#[cfg(test)]
mod tests;
