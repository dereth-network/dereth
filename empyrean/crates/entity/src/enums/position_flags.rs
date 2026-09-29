// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PositionFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PositionFlags.cs`; do not edit by hand

/// The PositionFlags indicate the fields present in the Position structure
///
/// ACE enum `PositionFlags` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PositionFlags(pub u32);

#[allow(non_upper_case_globals)]
impl PositionFlags {
    pub const None: Self = Self(0x0);
    pub const HasVelocity: Self = Self(0x1);
    pub const HasPlacementID: Self = Self(0x2);
    pub const IsGrounded: Self = Self(0x4);
    pub const OrientationHasNoW: Self = Self(0x8);
    pub const OrientationHasNoX: Self = Self(0x10);
    pub const OrientationHasNoY: Self = Self(0x20);
    pub const OrientationHasNoZ: Self = Self(0x40);
}

impl PositionFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::HasVelocity, Self::HasPlacementID, Self::IsGrounded, Self::OrientationHasNoW, Self::OrientationHasNoX, Self::OrientationHasNoY, Self::OrientationHasNoZ];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "HasVelocity", "HasPlacementID", "IsGrounded", "OrientationHasNoW", "OrientationHasNoX", "OrientationHasNoY", "OrientationHasNoZ"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 3, 0, 4, 5, 6, 7];
}

super::support::ace_enum!(PositionFlags, u32, flags);
super::support::ace_enum_from!(PositionFlags, u32 => u64, i64);
