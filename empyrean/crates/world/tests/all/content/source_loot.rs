//! Divergence: V466
//! Six-group source loot selection, mutations and typed consumer dispatch.
//! Fixture: original synthetic loot policy, templates, spells and isolated world.

use crate::support::treasure_world::H;
use crate::support::treasure_world::*;
use empyrean_common::era::{EraExt, EraId};
use empyrean_content::loot::{GroupedTreasureDeath, SlotGroup, SourceLootPolicy};
use empyrean_entity::enums::ItemType;
use empyrean_world::source_loot::{
    self,
    rng::{self, Favor, LootRng},
};
use std::collections::VecDeque;

fn policy() -> SourceLootPolicy {
    SourceLootPolicy::from_json(include_bytes!(
        "../../../../../fixtures/source-loot/synthetic.json"
    ))
    .unwrap()
}
fn row() -> GroupedTreasureDeath {
    GroupedTreasureDeath {
        id: 1,
        treasure_type: 99,
        tier: 1,
        quality: 0.0,
        groups: [SlotGroup {
            chance: 100,
            min: 1,
            max: 1,
        }; 6],
    }
}
#[derive(Debug)]
struct Low;
impl LootRng for Low {
    fn unit(&mut self) -> f64 {
        0.0
    }
}
#[derive(Debug)]
struct Script(VecDeque<i32>);
impl LootRng for Script {
    fn unit(&mut self) -> f64 {
        panic!("unexpected real draw")
    }
    fn integer(&mut self, lo: i32, hi: i32) -> i32 {
        let v = self.0.pop_front().expect("unexpected integer draw");
        assert!((lo..=hi).contains(&v), "{v} outside {lo}..{hi}");
        v
    }
}

fn harness(p: &SourceLootPolicy) -> H {
    let mut h = H::new();
    h.w.era = EraId::Launch.rules();
    let mut content = content()
        .era(EraId::Launch)
        .source_loot_policy(p.clone())
        .grouped_treasure(row());
    for id in 8001..=8017 {
        let (ty, kind) = match id {
            8001 => (ItemType::MeleeWeapon, WeenieType::MeleeWeapon),
            8002 => (ItemType::MissileWeapon, WeenieType::MissileLauncher),
            8017 => (ItemType::MissileWeapon, WeenieType::Missile),
            8003 => (ItemType::Armor, WeenieType::Clothing),
            8004 => (ItemType::Clothing, WeenieType::Clothing),
            8005 => (ItemType::Jewelry, WeenieType::Generic),
            8006 => (ItemType::Gem, WeenieType::Gem),
            8007 => (ItemType::Caster, WeenieType::Caster),
            8008 => (ItemType::Misc, WeenieType::Scroll),
            8009 => (ItemType::SpellComponents, WeenieType::SpellComponent),
            _ => (ItemType::Misc, WeenieType::Generic),
        };
        let template = setup_weenie(id, &format!("synthetic-{id}"), kind, "Synthetic")
            .with_int(PropertyInt::ItemType, ty.0 as i32)
            .with_int(PropertyInt::Value, 100)
            .with_int(PropertyInt::Damage, 10)
            .with_int(PropertyInt::WeaponTime, 50)
            .with_int(PropertyInt::ArmorLevel, 20)
            .with_int(PropertyInt::EncumbranceVal, 100)
            .with_int(PropertyInt::ValidLocations, EquipMask::ChestArmor.0 as i32)
            .with_int(PropertyInt::WeaponSkill, Skill::Sword.0)
            .with_int(PropertyInt::MaxStackSize, 100)
            .with_int(PropertyInt::StackUnitEncumbrance, 1)
            .with_int(PropertyInt::StackUnitValue, 100)
            .with_int(PropertyInt::TsysMutationData, 0x10000)
            .with_int(
                PropertyInt::DamageType,
                if id == 8002 {
                    DamageType::Acid.0
                } else {
                    DamageType::Slash.0
                },
            )
            .with_int(
                PropertyInt::DefaultCombatStyle,
                if id == 8017 { 0x80 } else { 1 },
            )
            .with_float(PropertyFloat::DamageVariance, 0.4)
            .with_float(PropertyFloat::DamageMod, 1.5)
            .with_float(PropertyFloat::ArmorModVsAcid, 1.2);
        content = content.weenie(template);
    }
    content = content.weenie(generator(
        8099,
        WeeniePropertiesGenerator {
            probability: -1.0,
            weenie_class_id: 99,
            init_create: 1,
            max_create: 1,
            where_create: RegenLocationType::Treasure.0,
            ..Default::default()
        },
    ));
    h.w.content = Arc::new(content);
    h
}
#[test]
fn counts_precede_independent_slot_chances_and_disabled_groups_consume_no_draws() {
    let p = policy();
    let mut h = harness(&p);
    let mut row = row();
    row.groups = [SlotGroup::default(); 6];
    row.groups[4] = SlotGroup {
        chance: 50,
        min: 2,
        max: 2,
    };
    let mut r = Script([2, 49, 0, 0, 0, 50].into());
    let made = source_loot::generate_with(&mut h.w, &row, &p, &mut r);
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].biota.weenie_class_id, 8011);
    assert!(r.0.is_empty());
    row.groups[4] = SlotGroup::default();
    assert!(source_loot::generate_with(&mut h.w, &row, &p, &mut r).is_empty());
}
#[test]
fn weighted_integer_boundaries_and_zero_weight_fallback_are_explicit() {
    assert_eq!(rng::weighted(&mut Script([2].into()), &[3, 2]), 0);
    assert_eq!(rng::weighted(&mut Script([3].into()), &[3, 2]), 1);
    assert_eq!(rng::weighted(&mut Script([1].into()), &[0, 0]), 1);
    assert_eq!(rng::favored(&mut Low, 3, 6, Favor::Mid, 2.0, 0.0), 3);
    assert_eq!(rng::real(&mut Low, 0.75, 1.0, None, 0.0, 0.0), 0.75);
    assert_eq!(rng::modifier_round(1.1234), 1.123);
}

