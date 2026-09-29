// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SetupConst.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SetupConst.cs`; do not edit by hand

/// ACE enum `SetupConst`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SetupConst(pub u32);

#[allow(non_upper_case_globals)]
impl SetupConst {
    pub const HumanMale: Self = Self(0x2000001);
    pub const HumanFemale: Self = Self(0x200004E);
    pub const UndeadMaleUndead: Self = Self(0x2001A0E);
    pub const UndeadMaleUndeadGen: Self = Self(0x2001A0F);
    pub const UndeadMaleSkeleton: Self = Self(0x2001A9C);
    pub const UndeadMaleSkeletonNoFlame: Self = Self(0x2001A9E);
    pub const UndeadMaleZombie: Self = Self(0x2001A9D);
    pub const UndeadMaleZombieNoFlame: Self = Self(0x2001A96);
    pub const UndeadFemaleUndead: Self = Self(0x2001A0C);
    pub const UndeadFemaleUndeadGen: Self = Self(0x2001A0D);
    pub const UndeadFemaleSkeleton: Self = Self(0x2001AA0);
    pub const UndeadFemaleSkeletonNoFlame: Self = Self(0x2001A9F);
    pub const UndeadFemaleZombie: Self = Self(0x2001AA1);
    pub const UndeadFemaleZombieNoFlame: Self = Self(0x2001AA2);
    pub const UmbraenMaleCrown: Self = Self(0x200196F);
    pub const UmbraenMaleCrownGen: Self = Self(0x2001972);
    pub const UmbraenMaleNoCrown: Self = Self(0x2001A5F);
    pub const UmbraenMaleVoid: Self = Self(0x2001A6F);
    pub const UmbraenFemaleCrown: Self = Self(0x2001970);
    pub const UmbraenFemaleCrownGen: Self = Self(0x2001970);
    pub const UmbraenFemaleNoCrown: Self = Self(0x2001A5E);
    pub const UmbraenFemaleVoid: Self = Self(0x2001A6EE);
    pub const PenumbraenMaleCrown: Self = Self(0x200196E);
    pub const PenumbraenMaleCrownGen: Self = Self(0x2001971);
    pub const PenumbraenMaleNoCrown: Self = Self(0x2001A5D);
    pub const PenumbraenMaleVoid: Self = Self(0x2001A70);
    pub const PenumbraenFemaleCrown: Self = Self(0x200196D);
    pub const PenumbraenFemaleCrownGen: Self = Self(0x200196D);
    pub const PenumbraenFemaleNoCrown: Self = Self(0x2001A5C);
    pub const PenumbraenFemaleVoid: Self = Self(0x2001A71);
    pub const AnakshayMale: Self = Self(0x2001AA3);
    pub const AnakshayFemale: Self = Self(0x2001AA4);
}

impl SetupConst {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::HumanMale, Self::HumanFemale, Self::PenumbraenFemaleCrownGen, Self::PenumbraenFemaleCrown, Self::PenumbraenMaleCrown, Self::UmbraenMaleCrown, Self::UmbraenFemaleCrownGen, Self::UmbraenFemaleCrown, Self::PenumbraenMaleCrownGen, Self::UmbraenMaleCrownGen, Self::UndeadFemaleUndead, Self::UndeadFemaleUndeadGen, Self::UndeadMaleUndead, Self::UndeadMaleUndeadGen, Self::PenumbraenFemaleNoCrown, Self::PenumbraenMaleNoCrown, Self::UmbraenFemaleNoCrown, Self::UmbraenMaleNoCrown, Self::UmbraenMaleVoid, Self::PenumbraenMaleVoid, Self::PenumbraenFemaleVoid, Self::UndeadMaleZombieNoFlame, Self::UndeadMaleSkeleton, Self::UndeadMaleZombie, Self::UndeadMaleSkeletonNoFlame, Self::UndeadFemaleSkeletonNoFlame, Self::UndeadFemaleSkeleton, Self::UndeadFemaleZombie, Self::UndeadFemaleZombieNoFlame, Self::AnakshayMale, Self::AnakshayFemale, Self::UmbraenFemaleVoid];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["HumanMale", "HumanFemale", "PenumbraenFemaleCrownGen", "PenumbraenFemaleCrown", "PenumbraenMaleCrown", "UmbraenMaleCrown", "UmbraenFemaleCrownGen", "UmbraenFemaleCrown", "PenumbraenMaleCrownGen", "UmbraenMaleCrownGen", "UndeadFemaleUndead", "UndeadFemaleUndeadGen", "UndeadMaleUndead", "UndeadMaleUndeadGen", "PenumbraenFemaleNoCrown", "PenumbraenMaleNoCrown", "UmbraenFemaleNoCrown", "UmbraenMaleNoCrown", "UmbraenMaleVoid", "PenumbraenMaleVoid", "PenumbraenFemaleVoid", "UndeadMaleZombieNoFlame", "UndeadMaleSkeleton", "UndeadMaleZombie", "UndeadMaleSkeletonNoFlame", "UndeadFemaleSkeletonNoFlame", "UndeadFemaleSkeleton", "UndeadFemaleZombie", "UndeadFemaleZombieNoFlame", "AnakshayMale", "AnakshayFemale", "UmbraenFemaleVoid"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[30, 29, 1, 0, 3, 2, 14, 20, 4, 8, 15, 19, 7, 6, 16, 31, 5, 9, 17, 18, 26, 25, 10, 11, 27, 28, 22, 24, 12, 13, 23, 21];
}

super::support::ace_enum!(SetupConst, u32, plain);
super::support::ace_enum_from!(SetupConst, u32 => u64, i64);
