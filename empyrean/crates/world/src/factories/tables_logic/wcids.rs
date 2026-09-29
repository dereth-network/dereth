// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Wcids/ArmorWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/ClothingWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/CoalescedManaWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/ConsumeWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/HealKitWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/LockpickWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/ManaStoneWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/PetDeviceWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/SocietyArmorWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/SpellComponentWcids.cs, Source/ACE.Server/Factories/Tables/Wcids/WeaponWcids.cs
//! The 4.10 members of the `Factories/Tables/Wcids` classes: every roll that takes a
//! `TreasureDeath` (see `empyrean_tables::logic::wcids` for the rest).

/// ACE `ArmorWcids`.
pub mod armor_wcids {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::enums::{TreasureArmorType, TreasureHeritageGroup, WeenieClassName};
    use empyrean_tables::logic::tables::heritage_chance;
    use empyrean_tables::logic::wcids::armor_wcids::roll_society_armor;
    use empyrean_tables::tables::wcids::armor_wcids::{
        ALDURESSA_WCIDS, AMULI_WCIDS, CELDON_WCIDS, CHAINMAIL_WCIDS, CHIRAN_WCIDS, COVENANT_WCIDS,
        DIFORSA_WCIDS, HAEBREAN_WCIDS, KNORR_ACADEMY_WCIDS, KOUJIA_WCIDS, LEATHER_WCIDS,
        LORICA_WCIDS, NARIYID_WCIDS, OLTHOI_ALDURESSA_WCIDS, OLTHOI_AMULI_WCIDS,
        OLTHOI_CELDON_WCIDS, OLTHOI_KOUJIA_WCIDS, OLTHOI_WCIDS, OVER_ROBE_T3_T5_WCIDS,
        OVER_ROBE_T6_T8_WCIDS, PLATEMAIL_WCIDS, SCALEMAIL_WCIDS, SEDGEMAIL_LEATHER_WCIDS,
        STUDDED_LEATHER_WCIDS, TENASSA_WCIDS, YOROI_WCIDS,
    };

    /// The armor wcid for an armor type; the heritage-dependent types resolve `armor_type` to the
    /// heritage's own type (ACE's `ref` parameter).
    // ACE: ArmorWcids.Roll
    #[must_use]
    pub fn roll(
        treasure_death: &TreasureDeath,
        armor_type: &mut TreasureArmorType,
    ) -> WeenieClassName {
        match *armor_type {
            TreasureArmorType::Leather => LEATHER_WCIDS.roll(0.0),
            TreasureArmorType::StuddedLeather => STUDDED_LEATHER_WCIDS.roll(0.0),
            TreasureArmorType::Chainmail => CHAINMAIL_WCIDS.roll(0.0),
            TreasureArmorType::Platemail => roll_platemail_wcid(treasure_death, armor_type),
            TreasureArmorType::HeritageLow => roll_heritage_low_wcid(treasure_death, armor_type),
            TreasureArmorType::Covenant => COVENANT_WCIDS.roll(0.0),
            TreasureArmorType::HeritageHigh => roll_heritage_high_wcid(treasure_death, armor_type),
            TreasureArmorType::Olthoi => OLTHOI_WCIDS.roll(0.0),
            TreasureArmorType::OlthoiHeritage => {
                roll_olthoi_heritage_wcid(treasure_death, armor_type)
            }
            TreasureArmorType::Society => roll_society_armor(armor_type),
            TreasureArmorType::Haebrean => HAEBREAN_WCIDS.roll(0.0),
            TreasureArmorType::KnorrAcademy => KNORR_ACADEMY_WCIDS.roll(0.0),
            TreasureArmorType::Sedgemail => SEDGEMAIL_LEATHER_WCIDS.roll(0.0),
            TreasureArmorType::Overrobe => roll_over_robe_wcid(treasure_death),
            _ => WeenieClassName::undef,
        }
    }

    // ACE: ArmorWcids.RollHeritage
    #[must_use]
    pub fn roll_heritage(treasure_death: &TreasureDeath) -> TreasureHeritageGroup {
        heritage_chance::roll(treasure_death.unknown_chances, true)
    }

