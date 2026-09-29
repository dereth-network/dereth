// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ItemXpStyle.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ItemXpStyle.cs`; do not edit by hand

/// ACE enum `ItemXpStyle`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ItemXpStyle(pub i32);

#[allow(non_upper_case_globals)]
impl ItemXpStyle {
    pub const Undef: Self = Self(0);
    pub const Fixed: Self = Self(1);
    pub const ScalesWithLevel: Self = Self(2);
    pub const FixedPlusBase: Self = Self(3);
}

impl ItemXpStyle {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Fixed, Self::ScalesWithLevel, Self::FixedPlusBase];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Fixed", "ScalesWithLevel", "FixedPlusBase"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 3, 2, 0];
}

super::support::ace_enum!(ItemXpStyle, i32, plain);
super::support::ace_enum_from!(ItemXpStyle, i32 => i64);
