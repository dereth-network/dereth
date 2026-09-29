// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CharacterOptionDataFlag.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CharacterOptionDataFlag.cs`; do not edit by hand

/// ACE enum `CharacterOptionDataFlag` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CharacterOptionDataFlag(pub u32);

#[allow(non_upper_case_globals)]
impl CharacterOptionDataFlag {
    pub const Shortcut: Self = Self(0x1);
    pub const SquelchList: Self = Self(0x2);
    pub const MultiSpellList: Self = Self(0x4);
    pub const DesiredComps: Self = Self(0x8);
    pub const ExtendedMultiSpellLists: Self = Self(0x10);
    pub const SpellbookFilters: Self = Self(0x20);
    pub const CharacterOptions2: Self = Self(0x40);
    pub const TimestampFormat: Self = Self(0x80);
    pub const GenericQualitiesData: Self = Self(0x100);
    pub const GameplayOptions: Self = Self(0x200);
    pub const SpellLists8: Self = Self(0x400);
}

impl CharacterOptionDataFlag {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Shortcut, Self::SquelchList, Self::MultiSpellList, Self::DesiredComps, Self::ExtendedMultiSpellLists, Self::SpellbookFilters, Self::CharacterOptions2, Self::TimestampFormat, Self::GenericQualitiesData, Self::GameplayOptions, Self::SpellLists8];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Shortcut", "SquelchList", "MultiSpellList", "DesiredComps", "ExtendedMultiSpellLists", "SpellbookFilters", "CharacterOptions2", "TimestampFormat", "GenericQualitiesData", "GameplayOptions", "SpellLists8"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 3, 4, 9, 8, 2, 0, 10, 5, 1, 7];
}

super::support::ace_enum!(CharacterOptionDataFlag, u32, flags);
super::support::ace_enum_from!(CharacterOptionDataFlag, u32 => u64, i64);
