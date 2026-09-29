//! Vectors: fixtures/vectors/stats/
//! Creature attributes/vitals/skills and formulas follow ACE, with retail deviations; one ignored
//! cross-check vs client model.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SkillBase, SkillFormula};
use dereth_primitives::DataId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::not_ported;
use empyrean_common::vectors::{self, f32_of, f64_of, same_f32, same_f64, throws, u64_of, Case};
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, SkillTable, XpTable};
use empyrean_dat::{DatManager, FakeDats};
use empyrean_entity::enums::{
    PropertyAttribute, PropertyAttribute2nd, PropertyFloat, PropertyInt, PropertyInt64,
    PropertyString, Skill, SkillAdvancementClass,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_world::dispatch::{self, Class};
use empyrean_world::entity::attribute_formula;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::entity::creature_attribute::{CreatureAttribute, StatCtx};
use empyrean_world::world_objects::entity::creature_skill::CreatureSkill;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    creature_vitals, player_attributes, player_skills, player_vitals,
};
use empyrean_world::World;
use serde_json::Value;

const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
pub(crate) const PLAYER: u32 = 0x5000_0001;

// ------------------------------------------------------------------ the synthetic dats

fn table_case() -> Case {
    let file = vectors::load_named("stats", "tables");
    file.cases
        .into_iter()
        .next()
        .expect("stats/tables: one case")
}

fn u32s(v: &Value) -> Vec<u32> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|x| u32::try_from(x.as_u64().expect("uint")).expect("u32"))
        .collect()
}

fn formula(v: &Value) -> SkillFormula {
    let g = |k: &str| u32::try_from(v[k].as_u64().expect(k)).expect("u32");
    SkillFormula {
        w: g("w"),
        x: g("x"),
        y: g("y"),
        z: g("z"),
        attr1: g("attr1"),
        attr2: g("attr2"),
    }
}

/// The tables of `stats/tables.json`: the skill table *before* `AddRetiredSkills`, which
/// `DatManager` runs itself.
fn tables() -> (XpTable, SkillTable, SecondaryAttributeTable) {
    let out = table_case().output;
    let xp = &out["xp"];
    let xp_table = XpTable {
        id: DataId(file_id::XP_TABLE),
        attribute_xp: u32s(&xp["attribute"]),
        vital_xp: u32s(&xp["vital"]),
        trained_xp: u32s(&xp["trained"]),
        specialized_xp: u32s(&xp["specialized"]),
        level_xp: vec![0, 0, 1000, 2500],
        level_credits: vec![0, 0, 1, 1],
    };
    let retired: Vec<u32> = u32s(&out["retired_added"]);
    let mut skills = BTreeMap::new();
    for (k, v) in out["skills"].as_object().expect("skills") {
        let id: u32 = k.parse().expect("skill id");
        if retired.contains(&id) {
            continue;
        }
        skills.insert(
            id,
            SkillBase {
                description: String::new(),
                name: String::new(),
                icon: 0,
                trained_cost: i32::try_from(v["trained_cost"].as_i64().expect("tc")).expect("i32"),
                specialized_cost: i32::try_from(v["specialized_cost"].as_i64().expect("sc"))
                    .expect("i32"),
                category: 0,
                chargen_use: 0,
                min_level: u32::try_from(v["min_level"].as_u64().expect("min")).expect("u32"),
                formula: formula(&v["formula"]),
                upper_bound: 0.0,
                lower_bound: 0.0,
                learn_mod: 0.0,
            },
        );
    }
    let skill_table = SkillTable {
        id: DataId(file_id::SKILL_TABLE),
        buckets: 64,
        skills,
    };
    let vitals = &out["vitals"];
    let secondary = SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: formula(&vitals["health"]),
        stamina: formula(&vitals["stamina"]),
        mana: formula(&vitals["mana"]),
    };
    (xp_table, skill_table, secondary)
}

fn dats() -> Arc<DatManager> {
    let (xp, skills, secondary) = tables();
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_xp_table(xp)
        .with_skill_table(skills)
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, secondary)
        .build()
        .expect("fake dats")
}

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, dats())
}

// ------------------------------------------------------------------ creatures from vector inputs

fn u(v: &Value) -> u32 {
    u32::try_from(u64_of(v).unwrap_or_else(|| panic!("not a uint: {v}"))).expect("u32")
}

fn i(v: &Value) -> i32 {
    i32::try_from(v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))).expect("i32")
}

/// The stat part of `Creature.SetEphemeralValues` exactly as the harness builds it (no vital
/// fill): vitals, attributes, then a `Skills` entry per biota skill.
fn wrap_stats(o: &mut WorldObject) {
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        let cv = CreatureVital::new(o, v);
        o.vitals_mut().insert(v, cv);
    }
    for a in 1..=6u16 {
        let ca = CreatureAttribute::new(o, PropertyAttribute(a));
        o.attributes_mut().insert(PropertyAttribute(a), ca);
    }
    let skills: Vec<Skill> = o
        .biota
        .properties_skill
        .as_ref()
        .map(|d| d.keys().copied().collect())
        .unwrap_or_default();
    for s in skills {
        o.skills_mut().insert(s, CreatureSkill::new(s));
    }
}

/// A creature (or player) whose biota is described by a `creature_stats`-style input.
fn creature_from(input: &Value, guid: u32) -> WorldObject {
    let player = input["player"].as_bool().expect("player");
    let mut o = WorldObject::allocate(if player {
        Class::Player
    } else {
        Class::Creature
    });
    o.guid = ObjectGuid::new(guid);
    o.biota.id = guid;
    o.biota.properties_attribute = Some(DotNetDict::new());
    o.biota.properties_attribute_2nd = Some(DotNetDict::new());
    o.biota.properties_skill = Some(DotNetDict::new());
    o.biota.properties_enchantment_registry = Some(Vec::new());

    for (k, a) in input["attributes"]
        .as_array()
        .expect("attributes")
        .iter()
        .enumerate()
    {
        if a.is_null() {
            continue;
        }
        let rec = PropertiesAttribute {
            init_level: u(&a[0]),
            level_from_cp: u(&a[1]),
            cp_spent: u(&a[2]),
        };
        o.biota
            .properties_attribute
            .as_mut()
            .unwrap()
            .insert(PropertyAttribute(u16::try_from(k + 1).unwrap()), rec);
    }
    for (k, v) in input["vitals"]
        .as_array()
        .expect("vitals")
        .iter()
        .enumerate()
    {
        if v.is_null() {
            continue;
        }
        let rec = PropertiesAttribute2nd {
            init_level: u(&v[0]),
            level_from_cp: u(&v[1]),
            cp_spent: u(&v[2]),
            current_level: u(&v[3]),
        };
        o.biota
            .properties_attribute_2nd
            .as_mut()
            .unwrap()
            .insert(PropertyAttribute2nd(u16::try_from(2 * k + 1).unwrap()), rec);
    }
    for s in input["skills"].as_array().expect("skills") {
        let rec = PropertiesSkill {
            sac: SkillAdvancementClass(u(&s[1])),
            init_level: u(&s[2]),
            level_from_pp: u16::try_from(u(&s[3])).unwrap(),
            pp: u(&s[4]),
            ..PropertiesSkill::default()
        };
        o.biota
            .properties_skill
            .as_mut()
            .unwrap()
            .insert(Skill(i(&s[0])), rec);
    }
    for p in input["ints"].as_array().expect("ints") {
        o.set_property(PropertyInt(u16::try_from(i(&p[0])).unwrap()), i(&p[1]));
    }
    for p in input["floats"].as_array().expect("floats") {
        o.set_property(
            PropertyFloat(u16::try_from(u(&p[0])).unwrap()),
            f64_of(&p[1]).expect("double"),
        );
    }
    wrap_stats(&mut o);
    o
}

/// The creature `g` in the world.
fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).expect("in the world")
}

/// A creature from vector inputs, put into the world (the enchanted getters read it there).
fn creature_in(w: &mut World, input: &Value, guid: u32) -> ObjectGuid {
    let o = creature_from(input, guid);
    let g = o.guid;
    w.objects.insert(o).expect("fresh guid");
    g
}

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("stats", name);
    let mut failures = Vec::new();
    for case in &file.cases {
        if let Err(got) = each(case) {
            failures.push(format!(
                "in {} expected {} got {got}",
                case.input, case.output
            ));
        }
    }
    assert!(!file.cases.is_empty(), "stats/{name}: no cases");
    assert!(
        failures.is_empty(),
        "stats/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn check<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T, errs: &mut Vec<String>) {
    if got != want {
        errs.push(format!("{what}: got {got:?} want {want:?}"));
    }
}

fn done(errs: Vec<String>) -> Result<(), String> {
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("; "))
    }
}

// ------------------------------------------------------------------ vector replays

#[test]
fn the_fake_dats_add_the_same_retired_skills_as_ace() {
    let d = dats();
    let out = table_case().output;
    for id in u32s(&out["retired_added"]) {
        let ours = d
            .portal_dat()
            .skill_table()
            .skills
            .get(&id)
            .expect("retired skill");
        let ace = &out["skills"][id.to_string()];
        assert_eq!(ours.formula, formula(&ace["formula"]), "skill {id}");
        assert_eq!(ours.min_level, u(&ace["min_level"]), "skill {id}");
    }
    assert_eq!(
        d.portal_dat().skill_table().skills.len(),
        out["skills"].as_object().unwrap().len()
    );
}

