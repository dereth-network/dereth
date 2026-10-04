//! Vectors: fixtures/vectors/quests/
//! QuestManager, ContractManager and Player_Character follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{
    CharGen, Contract, ContractTable, EnumMapper, EyeStrip, HairStyle,
    HeritageGroup as HeritageGroupCG, ObjDesc, SexCg,
};
use dereth_assets::ui::{StringTable, StringTableEntry};
use dereth_assets::PaletteSet;
use dereth_primitives::{CellId, DataId, Frame, Position};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::time_span_extensions::get_friendly_string;
use empyrean_common::vectors::{self, i64_of};
use empyrean_content::models::world::{Quest, Weenie as ContentWeenie};
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CharacterOption, CharacterTitle, ChatMessageType, HeritageGroup, PropertyDataId, PropertyInt,
    PropertyString, WeenieError, WeenieType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_store::models::shard::{Character, CharacterPropertiesContractRegistry};
use empyrean_store::MemShard;
use empyrean_world::dispatch::Class;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::property_manager as pm;
use empyrean_world::managers::quest_manager::{self as qm, QuestManager, QuestOwner};
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::managers::contract_manager as cm;
use empyrean_world::world_objects::player_character as pc;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::World;
use serde_json::{json, Value};

// ------------------------------------------------------------------ fixtures

const PLAYER_WCID: u32 = 1;
pub(crate) const PLAYER: u32 = 0x5000_0001;
const CHICKEN: u32 = 0x8000_0123;
pub(crate) const RAT: u32 = 0x8000_0200;
const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
pub(crate) const T0: f64 = 1_767_225_600.0;

// opcodes and game-event types
const GAME_EVENT: u32 = 0xF7B0;
const SYSTEM_CHAT: u32 = 0xF7E0;
const SAVE_FAILED: u32 = 0x00A0;
const WEENIE_ERROR: u32 = 0x028A;
const TRANSIENT: u32 = 0x02EB;
const UPDATE_TITLE: u32 = 0x002B;
const CONTRACT_TRACKER: u32 = 0x0315;
const PRIVATE_DATA_ID: u32 = 0x02D7;

/// The rows `QuestVectors.cs` installs (`quests/quest_table.json`).
fn quest_table() -> Vec<Quest> {
    let file = vectors::load_named("quests", "quest_table");
    file.cases
        .iter()
        .filter(|c| !c.output.is_null())
        .enumerate()
        .map(|(i, c)| Quest {
            id: u32::try_from(i + 1).unwrap(),
            name: c.input["name"].as_str().unwrap().to_owned(),
            min_delta: u32::try_from(i64_of(&c.output["min_delta"]).unwrap()).unwrap(),
            max_solves: i32::try_from(i64_of(&c.output["max_solves"]).unwrap()).unwrap(),
            message: Some(String::new()),
            ..Default::default()
        })
        .collect()
}

fn content() -> MemContent {
    let mut c = MemContent::new().weenie(
        ContentWeenie::new(PLAYER_WCID, "human", WeenieType::Creature)
            .with_did(
                empyrean_entity::enums::PropertyDataId::CombatTable,
                0x3000_0000,
            )
            .with_string(PropertyString::Name, "Tester")
            .with_int(PropertyInt::ItemsCapacity, 102)
            .with_int(PropertyInt::ContainersCapacity, 7),
    );
    for q in quest_table() {
        c = c.quest(q);
    }
    c
}

fn position() -> Position {
    Position::new(CellId(0), Frame::default())
}

/// Contract 7 watches `RatHunt`/`RatHuntStarted`/`RatHuntDone`; contract 9 watches only
/// `DailyRun`; contract 40 watches nothing.
fn contract_table() -> ContractTable {
    let contract = |id: u32, name: &str, flags: [&str; 6]| {
        let mut strings: [String; 11] = Default::default();
        strings[0] = name.to_owned();
        for (i, f) in flags.iter().enumerate() {
            strings[5 + i] = (*f).to_owned();
        }
        Contract {
            version: 1,
            contract_id: id,
            strings,
            location_npc_start: position(),
            location_npc_end: position(),
            location_quest_area: position(),
        }
    };
    let mut contracts = std::collections::BTreeMap::new();
    contracts.insert(
        7,
        contract(
            7,
            "Rat Hunt",
            ["", "RatHuntStarted", "RatHuntDone", "RatHunt", "", ""],
        ),
    );
    contracts.insert(
        9,
        contract(9, "Daily Run", ["DailyRun", " ", "", "", "", ""]),
    );
    contracts.insert(40, contract(40, "Nothing", ["", "", "", "", "", ""]));
    ContractTable {
        id: DataId(file_id::CONTRACT_TABLE),
        buckets: 32,
        contracts,
    }
}

fn objdesc(textures: &[(u32, u32)], parts: &[u32]) -> ObjDesc {
    ObjDesc {
        version: 0x11,
        palette: None,
        subpalettes: Vec::new(),
        texture_changes: textures
            .iter()
            .map(|&(o, n)| (0u8, DataId(o), DataId(n)))
            .collect(),
        anim_part_changes: parts.iter().map(|&p| (16u8, DataId(p))).collect(),
    }
}

/// Heritage 1 (Aluvian) and 9 (Empyrean), each with one male sex whose appearance lists a
/// single valid choice per feature.
fn char_gen() -> CharGen {
    let sex = || SexCg {
        naming_help: None,
        name: "Male".into(),
        scale: 100,
        setup: DataId(0x0200_0001),
        sound_table: DataId(0x2000_0001),
        icon: 0,
        base_palette: DataId(0x0400_007E),
        skin_palset: DataId(0x0F00_0001),
        physics_table: DataId(0x3400_0004),
        motion_table: DataId(0x0900_0001),
        combat_table: DataId(0x3000_0000),
        base_objdesc: objdesc(&[], &[]),
        hair_colors: vec![0x0F00_0002],
        hair_styles: vec![HairStyle {
            icon: 0,
            bald: 0,
            alternate_setup: DataId(0),
            objdesc: objdesc(&[(0x0500_1000, 0x0500_1001)], &[0x0100_0100]),
        }],
        eye_colors: vec![0x0400_0300],
        eye_strips: vec![EyeStrip {
            icon: 0,
            icon_bald: 0,
            objdesc: objdesc(&[(0x0500_2000, 0x0500_2001)], &[]),
            objdesc_bald: objdesc(&[], &[]),
        }],
        nose_strips: vec![(0, objdesc(&[(0x0500_3000, 0x0500_3001)], &[]))],
        mouth_strips: vec![(0, objdesc(&[(0x0500_4000, 0x0500_4001)], &[]))],
        headgear: Vec::new(),
        shirts: Vec::new(),
        pants: Vec::new(),
        footwear: Vec::new(),
        clothing_colors: Vec::new(),
    };
    let heritage = |name: &str| HeritageGroupCG {
        description: None,
        sex_order: vec![1],
        template_presentations: std::collections::BTreeMap::new(),
        name: name.into(),
        icon: 0,
        setup: DataId(0x0200_0001),
        environment_setup: DataId(0),
        attribute_credits: 330,
        skill_credits: 50,
        primary_start_areas: Vec::new(),
        secondary_start_areas: Vec::new(),
        skills: Vec::new(),
        templates: Vec::new(),
        sex_table_marker: 0,
        sexes: std::collections::BTreeMap::from([(1, sex())]),
    };
    CharGen {
        heritage_order: vec![1, 9],
        help_strings: Vec::new(),
        id: DataId(file_id::CHAR_GEN),
        second_data_id: DataId(0),
        starter_areas: Vec::new(),
        hg_table_marker: 0,
        heritage_groups: std::collections::BTreeMap::from([
            (1, heritage("Aluvian")),
            (9, heritage("Empyrean")),
        ]),
    }
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    let titles = EnumMapper {
        id: DataId(pc::ENUM_MAPPER_CHARACTER_TITLE_FILE_ID),
        base_emp_did: DataId(0),
        id_to_string: vec![(1, "ID_CharacterTitle_Adventurer".into())],
    };
    let entry = StringTableEntry {
        table: DataId(0),
        strings: vec!["Adventurer".into()],
        variables: Vec::new(),
        has_var_names: 0,
        var_names: Vec::new(),
    };
    let language = StringTable {
        id: DataId(file_id::CHARACTER_TITLES),
        version: 1,
        bucket_index: 0,
        strings: vec![(
            empyrean_dat::file_types::spell_table::compute_hash("ID_CharacterTitle_Adventurer"),
            entry,
        )],
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::CONTRACT_TABLE, contract_table())
        .with_char_gen(char_gen())
        .with_portal(
            0x0F00_0001,
            PaletteSet {
                id: DataId(0x0F00_0001),
                palette_ids: vec![DataId(0x0400_0101)],
            },
        )
        .with_portal(
            0x0F00_0002,
            PaletteSet {
                id: DataId(0x0F00_0002),
                palette_ids: vec![DataId(0x0400_0201)],
            },
        )
        .with_portal(pc::ENUM_MAPPER_CHARACTER_TITLE_FILE_ID, titles)
        .with_language(file_id::CHARACTER_TITLES, language)
        .build()
        .expect("fake dats")
}

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

