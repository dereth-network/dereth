//! Vectors: fixtures/vectors/spellcasting/
//! Player cast flow (Player_Magic, Player_Spells, MagicState) and SpellProjectile follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.
//! Divergence: V403

use empyrean_common::era::EraExt as _;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SpellBase, SpellComponent};
use dereth_assets::{DidMapper, DualDidMapper, SpellComponentTable, SpellTable};
use dereth_primitives::DataId;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::not_ported::take_local;
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f32_of, i64_of, u64_of, Case};
use empyrean_content::models::world::Spell as DbSpell;
use empyrean_content::models::world::Weenie as ContentWeenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::spell_table::compute_hash;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CombatMode, DamageType, ItemType, MagicSchool, MotionCommand as Mc, MotionStance, PhysicsState,
    PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInt, PropertyString, Skill, SkillAdvancementClass, SpellCategory,
    SpellType, WeenieError, WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_testkit::land;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::cast_spell_params::CastingPreCheckStatus;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::entity::spell::Spell;
use empyrean_world::entity::spell_projectile_type::ProjectileSpellType;
use empyrean_world::entity::timers::{self, TimersState};
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    container, creature_combat, player_magic, player_spells, spell_projectile,
};
use empyrean_world::World;
use serde_json::Value;

// ------------------------------------------------------------------------------------ vectors

fn replay(name: &str, mut each: impl FnMut(&Case) -> Option<String>) {
    let file = vectors::load_named("spellcasting", name);
    assert!(!file.cases.is_empty(), "spellcasting/{name}: no cases");
    let bad: Vec<String> = file.cases.iter().filter_map(&mut each).collect();
    assert!(
        bad.is_empty(),
        "spellcasting/{name}: {} of {} cases differ from ACE:\n  {}",
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

/// The harness's synthetic spell: a school and a power, an empty formula.
fn power_spell(school: i32, power: u32) -> Spell {
    let base = SpellBase {
        name: "Spell 1".into(),
        school: school.cast_unsigned(),
        power: i32::try_from(power).expect("a small power"),
        meta_spell_id: 1,
        ..spell_base_default()
    };
    let db = DbSpell {
        id: 1,
        ..DbSpell::default()
    };
    let mut spell = Spell {
        spell_base: Some(base),
        spell: Some(Arc::new(db)),
        formula: None,
    };
    spell.formula = Some(empyrean_world::entity::spell_formula::SpellFormula::new(
        &spell,
        Vec::new(),
    ));
    spell
}

/// `Player.GetCastingPreCheckStatus` on ACE's compiled code, per seed: each call's status and the
/// next draw after it (so a missing or extra draw shows).
#[test]
fn casting_pre_check_status_matches_ace() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    replay("casting_pre_check_status", |c| {
        ThreadSafeRandom::seed(u64_of(&c.input["seed"]).expect("seed"));
        let calls = c.input["calls"].as_array().expect("calls");
        let outs = c.output.as_array().expect("outs");
        player_magic::fields_mut(&mut h.w, PLAYER).last_success_cast_time = 0.0;
        for (call, want) in calls.iter().zip(outs) {
            let spell = power_spell(i32_of(&call[0]), u32_of(&call[1]));
            player_magic::fields_mut(&mut h.w, PLAYER).last_success_cast_school =
                MagicSchool(i32_of(&call[4]));
            let status = player_magic::get_casting_pre_check_status(
                &mut h.w,
                PLAYER,
                &spell,
                u32_of(&call[2]),
                call[3].as_bool().expect("bool"),
            );
            let next = ThreadSafeRandom::next_float(0.0, 1.0);
            let (ws, wn) = (i32_of(&want[0]), want[1].as_f64().expect("double"));
            if status.0 != ws || next.to_bits() != wn.to_bits() {
                return Some(format!(
                    "seed {} call {call}: ACE ({ws}, {wn}), we ({}, {next})",
                    c.input["seed"], status.0
                ));
            }
        }
        None
    });
}

/// `SpellProjectile.GetProjectileSpellType` on ACE's compiled code over synthetic spells.
#[test]
fn projectile_spell_type_matches_ace() {
    let file = vectors::load_named("spellcasting", "projectile_spell_type");
    let mut spells = BTreeMap::new();
    let mut content = MemContent::new();
    for c in &file.cases {
        let i = &c.input;
        let id = u32_of(&i["id"]);
        let name = i["name"].as_str().expect("name").to_owned();
        spells.insert(
            id,
            SpellBase {
                name: name.clone(),
                category: u32_of(&i["category"]),
                school: 1,
                meta_spell_type: 1,
                meta_spell_id: id,
                ..spell_base_default()
            },
        );
        let wcid = u32_of(&i["wcid"]);
        content = content.spell(DbSpell {
            id,
            name,
            wcid: (wcid != 0).then_some(wcid),
            num_projectiles: Some(i32_of(&i["num_projectiles"])),
            non_tracking: i["non_tracking"].as_bool(),
            spread_angle: i["spread_angle"]
                .as_f64()
                .map(|v| f32_of(&Value::from(v)).expect("float")),
            ..DbSpell::default()
        });
    }
    let table = SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells,
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    };
    let dats = FakeDats::new()
        .with_spell_table(table)
        .build()
        .expect("fake dats");
    let clock = VirtualClock::default();
    let mut w = World::new(ClockSnapshot::take(&clock, 0.0), dats);
    w.content = Arc::new(content);

    replay("projectile_spell_type", |c| {
        let got = spell_projectile::get_projectile_spell_type(&w, u32_of(&c.input["id"]));
        (got.0 != i32_of(&c.output)).then(|| format!("{}: ACE {}, we {}", c.input, c.output, got.0))
    });
}

/// `SpellProjectile.GetProjectileScriptIntensity` on ACE's compiled code.
#[test]
fn projectile_script_intensity_matches_ace() {
    replay("projectile_script_intensity", |c| {
        let mut spell = power_spell(1, u32_of(&c.input["power"]));
        let id = u32_of(&c.input["id"]);
        spell.spell_base.as_mut().unwrap().meta_spell_id = id;
        spell.spell = Some(Arc::new(DbSpell {
            id,
            ..DbSpell::default()
        }));
        let got = spell_projectile::get_projectile_script_intensity_of(
            &spell,
            ProjectileSpellType(i32_of(&c.input["spell_type"])),
        );
        let want = f32_of(&c.output).expect("float");
        (got.to_bits() != want.to_bits()).then(|| format!("{}: ACE {want}, we {got}", c.input))
    });
}

// ------------------------------------------------------------------------------------ fixture

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }

    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

const LB: u32 = 0xA9B4_0000;
const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
const MONSTER: ObjectGuid = ObjectGuid::new(0x8000_0100);
const SESSION: empyrean_net::SessionId = empyrean_net::SessionId {
    client_id: 1,
    generation: 1,
};
const SEED: i32 = 5252;

/// The projectile weenie of the bolts.
const PROJ_WCID: u32 = 64100;
/// The corpse weenie (`WorldObjectFactory.CreateNewWorldObject("corpse")`).
const CORPSE_WCID: u32 = 21;

/// Strength Self I: a self-targeted creature buff (power 10).
const STRENGTH_SELF: u32 = 1;
/// Flame Bolt I: a war bolt (30 fire damage, power 10).
const FLAME_BOLT: u32 = 2;
/// Force Bolt: a war bolt that kills (500 damage).
const FORCE_BOLT: u32 = 3;
/// Strength Self II: the buff at power 100 (the fizzle test casts it with skill 100: chance 0.5).
const STRENGTH_SELF_2: u32 = 4;
/// Mana Bolt: a war bolt that drains mana (30, category ManaLowering).
const MANA_BOLT: u32 = 5;

const BENEFICIAL: u32 = 0x4;
const SELF_TARGETED: u32 = 0x8;
const ADD_ATTR: u32 = 0x1 | 0x1000 | 0x8000;

/// Lead Scarab, Hyssop, Powdered Agate, Stibnite, Poplar Talisman (the component ids).
const FORMULA: [u32; 5] = [1, 10, 20, 30, 40];
/// Their weenie class ids (the `DualDidMapper`).
const COMPONENT_WCIDS: [(u32, u32); 5] = [(1, 691), (10, 774), (20, 789), (30, 760), (40, 749)];

struct Def {
    id: u32,
    name: &'static str,
    school: MagicSchool,
    meta: SpellType,
    flags: u32,
    power: u32,
    db: DbSpell,
}

