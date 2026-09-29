// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/WeenieHeaderFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/WeenieHeaderFlags.cs`; do not edit by hand

/// ACE enum `WeenieHeaderFlag` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WeenieHeaderFlag(pub u32);

#[allow(non_upper_case_globals)]
impl WeenieHeaderFlag {
    pub const None: Self = Self(0x0);
    pub const PluralName: Self = Self(0x1);
    pub const ItemsCapacity: Self = Self(0x2);
    pub const ContainersCapacity: Self = Self(0x4);
    pub const Value: Self = Self(0x8);
    pub const Usable: Self = Self(0x10);
    pub const UseRadius: Self = Self(0x20);
    pub const Monarch: Self = Self(0x40);
    pub const UiEffects: Self = Self(0x80);
    pub const AmmoType: Self = Self(0x100);
    pub const CombatUse: Self = Self(0x200);
    pub const Structure: Self = Self(0x400);
    pub const MaxStructure: Self = Self(0x800);
    pub const StackSize: Self = Self(0x1000);
    pub const MaxStackSize: Self = Self(0x2000);
    pub const Container: Self = Self(0x4000);
    pub const Wielder: Self = Self(0x8000);
    pub const ValidLocations: Self = Self(0x10000);
    pub const CurrentlyWieldedLocation: Self = Self(0x20000);
    pub const Priority: Self = Self(0x40000);
    pub const TargetType: Self = Self(0x80000);
    pub const RadarBlipColor: Self = Self(0x100000);
    pub const Burden: Self = Self(0x200000);
    pub const Spell: Self = Self(0x400000);
    pub const RadarBehavior: Self = Self(0x800000);
    pub const Workmanship: Self = Self(0x1000000);
    pub const HouseOwner: Self = Self(0x2000000);
    pub const HouseRestrictions: Self = Self(0x4000000);
    pub const PScript: Self = Self(0x8000000);
    pub const HookType: Self = Self(0x10000000);
    pub const HookItemTypes: Self = Self(0x20000000);
    pub const IconOverlay: Self = Self(0x40000000);
    pub const MaterialType: Self = Self(0x80000000);
}

impl WeenieHeaderFlag {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::PluralName, Self::ItemsCapacity, Self::ContainersCapacity, Self::Value, Self::Usable, Self::UseRadius, Self::Monarch, Self::UiEffects, Self::AmmoType, Self::CombatUse, Self::Structure, Self::MaxStructure, Self::StackSize, Self::MaxStackSize, Self::Container, Self::Wielder, Self::ValidLocations, Self::CurrentlyWieldedLocation, Self::Priority, Self::TargetType, Self::RadarBlipColor, Self::Burden, Self::Spell, Self::RadarBehavior, Self::Workmanship, Self::HouseOwner, Self::HouseRestrictions, Self::PScript, Self::HookType, Self::HookItemTypes, Self::IconOverlay, Self::MaterialType];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "PluralName", "ItemsCapacity", "ContainersCapacity", "Value", "Usable", "UseRadius", "Monarch", "UiEffects", "AmmoType", "CombatUse", "Structure", "MaxStructure", "StackSize", "MaxStackSize", "Container", "Wielder", "ValidLocations", "CurrentlyWieldedLocation", "Priority", "TargetType", "RadarBlipColor", "Burden", "Spell", "RadarBehavior", "Workmanship", "HouseOwner", "HouseRestrictions", "PScript", "HookType", "HookItemTypes", "IconOverlay", "MaterialType"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[9, 22, 10, 15, 3, 18, 30, 29, 26, 27, 31, 2, 32, 14, 12, 7, 0, 28, 1, 19, 24, 21, 23, 13, 11, 20, 8, 5, 6, 17, 4, 16, 25];
}

super::support::ace_enum!(WeenieHeaderFlag, u32, flags);
super::support::ace_enum_from!(WeenieHeaderFlag, u32 => u64, i64);

/// ACE enum `WeenieHeaderFlag2` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WeenieHeaderFlag2(pub u32);

#[allow(non_upper_case_globals)]
impl WeenieHeaderFlag2 {
    pub const None: Self = Self(0x0);
    pub const IconUnderlay: Self = Self(0x1);
    pub const Cooldown: Self = Self(0x2);
    pub const CooldownDuration: Self = Self(0x4);
    pub const PetOwner: Self = Self(0x8);
}

impl WeenieHeaderFlag2 {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::IconUnderlay, Self::Cooldown, Self::CooldownDuration, Self::PetOwner];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "IconUnderlay", "Cooldown", "CooldownDuration", "PetOwner"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 3, 1, 0, 4];
}

super::support::ace_enum!(WeenieHeaderFlag2, u32, flags);
super::support::ace_enum_from!(WeenieHeaderFlag2, u32 => u64, i64);