#[test]
fn calc_rank_matches_ace() {
    let w = world();
    replay("calc_rank", |c| {
        let xp = u(&c.input["xp"]);
        let got = match c.input["kind"].as_str().unwrap() {
            "attribute" => Ok(player_attributes::calc_attribute_rank(&w, xp)),
            "vital" => Ok(player_vitals::calc_vital_rank(&w, xp)),
            _ => {
                let sac = SkillAdvancementClass(u(&c.input["sac"]));
                catch_unwind(AssertUnwindSafe(|| {
                    player_skills::calc_skill_rank(&w, sac, xp)
                }))
                .map_err(|_| ())
            }
        };
        match (got, throws(&c.output)) {
            (Err(()), Some("System.NullReferenceException")) => Ok(()),
            (Ok(r), None) if i64::from(r) == c.output.as_i64().unwrap() => Ok(()),
            (g, _) => Err(format!("{g:?}")),
        }
    });
}

#[test]
fn xp_between_skill_levels_and_the_table_match_ace() {
    let w = world();
    replay("xp_between_skill_levels", |c| {
        let sac = SkillAdvancementClass(u(&c.input["sac"]));
        let (from, to) = (i(&c.input["from"]), i(&c.input["to"]));
        let between = catch_unwind(AssertUnwindSafe(|| {
            player_skills::get_xp_between_skill_levels(&w, sac, from, to)
        }));
        let len = player_skills::get_skill_xp_table(&w, sac).map(Vec::len);
        let want_len = c.output["table_len"]
            .as_u64()
            .map(|l| usize::try_from(l).unwrap());
        let want = &c.output["between"];
        let ok_between = match (&between, throws(want)) {
            (Err(_), Some("System.ArgumentOutOfRangeException")) => true,
            (Ok(None), None) => want.is_null(),
            (Ok(Some(v)), None) => want.as_u64() == Some(*v),
            _ => false,
        };
        (ok_between && len == want_len)
            .then_some(())
            .ok_or_else(|| format!("{:?} {len:?}", between.ok()))
    });
}

/// **V244 / V245:** Enlightenment, LumAugAllSkills and LumAugSkilledSpec are
/// added only when positive, as the retail client does. ACE added a negative Enlightenment as is
/// and put a negative luminance count through uint, so the sums wrapped. For a player case holding
/// a negative count, ACE's recorded skills and max health stay as the record of ACE's output; the
/// expected values are the gated ones, those of the same creature without the negative counts (ACE
/// adds nothing for a count of 0, and every other case pins that path to ACE). The retail values
/// themselves are checked against the client's inquiries by `rule3_player_stats_agree_with_dereth_client_model`.
fn gated_input(input: &Value) -> Option<Value> {
    if !input["player"].as_bool().expect("player") {
        return None;
    }
    let negative = |p: &Value| matches!(i(&p[0]), 344 | 365 | 390) && i(&p[1]) < 0;
    let ints = input["ints"].as_array().expect("ints");
    if !ints.iter().any(negative) {
        return None;
    }
    let mut gated = input.clone();
    gated["ints"] = Value::Array(ints.iter().filter(|p| !negative(p)).cloned().collect());
    Some(gated)
}

/// Where the gated twin of a case lives (`PLAYER + GATED + n`).
const GATED: u32 = 0x1_0000;

/// **V250 / V251:** a case whose registry retail's duel resolves differently
/// from ACE's order (a category holding both kinds, or a full tie: every vector entry starts at 0),
/// reduced to retail's survivors. Each of its categories then holds one entry, which ACE's order
/// and retail's agree on, so the twin's stats are retail's for the original registry; every other
/// case pins the path to ACE's record. Grouped as the getters query: by `StatMod` type (less the
/// kind bits), key and category.
fn dueled_input(input: &Value) -> Option<Value> {
    use crate::enchant_oracle::{survivors, Keys};
    use empyrean_entity::enums::{EnchantmentTypeFlags as F, SpellCategory};
    use empyrean_entity::models::properties_enchantment_registry::PropertiesEnchantmentRegistry as E;

    let rows = input["enchantments"].as_array().expect("enchantments");
    let entries: Vec<E> = rows
        .iter()
        .map(|e| E {
            spell_id: i(&e[0]),
            spell_category: SpellCategory(u(&e[1])),
            power_level: u(&e[2]),
            stat_mod_type: F(i32::try_from(u(&e[3])).unwrap()),
            stat_mod_key: u(&e[4]),
            stat_mod_value: f32_of(&e[5]).unwrap(),
            ..E::default()
        })
        .collect();
    let kinds = F::Additive.0 | F::Multiplicative.0;
    let group_of = |e: &E| (e.stat_mod_type.0 & !kinds, e.stat_mod_key);
    let mut groups: Vec<(i32, u32)> = entries.iter().map(group_of).collect();
    groups.sort_unstable();
    groups.dedup();
    let mut keep = vec![false; entries.len()];
    let mut differs = false;
    let index = |e: &E| entries.iter().position(|x| std::ptr::eq(x, e)).unwrap();
    for g in groups {
        let group: Vec<&E> = entries.iter().filter(|e| group_of(e) == g).collect();
        let mut retail: Vec<usize> = survivors(&group, true, &Keys::NONE)
            .into_iter()
            .map(index)
            .collect();
        let mut ace: Vec<usize> = [F::Additive.0, F::Multiplicative.0]
            .into_iter()
            .flat_map(|k| {
                survivors(
                    &group
                        .iter()
                        .copied()
                        .filter(|e| e.stat_mod_type.0 & k != 0)
                        .collect::<Vec<_>>(),
                    false,
                    &Keys::NONE,
                )
            })
            .map(index)
            .collect();
        retail.sort_unstable();
        ace.sort_unstable();
        ace.dedup();
        differs |= retail != ace;
        for r in retail {
            keep[r] = true;
        }
    }
    differs.then(|| {
        let mut twin = input.clone();
        twin["enchantments"] = Value::Array(
            rows.iter()
                .zip(&keep)
                .filter(|(_, k)| **k)
                .map(|(r, _)| r.clone())
                .collect(),
        );
        twin
    })
}

/// Where the dueled twin of a case lives (`PLAYER + DUELED + n`).
const DUELED: u32 = 0x2_0000;

/// A vital's getters as the `creature_stats` harness lists them, with `Percent` and `RegenRate`.
#[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
fn vital_row(
    w: &mut World,
    g: ObjectGuid,
    id: u16,
) -> (
    (u32, u32, u32, u32, u32, bool, u32, i32, u32, u32, u32),
    f32,
    f64,
) {
    let cv = obj(w, g)
        .vitals()
        .get(&PropertyAttribute2nd(id))
        .copied()
        .unwrap();
    let row = (
        cv.base(w, obj(w, g)),
        cv.max_value(&mut StatCtx::in_world(w, g)),
        cv.get_max_value(&mut StatCtx::in_world(w, g), false),
        cv.missing(&mut StatCtx::in_world(w, g)),
        cv.experience_left(w, obj(w, g)),
        cv.is_max_rank(w, obj(w, g)),
        cv.to_enum().0,
        cv.modifier_type(&mut StatCtx::in_world(w, g)).0,
        cv.current(obj(w, g)),
        cv.starting_value(obj(w, g)),
        cv.ranks(obj(w, g)),
    );
    (row, cv.percent(&mut StatCtx::in_world(w, g)), cv.regen_rate)
}

/// A skill's getters as the `creature_stats` harness lists them (`None`: no such skill).
#[allow(clippy::type_complexity)]
fn skill_row(
    w: &mut World,
    g: ObjectGuid,
    skill: Skill,
    player: bool,
) -> Option<(bool, u32, u32, u32, bool, u32, Option<u32>, Option<u32>)> {
    let cs = w
        .objects
        .get_mut(g)
        .unwrap()
        .get_creature_skill(skill, false)?;
    Some((
        cs.is_usable(w, obj(w, g)),
        cs.base(w, obj(w, g)),
        cs.current(w, g),
        cs.experience_left(w, obj(w, g)),
        cs.is_max_rank(w, obj(w, g)),
        cs.advancement_class(obj(w, g)).0,
        player.then(|| cs.get_aug_bonus_base(obj(w, g))),
        player.then(|| cs.get_aug_bonus_current(obj(w, g))),
    ))
}

