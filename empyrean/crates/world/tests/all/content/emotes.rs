//! Vectors: fixtures/vectors/emotes/
//! EmoteManager emote sets and actions follow ACE EmoteManager.cs.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use crate::content::emotes;

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SkillBase, SkillFormula};
use dereth_primitives::DataId;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::random::DotNetRandom;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f32_of, same_f32, throws, u64_of, Case};
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, SkillTable, XpTable};
use empyrean_dat::{DatManager, FakeDats};
use empyrean_entity::enums::{
    EmoteCategory, EmoteType, MotionCommand, MotionStance, PropertyAttribute, PropertyAttribute2nd,
    PropertyDataId, PropertyInt, PropertyString, VendorType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_emote::PropertiesEmote;
use empyrean_entity::models::properties_emote_action::PropertiesEmoteAction;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_testkit::land;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::damage_history::DamageHistory;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_event::game_event_type::GameEventType;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::creature_death;
use empyrean_world::world_objects::entity::creature_attribute::{CreatureAttribute, StatCtx};
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::managers::emote_manager as em;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;
use serde_json::Value;

// ------------------------------------------------------------------------------------ vectors

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

/// The synthetic tables the harness installs (`stats/tables.json`), as `stats.rs` builds them.
fn stat_dats() -> Arc<DatManager> {
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
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_xp_table(xp_table)
        .with_skill_table(skill_table)
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, secondary)
        .build()
        .expect("fake dats")
}

fn vector_world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 0.0,
        utc: empyrean_common::dotnet::datetime::DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, stat_dats())
}

fn u(v: &Value) -> u32 {
    u32::try_from(u64_of(v).unwrap_or_else(|| panic!("not a uint: {v}"))).expect("u32")
}

fn opt_u32(v: &Value) -> Option<u32> {
    v.as_u64().map(|x| u32::try_from(x).expect("u32"))
}

fn opt_i32(v: &Value) -> Option<i32> {
    v.as_i64().map(|x| i32::try_from(x).expect("i32"))
}

fn opt_str(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("emotes", name);
    assert!(!file.cases.is_empty(), "emotes/{name}: no cases");
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
        "emotes/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(10)].join("\n  ")
    );
}

/// The harness's creature: the biota records of `health`, the stat wrappers of
/// `Creature.SetEphemeralValues`, the emote table (row `i` has `DatabaseRecordId` `i`) and the
/// current motion.
fn vector_creature(input: &Value) -> WorldObject {
    let g = ObjectGuid::new(0x7000_0001);
    let mut o = WorldObject::allocate(Class::Creature);
    o.guid = g;
    o.biota.id = g.full();
    o.biota.properties_attribute = Some(DotNetDict::new());
    o.biota.properties_attribute_2nd = Some(DotNetDict::new());
    o.biota.properties_skill = Some(DotNetDict::new());
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let h = &input["health"];
    o.biota.properties_attribute_2nd.as_mut().unwrap().insert(
        PropertyAttribute2nd::MaxHealth,
        PropertiesAttribute2nd {
            init_level: u(&h[0]),
            level_from_cp: u(&h[1]),
            cp_spent: 0,
            current_level: u(&h[2]),
        },
    );
    o.biota.properties_attribute.as_mut().unwrap().insert(
        PropertyAttribute::Endurance,
        PropertiesAttribute {
            init_level: u(&h[3]),
            level_from_cp: 0,
            cp_spent: 0,
        },
    );
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        let cv = CreatureVital::new(&mut o, v);
        o.vitals_mut().insert(v, cv);
    }
    for a in 1..=6u16 {
        let ca = CreatureAttribute::new(&mut o, PropertyAttribute(a));
        o.attributes_mut().insert(PropertyAttribute(a), ca);
    }

    if let Some(rows) = input["table"].as_array() {
        let table: Vec<PropertiesEmote> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| PropertiesEmote {
                database_record_id: u32::try_from(i).unwrap(),
                category: EmoteCategory(opt_i32(&r[0]).unwrap()),
                probability: f32_of(&r[1]).unwrap(),
                quest: opt_str(&r[2]),
                vendor_type: opt_i32(&r[3]).map(VendorType),
                weenie_class_id: opt_u32(&r[4]),
                style: opt_u32(&r[5]).map(MotionStance),
                substyle: opt_u32(&r[6]).map(MotionCommand),
                min_health: f32_of(&r[7]),
                max_health: f32_of(&r[8]),
                ..PropertiesEmote::default()
            })
            .collect();
        o.biota.properties_emote = Some(Arc::new(table));
    }
    if let Some(m) = input["motion"].as_array() {
        o.wo.world_object_properties.current_motion_state = Some(Motion::new(
            MotionStance(u(&m[0])),
            MotionCommand(u(&m[1])),
            1.0,
        ));
    }
    o
}

