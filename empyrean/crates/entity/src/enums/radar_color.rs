// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RadarColor.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RadarColor.cs`; do not edit by hand

/// ACE enum `RadarColor`, underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RadarColor(pub u8);

#[allow(non_upper_case_globals)]
impl RadarColor {
    pub const Default: Self = Self(0);
    pub const Blue: Self = Self(1);
    pub const Gold: Self = Self(2);
    pub const White: Self = Self(3);
    pub const Purple: Self = Self(4);
    pub const Red: Self = Self(5);
    pub const Pink: Self = Self(6);
    pub const Green: Self = Self(7);
    pub const Yellow: Self = Self(8);
    pub const Cyan: Self = Self(9);
    pub const BrightGreen: Self = Self(16);
    pub const Admin: Self = Self(9);
    pub const Advocate: Self = Self(6);
    pub const Creature: Self = Self(2);
    pub const LifeStone: Self = Self(1);
    pub const NPC: Self = Self(8);
    pub const PlayerKiller: Self = Self(5);
    pub const Portal: Self = Self(4);
    pub const Sentinel: Self = Self(9);
    pub const Vendor: Self = Self(8);
    pub const Fellowship: Self = Self(16);
    pub const FellowshipLeader: Self = Self(16);
    pub const PKLite: Self = Self(6);
}

impl RadarColor {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Default, Self::Blue, Self::LifeStone, Self::Gold, Self::Creature, Self::White, Self::Purple, Self::Portal, Self::Red, Self::PlayerKiller, Self::Advocate, Self::PKLite, Self::Pink, Self::Green, Self::Yellow, Self::NPC, Self::Vendor, Self::Cyan, Self::Sentinel, Self::Admin, Self::BrightGreen, Self::Fellowship, Self::FellowshipLeader];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Default", "Blue", "LifeStone", "Gold", "Creature", "White", "Purple", "Portal", "Red", "PlayerKiller", "Advocate", "PKLite", "Pink", "Green", "Yellow", "NPC", "Vendor", "Cyan", "Sentinel", "Admin", "BrightGreen", "Fellowship", "FellowshipLeader"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[19, 10, 1, 20, 4, 17, 0, 21, 22, 3, 13, 2, 15, 11, 12, 9, 7, 6, 8, 18, 16, 5, 14];
}

super::support::ace_enum!(RadarColor, u8, plain);
super::support::ace_enum_from!(RadarColor, u8 => u16, u32, u64, i32, i64);
