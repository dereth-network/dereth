//! Vectors: fixtures/vectors/magic/
//! Spell, SpellProperties, SpellFormula, spell entities and WorldObject_Magic effect engine
//! follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Once};
use std::time::Duration;

use dereth_assets::tables::{SpellBase, SpellComponent};
use dereth_assets::{DidMapper, DualDidMapper, SpellComponentTable, SpellTable};
use dereth_primitives::DataId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f32_of, i64_of, same_f32, throws, u64_of, Case};
use empyrean_content::models::world::Spell as DbSpell;
use empyrean_content::MemContent;
use empyrean_dat::file_types::spell_table::compute_hash;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::properties::{
    PropertyAttribute2nd, PropertyFloat, PropertyInt, PropertyString,
};
use empyrean_entity::enums::{
    DamageType, EquipMask, ImbuedEffectType, MagicSchool, MotionCommand, ResistanceType, Skill,
    SpellFlags, SpellType, TransferFlags,
};
use empyrean_entity::models::{PropertiesAttribute2nd, PropertiesSkill};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::cast_spell_params::{CastSpellParams, CastingPreCheckStatus};
use empyrean_world::entity::damage_history;
use empyrean_world::entity::spell::Spell;
use empyrean_world::entity::spell_enchantment::SpellEnchantment;
use empyrean_world::entity::spell_formula::{self, Scarab, SpellFormula};
use empyrean_world::entity::spell_projectile_type::ProjectileSpellType;
use empyrean_world::entity::starter_spell::{JsonToken, StringToBoolConverter};
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::creature_magic;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::managers::enchantment_manager_with_caching as emc;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::world_object_magic as magic;
use empyrean_world::World;
use serde_json::Value;

// ---------------------------------------------------------------------------------- harness

thread_local!(static QUIET: Cell<bool> = const { Cell::new(false) });
static HOOK: Once = Once::new();

/// Runs `f`, turning a panic (an ACE exception) into `Err`, without printing it.
fn catch<R>(f: impl FnOnce() -> R) -> Result<R, ()> {
    HOOK.call_once(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                previous(info);
            }
        }));
    });
    QUIET.with(|q| q.set(true));
    let r = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(false));
    r.map_err(|_| ())
}

