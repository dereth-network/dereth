// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/SpellProjectileType.cs
//! Port of `Source/ACE.Server/Entity/SpellProjectileType.cs`.

/// Custom server enum (underlying `int`). Maps to `Spell.Category`.
// ACE: ProjectileSpellType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ProjectileSpellType(pub i32);

#[allow(non_upper_case_globals)]
impl ProjectileSpellType {
    pub const Undef: Self = Self(0);
    pub const Bolt: Self = Self(1);
    pub const Blast: Self = Self(2);
    pub const Volley: Self = Self(3);
    pub const Streak: Self = Self(4);
    pub const Arc: Self = Self(5);
    pub const Ring: Self = Self(6);
    pub const Wall: Self = Self(7);
    pub const Strike: Self = Self(8);

    /// Every declared member, in `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[
        Self::Undef,
        Self::Bolt,
        Self::Blast,
        Self::Volley,
        Self::Streak,
        Self::Arc,
        Self::Ring,
        Self::Wall,
        Self::Strike,
    ];
}
