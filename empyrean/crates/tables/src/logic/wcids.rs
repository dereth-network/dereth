// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/*.cs

//! The methods of the `Factories/Tables/Wcids` classes.
//!
//! In `empyrean-world`'s `factories::tables_logic` instead (they take `TreasureDeath`): `ArmorWcids.Roll`, `RollHeritage`,
//! `RollPlatemailWcid`, `RollHeritageLowWcid`, `RollHeritageHighWcid`, `RollOlthoiHeritageWcid`,
//! `RollOverRobeWcid`; `ClothingWcids.Roll`; `CoalescedManaWcids.Roll`; `ConsumeWcids.Roll`;
//! `HealKitWcids.Roll`; `LockpickWcids.Roll`; `ManaStoneWcids.Roll`; `PetDeviceWcids.Roll`;
//! `SocietyArmorWcids.Roll`/`GetSociety`; `SpellComponentWcids.Roll`/`Roll_Level8SpellComponent`;
//! `WeaponWcids.Roll`, `RollHeritage` and the per-weapon `Roll*Wcid`/`RollCaster` wrappers.

/// ACE `AetheriaWcids`.
pub mod aetheria_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::logic::at;
    use crate::rng;
    use crate::tables::wcids::aetheria_wcids::AETHERIA_COLORS;

    // ACE: AetheriaWcids.Roll
    pub fn roll(tier: i32) -> WeenieClassName {
        match tier {
            // blue only
            5 => AETHERIA_COLORS[0],

            // even chance between blue / yellow
            6 => {
                let rng = rng::next_int(0, 1);
                at(&AETHERIA_COLORS, rng)
            }

            // even chance between blue / yellow / red
            7 | 8 => {
                let rng = rng::next_int(0, 2);
                at(&AETHERIA_COLORS, rng)
            }
            _ => WeenieClassName::undef,
        }
    }

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(aetheria_wcids);

    // ACE: AetheriaWcids.AetheriaWcids
    fn aetheria_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for &aetheria_wcid in &AETHERIA_COLORS {
            combined.insert(aetheria_wcid);
        }
        combined
    }

    // ACE: AetheriaWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `ArmorWcids`.
pub mod armor_wcids {
    use std::collections::HashMap;
    use std::sync::LazyLock;

    use crate::entity::ChanceTable;
    use crate::enums::{TreasureArmorType, WeenieClassName};
    use crate::rng;
    use crate::tables::wcids::armor_wcids::{
        CELESTIAL_HAND_WCIDS, ELDRYTCH_WEB_WCIDS, RADIANT_BLOOD_WCIDS,
        STATIC_CTOR_BUILD_COMBINED_ARGS,
    };

    /// Picks one of the three societies evenly, sets `armor_type` to it and rolls its armor.
    // ACE: ArmorWcids.RollSocietyArmor
    pub fn roll_society_armor(armor_type: &mut TreasureArmorType) -> WeenieClassName {
        let rng = rng::next_int(1, 3);

        match rng {
            1 => {
                *armor_type = TreasureArmorType::CelestialHand;
                CELESTIAL_HAND_WCIDS.roll(0.0)
            }
            2 => {
                *armor_type = TreasureArmorType::EldrytchWeb;
                ELDRYTCH_WEB_WCIDS.roll(0.0)
            }
            3 => {
                *armor_type = TreasureArmorType::RadiantBlood;
                RADIANT_BLOOD_WCIDS.roll(0.0)
            }
            _ => WeenieClassName::undef,
        }
    }

    static COMBINED: LazyLock<HashMap<WeenieClassName, TreasureArmorType>> =
        LazyLock::new(armor_wcids);

    /// The static constructor: `BuildCombined` for each table, in ACE's order.
    // ACE: ArmorWcids.ArmorWcids
    fn armor_wcids() -> HashMap<WeenieClassName, TreasureArmorType> {
        let mut combined = HashMap::new();
        for (wcids, armor_type) in STATIC_CTOR_BUILD_COMBINED_ARGS {
            build_combined(&mut combined, wcids, armor_type);
        }
        combined
    }