fn defs() -> Vec<Def> {
    let bolt = |dmg: i32| DbSpell {
        wcid: Some(PROJ_WCID),
        num_projectiles: Some(1),
        base_intensity: Some(dmg),
        variance: Some(0),
        e_type: Some(DamageType::Fire.0.cast_unsigned()),
        ..DbSpell::default()
    };
    let buff = DbSpell {
        stat_mod_type: Some(ADD_ATTR),
        stat_mod_key: Some(1),
        stat_mod_val: Some(10.0),
        ..DbSpell::default()
    };
    vec![
        Def {
            id: STRENGTH_SELF,
            name: "Strength Self I",
            school: MagicSchool::CreatureEnchantment,
            meta: SpellType::Enchantment,
            flags: BENEFICIAL | SELF_TARGETED,
            power: 10,
            db: buff.clone(),
        },
        Def {
            id: FLAME_BOLT,
            name: "Flame Bolt I",
            school: MagicSchool::WarMagic,
            meta: SpellType::Projectile,
            flags: 0,
            power: 10,
            db: bolt(30),
        },
        Def {
            id: FORCE_BOLT,
            name: "Force Bolt",
            school: MagicSchool::WarMagic,
            meta: SpellType::Projectile,
            flags: 0,
            power: 10,
            db: bolt(500),
        },
        Def {
            id: STRENGTH_SELF_2,
            name: "Strength Self II",
            school: MagicSchool::CreatureEnchantment,
            meta: SpellType::Enchantment,
            flags: BENEFICIAL | SELF_TARGETED,
            power: 100,
            db: buff,
        },
        Def {
            id: MANA_BOLT,
            name: "Mana Bolt",
            school: MagicSchool::WarMagic,
            meta: SpellType::Projectile,
            flags: 0,
            power: 10,
            db: bolt(30),
        },
    ]
}

/// The encryption key of a spell's formula (`SpellTable.ComputeHash`).
fn formula_key(name: &str, desc: &str) -> u32 {
    (compute_hash(name) % 0x1210_7680).wrapping_add(compute_hash(desc) % 0xBEAD_CF45)
}

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
        category: if d.id == MANA_BOLT {
            SpellCategory::ManaLowering.0
        } else {
            1000 + d.id
        },
        bitfield: d.flags,
        power: i32::try_from(d.power).expect("a small power"),
        base_mana: 10,
        base_range_constant: 5.0,
        base_range_mod: 1.0,
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

fn component(name: &str, component_type: u32, gesture: u32) -> SpellComponent {
    SpellComponent {
        name: name.into(),
        category: 0,
        icon: 0,
        component_type,
        gesture,
        time: 0.0,
        text: String::new(),
        cdm: 1.0,
    }
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    dats_with(defs(), 0)
}

fn dats_with(defs: Vec<Def>, target_type: u32) -> Arc<empyrean_dat::DatManager> {
    let table = SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells: defs
            .iter()
            .map(|d| {
                let mut base = spell_base(d);
                base.non_component_target_type = target_type;
                (d.id, base)
            })
            .collect(),
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    };
    let comps = SpellComponentTable {
        id: DataId(0x0E00_000F),
        buckets: 256,
        components: BTreeMap::from([
            (1, component("Lead Scarab", 1, Mc::MagicHeal.0)),
            (10, component("Hyssop", 2, 0)),
            (20, component("Powdered Agate", 3, 0)),
            (30, component("Stibnite", 4, 0)),
            (40, component("Poplar Talisman", 5, Mc::MagicBlast.0)),
        ]),
    };
    let mapper = DualDidMapper(DidMapper {
        id: DataId(0x2700_0002),
        enum_to_id: COMPONENT_WCIDS.to_vec(),
        enum_to_name: Vec::new(),
        enum_to_id_internal: Vec::new(),
        enum_to_name_internal: Vec::new(),
    });
    let setup = dereth_assets::geometry::Setup {
        id: DataId(land::TEST_SETUP),
        flags: 0,
        parts: Vec::new(),
        parent_index: None,
        default_scale: None,
        allow_free_heading: false,
        has_physics_bsp: false,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::new(),
        cylspheres: Vec::new(),
        spheres: vec![dereth_assets::common::Sphere {
            center: dereth_primitives::Vec3::new(0.0, 0.0, 0.5),
            radius: 0.5,
        }],
        height: 1.0,
        radius: 0.5,
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: dereth_assets::common::Sphere {
            center: dereth_primitives::Vec3::new(0.0, 0.0, 0.5),
            radius: 1.0,
        },
        selection_sphere: dereth_assets::common::Sphere {
            center: dereth_primitives::Vec3::new(0.0, 0.0, 0.5),
            radius: 1.0,
        },
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_spell_table(table)
        .with_spell_components_table(comps)
        .with_portal(0x2700_0002, mapper)
        .with_portal(land::TEST_SETUP, setup)
        .with_xp_table(empyrean_dat::fake::sample::xp_table())
        .build()
        .expect("fake dats")
}

fn content() -> MemContent {
    content_with(None)
}

/// The test content, with `projectile` (when given) as Flame Bolt I's projectile weenie.
fn content_with(projectile: Option<ContentWeenie>) -> MemContent {
    let mut content = MemContent::new().weenie(
        ContentWeenie::new(PROJ_WCID, "flamebolt", WeenieType::ProjectileSpell)
            .with_string(PropertyString::Name, "Flame Bolt")
            .with_did(PropertyDataId::Setup, land::TEST_SETUP)
            .with_float(PropertyFloat::MaximumVelocity, 20.0),
    );
    let flame_bolt_wcid = projectile.as_ref().map(|p| p.class_id);
    if let Some(p) = projectile {
        content = content.weenie(p);
    }
    for d in defs() {
        let db = if d.id == FLAME_BOLT && flame_bolt_wcid.is_some() {
            DbSpell {
                wcid: flame_bolt_wcid,
                ..d.db
            }
        } else {
            d.db
        };
        content = content.spell(DbSpell {
            id: d.id,
            name: d.name.into(),
            ..db
        });
    }
    content = content.weenie(
        ContentWeenie::new(CORPSE_WCID, "corpse", WeenieType::Corpse)
            .with_string(PropertyString::Name, "Corpse")
            .with_did(PropertyDataId::Setup, land::TEST_SETUP),
    );
    for (_, wcid) in COMPONENT_WCIDS {
        content = content.weenie(
            ContentWeenie::new(wcid, "component", WeenieType::SpellComponent)
                .with_string(PropertyString::Name, "Component"),
        );
    }
    content
}

struct H {
    w: World,
    clock: VirtualClock,
}

impl H {
    fn new() -> Self {
        Self::with_dats(dats())
    }

