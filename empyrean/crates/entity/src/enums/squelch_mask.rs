// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SquelchMask.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SquelchMask.cs`; do not edit by hand

/// ACE enum `SquelchMask` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SquelchMask(pub u32);

#[allow(non_upper_case_globals)]
impl SquelchMask {
    pub const None: Self = Self(0x0);
    pub const Speech: Self = Self(0x4);
    pub const Tell: Self = Self(0x8);
    pub const Combat: Self = Self(0x40);
    pub const Magic: Self = Self(0x80);
    pub const Emote: Self = Self(0x1000);
    pub const Appraisal: Self = Self(0x10000);
    pub const Spellcasting: Self = Self(0x20000);
    pub const Allegiance: Self = Self(0x40000);
    pub const Fellowship: Self = Self(0x80000);
    pub const CombatEnemy: Self = Self(0x200000);
    pub const CombatSelf: Self = Self(0x400000);
    pub const Recall: Self = Self(0x800000);
    pub const Craft: Self = Self(0x1000000);
    pub const Salvaging: Self = Self(0x2000000);
    pub const Combined: Self = Self(0x3EF10CC);
    pub const AllChannels: Self = Self(0xFFFFFFFF);
}

impl SquelchMask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Speech, Self::Tell, Self::Combat, Self::Magic, Self::Emote, Self::Appraisal, Self::Spellcasting, Self::Allegiance, Self::Fellowship, Self::CombatEnemy, Self::CombatSelf, Self::Recall, Self::Craft, Self::Salvaging, Self::Combined, Self::AllChannels];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Speech", "Tell", "Combat", "Magic", "Emote", "Appraisal", "Spellcasting", "Allegiance", "Fellowship", "CombatEnemy", "CombatSelf", "Recall", "Craft", "Salvaging", "Combined", "AllChannels"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[16, 8, 6, 3, 10, 11, 15, 13, 5, 9, 4, 0, 12, 14, 1, 7, 2];
}

super::support::ace_enum!(SquelchMask, u32, flags);
super::support::ace_enum_from!(SquelchMask, u32 => u64, i64);