/// `GetEmoteSet`: the category, quest (HearChat / ReceiveTalkDirect also take quest-less rows),
/// vendor type, wcid, heartbeat style / substyle and wounded-taunt health filters, the one
/// `Next(0.0f, 1.0f)` draw (none without an emote table or with `useRNG` false), the strict
/// `Probability > rng` test and the stable `OrderBy(Probability)`.
#[test]
fn get_emote_set_matches_ace() {
    replay("get_emote_set", |c| {
        let mut w = vector_world();
        let o = vector_creature(&c.input);
        let g = o.guid;
        w.objects.insert(o).expect("fresh");

        let q = &c.input["query"];
        let seed = c.input["seed"].as_i64().unwrap();
        ThreadSafeRandom::seed(u64::try_from(seed).unwrap());
        let quest = opt_str(&q[1]);
        let result = em::get_emote_set(
            &mut w,
            g,
            EmoteCategory(opt_i32(&q[0]).unwrap()),
            quest.as_deref(),
            opt_i32(&q[2]).map(VendorType),
            opt_u32(&q[3]),
            q[4].as_bool().unwrap(),
        );
        let next = ThreadSafeRandom::next(0, 1_000_000);
        let health = w.objects.get(g).unwrap().health();
        let percent = health.percent(&mut StatCtx::in_world(&mut w, g));

        let index = result.map(|e| e.database_record_id);
        let want_percent = f32_of(&c.output[2]).unwrap();
        let got = serde_json::json!([index, next, c.output[2]]);
        if got == c.output && same_f32(percent, want_percent) {
            Ok(())
        } else {
            Err(format!("[{index:?}, {next}, {percent}]"))
        }
    });
}

/// `Replace` over its tokens on objects that are neither Creature nor Player: names, levels,
/// templates, heritage, the embedded `quest@` handling of `%tqt` and `%CDtime` (ordinal,
/// ignoring case), and the NullReferenceException of the null-message warning with no target.
#[test]
fn replace_matches_ace() {
    replay("replace", |c| {
        let mut w = vector_world();
        let generic = |w: &mut World, guid: u32, v: &Value| -> ObjectGuid {
            let g = ObjectGuid::new(guid);
            let mut o = WorldObject::allocate(Class::GenericObject);
            o.guid = g;
            o.biota.id = guid;
            if let Some(n) = opt_str(&v["name"]) {
                o.set_property(PropertyString::Name, n);
            }
            if let Some(l) = opt_i32(&v["level"]) {
                o.set_property(PropertyInt::Level, l);
            }
            if let Some(t) = opt_str(&v["template"]) {
                o.set_property(PropertyString::Template, t);
            }
            if let Some(h) = opt_str(&v["heritage"]) {
                o.set_property(PropertyString::HeritageGroup, h);
            }
            w.objects.insert(o).expect("fresh");
            g
        };
        let source = generic(&mut w, 0x7000_0001, &c.input["source"]);
        let target = (!c.input["target"].is_null())
            .then(|| generic(&mut w, 0x7000_0002, &c.input["target"]));
        let message = opt_str(&c.input["message"]);
        let quest = opt_str(&c.input["quest"]);

        let got = catch_unwind(AssertUnwindSafe(|| {
            em::replace(
                &w,
                source,
                message.as_deref(),
                Some(source),
                target,
                quest.as_deref(),
            )
        }));
        match (got, throws(&c.output)) {
            (Ok(s), None) if c.output.as_str() == Some(s.as_str()) => Ok(()),
            (Err(_), Some("System.NullReferenceException")) => Ok(()),
            (Ok(s), _) => Err(format!("{s:?}")),
            (Err(_), _) => Err("a panic".to_owned()),
        }
    });
}

// ------------------------------------------------------------------------------------ world

const SEED: i32 = 55;
const LB: u32 = 0xA9B4_0000;
pub(crate) const NPC: ObjectGuid = ObjectGuid::new(0x7000_1000);
pub(crate) const P1: ObjectGuid = ObjectGuid::new(0x5000_0001);
const S1: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

pub(crate) struct H {
    pub(crate) w: World,
    clock: VirtualClock,
}