#[test]
fn chance_boundaries_preserve_source_float_percentages_before_quality() {
    assert!(rng::chance(&mut Script([30].into()), 0.3, 0.0));
    assert!(!rng::chance(&mut Script([31].into()), 0.3, 0.0));
    assert!(rng::chance(&mut Script([14].into()), 0.1, 0.25));
    assert!(!rng::chance(&mut Script([15].into()), 0.1, 0.25));
}
#[test]
fn all_six_groups_and_each_pool_family_create_items() {
    let p = policy();
    let mut h = harness(&p);
    let all = source_loot::generate_with(&mut h.w, &row(), &p, &mut Low);
    assert_eq!(all.len(), 6);
    for kind in [
        "melee-weapons",
        "missile-weapons",
        "armor",
        "clothing",
        "jewelry",
        "gems",
        "casters",
        "scrolls",
        "components",
        "manaStones",
        "healingKits",
        "potions",
        "food",
        "coins",
        "lockpicks",
        "parchment",
    ] {
        let tier = if matches!(kind, "manaStones" | "potions") {
            2
        } else {
            1
        };
        assert!(
            source_loot::generate_kind(&mut h.w, &p, &p.tiers[&tier], kind, 0.0, &mut Low)
                .is_some(),
            "{kind}"
        );
    }
}
#[test]
fn direct_launcher_armor_and_mundane_mutations_do_not_use_ace_scripts() {
    let p = policy();
    let mut h = harness(&p);
    let melee =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "melee-weapons", 0.0, &mut Low)
            .unwrap();
    assert_eq!(melee.get_property(PropertyInt::Damage), Some(12));
    assert_eq!(melee.get_property(PropertyInt::WeaponTime), Some(25));
    assert_eq!(melee.get_property(PropertyFloat::DamageVariance), Some(0.4));
    let bow =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "missile-weapons", 0.0, &mut Low)
            .unwrap();
    assert_eq!(bow.get_property(PropertyFloat::DamageMod), Some(1.8));
    assert_eq!(bow.get_property(PropertyInt::Damage), Some(10));
    let armor =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "armor", 0.0, &mut Low).unwrap();
    assert_eq!(armor.get_property(PropertyInt::ArmorLevel), Some(30));
    assert_eq!(armor.get_property(PropertyInt::EncumbranceVal), Some(80));
    assert_eq!(
        armor.get_property(PropertyFloat::ArmorModVsAcid),
        Some(1.2),
        "source integer increment is zero"
    );
    let food =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "food", 0.0, &mut Low).unwrap();
    assert_eq!(food.get_property(PropertyInt::Value), Some(100));
    assert_eq!(food.get_property(PropertyInt::ResistItemAppraisal), None);
}
#[test]
fn gems_are_single_cast_and_clothing_is_single_spell() {
    let mut p = policy();
    p.tiers.get_mut(&1).unwrap().chance_of_spells = 1.0;
    let mut h = harness(&p);
    let gem =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "gems", 0.0, &mut Low).unwrap();
    assert_eq!(gem.biota.properties_spell_book.as_ref().unwrap().len(), 1);
    assert_eq!(gem.get_property(PropertyDataId::Spell), Some(1));
    assert_eq!(gem.get_property(PropertyInt::ItemDifficulty), Some(0));
    assert_eq!(
        gem.get_property(PropertyInt::ItemCurMana),
        gem.get_property(PropertyInt::ItemManaCost)
    );
    assert_eq!(
        gem.get_property(PropertyInt::ItemCurMana),
        Some(1),
        "synthetic base mana zero floors to one"
    );
    let clothing =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "clothing", 0.0, &mut Low).unwrap();
    assert_eq!(
        clothing.biota.properties_spell_book.as_ref().unwrap().len(),
        1
    );
    assert_eq!(
        clothing.get_property(PropertyInt::HeritageGroup),
        Some(1),
        "Aluvian is not the invalid enum value"
    );
    assert_eq!(
        clothing.get_property(PropertyInt::ItemAllegianceRankLimit),
        Some(2)
    );
}
#[test]
fn caster_use_spell_is_separate_and_has_source_spellcraft_floor() {
    let p = policy();
    let mut h = harness(&p);
    let caster =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "casters", 0.0, &mut Low).unwrap();
    assert_eq!(caster.get_property(PropertyDataId::Spell), Some(1));
    assert!(caster
        .biota
        .properties_spell_book
        .as_ref()
        .is_none_or(|b| b.is_empty()));
    assert_eq!(caster.get_property(PropertyInt::ItemSpellcraft), Some(25));
    assert_eq!(caster.get_property(PropertyDataId::ItemSkillLimit), None);
}
#[test]
fn typed_dispatch_works_and_launch_does_not_fall_back_to_ace() {
    let p = policy();
    let mut h = harness(&p);
    let profile = lgf::death_loot_profile(&h.w, 99, None).unwrap();
    assert_eq!(lgf::create_death_loot(&mut h.w, &profile).len(), 6);
    assert!(lgf::death_loot_profile(&h.w, PYREALS_DID, None).is_none());
    h.creature(Class::Creature, MONSTER, "Loot creature");
    h.o_mut(MONSTER)
        .set_property(PropertyInt::ItemsCapacity, 100);
    h.o_mut(MONSTER)
        .set_property(PropertyDataId::DeathTreasureType, 99);
    assert_eq!(
        creature_death::death_treasure(&h.w, MONSTER)
            .unwrap()
            .tier(),
        1
    );
    h.o_mut(MONSTER)
        .set_property(PropertyDataId::InventoryTreasureType, 99);
    creature_equipment::generate_inventory_treasure(&mut h.w, MONSTER);
    assert_eq!(
        empyrean_world::world_objects::container::inventory(h.o(MONSTER)).len(),
        6
    );
}

