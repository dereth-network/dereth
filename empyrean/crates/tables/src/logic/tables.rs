// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/*.cs

//! The methods of the classes directly in `Factories/Tables`.
//!
//! In `empyrean-world`'s `factories::tables_logic` instead (they take `TreasureDeath`, `WorldObject` or `TreasureRoll`, or read the
//! world database): `AetheriaChance.Roll_ItemMaxLevel`, `ArmorModVsTypeChance.RollQualityLevel`,
//! `CasterSlotSpells.Roll`/`IsOrb`, `CloakChance.Roll_ItemMaxLevel`, `EquipmentSetChance.Roll`,
//! `GearRatingChance.Roll`, all of `GemCountChance`, `PetDeviceChance.Roll`,
//! `QualityChance.RollTierChance`/`Roll`/`RollInterval`, `ScrollLevelChance.Roll`.

/// ACE `ArmorModVsTypeChance`.
pub mod armor_mod_vs_type_chance {
    use crate::logic::at;
    use crate::rng;
    use crate::tables::armor_mod_vs_type_chance::TIER_CHANCES;

    /// The preliminary per-element roll for an ArmorModVsType mutation.
    // ACE: ArmorModVsTypeChance.Roll
    pub fn roll(tier: i32) -> bool {
        // shortcut
        if tier < 2 {
            return false;
        }

        // quality mod?
        let rng = rng::next_double(0.0, 1.0);

        rng < f64::from(at(&TIER_CHANCES, tier - 1))
    }
}

/// ACE `ArmorTypeChance`.
pub mod armor_type_chance {
    use crate::enums::TreasureArmorType;
    use crate::logic::at;
    use crate::tables::armor_type_chance::ARMOR_TIERS;

    // ACE: ArmorTypeChance.Roll
    pub fn roll(tier: i32) -> TreasureArmorType {
        at(&ARMOR_TIERS, tier - 1).roll(0.0)
    }
}

/// ACE `CasterSlotSpells`.
pub mod caster_slot_spells {
    use empyrean_entity::enums::SpellId;

    use crate::logic::tables::spell_level_progression;
    use crate::tables::caster_slot_spells::DESCRIPTORS;

    /// `" of <descriptor>"` for a spell whose level-1 spell has a descriptor, else `""`.
    // ACE: CasterSlotSpells.GetDescriptor
    pub fn get_descriptor(spell_id: SpellId) -> String {
        let Some(spell_levels) = spell_level_progression::get_spell_levels(spell_id) else {
            return String::new();
        };

        let first_spell = spell_levels[0];

        let Some(descriptor) = DESCRIPTORS.get(&first_spell) else {
            return String::new();
        };

        format!(" of {descriptor}")
    }
}

/// ACE `CloakChance`.
pub mod cloak_chance {
    use empyrean_entity::enums::{EquipmentSet, SpellId};

    use crate::logic::{at, count};
    use crate::rng;
    use crate::tables::cloak_chance::{CLOAK_SETS, SURGE_SPELLS};

    // ACE: CloakChance.RollEquipmentSet
    pub fn roll_equipment_set() -> EquipmentSet {
        // verify: even chance per set
        let rng = rng::next_int(0, count(&CLOAK_SETS) - 1);

        at(&CLOAK_SETS, rng)
    }

    /// One of the surge spells, or `Undef` (the damage reduction proc) with an equal chance.
    // ACE: CloakChance.RollProcSpell
    pub fn roll_proc_spell() -> SpellId {
        // verify: even chance for each spell, including Damage Reduction Proc?
        let rng = rng::next_int(0, count(&SURGE_SPELLS));

        // handle Damage Reduction proc
        if rng == count(&SURGE_SPELLS) {
            return SpellId::Undef;
        }

        at(&SURGE_SPELLS, rng)
    }
}

/// ACE `GemClassChance`.
pub mod gem_class_chance {
    use crate::logic::at;
    use crate::tables::gem_class_chance::GEM_CLASS_CHANCES;

    /// Rolls for a GemClass value between 1-6 for a tier.
    // ACE: GemClassChance.Roll
    pub fn roll(tier: i32) -> i32 {
        // todo: add t7 / t8
        let tier = tier.clamp(1, 6);

        let gem_class_chance_table = at(&GEM_CLASS_CHANCES, tier - 1);

        gem_class_chance_table.roll(0.0)
    }
}

/// ACE `GemMaterialChance`.
pub mod gem_material_chance {
    use std::collections::{HashMap, HashSet};
    use std::sync::LazyLock;

    use empyrean_entity::enums::MaterialType;

    use crate::entity::GemResult;
    use crate::enums::WeenieClassName;
    use crate::logic::at;
    use crate::tables::gem_material_chance::{GEM_CLASS_VALUE, GEM_MATERIAL_CHANCES};

    /// Rolls for a MaterialType for a gem class.
    // ACE: GemMaterialChance.Roll
    pub fn roll(gem_class: i32) -> GemResult {
        let gem_class = gem_class.clamp(1, 6);

        let gem_material_chance = at(&GEM_MATERIAL_CHANCES, gem_class - 1);

        gem_material_chance.roll(0.0)
    }

