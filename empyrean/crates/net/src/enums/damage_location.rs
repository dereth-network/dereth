// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Enum/DamageLocation.cs

/// ACE `DamageLocation`, the body location `GameEventDefenderNotification` reports. C# casts any
/// `int` to it (`(DamageLocation)iDamageLocation`), so it is a newtype over the value it writes
/// (`(uint)damageLocation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DamageLocation(pub u32);

#[allow(non_upper_case_globals)]
impl DamageLocation {
    pub const Head: Self = Self(0x0);
    pub const Chest: Self = Self(0x1);
    pub const Abdomen: Self = Self(0x2);
    pub const UpperArm: Self = Self(0x3);
    pub const LowerArm: Self = Self(0x4);
    pub const Hand: Self = Self(0x5);
    pub const UpperLeg: Self = Self(0x6);
    pub const LowerLeg: Self = Self(0x7);
    pub const Foot: Self = Self(0x8);
}