/// Replays `<area>/<name>.json`; `each` answers `None` for a match or a description of the
/// mismatch. Fails once, listing up to 20 mismatches.
fn replay(area: &str, name: &str, mut each: impl FnMut(&Case) -> Option<String>) {
    let file = vectors::load_named(area, name);
    assert!(!file.cases.is_empty(), "{area}/{name}: no cases");
    let bad: Vec<String> = file.cases.iter().filter_map(&mut each).collect();
    assert!(
        bad.is_empty(),
        "{area}/{name}: {} of {} cases differ from ACE:\n  {}",
        bad.len(),
        file.cases.len(),
        bad.iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn u32_of(v: &Value) -> u32 {
    u32::try_from(u64_of(v).unwrap_or_else(|| panic!("not a u32: {v}"))).expect("u32")
}

fn i32_of(v: &Value) -> i32 {
    i32::try_from(i64_of(v).unwrap_or_else(|| panic!("not an i32: {v}"))).expect("i32")
}

fn u32_list(v: &Value) -> Vec<u32> {
    v.as_array()
        .unwrap_or_else(|| panic!("not a list: {v}"))
        .iter()
        .map(u32_of)
        .collect()
}

/// A recorded ACE result against ours: an ACE exception matches a Rust panic.
fn same<T: PartialEq + std::fmt::Debug>(
    want: &Value,
    got: Result<T, ()>,
    parse: impl Fn(&Value) -> T,
) -> Result<(), String> {
    match (throws(want), got) {
        (Some(_), Err(())) => Ok(()),
        (Some(t), Ok(g)) => Err(format!("ACE throws {t}, we answer {g:?}")),
        (None, Err(())) => Err(format!("ACE answers {want}, we panic")),
        (None, Ok(g)) => {
            let w = parse(want);
            if w == g {
                Ok(())
            } else {
                Err(format!("ACE {w:?}, we {g:?}"))
            }
        }
    }
}

// ------------------------------------------------------------------------------ vectors

/// `WorldObject.MagicDefenseCheck`: one `ThreadSafeRandom.Next(0f, 1f)` per call, from the same
/// seeded generator as ACE's; the resist chance is `(float)(1.0f - chance)`.
#[test]
fn magic_defense_check_matches_ace() {
    replay("magic", "magic_defense_check", |c| {
        ThreadSafeRandom::seed(u64_of(&c.input["seed"]).expect("seed"));
        let pairs = c.input["pairs"].as_array().expect("pairs");
        let want = c.output.as_array().expect("outs");
        for (p, w) in pairs.iter().zip(want) {
            let (resisted, chance) = magic::magic_defense_check(u32_of(&p[0]), u32_of(&p[1]));
            let (wr, wc) = (w[0].as_bool().expect("bool"), f32_of(&w[1]).expect("float"));
            if resisted != wr || !same_f32(chance, wc) {
                return Some(format!(
                    "seed {} pair {p}: ACE ({wr}, {wc}), we ({resisted}, {chance})",
                    c.input["seed"]
                ));
            }
        }
        None
    });
}

/// `Creature.GetManaCost`: two or three draws per call, in ACE's order.
#[test]
fn get_mana_cost_matches_ace() {
    replay("magic", "get_mana_cost", |c| {
        ThreadSafeRandom::seed(u64_of(&c.input["seed"]).expect("seed"));
        let triples = c.input["triples"].as_array().expect("triples");
        let want = u32_list(&c.output);
        for (t, &w) in triples.iter().zip(&want) {
            let got = creature_magic::get_mana_cost(u32_of(&t[0]), u32_of(&t[1]), u32_of(&t[2]));
            if got != w {
                return Some(format!(
                    "seed {} triple {t}: ACE {w}, we {got}",
                    c.input["seed"]
                ));
            }
        }
        None
    });
}

/// `GetBoostResistanceType` / `GetDrainResistanceType`, both overloads.
#[test]
fn resistance_types_match_ace() {
    replay("magic", "resistance_types", |c| {
        let v = i32_of(&c.input["value"]);
        let got = if c.input["kind"] == "damage_type" {
            [
                magic::get_boost_resistance_type(DamageType(v)),
                magic::get_drain_resistance_type(DamageType(v)),
            ]
        } else {
            let a = PropertyAttribute2nd(u16::try_from(v).expect("u16"));
            [
                magic::get_boost_resistance_type_vital(a),
                magic::get_drain_resistance_type_vital(a),
            ]
        };
        let want = [
            ResistanceType(i32_of(&c.output[0])),
            ResistanceType(i32_of(&c.output[1])),
        ];
        (got != want).then(|| format!("{}: ACE {want:?}, we {got:?}", c.input))
    });
}

/// A synthetic spell exactly as the harness builds it: `_spellBase` with the listed members,
/// `_spell` with the listed columns, `Formula = new SpellFormula(spell, formula)`.
fn synthetic_spell(i: &Value) -> Spell {
    let id = u32_of(&i["id"]);
    let formula = u32_list(&i["formula"]);
    let base = SpellBase {
        name: format!("Spell {id}"),
        category: u32_of(&i["category"]),
        bitfield: u32_of(&i["bitfield"]),
        meta_spell_type: u32_of(&i["meta"]),
        school: u32_of(&i["school"]),
        power: i32::try_from(u32_of(&i["power"])).expect("a small power"),
        meta_spell_id: id,
        ..spell_base_default()
    };
    let db = DbSpell {
        id,
        stat_mod_type: Some(u32_of(&i["stat_mod_type"])),
        stat_mod_key: Some(u32_of(&i["stat_mod_key"])),
        base_intensity: Some(i32_of(&i["base_intensity"])),
        variance: Some(i32_of(&i["variance"])),
        boost: Some(i32_of(&i["boost"])),
        boost_variance: Some(i32_of(&i["boost_variance"])),
        num_projectiles: Some(i32_of(&i["num_projectiles"])),
        spread_angle: Some(f32_of(&i["spread_angle"]).expect("float")),
        ..DbSpell::default()
    };
    let mut spell = Spell {
        spell_base: Some(base),
        spell: Some(Arc::new(db)),
        formula: None,
    };
    spell.formula = Some(SpellFormula::new(&spell, formula));
    spell
}

/// `SpellBase`'s all-zero values (ACE's `new SpellBase()`).
fn spell_base_default() -> SpellBase {
    SpellBase {
        name: String::new(),
        description: String::new(),
        school: 0,
        icon: 0,
        category: 0,
        bitfield: 0,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 0,
        meta_spell_id: 0,
        duration: None,
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    }
}

/// The derived members of `Spell`, `SpellFormula` and `CastSpellParams` over one synthetic spell
/// per `SpellCategory`: the level and category predicates, the damage and boost ranges, the spread
/// angle, the foci formula and the scarab scale.
#[test]
fn spell_derived_members_match_ace() {
    replay("magic", "spell_derived", |c| {
        let s = synthetic_spell(&c.input);
        let f = s.formula.clone().expect("formula");
        let o = &c.output;
        let mut errs = Vec::new();
        let mut eq = |name: &str, got: Value| {
            if o[name] != got {
                errs.push(format!("{name}: ACE {}, we {got}", o[name]));
            }
        };
        eq("level", s.level().into());
        eq("level_match", s.level_match().into());
        eq("is_resistable", s.is_resistable().into());
        eq("is_projectile", s.is_projectile().into());
        eq("is_self_targeted", s.is_self_targeted().into());
        eq("is_tracking", s.is_tracking().into());
        eq("is_fellowship", s.is_fellowship_spell().into());
        eq("magic_skill", s.get_magic_skill().0.into());
        eq("is_impen_bane", s.is_impen_bane_type().into());
        eq("is_item_redirectable", s.is_item_redirectable_type().into());
        eq(
            "is_negative_redirectable",
            s.is_negative_redirectable().into(),
        );
        eq(
            "is_other_negative_redirectable",
            s.is_other_negative_redirectable().into(),
        );
        eq("is_portal", s.is_portal_spell().into());
        eq("has_item_category", s.has_item_category().into());
        eq("updates_run_rate", s.updates_run_rate().into());
        eq(
            "updates_max_vitals",
            s.updates_max_vitals()
                .iter()
                .map(|v| i64::from(v.0))
                .collect::<Vec<_>>()
                .into(),
        );
        eq("min_damage", s.min_damage().into());
        eq("max_damage", s.max_damage().into());
        eq("max_boost", s.max_boost().into());
        eq("power_mod", s.power_mod().into());
        eq(
            "scarabs",
            f.scarabs().iter().map(|x| x.0).collect::<Vec<_>>().into(),
        );
        eq("formula_level", f.level().into());
        eq("formula_power", f.power().into());
        eq("has_windup", f.has_windup_gestures().into());
        // V380: the foci formula is retail's now; ACE's recorded one is only the record.
        let foci = f.clone().get_foci_formula();
        let comps: Vec<u32> = f.components.clone();
        let foci_err = (Some(foci.clone()) != client_foci(&comps)).then(|| {
            format!(
                "foci: {foci:?} is not the client's {:?}",
                client_foci(&comps)
            )
        });
        let params =
            CastSpellParams::new(s.clone(), None, 0, 0, None, CastingPreCheckStatus::Success);
        eq(
            "cast_params_has_windup",
            params.has_windup_gestures().into(),
        );
        errs.extend(foci_err);
        let spread = magic::get_spread_angle_per_step(&s);
        if !same_f32(spread, f32_of(&o["spread_angle_per_step"]).expect("float")) {
            errs.push(format!(
                "spread_angle_per_step: ACE {}, we {spread}",
                o["spread_angle_per_step"]
            ));
        }
        if let Err(e) = same(&o["first_scarab"], catch(|| f.first_scarab().0), i32_of) {
            errs.push(format!("first_scarab: {e}"));
        }
        if let Err(e) = same(&o["scale"], catch(|| f.scale()).map(f32::to_bits), |v| {
            f32_of(v).expect("float").to_bits()
        }) {
            errs.push(format!("scale: {e}"));
        }
        (!errs.is_empty()).then(|| format!("{}: {}", c.input, errs.join("; ")))
    });
}

// ------------------------------------------------------------------------------ fixture

const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0001);
const ITEM: ObjectGuid = ObjectGuid::new(0x8000_0002);
const GEM: ObjectGuid = ObjectGuid::new(0x8000_0003);

/// One test spell: the dat half and the database half.
struct Def {
    id: u32,
    name: &'static str,
    school: MagicSchool,
    meta: SpellType,
    flags: u32,
    category: u32,
    power: u32,
    db: DbSpell,
}

fn def(
    id: u32,
    name: &'static str,
    school: MagicSchool,
    meta: SpellType,
    flags: u32,
    db: DbSpell,
) -> Def {
    Def {
        id,
        name,
        school,
        meta,
        flags,
        category: id,
        power: 10,
        db: DbSpell {
            id,
            name: name.into(),
            ..db
        },
    }
}

const BENEFICIAL: u32 = 0x4;
const RESISTABLE: u32 = 0x1;
const ADD_ATTR: u32 = 0x1 | 0x1000 | 0x8000;

/// The spells the scenarios cast.
fn defs() -> Vec<Def> {
    let mana = u32::from(PropertyAttribute2nd::Mana.0);
    vec![
        // 1: a beneficial Strength buff (not resistable).
        def(
            1,
            "Strength Other I",
            MagicSchool::CreatureEnchantment,
            SpellType::Enchantment,
            BENEFICIAL,
            DbSpell {
                stat_mod_type: Some(ADD_ATTR),
                stat_mod_key: Some(1),
                stat_mod_val: Some(10.0),
                ..DbSpell::default()
            },
        ),
        // 2: a resistable Weakness debuff.
        def(
            2,
            "Weakness Other I",
            MagicSchool::CreatureEnchantment,
            SpellType::Enchantment,
            RESISTABLE,
            DbSpell {
                stat_mod_type: Some(ADD_ATTR),
                stat_mod_key: Some(1),
                stat_mod_val: Some(-10.0),
                ..DbSpell::default()
            },
        ),
        // 3: Heal Other: a fixed +10 health boost.
        def(
            3,
            "Heal Other I",
            MagicSchool::LifeMagic,
            SpellType::Boost,
            BENEFICIAL,
            DbSpell {
                boost: Some(10),
                boost_variance: Some(0),
                damage_type: Some(DamageType::Health.0),
                ..DbSpell::default()
            },
        ),
        // 4: Harm Other: a fixed -20 health boost.
        def(
            4,
            "Harm Other I",
            MagicSchool::LifeMagic,
            SpellType::Boost,
            0,
            DbSpell {
                boost: Some(-20),
                boost_variance: Some(0),
                damage_type: Some(DamageType::Health.0),
                ..DbSpell::default()
            },
        ),
        // 5: Drain Mana Other: half of the target's mana to the caster, 10% lost.
        def(
            5,
            "Drain Mana Other I",
            MagicSchool::LifeMagic,
            SpellType::Transfer,
            0,
            DbSpell {
                source: Some(i32::from(PropertyAttribute2nd::Mana.0)),
                destination: Some(i32::from(PropertyAttribute2nd::Mana.0)),
                proportion: Some(0.5),
                loss_percent: Some(0.1),
                transfer_bitfield: Some(
                    (TransferFlags::TargetSource.0 | TransferFlags::CasterDestination.0)
                        .cast_unsigned(),
                ),
                ..DbSpell::default()
            },
        ),
        // 6: a life projectile that drains a quarter of the caster's mana (no projectile rows).
        def(
            6,
            "Mana Bolt",
            MagicSchool::LifeMagic,
            SpellType::LifeProjectile,
            0,
            DbSpell {
                e_type: Some(DamageType::Mana.0.cast_unsigned()),
                drain_percentage: Some(0.25),
                ..DbSpell::default()
            },
        ),
        // 7: a dispel of every negative spell.
        def(
            7,
            "Dispel Other",
            MagicSchool::CreatureEnchantment,
            SpellType::Dispel,
            0,
            DbSpell {
                max_power: Some(1000),
                number: Some(-1),
                align: Some(2),
                ..DbSpell::default()
            },
        ),
        // 8: a second debuff, another category.
        def(
            8,
            "Clumsiness Other I",
            MagicSchool::CreatureEnchantment,
            SpellType::Enchantment,
            0,
            DbSpell {
                stat_mod_type: Some(ADD_ATTR),
                stat_mod_key: Some(4),
                stat_mod_val: Some(-10.0),
                ..DbSpell::default()
            },
        ),
        // 9: a spell type the engine has no case for.
        def(
            9,
            "Odd Spell",
            MagicSchool::CreatureEnchantment,
            SpellType::Undef,
            0,
            DbSpell::default(),
        ),
        // 10: a Heal Self (self-targeted).
        def(
            10,
            "Heal Self I",
            MagicSchool::LifeMagic,
            SpellType::Boost,
            BENEFICIAL | 0x8,
            DbSpell {
                boost: Some(10),
                boost_variance: Some(0),
                damage_type: Some(DamageType::Health.0),
                ..DbSpell::default()
            },
        ),
        // 11: Stamina to Mana Self (caster to caster).
        def(
            11,
            "Stamina to Mana Self I",
            MagicSchool::LifeMagic,
            SpellType::Transfer,
            BENEFICIAL | 0x8,
            DbSpell {
                source: Some(i32::from(PropertyAttribute2nd::Stamina.0)),
                destination: Some(i32::try_from(mana).unwrap()),
                proportion: Some(0.5),
                transfer_bitfield: Some(
                    (TransferFlags::CasterSource.0 | TransferFlags::CasterDestination.0)
                        .cast_unsigned(),
                ),
                ..DbSpell::default()
            },
        ),
        // 12: a war bolt with one projectile (ShowResistInfo buffers its report).
        def(
            12,
            "Flame Bolt I",
            MagicSchool::WarMagic,
            SpellType::Projectile,
            RESISTABLE,
            DbSpell {
                num_projectiles: Some(1),
                e_type: Some(DamageType::Fire.0.cast_unsigned()),
                ..DbSpell::default()
            },
        ),
    ]
}

/// The encryption key of a spell's formula: `ComputeHash(name) % 0x12107680 +
/// ComputeHash(desc) % 0xBEADCF45`.
fn formula_key(name: &str, desc: &str) -> u32 {
    (compute_hash(name) % 0x1210_7680).wrapping_add(compute_hash(desc) % 0xBEAD_CF45)
}

/// Every test spell's formula: Lead Scarab, a herb, a powder, a potion and a talisman.
const FORMULA: [u32; 5] = [1, 10, 20, 30, 40];

fn spell_base(d: &Def) -> SpellBase {
    let desc = "desc";
    let key = formula_key(d.name, desc);
    let mut raw_comps = [0u32; 8];
    for (slot, c) in raw_comps.iter_mut().zip(FORMULA) {
        *slot = c.wrapping_add(key);
    }
    SpellBase {
        name: d.name.into(),
        description: desc.into(),
        school: d.school.0.cast_unsigned(),
        category: d.category,
        bitfield: d.flags,
        power: i32::try_from(d.power).expect("a small power"),
        meta_spell_type: d.meta.0.cast_unsigned(),
        meta_spell_id: d.id,
        duration: matches!(d.meta, SpellType::Enchantment).then_some((1800.0, 0.0, 0.0)),
        raw_comps,
        comp_key: key,
        comps: FORMULA.to_vec(),
        component_loss: 0.5,
        ..spell_base_default()
    }
}

fn component(name: &str, component_type: u32, gesture: u32, cdm: f32) -> SpellComponent {
    SpellComponent {
        name: name.into(),
        category: 0,
        icon: 0,
        component_type,
        gesture,
        time: 0.0,
        text: String::new(),
        cdm,
    }
}

fn world() -> World {
    let defs = defs();
    let table = SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells: defs.iter().map(|d| (d.id, spell_base(d))).collect(),
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    };
    let comps = SpellComponentTable {
        id: DataId(0x0E00_000F),
        buckets: 256,
        components: BTreeMap::from([
            (1, component("Lead Scarab", 1, 0x1300_0001, 1.0)),
            (2, component("Iron Scarab", 1, 0x1300_0002, 1.0)),
            (10, component("Hyssop", 2, 0, 1.0)),
            (20, component("Powdered Agate", 3, 0, 1.0)),
            (30, component("Stibnite", 4, 0, 1.0)),
            (40, component("Poplar Talisman", 5, 0x1300_0040, 1.0)),
            (188, component("Prismatic Taper", 6, 0, 1.0)),
        ]),
    };
    let mapper = DualDidMapper(DidMapper {
        id: DataId(0x2700_0002),
        enum_to_id: vec![
            (1, 691),
            (2, 689),
            (10, 774),
            (20, 789),
            (30, 760),
            (40, 749),
            (188, 20631),
        ],
        enum_to_name: Vec::new(),
        enum_to_id_internal: Vec::new(),
        enum_to_name_internal: Vec::new(),
    });
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_spell_table(table)
        .with_spell_components_table(comps)
        .with_portal(0x2700_0002, mapper)
        .build()
        .expect("fake dats");
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_790_000_000.0,
        utc: DotNetDateTime::new(2026, 9, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats);
    let mut content = MemContent::new();
    for d in defs {
        content = content.spell(d.db);
    }
    w.content = Arc::new(content);

    for (guid, class, name) in [
        (PLAYER, Class::Player, "Caster"),
        (MONSTER, Class::Creature, "Drudge"),
        (ITEM, Class::GenericObject, "Sword"),
        (GEM, Class::Gem, "Gem"),
    ] {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.properties_enchantment_registry = Some(Vec::new());
        o.set_property(PropertyString::Name, name.to_owned());
        w.objects.insert(o).expect("fresh guid");
    }
    for g in [PLAYER, MONSTER] {
        set_vital(&mut w, g, PropertyAttribute2nd::MaxHealth, 100, 100);
        set_vital(&mut w, g, PropertyAttribute2nd::MaxStamina, 100, 100);
        set_vital(&mut w, g, PropertyAttribute2nd::MaxMana, 100, 100);
    }
    w.sessions.insert(
        S,
        SessionData {
            player: Some(PLAYER),
            account: Some("tester".to_owned()),
            ..SessionData::default()
        },
    );
    w
}

