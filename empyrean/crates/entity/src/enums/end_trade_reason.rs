// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EndTradeReason.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EndTradeReason.cs`; do not edit by hand

/// ACE enum `EndTradeReason`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EndTradeReason(pub i32);

#[allow(non_upper_case_globals)]
impl EndTradeReason {
    pub const Normal: Self = Self(1);
    pub const EnteredCombat: Self = Self(2);
    pub const Canceled: Self = Self(81);
}

impl EndTradeReason {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Normal, Self::EnteredCombat, Self::Canceled];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Normal", "EnteredCombat", "Canceled"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0];
}

super::support::ace_enum!(EndTradeReason, i32, plain);
super::support::ace_enum_from!(EndTradeReason, i32 => i64);