#[test]
fn material_and_elemental_mutation_keep_base_identity_and_price_the_generated_material() {
    let mut p = policy();
    let c = &mut p.pools.get_mut("melee-weapons").unwrap()[0];
    c.possible_materials = vec!["metal".into()];
    c.entries[0].elemental_variants = vec![8002];
    c.mutation_bands[0].elemental_chance = 1.0;
    let mut h = harness(&p);
    let item =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "melee-weapons", 0.0, &mut Low)
            .unwrap();
    assert_eq!(item.biota.weenie_class_id, 8001);
    assert_eq!(
        item.get_property(PropertyInt::DamageType),
        Some(DamageType::Acid.0)
    );
    assert_eq!(item.get_property(PropertyInt::MaterialType), Some(61));
    assert_eq!(item.get_property(PropertyInt::Value), Some(180));
    assert_eq!(item.get_property(PropertyInt::UiEffects), Some(0x100));
}
#[test]
fn thrown_stacks_keep_mutated_damage_and_total_value() {
    let mut p = policy();
    p.pools.get_mut("missile-weapons").unwrap().remove(0);
    p.pools.get_mut("missile-weapons").unwrap()[0].entries[0].stack_size = 10;
    let mut h = harness(&p);
    let mut item =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "missile-weapons", 0.0, &mut Low)
            .unwrap();
    assert_eq!(item.get_property(PropertyInt::Damage), Some(12));
    assert_eq!(item.get_property(PropertyFloat::DamageMod), Some(1.5));
    assert_eq!(item.get_property(PropertyInt::StackSize), Some(10));
    assert_eq!(item.get_property(PropertyInt::MaxStackSize), Some(10));
    assert_eq!(item.get_property(PropertyInt::EncumbranceVal), Some(10));
    assert_eq!(item.get_property(PropertyInt::Value), Some(900));
    assert_eq!(item.get_property(PropertyInt::StackUnitValue), Some(90));
    // Stack helpers used by splitting and consumption retain the generated price and damage.
    item.set_stack_size(Some(4));
    assert_eq!(item.get_property(PropertyInt::Value), Some(360));
    assert_eq!(item.get_property(PropertyInt::Damage), Some(12));
}
#[test]
fn base_tier_fallback_never_selects_an_entry_above_the_loot_tier() {
    let mut p = policy();
    p.pools.get_mut("melee-weapons").unwrap()[0].entries[0].base_tier = 2;
    let mut h = harness(&p);
    assert!(
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "melee-weapons", 0.0, &mut Low)
            .is_none()
    );
    assert!(
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&2], "melee-weapons", 0.0, &mut Low)
            .is_some()
    );
    p.pools.get_mut("melee-weapons").unwrap()[0].entries[0].base_tier = 0;
    assert!(
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "melee-weapons", 0.0, &mut Low)
            .is_some()
    );
}
#[test]
fn local_dependency_validation_rejects_missing_templates_and_spells() {
    let p = policy();
    let h = harness(&p);
    source_loot::validate_local_inputs(&h.w, &p).unwrap();
    let mut bad = p.clone();
    bad.pools.get_mut("gems").unwrap()[0].entries[0].wcid = 999999;
    assert!(source_loot::validate_local_inputs(&h.w, &bad).is_err());
    let mut bad = p;
    for families in bad.spells.values_mut() {
        families[0].spells[0] = 999999;
    }
    assert!(source_loot::validate_local_inputs(&h.w, &bad).is_err());
}

