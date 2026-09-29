// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RadarBehavior.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RadarBehavior.cs`; do not edit by hand

/// ACE enum `RadarBehavior`, underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RadarBehavior(pub u8);

#[allow(non_upper_case_globals)]
impl RadarBehavior {
    pub const Undefined: Self = Self(0);
    pub const ShowNever: Self = Self(1);
    pub const ShowMovement: Self = Self(2);
    pub const ShowAttacking: Self = Self(3);
    pub const ShowAlways: Self = Self(4);
}

impl RadarBehavior {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undefined, Self::ShowNever, Self::ShowMovement, Self::ShowAttacking, Self::ShowAlways];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undefined", "ShowNever", "ShowMovement", "ShowAttacking", "ShowAlways"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 3, 2, 1, 0];
}

super::support::ace_enum!(RadarBehavior, u8, plain);
super::support::ace_enum_from!(RadarBehavior, u8 => u16, u32, u64, i32, i64);
