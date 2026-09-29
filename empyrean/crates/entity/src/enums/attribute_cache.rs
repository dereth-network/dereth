// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AttributeCache.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AttributeCache.cs`; do not edit by hand

/// ACE enum `AttributeCache` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AttributeCache(pub u32);

#[allow(non_upper_case_globals)]
impl AttributeCache {
    pub const Undef: Self = Self(0x0);
    pub const Strength: Self = Self(0x1);
    pub const Endurance: Self = Self(0x2);
    pub const Quickness: Self = Self(0x4);
    pub const Coordination: Self = Self(0x8);
    pub const Focus: Self = Self(0x10);
    /// `Self` in ACE; `Self` is a Rust keyword. DIVERGE: renamed.
    pub const Self_: Self = Self(0x20);
    pub const Health: Self = Self(0x40);
    pub const Stamina: Self = Self(0x80);
    pub const Mana: Self = Self(0x100);
    pub const Full: Self = Self(0x1FF);
}

impl AttributeCache {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Strength, Self::Endurance, Self::Quickness, Self::Coordination, Self::Focus, Self::Self_, Self::Health, Self::Stamina, Self::Mana, Self::Full];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Strength", "Endurance", "Quickness", "Coordination", "Focus", "Self", "Health", "Stamina", "Mana", "Full"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 2, 5, 10, 7, 9, 3, 6, 8, 1, 0];
}

super::support::ace_enum!(AttributeCache, u32, flags);
super::support::ace_enum_from!(AttributeCache, u32 => u64, i64);
