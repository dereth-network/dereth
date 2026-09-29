// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CullMode.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CullMode.cs`; do not edit by hand

/// Polygon culling mode
///
/// ACE enum `CullMode`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CullMode(pub i32);

#[allow(non_upper_case_globals)]
impl CullMode {
    pub const Landblock: Self = Self(0);
    pub const None: Self = Self(1);
    pub const Clockwise: Self = Self(2);
    pub const CounterClockwise: Self = Self(3);
}

impl CullMode {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Landblock, Self::None, Self::Clockwise, Self::CounterClockwise];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Landblock", "None", "Clockwise", "CounterClockwise"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 3, 0, 1];
}

super::support::ace_enum!(CullMode, i32, plain);
super::support::ace_enum_from!(CullMode, i32 => i64);
