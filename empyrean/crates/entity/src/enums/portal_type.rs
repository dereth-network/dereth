// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PortalType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PortalType.cs`; do not edit by hand

/// ACE enum `PortalType` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PortalType(pub u32);

#[allow(non_upper_case_globals)]
impl PortalType {
    pub const Purple: Self = Self(0x20001B3);
    pub const Blue: Self = Self(0x20005D2);
    pub const Green: Self = Self(0x20005D3);
    pub const Orange: Self = Self(0x20005D4);
    pub const Red: Self = Self(0x20005D5);
    pub const Yellow: Self = Self(0x20005D6);
    pub const White: Self = Self(0x20006F4);
    pub const Shadow: Self = Self(0x20008FD);
    pub const Broken: Self = Self(0x2000F2E);
    pub const Destroyed: Self = Self(0x20019E4);
}

impl PortalType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Purple, Self::Blue, Self::Green, Self::Orange, Self::Red, Self::Yellow, Self::White, Self::Shadow, Self::Broken, Self::Destroyed];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Purple", "Blue", "Green", "Orange", "Red", "Yellow", "White", "Shadow", "Broken", "Destroyed"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 8, 9, 2, 3, 0, 4, 7, 6, 5];
}

super::support::ace_enum!(PortalType, u32, flags);
super::support::ace_enum_from!(PortalType, u32 => u64, i64);