/// A world on synthetic content with ACE's default server properties, one session S whose
/// player is PLAYER (with an empty Character), a non-player creature CHICKEN and a creature RAT.
pub(crate) fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: T0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats());
    w.content = Arc::new(content());
    guid_manager::initialize(&mut w, &mut EmptyShard);
    pm::install_shard_config(&mut w, pm::shard_config_handle(Box::new(MemShard::new())));
    pm::initialize(&mut w, true);

    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(&w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            Class::Player,
            weenie,
            ObjectGuid::new(PLAYER),
            1,
        )
    });
    o.player.as_mut().expect("a player").player.character = Some(Character {
        id: PLAYER,
        account_id: 7,
        name: "Tester".to_owned(),
        ..Default::default()
    });
    o.player
        .as_mut()
        .unwrap()
        .player_database
        .character_changes_detected = false;
    w.objects.insert(o).expect("fresh");

    for (guid, name, plural) in [(CHICKEN, "Chicken", None), (RAT, "Rat", Some("Rats"))] {
        let mut c = WorldObject::allocate(Class::Creature);
        c.guid = ObjectGuid::new(guid);
        c.biota = empyrean_entity::Biota {
            id: guid,
            weenie_class_id: 50,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        c.biota.set_property(PropertyString::Name, name.to_owned());
        if let Some(p) = plural {
            c.biota
                .set_property(PropertyString::PluralName, p.to_owned());
        }
        w.objects.insert(c).expect("fresh");
    }

    let mut s = SessionData::default();
    s.set_account(
        7,
        "acct".to_owned(),
        empyrean_entity::enums::AccessLevel::Player,
    );
    s.set_player(Some(ObjectGuid::new(PLAYER)));
    w.sessions.insert(S, s);
    w
}

pub(crate) fn player() -> ObjectGuid {
    ObjectGuid::new(PLAYER)
}

pub(crate) fn me() -> QuestOwner<'static> {
    QuestOwner::Creature(player())
}

fn character(w: &World) -> &Character {
    w.objects
        .get(player())
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player
        .character
        .as_ref()
        .unwrap()
}

fn changes_detected(w: &World) -> bool {
    w.objects
        .get(player())
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player_database
        .character_changes_detected
}

fn clear_changes(w: &mut World) {
    w.objects
        .get_mut(player())
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_database
        .character_changes_detected = false;
}

pub(crate) fn set_now(w: &mut World, unix_time: f64) {
    w.now.unix_time = unix_time;
}

/// Each sent message's kind: the game-event type inside a `0xF7B0`, else the opcode.
pub(crate) fn kinds(sent: &[(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)]) -> Vec<u32> {
    sent.iter()
        .map(|(_, _, b)| {
            if u32_at(b, 0) == GAME_EVENT {
                u32_at(b, 12)
            } else {
                u32_at(b, 0)
            }
        })
        .collect()
}

pub(crate) fn we(e: WeenieError) -> u32 {
    e.0.cs_cast()
}

fn we_chat(t: ChatMessageType) -> u32 {
    t.0.cs_cast()
}

pub(crate) fn u32_at(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
}

fn f64_at(bytes: &[u8], i: usize) -> f64 {
    f64::from_le_bytes(bytes[i..i + 8].try_into().unwrap())
}

/// A `GameMessageSystemChat`'s text and chat type.
fn chat(bytes: &[u8]) -> (String, u32) {
    let len = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
    let text = String::from_utf8(bytes[6..6 + len].to_vec()).unwrap();
    let mut end = 6 + len;
    end += (4 - (2 + len) % 4) % 4; // String16L pads to a dword boundary
    (text, u32_at(bytes, end))
}

pub(crate) fn chats(sent: &[(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)]) -> Vec<String> {
    sent.iter()
        .filter(|(_, _, b)| u32_at(b, 0) == SYSTEM_CHAT)
        .map(|(_, _, b)| chat(b).0)
        .collect()
}

/// A contract tracker event: (contract id, stage, time when done, time when repeats, delete).
fn tracker(bytes: &[u8]) -> (u32, u32, f64, f64, u32) {
    (
        u32_at(bytes, 20),
        u32_at(bytes, 24),
        f64_at(bytes, 28),
        f64_at(bytes, 36),
        u32_at(bytes, 44),
    )
}

// ------------------------------------------------------------------ vectors

fn span_json(t: TimeSpan) -> Value {
    json!({"friendly": get_friendly_string(t), "ticks": t.ticks(), "total_seconds": t.total_seconds()})
}

fn registry_json(w: &World, owner: &QuestOwner<'_>) -> Value {
    Value::Array(
        qm::get_quests(w, owner)
            .into_iter()
            .map(|q| {
                json!([
                    q.quest_name,
                    q.last_time_completed,
                    q.num_times_completed,
                    q.character_id
                ])
            })
            .collect(),
    )
}

fn opt_i32(v: &Value) -> Option<i32> {
    i64_of(v).map(|x| i32::try_from(x).unwrap())
}

fn int(v: &Value) -> i32 {
    opt_i32(v).expect("an int")
}

