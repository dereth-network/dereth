// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EquipMask.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EquipMask.cs`; do not edit by hand

/// This data is sent as loc in the player description message F7B0 -0013
///
/// ACE enum `EquipMask` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EquipMask(pub u32);

#[allow(non_upper_case_globals)]
impl EquipMask {
    pub const None: Self = Self(0x0);
    pub const HeadWear: Self = Self(0x1);
    pub const ChestWear: Self = Self(0x2);
    pub const AbdomenWear: Self = Self(0x4);
    pub const UpperArmWear: Self = Self(0x8);
    pub const LowerArmWear: Self = Self(0x10);
    pub const HandWear: Self = Self(0x20);
    pub const UpperLegWear: Self = Self(0x40);
    pub const LowerLegWear: Self = Self(0x80);
    pub const FootWear: Self = Self(0x100);
    pub const ChestArmor: Self = Self(0x200);
    pub const AbdomenArmor: Self = Self(0x400);
    pub const UpperArmArmor: Self = Self(0x800);
    pub const LowerArmArmor: Self = Self(0x1000);
    pub const UpperLegArmor: Self = Self(0x2000);
    pub const LowerLegArmor: Self = Self(0x4000);
    pub const NeckWear: Self = Self(0x8000);
    pub const WristWearLeft: Self = Self(0x10000);
    pub const WristWearRight: Self = Self(0x20000);
    pub const FingerWearLeft: Self = Self(0x40000);
    pub const FingerWearRight: Self = Self(0x80000);
    pub const MeleeWeapon: Self = Self(0x100000);
    pub const Shield: Self = Self(0x200000);
    pub const MissileWeapon: Self = Self(0x400000);
    pub const MissileAmmo: Self = Self(0x800000);
    pub const Held: Self = Self(0x1000000);
    pub const TwoHanded: Self = Self(0x2000000);
    pub const TrinketOne: Self = Self(0x4000000);
    pub const Cloak: Self = Self(0x8000000);
    pub const SigilOne: Self = Self(0x10000000);
    pub const SigilTwo: Self = Self(0x20000000);
    pub const SigilThree: Self = Self(0x40000000);
    pub const Clothing: Self = Self(0x800001FF);
    pub const Armor: Self = Self(0x7F00);
    pub const ArmorExclusive: Self = Self(0x7E00);
    pub const Extremity: Self = Self(0x121);
    pub const Jewelry: Self = Self(0x7C0F8000);
    pub const WristWear: Self = Self(0x30000);
    pub const FingerWear: Self = Self(0xC0000);
    pub const Sigil: Self = Self(0x70000000);
    pub const ReadySlot: Self = Self(0x3F000000);
    pub const Weapon: Self = Self(0x25000000);
    pub const WeaponReadySlot: Self = Self(0x35000000);
    pub const Selectable: Self = Self(0x3700000);
    pub const SelectablePlusAmmo: Self = Self(0x3F00000);
    pub const All: Self = Self(0x7FFFFFFF);
    pub const CanGoInReadySlot: Self = Self(0x7FFFFFFF);
}

impl EquipMask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::HeadWear, Self::ChestWear, Self::AbdomenWear, Self::UpperArmWear, Self::LowerArmWear, Self::HandWear, Self::UpperLegWear, Self::LowerLegWear, Self::FootWear, Self::Extremity, Self::ChestArmor, Self::AbdomenArmor, Self::UpperArmArmor, Self::LowerArmArmor, Self::UpperLegArmor, Self::LowerLegArmor, Self::ArmorExclusive, Self::Armor, Self::NeckWear, Self::WristWearLeft, Self::WristWearRight, Self::WristWear, Self::FingerWearLeft, Self::FingerWearRight, Self::FingerWear, Self::MeleeWeapon, Self::Shield, Self::MissileWeapon, Self::MissileAmmo, Self::Held, Self::TwoHanded, Self::Selectable, Self::SelectablePlusAmmo, Self::TrinketOne, Self::Cloak, Self::SigilOne, Self::SigilTwo, Self::Weapon, Self::WeaponReadySlot, Self::ReadySlot, Self::SigilThree, Self::Sigil, Self::Jewelry, Self::All, Self::CanGoInReadySlot, Self::Clothing];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "HeadWear", "ChestWear", "AbdomenWear", "UpperArmWear", "LowerArmWear", "HandWear", "UpperLegWear", "LowerLegWear", "FootWear", "Extremity", "ChestArmor", "AbdomenArmor", "UpperArmArmor", "LowerArmArmor", "UpperLegArmor", "LowerLegArmor", "ArmorExclusive", "Armor", "NeckWear", "WristWearLeft", "WristWearRight", "WristWear", "FingerWearLeft", "FingerWearRight", "FingerWear", "MeleeWeapon", "Shield", "MissileWeapon", "MissileAmmo", "Held", "TwoHanded", "Selectable", "SelectablePlusAmmo", "TrinketOne", "Cloak", "SigilOne", "SigilTwo", "Weapon", "WeaponReadySlot", "ReadySlot", "SigilThree", "Sigil", "Jewelry", "All", "CanGoInReadySlot", "Clothing"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[12, 3, 44, 18, 17, 45, 11, 2, 35, 46, 10, 25, 23, 24, 9, 6, 1, 30, 43, 14, 5, 16, 8, 26, 29, 28, 19, 0, 40, 32, 33, 27, 42, 36, 41, 37, 34, 31, 13, 4, 15, 7, 38, 39, 22, 20, 21];
}

super::support::ace_enum!(EquipMask, u32, flags);
super::support::ace_enum_from!(EquipMask, u32 => u64, i64);
