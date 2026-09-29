// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/TextureType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/TextureType.cs`; do not edit by hand

/// ACE enum `TextureType`, underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TextureType(pub u8);

#[allow(non_upper_case_globals)]
impl TextureType {
    pub const Undefined: Self = Self(1);
    pub const Texture2D: Self = Self(2);
    pub const Texture3D: Self = Self(3);
    pub const Cube: Self = Self(4);
    pub const Movie2D: Self = Self(5);
}

impl TextureType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undefined, Self::Texture2D, Self::Texture3D, Self::Cube, Self::Movie2D];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undefined", "Texture2D", "Texture3D", "Cube", "Movie2D"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 4, 1, 2, 0];
}

super::support::ace_enum!(TextureType, u8, plain);
super::support::ace_enum_from!(TextureType, u8 => u16, u32, u64, i32, i64);