/// The biota's vital record (starting value `max`; the neutral vital table adds nothing) and the
/// creature's live `Vitals[vital]` (4.0's `CreatureVital`, as the Creature constructor makes it).
fn set_vital(w: &mut World, g: ObjectGuid, vital: PropertyAttribute2nd, max: u32, current: u32) {
    let o = w.objects.get_mut(g).expect("object");
    o.biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new)
        .insert(
            vital,
            PropertiesAttribute2nd {
                init_level: max,
                current_level: current,
                ..PropertiesAttribute2nd::default()
            },
        );
    if !o.vitals().contains_key(&vital) {
        let cv = CreatureVital::new(o, vital);
        o.vitals_mut().insert(vital, cv);
    }
}

fn set_skill(w: &mut World, g: ObjectGuid, skill: Skill, level: u32) {
    let o = w.objects.get_mut(g).expect("object");
    o.biota
        .properties_skill
        .get_or_insert_with(DotNetDict::new)
        .insert(
            skill,
            PropertiesSkill {
                init_level: level,
                ..PropertiesSkill::default()
            },
        );
}

fn current(w: &World, g: ObjectGuid, vital: PropertyAttribute2nd) -> u32 {
    magic::vital_current(w, g, vital)
}

fn spell(w: &World, id: u32) -> Spell {
    Spell::new(w, id, true)
}

