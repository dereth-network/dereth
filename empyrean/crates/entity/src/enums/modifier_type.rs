// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ModifierType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ModifierType.cs`; do not edit by hand

/// ACE enum `ModifierType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ModifierType(pub i32);

#[allow(non_upper_case_globals)]
impl ModifierType {
    pub const None: Self = Self(0);
    pub const Buffed: Self = Self(1);
    pub const Debuffed: Self = Self(2);
}

impl ModifierType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Buffed, Self::Debuffed];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Buffed", "Debuffed"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(ModifierType, i32, plain);
super::support::ace_enum_from!(ModifierType, i32 => i64);
