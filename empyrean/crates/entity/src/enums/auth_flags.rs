// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AuthFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AuthFlags.cs`; do not edit by hand

/// ACE enum `AuthFlags` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AuthFlags(pub i32);

#[allow(non_upper_case_globals)]
impl AuthFlags {
    pub const None: Self = Self(0x0);
    pub const EnableCrypto: Self = Self(0x1);
    pub const AdminAccountOverride: Self = Self(0x2);
    pub const ExtraData: Self = Self(0x4);
    pub const LastDefault: Self = Self(0x4);
}

impl AuthFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::EnableCrypto, Self::AdminAccountOverride, Self::ExtraData, Self::LastDefault];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "EnableCrypto", "AdminAccountOverride", "ExtraData", "LastDefault"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 3, 4, 0];
}

super::support::ace_enum!(AuthFlags, i32, flags);
super::support::ace_enum_from!(AuthFlags, i32 => i64);