fn registry_ids(w: &World, g: ObjectGuid) -> Vec<i32> {
    w.objects
        .get(g)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .as_ref()
        .unwrap()
        .iter()
        .map(|e| e.spell_id)
        .collect()
}

/// The captured sends: `(opcode, game event type or 0, chat text if a system chat)`.
fn sent() -> Vec<(u32, u32, Option<String>)> {
    take_sent()
        .into_iter()
        .map(|(_, _, b)| {
            let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
            let ev = if op == 0xF7B0 {
                u32::from_le_bytes(b[12..16].try_into().unwrap())
            } else {
                0
            };
            let text = (op == 0xF7E0).then(|| {
                let len = usize::from(u16::from_le_bytes(b[4..6].try_into().unwrap()));
                String::from_utf8_lossy(&b[6..6 + len]).into_owned()
            });
            (op, ev, text)
        })
        .collect()
}

fn chats() -> Vec<String> {
    sent().into_iter().filter_map(|(_, _, t)| t).collect()
}

const EV_UPDATE_ENCHANTMENT: u32 = 0x02C2;
const EV_WEENIE_ERROR: u32 = 0x028A;
const SOUND: u32 = 0xF750;

// ------------------------------------------------------------------------------ scenarios

/// `TryCastSpell` -> `HandleCastSpell` (Enchantment) -> `CreateEnchantment`: the buff is added to
/// the target's registry through `EnchantmentManagerWithCaching.Add`, and the caster is told
/// "You cast X on Y" (`casterCheck`: the caster is `this`).
#[test]
fn a_buff_lands_through_the_enchantment_manager() {
    let mut w = world();
    let s = spell(&w, 1);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &s,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );

    assert_eq!(registry_ids(&w, MONSTER), [1]);
    assert_eq!(
        emc::get_attribute_mod_additive(
            &mut w,
            MONSTER,
            empyrean_entity::enums::properties::PropertyAttribute::Strength
        ),
        10
    );
    assert_eq!(chats(), ["You cast Strength Other I on Drudge"]);

    // On yourself: the registry event, then (from CreateEnchantment's player branch first) the chat.
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &s,
        Some(PLAYER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(registry_ids(&w, PLAYER), [1]);
    let got = sent();
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(
        got[0].2.as_deref(),
        Some("You cast Strength Other I on yourself")
    );
    assert_eq!((got[1].0, got[1].1), (0xF7B0, EV_UPDATE_ENCHANTMENT));

    // Recast: "refreshing".
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &s,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(
        chats(),
        ["You cast Strength Other I on Drudge, refreshing Strength Other I"]
    );
}

/// An item caster that is a gem casts as the item (`CreateEnchantment(target, itemCaster,
/// itemCaster, ..)`): the caster's guid in the registry is the gem's, the message still says
/// "You".
#[test]
fn a_gem_casts_as_itself() {
    let mut w = world();
    let s = spell(&w, 1);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &s,
        Some(MONSTER),
        Some(GEM),
        None,
        false,
        false,
        true,
    );
    let reg = w
        .objects
        .get(MONSTER)
        .unwrap()
        .biota
        .properties_enchantment_registry
        .clone()
        .unwrap();
    assert_eq!(reg[0].caster_object_id, GEM.full());
    assert_eq!(chats(), ["You cast Strength Other I on Drudge"]);
}

/// `HandleCastSpell_Boost`: `ThreadSafeRandom.Next(min, max)` (10..10), times the resistance mod,
/// through `UpdateVitalDelta`; the caster's message; a harm drains and a heal restores.
#[test]
fn a_boost_changes_the_vital() {
    let mut w = world();
    set_vital(&mut w, MONSTER, PropertyAttribute2nd::MaxHealth, 100, 50);
    let heal = spell(&w, 3);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &heal,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(current(&w, MONSTER, PropertyAttribute2nd::MaxHealth), 60);
    assert_eq!(
        chats(),
        ["With Heal Other I you restore 10 points of health to Drudge."]
    );

    let harm = spell(&w, 4);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &harm,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(current(&w, MONSTER, PropertyAttribute2nd::MaxHealth), 40);
    assert_eq!(
        chats(),
        ["With Harm Other I you drain 20 points of health from Drudge."]
    );

    let log: Vec<(ObjectGuid, DamageType, i32, u32)> = damage_history::of(&w, MONSTER)
        .log
        .iter()
        .map(|e| (e.attacker, e.damage_type, e.amount, e.current_health))
        .collect();
    assert_eq!(
        log,
        [
            (ObjectGuid::INVALID, DamageType::Undef, 10, 60),
            (PLAYER, DamageType::Health, -20, 40)
        ],
        "OnHeal then Add, each with the health after the change"
    );
    assert_eq!(
        damage_history::of(&w, MONSTER)
            .last_damager()
            .map(|d| d.guid),
        Some(PLAYER)
    );

    // Clamped at the maximum: the message reports the actual change.
    set_vital(&mut w, PLAYER, PropertyAttribute2nd::MaxHealth, 100, 95);
    let heal_self = spell(&w, 10);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &heal_self,
        Some(PLAYER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(current(&w, PLAYER, PropertyAttribute2nd::MaxHealth), 100);
    assert_eq!(
        chats(),
        ["You cast Heal Self I and restore 5 points of your health."]
    );

    // A boost on a dead target does nothing (prevent double deaths).
    set_vital(&mut w, MONSTER, PropertyAttribute2nd::MaxHealth, 100, 0);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &heal,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        false,
    );
    assert_eq!(current(&w, MONSTER, PropertyAttribute2nd::MaxHealth), 0);
    assert!(chats().is_empty());
}