impl H {
    pub(crate) fn new() -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let mut w = World::new(
            now,
            empyrean_testkit::dats::with_stat_tables(FakeDats::new())
                .build()
                .expect("fake dats"),
        );
        w.timers = timers;
        w.content = Arc::new(MemContent::new());
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 0);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(u64::from(SEED.unsigned_abs()));
        let mut h = H { w, clock };
        h.creature(Class::Player, P1, "Tester");
        // every ACE player has a Character (its QuestManager reads the quest registry there)
        h.w.objects
            .get_mut(P1)
            .unwrap()
            .player
            .as_mut()
            .unwrap()
            .player
            .character = Some(empyrean_store::models::shard::Character {
            id: P1.full(),
            ..Default::default()
        });
        h.w.sessions.insert(
            S1,
            SessionData {
                player: Some(P1),
                ..SessionData::default()
            },
        );
        h.creature(Class::Creature, NPC, "Guard");
        lm::get_landblock(&mut h.w, LandblockId::new(LB | 0xFFFF), false, false);
        assert!(lm::add_object(&mut h.w, NPC, false), "the NPC is placed");
        h
    }

    fn creature(&mut self, class: Class, guid: ObjectGuid, name: &str) {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.properties_enchantment_registry = Some(Vec::new());
        let mut vitals = DotNetDict::new();
        for v in [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ] {
            vitals.insert(
                v,
                PropertiesAttribute2nd {
                    init_level: 100,
                    level_from_cp: 0,
                    cp_spent: 0,
                    current_level: 100,
                },
            );
        }
        o.biota.properties_attribute_2nd = Some(vitals);
        for v in [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ] {
            let cv = CreatureVital::new(&mut o, v);
            o.vitals_mut().insert(v, cv);
        }
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
        o.creature.as_mut().unwrap().creature_death.damage_history =
            DamageHistory::new(guid, self.w.now.utc);
        o.set_location(Some(Position::from_components(
            LB | 0x0001,
            20.0,
            20.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
        o.set_heartbeat_interval(Some(0.0));
        empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
            &mut o,
            self.w.now.unix_time,
        );
        self.w.objects.insert(o).expect("fresh guid");
    }

    pub(crate) fn emotes(&mut self, g: ObjectGuid, sets: Vec<PropertiesEmote>) {
        self.w.objects.get_mut(g).unwrap().biota.properties_emote = Some(Arc::new(sets));
    }

    /// Advances virtual time by `secs`, then one pass: the delay manager, then the landblocks.
    fn run(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        empyrean_world::entity::timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        delay_manager::run_actions(&mut self.w);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    fn busy(&self) -> (bool, i32) {
        let e = &self
            .w
            .objects
            .get(NPC)
            .unwrap()
            .wo
            .world_object
            .emote_manager;
        (e.is_busy, e.nested)
    }
}

pub(crate) fn act(t: EmoteType, delay: f32, message: &str) -> PropertiesEmoteAction {
    PropertiesEmoteAction {
        r#type: t.0.cast_unsigned(),
        delay,
        message: Some(message.to_owned()),
        ..PropertiesEmoteAction::default()
    }
}

pub(crate) fn set(
    category: EmoteCategory,
    probability: f32,
    quest: Option<&str>,
    actions: Vec<PropertiesEmoteAction>,
) -> PropertiesEmote {
    PropertiesEmote {
        category,
        probability,
        quest: quest.map(str::to_owned),
        properties_emote_action: actions,
        ..PropertiesEmote::default()
    }
}

/// A `String16L` at `at`.
fn string16l(b: &[u8], at: usize) -> String {
    let len = usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
    String::from_utf8(b[at + 2..at + 2 + len].to_vec()).unwrap()
}

pub(crate) fn sent() -> Vec<String> {
    take_sent()
        .into_iter()
        .filter(|(s, _, _)| *s == S1)
        .map(|(_, _, b)| {
            let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
            if op == GameMessageOpcode::GameEvent.0
                && u32::from_le_bytes(b[12..16].try_into().unwrap()) == GameEventType::Tell.0
            {
                format!("tell:{}", string16l(&b, 16))
            } else if op == GameMessageOpcode::ServerMessage.0 {
                format!("chat:{}", string16l(&b, 4))
            } else {
                format!("0x{op:04X}")
            }
        })
        .collect()
}

// ------------------------------------------------------------------------------------ execution

/// `ExecuteEmoteSet` -> `Enqueue` -> `DoEnqueue`: an action with no delay runs at once; each
/// `Delay` is a pre-delay after the previous action's returned delay (ActionChain in virtual
/// time). The object is busy (`IsBusy`, `Nested` 1) until the last action ran, and a second,
/// non-nested set is refused meanwhile (`if (IsBusy && !nested) return false`).
#[test]
fn a_use_set_runs_its_actions_in_order_with_their_delays() {
    let mut h = H::new();
    h.emotes(
        NPC,
        vec![set(
            EmoteCategory::Use,
            1.0,
            None,
            vec![
                act(EmoteType::Tell, 0.0, "Well met, %s."),
                act(EmoteType::DirectBroadcast, 2.0, "%n nods."),
                act(EmoteType::TextDirect, 0.5, "Farewell, %tn."),
            ],
        )],
    );
    start_capture();
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(sent(), ["tell:Well met, Tester."]);
    assert_eq!(h.busy(), (true, 1));

    // a second use while busy is refused
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(h.busy(), (true, 1));

    h.run(1.9);
    assert_eq!(sent(), Vec::<String>::new());
    h.run(0.1);
    assert_eq!(sent(), ["chat:Guard nods."]);
    assert_eq!(h.busy(), (true, 1));
    h.run(0.5);
    assert_eq!(sent(), ["chat:Farewell, Tester."]);
    assert_eq!(h.busy(), (false, 0));
}

/// `Goto` runs the `GotoSet` of the same quest name nested (`ExecuteEmoteSet(gotoSet, target,
/// true)`), then the calling set continues.
#[test]
fn goto_runs_the_goto_set_nested() {
    let mut h = H::new();
    h.emotes(
        NPC,
        vec![
            set(
                EmoteCategory::Use,
                1.0,
                None,
                vec![
                    act(EmoteType::Goto, 0.0, "Branch"),
                    act(EmoteType::TextDirect, 0.0, "after"),
                ],
            ),
            set(
                EmoteCategory::GotoSet,
                1.0,
                Some("branch"),
                vec![act(EmoteType::TextDirect, 0.0, "in the branch")],
            ),
        ],
    );
    start_capture();
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(sent(), ["chat:in the branch", "chat:after"]);
    assert_eq!(h.busy(), (false, 0));
}

/// A GotoSet that goes to itself recurses synchronously until `Nested > 75` with the set's quest
/// equal to the branching emote's message; that level logs and returns, and every level unwinds.
#[test]
fn a_self_goto_is_aborted_past_75_levels() {
    let mut h = H::new();
    h.emotes(
        NPC,
        vec![
            set(
                EmoteCategory::Use,
                1.0,
                None,
                vec![act(EmoteType::Goto, 0.0, "Loop")],
            ),
            set(
                EmoteCategory::GotoSet,
                1.0,
                Some("Loop"),
                vec![act(EmoteType::Goto, 0.0, "Loop")],
            ),
        ],
    );
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(h.busy(), (false, 0));
}

/// The `Inq*Stat` emotes branch to the `TestSuccess` / `TestFailure` set of the same name, or to
/// `TestNoQuality` when the property is absent and such a set exists.
#[test]
fn inq_int_stat_branches_on_the_targets_property() {
    let mut h = H::new();
    let mut inq = act(EmoteType::InqIntStat, 0.0, "LevelCheck");
    inq.stat = Some(i32::from(PropertyInt::Level.0));
    inq.min = Some(10);
    inq.max = Some(20);
    h.emotes(
        NPC,
        vec![
            set(EmoteCategory::Use, 1.0, None, vec![inq]),
            set(
                EmoteCategory::TestSuccess,
                1.0,
                Some("LevelCheck"),
                vec![act(EmoteType::TextDirect, 0.0, "success")],
            ),
            set(
                EmoteCategory::TestFailure,
                1.0,
                Some("LevelCheck"),
                vec![act(EmoteType::TextDirect, 0.0, "failure")],
            ),
            set(
                EmoteCategory::TestNoQuality,
                1.0,
                Some("LevelCheck"),
                vec![act(EmoteType::TextDirect, 0.0, "no quality")],
            ),
        ],
    );
    start_capture();
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(sent(), ["chat:no quality"]);
    for (level, want) in [
        (15, "chat:success"),
        (21, "chat:failure"),
        (10, "chat:success"),
        (9, "chat:failure"),
    ] {
        h.w.objects
            .get_mut(P1)
            .unwrap()
            .set_property(PropertyInt::Level, level);
        em::on_use(&mut h.w, NPC, P1);
        assert_eq!(sent(), [want], "level {level}");
    }
}

/// `OnDeath`: the Death set runs (IsBusy cleared first) with the last damager (through
/// `TryGetPetOwnerOrAttacker`) as its target. Smite is `OnDeath` + `Die(smiter, smiter)`.
#[test]
fn the_death_emote_fires_on_death() {
    let mut h = H::new();
    h.emotes(
        NPC,
        vec![set(
            EmoteCategory::Death,
            1.0,
            None,
            vec![act(EmoteType::DirectBroadcast, 0.0, "%s has slain %n!")],
        )],
    );
    h.w.objects
        .get_mut(NPC)
        .unwrap()
        .wo
        .world_object
        .emote_manager
        .is_busy = true;
    start_capture();
    creature_death::smite(&mut h.w, NPC, P1, false);
    assert!(sent().contains(&"chat:Tester has slain Guard!".to_owned()));
}

/// `GetEmoteSet` with its RNG: one `Next(0.0f, 1.0f)` draw, then the row of lowest probability
/// above it (`Where(Probability > rng).OrderBy(Probability).FirstOrDefault()`), pinned against a
/// reference `System.Random` with the same seed.
#[test]
fn the_seeded_selection_takes_the_lowest_probability_above_the_roll() {
    let mut h = H::new();
    let probabilities = [1.0f32, 0.3, 0.6, 0.3];
    let sets = probabilities
        .iter()
        .enumerate()
        .map(|(i, &p)| PropertiesEmote {
            database_record_id: u32::try_from(i).unwrap(),
            ..set(EmoteCategory::HeartBeat, p, None, vec![])
        })
        .collect();
    h.emotes(NPC, sets);
    for seed in 0..40 {
        ThreadSafeRandom::seed(seed);
        let mut reference = DotNetRandom::new(i32::try_from(seed).unwrap());
        let rng = reference.next_double();
        let want = if rng < 0.3f32.into() {
            1 // the first of the two 0.3 rows (a stable sort)
        } else if rng < 0.6f32.into() {
            2
        } else {
            0
        };
        let got = em::get_emote_set(
            &mut h.w,
            NPC,
            EmoteCategory::HeartBeat,
            None,
            None,
            None,
            true,
        )
        .map(|e| e.database_record_id);
        assert_eq!(got, Some(want), "seed {seed}, roll {rng}");
        assert_eq!(
            ThreadSafeRandom::next(0, 1_000_000),
            reference.next_range(0, 1_000_001),
            "one draw per call"
        );
    }
}

/// ACE-BUG pinned: an emote set with no actions throws on `ElementAt(0)` after `IsBusy` was set
/// and `Nested` counted, so the object stays busy.
#[test]
fn an_emote_set_without_actions_throws_and_leaves_the_object_busy() {
    let mut h = H::new();
    h.emotes(NPC, vec![set(EmoteCategory::Use, 1.0, None, vec![])]);
    let r = catch_unwind(AssertUnwindSafe(|| em::on_use(&mut h.w, NPC, P1)));
    assert!(r.is_err());
    assert_eq!(h.busy(), (true, 1));
}

/// `OnPortal` clears `IsBusy` before running the Portal set (so a stuck object recovers).
#[test]
fn on_portal_clears_is_busy_first() {
    let mut h = H::new();
    h.emotes(
        NPC,
        vec![set(
            EmoteCategory::Portal,
            1.0,
            None,
            vec![act(EmoteType::TextDirect, 0.0, "portal")],
        )],
    );
    h.w.objects
        .get_mut(NPC)
        .unwrap()
        .wo
        .world_object
        .emote_manager
        .is_busy = true;
    start_capture();
    em::on_portal(&mut h.w, NPC, P1);
    assert_eq!(sent(), ["chat:portal"]);
}

/// `IncrementIntStat` / `DecrementIntStat` change the target's property (default amount 1) and
/// tell the player with a private update.
#[test]
fn increment_and_decrement_int_stat() {
    let mut h = H::new();
    let mut inc = act(EmoteType::IncrementIntStat, 0.0, "");
    inc.stat = Some(i32::from(PropertyInt::Level.0));
    inc.amount = Some(5);
    let mut dec = act(EmoteType::DecrementIntStat, 0.0, "");
    dec.stat = Some(i32::from(PropertyInt::Level.0));
    h.emotes(
        NPC,
        vec![set(EmoteCategory::Use, 1.0, None, vec![inc, dec])],
    );
    start_capture();
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(
        h.w.objects
            .get(P1)
            .unwrap()
            .get_property(PropertyInt::Level),
        Some(4)
    );
    assert_eq!(sent(), ["0x02CD", "0x02CD"]);
}

/// Set stat emotes update the player through update property.
#[test]
fn set_stat_emotes_update_the_player_through_update_property() {
    let mut h = H::new();
    let mut int = act(EmoteType::SetIntStat, 0.0, "");
    int.stat = Some(i32::from(PropertyInt::Level.0));
    int.amount = Some(7);
    let mut boolean = act(EmoteType::SetBoolStat, 0.0, "");
    boolean.stat = Some(i32::from(
        empyrean_entity::enums::PropertyBool::Attackable.0,
    ));
    boolean.amount = Some(0);
    let mut float = act(EmoteType::SetFloatStat, 0.0, "");
    float.stat = Some(i32::from(
        empyrean_entity::enums::PropertyFloat::DefaultScale.0,
    ));
    float.percent = Some(1.5);
    let mut int64 = act(EmoteType::SetInt64Stat, 0.0, "");
    int64.stat = Some(i32::from(
        empyrean_entity::enums::PropertyInt64::TotalExperience.0,
    ));
    int64.amount_64 = Some(12_345);
    h.emotes(
        NPC,
        vec![set(
            EmoteCategory::Use,
            1.0,
            None,
            vec![int, boolean, float, int64],
        )],
    );
    start_capture();
    em::on_use(&mut h.w, NPC, P1);
    let p = h.w.objects.get(P1).unwrap();
    assert_eq!(p.get_property(PropertyInt::Level), Some(7));
    assert_eq!(
        p.get_property(empyrean_entity::enums::PropertyBool::Attackable),
        Some(false)
    );
    assert_eq!(
        p.get_property(empyrean_entity::enums::PropertyFloat::DefaultScale),
        Some(1.5)
    );
    assert_eq!(
        p.get_property(empyrean_entity::enums::PropertyInt64::TotalExperience),
        Some(12_345)
    );
    assert_eq!(sent(), ["0x02CE", "0x02D2", "0x02D4", "0x02D0"]);
}

/// `GetQuestTarget`: the `*MyQuest` emotes target the emoter, the others the target (or the
/// emoter without one).
#[test]
fn quest_target() {
    let (t, s) = (Some(P1), Some(NPC));
    assert_eq!(em::get_quest_target(EmoteType::StampMyQuest, t, s), s);
    assert_eq!(em::get_quest_target(EmoteType::StampQuest, t, s), t);
    assert_eq!(em::get_quest_target(EmoteType::StampQuest, None, s), s);
    assert_eq!(em::get_quest_target(EmoteType::InqMyQuestBitsOn, t, s), s);
}

/// `WoundedTaunt` keeps the rows whose `MinHealth <= Health.Percent <= MaxHealth` (both bounds
/// inclusive; a null bound compares false): at exactly half health the [0.5, 0.5] row runs.
#[test]
fn wounded_taunt_takes_the_rows_around_the_health_percent() {
    let mut h = H::new();
    let row = |min: Option<f32>, max: Option<f32>, text: &str| PropertiesEmote {
        min_health: min,
        max_health: max,
        ..set(
            EmoteCategory::WoundedTaunt,
            1.0,
            None,
            vec![act(EmoteType::TextDirect, 0.0, text)],
        )
    };
    h.emotes(
        NPC,
        vec![
            row(None, Some(1.0), "no min"),
            row(Some(0.51), Some(1.0), "above"),
            row(Some(0.5), Some(0.5), "exactly half"),
        ],
    );
    let o = h.w.objects.get_mut(NPC).unwrap();
    let health = o.health();
    health.set_current(o, 50);
    start_capture();
    em::on_damage(&mut h.w, NPC, Some(P1));
    assert_eq!(sent(), ["chat:exactly half"]);
}

/// `HearChat` (and `ReceiveTalkDirect`) with a quest name also take the rows that have no quest.
#[test]
fn hear_chat_takes_rows_without_a_quest() {
    let mut h = H::new();
    h.emotes(
        NPC,
        vec![
            set(
                EmoteCategory::HearChat,
                1.0,
                None,
                vec![act(EmoteType::TextDirect, 0.0, "any words")],
            ),
            set(
                EmoteCategory::HearChat,
                1.0,
                Some("password"),
                vec![act(EmoteType::TextDirect, 0.0, "the password")],
            ),
        ],
    );
    start_capture();
    em::on_hear_chat(&mut h.w, NPC, P1, "hello");
    assert_eq!(sent(), ["chat:any words"]);
    em::on_hear_chat(&mut h.w, NPC, P1, "PASSWORD");
    assert_eq!(
        sent(),
        ["chat:any words"],
        "the first of the two rows of probability 1 (a stable sort)"
    );
    em::on_talk_direct(&mut h.w, NPC, P1, "hello");
    assert_eq!(sent(), Vec::<String>::new(), "no ReceiveTalkDirect rows");
}

/// `InqBoolStat`: TestNoQuality for an absent property when that set exists, else the property's
/// value (null reads false) picks TestSuccess or TestFailure.
#[test]
fn inq_bool_stat_branches_on_the_targets_property() {
    let mut h = H::new();
    let mut inq = act(EmoteType::InqBoolStat, 0.0, "Flag");
    inq.stat = Some(i32::from(empyrean_entity::enums::PropertyBool::IsAdmin.0));
    let sets = vec![
        set(EmoteCategory::Use, 1.0, None, vec![inq]),
        set(
            EmoteCategory::TestSuccess,
            1.0,
            Some("Flag"),
            vec![act(EmoteType::TextDirect, 0.0, "success")],
        ),
        set(
            EmoteCategory::TestFailure,
            1.0,
            Some("Flag"),
            vec![act(EmoteType::TextDirect, 0.0, "failure")],
        ),
    ];
    let mut with_no_quality = sets.clone();
    with_no_quality.push(set(
        EmoteCategory::TestNoQuality,
        1.0,
        Some("Flag"),
        vec![act(EmoteType::TextDirect, 0.0, "no quality")],
    ));
    h.emotes(NPC, with_no_quality);
    start_capture();
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(sent(), ["chat:no quality"]);
    h.emotes(NPC, sets);
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(
        sent(),
        ["chat:failure"],
        "absent reads false without a TestNoQuality set"
    );
    h.w.objects
        .get_mut(P1)
        .unwrap()
        .set_property(empyrean_entity::enums::PropertyBool::IsAdmin, true);
    em::on_use(&mut h.w, NPC, P1);
    assert_eq!(sent(), ["chat:success"]);
}

/// `AwardSkillPoints` of 3 raises a trained skill by exactly 3 ranks, the XP of those ranks granted
/// at once (V297; ACE granted the first rank's cost 3 times, so fewer ranks), and stops at the
/// skill's last rank.
#[test]
fn award_skill_points_raises_the_skill_by_that_many_ranks() {
    use empyrean_entity::enums::{Skill, SkillAdvancementClass};
    use empyrean_entity::models::properties_skill::PropertiesSkill;

    let mut h = H::new();
    h.w.dats = stat_dats();
    // the XP is granted on the player's own queue, which its landblock runs
    assert!(lm::add_object(&mut h.w, P1, false), "the player is placed");
    let table = h.w.dats.portal_dat().xp_table().trained_xp.clone();
    let last = u16::try_from(table.len() - 1).unwrap();
    let award = |h: &mut H, from: u16| {
        let mut skills = DotNetDict::new();
        let pp = table[usize::from(from)];
        skills.insert(
            Skill::MagicDefense,
            PropertiesSkill {
                sac: SkillAdvancementClass::Trained,
                level_from_pp: from,
                pp,
                ..PropertiesSkill::default()
            },
        );
        h.w.objects.get_mut(P1).unwrap().biota.properties_skill = Some(skills);
        let action = PropertiesEmoteAction {
            r#type: EmoteType::AwardSkillPoints.0.cast_unsigned(),
            stat: Some(Skill::MagicDefense.0),
            amount: Some(3),
            ..PropertiesEmoteAction::default()
        };
        h.emotes(NPC, vec![set(EmoteCategory::Use, 1.0, None, vec![action])]);
        em::on_use(&mut h.w, NPC, P1);
        h.run(1.0);
        h.run(1.0);
        let s =
            h.w.objects
                .get(P1)
                .unwrap()
                .biota
                .properties_skill
                .as_ref()
                .unwrap()
                .get(&Skill::MagicDefense)
                .unwrap()
                .clone();
        (s.level_from_pp, s.pp)
    };
    // a mid rank, where each rank costs more than the one before
    let from = last / 2;
    assert!(
        table[usize::from(from) + 3] - table[usize::from(from) + 2]
            > table[usize::from(from) + 1] - table[usize::from(from)]
    );
    assert_eq!(
        award(&mut h, from),
        (from + 3, table[usize::from(from) + 3])
    );
    // two ranks short of the last: raised to the last rank only
    assert_eq!(award(&mut h, last - 2), (last, table[usize::from(last)]));
}

mod activation_failures {
    use crate::support::content_interactions::*;

    /// `InqYesNo` when the player already has a yes/no question open: the question cannot be sent,
    /// and the TestFailure set of the emote's name runs (nested, as the other test emotes' branches
    /// do). ACE ran it as a new set, which the busy NPC refused, so nothing followed.
    #[test]
    fn a_yes_no_question_that_cannot_be_sent_runs_the_failure_set() {
        let mut h = H::new();
        let mut ask = act(EmoteType::InqYesNo, 0.0, "Ready");
        ask.test_string = Some("Are you ready?".to_owned());
        h.emotes(
            NPC,
            vec![
                set(EmoteCategory::Use, 1.0, None, vec![ask]),
                set(
                    EmoteCategory::TestFailure,
                    1.0,
                    Some("Ready"),
                    vec![act(EmoteType::TextDirect, 0.0, "Come back when you are.")],
                ),
            ],
        );
        start_capture();
        em::on_use(&mut h.w, NPC, P1);
        assert!(
            !sent().iter().any(|m| m.starts_with("chat:")),
            "the question is asked"
        );

        // the question is still open: the second one cannot be sent
        em::on_use(&mut h.w, NPC, P1);
        assert_eq!(sent(), ["chat:Come back when you are."]);
        let e = &h.w.objects.get(NPC).unwrap().wo.world_object.emote_manager;
        assert_eq!((e.is_busy, e.nested), (false, 0), "the set finished");
    }

    /// `OnTalk` on an object with no `ActivationTalk` says nothing (the world database's pressure
    /// plate wcid 70090 has Talk in its ActivationResponse and no text). ACE built the chat line from
    /// the null text and threw, ending the activation.
    #[test]
    fn talk_without_activation_talk_says_nothing() {
        let mut h = H::new();
        let plate = object(&mut h.w, Class::PressurePlate, 0x8000_0300);
        start_capture();
        let r = catch_unwind(AssertUnwindSafe(|| {
            dispatch::on_talk::on_talk(&mut h.w, plate, P1)
        }));
        assert!(r.is_ok(), "no panic");
        assert_eq!(sent(), Vec::<String>::new());

        o(&mut h.w, plate).set_property(PropertyString::ActivationTalk, "Click.".to_owned());
        dispatch::on_talk::on_talk(&mut h.w, plate, P1);
        assert_eq!(sent(), ["chat:Click."]);
    }
}

mod quest_actions {
    use crate::support::quest_world::*;

    /// The emote quest actions over 5.6's `QuestManager` (`EmoteManager.ExecuteEmote`): StampQuest
    /// stamps the player's registry, InqQuest then answers QuestSuccess (has it, and cannot solve it
    /// yet: MinDelta 7); StampMyQuest stamps the NPC's own runtime quests; AddCharacterTitle registers
    /// the title; InqContractsFull answers TestFailure for a player with no contracts.
    #[test]
    fn emote_quest_title_and_contract_actions_reach_their_managers() {
        use super::emotes::{act, sent, set, H, NPC, P1};
        let mut h = H::new();
        let quest = |id: u32, name: &str, max_solves: i32, min_delta: u32| {
            empyrean_content::models::world::Quest {
                id,
                name: name.to_owned(),
                max_solves,
                min_delta,
                message: Some(String::new()),
                ..Default::default()
            }
        };
        h.w.content = std::sync::Arc::new(
            empyrean_content::MemContent::new()
                .quest(quest(1, "ShortTimer", 5, 7))
                .quest(quest(2, "NpcMemory", -1, 0)),
        );

        let mut title = act(EmoteType::AddCharacterTitle, 0.0, "");
        title.amount = Some(i32::try_from(CharacterTitle(1).0).unwrap());
        let contracts_full = act(EmoteType::InqContractsFull, 0.0, "Contracts");
        h.emotes(
            NPC,
            vec![
                set(
                    EmoteCategory::Use,
                    1.0,
                    None,
                    vec![
                        act(EmoteType::StampQuest, 0.0, "ShortTimer"),
                        act(EmoteType::InqQuest, 0.0, "ShortTimer"),
                        act(EmoteType::StampMyQuest, 0.0, "NpcMemory"),
                        title,
                        contracts_full,
                    ],
                ),
                set(
                    EmoteCategory::QuestSuccess,
                    1.0,
                    Some("ShortTimer"),
                    vec![act(EmoteType::TextDirect, 0.0, "on cooldown")],
                ),
                set(
                    EmoteCategory::QuestFailure,
                    1.0,
                    Some("ShortTimer"),
                    vec![act(EmoteType::TextDirect, 0.0, "can solve")],
                ),
                set(
                    EmoteCategory::TestSuccess,
                    1.0,
                    Some("Contracts"),
                    vec![act(EmoteType::TextDirect, 0.0, "full")],
                ),
                set(
                    EmoteCategory::TestFailure,
                    1.0,
                    Some("Contracts"),
                    vec![act(EmoteType::TextDirect, 0.0, "not full")],
                ),
            ],
        );

        start_capture();
        em::on_use(&mut h.w, NPC, P1);
        assert_eq!(sent(), ["chat:on cooldown", "chat:not full"]);

        let p1 = QuestOwner::Creature(P1);
        assert_eq!(
            qm::get_quest(&h.w, &p1, "ShortTimer").map(|q| q.num_times_completed),
            Some(1)
        );
        assert!(
            qm::has_quest(&h.w, &QuestOwner::Creature(NPC), "NpcMemory"),
            "the NPC's own registry"
        );
        assert!(!qm::has_quest(&h.w, &p1, "NpcMemory"));
        assert_eq!(h.w.objects.get(P1).unwrap().num_character_titles(), Some(1));
    }

    /// A null quest name on a quest emote is ACE's `NullReferenceException` in
    /// `QuestManager.GetQuestName` (caught per action), not a silent "no quest".
    #[test]
    fn a_quest_emote_without_a_quest_name_throws() {
        use super::emotes::{act, set, H, NPC, P1};
        let mut h = H::new();
        let mut inq = act(EmoteType::InqQuest, 0.0, "");
        inq.message = None;
        h.emotes(NPC, vec![set(EmoteCategory::Use, 1.0, None, vec![inq])]);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            em::on_use(&mut h.w, NPC, P1)
        }));
        let err = r.expect_err("GetQuestName(null) throws");
        let text = err
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| err.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        assert!(text.contains("NullReferenceException"), "{text}");
    }
}

mod barber {
    use crate::support::player_services::*;

    /// The StartBarber emote runs `Player.StartBarber`: BarberActive set and the StartBarber event
    /// (0x0075) sent.
    #[test]
    fn the_start_barber_emote_opens_the_barber() {
        let mut h = H::small();
        let sa = h.player(A, "Alpha", 3);
        start_capture();
        empyrean_world::world_objects::managers::emote_manager::shims::player_start_barber(
            &mut h.w, A,
        );
        assert_eq!(events_to(&sent(), sa), [0x0075]);
        assert_eq!(
            h.w.objects
                .get(A)
                .unwrap()
                .get_property(PropertyBool::BarberActive),
            Some(true)
        );
    }
}