    fn with_dats(dats: Arc<empyrean_dat::DatManager>) -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let mut w = World::new(now, dats);
        w.content = Arc::new(content());
        w.timers = timers;
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 10);
        pm::initialize(&mut w, true);
        empyrean_world::world_objects::world_object_magic::clear_spell_cache();
        ThreadSafeRandom::seed(u64::from(SEED.unsigned_abs()));
        H { w, clock }
    }

    /// One world pass: the delay manager, the player's action queue, then `LandblockManager.Tick`.
    fn tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        delay_manager::run_actions(&mut self.w);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    fn advance(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
    }

    /// Ticks every 50 ms for `secs` seconds, stopping early when `until` holds.
    fn run_until(&mut self, secs: f64, mut until: impl FnMut(&Self) -> bool) -> bool {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few hundred steps
        let steps = (secs / 0.05).round() as u32;
        for _ in 0..steps {
            self.advance(0.05);
            self.tick();
            if until(self) {
                return true;
            }
        }
        false
    }

    fn o(&self, g: ObjectGuid) -> &WorldObject {
        self.w.objects.get(g).expect("object in store")
    }

    /// A creature as its constructor leaves it (vitals 100, attributes 60, the damage history, the
    /// motion state), at `x, y`. Not placed.
    fn creature(&mut self, class: Class, guid: ObjectGuid, name: &str, x: f32, y: f32) {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.weenie_type = WeenieType::Creature;
        o.biota.properties_enchantment_registry = Some(Vec::new());
        let vitals = [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ];
        let mut v2 = DotNetDict::new();
        for v in vitals {
            v2.insert(
                v,
                PropertiesAttribute2nd {
                    init_level: 100,
                    level_from_cp: 0,
                    cp_spent: 0,
                    current_level: 100,
                },
            );
        }
        o.biota.properties_attribute_2nd = Some(v2);
        for v in vitals {
            let cv = CreatureVital::new(&mut o, v);
            o.vitals_mut().insert(v, cv);
        }
        let attributes = [
            PropertyAttribute::Strength,
            PropertyAttribute::Endurance,
            PropertyAttribute::Coordination,
            PropertyAttribute::Quickness,
            PropertyAttribute::Focus,
            PropertyAttribute::Self_,
        ];
        let mut a1 = DotNetDict::new();
        for a in attributes {
            a1.insert(
                a,
                PropertiesAttribute {
                    init_level: 60,
                    level_from_cp: 0,
                    cp_spent: 0,
                },
            );
        }
        o.biota.properties_attribute = Some(a1);
        for a in attributes {
            let ca = CreatureAttribute::new(&mut o, a);
            o.attributes_mut().insert(a, ca);
        }
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
        o.set_property(PropertyBool::Attackable, true);
        o.creature.as_mut().unwrap().creature_death.damage_history =
            DamageHistory::new(guid, self.w.now.utc);
        let pos = Position::from_components(LB | 0x0001, x, y, 20.0, 0.0, 0.0, 0.0, 1.0, false);
        o.set_location(Some(pos));
        o.set_position(PositionType::Home, Some(pos));
        o.wo.world_object_properties.current_motion_state =
            Some(Motion::new(MotionStance::NonCombat, Mc::Ready, 1.0));
        o.set_heartbeat_interval(Some(0.0));
        empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
            &mut o,
            self.w.now.unix_time,
        );
        self.w.objects.insert(o).expect("fresh guid");
    }

    /// The caster, facing north at `x, y`: in magic combat mode, with a session, the magic skills
    /// (trained, 200; CreatureEnchantment 100 at `ce`), the test spells known, a pack.
    fn player(&mut self, x: f32, y: f32) {
        self.creature(Class::Player, PLAYER, "Caster", x, y);
        let o = self.w.objects.get_mut(PLAYER).unwrap();
        o.player.as_mut().expect("a player").player.character =
            Some(empyrean_store::models::shard::Character::default());
        let mut skills = DotNetDict::new();
        for (s, level) in [
            (Skill::WarMagic, 200),
            (Skill::CreatureEnchantment, 200),
            (Skill::LifeMagic, 200),
        ] {
            skills.insert(
                s,
                PropertiesSkill {
                    init_level: level,
                    sac: SkillAdvancementClass::Trained,
                    ..PropertiesSkill::default()
                },
            );
        }
        o.biota.properties_skill = Some(skills);
        let mut book = DotNetDict::new();
        for d in defs() {
            book.insert(i32::try_from(d.id).unwrap(), 2.0f32);
        }
        o.biota.properties_spell_book = Some(book);
        o.set_property(PropertyInt::ItemsCapacity, 102);
        o.set_property(PropertyInt::Level, 1);
        o.set_property(empyrean_entity::enums::PropertyInt64::TotalExperience, 0);
        o.set_property(
            empyrean_entity::enums::PropertyInt64::AvailableExperience,
            0,
        );
        o.wo.world_object_properties.current_motion_state =
            Some(Motion::new(MotionStance::Magic, Mc::Ready, 1.0));
        self.w.sessions.insert(
            SESSION,
            empyrean_world::sessions::SessionData {
                state: empyrean_net::SessionState::WorldConnected,
                player: Some(PLAYER),
                account: Some("tester".to_owned()),
                ..empyrean_world::sessions::SessionData::default()
            },
        );
        assert!(
            lm::add_object(&mut self.w, PLAYER, false),
            "the player joins its landblock"
        );
        phys_ext::set_physics_state(&mut self.w, PLAYER, PhysicsState::Hidden, Some(false));
        creature_combat::set_combat_mode_field(&mut self.w, PLAYER, CombatMode::Magic);
    }

    /// A drudge (a plain creature, no monster AI) placed at `x, y`.
    fn target(&mut self, x: f32, y: f32) {
        self.creature(Class::Creature, MONSTER, "Drudge", x, y);
        assert!(
            lm::add_object(&mut self.w, MONSTER, false),
            "the drudge joins its landblock"
        );
    }

    fn set_skill(&mut self, skill: Skill, level: u32) {
        let o = self.w.objects.get_mut(PLAYER).unwrap();
        o.biota.properties_skill.as_mut().unwrap().insert(
            skill,
            PropertiesSkill {
                init_level: level,
                sac: SkillAdvancementClass::Trained,
                ..PropertiesSkill::default()
            },
        );
    }

    fn vital(&self, g: ObjectGuid, v: PropertyAttribute2nd) -> u32 {
        let o = self.o(g);
        o.vitals().get(&v).expect("vital").current(o)
    }

    /// One of each formula component in the player's pack (weenie class ids from the mapper).
    fn give_components(&mut self) -> Vec<ObjectGuid> {
        let mut items = Vec::new();
        for (_, wcid) in COMPONENT_WCIDS {
            let weenie = self
                .w
                .content
                .get_cached_weenie(wcid)
                .expect("component weenie");
            let guid = gm::new_dynamic_guid(&mut self.w);
            let o =
                empyrean_world::world_objects::world_object::CtorEnv::with_world(&self.w, |env| {
                    empyrean_world::factories::world_object_factory::create_world_object(
                        env,
                        Some(weenie),
                        guid,
                    )
                })
                .expect("constructible");
            self.w.objects.insert(o).expect("fresh guid");
            assert!(
                container::try_add_to_inventory(&mut self.w, PLAYER, guid, 0, false, false),
                "the pack takes the component"
            );
            items.push(guid);
        }
        items
    }
}

/// One captured send: the opcode, the game event type (0 if none), the chat text (a system chat)
/// and the event's first argument (a UseDone's or WeenieError's error).
#[derive(Debug, Clone)]
struct Sent {
    op: u32,
    ev: u32,
    text: Option<String>,
    arg: Option<u32>,
}

fn sent() -> Vec<Sent> {
    take_sent()
        .into_iter()
        .map(|(_, _, b)| {
            let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
            let ev = if op == 0xF7B0 {
                u32::from_le_bytes(b[12..16].try_into().unwrap())
            } else {
                0
            };
            let arg = (op == 0xF7B0 && b.len() >= 20)
                .then(|| u32::from_le_bytes(b[16..20].try_into().unwrap()));
            let text = (op == 0xF7E0).then(|| {
                let len = usize::from(u16::from_le_bytes(b[4..6].try_into().unwrap()));
                String::from_utf8_lossy(&b[6..6 + len]).into_owned()
            });
            Sent { op, ev, text, arg }
        })
        .collect()
}

/// The first arguments of the `event` game events in the sends.
fn events(sends: &[Sent], event: u32) -> Vec<u32> {
    sends
        .iter()
        .filter(|s| s.ev == event)
        .map(|s| s.arg.unwrap_or(u32::MAX))
        .collect()
}

const EV_UPDATE_ENCHANTMENT: u32 = 0x02C2;
const EV_WEENIE_ERROR: u32 = 0x028A;
const EV_USE_DONE: u32 = 0x01C7;
const PLAY_EFFECT: u32 = 0xF755;

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

/// A recognized research formula follows target checks before learning. A successful cast
/// acknowledges both newly learned and already-known spells; a refusal teaches nothing.
/// Divergence: V432
#[test]
fn research_teaches_only_after_a_successful_cast_and_acknowledges_known_success() {
    use empyrean_world::world_objects::spell_research::handle_test_spell_formula;
    let mut definition = defs().remove(0);
    definition.flags = BENEFICIAL;
    let mut h = H::with_dats(dats_with(vec![definition], ItemType::Creature.0));
    let mut features = h.w.era.features;
    features.spell_research = true;
    h.w.era = empyrean_common::era::with_features(h.w.era, features);
    h.player(100.0, 100.0);
    let items = h.give_components();
    assert!(player_spells::remove_known_spell(
        &mut h.w,
        PLAYER,
        STRENGTH_SELF
    ));
    start_capture();
    handle_test_spell_formula(&mut h.w, PLAYER, &FORMULA, items[0].full());
    assert!(!player_spells::spell_is_known(&h.w, PLAYER, STRENGTH_SELF));
    assert!(
        events(&sent(), 0x02C1).is_empty(),
        "wrong target must not teach"
    );
    assert!(!player_magic::fields(&h.w, PLAYER).magic_state.is_casting);

    for was_known in [false, true] {
        // The first successful cast may consume formula components.
        if was_known {
            let _ = h.give_components();
        }
        start_capture();
        handle_test_spell_formula(&mut h.w, PLAYER, &FORMULA, PLAYER.full());
        assert_eq!(
            player_spells::spell_is_known(&h.w, PLAYER, STRENGTH_SELF),
            was_known,
            "starting the test does not teach before the cast succeeds"
        );
        assert!(h.run_until(2.0, |h| !player_magic::is_busy(&h.w, PLAYER)));
        assert!(player_spells::spell_is_known(&h.w, PLAYER, STRENGTH_SELF));
        let got = sent();
        assert_eq!(events(&got, 0x02C1), [STRENGTH_SELF], "{got:?}");
        assert_eq!(events(&got, EV_USE_DONE), [0], "{got:?}");
        assert_eq!(
            player_magic::fields(&h.w, PLAYER)
                .magic_state
                .research_spell,
            None
        );
    }
}

// ------------------------------------------------------------------------------------ scenarios