#[test]
fn creature_and_player_stats_match_ace() {
    let mut w = world();
    let mut n = 0u32;
    replay("creature_stats", |c| {
        n += 1;
        let g = creature_in(&mut w, &c.input, PLAYER + n);
        let gated = gated_input(&c.input).map(|gi| creature_in(&mut w, &gi, PLAYER + GATED + n));
        let player = c.input["player"].as_bool().unwrap();
        let out = &c.output;
        let mut errs = Vec::new();

        for (k, want) in out["attributes"].as_array().unwrap().iter().enumerate() {
            // the harness lists Strength, Endurance, Coordination, Quickness, Focus, Self
            let id = [1u16, 2, 4, 3, 5, 6][k];
            let ca = obj(&w, g)
                .attributes()
                .get(&PropertyAttribute(id))
                .copied()
                .unwrap();
            let got = (
                ca.base(obj(&w, g)),
                ca.current(&mut StatCtx::in_world(&mut w, g)),
                ca.get_current(&mut StatCtx::in_world(&mut w, g), false),
                ca.modifier_type(&mut StatCtx::in_world(&mut w, g)).0,
                ca.experience_left(&w, obj(&w, g)),
                ca.is_max_rank(&w, obj(&w, g)),
                ca.starting_value(obj(&w, g)),
                ca.ranks(obj(&w, g)),
                ca.experience_spent(obj(&w, g)),
            );
            let exp = (
                u(&want[0]),
                u(&want[1]),
                u(&want[2]),
                i(&want[3]),
                u(&want[4]),
                want[5].as_bool().unwrap(),
                u(&want[6]),
                u(&want[7]),
                u(&want[8]),
            );
            check(&format!("attribute {id}"), got, exp, &mut errs);
        }

        for (k, want) in out["vitals"].as_array().unwrap().iter().enumerate() {
            let id = u16::try_from(2 * k + 1).unwrap();
            let (got, percent, regen) = vital_row(&mut w, g, id);
            let (exp, want_percent, want_regen) = match gated {
                // max health: the gated Enlightenment (V244)
                Some(r) if id == 1 => vital_row(&mut w, r, id),
                _ => (
                    (
                        u(&want[0]),
                        u(&want[1]),
                        u(&want[2]),
                        u(&want[3]),
                        u(&want[5]),
                        want[6].as_bool().unwrap(),
                        u(&want[8]),
                        i(&want[9]),
                        u(&want[10]),
                        u(&want[11]),
                        u(&want[12]),
                    ),
                    f32_of(&want[4]).unwrap(),
                    f64_of(&want[7]).unwrap(),
                ),
            };
            check(&format!("vital {id}"), got, exp, &mut errs);
            if !same_f32(percent, want_percent) {
                errs.push(format!("vital {id} percent {percent}"));
            }
            if !same_f64(regen, want_regen) {
                errs.push(format!("vital {id} regen {regen}"));
            }
        }

        for (k, want) in out["skills"].as_object().unwrap() {
            let skill = Skill(k.parse().unwrap());
            let got = skill_row(&mut w, g, skill, player);
            let exp = match gated {
                // the gated Enlightenment and luminance augmentations (V244, V245)
                Some(r) => skill_row(&mut w, r, skill, player),
                None => (!want.is_null()).then(|| {
                    (
                        want[0].as_bool().unwrap(),
                        u(&want[1]),
                        u(&want[2]),
                        u(&want[3]),
                        want[4].as_bool().unwrap(),
                        u(&want[5]),
                        want[6].as_u64().map(|v| u32::try_from(v).unwrap()),
                        want[7].as_u64().map(|v| u32::try_from(v).unwrap()),
                    )
                }),
            };
            check(&format!("skill {k}"), got, exp, &mut errs);
        }

        for (k, want) in out["formula_skill"].as_object().unwrap() {
            let skill = Skill(k.parse().unwrap());
            let got = (
                attribute_formula::get_formula_skill(
                    &mut StatCtx::in_world(&mut w, g),
                    skill,
                    true,
                ),
                attribute_formula::get_formula_skill(
                    &mut StatCtx::in_world(&mut w, g),
                    skill,
                    false,
                ),
            );
            check(
                &format!("formula skill {k}"),
                got,
                (u(&want[0]), u(&want[1])),
                &mut errs,
            );
        }
        for (v, want) in out["formula_vital"].as_array().unwrap().iter().enumerate() {
            let vital = PropertyAttribute2nd(u16::try_from(v).unwrap());
            let got = (
                attribute_formula::get_formula_vital(
                    &mut StatCtx::in_world(&mut w, g),
                    vital,
                    true,
                ),
                attribute_formula::get_formula_vital(
                    &mut StatCtx::in_world(&mut w, g),
                    vital,
                    false,
                ),
            );
            check(
                &format!("formula vital {v}"),
                got,
                (u(&want[0]), u(&want[1])),
                &mut errs,
            );
        }
        for (v, want) in out["get_creature_vital"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            if want.get("skipped").is_some() {
                continue;
            }
            let got = obj(&w, g)
                .get_creature_vital(PropertyAttribute2nd(u16::try_from(v).unwrap()))
                .map(|cv| u32::from(cv.vital.0));
            check(
                &format!("GetCreatureVital({v})"),
                got,
                want.as_u64().map(|x| u32::try_from(x).unwrap()),
                &mut errs,
            );
        }
        for (k, want) in out["attribute_mod"].as_array().unwrap().iter().enumerate() {
            let o = obj(&w, g);
            let cv = o
                .vitals()
                .get(&PropertyAttribute2nd(u16::try_from(2 * k + 1).unwrap()))
                .copied()
                .unwrap();
            let got = o.get_attribute_mod(cv);
            if !same_f32(got, f32_of(want).unwrap()) {
                errs.push(format!("attribute_mod {k}: {got}"));
            }
        }
        done(errs)
    });
}

#[test]
fn regeneration_ticks_match_ace() {
    replay("regeneration", |c| {
        let mut w = world();
        let mut o = creature_from(&c.input, PLAYER);
        // Health regeneration: a player's health step scales with the seconds since its last
        // heartbeat; ACE's recorded ticks are 5 s ones.
        if let Some(p) = o.player.as_mut() {
            p.player_tick.heartbeat_elapsed = Some(5.0);
        }
        let g = o.guid;
        w.objects.insert(o).expect("fresh");
        let mut errs = Vec::new();
        let o = w.objects.get(g).unwrap();
        let mut before = (o.stamina().current(o), o.mana().current(o));
        for (t, want) in c.output.as_array().unwrap().iter().enumerate() {
            let ret = dispatch::vital_heart_beat::vital_heart_beat(&mut w, g);
            let o = w.objects.get(g).unwrap();
            let (h, s, m) = (o.health(), o.stamina(), o.mana());
            let got = (ret, h.current(o), s.current(o), m.current(o));
            // V259: a tick that changed stamina or mana also answers
            // true; ACE's recorded answer covered only health (and clamps).
            let after = (u(&want[2]), u(&want[3]));
            let changed = want[0].as_bool().unwrap() || after != before;
            before = after;
            let exp = (changed, u(&want[1]), u(&want[2]), u(&want[3]));
            check(&format!("tick {t}"), got, exp, &mut errs);
            for (k, cv) in [h, s, m].iter().enumerate() {
                if !same_f64(cv.partial_regen, f64_of(&want[4 + k]).unwrap()) {
                    errs.push(format!("tick {t} partial {k}: {}", cv.partial_regen));
                }
            }
            if !errs.is_empty() {
                break;
            }
        }
        done(errs)
    });
}

#[test]
fn a_players_health_regen_attribute_mod_matches_ace() {
    replay("attribute_mod", |c| {
        let input = serde_json::json!({
            "player": true,
            "attributes": [[u(&c.input["strength"]), 0, 0], [u(&c.input["endurance"]), 0, 0], null, null, null, null],
            "vitals": [null, null, null], "skills": [], "ints": [], "floats": [],
        });
        let o = creature_from(&input, PLAYER);
        let got: Vec<f32> = [o.health(), o.stamina(), o.mana()]
            .iter()
            .map(|v| o.get_attribute_mod(*v))
            .collect();
        let want: Vec<f32> = c
            .output
            .as_array()
            .unwrap()
            .iter()
            .map(|v| f32_of(v).unwrap())
            .collect();
        (got.iter().zip(&want).all(|(a, b)| same_f32(*a, *b)))
            .then_some(())
            .ok_or_else(|| format!("{got:?}"))
    });
}