/// `TryResistSpell`: a resistable spell against a far higher magic defense is resisted: nothing is
/// added, the caster hears "X resists your spell" and the resist sound. The same spell from a
/// caster with a far higher skill lands.
#[test]
fn a_resisted_spell_does_nothing() {
    let mut w = world();
    ThreadSafeRandom::seed(7);
    set_skill(&mut w, MONSTER, Skill::MagicDefense, 1000);
    let weakness = spell(&w, 2);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &weakness,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    assert!(registry_ids(&w, MONSTER).is_empty());
    let got = sent();
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0].2.as_deref(), Some("Drudge resists your spell"));
    assert_eq!(got[1].0, SOUND);

    // `tryResist: false` skips the roll.
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &weakness,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        false,
    );
    assert_eq!(registry_ids(&w, MONSTER), [2]);

    // `if (caster == target) resisted = false;`: a resistable spell on yourself always lands,
    // whatever your own magic defense.
    set_skill(&mut w, PLAYER, Skill::MagicDefense, 1000);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &weakness,
        Some(PLAYER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(registry_ids(&w, PLAYER), [2]);
    assert!(chats().iter().all(|t| !t.contains("resist")));

    // A caster far above the defense is not resisted.
    let mut w = world();
    set_skill(&mut w, PLAYER, Skill::CreatureEnchantment, 1000);
    assert!(!magic::try_resist_spell(
        &mut w,
        PLAYER,
        Some(MONSTER),
        &weakness,
        None,
        false
    ));
    // An invincible creature (not only a player) resists every spell (ACE c5f16b44; before it,
    // ACE tested players only).
    w.objects.get_mut(MONSTER).unwrap().set_invincible(true);
    assert!(magic::try_resist_spell(
        &mut w,
        PLAYER,
        Some(MONSTER),
        &weakness,
        None,
        false
    ));
    w.objects.get_mut(MONSTER).unwrap().set_invincible(false);

    // Beneficial, non-resistable spells never roll; a non-creature target cannot resist.
    let buff = spell(&w, 1);
    set_skill(&mut w, MONSTER, Skill::MagicDefense, 1000);
    assert!(!magic::try_resist_spell(
        &mut w,
        PLAYER,
        Some(MONSTER),
        &buff,
        None,
        false
    ));
    assert!(!magic::try_resist_spell(
        &mut w,
        PLAYER,
        Some(ITEM),
        &weakness,
        None,
        false
    ));
}

/// Mana is consumed: a Life projectile drains `DrainPercentage` of the caster's mana before its
/// projectiles are made (`HandleCastSpell_Projectile`); a Drain Mana transfer moves the target's
/// mana to the caster, less `LossPercent`; `GetManaCost` never goes below 1.
#[test]
fn mana_is_consumed() {
    let mut w = world();
    let bolt = spell(&w, 6);
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &bolt,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(current(&w, PLAYER, PropertyAttribute2nd::MaxMana), 75);

    let mut w = world();
    set_vital(&mut w, MONSTER, PropertyAttribute2nd::MaxMana, 100, 80);
    set_vital(&mut w, PLAYER, PropertyAttribute2nd::MaxMana, 100, 40);
    let drain = spell(&w, 5);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &drain,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    // src = Round(80 * 0.5) = 40; dest = Round(40 * (1 - 0.1f)) = 36.
    assert_eq!(current(&w, MONSTER, PropertyAttribute2nd::MaxMana), 40);
    assert_eq!(current(&w, PLAYER, PropertyAttribute2nd::MaxMana), 76);
    assert_eq!(
        chats(),
        ["You gain 36 points of mana due to casting Drain Mana Other I on Drudge"]
    );

    // Caster to caster, capped by the missing mana: 40 stamina would give 40, only 24 is missing;
    // the source change is scaled by 24/40.
    let mut w = world();
    set_vital(&mut w, PLAYER, PropertyAttribute2nd::MaxStamina, 100, 80);
    set_vital(&mut w, PLAYER, PropertyAttribute2nd::MaxMana, 100, 76);
    let s2m = spell(&w, 11);
    start_capture();
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &s2m,
        Some(PLAYER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(current(&w, PLAYER, PropertyAttribute2nd::MaxStamina), 56);
    assert_eq!(current(&w, PLAYER, PropertyAttribute2nd::MaxMana), 100);
    assert_eq!(chats(), ["You cast Stamina to Mana Self I on yourself and lose 24 points of stamina and also gain 24 points of mana"]);

    ThreadSafeRandom::seed(1);
    assert_eq!(
        creature_magic::get_mana_cost(50, 20, 0),
        20,
        "no Mana Conversion: the base cost"
    );
    assert_eq!(creature_magic::get_mana_cost(0, 0, 500), 1, "never below 1");
}

/// `HandleCastSpell_Dispel`: `SelectDispel` then `Dispel` on the target, and the message built by
/// `BuildSpellList` ("A, and B").
#[test]
fn a_dispel_removes_the_selected_spells() {
    let mut w = world();
    let sp = spell(&w, 2);
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &sp,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        false,
    );
    let sp = spell(&w, 8);
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &sp,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        false,
    );
    let sp = spell(&w, 1);
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &sp,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        false,
    );
    assert_eq!(registry_ids(&w, MONSTER), [2, 8, 1]);

    ThreadSafeRandom::seed(3);
    start_capture();
    let sp = spell(&w, 7);
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &sp,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(
        registry_ids(&w, MONSTER),
        [1],
        "only the beneficial spell stays"
    );
    let text = chats();
    assert_eq!(text.len(), 1);
    assert!(
        text[0] == "You cast Dispel Other on Drudge and dispel: Weakness Other I, and Clumsiness Other I."
            || text[0] == "You cast Dispel Other on Drudge and dispel: Clumsiness Other I, and Weakness Other I.",
        "{}",
        text[0]
    );

    start_capture();
    let sp = spell(&w, 7);
    magic::try_cast_spell(
        &mut w,
        PLAYER,
        &sp,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(
        chats(),
        ["You cast Dispel Other on Drudge, but the dispel fails."]
    );
}

/// `BuildSpellList`: "A", "A, and B", "A, B, and C".
#[test]
fn build_spell_list_joins_like_ace() {
    let w = world();
    let se = |id: u32| {
        SpellEnchantment::new(
            &w,
            empyrean_entity::models::PropertiesEnchantmentRegistry {
                spell_id: i32::try_from(id).unwrap(),
                ..Default::default()
            },
        )
    };
    assert_eq!(magic::build_spell_list(&[]), "");
    assert_eq!(magic::build_spell_list(&[se(1)]), "Strength Other I");
    assert_eq!(
        magic::build_spell_list(&[se(1), se(2)]),
        "Strength Other I, and Weakness Other I"
    );
    assert_eq!(
        magic::build_spell_list(&[se(1), se(2), se(3)]),
        "Strength Other I, Weakness Other I, and Heal Other I"
    );
}

/// `TryCastSpell` for a spell with no database row tells a player target it is not implemented;
/// `HandleCastSpell` on an unhandled `MetaSpellType` tells a player caster and answers false; a
/// Life spell with a target type needs a live target creature.
#[test]
fn unhandled_spells_report_not_implemented() {
    let mut w = world();
    let mut s = spell(&w, 1);
    s.spell = None;
    start_capture();
    magic::try_cast_spell(
        &mut w,
        MONSTER,
        &s,
        Some(PLAYER),
        None,
        None,
        false,
        false,
        true,
    );
    assert_eq!(chats(), ["Strength Other I spell not implemented, yet!"]);

    let odd = spell(&w, 9);
    start_capture();
    assert!(!magic::handle_cast_spell(
        &mut w,
        PLAYER,
        &odd,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        false
    ));
    assert_eq!(chats(), ["Spell not implemented, yet!"]);

    let mut heal = spell(&w, 3);
    heal.spell_base.as_mut().unwrap().non_component_target_type = 0x10;
    assert!(!magic::handle_cast_spell(
        &mut w, PLAYER, &heal, None, None, None, false, false, false
    ));
    set_vital(&mut w, MONSTER, PropertyAttribute2nd::MaxHealth, 100, 0);
    assert!(!magic::handle_cast_spell(
        &mut w,
        PLAYER,
        &heal,
        Some(MONSTER),
        None,
        None,
        false,
        false,
        false
    ));
}