/// `QuestVectors.Script`: 4,000 steps on a non-player creature's manager (its `runtimeQuests`),
/// each compared with ACE's result and ACE's whole registry in `HashSet` order.
#[test]
fn quest_manager_script_matches_ace() {
    let mut w = world();
    let chicken = ObjectGuid::new(CHICKEN);
    let file = vectors::load_named("quests", "script");
    assert_eq!(file.cases.len(), 4000);
    let mut failures = Vec::new();
    let mut throws = 0;
    for (i, c) in file.cases.iter().enumerate() {
        let input = &c.input;
        if input["reset"].as_bool().unwrap() {
            w.objects
                .get_mut(chicken)
                .unwrap()
                .creature
                .as_mut()
                .unwrap()
                .creature
                .quest_manager = None;
        }
        set_now(&mut w, input["now"].as_f64().unwrap());
        assert!(pm::modify_double(
            &w,
            "quest_mindelta_rate",
            input["rate"].as_f64().unwrap(),
            true
        ));
        let quest = input["quest"].as_str().unwrap();
        let (a, b) = (&input["a"], &input["b"]);
        let op = input["op"].as_str().unwrap();

        let got = catch_unwind(AssertUnwindSafe(|| {
            let mut owner = QuestOwner::Creature(chicken);
            let w = &mut w;
            match op {
                "Update" => {
                    qm::update(w, &mut owner, quest);
                    Value::Null
                }
                "Stamp" => {
                    qm::stamp(w, &mut owner, quest);
                    Value::Null
                }
                "Increment" => {
                    qm::increment(w, &mut owner, quest, int(a));
                    Value::Null
                }
                "Decrement" => {
                    qm::decrement(w, &mut owner, quest, int(a));
                    Value::Null
                }
                "Erase" => {
                    qm::erase(w, &mut owner, quest);
                    Value::Null
                }
                "EraseAll" => {
                    qm::erase_all(w, &mut owner);
                    Value::Null
                }
                "SetQuestCompletions" => {
                    qm::set_quest_completions(w, &mut owner, quest, int(a));
                    Value::Null
                }
                "SetQuestBits" => {
                    qm::set_quest_bits(w, &mut owner, quest, int(a), b.as_bool().unwrap());
                    Value::Null
                }
                "HasQuest" => json!(qm::has_quest(w, &owner, quest)),
                "HasQuestCompletes" => json!(qm::has_quest_completes(w, &owner, quest)),
                "GetQuest" => {
                    qm::get_quest(w, &owner, qm::get_quest_name(quest)).map_or(Value::Null, |q| {
                        json!([
                            q.quest_name,
                            q.last_time_completed,
                            q.num_times_completed,
                            q.character_id
                        ])
                    })
                }
                "CanSolve" => json!(qm::can_solve(w, &owner, quest)),
                "IsMaxSolves" => json!(qm::is_max_solves(w, &owner, quest)),
                "GetMaxSolves" => json!(qm::get_max_solves(w, quest)),
                "GetCurrentSolves" => json!(qm::get_current_solves(w, &owner, quest)),
                "GetNextSolveTime" => span_json(qm::get_next_solve_time(w, &owner, quest)),
                "HasQuestSolves" => {
                    json!(qm::has_quest_solves(
                        w,
                        &owner,
                        quest,
                        opt_i32(a),
                        opt_i32(b)
                    ))
                }
                "HasQuestBits" => json!(qm::has_quest_bits(w, &owner, quest, int(a))),
                "HasNoQuestBits" => json!(qm::has_no_quest_bits(w, &owner, quest, int(a))),
                "GetQuestName" => json!(qm::get_quest_name(quest)),
                other => panic!("unknown op {other}"),
            }
        }));
        let result = match got {
            Ok(v) => v,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                assert!(
                    msg.contains("OverflowException"),
                    "step {i}: unexpected panic {msg}"
                );
                throws += 1;
                json!({"throws": "OverflowException"})
            }
        };
        let registry = registry_json(&w, &QuestOwner::Creature(chicken));
        let expected = &c.output;
        let result_ok = match (&result, &expected["result"]) {
            (Value::Object(r), Value::Object(e)) if e.contains_key("total_seconds") => {
                r["friendly"] == e["friendly"]
                    && r["ticks"] == e["ticks"]
                    && vectors::same_f64(
                        r["total_seconds"].as_f64().unwrap(),
                        vectors::f64_of(&e["total_seconds"]).unwrap(),
                    )
            }
            (r, e) => r == e,
        };
        if !result_ok || registry != expected["registry"] {
            failures.push(format!(
                "step {i} {input}: expected {expected}, got result {result} registry {registry}"
            ));
        }
    }
    assert!(throws > 0, "the script's OverflowException cases ran");
    assert!(
        failures.is_empty(),
        "{} of 4000 steps differ from ACE:\n  {}",
        failures.len(),
        failures[..failures.len().min(8)].join("\n  ")
    );
}

// ------------------------------------------------------------------ QuestManager on a player

/// `Update` stamps the player's Character (not `runtimeQuests`), marks the Character changed;
/// `CanSolve` is false until `MinDelta` has passed on the clock, scaled by
/// `quest_mindelta_rate` except for `ColoArena*`; `MaxSolves` caps the solves for good.
#[test]
fn stamping_a_quest_gates_can_solve_by_min_delta_and_max_solves() {
    let mut w = world();
    let mut owner = me();

    assert!(
        qm::can_solve(&w, &owner, "ShortTimer"),
        "never solved: can solve"
    );
    assert_eq!(
        qm::get_next_solve_time(&w, &owner, "NoSuchQuest"),
        TimeSpan::MAX_VALUE,
        "a quest the world lacks can never be solved"
    );

    qm::stamp(&mut w, &mut owner, "ShortTimer@first solve");
    let row = character(&w)
        .get_quest("shorttimer")
        .cloned()
        .expect("in the Character");
    assert_eq!(
        (
            row.quest_name.as_str(),
            row.num_times_completed,
            row.last_time_completed,
            row.character_id
        ),
        ("ShortTimer", 1, 1_767_225_600_u32, PLAYER)
    );
    assert!(changes_detected(&w), "CharacterChangesDetected");
    assert!(w
        .objects
        .get(player())
        .unwrap()
        .creature
        .as_ref()
        .unwrap()
        .creature
        .quest_manager
        .as_ref()
        .unwrap()
        .runtime_quests
        .iter()
        .next()
        .is_none());

    // MinDelta 7: 6 s later it cannot be solved (1 s left), at 7 s it can.
    set_now(&mut w, T0 + 6.0);
    assert!(!qm::can_solve(&w, &owner, "ShortTimer"));
    assert_eq!(
        qm::get_next_solve_time(&w, &owner, "ShortTimer"),
        TimeSpan::from_seconds(1.0)
    );
    set_now(&mut w, T0 + 7.0);
    assert!(qm::can_solve(&w, &owner, "ShortTimer"));

    // quest_mindelta_rate 2: now 14 s (the stamp is at T0 + 7).
    qm::update(&mut w, &mut owner, "ShortTimer");
    assert!(pm::modify_double(&w, "quest_mindelta_rate", 2.0, true));
    set_now(&mut w, T0 + 7.0 + 13.0);
    assert!(!qm::can_solve(&w, &owner, "ShortTimer"));
    set_now(&mut w, T0 + 7.0 + 14.0);
    assert!(qm::can_solve(&w, &owner, "ShortTimer"));

    // ColoArena quests ignore the rate (MinDelta 3600).
    qm::update(&mut w, &mut owner, "ColoArenaKeyTimer");
    let at = w.now.unix_time;
    set_now(&mut w, at + 3600.0);
    assert!(qm::can_solve(&w, &owner, "ColoArenaKeyTimer"));
    assert!(pm::modify_double(&w, "quest_mindelta_rate", 1.0, true));

    // MaxSolves 5: the fifth solve caps it; further updates change nothing.
    for _ in 0..3 {
        qm::update(&mut w, &mut owner, "ShortTimer");
    }
    assert_eq!(qm::get_current_solves(&w, &owner, "ShortTimer"), 5);
    assert!(qm::is_max_solves(&w, &owner, "ShortTimer"));
    let before = character(&w).get_quest("ShortTimer").cloned().unwrap();
    set_now(&mut w, T0 + 1_000_000.0);
    qm::update(&mut w, &mut owner, "ShortTimer");
    assert_eq!(
        character(&w).get_quest("ShortTimer").cloned().unwrap(),
        before,
        "IsMaxSolves: not updated"
    );
    assert!(!qm::can_solve(&w, &owner, "ShortTimer"));
    assert_eq!(
        qm::get_next_solve_time(&w, &owner, "ShortTimer"),
        TimeSpan::MAX_VALUE
    );

    // Decrement and Erase work on the Character; erase marks changes only when a row went.
    qm::decrement(&mut w, &mut owner, "ShortTimer", 2);
    assert_eq!(qm::get_current_solves(&w, &owner, "ShortTimer"), 3);
    clear_changes(&mut w);
    qm::erase(&mut w, &mut owner, "NotThere");
    assert!(!changes_detected(&w));
    qm::erase(&mut w, &mut owner, "SHORTTIMER");
    assert!(changes_detected(&w));
    assert!(!qm::has_quest(&w, &owner, "ShortTimer"));
    qm::erase_all(&mut w, &mut owner);
    assert!(character(&w).character_properties_quest_registry.is_empty());
}

