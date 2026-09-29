// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/GfxObjFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/GfxObjFlags.cs`; do not edit by hand

/// ACE enum `GfxObjFlags` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct GfxObjFlags(pub u32);

#[allow(non_upper_case_globals)]
impl GfxObjFlags {
    pub const HasPhysics: Self = Self(0x1);
    pub const HasDrawing: Self = Self(0x2);
    pub const Unknown: Self = Self(0x4);
    pub const HasDIDDegrade: Self = Self(0x8);
}

impl GfxObjFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::HasPhysics, Self::HasDrawing, Self::Unknown, Self::HasDIDDegrade];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["HasPhysics", "HasDrawing", "Unknown", "HasDIDDegrade"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 1, 0, 2];
}

super::support::ace_enum!(GfxObjFlags, u32, flags);
super::support::ace_enum_from!(GfxObjFlags, u32 => u64, i64);
