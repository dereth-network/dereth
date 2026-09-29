// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/UpdatePositionFlag.cs
// @generated from ACE's `Source/ACE.Entity/Enum/UpdatePositionFlag.cs`; do not edit by hand

/// this is used as a flag to tell the client what we are sending about the position of the object.
///
/// ACE enum `UpdatePositionFlag` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct UpdatePositionFlag(pub i32);

#[allow(non_upper_case_globals)]
impl UpdatePositionFlag {
    pub const None: Self = Self(0x0);
    pub const Velocity: Self = Self(0x1);
    pub const Placement: Self = Self(0x2);
    pub const Contact: Self = Self(0x4);
    pub const ZeroQw: Self = Self(0x8);
    pub const ZeroQx: Self = Self(0x10);
    pub const ZeroQy: Self = Self(0x20);
    pub const ZeroQz: Self = Self(0x40);
}

impl UpdatePositionFlag {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Velocity, Self::Placement, Self::Contact, Self::ZeroQw, Self::ZeroQx, Self::ZeroQy, Self::ZeroQz];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Velocity", "Placement", "Contact", "ZeroQw", "ZeroQx", "ZeroQy", "ZeroQz"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 0, 2, 1, 4, 5, 6, 7];
}

super::support::ace_enum!(UpdatePositionFlag, i32, flags);
super::support::ace_enum_from!(UpdatePositionFlag, i32 => i64);