/// Enchanted stats match ace.
#[test]
fn enchanted_stats_match_ace() {
    use empyrean_entity::enums::{EnchantmentTypeFlags, SpellCategory};
    use empyrean_entity::models::properties_enchantment_registry::PropertiesEnchantmentRegistry;

    let enchanted = |input: &Value, guid: u32| {
        let mut o = creature_from(input, guid);
        let reg: Vec<PropertiesEnchantmentRegistry> = input["enchantments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| PropertiesEnchantmentRegistry {
                spell_id: i(&e[0]),
                layer_id: 1,
                spell_category: SpellCategory(u(&e[1])),
                power_level: u(&e[2]),
                stat_mod_type: EnchantmentTypeFlags(i32::try_from(u(&e[3])).unwrap()),
                stat_mod_key: u(&e[4]),
                stat_mod_value: f32_of(&e[5]).unwrap(),
                ..PropertiesEnchantmentRegistry::default()
            })
            .collect();
        o.biota.properties_enchantment_registry = Some(reg);
        o
    };
    let mut w = world();
    let mut n = 0u32;
    replay("enchanted_stats", |c| {
        n += 1;
        let o = enchanted(&c.input, PLAYER + n);
        // V250 / V251: where retail's duel keeps other enchantments than ACE's order, every stat is
        // the dueled twin's (`dueled_input`), gated as well; its categories hold one entry each.
        let dueled = dueled_input(&c.input)
            .map(|d| enchanted(&gated_input(&d).unwrap_or(d), PLAYER + DUELED + n));
        // V244 / V245: a negative count's skills and max health are the gated twin's (`gated_input`)
        let twin = gated_input(&c.input).map(|gi| enchanted(&gi, PLAYER + GATED + n));
        let mut errs = Vec::new();

        // detached: the construction path (no caches)
        for (k, want) in c.output["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let key = PropertyAttribute(u16::try_from(k + 1).unwrap());
            let row = |o: &WorldObject| {
                let ca = o.attributes().get(&key).copied().unwrap();
                let c = &mut StatCtx::detached(&w, o);
                (
                    ca.current(c),
                    ca.get_current(c, false),
                    ca.modifier_type(c).0,
                )
            };
            let got = row(&o);
            let exp = dueled
                .as_ref()
                .map_or_else(|| (u(&want[0]), u(&want[1]), i(&want[2])), row);
            check(
                &format!("detached attribute {}", k + 1),
                got,
                exp,
                &mut errs,
            );
        }
        for (k, want) in c.output["vitals"].as_array().unwrap().iter().enumerate() {
            let key = PropertyAttribute2nd(u16::try_from(2 * k + 1).unwrap());
            let row = |o: &WorldObject| {
                let cv = o.vitals().get(&key).copied().unwrap();
                let c = &mut StatCtx::detached(&w, o);
                (
                    cv.max_value(c),
                    cv.get_max_value(c, false),
                    cv.modifier_type(c).0,
                )
            };
            let got = row(&o);
            let exp = match (&dueled, &twin) {
                (Some(d), _) => row(d),
                (None, Some(t)) if k == 0 => row(t),
                _ => (u(&want[0]), u(&want[1]), i(&want[2])),
            };
            check(
                &format!("detached vital {}", 2 * k + 1),
                got,
                exp,
                &mut errs,
            );
        }

        // in the world: the caching manager, read twice (filled, then from the caches)
        let g = o.guid;
        w.objects.insert(o).expect("fresh guid");
        let mut place = |t: WorldObject| {
            let tg = t.guid;
            w.objects.insert(t).expect("fresh guid");
            tg
        };
        let dueled = dueled.map(&mut place);
        let twin = twin.map(&mut place);
        for pass in 0..2 {
            for (k, want) in c.output["attributes"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                let key = PropertyAttribute(u16::try_from(k + 1).unwrap());
                let mut row = |g: ObjectGuid| {
                    let ca = obj(&w, g).attributes().get(&key).copied().unwrap();
                    let c = &mut StatCtx::in_world(&mut w, g);
                    (
                        ca.current(c),
                        ca.get_current(c, false),
                        ca.modifier_type(c).0,
                    )
                };
                let got = row(g);
                let exp = dueled.map_or_else(|| (u(&want[0]), u(&want[1]), i(&want[2])), &mut row);
                check(
                    &format!("pass {pass} attribute {}", k + 1),
                    got,
                    exp,
                    &mut errs,
                );
            }
            for (k, want) in c.output["vitals"].as_array().unwrap().iter().enumerate() {
                let key = PropertyAttribute2nd(u16::try_from(2 * k + 1).unwrap());
                let mut row = |g: ObjectGuid| {
                    let cv = obj(&w, g).vitals().get(&key).copied().unwrap();
                    let c = &mut StatCtx::in_world(&mut w, g);
                    (
                        cv.max_value(c),
                        cv.get_max_value(c, false),
                        cv.modifier_type(c).0,
                    )
                };
                let got = row(g);
                let exp = match (dueled, twin) {
                    (Some(d), _) => row(d),
                    (None, Some(t)) if k == 0 => row(t),
                    _ => (u(&want[0]), u(&want[1]), i(&want[2])),
                };
                check(
                    &format!("pass {pass} vital {}", 2 * k + 1),
                    got,
                    exp,
                    &mut errs,
                );
            }
            for (k, want) in c.output["skills"].as_object().unwrap() {
                let skill = Skill(k.parse().unwrap());
                let mut row = |g: ObjectGuid| {
                    let cs = w
                        .objects
                        .get_mut(g)
                        .unwrap()
                        .get_creature_skill(skill, false)
                        .unwrap();
                    (cs.base(&w, obj(&w, g)), cs.current(&mut w, g))
                };
                let got = row(g);
                let exp = dueled
                    .or(twin)
                    .map_or_else(|| (u(&want[0]), u(&want[1])), &mut row);
                check(&format!("pass {pass} skill {k}"), got, exp, &mut errs);
            }
        }
        done(errs)
    });
}

// ------------------------------------------------------------------ hand-derived: construction

#[test]
fn set_ephemeral_stat_values_adds_missing_records_and_fills_empty_vitals() {
    let w = world();
    let mut o = WorldObject::allocate(Class::Creature);
    o.biota.properties_attribute = Some(DotNetDict::new());
    o.biota.properties_attribute_2nd = Some(DotNetDict::new());
    o.biota.properties_skill = Some(DotNetDict::new());
    o.biota.properties_enchantment_registry = Some(Vec::new());
    o.biota.properties_attribute.as_mut().unwrap().insert(
        PropertyAttribute::Endurance,
        PropertiesAttribute {
            init_level: 50,
            ..Default::default()
        },
    );
    o.biota.properties_attribute.as_mut().unwrap().insert(
        PropertyAttribute::Self_,
        PropertiesAttribute {
            init_level: 40,
            ..Default::default()
        },
    );
    let mana = PropertiesAttribute2nd {
        init_level: 10,
        current_level: 7,
        ..Default::default()
    };
    o.biota
        .properties_attribute_2nd
        .as_mut()
        .unwrap()
        .insert(PropertyAttribute2nd::MaxMana, mana);
    o.biota
        .properties_skill
        .as_mut()
        .unwrap()
        .insert(Skill::Run, PropertiesSkill::default());
    o.set_property(PropertyFloat::HealthRate, 2.5);

    creature_vitals::set_ephemeral_stat_values(&w, &mut o);

    assert_eq!(o.biota.properties_attribute.as_ref().unwrap().len(), 6);
    assert_eq!(o.biota.properties_attribute_2nd.as_ref().unwrap().len(), 3);
    assert_eq!(o.attributes().len(), 6);
    assert_eq!(
        o.skills().keys().copied().collect::<Vec<_>>(),
        vec![Skill::Run]
    );
    // health: End/2 = 25 (filled from 0), stamina: End = 50 (filled), mana: 10 + Self 40 = 50, but 7 kept
    assert_eq!(
        (
            o.health().current(&o),
            o.stamina().current(&o),
            o.mana().current(&o)
        ),
        (25, 50, 7)
    );
    assert_eq!((o.health().regen_rate, o.stamina().regen_rate), (2.5, 0.0));

    creature_vitals::fill_vitals_to_max(&w, &mut o);
    assert_eq!(o.mana().current(&o), 50);
}

#[test]
fn get_creature_skill_adds_an_untrained_record_only_when_asked() {
    let mut o = WorldObject::allocate(Class::Creature);
    o.biota.properties_skill = Some(DotNetDict::new());
    assert_eq!(o.get_creature_skill(Skill::Jump, false), None);
    assert!(!o.wo.world_object_database.changes_detected);
    let cs = o.get_creature_skill(Skill::Jump, true).unwrap();
    assert_eq!(cs.advancement_class(&o), SkillAdvancementClass::Untrained);
    assert!(o.wo.world_object_database.changes_detected);
    assert_eq!(o.get_creature_skill(Skill::Jump, false), Some(cs));
}

// ------------------------------------------------------------------ hand-derived: players

/// A world with one session S whose player is PLAYER: every attribute at 100, 1,000,000,000
/// available experience and 20 skill credits, Run trained, Healing untrained.
pub(crate) fn player_world() -> World {
    let mut w = world();
    let input = serde_json::json!({
        "player": true,
        "attributes": [[100, 0, 0], [100, 0, 0], [100, 0, 0], [100, 0, 0], [100, 0, 0], [100, 0, 0]],
        "vitals": [[0, 0, 0, 10], [0, 0, 0, 100], [0, 0, 0, 100]],
        "skills": [[24, 2, 0, 0, 0], [21, 1, 0, 0, 0]], "ints": [[23, 20], [24, 20]], "floats": [],
    });
    let mut o = creature_from(&input, PLAYER);
    o.set_property(PropertyInt64::AvailableExperience, 1_000_000_000);
    o.set_property(PropertyString::Name, "Tester".to_owned());
    w.objects.insert(o).expect("fresh");
    let mut s = SessionData::default();
    s.set_player(Some(ObjectGuid::new(PLAYER)));
    w.sessions.insert(S, s);
    w
}

fn p(w: &World) -> &WorldObject {
    w.objects.get(ObjectGuid::new(PLAYER)).unwrap()
}

fn opcodes(sent: &[(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)]) -> Vec<u32> {
    sent.iter()
        .map(|(_, _, b)| u32::from_le_bytes(b[..4].try_into().unwrap()))
        .collect()
}

/// A `GameMessageSystemChat`'s text (a `String16L` after the opcode).
fn chat_text(bytes: &[u8]) -> String {
    let len = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
    String::from_utf8(bytes[6..6 + len].to_vec()).unwrap()
}

const UPDATE_INT64: u32 = 0x02CF;
const UPDATE_INT: u32 = 0x02CD;
const UPDATE_SKILL: u32 = 0x02DD;
const UPDATE_ATTRIBUTE: u32 = 0x02E3;
const UPDATE_VITAL: u32 = 0x02E7;
const UPDATE_VITAL_LEVEL: u32 = 0x02E9;
const SOUND: u32 = 0xF750;
const SYSTEM_CHAT: u32 = 0xF7E0;
const GAME_EVENT: u32 = 0xF7B0;