/// `HandleSolveError`: too many solves, or too recently with the friendly remaining time;
/// `HandlePortalQuestError` and `HandleNoQuestError`.
#[test]
fn solve_errors_answer_the_player_as_ace_does() {
    let mut w = world();
    let mut owner = me();
    qm::stamp(&mut w, &mut owner, "DailyRun");
    set_now(&mut w, T0 + 3661.0);

    start_capture();
    qm::handle_solve_error(&mut w, &owner, "DailyRun");
    let sent = take_sent();
    assert_eq!(kinds(&sent), [SYSTEM_CHAT, SYSTEM_CHAT, SAVE_FAILED]);
    assert_eq!(
        chats(&sent),
        [
            "You have solved this quest too recently!",
            "You may complete this quest again in 22h 58m 59s."
        ]
    );
    assert_eq!(u32_at(&sent[2].2, 16), 0, "item guid 0");
    assert_eq!(
        u32_at(&sent[2].2, 20),
        we(WeenieError::YouHaveSolvedThisQuestTooRecently)
    );

    qm::stamp(&mut w, &mut owner, "OnceOnly");
    start_capture();
    qm::handle_solve_error(&mut w, &owner, "OnceOnly");
    let sent = take_sent();
    assert_eq!(kinds(&sent), [SYSTEM_CHAT, SAVE_FAILED]);
    assert_eq!(chats(&sent), ["You have solved this quest too many times!"]);
    assert_eq!(
        u32_at(&sent[1].2, 20),
        we(WeenieError::YouHaveSolvedThisQuestTooManyTimes)
    );

    // Portal: no quest -> YouMustCompleteQuestToUsePortal; a solvable (expired) flag -> the
    // chat line then QuestSolvedTooLongAgo; a flag still running -> nothing.
    start_capture();
    qm::handle_portal_quest_error(&mut w, &owner, "Weekly");
    set_now(&mut w, T0 + 86_400.0 * 2.0);
    qm::handle_portal_quest_error(&mut w, &owner, "DailyRun");
    qm::stamp(&mut w, &mut owner, "DailyRun");
    qm::handle_portal_quest_error(&mut w, &owner, "DailyRun");
    let sent = take_sent();
    assert_eq!(kinds(&sent), [WEENIE_ERROR, SYSTEM_CHAT, WEENIE_ERROR]);
    assert_eq!(
        u32_at(&sent[0].2, 16),
        we(WeenieError::YouMustCompleteQuestToUsePortal)
    );
    assert_eq!(
        chat(&sent[1].2),
        (
            "You completed the quest this portal requires too long ago!".to_owned(),
            we_chat(ChatMessageType::Magic)
        )
    );
    assert_eq!(
        u32_at(&sent[2].2, 16),
        we(WeenieError::QuestSolvedTooLongAgo)
    );

    start_capture();
    qm::handle_no_quest_error(&mut w, &owner, ObjectGuid::new(RAT));
    let sent = take_sent();
    assert_eq!(kinds(&sent), [SAVE_FAILED]);
    assert_eq!(
        (u32_at(&sent[0].2, 16), u32_at(&sent[0].2, 20)),
        (RAT, we(WeenieError::ItemRequiresQuestToBePickedUp))
    );

    // A non-player owner answers nothing.
    start_capture();
    qm::handle_solve_error(
        &mut w,
        &QuestOwner::Creature(ObjectGuid::new(CHICKEN)),
        "DailyRun",
    );
    assert!(take_sent().is_empty());
}

/// `HandleKillTask`: only a started task counts; each kill stamps it and tells the player the
/// count, until MaxSolves completes the task.
#[test]
fn a_kill_task_counts_kills_and_completes() {
    let mut w = world();
    let mut owner = me();
    let rat = Some(ObjectGuid::new(RAT));

    start_capture();
    qm::handle_kill_task(&mut w, &mut owner, "KillTaskRats", rat);
    assert!(take_sent().is_empty(), "not started: nothing");
    assert!(!qm::has_quest(&w, &owner, "KillTaskRats"));

    // The NPC starts the task at 0 kills (SetQuestCompletions).
    qm::set_quest_completions(&mut w, &mut owner, "KillTaskRats", 0);
    start_capture();
    for _ in 0..12 {
        qm::handle_kill_task(&mut w, &mut owner, "KillTaskRats@rat", rat);
    }
    let lines = chats(&take_sent());
    let mut expected: Vec<String> = (1..10)
        .map(|n| format!("You have killed {n} Rats! You must kill 10 to complete your task."))
        .collect();
    expected.push("You have killed 10 Rats! Your task is complete!".to_owned());
    // Past MaxSolves the stamp is refused, and the count stays at 10.
    expected.push("You have killed 10 Rats! Your task is complete!".to_owned());
    expected.push("You have killed 10 Rats! Your task is complete!".to_owned());
    assert_eq!(lines, expected);

    // A task the world database lacks, or no creature, answers nothing.
    start_capture();
    qm::handle_kill_task(&mut w, &mut owner, "NotInDb", rat);
    qm::handle_kill_task(&mut w, &mut owner, "KillTaskRats", None);
    assert!(take_sent().is_empty());
}

/// A fellowship's manager: `Name` is `Fellowship(<name>)`, rows carry id 1, and nothing reaches a
/// player.
#[test]
fn a_fellowship_quest_manager_keeps_its_own_runtime_quests() {
    let mut w = world();
    let mut fellowship = QuestManager::new_fellowship("Adventurers");
    let mut owner = QuestOwner::Fellowship(&mut fellowship);
    assert_eq!(qm::name(&w, &owner), "Fellowship(Adventurers)");
    qm::stamp(&mut w, &mut owner, "DailyRun");
    qm::set_quest_bits(&mut w, &mut owner, "BitFlags", 0b101, true);
    assert!(qm::has_quest_bits(&w, &owner, "BitFlags", 0b100));
    assert!(qm::has_no_quest_bits(&w, &owner, "BitFlags", 0b010));
    let rows = qm::get_quests(&w, &owner);
    assert_eq!(
        rows.iter()
            .map(|r| (r.quest_name.as_str(), r.character_id, r.num_times_completed))
            .collect::<Vec<_>>(),
        [("DailyRun", 1, 1), ("BitFlags", 1, 5)]
    );
    assert!(character(&w).character_properties_quest_registry.is_empty());
    assert_eq!(
        qm::name(&w, &QuestOwner::Creature(ObjectGuid::new(CHICKEN))),
        "Chicken"
    );
}

// ------------------------------------------------------------------ ContractManager

/// `Add`, the tracker it sends, `NotifyOfQuestUpdate` resending it on a watched quest's update
/// (case-insensitively), `Abandon` sending it with `DeleteContract`, and the 100-contract cap.
#[test]
fn contracts_add_track_quest_updates_and_abandon() {
    let mut w = world();
    let mut owner = me();

    assert!(
        !cm::add(&mut w, player(), 12345),
        "a contract the dat lacks"
    );

    start_capture();
    assert!(cm::add(&mut w, player(), 7));
    assert!(changes_detected(&w));
    assert!(
        cm::add(&mut w, player(), 7),
        "a duplicate succeeds without a second row"
    );
    let sent = take_sent();
    assert_eq!(
        kinds(&sent),
        [CONTRACT_TRACKER],
        "one tracker, for the new row only"
    );
    assert_eq!(
        tracker(&sent[0].2),
        (7, 1, 0.0, 0.0, 0),
        "Available, not deleted"
    );
    assert_eq!(character(&w).get_contracts_count(), 1);
    assert!(cm::has_contract(&w, player(), 7));

    // RatHuntStarted is watched: its stamp resends contract 7 (now InProgress). DailyRun is not
    // watched by any contract the player has.
    start_capture();
    qm::stamp(&mut w, &mut owner, "rathuntstarted");
    qm::stamp(&mut w, &mut owner, "DailyRun");
    let sent = take_sent();
    assert_eq!(kinds(&sent), [CONTRACT_TRACKER]);
    assert_eq!(tracker(&sent[0].2).1, 2, "InProgress");

    // RatHunt is the progress flag: stage ProgressCounter + progress. RatHunt is not in the quest
    // table, so GetMaxSolves answers 0 and SetQuestCompletions stores Min(3, 0) = 0: the stamp
    // makes it 1 (ACE's behaviour for a quest the world database lacks).
    qm::set_quest_completions(&mut w, &mut owner, "RatHunt", 3);
    assert_eq!(qm::get_current_solves(&w, &owner, "RatHunt"), 0);
    start_capture();
    assert!(cm::add(&mut w, player(), 9));
    qm::stamp(&mut w, &mut owner, "RatHunt");
    let sent = take_sent();
    assert_eq!(kinds(&sent), [CONTRACT_TRACKER, CONTRACT_TRACKER]);
    assert_eq!(tracker(&sent[0].2).0, 9);
    assert_eq!(
        (tracker(&sent[1].2).0, tracker(&sent[1].2).1),
        (7, 5),
        "contract 7 at ProgressCounter + 1"
    );

    // The table the login sends: header (count 2, 32 buckets), then the trackers by bucket.
    let mut table = Vec::new();
    cm::write(&mut table, &w, player());
    assert_eq!(
        (
            u16::from_le_bytes([table[0], table[1]]),
            u16::from_le_bytes([table[2], table[3]])
        ),
        (2, 32)
    );
    assert_eq!((u32_at(&table, 4), u32_at(&table, 36)), (7, 9));

    // Abandon: the tracker again, with DeleteContract; the row and its watch are gone.
    start_capture();
    empyrean_world::world_objects::player_contracts::handle_action_abandon_contract(
        &mut w,
        player(),
        7,
    );
    qm::stamp(&mut w, &mut owner, "RatHuntStarted");
    let sent = take_sent();
    assert_eq!(kinds(&sent), [CONTRACT_TRACKER]);
    assert_eq!(tracker(&sent[0].2).4, 1, "DeleteContract");
    assert!(!cm::has_contract(&w, player(), 7));

    // 100 contracts is full: the 101st is refused with ACE's chat line.
    let rows = &mut w
        .objects
        .get_mut(player())
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player
        .character
        .as_mut()
        .unwrap()
        .character_properties_contract_registry;
    for id in 1000..1099 {
        rows.push(CharacterPropertiesContractRegistry {
            contract_id: id,
            ..Default::default()
        });
    }
    assert!(cm::is_full(&w, player()));
    start_capture();
    assert!(!cm::add(&mut w, player(), 40));
    assert_eq!(
        chats(&take_sent()),
        ["You currently have the maximum amount of contracts for this character and cannot take on another! You must abandon at least one contract before you can accept the contract for Nothing."]
    );
}

