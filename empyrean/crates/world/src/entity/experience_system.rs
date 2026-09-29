// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/ExperienceSystem.cs
//! Port of `Source/ACE.Server/Entity/ExperienceSystem.cs`.
//!
//! Item levelling curves. **Retail's, not ACE's (V379)**: both members
//! keep ACE's names and anchors and compute through the one shared implementation,
//! `dereth_rules::advancement`, which the client uses for the level it shows. For the two styles
//! content uses (Fixed, ScalesWithLevel) nothing changes in range; FixedPlusBase is retail's
//! triangular curve instead of ACE's linear one, Undef is 0 both ways, a zero base no longer loops
//! forever, and out-of-range quotients wrap as the client's do.
use empyrean_entity::enums::ItemXpStyle;

// ACE: ExperienceSystem.ItemLevelToTotalXP
/// The cumulative XP an item needs to reach `item_level` (clamped to `max_level`).
#[must_use]
pub fn item_level_to_total_xp(
    item_level: i32,
    base_xp: u64,
    max_level: i32,
    xp_scheme: ItemXpStyle,
) -> u64 {
    dereth_rules::advancement::item_level_to_total_xp(item_level, base_xp, max_level, xp_scheme.0)
}

// ACE: ExperienceSystem.ItemTotalXPToLevel
/// The level an item with `gained_xp` has reached (clamped to `max_level`).
#[must_use]
pub fn item_total_xp_to_level(
    gained_xp: u64,
    base_xp: u64,
    max_level: i32,
    xp_scheme: ItemXpStyle,
) -> i32 {
    dereth_rules::advancement::item_total_xp_to_level(gained_xp, base_xp, max_level, xp_scheme.0)
}