#[test]
fn grouped_loot_reaches_corpses_and_treasure_generators() {
    let p = policy();
    let mut h = harness(&p);
    h.load_landblock();
    let generator = h.place_new(8099, GEN);
    h.regenerate(generator);
    assert_eq!(h.spawned(generator).len(), 6);
    h.creature(Class::Creature, MONSTER, "Creature");
    h.o_mut(MONSTER)
        .set_property(PropertyDataId::DeathTreasureType, 99);
    assert!(lm::add_object(&mut h.w, MONSTER, false));
    creature_death::create_corpse(&mut h.w, MONSTER, None, false);
    let corpse = (0x80000000..0x80000100)
        .map(ObjectGuid::new)
        .find(|g| h.w.objects.get(*g).is_some_and(WorldObject::is_corpse))
        .unwrap();
    assert_eq!(creature_death::container_inventory(&h.w, corpse).len(), 6);
}

#[derive(Debug)]
struct High;
impl LootRng for High {
    fn unit(&mut self) -> f64 {
        0.999999
    }
}

#[test]
fn highest_tier_uses_its_mutation_band_and_sixth_spell_level_with_bounded_value() {
    let mut p = policy();
    let t = p.tiers.get_mut(&5).unwrap();
    t.chance_of_spells = 1.0;
    t.min_spell_level = 6;
    t.max_spell_level = 6;
    t.preferred_spell_level = 6;
    p.profile.spell_properties.spell_power = vec![20, 40, 60, 80, 100, 120];
    p.profile.spell_properties.spell_mana = vec![10, 20, 30, 40, 50, 60];
    let band = &mut p.pools.get_mut("melee-weapons").unwrap()[0].mutation_bands[4];
    band.min_damage_bonus = 20;
    band.max_damage_bonus = 20;
    let mut h = harness(&p);
    let item =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&5], "melee-weapons", 0.0, &mut High)
            .unwrap();
    assert_eq!(item.get_property(PropertyInt::Damage), Some(30));
    assert_eq!(item.get_property(PropertyInt::ItemMaxMana), Some(60));
    assert_eq!(item.get_property(PropertyInt::ItemSpellcraft), Some(132));
    assert_eq!(item.get_property(PropertyInt::Value), Some(1430));
    assert_eq!(
        item.get_property(PropertyInt::ResistItemAppraisal),
        Some(225)
    );
}
#[test]
fn spell_count_cap_is_total_and_single_spell_kinds_ignore_the_rolled_extra_count() {
    let mut p = policy();
    p.tiers.get_mut(&1).unwrap().chance_of_spells = 1.0;
    for name in ["armor-item", "clothing-item"] {
        let pool = p.spells.get_mut(name).unwrap();
        for n in 0..2 {
            let mut family = pool[0].clone();
            family.spell_name = format!("Extra {n}");
            pool.push(family);
        }
    }
    let mut h = harness(&p);
    let armor =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "armor", 0.0, &mut High).unwrap();
    assert_eq!(armor.get_property(PropertyInt::ItemMaxMana), Some(90));
    let clothing =
        source_loot::generate_kind(&mut h.w, &p, &p.tiers[&1], "clothing", 0.0, &mut High).unwrap();
    assert_eq!(clothing.get_property(PropertyInt::ItemMaxMana), Some(30));
}
#[test]
fn source_item_rank_gate_is_enforced_without_changing_eor() {
    let p = policy();
    let mut h = harness(&p);
    h.creature(Class::Player, PLAYER, "Player");
    let mut session = empyrean_world::sessions::SessionData::default();
    session.set_player(Some(PLAYER));
    h.w.sessions.insert(
        empyrean_net::SessionId {
            client_id: 1,
            generation: 1,
        },
        session,
    );
    h.o_mut(PLAYER).set_property(PropertyInt::AllegianceRank, 1);
    let guid = ObjectGuid::new(0x80001000);
    let mut item = WorldObject::allocate(Class::WorldObject);
    item.guid = guid;
    item.biota.id = guid.full();
    item.set_property(PropertyInt::ItemAllegianceRankLimit, 2);
    h.w.objects.insert(item).unwrap();
    use empyrean_world::world_objects::world_object_use::world_object_check_use_requirements as check;
    assert!(!check(&mut h.w, guid, PLAYER).success);
    h.o_mut(PLAYER).set_property(PropertyInt::AllegianceRank, 2);
    assert!(check(&mut h.w, guid, PLAYER).success);
    h.o_mut(PLAYER).set_property(PropertyInt::AllegianceRank, 1);
    h.w.era = EraId::Eor.rules();
    assert!(check(&mut h.w, guid, PLAYER).success);
}