// ------------------------------------------------------------------ Player_Character

/// `GetCharacterOption`/`SetCharacterOption` over both option words, and `SetAppearOffline`.
#[test]
fn character_options_read_and_write_both_words() {
    let mut w = world();
    assert!(!pc::get_character_option(
        &w,
        player(),
        CharacterOption::AppearOffline
    ));
    pc::set_character_option(&mut w, player(), CharacterOption::AppearOffline, true);
    assert!(pc::get_appear_offline(&w, player()));
    assert!(changes_detected(&w));
    let appear_offline_bit = CharacterOption::AppearOffline
        .character_options2()
        .expect("AppearOffline is an options-2 bit")
        .0
        .cast_signed();
    assert_eq!(character(&w).character_options_2, appear_offline_bit);
    let option1 = CharacterOption::ALL
        .iter()
        .copied()
        .find(|o| o.character_options1().is_some())
        .expect("an options-1 option");
    let bit1 = option1.character_options1().unwrap().0.cast_signed();
    let before = character(&w).character_options_1;
    let flipped = !pc::get_character_option(&w, player(), option1);
    pc::set_character_option(&mut w, player(), option1, flipped);
    assert_eq!(character(&w).character_options_1, before ^ bit1);
    pc::set_character_option(&mut w, player(), option1, true);
    pc::set_character_option(&mut w, player(), option1, false);
    assert_eq!(
        character(&w).character_options_1 & bit1,
        0,
        "cleared, the other bits kept"
    );
    pc::set_character_options1(&mut w, player(), 0);
    assert!(!pc::get_character_option(&w, player(), option1));
    pc::set_appear_offline(&mut w, player(), false);
    assert_eq!(character(&w).character_options_2, 0);
    assert!(!pc::get_appear_offline(&w, player()));
    pc::set_character_gameplay_options(&mut w, player(), vec![1, 2, 3]);
    assert_eq!(
        character(&w).gameplay_options.as_deref(),
        Some(&[1u8, 2, 3][..])
    );
}

/// Titles: an unknown title is ignored; a new one is registered (NumCharacterTitles), and once
/// the player has entered the world the client hears of it; `GetTitle` reads the dat strings.
#[test]
fn titles_register_display_and_notify() {
    let mut w = world();
    pc::add_title(&mut w, player(), 999_999, true);
    assert!(
        character(&w).character_properties_title_book.is_empty(),
        "not a CharacterTitle"
    );

    start_capture();
    pc::add_title_enum(&mut w, player(), CharacterTitle::Adventurer, false);
    assert!(
        take_sent().is_empty(),
        "before FirstEnterWorldDone nothing is sent"
    );
    assert_eq!(
        w.objects.get(player()).unwrap().num_character_titles(),
        Some(1)
    );

    w.objects
        .get_mut(player())
        .unwrap()
        .set_first_enter_world_done(true);
    start_capture();
    pc::handle_action_set_title(&mut w, player(), CharacterTitle::Archer.0);
    pc::set_title(&mut w, player(), CharacterTitle::Archer);
    let sent = take_sent();
    assert_eq!(
        kinds(&sent),
        [UPDATE_TITLE, TRANSIENT],
        "the second SetTitle changes nothing"
    );
    assert_eq!(
        (u32_at(&sent[0].2, 16), u32_at(&sent[0].2, 20)),
        (CharacterTitle::Archer.0, 1)
    );
    assert_eq!(
        w.objects.get(player()).unwrap().character_title_id(),
        Some(2)
    );
    assert_eq!(
        w.objects.get(player()).unwrap().num_character_titles(),
        Some(2)
    );

    assert_eq!(
        pc::get_title(&w, CharacterTitle::Adventurer).as_deref(),
        Some("Adventurer")
    );
    assert_eq!(pc::get_title(&w, CharacterTitle::Archer), None);
}

/// Shortcuts and spell bars go through the Character's extensions (1-based storage).
#[test]
fn shortcuts_and_spell_bars() {
    let mut w = world();
    let sc = |index: i32, object: u32| dereth_protocol::login::ShortCutData {
        index,
        object_id: dereth_primitives::ObjectId(object),
        spell_id: 0,
    };
    pc::handle_action_add_shortcut(&mut w, player(), sc(0, 0x8000_0001));
    pc::handle_action_add_shortcut(&mut w, player(), sc(3, 0x8000_0002));
    let shortcuts = pc::get_shortcuts(&w, player());
    assert_eq!(
        shortcuts
            .iter()
            .map(|s| (s.index, s.object_id))
            .collect::<Vec<_>>(),
        [(0, 0x8000_0001), (3, 0x8000_0002)]
    );
    clear_changes(&mut w);
    pc::handle_action_remove_shortcut(&mut w, player(), 5);
    assert!(!changes_detected(&w), "nothing removed");
    pc::handle_action_remove_shortcut(&mut w, player(), 0);
    assert!(changes_detected(&w));
    assert_eq!(pc::get_shortcuts(&w, player()).len(), 1);

    // An unknown spell, bar 8 and a slot past NumSpells are refused.
    pc::handle_action_add_spell_favorite(&mut w, player(), 1, 0, 0);
    assert!(pc::get_spells_in_spell_bar(&w, player(), 0).is_empty());
    w.objects
        .get_mut(player())
        .unwrap()
        .biota
        .properties_spell_book
        .get_or_insert_with(Default::default)
        .insert(1, 2.0);
    w.objects
        .get_mut(player())
        .unwrap()
        .biota
        .properties_spell_book
        .as_mut()
        .unwrap()
        .insert(2, 2.0);
    pc::handle_action_add_spell_favorite(&mut w, player(), 1, 0, 8);
    pc::handle_action_add_spell_favorite(&mut w, player(), 1, 8193, 0);
    assert!(pc::get_spells_in_spell_bar(&w, player(), 0).is_empty());
    assert!(
        pc::get_spells_in_spell_bar(&w, player(), 8).is_empty(),
        "bar 8 refused"
    );
    pc::handle_action_add_spell_favorite(&mut w, player(), 1, 0, 7);
    assert_eq!(
        pc::get_spells_in_spell_bar(&w, player(), 7).len(),
        1,
        "bar 7 is the last"
    );
    pc::handle_action_remove_spell_favorite(&mut w, player(), 1, 7);
    pc::handle_action_add_spell_favorite(&mut w, player(), 1, 0, 0);
    pc::handle_action_add_spell_favorite(&mut w, player(), 2, 0, 0);
    let bar = pc::get_spells_in_spell_bar(&w, player(), 0);
    assert_eq!(
        bar.iter()
            .map(|s| (s.spell_bar_id, s.spell_bar_position_id, s.spell_id))
            .collect::<Vec<_>>(),
        [(0, 0, 2), (0, 1, 1)]
    );
    pc::handle_action_remove_spell_favorite(&mut w, player(), 2, 0);
    assert_eq!(
        pc::get_spells_in_spell_bar(&w, player(), 0)
            .iter()
            .map(|s| (s.spell_bar_position_id, s.spell_id))
            .collect::<Vec<_>>(),
        [(0, 1)]
    );
}