    // ACE: ArmorWcids.BuildCombined
    fn build_combined(
        combined: &mut HashMap<WeenieClassName, TreasureArmorType>,
        wcids: &ChanceTable<WeenieClassName>,
        armor_type: TreasureArmorType,
    ) {
        for entry in wcids.entries() {
            // Dictionary.TryAdd: the first table a wcid appears in wins
            combined.entry(entry.0).or_insert(armor_type);
        }
    }

    // ACE: ArmorWcids.TryGetValue
    pub fn try_get_value(wcid: WeenieClassName) -> Option<TreasureArmorType> {
        COMBINED.get(&wcid).copied()
    }
}

/// ACE `CloakWcids`.
pub mod cloak_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::logic::{at, count};
    use crate::rng;
    use crate::tables::wcids::cloak_wcids::CLOAK_WCIDS;

    // ACE: CloakWcids.Roll
    pub fn roll() -> WeenieClassName {
        let rng = rng::next_int(0, count(&CLOAK_WCIDS) - 1);

        at(&CLOAK_WCIDS, rng)
    }

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(cloak_wcids);

    // ACE: CloakWcids.CloakWcids
    fn cloak_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for &cloak_wcid in &CLOAK_WCIDS {
            combined.insert(cloak_wcid);
        }
        combined
    }

    // ACE: CloakWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `ClothingWcids`.
pub mod clothing_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::entity::ChanceTable;
    use crate::enums::WeenieClassName;
    use crate::tables::wcids::clothing_wcids::STATIC_CTOR_BUILD_COMBINED_ARGS;

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(clothing_wcids);

    /// The static constructor: `BuildCombined` for each heritage's table, in ACE's order.
    // ACE: ClothingWcids.ClothingWcids
    fn clothing_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for wcids in STATIC_CTOR_BUILD_COMBINED_ARGS {
            build_combined(&mut combined, wcids);
        }
        combined
    }

    // ACE: ClothingWcids.BuildCombined
    fn build_combined(
        combined: &mut HashSet<WeenieClassName>,
        wcids: &ChanceTable<WeenieClassName>,
    ) {
        for entry in wcids.entries() {
            combined.insert(entry.0);
        }
    }

    // ACE: ClothingWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `GenericWcids`.
pub mod generic_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::logic::at;
    use crate::tables::wcids::generic_wcids::TIER_CHANCES;

    // ACE: GenericWcids.Roll
    pub fn roll(tier: i32) -> WeenieClassName {
        let tier = tier.clamp(1, 6);

        at(&TIER_CHANCES, tier - 1).roll(0.0)
    }

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(generic_wcids);

    // ACE: GenericWcids.GenericWcids
    fn generic_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for tier_chance in TIER_CHANCES {
            for entry in tier_chance.entries() {
                combined.insert(entry.0);
            }
        }
        combined
    }

    // ACE: GenericWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `JewelryWcids`.
pub mod jewelry_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::logic::at;
    use crate::tables::wcids::jewelry_wcids::TIER_CHANCES;

    // ACE: JewelryWcids.Roll
    pub fn roll(tier: i32) -> WeenieClassName {
        let tier = tier.clamp(1, 6);

        at(&TIER_CHANCES, tier - 1).roll(0.0)
    }

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(jewelry_wcids);

    // ACE: JewelryWcids.JewelryWcids
    fn jewelry_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for tier_chance in TIER_CHANCES {
            for entry in tier_chance.entries() {
                combined.insert(entry.0);
            }
        }
        combined
    }

    // ACE: JewelryWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `PetDeviceWcids`.