/// The small members: `VerifyDispelPKStatus` (no PK timer: true), `GetAbsorbMagicDamage` (the
/// property, else 0.25 for the imbue), `OnSpellsActivated/Deactivated`, `GetMaxSpellLevel`'s
/// cache, `GetNonComponentTargetTypes` (null for other target types), the projectile helpers.
#[test]
fn small_members_follow_ace() {
    let mut w = world();
    assert!(magic::verify_dispel_pk_status(
        &mut w,
        Some(PLAYER),
        Some(MONSTER)
    ));

    let o = w.objects.get_mut(ITEM).unwrap();
    assert_eq!(magic::get_absorb_magic_damage(o), None);
    o.set_property(
        PropertyInt::ImbuedEffect,
        ImbuedEffectType::IgnoreSomeMagicProjectileDamage
            .0
            .cast_signed(),
    );
    assert_eq!(magic::get_absorb_magic_damage(o), Some(0.25));
    o.set_property(PropertyFloat::AbsorbMagicDamage, 0.5);
    assert_eq!(magic::get_absorb_magic_damage(o), Some(0.5));

    magic::on_spells_activated(&mut w, ITEM);
    assert!(w.objects.get(ITEM).unwrap().is_affecting());
    magic::on_spells_deactivated(&mut w, ITEM);
    assert!(!w.objects.get(ITEM).unwrap().is_affecting());

    assert_eq!(magic::get_max_spell_level(&mut w, ITEM), 0);
    assert_eq!(
        w.objects
            .get(ITEM)
            .unwrap()
            .wo
            .world_object_magic
            .max_spell_level,
        Some(0)
    );

    let s = spell(&w, 1);
    assert_eq!(magic::get_non_component_target_types(&w, &s, MONSTER), None);

    assert!((magic::PROJ_HEIGHT - 2.0 / 3.0).abs() < f32::EPSILON);
    assert!((magic::PROJ_HEIGHT_ARC - 5.0 / 6.0).abs() < f32::EPSILON);
    let q = magic::one_eighty();
    assert!(q.z > 0.999_999 && q.w.abs() < 1e-6);

    // `CreateSpellProjectiles` with NumProjectiles == 0 logs and makes nothing.
    assert!(magic::create_spell_projectiles(
        &mut w,
        PLAYER,
        &s,
        Some(MONSTER),
        None,
        false,
        false,
        0
    )
    .is_empty());
    let _ = ProjectileSpellType::Arc;
    let _ = EquipMask::Cloak;
}

/// `SpellFormula` over the dat: the decrypted components, the account-randomised player formula
/// (formula version 0: the base formula), the foci formula for a player with the school's
/// Infused augmentation, the windup and cast gestures, `GetRequiredComps` through the component
/// `DualDidMapper`, `TryBurnComponents` (CDM 1 and `ComponentLoss` 0.5: each component a coin
/// flip) and the consume string.
#[test]
fn spell_formula_on_the_dat() {
    let mut w = world();
    let s = spell(&w, 1);
    let mut f = s.formula.clone().expect("formula");
    assert_eq!(f.components, FORMULA);
    assert_eq!(f.scarabs(), [Scarab::Lead]);
    assert_eq!(
        (f.level(), f.power(), s.level(), s.level_match()),
        (1, 1, 1, true)
    );
    assert_eq!(f.windup_gestures(&w), [MotionCommand(0x1300_0001)]);
    assert!(!f.has_windup_gestures());
    assert_eq!(
        f.cast_gesture(&w),
        MotionCommand::Invalid,
        "no player formula yet"
    );

    assert_eq!(f.get_player_formula(&w, PLAYER), FORMULA);
    assert_eq!(f.foci_formula.as_deref(), Some(&[1, 188][..]));
    assert_eq!(
        f.current_formula.as_deref(),
        Some(&FORMULA[..]),
        "no foci: the player formula"
    );
    assert_eq!(f.cast_gesture(&w), MotionCommand(0x1300_0040));
    let comps: Vec<(u32, i32)> = f
        .get_required_comps(&w)
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    assert_eq!(comps, [(691, 1), (774, 1), (789, 1), (760, 1), (749, 1)]);
    assert_eq!(
        empyrean_world::entity::spell::Spell::get_component_wcid(&w, 999),
        0
    );

    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_property(PropertyInt::AugmentationInfusedCreatureMagic, 1);
    f.get_current_formula(&w, PLAYER);
    assert_eq!(
        f.current_formula.as_deref(),
        Some(&[1, 188][..]),
        "infused: the foci formula"
    );

    let mut s2 = s.clone();
    s2.formula = Some(f.clone());
    set_skill(&mut w, PLAYER, Skill::CreatureEnchantment, 10);
    ThreadSafeRandom::seed(11);
    let burned = s2.try_burn_components(&mut w, PLAYER);
    assert!(burned.iter().all(|c| [1, 188].contains(c)), "{burned:?}");
    assert_eq!(
        Spell::get_consume_string(&w, &[1, 188]),
        "The spell consumed the following components: Lead Scarab, Prismatic Taper"
    );
    assert_eq!(
        Spell::get_component_names(&w, &[1, 999, 40]),
        ["Lead Scarab", "Poplar Talisman"]
    );

    // A spell whose formula starts with a non-scarab has client level 0 and throws on Scale.
    let odd = SpellFormula {
        components: vec![10, 1],
        ..SpellFormula::default()
    };
    assert_eq!((odd.level(), odd.power()), (0, 0));
    assert_eq!(odd.scarabs(), [Scarab::Lead]);
    assert!((odd.scale() - 0.05).abs() < f32::EPSILON);
    assert!(catch(|| SpellFormula::default().scale()).is_err());
    assert_eq!(spell_formula::min_power(8), 400);
    assert!(catch(|| spell_formula::min_power(9)).is_err());
    let _ = SpellFlags::FastCast;
}

/// `CastSpellParams`: `HasWindupGestures` (no FastCast, no caster item, a non-Lead scarab) and
/// `ToString`.
#[test]
fn cast_spell_params_follow_ace() {
    let w = world();
    let mut s = spell(&w, 1);
    let p = CastSpellParams::new(
        s.clone(),
        None,
        150,
        20,
        Some(MONSTER),
        CastingPreCheckStatus::Success,
    );
    assert!(!p.has_windup_gestures(), "Lead Scarab only");
    assert_eq!(
        p.to_string(&w),
        "Strength Other I, , 150, 20, Drudge, Success"
    );
    let p = CastSpellParams::new(
        s.clone(),
        Some(GEM),
        0,
        0,
        None,
        CastingPreCheckStatus::InvalidPKStatus,
    );
    assert_eq!(
        p.to_string(&w),
        "Strength Other I, Gem, 0, 0, null, InvalidPKStatus"
    );

    s.formula.as_mut().unwrap().components = vec![2, 10];
    assert!(
        CastSpellParams::new(s.clone(), None, 0, 0, None, CastingPreCheckStatus::Success)
            .has_windup_gestures()
    );
    assert!(!CastSpellParams::new(
        s.clone(),
        Some(GEM),
        0,
        0,
        None,
        CastingPreCheckStatus::Success
    )
    .has_windup_gestures());
    s.spell_base.as_mut().unwrap().bitfield = SpellFlags::FastCast.0.cast_unsigned();
    assert!(
        !CastSpellParams::new(s, None, 0, 0, None, CastingPreCheckStatus::Success)
            .has_windup_gestures()
    );
}