/// The barber's FinishBarber payload: 16 dwords (the last two the Empyrean levitation option
/// and one unknown).
fn barber_payload(values: [u32; 16]) -> Vec<u8> {
    // The opcode (the handler's payload starts after it), then the dwords.
    [0x0000_0000u32]
        .iter()
        .chain(values.iter())
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

fn finish_barber(w: &mut World, values: [u32; 16]) {
    let bytes = barber_payload(values);
    let mut payload =
        empyrean_world::network::managers::inbound_message_manager::Payload::new(&bytes);
    pc::handle_action_finish_barber(w, player(), &mut payload).expect("a whole payload");
}

/// A valid appearance: every requested id is one the heritage's CharGen lists.
const VALID: [u32; 16] = [
    0x0400_007E, // palette base
    0x0100_0100, // head object (the hair style's part)
    0x0500_1001, // hair texture (new)
    0x0500_1000, // default hair texture (old)
    0x0500_2001, // eyes (new)
    0x0500_2000, // default eyes (old)
    0x0500_3001, // nose
    0x0500_3000, // default nose
    0x0500_4001, // mouth
    0x0500_4000, // default mouth
    0x0400_0101, // skin palette (in palette set 0x0F000001)
    0x0400_0201, // hair palette (in palette set 0x0F000002)
    0x0400_0300, // eyes palette
    0x0200_0001, // setup (the sex's)
    1,           // option_bound
    0,
];

/// `StartBarber`/`HandleActionFinishBarber`: a valid request changes the appearance and the
/// Character's hair textures and broadcasts the ObjDesc; an invalid one changes nothing; an
/// Empyrean's levitation option picks its motion table; without an active barber nothing runs.
#[test]
fn the_barber_validates_and_applies_the_appearance() {
    let mut w = world();
    w.objects
        .get_mut(player())
        .unwrap()
        .set_heritage(Some(HeritageGroup::Aluvian.0));
    w.objects.get_mut(player()).unwrap().set_gender(Some(1));

    finish_barber(&mut w, VALID);
    assert_eq!(
        w.objects.get(player()).unwrap().palette_base_id(),
        None,
        "no barber: ignored"
    );

    start_capture();
    pc::start_barber(&mut w, player());
    assert!(w.objects.get(player()).unwrap().barber_active());
    let mut invalid = VALID;
    invalid[12] = 0x0400_0999; // an eye palette the CharGen does not list
    finish_barber(&mut w, invalid);
    assert!(
        !w.objects.get(player()).unwrap().barber_active(),
        "an invalid request ends the session"
    );
    assert_eq!(w.objects.get(player()).unwrap().eyes_palette_did(), None);

    pc::start_barber(&mut w, player());
    finish_barber(&mut w, VALID);
    let o = w.objects.get(player()).unwrap();
    assert!(!o.barber_active());
    assert_eq!(
        (
            o.palette_base_id(),
            o.head_object_did(),
            o.eyes_texture_did(),
            o.default_mouth_texture_did(),
            o.skin_palette_did(),
            o.hair_palette_did(),
            o.setup_table_id()
        ),
        (
            Some(0x0400_007E),
            Some(0x0100_0100),
            Some(0x0500_2001),
            Some(0x0500_4000),
            Some(0x0400_0101),
            Some(0x0400_0201),
            0x0200_0001
        )
    );
    assert_eq!(
        (
            character(&w).hair_texture,
            character(&w).default_hair_texture
        ),
        (0x0500_1001, 0x0500_1000)
    );
    // (The ObjDesc broadcast needs a physics body; the TestServer scenario sees it.)

    // Empyrean male with the levitation option off: the float motion table, sent privately.
    w.objects
        .get_mut(player())
        .unwrap()
        .set_heritage(Some(HeritageGroup::Empyrean.0));
    pc::start_barber(&mut w, player());
    let mut float = VALID;
    float[14] = 0;
    start_capture();
    finish_barber(&mut w, float);
    let sent = take_sent();
    assert_eq!(
        w.objects.get(player()).unwrap().motion_table_id(),
        0x0900_020B
    );
    let update = sent
        .iter()
        .find(|(_, _, b)| u32_at(b, 0) == PRIVATE_DATA_ID)
        .expect("the motion table update");
    assert_eq!(
        (u32_at(&update.2, 5), u32_at(&update.2, 9)),
        (u32::from(PropertyDataId::MotionTable.0), 0x0900_020B)
    );
}

/// ACE-BUG (`ValidateHairTexture`): hair texture 0 with no valid hair style, from a heritage
/// with hair, dereferences the null style.
#[test]
fn the_barber_null_hair_style_throws_as_ace_does() {
    let mut w = world();
    w.objects
        .get_mut(player())
        .unwrap()
        .set_heritage(Some(HeritageGroup::Aluvian.0));
    w.objects.get_mut(player()).unwrap().set_gender(Some(1));
    pc::start_barber(&mut w, player());
    let mut bad = VALID;
    bad[1] = 0x0100_0999; // no such hair style
    bad[2] = 0;
    let r = catch_unwind(AssertUnwindSafe(|| finish_barber(&mut w, bad)));
    let msg = r
        .expect_err("NullReferenceException")
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(msg.contains("NullReferenceException"), "{msg}");
}

// ------------------------------------------------------------------ the whole server

mod scenario {
    use super::*;

    use dereth_primitives::NetQueue;
    use dereth_primitives::ObjectId;
    use dereth_protocol::comms::{CommunicationTextboxString, CommunicationTransientString};
    use dereth_protocol::events::split_ui_blob;
    use dereth_protocol::login::{
        CharacterAddShortCut, LoginExecuteLogOff, LoginExecuteLogOffRequest, LoginSendEnterWorld,
        LoginSendEnterWorldRequest, ShortCutData,
    };
    use dereth_protocol::social::{
        SocialAbandonContract, SocialAddFriend, SocialAddOrSetCharacterTitle, SocialClearFriends,
        SocialFriendsUpdate, SocialRemoveFriend, SocialSendClientContractTracker,
        SocialSendClientContractTrackerTable, SocialSetDisplayCharacterTitle,
    };
    use dereth_protocol::Message;
    use empyrean_entity::enums::{AccessLevel, PositionType};
    use empyrean_entity::Biota;
    use empyrean_net::SessionState;
    use empyrean_testkit::land;
    use empyrean_testkit::{ClientId, TestServer};
    use empyrean_world::managers::player_manager;
    use empyrean_world::managers::world_manager::WorldStatusState;
    use empyrean_world::physics::phys_ext;

    const ACCOUNT: &str = "acct";
    const ME: u32 = 0x5000_0001;
    const FRIEND: u32 = 0x5000_0002;
    const PLAYER_SETUP: u32 = 0x0200_0001;
    const START: u16 = 0xA9B4;
    const HEIGHT: u8 = 47;

    fn blocks() -> Vec<u16> {
        let mut v = Vec::new();
        for dx in [-1i32, 0, 1] {
            for dy in [-1i32, 0, 1] {
                let x = u16::try_from(i32::from(START >> 8) + dx).expect("on the map");
                let y = u16::try_from(i32::from(START & 0xFF) + dy).expect("on the map");
                v.push(x << 8 | y);
            }
        }
        v
    }

    fn character_biota(id: u32, name: &str) -> Biota {
        let mut b = Biota {
            id,
            weenie_class_id: PLAYER_WCID,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        b.set_property(PropertyDataId::CombatTable, 0x3000_0000);
        b.set_property(PropertyString::Name, name.to_owned());
        b.set_property(PropertyDataId::Setup, PLAYER_SETUP);
        b.set_property(PropertyInt::HeritageGroup, 1);
        b.set_property(PropertyInt::ItemsCapacity, 102);
        b.set_property(PropertyInt::ContainersCapacity, 7);
        b.set_property_position(
            PositionType::Location,
            empyrean_entity::models::PropertiesPosition {
                obj_cell_id: 0xA9B4_0019,
                position_x: 84.0,
                position_y: 7.1,
                position_z: 94.005,
                rotation_w: 1.0,
                ..Default::default()
            },
        );
        b.properties_enchantment_registry = Some(Vec::new());
        b
    }

    /// A server with two characters on one account (Aldric, who plays, and Bryn, offline), and a
    /// bot at character select.
    fn server() -> (TestServer, ClientId) {
        let setup = |w: &mut World| {
            let account_id = w
                .auth
                .lock()
                .create_account(
                    ACCOUNT,
                    "pw",
                    AccessLevel::Player,
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                )
                .expect("created")
                .account_id;
            for (id, name) in [(ME, "Aldric"), (FRIEND, "Bryn")] {
                let mut biota = character_biota(id, name);
                let character = Character {
                    id,
                    account_id,
                    name: name.to_owned(),
                    ..Default::default()
                };
                assert!(w.shard.base_database().add_character_in_parallel(
                    &mut biota,
                    &mut Vec::new(),
                    &character
                ));
            }
        };
        let mut ts = TestServer::with_setup(dats(), setup);
        ts.world.content = Arc::new(content());
        ts.world.world_manager.world_status = WorldStatusState::Open;
        guid_manager::initialize(&mut ts.world, &mut EmptyShard);
        land::use_flat_land_with_setup(
            &mut ts.world,
            &blocks(),
            HEIGHT,
            PLAYER_SETUP,
            land::test_setup_geometry(),
        );
        phys_ext::register_setup(&mut ts.world, land::TEST_SETUP, land::test_setup_geometry());
        let id = ts.connect(ACCOUNT, "pw");
        let session = ts
            .world
            .sessions
            .iter()
            .map(|(s, _)| s)
            .next()
            .expect("session");
        assert!(ts.run_until(1.0, |ts| ts
            .world
            .sessions
            .get(session)
            .is_some_and(|s| s.state == SessionState::AuthConnected)));
        (ts, id)
    }

    fn enter(ts: &mut TestServer, id: ClientId) {
        ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
        ts.advance(0.1);
        ts.send_message(
            id,
            NetQueue::Logon,
            &LoginSendEnterWorld {
                character: ObjectId(ME),
                account: ACCOUNT.to_owned(),
            },
        );
        ts.advance(0.5);
        assert!(
            player_manager::get_online_player(&ts.world, ME).is_some(),
            "in the world"
        );
        ts.world
            .objects
            .get_mut(ObjectGuid::new(ME))
            .unwrap()
            .set_first_enter_world_done(true);
    }

    fn me_character(ts: &TestServer) -> Character {
        ts.world
            .objects
            .get(ObjectGuid::new(ME))
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player
            .character
            .clone()
            .unwrap()
    }

    /// The messages of type `M` (a game event through its `0xF7B0` wrapper, or a plain message)
    /// received from index `from` on.
    fn got<M: Message>(ts: &TestServer, id: ClientId, from: usize) -> Vec<M> {
        ts.received_raw(id)[from..]
            .iter()
            .filter_map(|m| {
                let mut blob = m.opcode.to_le_bytes().to_vec();
                blob.extend_from_slice(&m.body);
                let kind = if m.opcode == GAME_EVENT {
                    u32::from_le_bytes(m.body[8..12].try_into().unwrap())
                } else {
                    m.opcode
                };
                (kind == M::OPCODE.0).then(|| {
                    let split = split_ui_blob(&blob).expect("a blob");
                    let mut body = split.body;
                    M::read(&mut body).unwrap_or_else(|e| panic!("0x{kind:04X} decodes: {e:?}"))
                })
            })
            .collect()
    }

    fn texts(ts: &TestServer, id: ClientId, from: usize) -> Vec<String> {
        got::<CommunicationTextboxString>(ts, id, from)
            .into_iter()
            .map(|t| t.text)
            .collect()
    }

    /// Quests stamped by the world and the social game actions over the wire, then a log out and
    /// back in: the quest registry, friends, title, shortcut and contracts come back from the
    /// shard, and the login sends the contract tracker table.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn quests_friends_titles_and_contracts_survive_a_log_out_and_back_in() {
        let (mut ts, id) = server();
        enter(&mut ts, id);
        let me = ObjectGuid::new(ME);

        // An NPC's emotes would stamp these and hand out the contracts.
        let mark = ts.received_raw(id).len();
        qm::stamp(
            &mut ts.world,
            &mut QuestOwner::Creature(me),
            "DailyRun@first",
        );
        qm::set_quest_bits(
            &mut ts.world,
            &mut QuestOwner::Creature(me),
            "BitFlags",
            0b101,
            true,
        );
        assert!(cm::add(&mut ts.world, me, 9));
        assert!(cm::add(&mut ts.world, me, 7));
        ts.advance(0.2);
        let trackers = got::<SocialSendClientContractTracker>(&ts, id, mark);
        assert_eq!(
            trackers
                .iter()
                .map(|t| (
                    t.tracker.contract_id,
                    t.tracker.contract_stage,
                    t.delete_contract
                ))
                .collect::<Vec<_>>(),
            [(9, 2, 0), (7, 1, 0)],
            "contract 9's stamped flag (DailyRun) is set: InProgress"
        );

        // Friends by name: the friend, yourself, nobody, a duplicate.
        let mark = ts.received_raw(id).len();
        for name in ["bryn", "ALDRIC", "Nobody", "Bryn"] {
            ts.send_game_action(
                id,
                &SocialAddFriend {
                    name: name.to_owned(),
                },
            );
        }
        ts.advance(0.3);
        let updates = got::<SocialFriendsUpdate>(&ts, id, mark);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].update_type, 1, "FriendAdded");
        assert_eq!(
            (
                updates[0].friends[0].id,
                updates[0].friends[0].online,
                updates[0].friends[0].name.as_str()
            ),
            (ObjectId(FRIEND), 0, "Bryn")
        );
        assert_eq!(
            texts(&ts, id, mark),
            [
                "Bryn has been added to your friends list.",
                "Sorry, but you can't be friends with yourself.",
                "That character does not exist",
                "That character is already in your friends list",
            ]
        );

        // Remove, then add again (it is kept for the log in).
        let mark = ts.received_raw(id).len();
        ts.send_game_action(
            id,
            &SocialRemoveFriend {
                friend_id: ObjectId(FRIEND),
            },
        );
        ts.send_game_action(
            id,
            &SocialRemoveFriend {
                friend_id: ObjectId(FRIEND),
            },
        );
        ts.send_game_action(
            id,
            &SocialAddFriend {
                name: "Bryn".to_owned(),
            },
        );
        ts.advance(0.3);
        assert_eq!(
            got::<SocialFriendsUpdate>(&ts, id, mark)
                .iter()
                .map(|u| u.update_type)
                .collect::<Vec<_>>(),
            [2, 1]
        );
        assert_eq!(
            texts(&ts, id, mark),
            [
                "Bryn has been removed from your friends list.",
                "That character is not in your friends list!",
                "Bryn has been added to your friends list."
            ]
        );

        // A title, a shortcut, and abandoning contract 7.
        let mark = ts.received_raw(id).len();
        ts.send_game_action(
            id,
            &SocialSetDisplayCharacterTitle {
                title_id: CharacterTitle::Adventurer.0,
            },
        );
        ts.send_game_action(
            id,
            &CharacterAddShortCut {
                shortcut: ShortCutData {
                    index: 2,
                    object_id: ObjectId(ME),
                    spell_id: 0,
                },
            },
        );
        ts.send_game_action(id, &SocialAbandonContract { contract_id: 7 });
        ts.advance(0.3);
        assert_eq!(
            got::<SocialAddOrSetCharacterTitle>(&ts, id, mark),
            [SocialAddOrSetCharacterTitle {
                new_title: 1,
                set_as_display_title: 1
            }]
        );
        assert_eq!(
            got::<CommunicationTransientString>(&ts, id, mark).len(),
            1,
            "You have been granted a new title."
        );
        let trackers = got::<SocialSendClientContractTracker>(&ts, id, mark);
        assert_eq!(
            trackers
                .iter()
                .map(|t| (t.tracker.contract_id, t.delete_contract))
                .collect::<Vec<_>>(),
            [(7, 1)]
        );
        let before = me_character(&ts);

        // Log off and back on.
        ts.send_message(
            id,
            NetQueue::Logon,
            &LoginExecuteLogOffRequest {
                character: ObjectId(ME),
            },
        );
        assert!(
            ts.run_until(3.0, |ts| player_manager::get_online_player(&ts.world, ME)
                .is_none()),
            "offline"
        );
        let saved = ts
            .world
            .shard
            .base_database()
            .get_character(ME)
            .expect("the character");
        assert_eq!(
            saved.character_properties_quest_registry.len(),
            2,
            "the quests were saved with the Character"
        );
        assert!(
            ts.run_until(10.0, |ts| !ts.received::<LoginExecuteLogOff>(id).is_empty()),
            "logged off"
        );
        ts.advance(0.2);
        let mark = ts.received_raw(id).len();
        enter(&mut ts, id);

        let back = me_character(&ts);
        let quests = |c: &Character| {
            let mut q: Vec<(String, u32, i32)> = c
                .character_properties_quest_registry
                .iter()
                .map(|q| {
                    (
                        q.quest_name.clone(),
                        q.last_time_completed,
                        q.num_times_completed,
                    )
                })
                .collect();
            q.sort();
            q
        };
        assert_eq!(quests(&back), quests(&before));
        assert_eq!(
            quests(&back)
                .iter()
                .map(|q| (q.0.as_str(), q.2))
                .collect::<Vec<_>>(),
            [("BitFlags", 5), ("DailyRun", 1)]
        );
        assert_eq!(
            back.character_properties_friend_list
                .iter()
                .map(|f| f.friend_id)
                .collect::<Vec<_>>(),
            [FRIEND]
        );
        assert_eq!(
            back.character_properties_title_book
                .iter()
                .map(|t| t.title_id)
                .collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(
            back.character_properties_shortcut_bar
                .iter()
                .map(|s| (s.shortcut_bar_index, s.shortcut_object_id))
                .collect::<Vec<_>>(),
            [(3, ME)]
        );
        assert_eq!(
            back.character_properties_contract_registry
                .iter()
                .map(|c| c.contract_id)
                .collect::<Vec<_>>(),
            [9]
        );
        assert_eq!(
            ts.world.objects.get(me).unwrap().character_title_id(),
            Some(1)
        );
        let owner = QuestOwner::Creature(me);
        assert!(
            qm::has_quest(&ts.world, &owner, "dailyrun")
                && !qm::can_solve(&ts.world, &owner, "DailyRun"),
            "still on its timer"
        );

        // The login's contract tracker table: contract 9, InProgress.
        let tables = got::<SocialSendClientContractTrackerTable>(&ts, id, mark);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].0.table_size, 32);
        assert_eq!(
            tables[0]
                .0
                .entries
                .iter()
                .map(|(k, t)| (*k, t.contract_id, t.contract_stage))
                .collect::<Vec<_>>(),
            [(9, 9, 2)]
        );

        // Clearing the friends list answers nothing and empties it.
        let mark = ts.received_raw(id).len();
        ts.send_game_action(id, &SocialClearFriends);
        ts.advance(0.2);
        assert!(me_character(&ts)
            .character_properties_friend_list
            .is_empty());
        assert!(got::<SocialFriendsUpdate>(&ts, id, mark).is_empty());
    }
}