/// A self buff (`HandleActionCastTargetedSpell` at yourself): the target is `Self`, so no turn;
/// `CreatePlayerSpell` validates, rolls the fizzle (skill 200 vs power 10: success), pays the base
/// mana, and the chain (windup, cast gesture, `DoCastSpell`) lands the enchantment through 5.1a's
/// `HandleCastSpell`: the registry holds it and the client gets `MagicUpdateEnchantment`. The
/// recoil (`FinishCast`) sends UseDone a second later.
#[test]
fn a_self_buff_lands_as_an_enchantment_and_the_client_gets_the_registry_message() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    let _ = h.give_components();
    start_capture();
    let mana_before = h.vital(PLAYER, PropertyAttribute2nd::MaxMana);

    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        PLAYER.full(),
        STRENGTH_SELF,
        None,
    );
    assert!(
        player_magic::fields(&h.w, PLAYER).magic_state.is_casting,
        "OnCastStart"
    );
    assert!(player_magic::is_busy(&h.w, PLAYER));

    assert!(
        h.run_until(0.5, |h| registry_ids(&h.w, PLAYER)
            == [i32::try_from(STRENGTH_SELF).unwrap()]),
        "the buff lands"
    );
    let got = sent();
    assert_eq!(
        events(&got, EV_UPDATE_ENCHANTMENT).len(),
        1,
        "the registry message: {got:?}"
    );
    assert!(
        got.iter()
            .any(|s| s.text.as_deref() == Some("You cast Strength Self I on yourself")),
        "{got:?}"
    );
    assert_eq!(
        h.vital(PLAYER, PropertyAttribute2nd::MaxMana),
        mana_before - 10,
        "the base mana"
    );
    assert!(
        !player_magic::fields(&h.w, PLAYER).magic_state.is_casting,
        "OnCastDone"
    );
    assert!(
        player_magic::is_busy(&h.w, PLAYER),
        "busy through the recoil"
    );
    h.run_until(0.5, |_| false);
    assert!(
        player_magic::is_busy(&h.w, PLAYER),
        "the recoil lasts a second"
    );

    assert!(
        h.run_until(1.5, |h| !player_magic::is_busy(&h.w, PLAYER)),
        "the recoil ends"
    );
    assert_eq!(
        events(&sent(), EV_USE_DONE).len(),
        1,
        "UseDone after the recoil"
    );
    assert_eq!(
        player_magic::fields(&h.w, PLAYER).last_success_cast_school,
        MagicSchool::CreatureEnchantment
    );
}

/// A fizzle (skill 100 vs power 100: chance 0.5, and this seed's first draw is above it): the cast
/// still runs its gestures, then `DoCastSpell_Inner` pays the fizzle's 5 mana, rolls every
/// component for burning (`Spell.TryBurnComponents`: 0.5 each here), consumes the burned ones from
/// the pack (last first) and says which, then plays Fizzle and sends YourSpellFizzled. The draws
/// are pinned by replaying the seed: one for the fizzle, then one per component.
#[test]
fn a_fizzle_burns_components_per_aces_roll() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.set_skill(Skill::CreatureEnchantment, 100);
    // no attribute formula: the current skill is its init level (so the burn rate's skill mod is 1)
    for a in [PropertyAttribute::Focus, PropertyAttribute::Self_] {
        h.w.objects
            .get_mut(PLAYER)
            .unwrap()
            .biota
            .properties_attribute
            .as_mut()
            .unwrap()
            .insert(a, PropertiesAttribute::default());
    }
    let skill =
        h.w.objects
            .get_mut(PLAYER)
            .unwrap()
            .get_creature_skill(Skill::CreatureEnchantment, false)
            .unwrap();
    assert_eq!(skill.current(&mut h.w, PLAYER), 100);
    let items = h.give_components();
    let _ = take_local();
    start_capture();
    let mana_before = h.vital(PLAYER, PropertyAttribute2nd::MaxMana);

    // the first seed whose first draw fizzles (chance 0.5 at skill = power)
    let chance = empyrean_world::world_objects::skill_check::get_magic_skill_chance(100, 100);
    let seed = (1..)
        .find(|&s| DotNetRandom::new(s).next_double() >= chance)
        .expect("a fizzling seed");
    ThreadSafeRandom::seed(u64::from(seed.unsigned_abs()));

    // the reference generator: the fizzle roll, then one burn roll per component
    let mut r = DotNetRandom::new(seed);
    let fizzle_roll = r.next_double();
    assert!(
        chance <= fizzle_roll,
        "the seed fizzles (chance {chance}, roll {fizzle_roll})"
    );
    let burned: Vec<bool> = (0..FORMULA.len()).map(|_| r.next_double() < 0.5).collect();
    let next_after = r.next_double();

    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        PLAYER.full(),
        STRENGTH_SELF_2,
        None,
    );
    assert!(
        h.run_until(0.5, |h| !player_magic::fields(&h.w, PLAYER)
            .magic_state
            .is_casting),
        "the cast ends"
    );

    let got = sent();
    assert!(registry_ids(&h.w, PLAYER).is_empty(), "nothing lands");
    assert_eq!(
        events(&got, EV_WEENIE_ERROR).len(),
        1,
        "YourSpellFizzled: {got:?}"
    );
    assert_eq!(
        h.vital(PLAYER, PropertyAttribute2nd::MaxMana),
        mana_before - 5,
        "a fizzle costs 5 mana"
    );

    // the player's formula is the account's shuffle of the base one; consumed in formula order
    let mut spell = Spell::new(&h.w, STRENGTH_SELF_2, true);
    let formula = spell
        .formula
        .as_mut()
        .unwrap()
        .get_player_formula(&h.w, PLAYER);
    let current = spell
        .formula
        .as_ref()
        .unwrap()
        .current_formula
        .clone()
        .unwrap_or(formula);
    let names: BTreeMap<u32, &str> = BTreeMap::from([
        (1, "Lead Scarab"),
        (10, "Hyssop"),
        (20, "Powdered Agate"),
        (30, "Stibnite"),
        (40, "Poplar Talisman"),
    ]);
    let want: Vec<&str> = current
        .iter()
        .zip(&burned)
        .filter(|(_, &b)| b)
        .map(|(c, _)| names[c])
        .collect();
    assert!(!want.is_empty(), "the seed burns something: {burned:?}");
    let msg = format!(
        "The spell consumed the following components: {}",
        want.join(", ")
    );
    assert!(
        got.iter().any(|s| s.text.as_deref() == Some(msg.as_str())),
        "{msg} in {got:?}"
    );

    // the burned components left the pack, the rest are still there
    let wcid_of = |c: u32| COMPONENT_WCIDS.iter().find(|(id, _)| *id == c).unwrap().1;
    for (c, &b) in current.iter().zip(&burned) {
        let left = container::get_num_inventory_items_of_wcid(&h.w, PLAYER, wcid_of(*c));
        assert_eq!(left, i32::from(!b), "component {c}");
    }
    assert_eq!(items.len(), 5);

    // nothing else drew from ThreadSafeRandom in between
    assert_eq!(
        ThreadSafeRandom::next_float(0.0, 1.0).to_bits(),
        next_after.to_bits(),
        "the draw order"
    );
}

/// A war bolt at a drudge 10 m north: the player turns (`Rotate`), `CreatePlayerSpell` runs the
/// chain, `DoCastSpell` checks the angle and range, and `HandleCastSpell` launches the projectile
/// (5.1a's `LaunchSpellProjectiles` over `SpellProjectile.Setup`). The projectile flies on the
/// server's physics, collides with the drudge, and `OnCollideObject` rolls and applies the damage
/// (a fire bolt: 30, or 45 on a critical); a second, stronger bolt kills it.
#[test]
fn a_war_bolt_flies_hits_the_target_and_a_second_one_kills_it() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.target(100.0, 110.0);
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_spell_components_required(false);
    let _ = take_local();
    start_capture();

    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        MONSTER.full(),
        FLAME_BOLT,
        None,
    );

    let projectiles = |w: &World| {
        let lb = w
            .landblock_manager
            .landblocks
            .get(LandblockId::new(LB | 0xFFFF))
            .expect("the landblock");
        lb.get_all_world_objects_for_diagnostics()
            .into_iter()
            .filter(|&g| {
                w.objects
                    .get(g)
                    .is_some_and(WorldObject::is_spell_projectile)
            })
            .collect::<Vec<_>>()
    };
    assert!(
        h.run_until(1.0, |h| !projectiles(&h.w).is_empty()),
        "a projectile is launched"
    );
    let sp = projectiles(&h.w)[0];
    let spf = spell_projectile::fields(&h.w, sp);
    assert_eq!(spf.spell_type, ProjectileSpellType::Bolt);
    assert_eq!(spf.spell.as_ref().map(Spell::id), Some(FLAME_BOLT));
    let links = h.o(sp).projectile.unwrap();
    assert_eq!((links.source, links.target), (Some(PLAYER), Some(MONSTER)));
    let body = h.o(sp).phys.expect("a physics body");
    let v = phys_ext::velocity(&h.w, body);
    assert!(
        (v.y - 20.0).abs() < 0.5 && v.x.abs() < 0.5,
        "it flies north at the weenie's MaximumVelocity: {v:?}"
    );
    assert!(
        phys_ext::get_physics_state(&h.w, sp, PhysicsState::Missile),
        "a missile"
    );

    // the flight: 10 m at 20 m/s
    assert!(
        h.run_until(2.0, |h| h.vital(MONSTER, PropertyAttribute2nd::MaxHealth)
            < 100),
        "the bolt hits"
    );
    let health = h.vital(MONSTER, PropertyAttribute2nd::MaxHealth);
    // war magic's skill bonus: MinDamage * (skill - power) / 1000 on top of the 30 (and +15, half
    // the max damage, on a critical); no ratings, resistances or absorption here
    let skill =
        h.w.objects
            .get_mut(PLAYER)
            .unwrap()
            .get_creature_skill(Skill::WarMagic, false)
            .unwrap()
            .current(&mut h.w, PLAYER);
    #[allow(clippy::cast_precision_loss)]
    let bonus = 30.0f32 * ((skill - 10) as f32 / 1000.0);
    let normal = empyrean_common::dotnet::math::round(f64::from(30.0f32 + bonus));
    let critical = empyrean_common::dotnet::math::round(f64::from(30.0f32 + 15.0 + bonus));
    let dealt = f64::from(100 - health);
    assert!(
        dealt == normal || dealt == critical,
        "{dealt}: {normal} (or {critical} on a critical) at skill {skill}"
    );
    let got = sent();
    let amount = 100 - health;
    // Strings.GetAttackVerb(Fire, amount / MaxHealth)
    #[allow(clippy::cast_precision_loss)]
    let (verb, _) = empyrean_world::entity::strings::get_attack_verb(
        empyrean_entity::enums::DamageType::Fire,
        amount as f32 / 100.0,
    )
    .unwrap();
    let chat = format!("You {verb} Drudge for {amount} points with Flame Bolt I.");
    assert!(
        got.iter()
            .filter_map(|s| s.text.as_deref())
            .any(|t| t.ends_with(&chat)),
        "the attacker's message ({chat}) in {got:?}"
    );
    assert_eq!(
        player_magic::get_current_magic_skill(&h.w, PLAYER),
        Skill::WarMagic,
        "LastHitSpellProjectile"
    );
    assert_eq!(
        player_magic::fields(&h.w, PLAYER)
            .last_hit_spell_projectile
            .as_ref()
            .map(Spell::id),
        Some(FLAME_BOLT)
    );
    assert!(
        !phys_ext::get_physics_state(&h.w, sp, PhysicsState::ReportCollisions),
        "the impact"
    );
    assert!(
        phys_ext::get_physics_state(&h.w, sp, PhysicsState::Ethereal),
        "the impact"
    );
    assert!(
        got.iter().any(|s| s.op == PLAY_EFFECT),
        "the explosion script"
    );

    // after the recoil, a killing bolt
    assert!(
        h.run_until(2.0, |h| !player_magic::is_busy(&h.w, PLAYER)),
        "the recoil ends"
    );
    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        MONSTER.full(),
        FORCE_BOLT,
        None,
    );
    assert!(
        h.run_until(3.0, |h| h
            .w
            .objects
            .get(MONSTER)
            .is_none_or(|o| o.health().current(o) == 0)),
        "the drudge dies"
    );
    assert!(
        h.run_until(7.0, |h| h.w.objects.get(sp).is_none()),
        "the first projectile is destroyed 5 s after its impact"
    );
}

