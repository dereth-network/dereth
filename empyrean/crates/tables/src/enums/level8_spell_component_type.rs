// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/Level8_SpellComponentType.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/Level8_SpellComponentType.cs`; do not edit by hand

/// ACE enum `Level8_SpellComponentType`, underlying `int`.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Level8_SpellComponentType(pub i32);

#[allow(non_upper_case_globals)]
impl Level8_SpellComponentType {
    pub const Undef: Self = Self(0);
    pub const Quill: Self = Self(1);
    pub const Ink: Self = Self(2);
    pub const Glyph: Self = Self(3);
}

impl Level8_SpellComponentType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Quill, Self::Ink, Self::Glyph];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Quill", "Ink", "Glyph"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 1, 0];
}

super::support::ace_enum!(Level8_SpellComponentType, i32, plain);
super::support::ace_enum_from!(Level8_SpellComponentType, i32 => i64);
