// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AiOption.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AiOption.cs`; do not edit by hand

/// Determines optional AI abilities
///
/// ACE enum `AiOption` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AiOption(pub i32);

#[allow(non_upper_case_globals)]
impl AiOption {
    pub const None: Self = Self(0x0);
    pub const CanOpenDoors: Self = Self(0x1);
}

impl AiOption {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::CanOpenDoors];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "CanOpenDoors"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 0];
}

super::support::ace_enum!(AiOption, i32, flags);
super::support::ace_enum_from!(AiOption, i32 => i64);
