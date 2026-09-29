// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ParticleType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ParticleType.cs`; do not edit by hand

/// ACE enum `ParticleType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ParticleType(pub i32);

#[allow(non_upper_case_globals)]
impl ParticleType {
    pub const Unknown: Self = Self(0);
    pub const Still: Self = Self(1);
    pub const LocalVelocity: Self = Self(2);
    pub const ParabolicLVGA: Self = Self(3);
    pub const ParabolicLVGAGR: Self = Self(4);
    pub const Swarm: Self = Self(5);
    pub const Explode: Self = Self(6);
    pub const Implode: Self = Self(7);
    pub const ParabolicLVLA: Self = Self(8);
    pub const ParabolicLVLALR: Self = Self(9);
    pub const ParabolicGVGA: Self = Self(10);
    pub const ParabolicGVGAGR: Self = Self(11);
    pub const GlobalVelocity: Self = Self(12);
    pub const NumParticleType: Self = Self(13);
}

impl ParticleType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Unknown, Self::Still, Self::LocalVelocity, Self::ParabolicLVGA, Self::ParabolicLVGAGR, Self::Swarm, Self::Explode, Self::Implode, Self::ParabolicLVLA, Self::ParabolicLVLALR, Self::ParabolicGVGA, Self::ParabolicGVGAGR, Self::GlobalVelocity, Self::NumParticleType];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Unknown", "Still", "LocalVelocity", "ParabolicLVGA", "ParabolicLVGAGR", "Swarm", "Explode", "Implode", "ParabolicLVLA", "ParabolicLVLALR", "ParabolicGVGA", "ParabolicGVGAGR", "GlobalVelocity", "NumParticleType"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 12, 7, 2, 13, 10, 11, 3, 4, 8, 9, 1, 5, 0];
}

super::support::ace_enum!(ParticleType, i32, plain);
super::support::ace_enum_from!(ParticleType, i32 => i64);