#[test]
fn raising_an_attribute_spends_xp_and_sends_aces_messages() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    let cost = w.dats.portal_dat().xp_table().attribute_xp[1];
    start_capture();
    assert!(player_attributes::handle_action_raise_attribute(
        &mut w,
        g,
        PropertyAttribute::Endurance,
        cost
    ));
    let sent = take_sent();
    assert_eq!(
        opcodes(&sent),
        [
            UPDATE_INT64,
            UPDATE_ATTRIBUTE,
            SOUND,
            SYSTEM_CHAT,
            UPDATE_VITAL
        ]
    );
    assert_eq!(chat_text(&sent[3].2), "Your base Endurance is now 101!");
    let o = p(&w);
    assert_eq!(
        o.available_experience(),
        Some(1_000_000_000 - i64::from(cost))
    );
    assert_eq!(
        (o.endurance().ranks(o), o.endurance().experience_spent(o)),
        (1, cost)
    );
    // UpdateAttribute: sequence byte, attribute, ranks, starting value, experience spent
    let mut want = vec![2, 0, 0, 0, 1, 0, 0, 0, 100, 0, 0, 0];
    want.extend(cost.to_le_bytes());
    assert_eq!(&sent[1].2[5..], want.as_slice());

    // one XP short of the next rank: spends, no rank change, only the two updates
    let next = w.dats.portal_dat().xp_table().attribute_xp[2] - cost - 1;
    assert!(player_attributes::handle_action_raise_attribute(
        &mut w,
        g,
        PropertyAttribute::Endurance,
        next
    ));
    assert_eq!(opcodes(&take_sent()), [UPDATE_INT64, UPDATE_ATTRIBUTE]);

    // more than the available experience: refused with a log line only
    not_ported::take_local();
    assert!(!player_attributes::handle_action_raise_attribute(
        &mut w,
        g,
        PropertyAttribute::Strength,
        u32::MAX
    ));
    assert_eq!(
        not_ported::take_local().get("ACE: ChatPacket.SendServerMessage"),
        None
    );
    w.objects
        .get_mut(g)
        .unwrap()
        .set_property(PropertyInt64::AvailableExperience, 5_000_000_000);
    assert!(!player_attributes::handle_action_raise_attribute(
        &mut w,
        g,
        PropertyAttribute::Strength,
        u32::MAX
    ));
    let sent = take_sent();
    assert_eq!(opcodes(&sent), [SYSTEM_CHAT]);
    assert!(
        String::from_utf8_lossy(&sent[0].2).contains("Your attempt to raise Strength has failed.")
    );
    assert_eq!(
        not_ported::take_local().get("ACE: ChatPacket.SendServerMessage"),
        None
    );
    assert!(!player_attributes::handle_action_raise_attribute(
        &mut w,
        g,
        PropertyAttribute(9),
        1
    ));
}

#[test]
fn raising_an_attribute_through_the_inbound_dispatch() {
    use dereth_protocol::{self as proto, actions::pack_action};
    use empyrean_world::network::managers::inbound_message_manager::{
        handle_client_message, run_inbound_message_queue,
    };

    let mut w = player_world();
    w.sessions.get_mut(S).unwrap().state = empyrean_net::SessionState::WorldConnected;
    let cost = w.dats.portal_dat().xp_table().attribute_xp[3];
    let data = pack_action(
        0x10,
        &proto::admin::TrainAttribute {
            attribute_id: 3,
            xp_spent: cost,
        },
    )
    .unwrap();
    start_capture();
    handle_client_message(&mut w, empyrean_net::ClientMessage::new(data).unwrap(), S);
    run_inbound_message_queue(&mut w);
    let sent = take_sent();
    assert_eq!(
        opcodes(&sent),
        [UPDATE_INT64, UPDATE_ATTRIBUTE, SOUND, SYSTEM_CHAT]
    );
    assert_eq!(chat_text(&sent[3].2), "Your base Quickness is now 103!");
}

#[test]
fn raising_a_skill_spends_xp_and_sends_aces_messages() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    let t = w.dats.portal_dat().xp_table().trained_xp.clone();
    start_capture();
    assert!(player_skills::handle_action_raise_skill(
        &mut w,
        g,
        Skill::Run,
        t[2]
    ));
    let sent = take_sent();
    assert_eq!(
        opcodes(&sent),
        [UPDATE_INT64, UPDATE_SKILL, SOUND, SYSTEM_CHAT]
    );
    // Run: Quickness 100 / 1 + 2 ranks
    assert_eq!(chat_text(&sent[3].2), "Your base Run skill is now 102!");
    let o = p(&w);
    let cs = o.skills().get(&Skill::Run).copied().unwrap();
    assert_eq!((cs.ranks(o), cs.experience_spent(o)), (2, t[2]));

    // an untrained skill is refused without a message
    assert!(!player_skills::handle_action_raise_skill(
        &mut w,
        g,
        Skill::Healing,
        1
    ));
    // an absent skill too
    assert!(!player_skills::handle_action_raise_skill(
        &mut w,
        g,
        Skill::Jump,
        1
    ));
    assert!(take_sent().is_empty());
}

#[test]
fn raising_a_vital_to_max_rank_plays_the_fireworks() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    let table = w.dats.portal_dat().xp_table().vital_xp.clone();
    let all = *table.last().unwrap();
    w.objects
        .get_mut(g)
        .unwrap()
        .set_property(PropertyInt64::AvailableExperience, 5_000_000_000);
    not_ported::take_local();
    start_capture();
    assert!(player_vitals::handle_action_raise_vital(
        &mut w,
        g,
        PropertyAttribute2nd::MaxMana,
        all
    ));
    let sent = take_sent();
    assert_eq!(
        opcodes(&sent),
        [UPDATE_INT64, UPDATE_VITAL, SOUND, SYSTEM_CHAT]
    );
    let ranks = u32::try_from(table.len() - 1).unwrap();
    assert_eq!(
        chat_text(&sent[3].2),
        format!(
            "Your base Maximum Mana is now {} and has reached its upper limit!",
            100 + ranks
        )
    );
    assert_eq!(
        not_ported::take_local().get("ACE: WorldObject.PlayParticleEffect"),
        None
    );
    // at max rank now: refused
    assert!(!player_vitals::handle_action_raise_vital(
        &mut w,
        g,
        PropertyAttribute2nd::MaxMana,
        1
    ));
    // not a max vital
    assert!(!player_vitals::handle_action_raise_vital(
        &mut w,
        g,
        PropertyAttribute2nd::Mana,
        1
    ));
    assert!(take_sent().is_empty());
}

#[test]
fn training_a_skill_costs_credits_and_reports_them() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    start_capture();
    // Healing costs 6 in the synthetic table
    assert!(!player_skills::handle_action_train_skill(
        &mut w,
        g,
        Skill::Healing,
        5
    ));
    assert!(take_sent().is_empty());
    assert!(player_skills::handle_action_train_skill(
        &mut w,
        g,
        Skill::Healing,
        6
    ));
    let sent = take_sent();
    assert_eq!(opcodes(&sent), [UPDATE_SKILL, UPDATE_INT, SYSTEM_CHAT]);
    assert_eq!(
        chat_text(&sent[2].2),
        "Healing trained. You now have 14 credits available."
    );
    let o = p(&w);
    let cs = o.skills().get(&Skill::Healing).copied().unwrap();
    assert_eq!(cs.advancement_class(o), SkillAdvancementClass::Trained);
    // already trained: TrainSkill fails
    assert!(!player_skills::handle_action_train_skill(
        &mut w,
        g,
        Skill::Healing,
        6
    ));
    let sent = take_sent();
    assert_eq!(
        chat_text(&sent[0].2),
        "Failed to train Healing! You now have 14 credits available."
    );
}

#[test]
fn specializing_then_resetting_refunds_credits_and_experience() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    let t = w.dats.portal_dat().xp_table().trained_xp.clone();
    start_capture();
    assert!(player_skills::handle_action_raise_skill(
        &mut w,
        g,
        Skill::Run,
        t[3]
    ));
    // Run upgrade cost 4 - 0
    assert!(player_skills::specialize_skill(
        &mut w,
        g,
        Skill::Run,
        false
    ));
    let o = p(&w);
    let cs = o.skills().get(&Skill::Run).copied().unwrap();
    // ACE-BUG: resetSkill=false is dropped, so the ranks and experience were reset
    assert_eq!(
        (
            cs.advancement_class(o),
            cs.ranks(o),
            cs.experience_spent(o),
            cs.init_level(o)
        ),
        (SkillAdvancementClass::Specialized, 0, 0, 10)
    );
    assert_eq!(o.available_skill_credits(), Some(16));
    take_sent();
    // Run is always trained: the reset refunds the upgrade and leaves it trained
    assert!(player_skills::reset_skill(&mut w, g, Skill::Run, true));
    let sent = take_sent();
    assert_eq!(
        opcodes(&sent),
        [UPDATE_INT64, UPDATE_SKILL, UPDATE_INT, SYSTEM_CHAT]
    );
    assert_eq!(
        chat_text(&sent[3].2),
        "Your Run skill has been reset. All the experience and skill credits that you spent on this skill have been refunded to you."
    );
    let o = p(&w);
    assert_eq!(o.available_skill_credits(), Some(20));
    assert_eq!(
        o.skills().get(&Skill::Run).unwrap().advancement_class(o),
        SkillAdvancementClass::Trained
    );
}

#[test]
fn a_players_update_vital_sends_the_level_and_signals_exhaustion() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    let stamina = p(&w).stamina();
    not_ported::take_local();
    start_capture();
    // newVal -40 clamps the stamina to 0: exhausted (ACE c5f16b44 tests the resulting stamina;
    // before it, ACE tested newVal and sent no exhaustion here)
    assert_eq!(
        dispatch::update_vital::update_vital(&mut w, g, stamina, -40),
        -100
    );
    let sent = take_sent();
    assert_eq!(opcodes(&sent), [UPDATE_VITAL_LEVEL, GAME_EVENT]);
    assert_eq!(
        u32::from_le_bytes(sent[1].2[12..16].try_into().unwrap()),
        0x02EB
    );
    assert_eq!(
        not_ported::take_local().get("ACE: Player.OnExhausted"),
        None
    );
    assert_eq!(
        dispatch::update_vital::update_vital(&mut w, g, stamina, 0),
        0
    );
    assert!(take_sent().is_empty());
    assert_eq!(
        dispatch::update_vital::update_vital(&mut w, g, stamina, 50),
        50
    );
    take_sent();
    assert_eq!(
        dispatch::update_vital::update_vital(&mut w, g, stamina, 0),
        -50
    );
    let sent = take_sent();
    assert_eq!(opcodes(&sent), [UPDATE_VITAL_LEVEL, GAME_EVENT]);
    assert_eq!(
        u32::from_le_bytes(sent[1].2[12..16].try_into().unwrap()),
        0x02EB
    );
    // clamps at MaxValue (Endurance 100)
    assert_eq!(
        creature_vitals::update_vital_delta(&mut w, g, stamina, 1000),
        100
    );
}

