// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ActivationResponse.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ActivationResponse.cs`; do not edit by hand

/// ACE enum `ActivationResponse` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ActivationResponse(pub i32);

#[allow(non_upper_case_globals)]
impl ActivationResponse {
    pub const Undef: Self = Self(0x0);
    pub const Use: Self = Self(0x2);
    pub const Animate: Self = Self(0x4);
    pub const Talk: Self = Self(0x10);
    pub const Emote: Self = Self(0x800);
    pub const CastSpell: Self = Self(0x1000);
    pub const Generate: Self = Self(0x10000);
}

impl ActivationResponse {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Use, Self::Animate, Self::Talk, Self::Emote, Self::CastSpell, Self::Generate];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Use", "Animate", "Talk", "Emote", "CastSpell", "Generate"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 5, 4, 6, 3, 0, 1];
}

super::support::ace_enum!(ActivationResponse, i32, flags);
super::support::ace_enum_from!(ActivationResponse, i32 => i64);
