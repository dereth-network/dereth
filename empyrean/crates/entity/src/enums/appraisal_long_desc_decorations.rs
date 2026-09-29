// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AppraisalLongDescDecorations.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AppraisalLongDescDecorations.cs`; do not edit by hand

/// ACE enum `AppraisalLongDescDecorations` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AppraisalLongDescDecorations(pub i32);

#[allow(non_upper_case_globals)]
impl AppraisalLongDescDecorations {
    pub const None: Self = Self(0x0);
    pub const PrependWorkmanship: Self = Self(0x1);
    pub const PrependMaterial: Self = Self(0x2);
    pub const AppendGemInfo: Self = Self(0x4);
}

impl AppraisalLongDescDecorations {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::PrependWorkmanship, Self::PrependMaterial, Self::AppendGemInfo];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "PrependWorkmanship", "PrependMaterial", "AppendGemInfo"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 0, 2, 1];
}

super::support::ace_enum!(AppraisalLongDescDecorations, i32, flags);
super::support::ace_enum_from!(AppraisalLongDescDecorations, i32 => i64);