#[test]
fn a_players_regeneration_tick_updates_health_through_the_player_override() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    {
        let o = w.objects.get_mut(g).unwrap();
        o.vital_mut(PropertyAttribute2nd::MaxHealth).regen_rate = 3.0;
    }
    start_capture();
    // health 10 of 50; str 100 + 2 * end 100 = 300 -> attribute mod 1 + 100/600
    assert!(dispatch::vital_heart_beat::vital_heart_beat(&mut w, g));
    let sent = take_sent();
    assert_eq!(opcodes(&sent), [UPDATE_VITAL_LEVEL]);
    let o = p(&w);
    let tick = 3.0f64 * f64::from(1.0f32 + 100.0f32 / 600.0f32);
    assert_eq!(o.health().current(o), 13);
    assert!(same_f64(o.health().partial_regen, tick - 3.0));
}

// ---- Health regeneration: the player's one 4-6 s tick ----

/// Runs the player's heartbeat `n` times on its own schedule from `t0`; answers the beat times.
fn run_player_beats(w: &mut World, n: usize, t0: f64) -> Vec<f64> {
    use empyrean_world::world_objects::player_tick::player_heartbeat;
    let g = ObjectGuid::new(PLAYER);
    // as `InitializeHeartbeats` leaves a player: ACE's 5 s interval, which its first beat credits
    w.objects
        .get_mut(g)
        .unwrap()
        .wo
        .world_object_tick
        .cached_heartbeat_interval = 5.0;
    let mut t = t0;
    let mut times = Vec::new();
    for _ in 0..n {
        player_heartbeat(w, g, t);
        times.push(t);
        t = w
            .objects
            .get(g)
            .unwrap()
            .wo
            .world_object_tick
            .next_heartbeat_time;
    }
    times
}

/// A vital's regeneration so far: points gained plus the carried fraction.
fn regen_so_far(w: &World, vital: PropertyAttribute2nd, start: u32) -> f64 {
    let o = p(w);
    let cv = match vital {
        PropertyAttribute2nd::MaxHealth => o.health(),
        PropertyAttribute2nd::MaxStamina => o.stamina(),
        _ => o.mana(),
    };
    f64::from(cv.current(o)) - f64::from(start) + cv.partial_regen
}

/// A player with every vital regenerating at `rate` per tick (before its modifiers), stamina and
/// mana emptied; answers the starting (health, stamina, mana).
fn regenerating_player(rate: f64) -> (World, [u32; 3]) {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        w.objects.get_mut(g).unwrap().vital_mut(v).regen_rate = rate;
    }
    for cv in [p(&w).stamina(), p(&w).mana()] {
        dispatch::update_vital::update_vital(&mut w, g, cv, 0);
    }
    let o = p(&w);
    let start = [
        o.health().current(o),
        o.stamina().current(o),
        o.mana().current(o),
    ];
    (w, start)
}

/// Health regeneration: the player's heartbeat comes every 4 to 6 s, drawn uniformly and afresh each time
/// (mean 5 s), not ACE's fixed 5 s.
#[test]
fn a_players_heartbeat_comes_every_four_to_six_seconds() {
    let mut w = player_world();
    let times = run_player_beats(&mut w, 2001, 1_000_000.0);
    let gaps: Vec<f64> = times.windows(2).map(|t| t[1] - t[0]).collect();
    assert!(
        gaps.iter().all(|g| (4.0..6.0).contains(g)),
        "every gap in [4, 6)"
    );
    let (min, max) = gaps
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), &g| (a.min(g), b.max(g)));
    assert!(
        min < 4.05 && max > 5.95,
        "the whole range is drawn: {min}..{max}"
    );
    #[allow(clippy::cast_precision_loss)]
    let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
    assert!((mean - 5.0).abs() < 0.05, "mean gap {mean}");
}

/// Health regeneration: a player's health step is the 5 s step times elapsed / 5; stamina and mana take the same
/// step whatever the elapsed time.
#[test]
fn a_players_health_step_scales_with_elapsed_and_stamina_and_mana_do_not() {
    let step = |elapsed: f64| {
        let (mut w, start) = regenerating_player(3.0);
        let g = ObjectGuid::new(PLAYER);
        w.objects
            .get_mut(g)
            .unwrap()
            .player
            .as_mut()
            .unwrap()
            .player_tick
            .heartbeat_elapsed = Some(elapsed);
        dispatch::vital_heart_beat::vital_heart_beat(&mut w, g);
        [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ]
        .iter()
        .zip(start)
        .map(|(&v, s)| regen_so_far(&w, v, s))
        .collect::<Vec<_>>()
    };
    let (at5, short, long) = (step(5.0), step(4.1), step(5.9));
    let health5 = 3.0f64 * f64::from(1.0f32 + 100.0f32 / 600.0f32);
    assert!(same_f64(at5[0], health5), "ACE's 5 s step: {}", at5[0]);
    assert!(
        (short[0] - health5 * 4.1 / 5.0).abs() < 1e-9,
        "4.1 s: {}",
        short[0]
    );
    assert!(
        (long[0] - health5 * 5.9 / 5.0).abs() < 1e-9,
        "5.9 s: {}",
        long[0]
    );
    assert!(at5[1] > 0.0 && at5[2] > 0.0, "stamina and mana regenerate");
    for k in 1..3 {
        assert!(
            same_f64(short[k], at5[k]) && same_f64(long[k], at5[k]),
            "vital {k}: {} {} {}",
            short[k],
            at5[k],
            long[k]
        );
    }
}

/// Health regeneration: over many jittered ticks a player's health regenerates exactly as ACE's fixed 5 s ticks
/// would over the same time, and stamina and mana on average (one fixed step per 5 s mean tick).
#[test]
fn a_players_average_regeneration_matches_aces_fixed_five_seconds() {
    let (mut w, start) = regenerating_player(0.1);
    let times = run_player_beats(&mut w, 300, 1_000_000.0);
    // the first beat credits ACE's 5 s; each later one the time since the last
    let credited = 5.0 + (times[times.len() - 1] - times[0]);
    let per5 = |mods: f64| 0.1 * mods;
    let health5 = per5(f64::from(1.0f32 + 100.0f32 / 600.0f32));
    let health = regen_so_far(&w, PropertyAttribute2nd::MaxHealth, start[0]);
    assert!(
        (health - health5 * credited / 5.0).abs() < 1e-6,
        "health {health} over {credited} s"
    );
    let beats = 300.0;
    for (k, v) in [
        (1, PropertyAttribute2nd::MaxStamina),
        (2, PropertyAttribute2nd::MaxMana),
    ] {
        let got = regen_so_far(&w, v, start[k]);
        let per_tick = got / beats;
        let ace = per_tick * credited / 5.0;
        assert!(
            (got / ace - 1.0).abs() < 0.03,
            "vital {k}: {got} against ACE's {ace}"
        );
    }
}

/// Health regeneration: a player's per-beat timers (an enchantment's remaining time, the gag, the lifestone
/// protection) run on real time under the jittered tick: each beat credits the time since the
/// previous one.
#[test]
fn a_players_per_beat_timers_track_real_elapsed_time() {
    use empyrean_entity::models::properties_enchantment_registry::PropertiesEnchantmentRegistry;
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    {
        let o = w.objects.get_mut(g).unwrap();
        o.biota.properties_enchantment_registry = Some(vec![PropertiesEnchantmentRegistry {
            spell_id: 2,
            layer_id: 1,
            duration: 1800.0,
            ..PropertiesEnchantmentRegistry::default()
        }]);
        o.set_is_gagged(true);
        o.set_gag_duration(1000.0);
        o.set_under_lifestone_protection(true);
        o.set_lifestone_protection_timestamp(Some(0.0));
    }
    let times = run_player_beats(&mut w, 9, 1_000_000.0);
    let credited = 5.0 + (times[times.len() - 1] - times[0]);
    assert!(
        (credited - 5.0 * 9.0).abs() > 1e-3,
        "the beats are not 5 s apart"
    );
    let o = p(&w);
    let start_time = o.biota.properties_enchantment_registry.as_ref().unwrap()[0].start_time;
    assert!(
        (start_time + credited).abs() < 1e-6,
        "the enchantment has run {} s of {credited}",
        -start_time
    );
    assert!(
        (o.gag_duration() - (1000.0 - credited)).abs() < 1e-6,
        "gag {}",
        o.gag_duration()
    );
    let lifestone = o.lifestone_protection_timestamp().unwrap();
    assert!((lifestone - credited).abs() < 1e-6, "lifestone {lifestone}");
}

#[test]
fn set_max_vitals_fills_and_reports_all_three() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    start_capture();
    dispatch::set_max_vitals::set_max_vitals(&mut w, g);
    assert_eq!(
        opcodes(&take_sent()),
        [UPDATE_VITAL_LEVEL, UPDATE_VITAL_LEVEL, UPDATE_VITAL_LEVEL]
    );
    let o = p(&w);
    assert_eq!(
        (
            o.health().current(o),
            o.stamina().current(o),
            o.mana().current(o)
        ),
        (50, 100, 100)
    );
}