/// An invincible creature, not only an invincible player, takes no damage from a spell
/// projectile: neither the hit (`CalculateDamage`) nor `DamageTarget` called on it directly (ACE
/// c5f16b44; before it, ACE tested players only).
#[test]
fn a_war_bolt_does_not_hurt_an_invincible_creature() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.target(100.0, 110.0);
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_spell_components_required(false);
    h.w.objects.get_mut(MONSTER).unwrap().set_invincible(true);
    let _ = take_local();

    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        MONSTER.full(),
        FLAME_BOLT,
        None,
    );
    let projectile = |w: &World| {
        let lb = w
            .landblock_manager
            .landblocks
            .get(LandblockId::new(LB | 0xFFFF))
            .expect("the landblock");
        lb.get_all_world_objects_for_diagnostics()
            .into_iter()
            .find(|&g| {
                w.objects
                    .get(g)
                    .is_some_and(WorldObject::is_spell_projectile)
            })
    };
    assert!(
        h.run_until(1.0, |h| projectile(&h.w).is_some()),
        "a projectile is launched"
    );
    let sp = projectile(&h.w).unwrap();

    spell_projectile::damage_target(&mut h.w, sp, MONSTER, 30.0, false, false, false);
    assert_eq!(
        h.vital(MONSTER, PropertyAttribute2nd::MaxHealth),
        100,
        "DamageTarget"
    );

    assert!(
        h.run_until(2.0, |h| !phys_ext::get_physics_state(
            &h.w,
            sp,
            PhysicsState::ReportCollisions
        )),
        "the bolt hits"
    );
    assert_eq!(
        h.vital(MONSTER, PropertyAttribute2nd::MaxHealth),
        100,
        "the hit"
    );
}

/// V272: a drain bolt's chat lines report the amount the vital actually lost,
/// not the roll. In the retail captures a Mana Blast that drained a constant 241 said 231 and 229
/// when it emptied the pool, and about 170 drain lines equal the vital's last value, down to
/// "drains 0". ACE printed the rolled damage.
#[test]
fn a_drain_bolt_that_empties_the_pool_reports_what_it_took() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.target(100.0, 110.0);
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_spell_components_required(false);
    let mana = h.o(MONSTER).mana();
    let _ = empyrean_world::world_objects::creature_vitals::update_vital_delta(
        &mut h.w, MONSTER, mana, -90,
    );
    assert_eq!(h.vital(MONSTER, PropertyAttribute2nd::MaxMana), 10);
    let _ = take_local();
    start_capture();

    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        MONSTER.full(),
        MANA_BOLT,
        None,
    );
    assert!(
        h.run_until(3.0, |h| h.vital(MONSTER, PropertyAttribute2nd::MaxMana)
            < 10),
        "the bolt hits"
    );
    assert_eq!(
        h.vital(MONSTER, PropertyAttribute2nd::MaxMana),
        0,
        "the roll (30 or more) empties the pool"
    );

    let got = sent();
    let chat = "With Mana Bolt you drain 10 points of mana from Drudge.";
    assert!(
        got.iter()
            .filter_map(|s| s.text.as_deref())
            .any(|t| t == chat),
        "the attacker's message ({chat}) in {got:?}"
    );
}

/// The range check (`VerifySpellRange`): init + ranks of the school's skill, not the current
/// skill; a target beyond `BaseRangeConstant + skill * BaseRangeMod` (capped at 75) is refused
/// with MissileOutOfRange before any mana is paid.
#[test]
fn a_target_out_of_range_is_refused() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.set_skill(Skill::WarMagic, 10); // 5 + 10 * 1 = 15 m
    h.target(100.0, 130.0);
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_spell_components_required(false);
    start_capture();
    let mana_before = h.vital(PLAYER, PropertyAttribute2nd::MaxMana);

    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        MONSTER.full(),
        FLAME_BOLT,
        None,
    );
    h.run_until(0.5, |_| false);

    let got = sent();
    assert_eq!(
        events(&got, EV_USE_DONE),
        [WeenieError::MissileOutOfRange.0.cast_unsigned()],
        "{got:?}"
    );
    assert_eq!(h.vital(PLAYER, PropertyAttribute2nd::MaxMana), mana_before);
    assert!(!player_magic::fields(&h.w, PLAYER).magic_state.is_casting);
    assert!(
        !player_magic::is_busy(&h.w, PLAYER),
        "OnCastDone clears IsBusy"
    );

    // the range comes from init + ranks, whatever skill the caller passes
    let spell = Spell::new(&h.w, FLAME_BOLT, true);
    let category = player_magic::TargetCategory::WorldObject;
    assert!(!player_magic::verify_spell_range(
        &mut h.w, PLAYER, MONSTER, category, &spell, None, 1000
    ));
    h.set_skill(Skill::WarMagic, 30); // 5 + 30 = 35 m
    assert!(player_magic::verify_spell_range(
        &mut h.w, PLAYER, MONSTER, category, &spell, None, 0
    ));
}

/// The request checks, in ACE's order: out of magic mode (and never in it) answers UseDone and
/// nothing else; an unknown spell is MagicInvalidSpellType; a missing target TargetNotAcquired; a
/// busy player YoureTooBusy (or, with `spellcast_recoil_queue`, a queued cast).
#[test]
fn a_cast_request_is_refused_in_aces_order() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_spell_components_required(false);

    creature_combat::set_combat_mode_field(&mut h.w, PLAYER, CombatMode::NonCombat);
    start_capture();
    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        PLAYER.full(),
        STRENGTH_SELF,
        None,
    );
    assert_eq!(events(&sent(), EV_USE_DONE).len(), 1);
    assert!(!player_magic::fields(&h.w, PLAYER).magic_state.is_casting);
    creature_combat::set_combat_mode_field(&mut h.w, PLAYER, CombatMode::Magic);

    // not in the spellbook
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .biota
        .properties_spell_book
        .as_mut()
        .unwrap()
        .remove(&i32::try_from(FLAME_BOLT).unwrap());
    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        PLAYER.full(),
        FLAME_BOLT,
        None,
    );
    assert_eq!(events(&sent(), EV_USE_DONE).len(), 1);

    // no such target
    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        0x8000_7777,
        STRENGTH_SELF,
        None,
    );
    assert_eq!(events(&sent(), EV_USE_DONE).len(), 1);
    assert!(!player_magic::fields(&h.w, PLAYER).magic_state.is_casting);

    // busy
    player_magic::set_is_busy(&mut h.w, PLAYER, true);
    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        PLAYER.full(),
        STRENGTH_SELF,
        None,
    );
    assert_eq!(events(&sent(), EV_USE_DONE).len(), 1, "YoureTooBusy");
    player_magic::fields_mut(&mut h.w, PLAYER)
        .magic_state
        .can_queue = true;
    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        PLAYER.full(),
        STRENGTH_SELF,
        None,
    );
    assert!(sent().is_empty(), "queued silently");
    let q = player_magic::fields(&h.w, PLAYER)
        .magic_state
        .cast_queue
        .expect("queued");
    assert_eq!((q.target_guid, q.spell_id), (PLAYER.full(), STRENGTH_SELF));
    assert!(!player_magic::fields(&h.w, PLAYER).magic_state.can_queue);
}