    // ACE: ArmorWcids.RollPlatemailWcid
    #[must_use]
    pub fn roll_platemail_wcid(
        treasure_death: &TreasureDeath,
        armor_type: &mut TreasureArmorType,
    ) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);

        match heritage {
            TreasureHeritageGroup::Aluvian => {
                *armor_type = TreasureArmorType::Platemail;
                PLATEMAIL_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Gharundim => {
                *armor_type = TreasureArmorType::Scalemail;
                SCALEMAIL_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Sho => {
                *armor_type = TreasureArmorType::Yoroi;
                YOROI_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Viamontian => {
                *armor_type = TreasureArmorType::Diforsa;
                DIFORSA_WCIDS.roll(0.0)
            }
            _ => WeenieClassName::undef,
        }
    }

    // ACE: ArmorWcids.RollHeritageLowWcid
    #[must_use]
    pub fn roll_heritage_low_wcid(
        treasure_death: &TreasureDeath,
        armor_type: &mut TreasureArmorType,
    ) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);

        match heritage {
            TreasureHeritageGroup::Aluvian => {
                *armor_type = TreasureArmorType::Celdon;
                CELDON_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Gharundim => {
                *armor_type = TreasureArmorType::Amuli;
                AMULI_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Sho => {
                *armor_type = TreasureArmorType::Koujia;
                KOUJIA_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Viamontian => {
                *armor_type = TreasureArmorType::Tenassa;
                TENASSA_WCIDS.roll(0.0)
            }
            _ => WeenieClassName::undef,
        }
    }

    // ACE: ArmorWcids.RollHeritageHighWcid
    #[must_use]
    pub fn roll_heritage_high_wcid(
        treasure_death: &TreasureDeath,
        armor_type: &mut TreasureArmorType,
    ) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);

        match heritage {
            TreasureHeritageGroup::Aluvian => {
                *armor_type = TreasureArmorType::Lorica;
                LORICA_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Gharundim => {
                *armor_type = TreasureArmorType::Nariyid;
                NARIYID_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Sho => {
                *armor_type = TreasureArmorType::Chiran;
                CHIRAN_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Viamontian => {
                *armor_type = TreasureArmorType::Alduressa;
                ALDURESSA_WCIDS.roll(0.0)
            }
            _ => WeenieClassName::undef,
        }
    }

    // ACE: ArmorWcids.RollOlthoiHeritageWcid
    #[must_use]
    pub fn roll_olthoi_heritage_wcid(
        treasure_death: &TreasureDeath,
        armor_type: &mut TreasureArmorType,
    ) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);

        match heritage {
            TreasureHeritageGroup::Aluvian => {
                *armor_type = TreasureArmorType::OlthoiCeldon;
                OLTHOI_CELDON_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Gharundim => {
                *armor_type = TreasureArmorType::OlthoiAmuli;
                OLTHOI_AMULI_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Sho => {
                *armor_type = TreasureArmorType::OlthoiKoujia;
                OLTHOI_KOUJIA_WCIDS.roll(0.0)
            }
            TreasureHeritageGroup::Viamontian => {
                *armor_type = TreasureArmorType::OlthoiAlduressa;
                OLTHOI_ALDURESSA_WCIDS.roll(0.0)
            }
            _ => WeenieClassName::undef,
        }
    }

    // ACE: ArmorWcids.RollOverRobeWcid
    #[must_use]
    pub fn roll_over_robe_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        if treasure_death.tier < 6 {
            OVER_ROBE_T3_T5_WCIDS.roll(0.0)
        } else {
            OVER_ROBE_T6_T8_WCIDS.roll(0.0)
        }
    }
}

/// ACE `ClothingWcids`.
pub mod clothing_wcids {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::enums::{TreasureHeritageGroup, WeenieClassName};
    use empyrean_tables::logic::tables::heritage_chance;
    use empyrean_tables::tables::wcids::clothing_wcids::{
        CLOTHING_WCIDS_ALUVIAN, CLOTHING_WCIDS_GHARUNDIM, CLOTHING_WCIDS_SHO,
        CLOTHING_WCIDS_VIAMONTIAN,
    };

    // ACE: ClothingWcids.Roll
    #[must_use]
    pub fn roll(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = heritage_chance::roll(treasure_death.unknown_chances, true);

        match heritage {
            TreasureHeritageGroup::Aluvian => CLOTHING_WCIDS_ALUVIAN.roll(0.0),
            TreasureHeritageGroup::Gharundim => CLOTHING_WCIDS_GHARUNDIM.roll(0.0),
            TreasureHeritageGroup::Sho => CLOTHING_WCIDS_SHO.roll(0.0),
            TreasureHeritageGroup::Viamontian => CLOTHING_WCIDS_VIAMONTIAN.roll(0.0),
            _ => WeenieClassName::undef,
        }
    }
}

/// ACE `CoalescedManaWcids`.
pub mod coalesced_mana_wcids {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::enums::WeenieClassName;
    use empyrean_tables::tables::wcids::coalesced_mana_wcids::TIER_CHANCES;

    use super::super::at;

    // ACE: CoalescedManaWcids.Roll
    #[must_use]
    pub fn roll(profile: &TreasureDeath) -> WeenieClassName {
        if profile.tier > 4 {
            return WeenieClassName::undef;
        }

        let table = at(&TIER_CHANCES, profile.tier - 1);

        table.roll(profile.loot_quality_mod)
    }
}

/// The per-tier wcid classes whose `Roll` is `tiers[profile.Tier - 1].Roll(profile.LootQualityMod)`.
macro_rules! tier_roll {
    ($module:ident, $class:literal, $data:ident, $tiers:ident, $anchor:literal) => {
        #[doc = concat!("ACE `", $class, "`.")]
        pub mod $module {
            use empyrean_content::models::world::TreasureDeath;
            use empyrean_tables::enums::WeenieClassName;
            use empyrean_tables::tables::wcids::$data::$tiers;

            use super::super::at;

            #[doc = concat!("ACE `", $anchor, "`.")]
            #[must_use]
            pub fn roll(profile: &TreasureDeath) -> WeenieClassName {
                let table = at(&$tiers, profile.tier - 1);

                table.roll(profile.loot_quality_mod)
            }
        }
    };
}

// ACE: ConsumeWcids.Roll
tier_roll!(
    consume_wcids,
    "ConsumeWcids",
    consume_wcids,
    CONSUME_TIERS,
    "ConsumeWcids.Roll"
);
// ACE: HealKitWcids.Roll
tier_roll!(
    heal_kit_wcids,
    "HealKitWcids",
    heal_kit_wcids,
    HEAL_KIT_TIERS,
    "HealKitWcids.Roll"
);
// ACE: LockpickWcids.Roll
tier_roll!(
    lockpick_wcids,
    "LockpickWcids",
    lockpick_wcids,
    LOCKPICK_TIERS,
    "LockpickWcids.Roll"
);
// ACE: ManaStoneWcids.Roll
tier_roll!(
    mana_stone_wcids,
    "ManaStoneWcids",
    mana_stone_wcids,
    MANA_STONE_TIERS,
    "ManaStoneWcids.Roll"
);

/// ACE `PetDeviceWcids`.
pub mod pet_device_wcids {
    use empyrean_common::thread_safe_random::ThreadSafeRandom;
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::enums::WeenieClassName;
    use empyrean_tables::logic::wcids::pet_device_wcids::PET_DEVICES;
    use empyrean_tables::tables::wcids::pet_device_wcids::PET_LEVEL_INDEXES;

    use super::super::{at, count};
    use crate::factories::loot_generation_factory::tables_logic::tables::pet_device_chance;

    /// A pet level for the tier, then an even pick of pet family.
    ///
    /// # Panics
    /// As ACE's dictionary indexer throws, if the pet level has no index.
    // ACE: PetDeviceWcids.Roll
    #[must_use]
    pub fn roll(profile: &TreasureDeath) -> WeenieClassName {
        let pet_level = pet_device_chance::roll(profile);

        let rng = ThreadSafeRandom::next(0, count(&PET_DEVICES) - 1);

        let table = at(&PET_DEVICES, rng);

        let pet_level_idx = *PET_LEVEL_INDEXES
            .get(&pet_level)
            .unwrap_or_else(|| panic!("KeyNotFoundException: pet level {pet_level}"));

        at(table, pet_level_idx)
    }
}

/// ACE `SocietyArmorWcids`.
pub mod society_armor_wcids {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::enums::{
        SocietyArmorType, SocietyType, TreasureItemType, WeenieClassName,
    };
    use empyrean_tables::logic::tables::heritage_chance;
    use empyrean_tables::tables::wcids::society_armor_wcids::SOCIETY_ARMOR_TABLES;

    use super::super::at;

    /// The society piece for a `Society*` item type, from a society rolled by heritage.
    // ACE: SocietyArmorWcids.Roll
    #[must_use]
    pub fn roll(profile: &TreasureDeath, treasure_item_type: TreasureItemType) -> WeenieClassName {
        let society = get_society(profile);

        if society == SocietyType::Undef {
            return WeenieClassName::undef;
        }

        let table = at(&SOCIETY_ARMOR_TABLES, society.0 - 1);

        let society_armor_type = treasure_item_type.get_society_armor_type();

        if society_armor_type == SocietyArmorType::Undef {
            return WeenieClassName::undef;
        }

        at(table, society_armor_type.0 - 1)
    }

    // ACE: SocietyArmorWcids.GetSociety
    #[must_use]
    pub fn get_society(profile: &TreasureDeath) -> SocietyType {
        let heritage = heritage_chance::roll(profile.unknown_chances, false);

        heritage.to_society()
    }
}

/// ACE `SpellComponentWcids`.
pub mod spell_component_wcids {
    use empyrean_common::thread_safe_random::ThreadSafeRandom;
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::enums::{Level8_SpellComponentType, WeenieClassName};
    use empyrean_tables::tables::wcids::spell_component_wcids::{
        GLYPHS, INKS, LEVEL8_SPELL_COMPONENT_CHANCE, PEA_TIERS, QUILLS,
    };

    use super::super::{at, count};

    /// T7+ first rolls for a level 8 component (quill, ink or glyph); otherwise the tier's peas.
    // ACE: SpellComponentWcids.Roll
    #[must_use]
    pub fn roll(profile: &TreasureDeath) -> WeenieClassName {
        if profile.tier >= 7 {
            let level8_spell_component =
                LEVEL8_SPELL_COMPONENT_CHANCE.roll(profile.loot_quality_mod);

            if level8_spell_component {
                return roll_level8_spell_component(profile);
            }
        }

        let table = at(&PEA_TIERS, profile.tier - 1);

        table.roll(profile.loot_quality_mod)
    }

    // ACE: SpellComponentWcids.Roll_Level8SpellComponent
    fn roll_level8_spell_component(profile: &TreasureDeath) -> WeenieClassName {
        let r#type = Level8_SpellComponentType(ThreadSafeRandom::next(1, 3));

        match r#type {
            Level8_SpellComponentType::Quill => QUILLS.roll(profile.loot_quality_mod),
            Level8_SpellComponentType::Ink => INKS.roll(profile.loot_quality_mod),
            Level8_SpellComponentType::Glyph => {
                let rng = ThreadSafeRandom::next(0, count(&GLYPHS) - 1);
                at(&GLYPHS, rng)
            }
            _ => WeenieClassName::undef,
        }
    }
}

/// ACE `WeaponWcids`: the dispatchers that take a `TreasureDeath` (the other two are in
/// `empyrean_tables::logic::wcids::weapon_wcids`).
pub mod weapon_wcids {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::enums::{TreasureHeritageGroup, TreasureWeaponType, WeenieClassName};
    use empyrean_tables::logic::legacy::{
        axe_wcids, dagger_wcids_aluvian_sho, dagger_wcids_gharundim, mace_wcids, spear_wcids,
        staff_wcids, sword_wcids_aluvian, sword_wcids_gharundim, sword_wcids_sho, unarmed_wcids,
    };
    use empyrean_tables::logic::tables::heritage_chance;
    use empyrean_tables::logic::wcids::weapon_wcids::{
        roll_melee_weapon, roll_two_handed_weapon_wcid,
    };
    use empyrean_tables::logic::weapons::{
        atlatl_wcids, bow_wcids_aluvian, bow_wcids_gharundim, bow_wcids_sho, caster_wcids,
        crossbow_wcids,
    };

    /// The weapon wcid for a weapon type; melee and two-handed rolls set `weapon_type` to the
    /// rolled weapon's own type (ACE's `ref` parameter).
    // ACE: WeaponWcids.Roll
    #[must_use]
    pub fn roll(
        treasure_death: &TreasureDeath,
        weapon_type: &mut TreasureWeaponType,
    ) -> WeenieClassName {
        match *weapon_type {
            /*case TreasureWeaponType.Sword:
                return RollSwordWcid(treasureDeath);
            ...*/
            TreasureWeaponType::Axe
            | TreasureWeaponType::Dagger
            | TreasureWeaponType::Mace
            | TreasureWeaponType::Spear
            | TreasureWeaponType::Staff
            | TreasureWeaponType::Sword
            | TreasureWeaponType::Unarmed => roll_melee_weapon(weapon_type),

            TreasureWeaponType::Bow => roll_bow_wcid(treasure_death),

            TreasureWeaponType::Crossbow => roll_crossbow_wcid(treasure_death),

            TreasureWeaponType::Atlatl => roll_atlatl_wcid(treasure_death),

            TreasureWeaponType::Caster => roll_caster(treasure_death),

            TreasureWeaponType::TwoHandedWeapon => roll_two_handed_weapon_wcid(weapon_type),

            _ => WeenieClassName::undef,
        }
    }

    // ACE: WeaponWcids.RollHeritage
    #[must_use]
    pub fn roll_heritage(treasure_death: &TreasureDeath) -> TreasureHeritageGroup {
        heritage_chance::roll(treasure_death.unknown_chances, false)
    }

    // ACE: WeaponWcids.RollSwordWcid
    #[must_use]
    pub fn roll_sword_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);

        match heritage {
            TreasureHeritageGroup::Aluvian => sword_wcids_aluvian::roll(treasure_death.tier),
            TreasureHeritageGroup::Gharundim => sword_wcids_gharundim::roll(treasure_death.tier),
            TreasureHeritageGroup::Sho => sword_wcids_sho::roll(treasure_death.tier),
            _ => WeenieClassName::undef,
        }
    }

    // ACE: WeaponWcids.RollMaceWcid
    #[must_use]
    pub fn roll_mace_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);
        mace_wcids::roll(heritage)
    }

    // ACE: WeaponWcids.RollAxeWcid
    #[must_use]
    pub fn roll_axe_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);
        axe_wcids::roll(heritage)
    }

    // ACE: WeaponWcids.RollSpearWcid
    #[must_use]
    pub fn roll_spear_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);
        spear_wcids::roll(heritage)
    }

    // ACE: WeaponWcids.RollUnarmedWcid
    #[must_use]
    pub fn roll_unarmed_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);
        unarmed_wcids::roll(heritage)
    }

    // ACE: WeaponWcids.RollStaffWcid
    #[must_use]
    pub fn roll_staff_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);
        staff_wcids::roll(heritage)
    }

    // ACE: WeaponWcids.RollDaggerWcid
    #[must_use]
    pub fn roll_dagger_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);

        match heritage {
            TreasureHeritageGroup::Aluvian | TreasureHeritageGroup::Sho => {
                dagger_wcids_aluvian_sho::roll(treasure_death.tier)
            }
            TreasureHeritageGroup::Gharundim => dagger_wcids_gharundim::roll(treasure_death.tier),
            _ => WeenieClassName::undef,
        }
    }

    // ACE: WeaponWcids.RollBowWcid
    #[must_use]
    pub fn roll_bow_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        let heritage = roll_heritage(treasure_death);

        match heritage {
            TreasureHeritageGroup::Aluvian => bow_wcids_aluvian::roll(treasure_death.tier),
            TreasureHeritageGroup::Gharundim => bow_wcids_gharundim::roll(treasure_death.tier),
            TreasureHeritageGroup::Sho => bow_wcids_sho::roll(treasure_death.tier),
            _ => WeenieClassName::undef,
        }
    }

    // ACE: WeaponWcids.RollCrossbowWcid
    #[must_use]
    pub fn roll_crossbow_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        crossbow_wcids::roll(treasure_death.tier)
    }

    // ACE: WeaponWcids.RollAtlatlWcid
    #[must_use]
    pub fn roll_atlatl_wcid(treasure_death: &TreasureDeath) -> WeenieClassName {
        atlatl_wcids::roll(treasure_death.tier)
    }

    // ACE: WeaponWcids.RollCaster
    #[must_use]
    pub fn roll_caster(treasure_death: &TreasureDeath) -> WeenieClassName {
        caster_wcids::roll(treasure_death.tier)
    }
}