/// `StringToBoolConverter`: `Convert.ToBoolean` for strings, the JSON literals, `JsonException`
/// otherwise; `Write` is the literal.
#[test]
fn string_to_bool_converter_follows_ace() {
    assert_eq!(
        StringToBoolConverter::read(&JsonToken::String("true")),
        Ok(true)
    );
    assert_eq!(
        StringToBoolConverter::read(&JsonToken::String(" False ")),
        Ok(false)
    );
    assert_eq!(
        StringToBoolConverter::read(&JsonToken::String("TRUE")),
        Ok(true)
    );
    assert_eq!(
        StringToBoolConverter::read(&JsonToken::String("yes")),
        Err("FormatException")
    );
    assert_eq!(StringToBoolConverter::read(&JsonToken::True), Ok(true));
    assert_eq!(StringToBoolConverter::read(&JsonToken::False), Ok(false));
    assert_eq!(
        StringToBoolConverter::read(&JsonToken::Other),
        Err("JsonException")
    );
    assert_eq!(
        (
            StringToBoolConverter::write(true),
            StringToBoolConverter::write(false)
        ),
        ("true", "false")
    );
}

/// Every error in `sent` is a WeenieError event, for the portal paths that answer with one.
#[test]
fn portal_recall_without_a_link_sends_the_weenie_error() {
    let mut w = world();
    let mut s = spell(&w, 1);
    s.spell_base.as_mut().unwrap().meta_spell_id = 2645; // PortalRecall
    start_capture();
    magic::handle_cast_spell_portal_recall(&mut w, PLAYER, &s, Some(PLAYER));
    let got = sent();
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].0, got[0].1), (0xF7B0, EV_WEENIE_ERROR));
}

// ------------------------------------------------------------------ Shared rules: dereth-client-model (client)

/// `SpellFormula.Power` / `.Level` of a one-component formula against the client's
/// `scarab_power_level` and `spell_level_by_rough_heuristic`, for every component id 0..=300.
#[test]
fn scarab_power_and_level_agree_with_dereth_client_model() {
    for c in 0..=300u32 {
        let f = SpellFormula {
            components: vec![c],
            ..SpellFormula::default()
        };
        let client_power = dereth_rules::magic::scarab_power_level(c);
        assert_eq!(f.power(), client_power, "power of component {c}");
        let client_level = if client_power == 0 {
            0
        } else {
            dereth_client_model::magic::spell_level_by_rough_heuristic(client_power)
        };
        assert_eq!(f.level(), client_level, "level of component {c}");
    }
}

