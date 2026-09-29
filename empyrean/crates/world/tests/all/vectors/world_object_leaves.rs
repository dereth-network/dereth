//! Vectors: fixtures/vectors/leaves/
//! Cloak proc/reduction, enlightenment checks, armor levels, scenery placement replay ACE
//! vectors; treasure-wielded/create-list parsing; small data classes.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use dereth_assets::world::ObjectDesc;
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::numerics::{Vector2, Vector3};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f32_of, f64_of, i64_of, u64_of, Case};
use empyrean_content::models::world::TreasureWielded;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    DamageType, EquipMask, PropertyFloat, PropertyInt, PropertyInt64, TargetingTactic,
};
use empyrean_entity::models::PropertiesCreateList;
use empyrean_entity::ObjectGuid;
use empyrean_store::models::shard::BiotaPropertiesBodyPart;
use empyrean_world::entity::create_list_set::CreateListSet;
use empyrean_world::entity::line2::Line2;
use empyrean_world::entity::treasure_wielded_set::TreasureWieldedSet;
use empyrean_world::entity::{cloak, core_plating, enlightenment, scenery};
use empyrean_world::managers::property_manager as pm;
use empyrean_world::world_objects::armor::{self, Armor};
use empyrean_world::world_objects::kinds::KindData;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    creature, creature_properties, pet_device, skill_alteration_device,
};
use empyrean_world::World;
use serde_json::Value;

const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
const CREATURE: ObjectGuid = ObjectGuid::new(0x8000_0001);
const CLOAK: ObjectGuid = ObjectGuid::new(0x7000_0001);

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_700_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(
        now,
        FakeDats::new()
            .with_spell_table(empyrean_dat::fake::sample::spell_table())
            .build()
            .expect("fake dats"),
    );
    pm::initialize(&mut w, true);
    let player = WorldObject {
        guid: PLAYER,
        container: Some(Box::default()),
        creature: Some(Box::default()),
        player: Some(Box::default()),
        kind: KindData::Player,
        ..Default::default()
    };
    let creature = WorldObject {
        guid: CREATURE,
        container: Some(Box::default()),
        creature: Some(Box::default()),
        kind: KindData::Creature,
        ..Default::default()
    };
    for mut o in [player, creature] {
        o.biota.properties_enchantment_registry = Some(Vec::new());
        w.objects.insert(o).expect("fresh");
    }
    w
}

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("leaves", name);
    assert!(!file.cases.is_empty(), "leaves/{name}: no cases");
    let failures: Vec<String> = file
        .cases
        .iter()
        .filter_map(|c| {
            each(c)
                .err()
                .map(|got| format!("in {} expected {} got {got}", c.input, c.output))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "leaves/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(10)].join("\n  ")
    );
}

fn opt_i64(c: &Case, key: &str) -> Option<i64> {
    i64_of(&c.input[key])
}

fn opt_i32(c: &Case, key: &str) -> Option<i32> {
    opt_i64(c, key).map(|v| i32::try_from(v).unwrap())
}

// ------------------------------------------------------------------------------------ Cloak