    /// What ACE's static constructor builds.
    struct Built {
        gem_material_value: HashMap<MaterialType, i32>,
        combined: HashSet<WeenieClassName>,
    }

    static BUILT: LazyLock<Built> = LazyLock::new(gem_material_chance);

    // ACE: GemMaterialChance.GemMaterialChance
    fn gem_material_chance() -> Built {
        let mut gem_material_value = HashMap::new();
        let mut combined = HashSet::new();

        // build gemMaterialValue
        for (i, table) in GEM_MATERIAL_CHANCES.iter().enumerate() {
            for material in table.entries().iter().map(|i| i.0) {
                // Dictionary.Add throws on a duplicate key
                let prev = gem_material_value.insert(material.material_type, GEM_CLASS_VALUE[i]);
                assert!(
                    prev.is_none(),
                    "duplicate gem MaterialType {:?}",
                    material.material_type
                );
            }
        }

        // build wcid hashset for lootgen command
        for gem_material_chance in GEM_MATERIAL_CHANCES.iter() {
            for entry in gem_material_chance.entries() {
                combined.insert(entry.0.class_name);
            }
        }

        Built {
            gem_material_value,
            combined,
        }
    }

    /// The value for a gem type; 0 for none or an unknown one.
    // ACE: GemMaterialChance.GemValue
    pub fn gem_value(gem_type: Option<MaterialType>) -> i32 {
        match gem_type.and_then(|g| BUILT.gem_material_value.get(&g)) {
            Some(&gem_value) => gem_value,
            None => 0, // default?
        }
    }

    // ACE: GemMaterialChance.Contains
    pub fn contains(wcid: WeenieClassName) -> bool {
        BUILT.combined.contains(&wcid)
    }
}

/// ACE `HeritageChance`.
pub mod heritage_chance {
    use crate::enums::TreasureHeritageGroup;
    use crate::logic::{at, count};
    use crate::rng;
    use crate::tables::heritage_chance::HERITAGE_PROFILES;

    /// ACE's `addViamontian` defaults to `false`.
    // ACE: HeritageChance.Roll
    pub fn roll(heritage_profile: i32, add_viamontian: bool) -> TreasureHeritageGroup {
        if heritage_profile < 1 || heritage_profile > count(&HERITAGE_PROFILES) {
            return TreasureHeritageGroup(rng::next_int(1, 3));
        }

        let mut heritage_profile = heritage_profile;
        if add_viamontian && heritage_profile == 19 {
            heritage_profile = 21;
        }

        at(&HERITAGE_PROFILES, heritage_profile - 1).roll(0.0)
    }
}

/// ACE `MaterialTable`.
pub mod material_table {
    use empyrean_entity::enums::MaterialType;

    use crate::tables::material_table::VALUE_MOD;

    /// The value modifier for a material; 1.0 for none or an unknown one.
    // ACE: MaterialTable.GetValueMod
    pub fn get_value_mod(material_type: Option<MaterialType>) -> f32 {
        match material_type.and_then(|m| VALUE_MOD.get(&m)) {
            Some(&value_mod) => value_mod,
            None => 1.0, // default?
        }
    }
}

/// ACE `QualityChance`.
pub mod quality_chance {
    use crate::tables::quality_chance::{
        T1_QUALITY_CHANCES, T2_QUALITY_CHANCES, T3_QUALITY_CHANCES, T4_QUALITY_CHANCES,
        T5_QUALITY_CHANCES, T6_QUALITY_CHANCES, T7_QUALITY_CHANCES, T8_QUALITY_CHANCES,
    };

    // ACE: QualityChance.GetQualityChancesForTier
    pub fn get_quality_chances_for_tier(tier: i32) -> &'static [f32] {
        match tier {
            2 => &T2_QUALITY_CHANCES,
            3 => &T3_QUALITY_CHANCES,
            4 => &T4_QUALITY_CHANCES,
            5 => &T5_QUALITY_CHANCES,
            6 => &T6_QUALITY_CHANCES,
            7 => &T7_QUALITY_CHANCES,
            8 => &T8_QUALITY_CHANCES,
            // case 1 and default
            _ => &T1_QUALITY_CHANCES,
        }
    }
}

/// ACE `SpellLevelChance`.
pub mod spell_level_chance {
    use crate::logic::at;
    use crate::tables::spell_level_chance::SPELL_LEVEL_CHANCES;

    // ACE: SpellLevelChance.Roll
    pub fn roll(tier: i32) -> i32 {
        at(&SPELL_LEVEL_CHANCES, tier - 1).roll(0.0)
    }
}

/// ACE `SpellLevelProgression`: every spell's level progression, keyed by each of its levels.
pub mod spell_level_progression {
    use std::collections::HashMap;
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::tables::spell_level_progression::STATIC_CTOR_ADD_SPELLS_ARGS;

    static SPELL_PROGRESSION: LazyLock<HashMap<SpellId, &'static [SpellId]>> =
        LazyLock::new(spell_level_progression);

