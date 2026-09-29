// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CombatBodyPart.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CombatBodyPart.cs`; do not edit by hand

/// ACE enum `CombatBodyPart`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CombatBodyPart(pub i32);

#[allow(non_upper_case_globals)]
impl CombatBodyPart {
    pub const Undefined: Self = Self(-1);
    pub const Head: Self = Self(0);
    pub const Chest: Self = Self(1);
    pub const Abdomen: Self = Self(2);
    pub const UpperArm: Self = Self(3);
    pub const LowerArm: Self = Self(4);
    pub const Hand: Self = Self(5);
    pub const UpperLeg: Self = Self(6);
    pub const LowerLeg: Self = Self(7);
    pub const Foot: Self = Self(8);
    pub const Horn: Self = Self(9);
    pub const FrontLeg: Self = Self(10);
    pub const FrontFoot: Self = Self(12);
    pub const RearLeg: Self = Self(13);
    pub const RearFoot: Self = Self(15);
    pub const Torso: Self = Self(16);
    pub const Tail: Self = Self(17);
    pub const Arm: Self = Self(18);
    pub const Leg: Self = Self(19);
    pub const Claw: Self = Self(20);
    pub const Wings: Self = Self(21);
    pub const Breath: Self = Self(22);
    pub const Tentacle: Self = Self(23);
    pub const UpperTentacle: Self = Self(24);
    pub const LowerTentacle: Self = Self(25);
    pub const Cloak: Self = Self(26);
    pub const NumParts: Self = Self(27);
}

impl CombatBodyPart {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Head, Self::Chest, Self::Abdomen, Self::UpperArm, Self::LowerArm, Self::Hand, Self::UpperLeg, Self::LowerLeg, Self::Foot, Self::Horn, Self::FrontLeg, Self::FrontFoot, Self::RearLeg, Self::RearFoot, Self::Torso, Self::Tail, Self::Arm, Self::Leg, Self::Claw, Self::Wings, Self::Breath, Self::Tentacle, Self::UpperTentacle, Self::LowerTentacle, Self::Cloak, Self::NumParts, Self::Undefined];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Head", "Chest", "Abdomen", "UpperArm", "LowerArm", "Hand", "UpperLeg", "LowerLeg", "Foot", "Horn", "FrontLeg", "FrontFoot", "RearLeg", "RearFoot", "Torso", "Tail", "Arm", "Leg", "Claw", "Wings", "Breath", "Tentacle", "UpperTentacle", "LowerTentacle", "Cloak", "NumParts", "Undefined"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 16, 20, 1, 18, 24, 8, 11, 10, 5, 0, 9, 17, 4, 7, 23, 25, 13, 12, 15, 21, 14, 26, 3, 6, 22, 19];
}

super::support::ace_enum!(CombatBodyPart, i32, plain);
super::support::ace_enum_from!(CombatBodyPart, i32 => i64);