mod kill_tasks_and_portals {
    use crate::support::quest_world::*;

    /// `Creature.OnDeath_HandleKillTask` over 5.6's `QuestManager`: a player damager without the task
    /// gets nothing; once an NPC has given it (at 0 kills), the kill stamps it and the player is told
    /// the count (`HandleKillTask`), `GetQuestName` stripping the `@` comment of the creature's
    /// `KillQuest`.
    #[test]
    fn a_real_kill_counts_toward_the_players_kill_task() {
        let mut w = world();
        let rat = ObjectGuid::new(RAT);
        give_health(&mut w, rat, 20);
        damage_history::add(&mut w, rat, player(), DamageType::Slash, 5);

        start_capture();
        creature_death::on_death_handle_kill_task(&mut w, rat, "KillTaskRats@rat");
        assert!(take_sent().is_empty(), "no task: nothing");
        assert!(!qm::has_quest(&w, &me(), "KillTaskRats"));

        qm::set_quest_completions(&mut w, &mut me(), "KillTaskRats", 0);
        start_capture();
        creature_death::on_death_handle_kill_task(&mut w, rat, "KillTaskRats@rat");
        assert_eq!(
            chats(&take_sent()),
            ["You have killed 1 Rats! You must kill 10 to complete your task."]
        );
        assert_eq!(
            qm::get_quest(&w, &me(), "KillTaskRats").map(|q| q.num_times_completed),
            Some(1)
        );

        // the credit cap is one per player per kill: a second pass over the same history counts again
        // only because it is a new call (ACE's credit dictionary is per call)
        creature_death::on_death_handle_kill_task(&mut w, rat, "KillTaskRats");
        assert_eq!(
            qm::get_quest(&w, &me(), "KillTaskRats").map(|q| q.num_times_completed),
            Some(2)
        );
    }