/// `GetCastingPreCheckStatus`'s war/void switch: casting void within the time limit (a draw of
/// 3..5 s) after a successful war cast fails with ACE's message.
#[test]
fn switching_from_war_to_void_within_the_limit_fails_with_the_message() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    start_capture();
    let now = h.w.now.unix_time;
    let f = player_magic::fields_mut(&mut h.w, PLAYER);
    f.last_success_cast_school = MagicSchool::WarMagic;
    f.last_success_cast_time = now - 1.0;
    let spell = power_spell(MagicSchool::VoidMagic.0, 10);
    let status = player_magic::get_casting_pre_check_status(&mut h.w, PLAYER, &spell, 200, false);
    assert_eq!(status, CastingPreCheckStatus::CastFailed);
    let got = sent();
    assert_eq!(
        got.iter()
            .filter_map(|s| s.text.clone())
            .collect::<Vec<_>>(),
        ["The Elemental energies permeating your blood cause this Void magic to fail."]
    );
}

/// `VerifyNonComponentTargetType`: ACE's widened rules (a creature passes the item types), the
/// portal-spell split, and the untargeted case.
#[test]
fn verify_non_component_target_type_follows_ace() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.target(100.0, 110.0);
    let mut spell = Spell::new(&h.w, STRENGTH_SELF, true);
    let set_type = |spell: &mut Spell, t: ItemType| {
        let mut base = spell.spell_base.clone().unwrap();
        base.non_component_target_type = t.0;
        spell.spell_base = Some(base);
    };
    for (t, creature_ok) in [
        (ItemType::Creature, true),
        (ItemType::Vestements, true),
        (ItemType::Weapon, true),
        (ItemType::Caster, true),
        (ItemType::WeaponOrCaster, true),
        (ItemType::Portal, false),
        (ItemType::LockableMagicTarget, false),
        (ItemType::Item, false),
        (ItemType::LifeStone, false),
        (ItemType::Armor, false),
    ] {
        set_type(&mut spell, t);
        assert_eq!(
            player_magic::verify_non_component_target_type(&h.w, &spell, Some(MONSTER)),
            creature_ok,
            "{t:?}"
        );
    }
    set_type(&mut spell, ItemType::None);
    assert!(
        player_magic::verify_non_component_target_type(&h.w, &spell, None),
        "untargeted"
    );
    set_type(&mut spell, ItemType::Creature);
    assert!(!player_magic::verify_non_component_target_type(
        &h.w, &spell, None
    ));
}

/// The spellbook: learning sends `MagicUpdateSpell`, the chat line and the effect; relearning says
/// "You already know that spell!"; an id not in the table is refused; removal sends
/// `MagicRemoveSpell` and an unknown id is only logged.
#[test]
fn learning_and_removing_spells() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .biota
        .properties_spell_book
        .as_mut()
        .unwrap()
        .remove(&i32::try_from(FLAME_BOLT).unwrap());
    start_capture();

    player_spells::learn_spell_with_networking(&mut h.w, PLAYER, FLAME_BOLT, true);
    assert!(player_spells::spell_is_known(&h.w, PLAYER, FLAME_BOLT));
    let got = sent();
    assert_eq!(events(&got, 0x02C1).len(), 1, "MagicUpdateSpell: {got:?}");
    assert!(got
        .iter()
        .any(|s| s.text.as_deref() == Some("You learn the Flame Bolt I spell.\n")));
    assert!(h.o(PLAYER).wo.world_object_database.changes_detected);

    player_spells::learn_spell_with_networking(&mut h.w, PLAYER, FLAME_BOLT, true);
    assert_eq!(
        sent()
            .into_iter()
            .filter_map(|s| s.text)
            .collect::<Vec<_>>(),
        ["You already know that spell!"]
    );
    player_spells::learn_spell_with_networking(&mut h.w, PLAYER, 999, true);
    assert_eq!(
        sent()
            .into_iter()
            .filter_map(|s| s.text)
            .collect::<Vec<_>>(),
        ["SpellID not found in Spell Table"]
    );

    player_spells::handle_action_magic_remove_spell_id(&mut h.w, PLAYER, FLAME_BOLT);
    assert!(!player_spells::spell_is_known(&h.w, PLAYER, FLAME_BOLT));
    assert_eq!(events(&sent(), 0x01A8).len(), 1, "MagicRemoveSpell");
    player_spells::handle_action_magic_remove_spell_id(&mut h.w, PLAYER, FLAME_BOLT);
    assert!(sent().is_empty());
}

/// The spellbook filters and the component fill levels (`Character`), with ACE's validity check.
#[test]
fn spellbook_filters_and_component_levels() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    player_spells::handle_spellbook_filters(
        &mut h.w,
        PLAYER,
        empyrean_entity::enums::SpellBookFilterOptions(0x55),
    );
    let character = |h: &H| {
        h.o(PLAYER)
            .player
            .as_ref()
            .unwrap()
            .player
            .character
            .clone()
            .unwrap()
    };
    assert_eq!(character(&h).spellbook_filters, 0x55);

    player_spells::handle_set_desired_component_level(&mut h.w, PLAYER, 774, 25);
    player_spells::handle_set_desired_component_level(&mut h.w, PLAYER, 774, 30);
    player_spells::handle_set_desired_component_level(&mut h.w, PLAYER, 12345, 30); // not a component
    let book = character(&h).character_properties_fill_comp_book;
    assert_eq!(
        book.iter()
            .map(|c| (c.spell_component_id, c.quantity_to_rebuy))
            .collect::<Vec<_>>(),
        [(774, 30)]
    );
    player_spells::handle_set_desired_component_level(&mut h.w, PLAYER, 774, 0);
    assert!(character(&h).character_properties_fill_comp_book.is_empty());
    assert!(
        h.o(PLAYER)
            .player
            .as_ref()
            .unwrap()
            .player_database
            .character_changes_detected
    );
}

/// `HasFoci`: the infusion augmentation, else a foci of the school in the pack.
#[test]
fn has_foci_reads_the_augmentation_then_the_pack() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    assert!(!player_spells::has_foci(
        &h.w,
        PLAYER,
        MagicSchool::WarMagic
    ));
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_property(PropertyInt::AugmentationInfusedWarMagic, 1);
    assert!(player_spells::has_foci(&h.w, PLAYER, MagicSchool::WarMagic));
    assert!(!player_spells::has_foci(
        &h.w,
        PLAYER,
        MagicSchool::LifeMagic
    ));
}

/// `AbsorbMagic` (a 25% tome, base magic defense 219): `25% - 25% * 100 * 0.004` = 15% absorbed.
#[test]
fn absorb_magic_scales_with_base_magic_defense() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.set_skill(Skill::MagicDefense, 219);
    let tome = gm::new_dynamic_guid(&mut h.w);
    let mut o = WorldObject::allocate(Class::Caster);
    o.guid = tome;
    o.set_property(PropertyFloat::AbsorbMagicDamage, 0.25);
    h.w.objects.insert(o).unwrap();
    let base = {
        let skill =
            h.w.objects
                .get_mut(PLAYER)
                .unwrap()
                .get_creature_skill(Skill::MagicDefense, true)
                .unwrap();
        skill.base(&h.w, h.o(PLAYER))
    };
    let got = spell_projectile::absorb_magic(&mut h.w, PLAYER, tome);
    #[allow(clippy::cast_possible_truncation)]
    let want =
        (1.0 - (0.25 - 0.25 * f64::from(319u32.saturating_sub(base)) * f64::from(0.004f32))) as f32;
    assert_eq!(got.to_bits(), want.min(1.0).to_bits(), "base {base}");
    assert!(base >= 219);
}

// ----------------------------------------------------------------- V305/V313/V314: in-flight state

/// A projectile weenie as ACE's world DB stores it: its `PhysicsState` and, when `bools` is
/// given, the GravityStatus, LightsStatus, ScriptedCollision and Inelastic bools most projectile
/// weenies store.
fn stored_projectile(wcid: u32, state: u32, bools: Option<[bool; 4]>) -> ContentWeenie {
    let mut w = ContentWeenie::new(wcid, "projectile", WeenieType::ProjectileSpell)
        .with_string(PropertyString::Name, "Projectile")
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
        .with_float(PropertyFloat::MaximumVelocity, 20.0)
        .with_int(PropertyInt::PhysicsState, state.cast_signed())
        .with_bool(PropertyBool::Stuck, true)
        .with_bool(PropertyBool::UiHidden, true);
    if let Some([gravity, lights, scripted, inelastic]) = bools {
        w = w
            .with_bool(PropertyBool::GravityStatus, gravity)
            .with_bool(PropertyBool::LightsStatus, lights)
            .with_bool(PropertyBool::ScriptedCollision, scripted)
            .with_bool(PropertyBool::Inelastic, inelastic);
    }
    w
}