#[test]
fn add_skill_credits_and_spend_xp_update_the_client() {
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    start_capture();
    player_skills::add_skill_credits(&mut w, g, 3);
    let sent = take_sent();
    assert_eq!(opcodes(&sent), [UPDATE_INT, GAME_EVENT]);
    assert_eq!(p(&w).available_skill_credits(), Some(23));
    assert!(!player_skills::spend_xp(&mut w, g, 2_000_000_000, true));
    assert!(player_skills::spend_xp(&mut w, g, 1_000, false));
    assert!(take_sent().is_empty());
    player_skills::refund_xp(&mut w, g, 1_000);
    assert_eq!(opcodes(&take_sent()), [UPDATE_INT64]);
    assert_eq!(p(&w).available_experience(), Some(1_000_000_000));
}

#[test]
fn heritage_bonus_and_weapon_type() {
    use empyrean_entity::enums::{HeritageGroup, WeaponType};
    let mut w = player_world();
    let g = ObjectGuid::new(PLAYER);
    w.objects
        .get_mut(g)
        .unwrap()
        .set_property(PropertyInt::HeritageGroup, HeritageGroup::Sho.0);
    let o = p(&w);
    assert!(player_skills::get_heritage_bonus_weapon_type(
        o,
        WeaponType::Unarmed
    ));
    assert!(!player_skills::get_heritage_bonus_weapon_type(
        o,
        WeaponType::Sword
    ));
    let mut sword = WorldObject::allocate(Class::MeleeWeapon);
    sword.guid = ObjectGuid::new(0x8000_0001);
    sword.set_property(PropertyInt::WeaponSkill, Skill::Sword.0);
    assert_eq!(
        player_skills::get_weapon_type(Some(&sword)),
        WeaponType::Sword
    );
    sword.set_property(
        PropertyString::LongDesc,
        "this weapon SEEMS tough to master. indeed".to_owned(),
    );
    w.objects.insert(sword).unwrap();
    // universal masteries (on by default), but not masterable
    assert!(!dispatch::get_heritage_bonus::get_heritage_bonus(
        &w,
        g,
        ObjectGuid::new(0x8000_0001)
    ));
    assert!(!dispatch::get_heritage_bonus::get_heritage_bonus(
        &w,
        g,
        ObjectGuid::new(0x8000_0002)
    ));
}

// ------------------------------------------------------------------ Shared rules cross-checks
//
// The shared rules (`dereth-rules`, the client's retail inquiries) compute the same numbers for the
// client's panels: ranks from experience, the XP cost of the next skill rank, max vitals, and skill
// base and current values. They run here on the server's own `WorldObject`, through its
// `QualityRead` view (`world_objects/quality_read.rs`). The port follows ACE; these tests assert
// the two agree, or record the disagreement.

/// The synthetic dats with each two-attribute formula given `Y = 1`, the shape of the retail
/// tables (the client computes `X*a1 + Y*a2 + W`; ACE adds the two attributes and reads only
/// `X`). The retired skills keep ACE's `new SkillFormula(a1, a2, z)`, which leaves `Y = 0`.
fn retail_shaped_world() -> World {
    let (xp, mut skills, secondary) = tables();
    for b in skills.skills.values_mut() {
        if b.formula.attr2 != 0 {
            b.formula.y = 1;
        }
    }
    let d = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_xp_table(xp)
        .with_skill_table(skills)
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, secondary)
        .build()
        .expect("fake dats");
    let mut w = world();
    w.dats = d;
    w
}

/// Every augmentation count the stats read is one a player can buy (`MaxAugs` is 1 for each).
fn reachable_augmentations(o: &WorldObject) -> bool {
    [
        o.augmentation_skilled_melee(),
        o.augmentation_skilled_missile(),
        o.augmentation_skilled_magic(),
        o.augmentation_jack_of_all_trades(),
    ]
    .iter()
    .all(|&n| (0..=1).contains(&n))
}

/// Every disagreement between the port (= ACE, by the vectors) and the client's retail inquiries
/// (`dereth_rules`, asked of the same `WorldObject`) over the `creature_stats` creatures, as
/// `(category, detail)`.
fn rule3_stat_disagreements() -> Vec<(&'static str, String)> {
    use dereth_rules::attributes::{inq_attribute, inq_attribute_2nd};
    use dereth_rules::skills::inq_skill;

    let mut w = retail_shaped_world();
    let secondary = *w.dats.portal_dat().secondary_attribute_table();
    // The client enchants a secondary attribute only where its quality filter allows: the shipped
    // filter lists the three maxima, which are all this compares.
    let maxima = dereth_assets::tables::QualityFilter {
        id: dereth_primitives::DataId(0x0E01_0001),
        property_lists: Default::default(),
        attribute_lists: [Vec::new(), vec![1, 3, 5], Vec::new()],
    };
    let table = w.dats.portal_dat().skill_table().clone();
    let retired = u32s(&table_case().output["retired_added"]);
    let file = vectors::load_named("stats", "creature_stats");
    let mut out = Vec::new();
    for (n, c) in file.cases.iter().enumerate() {
        let g = creature_in(&mut w, &c.input, PLAYER + u32::try_from(n).unwrap());
        let player = obj(&w, g).is_player();
        if player && !reachable_augmentations(obj(&w, g)) {
            continue;
        }
        let who = if player { "player" } else { "creature" };
        for id in 1..=6u16 {
            let ca = obj(&w, g)
                .attributes()
                .get(&PropertyAttribute(id))
                .copied()
                .unwrap();
            let ours = ca.current(&mut StatCtx::in_world(&mut w, g));
            let theirs = inq_attribute(obj(&w, g), u32::from(id), false).unwrap_or(0);
            if ours != theirs {
                out.push((
                    "attribute current",
                    format!("case {n} {who} attribute {id}: ACE {ours}, client {theirs}"),
                ));
            }
        }
        for id in [1u16, 3, 5] {
            let cv = obj(&w, g)
                .vitals()
                .get(&PropertyAttribute2nd(id))
                .copied()
                .unwrap();
            let ours = cv.max_value(&mut StatCtx::in_world(&mut w, g));
            let theirs =
                inq_attribute_2nd(obj(&w, g), &secondary, u32::from(id), false, Some(&maxima))
                    .unwrap_or(0);
            if ours != theirs {
                let cat = if player && id == 1 && obj(&w, g).enlightenment() != 0 {
                    "max health: Enlightenment * 2"
                } else {
                    "max vital"
                };
                out.push((
                    cat,
                    format!("case {n} {who} vital {id}: ACE {ours}, client {theirs}"),
                ));
            }
        }
        let skills: Vec<Skill> = obj(&w, g).skills().keys().copied().collect();
        for s in skills {
            let key = u32::try_from(s.0).unwrap();
            if !table.skills.contains_key(&key) {
                continue; // the client has no value for a skill its table lacks
            }
            let cs = w
                .objects
                .get_mut(g)
                .unwrap()
                .get_creature_skill(s, false)
                .unwrap();
            let sac = cs.advancement_class(obj(&w, g));
            let (ours_base, ours_cur) = (cs.base(&w, obj(&w, g)), cs.current(&mut w, g));
            let o = obj(&w, g);
            let (theirs_base, theirs_cur) = (
                inq_skill(o, &table, key, true).unwrap(),
                inq_skill(o, &table, key, false).unwrap(),
            );
            if (ours_base, ours_cur) == (theirs_base, theirs_cur) {
                continue;
            }
            let cat = if retired.contains(&key) {
                "retired skill (ACE formula Y = 0, MinLevel 0)"
            } else if player && sac >= SkillAdvancementClass::Trained && o.enlightenment() != 0 {
                "skill: Enlightenment"
            } else if player && (o.lum_aug_all_skills() < 0 || o.lum_aug_skilled_spec() < 0) {
                "skill: negative LumAugAllSkills / LumAugSkilledSpec (ACE-BUG: uint wrap)"
            } else if player && ours_base != theirs_base {
                "skill base: augmentation"
            } else if player {
                "skill current: augmentation"
            } else {
                "skill (other)"
            };
            out.push((
                cat,
                format!("case {n} {who} skill {key} sac {}: ACE {ours_base}/{ours_cur}, client {theirs_base}/{theirs_cur}", sac.0),
            ));
        }
    }
    out
}

/// The ranks bought by experience (`CalcAttributeRank`, `CalcVitalRank`, `CalcSkillRank`) are the
/// shared rules' own search now, pinned to ACE by `calc_rank_matches_ace`; this checks the XP to
/// the next skill rank against the client's raise cost (`dereth_rules::advancement`).
#[test]
fn rule3_xp_ranks_and_skill_costs_agree_with_dereth_client_model() {
    use dereth_rules::advancement::skill_cost_to_raise;

    let w = world();
    let xp = w.dats.portal_dat().xp_table().clone();
    let skills = w.dats.portal_dat().skill_table().clone();
    let mut checked = 0;
    for (sac, t) in [
        (SkillAdvancementClass::Trained, xp.trained_xp.clone()),
        (
            SkillAdvancementClass::Specialized,
            xp.specialized_xp.clone(),
        ),
    ] {
        // the XP to the next rank for a skill whose ranks match its experience
        for (rank, &at) in t.iter().enumerate() {
            for pp in [at, at + 1, t.get(rank + 1).map_or(at, |n| n - 1)] {
                let input = serde_json::json!({"player": false, "attributes": [], "vitals": [], "ints": [], "floats": [],
                    "skills": [[6, sac.0, 0, rank, pp]]});
                let mut o = creature_from(&input, PLAYER);
                let cs = o.get_creature_skill(Skill::MeleeDefense, false).unwrap();
                let ours = player_skills::get_xp_to_next_rank(&w, &o, cs).unwrap_or(0);
                let theirs = skill_cost_to_raise(&o, &skills, &xp, 6);
                assert_eq!(ours, theirs, "{sac:?} rank {rank} pp {pp}");
                checked += 1;
            }
        }
    }
    assert!(checked > 1000);
}

