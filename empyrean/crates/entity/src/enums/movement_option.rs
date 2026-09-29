// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MovementOption.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MovementOption.cs`; do not edit by hand

/// ACE enum `MovementOption` (`[Flags]`), underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MovementOption(pub u8);

#[allow(non_upper_case_globals)]
impl MovementOption {
    pub const None: Self = Self(0x0);
    pub const StickToObject: Self = Self(0x1);
    pub const StandingLongJump: Self = Self(0x2);
}

impl MovementOption {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::StickToObject, Self::StandingLongJump];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "StickToObject", "StandingLongJump"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 2, 1];
}

super::support::ace_enum!(MovementOption, u8, flags);
super::support::ace_enum_from!(MovementOption, u8 => u16, u32, u64, i32, i64);