/// The physics state of the CreateObject for a projectile of `weenie`, launched as a
/// `spell_type` spell (Flame Bolt I carrying it), decoded with dereth-protocol.
fn launched_state(weenie: ContentWeenie, spell_type: ProjectileSpellType) -> u32 {
    use dereth_protocol::objects::ItemCreateObject;
    use empyrean_world::network::game_messages::messages::game_message_create_object::game_message_create_object;

    let mut h = H::new();
    h.w.content = Arc::new(content_with(Some(weenie)));
    h.player(100.0, 100.0);
    let spell = Spell::new(&h.w, FLAME_BOLT, true);
    let origins = [empyrean_common::dotnet::Vector3::new(0.0, 1.0, 1.0)];
    let velocity = empyrean_common::dotnet::Vector3::new(0.0, 20.0, 0.0);
    let sps = empyrean_world::world_objects::world_object_magic::launch_spell_projectiles(
        &mut h.w, PLAYER, &spell, None, spell_type, None, false, false, &origins, velocity, 0,
    );
    let sp = *sps.first().expect("a projectile in flight");
    let m = game_message_create_object(&mut h.w, sp, false, false);
    dereth_protocol::read_body_padded::<ItemCreateObject>(&m.data[4..])
        .expect("the CreateObject decodes")
        .0
        .physicsdesc
        .state
}

/// V305/V313/V314 (V313): the nine ring, wave and bomb pieces ACE's world DB stores with their
/// after-impact state (0x120034: Ethereal, IgnoreCollisions, NoDraw, Inelastic, Cloaked) fly in
/// retail's live state, 0x20B48 (Flame Wave 33862 and Acid Bomb 33845 here).
#[test]
fn a_wave_piece_flies_in_retails_live_state() {
    for wcid in [33862, 33845] {
        assert_eq!(
            launched_state(
                stored_projectile(wcid, 0x12_0034, None),
                ProjectileSpellType::Ring
            ),
            0x2_0B48,
            "{wcid}"
        );
    }
}

/// V305/V313/V314 (V313): Dark Vortex's LightsStatus and Deadly Lightning Volley's GravityStatus now agree
/// with their stored states and retail's CreateObjects (0x20348, 0x28F48).
#[test]
fn dark_vortex_and_lightning_volley_fly_as_on_retail() {
    let vortex = stored_projectile(33498, 0x2_0348, Some([false, true, true, true]));
    assert_eq!(launched_state(vortex, ProjectileSpellType::Ring), 0x2_0348);
    let volley = stored_projectile(52621, 0x2_8F48, Some([false, true, true, true]));
    assert_eq!(
        launched_state(volley, ProjectileSpellType::Volley),
        0x2_8F48
    );
}

/// V305/V313/V314 (V313): retail's 43231 flies with Gravity (0x28F48; the Nether Arc spells launch it)
/// and its 43232 without (0x28B48; the Nether Streak spells launch it), whatever the dump's
/// GravityStatus bools say.
#[test]
fn the_nether_projectiles_have_retails_gravity() {
    let arc = stored_projectile(43231, 0x2_0814, Some([false, true, true, true]));
    assert_eq!(launched_state(arc, ProjectileSpellType::Arc), 0x2_8F48);
    let streak = stored_projectile(43232, 0x2_0C14, Some([true, true, true, true]));
    assert_eq!(
        launched_state(streak, ProjectileSpellType::Streak),
        0x2_8B48
    );
}

/// V305/V313/V314 (V314): a ring piece keeps the ScriptedCollision its stored state has (the Mana Cloud
/// 29030, and Ring around the Rabbit 33040 with its state corrected by V313: 0x28B48 on retail);
/// one whose stored state has none (the Flame Ring's 7270) still flies without it, whatever its
/// ScriptedCollision bool says (0x20B48).
#[test]
fn a_ring_piece_keeps_its_stored_scripted_collision() {
    let mana_cloud = stored_projectile(29030, 0x2_8B48, Some([false, true, true, true]));
    assert_eq!(
        launched_state(mana_cloud, ProjectileSpellType::Ring),
        0x2_8B48
    );
    let rabbit = stored_projectile(33040, 0x2_0814, Some([false, true, true, true]));
    assert_eq!(launched_state(rabbit, ProjectileSpellType::Ring), 0x2_8B48);
    let flame_ring = stored_projectile(7270, 0x2_0B48, Some([false, true, true, true]));
    assert_eq!(
        launched_state(flame_ring, ProjectileSpellType::Ring),
        0x2_0B48
    );
}

/// V305/V313/V314 (V314): a projectile whose weenie is stored in flight without AlignPath and does not
/// spin (the Egg 33039, the Snowball 34434, a Dark Nanner 35960) keeps it off, as on retail; one
/// stored in a state that is not a flight state (Nether Bolt 43230) gets it, as ACE gives it.
#[test]
fn a_projectile_stored_without_align_path_keeps_it_off() {
    assert_eq!(
        launched_state(
            stored_projectile(33039, 0x2_8E48, None),
            ProjectileSpellType::Bolt
        ),
        0x2_8E48
    );
    assert_eq!(
        launched_state(
            stored_projectile(34434, 0x2_8A48, None),
            ProjectileSpellType::Bolt
        ),
        0x2_8A48
    );
    assert_eq!(
        launched_state(
            stored_projectile(35960, 0x2_0A48, None),
            ProjectileSpellType::Wall
        ),
        0x2_0A48
    );
    let nether_bolt = stored_projectile(43230, 0x2_0814, Some([false, true, true, true]));
    assert_eq!(
        launched_state(nether_bolt, ProjectileSpellType::Bolt),
        0x2_8B48
    );
}

mod casting_state {
    use crate::support::player_services::*;

    /// `MagicState.IsCasting` is the player's real magic state (it gates the cast motions' broadcast
    /// and `OnMoveComplete_Magic`).
    #[test]
    fn is_casting_reads_the_players_magic_state() {
        let mut h = H::small();
        h.player(A, "Alpha", 3);
        let shim = empyrean_world::world_objects::world_object_networking::shims::player_magic_state_is_casting;
        assert!(!shim(&h.w, A));
        empyrean_world::world_objects::player_magic::fields_mut(&mut h.w, A)
            .magic_state
            .is_casting = true;
        assert!(shim(&h.w, A));
    }
}

mod mana_and_cast_records {
    use crate::support::social_world::*;

    /// `RecordCast` (Entity/RecordCast.cs): `Log` buffers `[timestamp] line` (ACE's
    /// `yyyy-MM-dd hh:mm:ss,fff`); the file is `{Name}Cast.log`.
    #[test]
    fn record_cast_buffers_time_stamped_lines() {
        use empyrean_world::entity::record_cast;
        let mut h = H::small();
        h.player(A, "Alpha", 3);
        assert_eq!(record_cast::filename(&h.w, A), "AlphaCast.log");
        record_cast::log(&mut h.w, A, "MagicState.OnCastDone()");
        let buffer = empyrean_world::world_objects::player_magic::fields(&h.w, A)
            .record_cast
            .buffer
            .clone();
        let stamp = h.w.now.utc.format("yyyy-MM-dd hh:mm:ss,fff");
        assert_eq!(buffer, format!("[{stamp}] MagicState.OnCastDone()\n"));
    }

