// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyAttribute.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyAttribute.cs`; do not edit by hand

/// ACE enum `PropertyAttribute`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyAttribute(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyAttribute {
    pub const Undef: Self = Self(0);
    pub const Strength: Self = Self(1);
    pub const Endurance: Self = Self(2);
    pub const Quickness: Self = Self(3);
    pub const Coordination: Self = Self(4);
    pub const Focus: Self = Self(5);
    /// `Self` in ACE; `Self` is a Rust keyword. DIVERGE: renamed.
    pub const Self_: Self = Self(6);
}

impl PropertyAttribute {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Strength, Self::Endurance, Self::Quickness, Self::Coordination, Self::Focus, Self::Self_];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Strength", "Endurance", "Quickness", "Coordination", "Focus", "Self"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 2, 5, 3, 6, 1, 0];
}

super::support::ace_enum!(PropertyAttribute, u16, plain);
super::support::ace_enum_from!(PropertyAttribute, u16 => u32, u64, i32, i64);
