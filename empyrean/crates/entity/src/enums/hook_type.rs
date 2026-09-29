// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HookType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HookType.cs`; do not edit by hand

/// ACE enum `HookType` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HookType(pub i32);

#[allow(non_upper_case_globals)]
impl HookType {
    pub const Undef: Self = Self(0x0);
    pub const Floor: Self = Self(0x1);
    pub const Wall: Self = Self(0x2);
    pub const Ceiling: Self = Self(0x4);
    pub const Yard: Self = Self(0x8);
    pub const Roof: Self = Self(0x10);
}

impl HookType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Floor, Self::Wall, Self::Ceiling, Self::Yard, Self::Roof];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Floor", "Wall", "Ceiling", "Yard", "Roof"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 1, 5, 0, 2, 4];
}

super::support::ace_enum!(HookType, i32, flags);
super::support::ace_enum_from!(HookType, i32 => i64);
