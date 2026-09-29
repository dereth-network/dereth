// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SurfaceType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SurfaceType.cs`; do not edit by hand

/// ACE enum `SurfaceType` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SurfaceType(pub u32);

#[allow(non_upper_case_globals)]
impl SurfaceType {
    pub const Base1Solid: Self = Self(0x1);
    pub const Base1Image: Self = Self(0x2);
    pub const Base1ClipMap: Self = Self(0x4);
    pub const Translucent: Self = Self(0x10);
    pub const Diffuse: Self = Self(0x20);
    pub const Luminous: Self = Self(0x40);
    pub const Alpha: Self = Self(0x100);
    pub const InvAlpha: Self = Self(0x200);
    pub const Additive: Self = Self(0x10000);
    pub const Detail: Self = Self(0x20000);
    pub const Gouraud: Self = Self(0x10000000);
    pub const Stippled: Self = Self(0x40000000);
    pub const Perspective: Self = Self(0x80000000);
}

impl SurfaceType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Base1Solid, Self::Base1Image, Self::Base1ClipMap, Self::Translucent, Self::Diffuse, Self::Luminous, Self::Alpha, Self::InvAlpha, Self::Additive, Self::Detail, Self::Gouraud, Self::Stippled, Self::Perspective];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Base1Solid", "Base1Image", "Base1ClipMap", "Translucent", "Diffuse", "Luminous", "Alpha", "InvAlpha", "Additive", "Detail", "Gouraud", "Stippled", "Perspective"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[8, 6, 2, 1, 0, 9, 4, 10, 7, 5, 12, 11, 3];
}

super::support::ace_enum!(SurfaceType, u32, flags);
super::support::ace_enum_from!(SurfaceType, u32 => u64, i64);