    /// `Portal.CheckUseRequirements`' quest restriction over the real `HasQuest`/`CanSolve`/
    /// `HandlePortalQuestError`: without the quest, `YouMustCompleteQuestToUsePortal`; solved within
    /// its MinDelta (the flag is fresh), the portal lets the player through; once it could be solved
    /// again (the flag has expired), `QuestSolvedTooLongAgo` and ACE's explanatory chat.
    #[test]
    fn a_portals_quest_restriction_uses_the_players_quest_registry() {
        let mut w = world();
        let p = spawn_portal(&mut w, None);
        w.objects
            .get_mut(p)
            .unwrap()
            .set_property(PropertyString::QuestRestriction, "ShortTimer".to_owned());

        start_capture();
        let r = portal::check_use_requirements(&mut w, p, player());
        assert!(!r.success);
        let sent = take_sent();
        assert_eq!(kinds(&sent), [WEENIE_ERROR]);
        assert_eq!(
            u32_at(&sent[0].2, 16),
            we(WeenieError::YouMustCompleteQuestToUsePortal)
        );

        qm::stamp(&mut w, &mut me(), "ShortTimer");
        set_now(&mut w, T0 + 3.0);
        start_capture();
        let r = portal::check_use_requirements(&mut w, p, player());
        assert!(r.success, "flagged within MinDelta 7");
        assert!(take_sent().is_empty());

        set_now(&mut w, T0 + 7.0);
        start_capture();
        let r = portal::check_use_requirements(&mut w, p, player());
        assert!(!r.success);
        let sent = take_sent();
        assert_eq!(
            chats(&sent),
            ["You completed the quest this portal requires too long ago!"]
        );
        let error = sent
            .iter()
            .find(|(_, _, b)| u32_at(b, 0) == 0xF7B0)
            .expect("the weenie error");
        assert_eq!(
            (u32_at(&error.2, 12), u32_at(&error.2, 16)),
            (WEENIE_ERROR, we(WeenieError::QuestSolvedTooLongAgo))
        );
    }
}

/// Divergence: V426, V427
/// A world without the contract tracker takes no contract on (an emote's add fails, nothing is
/// sent) and refuses abandoning one, telling the player why; a world without titles grants none
/// and sets none as the one shown. An end-of-retail world has both.
#[test]
fn a_world_without_contracts_or_titles_takes_none_on_and_grants_none() {
    use empyrean_common::era::{with_features, EraExt as _, EraFeatures, EraId};
    let mut w = world();
    let eor = EraId::Eor.rules();
    w.era = with_features(
        eor,
        EraFeatures {
            contracts: false,
            titles: false,
            ..eor.features
        },
    );
    w.objects
        .get_mut(player())
        .unwrap()
        .set_first_enter_world_done(true);
    start_capture();
    assert!(!cm::add(&mut w, player(), 7));
    pc::add_title_enum(&mut w, player(), CharacterTitle::Adventurer, true);
    pc::handle_action_set_title(&mut w, player(), CharacterTitle::Archer.0);
    assert!(take_sent().is_empty(), "nothing is sent");
    assert!(!cm::has_contract(&w, player(), 7));
    assert!(character(&w).character_properties_title_book.is_empty());
    assert_eq!(w.objects.get(player()).unwrap().character_title_id(), None);
    start_capture();
    empyrean_world::world_objects::player_contracts::handle_action_abandon_contract(
        &mut w,
        player(),
        7,
    );
    assert_eq!(chats(&take_sent()), ["This world has no contracts."]);

    w.era = eor;
    assert!(cm::add(&mut w, player(), 7));
    pc::add_title_enum(&mut w, player(), CharacterTitle::Adventurer, true);
    assert_eq!(character(&w).character_properties_title_book.len(), 1);
}