    /// `Player.ManaConsumersTick` / `CheckLowMana` / `HandleManaDepleted` (Player_Tick.cs): an
    /// affecting item burns `-ManaRate * heartbeat` into its accumulator and loses the whole mana
    /// points; under two minutes left, one "low on Mana" line; at zero, "out of Mana" and the sound,
    /// and the item stops affecting.
    #[test]
    fn equipped_items_burn_mana_on_the_heartbeat() {
        use empyrean_entity::enums::{PropertyFloat, PropertyInt};
        let mut h = H::small();
        let sa = h.player(A, "Alpha", 3);
        let item = ObjectGuid::new(0x8000_0301);
        let mut o = empyrean_world::world_objects::world_object::WorldObject::allocate(
            empyrean_world::dispatch::Class::GenericObject,
        );
        o.guid = item;
        o.set_property(
            empyrean_entity::enums::PropertyString::Name,
            "Ring".to_owned(),
        );
        o.set_property(PropertyInt::ItemCurMana, 30);
        o.set_property(PropertyInt::ItemMaxMana, 100);
        o.set_property(PropertyFloat::ManaRate, -0.1);
        o.set_is_affecting(true);
        h.w.objects.insert(o).unwrap();
        {
            let c = h.w.objects.get_mut(A).unwrap().creature.as_mut().unwrap();
            c.creature_equipment.equipped_objects.insert(item, ());
            c.creature_equipment.equipped_objects_loaded = true;
        }
        h.w.objects
            .get_mut(A)
            .unwrap()
            .wo
            .world_object_tick
            .cached_heartbeat_interval = 5.0;

        // 0.1 * 5 = 0.5: nothing burned yet
        start_capture();
        empyrean_world::world_objects::player_tick::mana_consumers_tick(&mut h.w, A);
        assert_eq!(h.w.objects.get(item).unwrap().item_cur_mana(), Some(30));
        assert!(sent().is_empty());

        // 1.0: one point; 29 / 0.1 = 290 s left, not low
        empyrean_world::world_objects::player_tick::mana_consumers_tick(&mut h.w, A);
        assert_eq!(h.w.objects.get(item).unwrap().item_cur_mana(), Some(29));

        // 11 left: 110 s, low (once)
        h.w.objects
            .get_mut(item)
            .unwrap()
            .set_property(PropertyInt::ItemCurMana, 12);
        start_capture();
        empyrean_world::world_objects::player_tick::mana_consumers_tick(&mut h.w, A);
        empyrean_world::world_objects::player_tick::mana_consumers_tick(&mut h.w, A);
        empyrean_world::world_objects::player_tick::mana_consumers_tick(&mut h.w, A);
        assert_eq!(chats_to(&sent(), sa), ["Your Ring is low on Mana."]);

        h.w.objects
            .get_mut(item)
            .unwrap()
            .set_property(PropertyInt::ItemCurMana, 1);
        start_capture();
        empyrean_world::world_objects::player_tick::mana_consumers_tick(&mut h.w, A);
        empyrean_world::world_objects::player_tick::mana_consumers_tick(&mut h.w, A);
        let msgs = sent();
        assert_eq!(chats_to(&msgs, sa), ["Your Ring is out of Mana."]);
        assert!(!h.w.objects.get(item).unwrap().is_affecting());
    }
}

/// Divergence: V403
/// The same bolt, with the same draws, does half its base damage from a creature (not a player)
/// in an era whose creature projectiles are halved, and the same from a player.
#[test]
fn an_era_halves_a_creatures_war_projectile() {
    let mut h = H::new();
    h.player(100.0, 100.0);
    h.target(100.0, 110.0);
    h.w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_spell_components_required(false);
    let _ = take_local();
    player_magic::handle_action_cast_targeted_spell(
        &mut h.w,
        PLAYER,
        MONSTER.full(),
        FLAME_BOLT,
        None,
    );
    let projectile = |w: &World| {
        let lb = w
            .landblock_manager
            .landblocks
            .get(LandblockId::new(LB | 0xFFFF))
            .expect("the landblock");
        lb.get_all_world_objects_for_diagnostics()
            .into_iter()
            .find(|&g| {
                w.objects
                    .get(g)
                    .is_some_and(WorldObject::is_spell_projectile)
            })
    };
    assert!(h.run_until(1.0, |h| projectile(&h.w).is_some()));
    let sp = projectile(&h.w).unwrap();

    let hit = |w: &mut World, source: ObjectGuid, target: ObjectGuid, seed: u64| {
        ThreadSafeRandom::seed(seed);
        let (mut critical, mut defended, mut overpower) = (false, false, false);
        spell_projectile::calculate_damage(
            w,
            sp,
            Some(source),
            target,
            &mut critical,
            &mut defended,
            &mut overpower,
        )
        .map(|d| (d, critical))
    };
    let mut halved = 0;
    for seed in 1..40u64 {
        let eor_creature = hit(&mut h.w, MONSTER, PLAYER, seed);
        let eor_player = hit(&mut h.w, PLAYER, MONSTER, seed);
        h.w.era = empyrean_common::era::EraId::Infiltration.rules();
        let old_creature = hit(&mut h.w, MONSTER, PLAYER, seed);
        let old_player = hit(&mut h.w, PLAYER, MONSTER, seed);
        h.w.era = empyrean_common::era::EraId::Eor.rules();
        assert_eq!(old_player, eor_player, "a player's bolt is unchanged");
        if let (Some((eor, false)), Some((old, false))) = (eor_creature, old_creature) {
            assert!(old < eor && old >= eor * 0.4, "seed {seed}: {old} of {eor}");
            halved += 1;
        }
    }
    assert!(halved > 0, "some creature bolt landed without a critical");
}

// ------------------------------------------------------------------ real content (component weenies)

#[cfg(feature = "real-content")]
mod real_content_components {
    //! The retail dats and the world packs; fails, never skips, when they are absent.

    use super::*;
    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};
    use empyrean_world::entity::spell_formula;

    fn world(dir: &std::path::Path, pack: &std::path::Path) -> World {
        let real = RealDats::open(dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {}: {e}",
                dir.display()
            )
        });
        let dats = DatManager::initialize(Arc::new(real)).expect("the dats initialize");
        let clock = VirtualClock::default();
        let mut w = World::new(ClockSnapshot::take(&clock, 0.0), dats);
        w.content = Arc::new(PackContent::open(pack).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the world pack at {}: {e}",
                pack.display()
            )
        }));
        w
    }

    /// On the end-of-retail world every component the dats map to a weenie is the weenie the
    /// world's own spell component weenies name for it (two of them, Quicksilver and Moo Juice,
    /// stored under another component id and read under their own), so reading the weenies
    /// stands in for the map where a dat set has none.
    /// Divergence: V415
    #[test]
    fn the_worlds_component_weenies_name_the_same_weenies_the_dats_map() {
        let w = world(
            &dereth_dat::testing::dat_dir(),
            &empyrean_common::test_paths::world_pack(),
        );
        let from_content = Spell::component_wcids_from_content(&w);
        let mut compared = 0;
        let mut differ = Vec::new();
        for &component in spell_formula::spell_components_table(&w).components.keys() {
            let mapped = Spell::get_component_wcid(&w, component);
            if mapped == 0 {
                continue;
            }
            if from_content.get(&component).copied() != Some(mapped) {
                differ.push(component);
            }
            compared += 1;
        }
        assert!(
            differ.is_empty(),
            "components naming another weenie: {differ:?}"
        );
        assert!(compared > 100, "only {compared} components compared");
    }

    /// On the February 2005 dats, which carry no component map, every component a spell's formula
    /// names is found as the Infiltration world's weenie for it (a Lead Scarab is weenie 691), so
    /// a caster carrying the components can cast, and so is every other component in the table.
    /// Divergence: V415
    #[test]
    fn on_the_february_2005_dats_each_component_a_spell_uses_is_found_as_its_weenie() {
        let dir = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_else(|| {
            panic!(
                "{}",
                dereth_dat::testing::pre_tod_shortfall().unwrap_or_default()
            )
        });
        let mut w = world(&dir, &empyrean_common::test_paths::infiltration_pack());
        w.era = empyrean_common::era::EraId::Infiltration.rules();
        assert_eq!(Spell::get_component_wcid(&w, 1), 691, "Lead Scarab");

        let used: std::collections::BTreeSet<u32> = spell_formula::spell_table(&w)
            .spells
            .values()
            .flat_map(|s| s.comps.iter().copied())
            .collect();
        assert!(used.len() > 50, "only {} components used", used.len());
        for &component in &used {
            assert_ne!(
                Spell::get_component_wcid(&w, component),
                0,
                "component {component}"
            );
        }

        let missing: Vec<u32> = spell_formula::spell_components_table(&w)
            .components
            .keys()
            .copied()
            .filter(|&c| Spell::get_component_wcid(&w, c) == 0)
            .collect();
        assert!(
            missing.is_empty(),
            "components without a weenie: {missing:?}"
        );
    }

    /// A spell research formula is the account's own formula for a spell: laid in order, it names
    /// that spell; the same components out of order, or a slot after an empty one, name none.
    /// Divergence: V432
    #[test]
    fn a_research_formula_names_the_spell_whose_formula_it_is() {
        use empyrean_dat::file_types::spell_table as dat_spell_table;
        use empyrean_world::world_objects::spell_research::spell_of_formula;
        let w = world(
            &dereth_dat::testing::dat_dir(),
            &empyrean_common::test_paths::world_pack(),
        );
        let table = spell_formula::spell_table(&w);
        let formula = dat_spell_table::get_spell_formula(table, 1, "probe").expect("spell 1");
        let mut slots = [0u32; 8];
        for (slot, c) in slots.iter_mut().zip(&formula) {
            *slot = *c;
        }
        let named = spell_of_formula(&w, "probe", &slots).expect("the formula names a spell");
        assert_eq!(
            dat_spell_table::get_spell_formula(table, named, "probe").expect("named"),
            formula
        );
        let mut reversed = slots;
        reversed[..formula.len()].reverse();
        assert_ne!(spell_of_formula(&w, "probe", &reversed), Some(named));
        let mut gap = slots;
        gap.copy_within(1..7, 2);
        gap[1] = 0;
        assert_eq!(spell_of_formula(&w, "probe", &gap), None);
        assert_eq!(spell_of_formula(&w, "probe", &[0; 8]), None);
    }
}