/// `Cloak.RollProc`: the proc, the new `UseTimestamp` and the draw count, against ACE (2,000
/// cases: cooldowns, item levels, -200 cloaks, the custom-scale properties).
#[test]
fn cloak_roll_proc_matches_ace() {
    let mut w = world();
    replay("cloak_roll_proc", |c| {
        let custom = c.input["custom"] == Value::Bool(true);
        pm::modify_bool(&w, "use_cloak_proc_custom_scale", custom);
        let (cooldown, max_base, plateau) = if custom {
            (
                f64_of(&c.input["cooldown"]).unwrap(),
                f64_of(&c.input["max_base"]).unwrap(),
                f64_of(&c.input["plateau"]).unwrap(),
            )
        } else {
            (5.0, 0.25, 0.30)
        };
        pm::modify_double(&w, "cloak_cooldown_seconds", cooldown, true);
        pm::modify_double(&w, "cloak_max_proc_base", max_base, true);
        pm::modify_double(&w, "cloak_max_proc_damage_percentage", plateau, true);

        let mut o = WorldObject {
            guid: CLOAK,
            kind: KindData::GenericObject(Box::default()),
            ..Default::default()
        };
        if let Some(t) = f64_of(&c.input["use_timestamp"]) {
            o.set_property(PropertyFloat::UseTimestamp, t);
        }
        if let Some(v) = opt_i64(c, "base_xp") {
            o.set_property(PropertyInt64::ItemBaseXp, v);
        }
        if let Some(v) = opt_i64(c, "total_xp") {
            o.set_property(PropertyInt64::ItemTotalXp, v);
        }
        if let Some(v) = opt_i32(c, "max_level") {
            o.set_property(PropertyInt::ItemMaxLevel, v);
        }
        if let Some(v) = opt_i32(c, "style") {
            o.set_property(PropertyInt::ItemXpStyle, v);
        }
        if let Some(v) = opt_i32(c, "weave") {
            o.set_property(PropertyInt::CloakWeaveProc, v);
        }
        w.objects.remove(CLOAK);
        w.objects.insert(o).expect("fresh");

        w.now.unix_time = f64_of(&c.input["now"]).unwrap();
        ThreadSafeRandom::seed(u64_of(&c.input["seed"]).unwrap());
        let pct = f32_of(&c.input["pct"]).unwrap();

        let result = catch_unwind(AssertUnwindSafe(|| cloak::roll_proc(&mut w, CLOAK, pct)));
        match (result, vectors::throws(&c.output)) {
            (Err(_), Some(_)) => Ok(()),
            (Err(_), None) => Err("panicked".into()),
            (Ok(r), Some(_)) => Err(format!("{r}, ACE threw")),
            (Ok(r), None) => {
                let after = w.objects.get(CLOAK).unwrap().use_timestamp();
                let next = ThreadSafeRandom::next(0, 1_000_000);
                let got = serde_json::json!([r, after, next]);
                let want = &c.output;
                let same = got[0] == want[0]
                    && got[2] == want[2]
                    && match (f64_of(&got[1]), f64_of(&want[1])) {
                        (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
                        (None, None) => true,
                        _ => false,
                    };
                if same {
                    Ok(())
                } else {
                    Err(got.to_string())
                }
            }
        }
    });
}

/// `Cloak.GetReducedAmount` (uint, int, float) and `ShowMessage`'s rounding, against ACE (800
/// cases; half the sources are players, whose reduction is halved).
#[test]
fn cloak_reduced_amount_matches_ace() {
    let w = world();
    replay("cloak_reduced_amount", |c| {
        let source = if c.input["player"] == Value::Bool(true) {
            PLAYER
        } else {
            CREATURE
        };
        let u = u32::try_from(u64_of(&c.input["u"]).unwrap()).unwrap();
        let i = i32::try_from(i64_of(&c.input["i"]).unwrap()).unwrap();
        let f = f32_of(&c.input["f"]).unwrap();
        let ru = cloak::get_reduced_amount_uint(&w, Some(source), u);
        let ri = cloak::get_reduced_amount_int(&w, Some(source), i);
        let rf = cloak::get_reduced_amount_float(&w, Some(source), f);
        let round = |x: f32| -> i32 {
            empyrean_common::dotnet::CsCast::cs_cast(empyrean_common::dotnet::math::round(
                f64::from(x),
            ))
        };
        let ok = u64_of(&c.output[0]) == Some(u64::from(ru))
            && i64_of(&c.output[1]) == Some(i64::from(ri))
            && f32_of(&c.output[2]).is_some_and(|x| x.to_bits() == rf.to_bits())
            && i64_of(&c.output[3]) == Some(i64::from(round(f)))
            && i64_of(&c.output[4]) == Some(i64::from(round(rf)));
        if ok {
            Ok(())
        } else {
            Err(format!("[{ru}, {ri}, {rf}, {}, {}]", round(f), round(rf)))
        }
    });
}

#[test]
fn cloak_predicates_and_the_pvp_reduction() {
    let mut o = WorldObject::default();
    assert!(!cloak::is_cloak(&o));
    o.set_property(
        PropertyInt::ValidLocations,
        EquipMask::Cloak.0.cast_signed(),
    );
    assert!(cloak::is_cloak(&o));
    assert!(!cloak::has_damage_proc(Some(&o)) && !cloak::has_damage_proc(None));
    o.set_property(PropertyInt::CloakWeaveProc, 2);
    assert!(cloak::has_damage_proc(Some(&o)));
    assert!(!cloak::has_proc_spell(Some(&o)));
    o.set_property(empyrean_entity::enums::PropertyDataId::ProcSpell, 5120);
    assert!(cloak::has_proc_spell(Some(&o)));
    assert_eq!(cloak::get_damage_reduction_amount(false), 200);
    assert_eq!(cloak::get_damage_reduction_amount(true), 100);
    assert_eq!(cloak::reduced_amount_uint(200, 150), 0);
    assert_eq!(cloak::reduced_amount_uint(200, 250), 50);
}

// ------------------------------------------------------------------------------------ Enlightenment

const LUM_AUGS: [PropertyInt; 11] = [
    PropertyInt::LumAugAllSkills,
    PropertyInt::LumAugSurgeChanceRating,
    PropertyInt::LumAugCritDamageRating,
    PropertyInt::LumAugCritReductionRating,
    PropertyInt::LumAugDamageRating,
    PropertyInt::LumAugDamageReductionRating,
    PropertyInt::LumAugItemManaUsage,
    PropertyInt::LumAugItemManaGain,
    PropertyInt::LumAugHealingRating,
    PropertyInt::LumAugSkilledCraft,
    PropertyInt::LumAugSkilledSpec,
];

/// `Enlightenment.VerifyLumAugs` and `VerifySocietyMaster`, against ACE (600 cases).
#[test]
fn enlightenment_checks_match_ace() {
    replay("enlightenment_checks", |c| {
        let mut o = WorldObject {
            kind: KindData::Player,
            ..Default::default()
        };
        for (k, prop) in LUM_AUGS.iter().enumerate() {
            let v = i32::try_from(i64_of(&c.input["augs"][k]).unwrap()).unwrap();
            if v != 0 {
                o.set_property(*prop, v);
            }
        }
        for (key, prop) in [
            ("celhan", PropertyInt::SocietyRankCelhan),
            ("eldweb", PropertyInt::SocietyRankEldweb),
            ("radblo", PropertyInt::SocietyRankRadblo),
        ] {
            if let Some(v) = opt_i32(c, key) {
                o.set_property(prop, v);
            }
        }
        let got = serde_json::json!([
            enlightenment::verify_lum_augs(&o),
            enlightenment::verify_society_master(&o)
        ]);
        if got == c.output {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

// ------------------------------------------------------------------------------------ Armor

/// `Armor.BaseArmorMod`, its eight `ArmorVs<Type>` and `Creature.GetArmorVsType`, against ACE
/// (600 cases; zero and negative base armor included).
#[test]
fn armor_levels_match_ace() {
    let mut w = world();
    let props = [
        PropertyFloat::ArmorModVsSlash,
        PropertyFloat::ArmorModVsPierce,
        PropertyFloat::ArmorModVsBludgeon,
        PropertyFloat::ArmorModVsFire,
        PropertyFloat::ArmorModVsCold,
        PropertyFloat::ArmorModVsAcid,
        PropertyFloat::ArmorModVsElectric,
        PropertyFloat::ArmorModVsNether,
    ];
    let types = [
        DamageType::Slash,
        DamageType::Pierce,
        DamageType::Bludgeon,
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Acid,
        DamageType::Electric,
        DamageType::Nether,
    ];
    replay("armor_levels", |c| {
        let vs: Vec<i32> = (0..8)
            .map(|k| i32::try_from(i64_of(&c.input["vs"][k]).unwrap()).unwrap())
            .collect();
        let part = BiotaPropertiesBodyPart {
            base_armor: i32::try_from(i64_of(&c.input["base"]).unwrap()).unwrap(),
            armor_vs_slash: vs[0],
            armor_vs_pierce: vs[1],
            armor_vs_bludgeon: vs[2],
            armor_vs_fire: vs[3],
            armor_vs_cold: vs[4],
            armor_vs_acid: vs[5],
            armor_vs_electric: vs[6],
            armor_vs_nether: vs[7],
            ..Default::default()
        };
        {
            let o = w.objects.get_mut(PLAYER).unwrap();
            for (k, prop) in props.iter().enumerate() {
                match f64_of(&c.input["mods"][k]) {
                    Some(v) => o.set_property(*prop, v),
                    None => o.remove_property(*prop),
                }
            }
        }
        let a = Armor::new(PLAYER, None, part);
        let mut got = vec![
            serde_json::json!(a.base_armor_mod(&mut w)),
            serde_json::json!(a.armor_vs_slash(&mut w)),
            serde_json::json!(a.armor_vs_pierce(&mut w)),
            serde_json::json!(a.armor_vs_bludgeon(&mut w)),
            serde_json::json!(a.armor_vs_fire(&mut w)),
            serde_json::json!(a.armor_vs_cold(&mut w)),
            serde_json::json!(a.armor_vs_acid(&mut w)),
            serde_json::json!(a.armor_vs_electric(&mut w)),
            serde_json::json!(a.armor_vs_nether(&mut w)),
        ];
        let o = w.objects.get(PLAYER).unwrap();
        for t in types {
            got.push(serde_json::json!(creature_properties::get_armor_vs_type(
                o, t
            )));
        }
        let want = c.output.as_array().unwrap();
        let same = got.iter().zip(want).enumerate().all(|(k, (g, e))| {
            if k < 9 {
                g.as_i64() == e.as_i64()
            } else {
                f64_of(g).map(f64::to_bits) == f64_of(e).map(f64::to_bits)
            }
        });
        if same {
            Ok(())
        } else {
            Err(Value::Array(got).to_string())
        }
    });
}

#[test]
fn armor_formula_edges() {
    // a zero base armor divides to NaN (x/0) or infinity: `(int)Math.Round(NaN)` is 0 on net10
    assert_eq!(armor::get_armor_vs_type_formula(0, 0, 1.0, 0), 0);
    // a negative base armor mod mirrors the resistance around 1
    assert_eq!(armor::get_armor_vs_type_formula(-10, -10, 1.0, -5), -15);
    assert_eq!(armor::get_armor_vs_type_formula(100, 150, 2.0, 50), 38);
}

// ------------------------------------------------------------------------------------ Scenery

/// `Scenery.Displace`, `ScaleObj` and `RotateObj`, against ACE (1,000 cases).
#[test]
fn scenery_placement_matches_ace() {
    replay("scenery_placement", |c| {
        let f = |k: &str| f32_of(&c.input[k]).unwrap();
        let u = |k: &str| u32::try_from(u64_of(&c.input[k]).unwrap()).unwrap();
        let d = ObjectDesc {
            obj_id: DataId(0x0100_0001),
            base_loc: Frame::new(
                Vec3::new(f("ox"), f("oy"), 0.0),
                Quat::new(1.0, 0.0, 0.0, 0.0),
            ),
            freq: 0.5,
            displace_x: f("dx"),
            displace_y: f("dy"),
            min_scale: f("min"),
            max_scale: f("max"),
            max_rot: f("rot"),
            min_slope: 0.0,
            max_slope: 1.0,
            align: 0,
            orient: 0,
            weenie_obj: 0,
        };
        let (x, y, k) = (u("x"), u("y"), u("k"));
        let p = scenery::displace(&d, x, y, k);
        let got = [
            p.x,
            p.y,
            scenery::scale_obj(&d, x, y, k),
            scenery::rotate_obj(&d, x, y, k),
        ];
        let same = got
            .iter()
            .enumerate()
            .all(|(i, g)| f32_of(&c.output[i]).is_some_and(|e| e.to_bits() == g.to_bits()));
        if same {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

// ------------------------------------------------------------------------------------ small types

/// `Line2` (hand-derived from `Entity/Line2.cs`).
#[test]
fn line2_sides() {
    let l = Line2::from_coords(0.0, 0.0, 10.0, 0.0);
    assert_eq!(Line2::determinant_xy(&l, 5.0, 2.0), 20.0);
    assert!(l.right_side(Vector2::new(5.0, 2.0)) && l.right_side_xy(5.0, 2.0));
    assert!(l.left_side(Vector2::new(5.0, -2.0)) && l.left_side_xy(5.0, -2.0));
    assert!(l.collinear(Vector2::new(3.0, 0.0)) && l.collinear_xy(-3.0, 0.0));
    // a diagonal line exercises both cross terms: (4 * 0) - (4 * 4)
    let d = Line2::from_coords(0.0, 0.0, 4.0, 4.0);
    assert_eq!(Line2::determinant_xy(&d, 4.0, 0.0), -16.0);
    assert!(d.left_side_xy(4.0, 0.0) && d.right_side_xy(0.0, 4.0));
    let l3 = Line2::from_vector3(Vector3::new(1.0, 2.0, 3.0), Vector3::new(4.0, 5.0, 6.0));
    assert_eq!(
        l3,
        Line2::new(Vector2::new(1.0, 2.0), Vector2::new(4.0, 5.0))
    );
}

/// `CreateListSet` (hand-derived): trophies are non-zero wcids, and the probabilities sum the
/// shades in double.
#[test]
fn create_list_set_splits_trophies_and_nones() {
    let mut s = CreateListSet::new();
    s.add(PropertiesCreateList {
        weenie_class_id: 12,
        shade: 0.1,
        ..Default::default()
    });
    s.add(PropertiesCreateList {
        weenie_class_id: 0,
        shade: 0.7,
        ..Default::default()
    });
    s.add(PropertiesCreateList {
        weenie_class_id: 13,
        shade: 0.2,
        ..Default::default()
    });
    assert_eq!(s.trophies().len(), 2);
    assert_eq!(s.none().len(), 1);
    #[allow(clippy::cast_possible_truncation)]
    let sum = (f64::from(0.1f32) + f64::from(0.7f32) + f64::from(0.2f32)) as f32;
    assert_eq!(s.total_probability().to_bits(), sum.to_bits());
    assert_eq!(s.none_probability(), 0.7);
}

fn tw(id: u32, start: bool, sub: bool, cont: bool, p: f32) -> TreasureWielded {
    TreasureWielded {
        id,
        weenie_class_id: id,
        set_start: start,
        has_sub_set: sub,
        continues_previous_set: cont,
        probability: p,
        ..Default::default()
    }
}

/// `TreasureWieldedSet`'s parse (hand-derived from `Entity/TreasureWieldedSet.cs`): a set, a
/// subset under its second item, and the item continuing the set after the subset.
#[test]
fn treasure_wielded_set_parses_subsets() {
    let rows = vec![
        tw(1, true, false, false, 0.5),
        tw(2, false, true, false, 0.25),
        tw(3, true, false, false, 1.0),
        tw(4, false, false, false, 0.0),
        tw(5, true, false, true, 0.1),
        tw(6, true, false, false, 1.0),
    ];
    let set = TreasureWieldedSet::new(&rows, 0, 0);
    assert_eq!(
        set.items.len(),
        3,
        "1, 2 (with the 3-4 subset), 5; 6 starts a new set"
    );
    assert_eq!(set.items[1].subset.as_ref().map(|s| s.items.len()), Some(2));
    assert_eq!(set.total_nested_items(), 5);
    assert_eq!(set.total_nested_sets(), 2);
    assert_eq!(set.get_max_depth(0), 2);
    assert_eq!(
        set.total_probability(),
        1.0,
        "0.85, raised to the 100% minimum"
    );
}

// ------------------------------------------------------------------------------------ predicates

#[test]
fn leaf_predicates() {
    let mut o = WorldObject::default();
    o.biota.weenie_class_id = core_plating::CORE_PLATING_INTEGRATOR;
    assert!(core_plating::is_core_plating_device(&o));
    o.biota.weenie_class_id = 49485;
    assert!(pet_device::is_encapsulated_spirit(&o));
    // Advocate's predicates take the world and a guid (6.4's port); empyrean-command's tests cover them.

    o.set_property(PropertyInt::TsysMutationData, 0x0403_0201);
    assert_eq!(
        (
            o.material_code(),
            o.gem_code(),
            o.color_code(),
            o.spell_selection_code()
        ),
        (Some(1), Some(2), Some(3), Some(4))
    );
    o.set_property(PropertyInt::PkLevelModifier, 1);
    assert_eq!(o.pk_level(), empyrean_entity::enums::PKLevel::PK);

    // Creature.IsNPC: not a player, not attackable, no targeting tactic
    let mut c = WorldObject {
        creature: Some(Box::default()),
        kind: KindData::Creature,
        ..Default::default()
    };
    c.set_property(empyrean_entity::enums::PropertyBool::Attackable, false);
    assert!(creature::is_npc(&c));
    let mut c2 = WorldObject {
        creature: Some(Box::default()),
        kind: KindData::Creature,
        ..Default::default()
    };
    c2.set_property(empyrean_entity::enums::PropertyBool::Attackable, false);
    c2.set_property(PropertyInt::TargetingTactic, 1);
    assert_eq!(c2.targeting_tactic(), TargetingTactic(1));
    assert!(!creature::is_npc(&c2));

    let d = skill_alteration_device::SkillAlterationType::Lower;
    let mut g = WorldObject::default();
    g.set_type_of_alteration(d);
    assert_eq!(g.type_of_alteration(), d);
}

/// `TreasureWieldedTable` (TreasureWieldedTable.cs): the rows split into top-level sets at each
/// SetStart (a stray non-start row is skipped with a warning), with the sets' nested counts.
#[test]
fn treasure_wielded_table_splits_the_top_level_sets() {
    use empyrean_world::entity::treasure_wielded_table::TreasureWieldedTable;
    let rows = vec![
        tw(9, false, false, false, 0.5),
        tw(1, true, false, false, 0.5),
        tw(2, false, true, false, 0.25),
        tw(3, true, false, false, 1.0),
        tw(4, false, false, false, 0.0),
        tw(5, true, false, true, 0.1),
        tw(6, true, false, false, 1.0),
    ];
    let t = TreasureWieldedTable::new(&rows);
    assert_eq!(t.sets.len(), 2);
    assert_eq!(t.sets[0].items[0].item.id, 1);
    assert_eq!(t.sets[1].items[0].item.id, 6);
    assert_eq!(t.total_nested_sets(), 3);
    assert_eq!(t.max_depth(), 2);
}

/// `AttackDamage` / `AttackList` (unused by ACE): the total per source, the last hit's critical
/// flag, the top damager, and OnHeal's integer-division scalar (ACE-BUG: 1 - 5/10 is 1).
#[test]
fn attack_damage_and_attack_list() {
    use empyrean_entity::ObjectGuid;
    use empyrean_world::entity::attack_damage::AttackDamage;
    use empyrean_world::entity::attack_list::AttackList;
    let (a, b) = (Some(ObjectGuid::new(1)), Some(ObjectGuid::new(2)));
    let hit = |source, amount, critical_hit| AttackDamage {
        source,
        amount,
        time: empyrean_common::dotnet::DotNetDateTime::default(),
        is_critical: critical_hit,
    };
    let attacks = vec![hit(a, 10, false), hit(b, 15, false), hit(a, 10, true)];
    assert_eq!(AttackDamage::get_total_damage(&attacks, a), 20);
    assert!(AttackDamage::last_hit_critical(&attacks));
    assert!(!AttackDamage::last_hit_critical(&[]));
    assert_eq!(AttackDamage::get_top_damager(&attacks), a);
    let mut list = AttackList::from_attacks(&attacks);
    list.on_heal(5, 10);
    assert_eq!(list.damagers, [(a, 20), (b, 15)]);
    list.on_heal(10, 10);
    assert_eq!(list.damagers, [(a, 0), (b, 0)]);
}

/// `MutateFilters` (MutateFilters.cs): the six 0x0E filter ids; an unknown or missing id is Undef.
#[test]
fn mutate_filters_by_id() {
    use empyrean_entity::enums::MutateFilter;
    use empyrean_world::entity::mutate_filters::{get_mutate_filters, has_mutate_filter};
    let mut o = WorldObject::default();
    assert_eq!(get_mutate_filters(&o), MutateFilter::Undef);
    assert!(!has_mutate_filter(&o, MutateFilter::Base));
    o.set_property(
        empyrean_entity::enums::PropertyDataId::MutateFilter,
        0x0E00_0013,
    );
    assert_eq!(
        get_mutate_filters(&o),
        MutateFilter::Base | MutateFilter::ArmorModVsType | MutateFilter::ShieldValue
    );
    assert!(
        has_mutate_filter(&o, MutateFilter::ShieldValue)
            && !has_mutate_filter(&o, MutateFilter::WeaponTime)
    );
    o.set_property(
        empyrean_entity::enums::PropertyDataId::MutateFilter,
        0x0E00_0099,
    );
    assert_eq!(get_mutate_filters(&o), MutateFilter::Undef);
}

/// The data classes: `StarterItem()` stacks 1, `HeldItem`'s fields.
#[test]
fn small_data_classes() {
    let s = empyrean_world::entity::starter_item::StarterItem::default();
    assert_eq!((s.weenie_id, s.stack_size), (0, 1));
    let h = empyrean_world::entity::held_item::HeldItem::new(
        7,
        1,
        empyrean_entity::enums::EquipMask::Shield,
    );
    assert_eq!(
        (h.guid, h.location_id, h.equip_mask),
        (7, 1, empyrean_entity::enums::EquipMask::Shield)
    );
}

/// V229: the set a Gear Knight must have integrated (and the Core Plating Integrator accepts) is the
/// client's clothing and armour without the cloak: exactly what ACE's `Clothing | Armor` tests,
/// since ACE's `Clothing` lacks the cloak bit and its stray top bit never appears in item data.
#[test]
fn gear_knight_plated_locations_are_clothing_and_armor_without_the_cloak() {
    use empyrean_entity::enums::EquipMask;
    use empyrean_world::entity::core_plating::GEAR_KNIGHT_PLATED_LOCATIONS;
    assert_eq!(GEAR_KNIGHT_PLATED_LOCATIONS.0, 0x7FFF);
    assert_eq!(
        GEAR_KNIGHT_PLATED_LOCATIONS.0,
        (EquipMask::Clothing.0 | EquipMask::Armor.0) & EquipMask::All.0
    );
    assert_eq!(
        GEAR_KNIGHT_PLATED_LOCATIONS.0 & EquipMask::Cloak.0,
        0,
        "a cloak needs no integrator"
    );
}