/// foci formula agrees with dereth client model.
/// V380.
#[test]
fn foci_formula_agrees_with_dereth_client_model() {
    let mut bad = Vec::new();
    for f in foci_sweep() {
        let ours = SpellFormula {
            components: f.clone(),
            ..SpellFormula::default()
        }
        .get_foci_formula();
        if let Some(theirs) = client_foci(&f) {
            if ours != theirs {
                bad.push(format!("{f:?}: ACE {ours:?}, client {theirs:?}"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} disagreements, e.g. {}",
        bad.len(),
        bad.iter().take(5).cloned().collect::<Vec<_>>().join("; ")
    );
}

/// The formulas where both rules must agree: a single scarab first, then non-scarab components.
#[test]
fn foci_formula_agrees_with_dereth_client_model_for_single_scarab_formulas() {
    for &s in &[1u32, 2, 3, 4, 5, 6, 110, 112, 192, 193] {
        let f = vec![s, 10, 20, 30, 40];
        let ours = SpellFormula {
            components: f.clone(),
            ..SpellFormula::default()
        }
        .get_foci_formula();
        assert_eq!(Some(ours), client_foci(&f), "{f:?}");
    }
}

fn foci_sweep() -> Vec<Vec<u32>> {
    let scarabs = [1u32, 2, 3, 4, 5, 6, 110, 111, 112, 192, 193];
    let mut out = Vec::new();
    for &a in &scarabs {
        for &b in &scarabs {
            out.push(vec![a, b, 10, 40]);
            out.push(vec![10, a, b, 40]);
        }
    }
    out
}

/// `scarab_only_formula` on an 8-slot formula, the trailing zeros dropped; `None` when the
/// formula does not fit the client's 8 slots.
fn client_foci(f: &[u32]) -> Option<Vec<u32>> {
    if f.len() > 8 {
        return None;
    }
    let mut comps = [0u32; 8];
    comps[..f.len()].copy_from_slice(f);
    Some(
        dereth_rules::magic::scarab_only_formula(&comps)
            .into_iter()
            .take_while(|c| *c != 0)
            .collect(),
    )
}

// ------------------------------------------------------------------ real content (retail dats)

#[cfg(feature = "real-content")]
mod real_content {
    //! The retail dats under `DERETH_TEST_DAT_DIR`; fails, never skips,
    //! when they are absent.

    use super::*;
    use empyrean_dat::{DatManager, RealDats};
    use std::sync::OnceLock;

    fn dats() -> Arc<DatManager> {
        static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
        Arc::clone(DATS.get_or_init(|| {
            let dir = dereth_dat::testing::dat_dir();
            let real = RealDats::open(&dir).unwrap_or_else(|e| {
                panic!(
                    "the real-content tier needs the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
                    dir.display()
                )
            });
            DatManager::initialize(Arc::new(real)).expect("the retail dats initialize")
        }))
    }

    fn real_world() -> World {
        let now = ClockSnapshot {
            portal_year_ticks: 0.0,
            unix_time: 0.0,
            utc: DotNetDateTime::new(2026, 1, 1),
            monotonic: Duration::ZERO,
        };
        World::new(now, dats())
    }

    /// ACE's foci formula, as its `GetFociFormula` builds it: the kept power components, then as
    /// many Prismatic Tapers as the *first* component's power asks for, with no eight-slot cap.
    fn ace_foci(components: &[u32], first_power: u32) -> Vec<u32> {
        let mut out: Vec<u32> = components
            .iter()
            .copied()
            .filter(|&c| spell_formula::is_scarab(c) || c == 111)
            .collect();
        let n = dereth_rules::magic::scarab_only_filler_count(first_power);
        out.extend(std::iter::repeat_n(
            dereth_rules::magic::SCARAB_ONLY_FILLER_SCID,
            n as usize,
        ));
        out
    }

    /// Retail of ace foci.
    /// V380.
    fn retail_of_ace_foci(ace: &[u32]) -> Vec<u32> {
        let taper = dereth_rules::magic::SCARAB_ONLY_FILLER_SCID;
        let kept: Vec<u32> = ace.iter().copied().filter(|&c| c != taper).collect();
        let level = kept
            .iter()
            .map(|&c| dereth_rules::magic::scarab_power_level(c))
            .max()
            .unwrap_or(0);
        let n = (dereth_rules::magic::scarab_only_filler_count(level) as usize)
            .min(8usize.saturating_sub(kept.len()));
        let mut out = kept;
        out.extend(std::iter::repeat_n(taper, n));
        out
    }

    /// Spell formulas match ace on retail dats.
    /// V380.
    #[test]
    fn spell_formulas_match_ace_on_retail_dats() {
        let w = real_world();
        let ruled = Cell::new(0usize);
        replay("spell_formula", "spell_formulas", |c| {
            let id = u32_of(&c.input["id"]);
            let s = Spell::new(&w, id, false);
            let mut f = s.formula.clone().expect("formula");
            let o = &c.output;
            let mut errs = Vec::new();
            let eq = |errs: &mut Vec<String>, name: &str, got: Value| {
                if o[name] != got {
                    errs.push(format!("{name}: ACE {}, we {got}", o[name]));
                }
            };
            eq(&mut errs, "components", f.components.clone().into());
            eq(
                &mut errs,
                "scarabs",
                f.scarabs().iter().map(|x| x.0).collect::<Vec<_>>().into(),
            );
            eq(&mut errs, "level", f.level().into());
            eq(&mut errs, "power", f.power().into());
            eq(&mut errs, "server_level", s.level().into());
            let foci = f.clone().get_foci_formula();
            let ace_record = u32_list(&o["foci"]);
            if ace_record != foci {
                let ace_rule = ace_foci(&f.components, f.power());
                let retail_rule = client_foci(&f.components);
                if ace_record != ace_rule {
                    errs.push(format!(
                        "foci: ACE's record {ace_record:?} is not ACE's rule {ace_rule:?}"
                    ));
                } else if retail_rule.as_ref() == Some(&ace_rule) {
                    errs.push(format!(
                        "foci: ACE {ace_record:?}, we {foci:?}, though the rules agree"
                    ));
                } else if Some(&foci) != retail_rule.as_ref()
                    || foci != retail_of_ace_foci(&ace_record)
                {
                    errs.push(format!(
                        "foci: ACE {ace_record:?}, we {foci:?}, retail {retail_rule:?}"
                    ));
                } else {
                    ruled.set(ruled.get() + 1);
                }
            }
            eq(
                &mut errs,
                "windup",
                f.windup_gestures(&w)
                    .iter()
                    .map(|m| m.0)
                    .collect::<Vec<_>>()
                    .into(),
            );
            eq(&mut errs, "has_windup", f.has_windup_gestures().into());
            if let Err(e) = same(
                &o["monster_formula"],
                catch(|| f.clone().get_monster_formula(&w)),
                u32_list,
            ) {
                errs.push(format!("monster_formula: {e}"));
            }
            f.get_monster_formula(&w);
            eq(&mut errs, "cast_gesture", f.cast_gesture(&w).0.into());
            if let Err(e) = same(&o["scale"], catch(|| f.scale().to_bits()), |v| {
                f32_of(v).expect("float").to_bits()
            }) {
                errs.push(format!("scale: {e}"));
            }
            // V352 (retail): the windup and cast gestures last as long as the client plays them,
            // which is ACE's length plus the frame or two per gesture that ACE's arithmetic leaves
            // out when a gesture's frame range is explicit (up to 0.5 s over a long chain of
            // windups; `anim_timing` checks the playback itself).
            let ace = throws(&o["cast_time"])
                .is_none()
                .then(|| f32_of(&o["cast_time"]).expect("float"));
            let ours = catch(|| f.get_cast_time(&w, 0x0900_0001, 1.0, None));
            let ok = match (ace, &ours) {
                (None, Err(())) => true,
                (Some(a), Ok(g)) => *g >= a - 1e-4 && *g - a <= 0.1 + 0.05 * a,
                _ => false,
            };
            if !ok {
                errs.push(format!("cast_time: ACE {ace:?}, we {ours:?}"));
            }
            f.current_formula.clone_from(&f.player_formula);
            let comps: Vec<Value> = f
                .get_required_comps(&w)
                .iter()
                .map(|(k, v)| serde_json::json!([k, v]))
                .collect();
            eq(&mut errs, "required_comps", comps.into());
            if !o["consume"].is_null() {
                eq(
                    &mut errs,
                    "consume",
                    Spell::get_consume_string(&w, f.current_formula.as_deref().unwrap_or_default())
                        .into(),
                );
            }
            (!errs.is_empty()).then(|| format!("spell {id}: {}", errs.join("; ")))
        });
        // the recorded spells where the two taper rules part (of the 187 in the whole table)
        assert!(
            ruled.get() > 0,
            "the vectors exercise the foci formula difference in V380"
        );
    }

    /// foci formula on retail dats.
    #[test]
    fn foci_formula_matches_single_scarab_spells_on_retail_dats() {
        let w = real_world();
        let (mut agree, mut differ, mut single_scarab_differ) = (0, 0, Vec::new());
        for (&id, base) in &w.dats.portal_dat().spell_table().spells {
            let _ = base;
            let s = Spell::new(&w, id, false);
            let f = s.formula.clone().expect("formula").components;
            let ours = SpellFormula {
                components: f.clone(),
                ..SpellFormula::default()
            }
            .get_foci_formula();
            let Some(theirs) = client_foci(&f) else {
                continue;
            };
            if ours == theirs {
                agree += 1;
            } else {
                differ += 1;
                if f.iter().filter(|c| spell_formula::is_scarab(**c)).count() == 1
                    && !f.contains(&111)
                {
                    single_scarab_differ.push(id);
                }
            }
        }
        eprintln!("foci formula on retail: {agree} agree, {differ} differ");
        assert!(agree > 0);
        assert!(
            single_scarab_differ.is_empty(),
            "single-scarab spells differ: {single_scarab_differ:?}"
        );
    }
}

/// Vital max value is the stat formula.
#[test]
fn vital_max_value_is_the_stat_formula() {
    let mut w = world();
    assert_eq!(
        magic::vital_max_value(&mut w, PLAYER, PropertyAttribute2nd::MaxHealth),
        100
    );
    w.objects.get_mut(PLAYER).unwrap().set_enlightenment(3);
    assert_eq!(
        magic::vital_max_value(&mut w, PLAYER, PropertyAttribute2nd::MaxHealth),
        106
    );
    assert_eq!(
        magic::vital_max_value(&mut w, MONSTER, PropertyAttribute2nd::MaxHealth),
        100
    );
}

/// `SpellComponent.IsValid`: the component table mapped through the `DualDidMapper` (wcid ->
/// component id); a wcid outside the mapper is not a component.
#[test]
fn spell_component_wcids_are_the_mapped_component_table() {
    use empyrean_world::world_objects::spell_component;
    let w = world();
    let wcids = spell_component::build_spell_component_wcids(&w);
    assert_eq!(wcids.len(), 7);
    assert_eq!(wcids.get(&691), Some(&1));
    assert_eq!(wcids.get(&20631), Some(&188));
    assert!(spell_component::is_valid(&w, 749));
    assert!(!spell_component::is_valid(&w, 12345));
    assert!(!spell_component::is_valid(&w, 1));
}

/// Show resist info buffers an unresisted projectiles report.
#[test]
fn show_resist_info_buffers_an_unresisted_projectiles_report() {
    use empyrean_world::managers::player_manager::OnlinePlayer;
    let mut w = world();
    w.player_manager.online_players.insert(
        PLAYER.full(),
        OnlinePlayer {
            guid: PLAYER,
            account: None,
        },
    );
    empyrean_world::world_objects::creature_combat::fields_mut(
        w.objects.get_mut(MONSTER).unwrap(),
    )
    .debug_damage_target = PLAYER;
    let bolt = spell(&w, 12);
    assert_eq!(bolt.num_projectiles(), 1);

    start_capture();
    magic::show_resist_info(
        &mut w, MONSTER, MONSTER, PLAYER, &bolt, 100, 50, 0.25, false,
    );
    assert!(sent().is_empty(), "buffered, not sent");
    let buffer = empyrean_world::world_objects::player_magic::fields(&w, PLAYER)
        .debug_damage_buffer
        .clone()
        .expect("buffered");
    assert!(
        buffer.starts_with("Attacker: Drudge (")
            && buffer.ends_with(
                "Resisted: False
"
            ),
        "{buffer:?}"
    );

    empyrean_world::world_objects::player_magic::fields_mut(&mut w, PLAYER).debug_damage_buffer =
        None;
    start_capture();
    magic::show_resist_info(&mut w, MONSTER, MONSTER, PLAYER, &bolt, 100, 50, 0.25, true);
    let chats: Vec<Option<String>> = sent().into_iter().map(|(_, _, t)| t).collect();
    assert_eq!(chats.len(), 1);
    assert!(
        chats[0]
            .as_deref()
            .is_some_and(|t| t.ends_with("Resisted: True")),
        "{chats:?}"
    );
    assert!(
        empyrean_world::world_objects::player_magic::fields(&w, PLAYER)
            .debug_damage_buffer
            .is_none()
    );
}
