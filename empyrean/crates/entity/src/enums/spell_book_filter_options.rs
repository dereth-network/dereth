// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SpellBookFilterOptions.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SpellBookFilterOptions.cs`; do not edit by hand

/// The various options for filtering the spellbook
///
/// ACE enum `SpellBookFilterOptions`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SpellBookFilterOptions(pub u32);

#[allow(non_upper_case_globals)]
impl SpellBookFilterOptions {
    pub const None: Self = Self(0);
    pub const Creature: Self = Self(1);
    pub const Item: Self = Self(2);
    pub const Life: Self = Self(4);
    pub const War: Self = Self(8);
    pub const Level1: Self = Self(16);
    pub const Level2: Self = Self(32);
    pub const Level3: Self = Self(64);
    pub const Level4: Self = Self(128);
    pub const Level5: Self = Self(256);
    pub const Level6: Self = Self(512);
    pub const Level7: Self = Self(1024);
    pub const Level8: Self = Self(2048);
    pub const Level9: Self = Self(4096);
    pub const Void: Self = Self(8192);
}

impl SpellBookFilterOptions {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Creature, Self::Item, Self::Life, Self::War, Self::Level1, Self::Level2, Self::Level3, Self::Level4, Self::Level5, Self::Level6, Self::Level7, Self::Level8, Self::Level9, Self::Void];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Creature", "Item", "Life", "War", "Level1", "Level2", "Level3", "Level4", "Level5", "Level6", "Level7", "Level8", "Level9", "Void"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 5, 6, 7, 8, 9, 10, 11, 12, 13, 3, 0, 14, 4];
}

super::support::ace_enum!(SpellBookFilterOptions, u32, plain);
super::support::ace_enum_from!(SpellBookFilterOptions, u32 => u64, i64);
