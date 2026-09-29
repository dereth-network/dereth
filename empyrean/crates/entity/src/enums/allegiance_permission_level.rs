// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AllegiancePermissionLevel.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AllegiancePermissionLevel.cs`; do not edit by hand

/// ACE enum `AllegiancePermissionLevel`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AllegiancePermissionLevel(pub i32);

#[allow(non_upper_case_globals)]
impl AllegiancePermissionLevel {
    pub const None: Self = Self(0);
    pub const Speaker: Self = Self(1);
    pub const Seneschal: Self = Self(2);
    pub const Castellan: Self = Self(3);
    pub const Monarch: Self = Self(4);
}

impl AllegiancePermissionLevel {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Speaker, Self::Seneschal, Self::Castellan, Self::Monarch];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Speaker", "Seneschal", "Castellan", "Monarch"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 4, 0, 2, 1];
}

super::support::ace_enum!(AllegiancePermissionLevel, i32, plain);
super::support::ace_enum_from!(AllegiancePermissionLevel, i32 => i64);