pub mod pet_device_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::tables::wcids::pet_device_wcids::{
        NATURALIST_PET_DEVICES, NECROMANCER_PET_DEVICES, PRIMALIST_PET_DEVICES,
    };

    /// `petDevices`: `Necromancer.Union(Primalist).Union(Naturalist).ToList()`. LINQ's `Union`
    /// keeps first occurrences and compares the inner `List`s by reference, so a list shared by
    /// two groups appears once.
    pub static PET_DEVICES: LazyLock<Vec<&'static [WeenieClassName]>> = LazyLock::new(|| {
        let mut out: Vec<&'static [WeenieClassName]> = Vec::new();
        for group in [
            &NECROMANCER_PET_DEVICES,
            &PRIMALIST_PET_DEVICES,
            &NATURALIST_PET_DEVICES,
        ] {
            for &list in group.iter() {
                if !out.iter().any(|seen| std::ptr::eq(*seen, list)) {
                    out.push(list);
                }
            }
        }
        out
    });

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(pet_device_wcids);

    // ACE: PetDeviceWcids.PetDeviceWcids
    fn pet_device_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for pet_device in PET_DEVICES.iter() {
            for &wcid in pet_device.iter() {
                combined.insert(wcid);
            }
        }
        combined
    }

    // ACE: PetDeviceWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `ScrollWcids`.
pub mod scroll_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::logic::{at, count};
    use crate::rng;
    use crate::tables::wcids::scroll_wcids::SCROLL_WCIDS;

    // ACE: ScrollWcids.Roll
    pub fn roll() -> WeenieClassName {
        let rng = rng::next_int(0, count(&SCROLL_WCIDS) - 1);

        at(&SCROLL_WCIDS, rng)
    }

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(scroll_wcids);

    // ACE: ScrollWcids.ScrollWcids
    fn scroll_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for &scroll_wcid in &SCROLL_WCIDS {
            combined.insert(scroll_wcid);
        }
        combined
    }

    // ACE: ScrollWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `SocietyArmorWcids`.
pub mod society_armor_wcids {
    use std::collections::HashSet;
    use std::sync::LazyLock;

    use crate::enums::WeenieClassName;
    use crate::tables::wcids::society_armor_wcids::SOCIETY_ARMOR_TABLES;

    static COMBINED: LazyLock<HashSet<WeenieClassName>> = LazyLock::new(society_armor_wcids);

    // ACE: SocietyArmorWcids.SocietyArmorWcids
    fn society_armor_wcids() -> HashSet<WeenieClassName> {
        let mut combined = HashSet::new();
        for table in SOCIETY_ARMOR_TABLES {
            for &wcid in table {
                combined.insert(wcid);
            }
        }
        combined
    }

    // ACE: SocietyArmorWcids.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        COMBINED.contains(&wcid)
    }
}

/// ACE `WeaponWcids`: the two dispatchers that do not take a `TreasureDeath`.
pub mod weapon_wcids {
    use crate::enums::{MeleeWeaponSkill, TreasureWeaponType, WeenieClassName};
    use crate::logic::weapons::{
        finesse_weapon_wcids, heavy_weapon_wcids, light_weapon_wcids, two_handed_weapon_wcids,
    };
    use crate::rng;

    /// Picks heavy, light or finesse evenly and rolls a weapon of it, setting `weapon_type`.
    // ACE: WeaponWcids.RollMeleeWeapon
    pub fn roll_melee_weapon(weapon_type: &mut TreasureWeaponType) -> WeenieClassName {
        let weapon_skill = MeleeWeaponSkill(rng::next_int(1, 3));

        match weapon_skill {
            MeleeWeaponSkill::HeavyWeapons => heavy_weapon_wcids::roll(weapon_type),
            MeleeWeaponSkill::LightWeapons => light_weapon_wcids::roll(weapon_type),
            MeleeWeaponSkill::FinesseWeapons => finesse_weapon_wcids::roll(weapon_type),
            _ => WeenieClassName::undef,
        }
    }

    // ACE: WeaponWcids.RollTwoHandedWeaponWcid
    pub fn roll_two_handed_weapon_wcid(weapon_type: &mut TreasureWeaponType) -> WeenieClassName {
        two_handed_weapon_wcids::roll(weapon_type)
    }
}