    /// The static constructor: `AddSpells` for each list, in ACE's order (the generated
    /// `STATIC_CTOR_ADD_SPELLS_ARGS`).
    // ACE: SpellLevelProgression.SpellLevelProgression
    fn spell_level_progression() -> HashMap<SpellId, &'static [SpellId]> {
        // takes ~5ms
        let mut spell_progression = HashMap::new();

        for spells in STATIC_CTOR_ADD_SPELLS_ARGS {
            add_spells(&mut spell_progression, spells);
        }

        spell_progression
    }

    // ACE: SpellLevelProgression.AddSpells
    fn add_spells(
        spell_progression: &mut HashMap<SpellId, &'static [SpellId]>,
        spells: &'static [SpellId],
    ) {
        for &spell in spells {
            if spell != SpellId::Undef {
                // Dictionary.Add throws on a duplicate key
                let prev = spell_progression.insert(spell, spells);
                assert!(prev.is_none(), "SpellLevelProgression: duplicate {spell:?}");
            }
        }
    }

    /// The level progression containing `spell_id`, or `None`.
    // ACE: SpellLevelProgression.GetSpellLevels
    pub fn get_spell_levels(spell_id: SpellId) -> Option<&'static [SpellId]> {
        SPELL_PROGRESSION.get(&spell_id).copied()
    }
}

/// ACE `SpellSelectionTable`.
pub mod spell_selection_table {
    use empyrean_entity::enums::SpellId;

    use crate::logic::at;
    use crate::tables::spell_selection_table::SPELL_SELECTION_GROUP;

    // ACE: SpellSelectionTable.Roll
    pub fn roll(spell_code: i32) -> SpellId {
        at(&SPELL_SELECTION_GROUP, spell_code - 1).roll(0.0)
    }
}

/// ACE `TreasureProfile_Item`.
pub mod treasure_profile_item {
    use crate::enums::TreasureItemType;
    use crate::logic::{at, count};
    use crate::tables::treasure_profile_item::ITEM_PROFILES;

    // ACE: TreasureProfile_Item.Roll
    pub fn roll(item_profile: i32) -> TreasureItemType {
        if item_profile < 1 || item_profile > count(&ITEM_PROFILES) {
            return TreasureItemType::Undef;
        }

        at(&ITEM_PROFILES, item_profile - 1).roll(0.0)
    }
}

/// ACE `TreasureProfile_MagicItem`.
pub mod treasure_profile_magic_item {
    use crate::enums::TreasureItemType;
    use crate::logic::{at, count};
    use crate::tables::treasure_profile_magic_item::MAGIC_ITEM_PROFILES;

    // ACE: TreasureProfile_MagicItem.Roll
    pub fn roll(magic_item_profile: i32) -> TreasureItemType {
        if magic_item_profile < 1 || magic_item_profile > count(&MAGIC_ITEM_PROFILES) {
            return TreasureItemType::Undef;
        }

        at(&MAGIC_ITEM_PROFILES, magic_item_profile - 1).roll(0.0)
    }
}

/// ACE `TreasureProfile_Mundane`.
pub mod treasure_profile_mundane {
    use crate::enums::TreasureItemType;
    use crate::logic::{at, count};
    use crate::tables::treasure_profile_mundane::MUNDANE_PROFILES;

    // ACE: TreasureProfile_Mundane.Roll
    pub fn roll(mundane_profile: i32) -> TreasureItemType {
        if mundane_profile < 1 || mundane_profile > count(&MUNDANE_PROFILES) {
            return TreasureItemType::Undef;
        }

        at(&MUNDANE_PROFILES, mundane_profile - 1).roll(0.0)
    }
}

/// ACE `WeaponTypeChance`.
pub mod weapon_type_chance {
    use crate::enums::TreasureWeaponType;
    use crate::tables::weapon_type_chance::RETAIL_CHANCES;

    /// ACE ignores `tier` here (the per-tier roll is commented out) and rolls `RetailChances`.
    // ACE: WeaponTypeChance.Roll
    pub fn roll(tier: i32) -> TreasureWeaponType {
        let _ = tier;
        //return weaponTiers[tier - 1].Roll();

        RETAIL_CHANCES.roll(0.0)
    }
}

/// ACE `WorkmanshipChance`.
pub mod workmanship_chance {
    use crate::logic::at;
    use crate::tables::workmanship_chance::WORKMANSHIP_CHANCES;

    // ACE: WorkmanshipChance.Roll
    pub fn roll(tier: i32) -> i32 {
        let tier = tier.clamp(1, 6);

        let workmanship_chance = at(&WORKMANSHIP_CHANCES, tier - 1);

        workmanship_chance.roll(0.0)
    }

    // ACE: WorkmanshipChance.GetModifier
    #[allow(clippy::cast_precision_loss)] // C#'s implicit int -> float
    pub fn get_modifier(workmanship: Option<i32>) -> f32 {
        let mut modifier = 1.0f32;

        if let Some(workmanship) = workmanship {
            modifier += workmanship as f32 / 9.0;
        }

        modifier
    }
}
