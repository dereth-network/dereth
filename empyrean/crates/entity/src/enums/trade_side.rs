// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/TradeSide.cs
// @generated from ACE's `Source/ACE.Entity/Enum/TradeSide.cs`; do not edit by hand

/// ACE enum `TradeSide`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TradeSide(pub i32);

#[allow(non_upper_case_globals)]
impl TradeSide {
    /// `Self` in ACE; `Self` is a Rust keyword. DIVERGE: renamed.
    pub const Self_: Self = Self(1);
    pub const Partner: Self = Self(2);
}

impl TradeSide {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Self_, Self::Partner];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Self", "Partner"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 0];
}

super::support::ace_enum!(TradeSide, i32, plain);
super::support::ace_enum_from!(TradeSide, i32 => i64);