#[test]
fn rule3_creature_stats_agree_with_dereth_client_model() {
    // Everything a non-player creature shows agrees, except the retired skills (below).
    let d: Vec<_> = rule3_stat_disagreements()
        .into_iter()
        .filter(|(cat, detail)| detail.contains(" creature ") && !cat.starts_with("retired"))
        .collect();
    let shown: Vec<String> = d
        .iter()
        .take(10)
        .map(|(c, x)| format!("{c}: {x}"))
        .collect();
    assert!(
        d.is_empty(),
        "{} disagreements:\n  {}",
        d.len(),
        shown.join("\n  ")
    );
}

/// player stats agree with dereth client model.
/// V244, V245.
#[test]
fn rule3_player_stats_agree_with_dereth_client_model() {
    let d: Vec<_> = rule3_stat_disagreements()
        .into_iter()
        .filter(|(cat, detail)| detail.contains(" player ") && !cat.starts_with("retired"))
        .collect();
    let shown: Vec<String> = d
        .iter()
        .take(10)
        .map(|(c, x)| format!("{c}: {x}"))
        .collect();
    assert!(
        d.is_empty(),
        "{} disagreements:\n  {}",
        d.len(),
        shown.join("\n  ")
    );
}

/// The server keeps ACE's formulas for retired skills (minimum level 0, second attribute weight 0),
/// which disagree with the client's formula; unreachable, because retired skills cannot be trained.
/// Run with `--ignored` to list the disagreements.
#[test]
#[ignore = "retired skills keep ACE's formulas; unreachable"]
fn rule3_retired_skills_agree_with_dereth_client_model() {
    let d = rule3_stat_disagreements();
    let mut by: BTreeMap<&str, (usize, String)> = BTreeMap::new();
    for (cat, detail) in &d {
        let e = by.entry(cat).or_insert((0, detail.clone()));
        e.0 += 1;
    }
    let shown: Vec<String> = by
        .iter()
        .map(|(c, (n, ex))| format!("{c}: {n} (e.g. {ex})"))
        .collect();
    assert!(
        d.is_empty(),
        "{} disagreements:\n  {}",
        d.len(),
        shown.join("\n  ")
    );
}

/// The `WorldObject` view the shared inquiries read (`world_objects/quality_read.rs`): the stored
/// attribute and vital records, the odd and even vital ids naming one record, the int table, and
/// the registry rows sorted into their lists, so an additive Strength buff reaches the retail
/// stacking.
#[test]
fn the_world_object_quality_view_reads_the_stored_records_and_the_enchantments() {
    use dereth_rules::attributes::{inq_attribute, inq_attribute_2nd_stored};
    use dereth_rules::enchant::ench_type as t;
    use dereth_rules::quality::QualityRead;
    use empyrean_entity::enums::EnchantmentTypeFlags;
    use empyrean_entity::models::PropertiesEnchantmentRegistry;

    let input = serde_json::json!({"player": false, "attributes": [[50, 0, 0]], "vitals": [[0, 0, 0, 37]],
        "ints": [[5, 750]], "floats": [], "skills": [[6, 2, 0, 3, 100]]});
    let mut o = creature_from(&input, PLAYER);
    assert_eq!(o.inq_int(5), 750);
    assert_eq!(inq_attribute(&o, 1, true), Some(50));
    assert_eq!(
        inq_attribute_2nd_stored(&o, 2),
        Some(37),
        "the even id reads the current value"
    );
    assert_eq!(o.attribute_2nd(1), o.attribute_2nd(2), "one record");
    assert_eq!(o.skill(6).map(|s| (s.sac, s.level_from_pp)), Some((2, 3)));
    assert!(o.enchantments().add_list.is_empty());

    let row = |kind: u32, key: u32, value: f32, spell: i32| PropertiesEnchantmentRegistry {
        spell_id: spell,
        layer_id: 1,
        stat_mod_type: EnchantmentTypeFlags(kind.cast_signed()),
        stat_mod_key: key,
        stat_mod_value: value,
        duration: -1.0,
        ..PropertiesEnchantmentRegistry::default()
    };
    o.biota.properties_enchantment_registry = Some(vec![
        row(t::ADDITIVE | t::SINGLE_STAT | t::ATTRIBUTE, 1, 10.0, 2),
        row(t::MULTIPLICATIVE | t::SINGLE_STAT | t::SKILL, 6, 1.5, 3),
    ]);
    let reg = o.enchantments();
    assert_eq!((reg.add_list.len(), reg.mult_list.len()), (1, 1));
    assert_eq!(inq_attribute(&o, 1, false), Some(60));
    assert_eq!(inq_attribute(&o, 1, true), Some(50));
}

/// A player stuck in portal space for more than five minutes is logged off by its heartbeat; one
/// still inside the five minutes, or not teleporting, is left alone.
#[test]
fn a_player_stuck_in_portal_space_is_logged_off_after_five_minutes() {
    use empyrean_world::world_objects::player_tick::player_heartbeat;
    let run = |teleporting: bool, seconds_ago: f64| {
        let mut w = player_world();
        let g = ObjectGuid::new(PLAYER);
        let now_unix = w.now.unix_time;
        {
            let o = w.objects.get_mut(g).unwrap();
            o.wo.world_object.teleporting = teleporting;
            o.set_last_teleport_start_timestamp(Some(now_unix - seconds_ago));
            // a logged-in player always has its character (the log-off saves it)
            o.player.as_mut().unwrap().player.character =
                Some(empyrean_store::models::shard::Character::default());
        }
        not_ported::take_local();
        player_heartbeat(&mut w, g, now_unix);
        assert!(!not_ported::take_local().contains_key("ACE: Player.Teleporting"));
        w.sessions.get(S).unwrap().log_off_request_time != DotNetDateTime::MIN_VALUE
    };
    assert!(
        run(true, 301.0),
        "five minutes and a second in portal space logs the player off"
    );
    assert!(!run(true, 299.0), "inside five minutes the player waits");
    assert!(
        !run(false, 3600.0),
        "a player not teleporting is not affected"
    );
}

mod item_experience {
    use crate::support::creature_world::*;

    /// `WorldObject.ItemLevel` / `HasItemLevel` / `AddItemXP`, Fixed style (1): level =
    /// floor(total / base), capped at ItemMaxLevel; XP is capped at the maximum level's
    /// (5 x 1000).
    #[test]
    fn item_level_and_item_xp() {
        let mut o = WorldObject::default();
        assert!(!o.has_item_level());
        assert_eq!(o.item_level(), None);
        assert_eq!(o.add_item_xp(100), 0, "no item level: nothing added");

        o.set_property(PropertyInt64::ItemBaseXp, 1000);
        o.set_property(PropertyInt::ItemMaxLevel, 5);
        o.set_property(PropertyInt::ItemXpStyle, 1);
        o.set_property(PropertyInt64::ItemTotalXp, 3500);
        assert!(o.has_item_level());
        assert_eq!(o.item_level(), Some(3));

        assert_eq!(o.add_item_xp(10_000), 1500, "capped at 5000");
        assert_eq!(o.item_total_xp(), Some(5000));
        assert_eq!(o.item_level(), Some(5));
        assert_eq!(o.add_item_xp(1), 0, "already at the cap");

        o.set_property(PropertyInt::ItemXpStyle, 0);
        assert!(!o.has_item_level(), "ItemXpStyle 0 is no item level");
        assert!(!o.has_item_set());
        o.set_property(PropertyInt::EquipmentSetId, 13);
        assert!(o.has_item_set());
    }
}

mod heartbeat_regeneration {
    use crate::support::player_world::*;

    /// `Creature.Heartbeat` runs `VitalHeartBeat`: a wounded creature with a health regeneration rate
    /// regains health on each heartbeat, and a creature at full health stays there.
    #[test]
    fn a_creature_heartbeat_regenerates_its_vitals() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        {
            let o = h.w.objects.get_mut(MONSTER).unwrap();
            o.vitals_mut()
                .get_mut(&PropertyAttribute2nd::MaxHealth)
                .unwrap()
                .regen_rate = 1.0;
        }
        let vital = h.w.objects.get(MONSTER).unwrap().health();
        empyrean_world::dispatch::update_vital::update_vital(&mut h.w, MONSTER, vital, 40);
        assert_eq!(health(&h.w, MONSTER), 40);

        let now = h.w.now.unix_time;
        creature_tick::creature_heartbeat(&mut h.w, MONSTER, now);
        let after = health(&h.w, MONSTER);
        assert!(after > 40, "regenerated: {after}");

        // the heartbeat runs the base (Container -> WorldObject) heartbeat too: the timestamp is set
        assert_eq!(
            h.w.objects
                .get(MONSTER)
                .unwrap()
                .get_property(PropertyFloat::HeartbeatTimestamp),
            Some(now)
        );
    }
}

mod melee_skill {
    use crate::support::navigation_world::*;

    /// `ConvertToMoASkill`: a player's retired melee skill becomes its highest melee skill (Light
    /// Weapons when all are equal), a retired missile skill Missile Weapons.
    #[test]
    fn a_retired_melee_skill_becomes_the_players_highest_melee_skill() {
        let mut w = world();
        assert_eq!(
            wo::convert_to_mo_a_skill(&mut w, g(), Skill::Sword),
            Skill::LightWeapons
        );
        assert_eq!(
            wo::convert_to_mo_a_skill(&mut w, g(), Skill::Bow),
            Skill::MissileWeapons
        );
        assert_eq!(
            wo::convert_to_mo_a_skill(&mut w, g(), Skill::Alchemy),
            Skill::Alchemy
        );
    }
}
