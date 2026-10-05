//! Vectors: fixtures/vectors/fellowship/
//! Fellowship create/recruit/leave/XP share and fellowship game events follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::DataId;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, Case};
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, XpTable};
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    MotionCommand, MotionStance, PropertyAttribute2nd, PropertyDataId, PropertyInt, PropertyInt64,
    PropertyString, ShareType, WeenieError, WeenieErrorWithString, XpType,
};
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_store::models::shard::Character;
use empyrean_testkit::land;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::fellowship::{self, FellowshipRef};
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::guid_manager as gm;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::managers::player_manager::OnlinePlayer;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::managers::quest_manager::{self, QuestOwner};
use empyrean_world::network::game_event::game_event_type::GameEventType;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::player_fellowship;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

// ------------------------------------------------------------------------------------ harness

use empyrean_testkit::EmptyShard;

const LB: u32 = 0xA9B4_0000;

fn xp_table(level_xp: Vec<u64>) -> XpTable {
    let level_credits = vec![0; level_xp.len()];
    XpTable {
        id: DataId(file_id::XP_TABLE),
        level_xp,
        level_credits,
        ..empyrean_dat::fake::sample::xp_table()
    }
}

/// No attribute contribution to the vitals: a max vital is its starting value plus ranks.
fn secondary() -> SecondaryAttributeTable {
    let zero = SkillFormula {
        w: 0,
        x: 0,
        y: 0,
        z: 0,
        attr1: 0,
        attr2: 0,
    };
    SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: zero,
        stamina: zero,
        mana: zero,
    }
}

pub(crate) struct H {
    pub(crate) w: World,
    clock: VirtualClock,
    next_session: u16,
}

impl H {
    fn new(level_xp: Vec<u64>) -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let dats = empyrean_testkit::dats::with_stat_tables(
            FakeDats::new().with_xp_table(xp_table(level_xp)),
        )
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, secondary())
        .build()
        .expect("fake dats");
        let mut w = World::new(now, dats);
        w.timers = timers;
        w.content = Arc::new(MemContent::new());
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 0);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(511);
        lm::get_landblock(&mut w, LandblockId::new(LB | 0xFFFF), false, false);
        H {
            w,
            clock,
            next_session: 1,
        }
    }

    pub(crate) fn small() -> Self {
        Self::new(vec![0, 0, 1_000, 2_500, 5_000, 9_000, 14_000])
    }

    /// One pass: the delay manager, then `LandblockManager.Tick`.
    fn tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        delay_manager::run_actions(&mut self.w);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    fn advance(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        empyrean_world::entity::timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
    }

    fn o(&self, g: ObjectGuid) -> &WorldObject {
        self.w.objects.get(g).expect("object in store")
    }

    fn o_mut(&mut self, g: ObjectGuid) -> &mut WorldObject {
        self.w.objects.get_mut(g).expect("object in store")
    }

    /// An online level-`level` player with a session and a Character, placed on the landblock at
    /// (20, 20), at peace. Returns its session.
    pub(crate) fn player(&mut self, guid: ObjectGuid, name: &str, level: i32) -> SessionId {
        let mut o = WorldObject::allocate(Class::Player);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.properties_enchantment_registry = Some(Vec::new());
        let mut vitals = empyrean_common::dotnet::DotNetDict::new();
        for (v, level) in [
            (PropertyAttribute2nd::MaxHealth, 120),
            (PropertyAttribute2nd::MaxStamina, 90),
            (PropertyAttribute2nd::MaxMana, 60),
        ] {
            vitals.insert(
                v,
                PropertiesAttribute2nd {
                    init_level: level,
                    level_from_cp: 0,
                    cp_spent: 0,
                    current_level: level - 10,
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
        o.set_location(Some(at(20.0, 20.0)));
        o.set_heartbeat_interval(Some(0.0));
        empyrean_world::world_objects::world_object_tick::world_object_initialize_heartbeats(
            &mut o,
            self.w.now.unix_time,
        );
        o.set_level(Some(level));
        o.set_property(PropertyInt64::TotalExperience, 0);
        o.set_property(PropertyInt64::AvailableExperience, 0);
        o.set_property(PropertyInt::AvailableSkillCredits, 0);
        o.set_property(PropertyInt::TotalSkillCredits, 0);
        o.player.as_mut().expect("a player").player.character = Some(Character::default());
        o.wo.world_object_properties.current_motion_state = Some(Motion::new(
            MotionStance::NonCombat,
            MotionCommand::Ready,
            1.0,
        ));
        self.w.objects.insert(o).expect("fresh guid");

        let session = SessionId {
            client_id: self.next_session,
            generation: 1,
        };
        self.next_session += 1;
        self.w.sessions.insert(
            session,
            SessionData {
                player: Some(guid),
                ..SessionData::default()
            },
        );
        self.w.player_manager.online_players.insert(
            guid.full(),
            OnlinePlayer {
                guid,
                account: None,
            },
        );
        assert!(
            lm::add_object(&mut self.w, guid, false),
            "placed on the landblock"
        );
        session
    }

    /// Removes a player and its session (between replayed cases).
    fn remove(&mut self, guid: ObjectGuid, session: SessionId) {
        let lb = self.o(guid).current_landblock.expect("on the landblock");
        empyrean_world::entity::landblock::remove_world_object(
            &mut self.w,
            lb,
            guid,
            false,
            false,
            true,
        );
        self.w.objects.remove(guid);
        self.w.sessions.remove(session);
        self.w.player_manager.online_players.remove(&guid.full());
    }

    fn fellowship(&self, g: ObjectGuid) -> Option<FellowshipRef> {
        player_fellowship::fellowship(&self.w, g)
    }

    fn members(&mut self, f: &FellowshipRef) -> Vec<ObjectGuid> {
        fellowship::get_fellowship_members(&mut self.w, f)
            .values()
            .copied()
            .collect()
    }
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

/// `(session, opcode, game event type or 0, bytes)` of each captured send.
pub(crate) fn sent() -> Vec<(SessionId, u32, u32, Vec<u8>)> {
    take_sent()
        .into_iter()
        .map(|(s, _, b)| {
            let op = u32::from_le_bytes(b[0..4].try_into().unwrap());
            let ev = if op == GameMessageOpcode::GameEvent.0 {
                u32::from_le_bytes(b[12..16].try_into().unwrap())
            } else {
                0
            };
            (s, op, ev, b)
        })
        .collect()
}

fn string16l(b: &[u8], at: usize) -> String {
    let len = usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
    String::from_utf8(b[at + 2..at + 2 + len].to_vec()).unwrap()
}

/// The system chats `session` got.
pub(crate) fn chats_to(msgs: &[(SessionId, u32, u32, Vec<u8>)], session: SessionId) -> Vec<String> {
    msgs.iter()
        .filter(|m| m.0 == session && m.1 == GameMessageOpcode::ServerMessage.0)
        .map(|m| string16l(&m.3, 4))
        .collect()
}

/// The game event types `session` got, in order.
pub(crate) fn events_to(msgs: &[(SessionId, u32, u32, Vec<u8>)], session: SessionId) -> Vec<u32> {
    msgs.iter()
        .filter(|m| m.0 == session && m.2 != 0)
        .map(|m| m.2)
        .collect()
}

fn u(v: &serde_json::Value) -> u64 {
    v.as_u64().unwrap_or_else(|| panic!("not a uint: {v}"))
}

fn u32_of(v: &serde_json::Value) -> u32 {
    u32::try_from(u(v)).unwrap()
}

fn f32_of(v: &serde_json::Value) -> f32 {
    vectors::f32_of(v).unwrap()
}

fn xp_type(v: &serde_json::Value) -> XpType {
    XpType(i32::try_from(v.as_i64().unwrap()).unwrap())
}

// ------------------------------------------------------------------------------------ vectors

/// `GetDistanceScalar` over ACE's cases: outdoor steps at 600 and 1200 units, indoor cells in one
/// and in two landblocks, indoor against outdoor, quest XP and null players.
#[test]
fn distance_scalar_matches_ace() {
    let file = vectors::load_named("fellowship", "distance_scalar");
    let mut h = H::small();
    let (earner, fellow) = (ObjectGuid::new(0x5000_0101), ObjectGuid::new(0x5000_0102));
    h.player(earner, "Earner", 10);
    h.player(fellow, "Fellow", 10);
    let mut failures = Vec::new();
    for Case { input, output, .. } in &file.cases {
        let place = |h: &mut H, g: ObjectGuid, v: &serde_json::Value| -> Option<ObjectGuid> {
            let a = v.as_array()?;
            let pos = Position::from_components(
                u32_of(&a[0]),
                f32_of(&a[1]),
                f32_of(&a[2]),
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                false,
            );
            h.o_mut(g).set_location(Some(pos));
            Some(g)
        };
        let e = place(&mut h, earner, &input["earner"]);
        let f = place(&mut h, fellow, &input["fellow"]);
        let got = fellowship::get_distance_scalar(&h.w, e, f, xp_type(&input["xp_type"]));
        let want = vectors::f64_of(output).unwrap();
        if got.to_bits() != want.to_bits() {
            failures.push(format!("{input}: expected {want} got {got}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        file.cases.len(),
        failures.join("\n")
    );
}

/// `CalculateXPSharing`, `GetMemberSharePercent` and `SplitXp` over ACE's 240 fellowships: the
/// sharing flags, the percentage, and every member's grant (amount, XP type, share type).
#[test]
fn split_xp_matches_ace() {
    let file = vectors::load_named("fellowship", "split_xp");
    let level_xp: Vec<u64> = file.cases[0].input["level_xp"]
        .as_array()
        .unwrap()
        .iter()
        .map(u)
        .collect();
    let mut h = H::new(level_xp);
    let mut failures = Vec::new();
    let mut checked = 0;
    for Case { input, output, .. } in file.cases.iter().skip(1) {
        let members: Vec<(ObjectGuid, i32, Position)> = input["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                let m = m.as_array().unwrap();
                let pos = Position::from_components(
                    u32_of(&m[2]),
                    f32_of(&m[3]),
                    f32_of(&m[4]),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    false,
                );
                (
                    ObjectGuid::new(u32_of(&m[0])),
                    i32::try_from(m[1].as_i64().unwrap()).unwrap(),
                    pos,
                )
            })
            .collect();
        let sessions: Vec<SessionId> = members
            .iter()
            .map(|(g, level, _)| h.player(*g, "Fellow", *level))
            .collect();
        // the vector's Location (the body stays where it was placed: only Location is read)
        for (g, _, pos) in &members {
            h.o_mut(*g).set_location(Some(*pos));
        }
        let leader = members[0].0;
        if !input["leader_online"].as_bool().unwrap() {
            h.w.player_manager.online_players.remove(&leader.full());
        }
        let f = fellowship::new(
            &mut h.w,
            leader,
            "Vectors",
            input["desired_share_xp"].as_bool().unwrap(),
        );
        for (g, _, _) in &members {
            f.get_mut(&mut h.w).fellowship_members.try_add(g.full(), *g);
            player_fellowship::set_fellowship(&mut h.w, *g, Some(f));
        }
        pm::modify_bool(
            &h.w,
            "fellow_quest_bonus",
            input["quest_bonus"].as_bool().unwrap(),
        );

        fellowship::calculate_xp_sharing(&mut h.w, &f);
        let (share_xp, even_share) = {
            let l = f.get(&h.w);
            (l.share_xp, l.even_share)
        };
        let percent = fellowship::get_member_share_percent(&mut h.w, &f);
        let earner = members[usize::try_from(u(&input["earner"])).unwrap()].0;

        start_capture();
        fellowship::split_xp(
            &mut h.w,
            &f,
            u(&input["amount"]),
            xp_type(&input["xp_type"]),
            ShareType(i32::try_from(input["share_type"].as_i64().unwrap()).unwrap()),
            earner,
        );
        h.tick();
        let msgs = sent();

        let input_share = i32::try_from(input["share_type"].as_i64().unwrap()).unwrap();
        let grants = output["grants"].as_array().unwrap();
        let mut problems = Vec::new();
        if share_xp != output["share_xp"].as_bool().unwrap()
            || even_share != output["even_share"].as_bool().unwrap()
        {
            problems.push(format!("sharing ({share_xp}, {even_share})"));
        }
        // V226: the share is retail's single-precision table, widened; ACE's recorded percent is
        // the record of what ACE paid, and bounds how far each grant may move from ACE's.
        let ace_percent = vectors::f64_of(&output["percent"]).unwrap();
        let retail_percent = f64::from(dereth_rules::fellowship::even_split_xp_percentage(
            members.len(),
        ));
        if percent.to_bits() != retail_percent.to_bits() {
            problems.push(format!("percent {percent}, retail {retail_percent}"));
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let bound = if even_share {
            (u(&input["amount"]) as f64 * (retail_percent - ace_percent).abs()).ceil() as i64 + 1
        } else {
            0
        };
        if grants.len() != members.len() {
            problems.push(format!(
                "{} grants for {} members",
                grants.len(),
                members.len()
            ));
        }
        for (i, grant) in grants.iter().enumerate() {
            let g = grant.as_array().unwrap();
            let (guid, amount) = (ObjectGuid::new(u32_of(&g[0])), g[1].as_i64().unwrap());
            let (ty, share) = (
                xp_type(&g[2]),
                ShareType(i32::try_from(g[3].as_i64().unwrap()).unwrap()),
            );
            if guid != members[i].0 {
                problems.push(format!("grant {i} to {guid:?}"));
            }
            // UpdateXpAndLevel adds nothing at the max level (275 in this table)
            let expected = if h.o(guid).level() == Some(275) {
                0
            } else {
                amount
            };
            let total = h.o(guid).total_experience().unwrap_or(0);
            if (total - expected).abs() > if expected == 0 { 0 } else { bound } {
                problems.push(format!("member {i}: {total} XP, ACE granted {amount}"));
            }
            let quest_chats = chats_to(&msgs, sessions[i])
                .iter()
                .filter(|c| c.starts_with("You've earned "))
                .count();
            if quest_chats != usize::from(ty == XpType::Quest) {
                problems.push(format!(
                    "member {i}: {quest_chats} quest XP chats for {ty:?}"
                ));
            }
            // `shareType &= ~ShareType.Fellowship` for every member. No member here has an allegiance,
            // so that the port hands the allegiance bit on to GrantXP is checked by
            // `split_xp_hands_the_allegiance_bit_to_grant_xp`.
            if share != ShareType(input_share & !ShareType::Fellowship.0) {
                problems.push(format!(
                    "member {i}: ACE's share type {share:?} for input {input_share}"
                ));
            }
        }
        if !problems.is_empty() {
            failures.push(format!("{input}: {}", problems.join("; ")));
        }
        checked += 1;

        for (g, s) in members.iter().map(|m| m.0).zip(sessions) {
            h.remove(g, s);
        }
    }
    pm::modify_bool(&h.w, "fellow_quest_bonus", false);
    assert_eq!(checked, 240);
    assert!(
        failures.is_empty(),
        "{} of {checked} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ------------------------------------------------------------------------------------ behaviour

const A: ObjectGuid = ObjectGuid::new(0x5000_0001);
const B: ObjectGuid = ObjectGuid::new(0x5000_0002);
const C: ObjectGuid = ObjectGuid::new(0x5000_0003);

/// Sets `AutomaticallyAcceptFellowshipRequests` (CharacterOptions2 bit) on a player's Character.
fn auto_accept(h: &mut H, g: ObjectGuid) {
    let option = empyrean_entity::enums::CharacterOption::AutomaticallyAcceptFellowshipRequests;
    let ch = h
        .o_mut(g)
        .player
        .as_mut()
        .unwrap()
        .player
        .character
        .as_mut()
        .unwrap();
    if let Some(bit) = option.character_options1() {
        ch.character_options_1 |= bit.0.cast_signed();
    } else {
        ch.character_options_2 |= option
            .character_options2()
            .expect("an options flag")
            .0
            .cast_signed();
    }
}

/// Create, then recruit an auto-accepting player: the leader's full update and done; the recruit
/// joins (the fellow update to the others, a full update to every member); a second recruit of the
/// same player is refused as already in a fellowship.
#[test]
fn create_and_recruit_an_auto_accepting_player() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    auto_accept(&mut h, B);

    start_capture();
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    let msgs = sent();
    assert_eq!(
        events_to(&msgs, sa),
        [
            GameEventType::FellowshipFullUpdate.0,
            GameEventType::FellowshipFellowUpdateDone.0
        ]
    );
    let f = h.fellowship(A).expect("created");
    assert_eq!(f.leader_guid(&h.w), A.full());
    assert!(f.get(&h.w).share_xp && !f.get(&h.w).even_share && !f.get(&h.w).open);

    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let msgs = sent();
    assert_eq!(h.fellowship(B), Some(f), "the same fellowship object");
    assert_eq!(h.members(&f), [A, B]);
    // levels 3 and 4 are within 5 of the leader: even share
    assert!(f.get(&h.w).share_xp && f.get(&h.w).even_share);
    assert_eq!(
        events_to(&msgs, sa),
        [
            GameEventType::FellowshipUpdateFellow.0,
            GameEventType::FellowshipFullUpdate.0
        ]
    );
    assert_eq!(
        events_to(&msgs, sb),
        [GameEventType::FellowshipFullUpdate.0]
    );

    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let msgs = sent();
    assert_eq!(
        chats_to(&msgs, sa),
        ["Bravo is already a member of a Fellowship."]
    );
}

/// A recruit without auto-accept gets the confirmation popup (the shim); a yes adds it, a second
/// answer finds the offer gone; a no tells the inviter; an unanswered one times out after 30 s.
#[test]
fn the_confirmation_popup_adds_declines_and_expires() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    let sc = h.player(C, "Charlie", 2);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    let f = h.fellowship(A).unwrap();

    start_capture();
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let msgs = sent();
    let request: Vec<&Vec<u8>> = msgs
        .iter()
        .filter(|m| m.0 == sb && m.2 == GameEventType::CharacterConfirmationRequest.0)
        .map(|m| &m.3)
        .collect();
    assert_eq!(request.len(), 1);
    // type Fellowship (4), context 1, the inviter's name
    assert_eq!(
        u32::from_le_bytes(request[0][16..20].try_into().unwrap()),
        4
    );
    assert_eq!(
        u32::from_le_bytes(request[0][20..24].try_into().unwrap()),
        1
    );
    assert_eq!(string16l(request[0], 24), "Alpha");
    assert_eq!(h.members(&f), [A]);

    // a second recruit while the popup is up: busy
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    assert_eq!(chats_to(&sent(), sa), ["Bravo is busy."]);

    assert!(
        empyrean_world::world_objects::managers::confirmation_manager::handle_response(
            &mut h.w,
            B,
            empyrean_entity::enums::ConfirmationType::Fellowship,
            1,
            true,
            false
        )
    );
    assert_eq!(h.members(&f), [A, B]);
    assert!(
        !empyrean_world::world_objects::managers::confirmation_manager::handle_response(
            &mut h.w,
            B,
            empyrean_entity::enums::ConfirmationType::Fellowship,
            1,
            true,
            false
        )
    );
    assert_eq!(
        chats_to(&sent(), sb).last().map(String::as_str),
        Some("That offer of fellowship has expired.")
    );

    // Charlie says no
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(C));
    let _ = sent();
    assert!(
        empyrean_world::world_objects::managers::confirmation_manager::handle_response(
            &mut h.w,
            C,
            empyrean_entity::enums::ConfirmationType::Fellowship,
            1,
            false,
            false
        )
    );
    assert_eq!(
        chats_to(&sent(), sa),
        ["Charlie has declined your offer of fellowship."]
    );
    assert_eq!(h.members(&f), [A, B]);

    // and then lets it time out: ConfirmationDone to Charlie, "did not respond" to the leader
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(C));
    let _ = sent();
    h.advance(29.0);
    h.tick();
    assert!(events_to(&sent(), sc).is_empty());
    h.advance(1.5);
    h.tick();
    h.tick();
    let msgs = sent();
    assert_eq!(
        events_to(&msgs, sc),
        [GameEventType::CharacterConfirmationDone.0]
    );
    assert_eq!(
        chats_to(&msgs, sa),
        ["Charlie did not respond to your offer of fellowship."]
    );
    assert_eq!(h.members(&f), [A, B]);
}

/// Openness and dismissal are the leader's; a closed fellowship refuses a member's recruit; the
/// leader cannot dismiss itself; a dismissal tells everyone and clears the player's fellowship.
#[test]
fn only_the_leader_opens_and_dismisses() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    let sc = h.player(C, "Charlie", 2);
    auto_accept(&mut h, B);
    auto_accept(&mut h, C);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let f = h.fellowship(A).unwrap();

    start_capture();
    player_fellowship::fellowship_recruit(&mut h.w, B, Some(C));
    player_fellowship::handle_action_fellowship_change_openness(&mut h.w, B, true);
    player_fellowship::fellowship_dismiss_player(&mut h.w, B, A.full());
    let msgs = sent();
    let err =
        |m: &(SessionId, u32, u32, Vec<u8>)| u32::from_le_bytes(m.3[16..20].try_into().unwrap());
    let errors: Vec<u32> = msgs
        .iter()
        .filter(|m| m.0 == sb && m.2 == GameEventType::WeenieError.0)
        .map(err)
        .collect();
    let must = u32::try_from(WeenieError::YouMustBeLeaderOfFellowship.0).unwrap();
    assert_eq!(errors, [must, must, must]);
    assert_eq!(h.members(&f), [A, B]);

    player_fellowship::handle_action_fellowship_change_openness(&mut h.w, A, true);
    let msgs = sent();
    let opened: Vec<&(SessionId, u32, u32, Vec<u8>)> = msgs
        .iter()
        .filter(|m| m.2 == GameEventType::WeenieErrorWithString.0)
        .collect();
    assert_eq!(opened.len(), 2, "both members hear it");
    assert_eq!(
        u32::from_le_bytes(opened[0].3[16..20].try_into().unwrap()),
        u32::try_from(WeenieErrorWithString::_IsNowOpenFellowship.0).unwrap()
    );
    assert_eq!(string16l(&opened[0].3, 20), "Company");
    assert!(f.get(&h.w).open);

    // open: a member may recruit
    player_fellowship::fellowship_recruit(&mut h.w, B, Some(C));
    assert_eq!(h.members(&f), [A, B, C]);
    let _ = sent();

    player_fellowship::fellowship_dismiss_player(&mut h.w, A, A.full());
    assert_eq!(
        chats_to(&sent(), sa),
        ["You can't dismiss yourself from the fellowship"]
    );

    player_fellowship::fellowship_dismiss_player(&mut h.w, A, C.full());
    let msgs = sent();
    for s in [sa, sb, sc] {
        assert_eq!(chats_to(&msgs, s), ["Charlie dismissed from fellowship"]);
        assert_eq!(events_to(&msgs, s)[0], GameEventType::FellowshipDismiss.0);
    }
    assert!(h.fellowship(C).is_none());
    assert_eq!(h.members(&f), [A, B]);
}

/// A member's quit (FellowshipQuit to it and to the rest); the leader's quit hands leadership to a
/// random remaining member (`ThreadSafeRandom.Next(0, count - 1)`); a disband clears everyone.
#[test]
fn quit_new_leader_and_disband() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    let sc = h.player(C, "Charlie", 2);
    auto_accept(&mut h, B);
    auto_accept(&mut h, C);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(C));
    let f = h.fellowship(A).unwrap();

    start_capture();
    player_fellowship::fellowship_quit(&mut h.w, C, false);
    let msgs = sent();
    assert_eq!(events_to(&msgs, sc), [GameEventType::FellowshipQuit.0]);
    assert_eq!(events_to(&msgs, sa), [GameEventType::FellowshipQuit.0]);
    assert!(h.fellowship(C).is_none());
    assert_eq!(h.members(&f), [A, B]);

    // the leader leaves without disbanding: B is the only candidate
    ThreadSafeRandom::seed(7);
    player_fellowship::fellowship_quit(&mut h.w, A, false);
    let msgs = sent();
    assert_eq!(f.leader_guid(&h.w), B.full());
    assert_eq!(
        chats_to(&msgs, sa),
        ["You no longer have permission to loot anyone else's kills."]
    );
    let lead: Vec<String> = msgs
        .iter()
        .filter(|m| m.0 == sb && m.2 == GameEventType::WeenieErrorWithString.0)
        .map(|m| string16l(&m.3, 20))
        .collect();
    assert_eq!(lead, ["Bravo"]);
    assert!(h.fellowship(A).is_none());

    // B disbands: FellowshipDisband, everyone's fellowship cleared
    player_fellowship::fellowship_quit(&mut h.w, B, true);
    let msgs = sent();
    assert_eq!(events_to(&msgs, sb), [GameEventType::FellowshipDisband.0]);
    assert!(h.fellowship(B).is_none());
}

/// A locked fellowship refuses a stranger and takes a departed member back only within 900 s
/// (V274, owner 2026-09-24: the 15 minutes the lock message promises; ACE allowed 600 s);
/// the lock is in the full update.
#[test]
fn a_locked_fellowship_takes_back_only_recent_departures() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    h.player(B, "Bravo", 4);
    h.player(C, "Charlie", 2);
    auto_accept(&mut h, B);
    auto_accept(&mut h, C);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let f = h.fellowship(A).unwrap();
    player_fellowship::handle_action_fellowship_change_lock(&mut h.w, A, true, Some("Crypt"));
    assert!(f.get(&h.w).is_locked && f.get(&h.w).fellowship_locks.contains_key("Crypt"));

    start_capture();
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(C));
    let msgs = sent();
    let refused: Vec<String> = msgs
        .iter()
        .filter(|m| m.0 == sa && m.2 == GameEventType::WeenieErrorWithString.0)
        .map(|m| string16l(&m.3, 20))
        .collect();
    assert_eq!(refused, ["Charlie"]);

    player_fellowship::fellowship_quit(&mut h.w, B, false);
    assert!(f.get(&h.w).departed_members.contains_key(&B.full()));
    h.advance(601.0);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    assert_eq!(h.members(&f), [A, B], "back after 601 s, within 15 minutes");
    player_fellowship::fellowship_quit(&mut h.w, B, false);
    h.advance(899.0);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    assert_eq!(h.members(&f), [A, B], "back within 900 s");
    player_fellowship::fellowship_quit(&mut h.w, B, false);
    h.advance(901.0);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    assert_eq!(h.members(&f), [A], "too late");
    // openness can't change while locked
    let _ = sent();
    player_fellowship::handle_action_fellowship_change_openness(&mut h.w, A, true);
    let errors: Vec<u32> = sent()
        .iter()
        .filter(|m| m.0 == sa && m.2 == GameEventType::WeenieError.0)
        .map(|m| u32::from_le_bytes(m.3[16..20].try_into().unwrap()))
        .collect();
    assert_eq!(
        errors,
        [u32::try_from(WeenieError::FellowshipIsLocked.0).unwrap()]
    );
}

/// A member whose session is gone is dropped by the next `GetFellowshipMembers`
/// (`ProcessDropList`): the leader drops, a new one is drawn, the rest get full updates.
#[test]
fn a_member_without_a_session_is_dropped() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    auto_accept(&mut h, B);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let f = h.fellowship(A).unwrap();

    h.w.sessions.remove(sa);
    start_capture();
    assert_eq!(h.members(&f), [B]);
    assert_eq!(f.leader_guid(&h.w), B.full());
    let msgs = sent();
    assert!(events_to(&msgs, sb).contains(&GameEventType::FellowshipFullUpdate.0));
    assert!(!f.get(&h.w).fellowship_members.contains_key(&A.full()));
}

/// Vitals reach the fellows whose panel is open (`OnVitalUpdate` from the player tick's flag);
/// deaths and level-ups are announced to the others.
#[test]
fn vitals_deaths_and_level_ups_reach_the_fellows() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    auto_accept(&mut h, B);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let f = h.fellowship(A).unwrap();

    player_fellowship::handle_fellowship_update_request(&mut h.w, B, true);
    start_capture();
    fellowship::on_vital_update(&mut h.w, &f, A);
    let msgs = sent();
    let updates: Vec<&Vec<u8>> = msgs
        .iter()
        .filter(|m| m.2 == GameEventType::FellowshipUpdateFellow.0)
        .map(|m| &m.3)
        .collect();
    assert_eq!(updates.len(), 1, "only Bravo's panel is open");
    assert_eq!(
        msgs.iter()
            .find(|m| m.2 == GameEventType::FellowshipUpdateFellow.0)
            .unwrap()
            .0,
        sb
    );
    let body = &updates[0][16..];
    let word = |i: usize| u32::from_le_bytes(body[i * 4..i * 4 + 4].try_into().unwrap());
    // guid, cp, lum, level, max h/s/m, current h/s/m, share loot << 1
    assert_eq!([word(0), word(1), word(2), word(3)], [A.full(), 0, 0, 3]);
    assert_eq!(
        [
            word(4),
            word(5),
            word(6),
            word(7),
            word(8),
            word(9),
            word(10)
        ],
        [120, 90, 60, 110, 80, 50, 0]
    );
    assert_eq!(string16l(body, 44), "Alpha");

    fellowship::on_death(&mut h.w, &f, A);
    fellowship::on_fellow_level_up(&mut h.w, &f, A);
    let msgs = sent();
    assert_eq!(
        chats_to(&msgs, sb),
        ["Your fellow Alpha has died!", "Alpha is now level 3!"]
    );
    assert!(chats_to(&msgs, sa).is_empty());
}

/// A fellowship quest stamp reaches every member.
#[test]
fn a_fellowship_quest_stamp_reaches_every_member() {
    let mut h = H::small();
    h.player(A, "Alpha", 3);
    h.player(B, "Bravo", 4);
    auto_accept(&mut h, B);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));

    let fa = h.fellowship(A).unwrap();
    fa.with_quest_manager(&mut h.w, |w, qm| {
        quest_manager::stamp(w, &mut QuestOwner::Fellowship(qm), "CryptCleared@fellow")
    });
    let fb = h.fellowship(B).unwrap();
    assert!(fb.read_quest_manager(&h.w, |qm| quest_manager::has_quest(
        &h.w,
        &QuestOwner::Fellowship(qm),
        "CryptCleared"
    )));
    assert!(
        fb.read_quest_manager(&h.w, |qm| quest_manager::name(
            &h.w,
            &QuestOwner::Fellowship(qm)
        )) == "Fellowship(Company)"
    );
    // not the players' own registries
    assert!(!quest_manager::has_quest(
        &h.w,
        &QuestOwner::Creature(B),
        "CryptCleared"
    ));
}

/// The full update: the members in `HashComparer(16)` order with their records, then the name,
/// leader, flags, the departed table (32 buckets) and the lock table.
#[test]
fn the_full_update_decodes_with_dereth_protocol() {
    use dereth_protocol::social::FellowshipFullUpdate;
    use dereth_protocol::{Message, Reader};

    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    h.player(ObjectGuid::new(0x5000_0011), "Kilo", 4);
    auto_accept(&mut h, ObjectGuid::new(0x5000_0011));
    player_fellowship::fellowship_create(&mut h.w, A, "Company", false);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(ObjectGuid::new(0x5000_0011)));

    start_capture();
    player_fellowship::handle_fellowship_update_request(&mut h.w, A, true);
    let msgs = sent();
    let full = msgs
        .iter()
        .find(|m| m.0 == sa && m.2 == GameEventType::FellowshipFullUpdate.0)
        .expect("a full update");
    let mut r = Reader::with_origin(&full.3[16..], 16);
    let m = FellowshipFullUpdate::read(&mut r).expect("decodes");
    let f = m.0;
    // 0x50000011 % 16 == 1 sorts before 0x50000001 % 16 == 1? equal buckets: by key
    let keys: Vec<u32> = f.members.entries.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, [A.full(), 0x5000_0011]);
    let alpha = &f
        .members
        .entries
        .iter()
        .find(|(k, _)| *k == A.full())
        .unwrap()
        .1;
    assert_eq!(
        (
            alpha.name.as_str(),
            alpha.level,
            alpha.max_health,
            alpha.current_health,
            alpha.share_loot
        ),
        ("Alpha", 3, 120, 110, 0x10)
    );
    assert_eq!(
        (
            f.name.as_str(),
            f.leader.0,
            f.share_xp,
            f.even_xp_split,
            f.open_fellow,
            f.locked
        ),
        ("Company", A.full(), 0, 1, 0, 0)
    );
    // The (empty) lock table ends it, as the retail server sent it (V256/V279)
    assert_eq!(f.locks, dereth_protocol::social::FellowshipLocks::default());
    assert_eq!(r.remaining(), 0);
    assert_eq!(full.3[full.3.len() - 4..], [0x00, 0x00, 0x20, 0x00]);
}

/// A leader who quits hands the fellowship to `fellowGuids[ThreadSafeRandom.Next(0, count - 1)]`
/// (inclusive), drawn over the remaining members in dictionary order.
#[test]
fn a_departing_leader_hands_over_to_a_random_member() {
    for seed in [3, 11, 29, 47] {
        let mut h = H::small();
        let guids: Vec<ObjectGuid> = (1..=4).map(|i| ObjectGuid::new(0x5000_0000 + i)).collect();
        for (i, g) in guids.iter().enumerate() {
            h.player(*g, ["Alpha", "Bravo", "Charlie", "Delta"][i], 3);
        }
        for g in &guids[1..] {
            auto_accept(&mut h, *g);
        }
        player_fellowship::fellowship_create(&mut h.w, guids[0], "Company", true);
        for g in &guids[1..] {
            player_fellowship::fellowship_recruit(&mut h.w, guids[0], Some(*g));
        }
        let f = h.fellowship(guids[0]).unwrap();

        ThreadSafeRandom::seed(seed);
        player_fellowship::fellowship_quit(&mut h.w, guids[0], false);
        let mut reference =
            empyrean_common::random::DotNetRandom::new(i32::try_from(seed).unwrap());
        let idx = usize::try_from(reference.next_range(0, 3)).unwrap();
        assert_eq!(f.leader_guid(&h.w), guids[1 + idx].full(), "seed {seed}");
    }
}

/// `MaxFellows` is 9: the tenth recruit is refused with YourFellowshipIsFull.
#[test]
fn a_full_fellowship_refuses_a_tenth_member() {
    let mut h = H::small();
    let guids: Vec<ObjectGuid> = (1..=10).map(|i| ObjectGuid::new(0x5000_0000 + i)).collect();
    let sa = h.player(guids[0], "Leader", 3);
    let mut sessions = Vec::new();
    for g in &guids[1..] {
        sessions.push(h.player(*g, "Fellow", 3));
    }
    // the first eight accept automatically; the tenth would get the popup
    for g in &guids[1..9] {
        auto_accept(&mut h, *g);
    }
    player_fellowship::fellowship_create(&mut h.w, guids[0], "Company", true);
    for g in &guids[1..9] {
        player_fellowship::fellowship_recruit(&mut h.w, guids[0], Some(*g));
    }
    let f = h.fellowship(guids[0]).unwrap();
    assert_eq!(h.members(&f).len(), 9);
    start_capture();
    player_fellowship::fellowship_recruit(&mut h.w, guids[0], Some(guids[9]));
    let msgs = sent();
    let errors: Vec<u32> = msgs
        .iter()
        .filter(|m| m.0 == sa && m.2 == GameEventType::WeenieError.0)
        .map(|m| u32::from_le_bytes(m.3[16..20].try_into().unwrap()))
        .collect();
    assert_eq!(
        errors,
        [u32::try_from(WeenieError::YourFellowshipIsFull.0).unwrap()]
    );
    assert!(
        events_to(&msgs, sessions[8]).is_empty(),
        "no confirmation popup"
    );
    assert!(h.fellowship(guids[9]).is_none());
}

/// The departed table of the full update is a `HashComparer(32)` table: 0x50000002 (bucket 2)
/// before 0x50000011 (bucket 17), though with 16 buckets the order would flip.
#[test]
fn the_departed_table_is_in_32_bucket_order() {
    use dereth_protocol::social::FellowshipFullUpdate;
    use dereth_protocol::{Message, Reader};

    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let (b, k) = (ObjectGuid::new(0x5000_0002), ObjectGuid::new(0x5000_0011));
    h.player(b, "Bravo", 3);
    h.player(k, "Kilo", 3);
    auto_accept(&mut h, b);
    auto_accept(&mut h, k);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(k));
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(b));
    player_fellowship::handle_action_fellowship_change_lock(&mut h.w, A, true, None);
    player_fellowship::fellowship_quit(&mut h.w, k, false);
    player_fellowship::fellowship_quit(&mut h.w, b, false);

    start_capture();
    player_fellowship::handle_fellowship_update_request(&mut h.w, A, true);
    let msgs = sent();
    let full = msgs
        .iter()
        .find(|m| m.0 == sa && m.2 == GameEventType::FellowshipFullUpdate.0)
        .expect("a full update");
    let mut r = Reader::with_origin(&full.3[16..], 16);
    let f = FellowshipFullUpdate::read(&mut r).expect("decodes").0;
    assert_eq!(f.locked, 1);
    assert_eq!(
        f.fellows_departed
            .entries
            .iter()
            .map(|(g, _)| *g)
            .collect::<Vec<_>>(),
        [b.full(), k.full()]
    );
    assert_eq!(f.fellows_departed.table_size, 32);
}

/// The allegiance bit SplitXp leaves in the share type reaches GrantXP: a member sworn to a patron
/// outside the fellowship passes up its share (`AllegianceManager.PassXP` on the world queue) for
/// `ShareType.All`, and nothing for `ShareType.Fellowship` alone.
#[test]
fn split_xp_hands_the_allegiance_bit_to_grant_xp() {
    use dereth_protocol::comms::CharacterConfirmationRequest;
    use dereth_protocol::events::split_ui_blob;
    use dereth_protocol::Message;
    use empyrean_common::dotnet::datetime::DotNetDateTime;
    use empyrean_content::models::world::Weenie as ContentWeenie;
    use empyrean_entity::enums::{AccessLevel, ConfirmationType, WeenieType};
    use empyrean_net::SessionState;
    use empyrean_store::models::auth::Account;
    use empyrean_store::MemShard;
    use empyrean_world::entity::actions::action_queue::run_actions;
    use empyrean_world::entity::actions::i_actor::Actor;
    use empyrean_world::entity::i_player::IPlayer;
    use empyrean_world::managers::player_manager::{self, OrdinalIgnoreCase};
    use empyrean_world::world_objects::world_object::CtorEnv;
    use empyrean_world::world_objects::{player, player_allegiance as pa};

    const PLAYER_WCID: u32 = 1;
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
    );
    w.content = Arc::new(
        MemContent::new()
            .weenie(
                ContentWeenie::new(PLAYER_WCID, "human", WeenieType::Creature).with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                ),
            )
            .weenie(ContentWeenie::new(
                1149,
                "allegiance",
                WeenieType::Allegiance,
            )),
    );
    gm::initialize(&mut w, &mut EmptyShard);
    pm::install_shard_config(&mut w, pm::shard_config_handle(Box::new(MemShard::new())));
    pm::initialize(&mut w, true);

    // Patron (outside the fellowship), Vassal and Cleric (the fellowship), each on its own account
    let people = [
        (0x5000_0001u32, "Patron", 50, 1u32),
        (0x5000_0002, "Vassal", 20, 2),
        (0x5000_0003, "Cleric", 20, 3),
    ];
    for (g, name, level, account_id) in people {
        let account = Account {
            account_id,
            account_name: name.to_lowercase(),
            ..Default::default()
        };
        let weenie = w
            .content
            .get_cached_weenie(PLAYER_WCID)
            .expect("player weenie");
        let mut o = CtorEnv::with_world(&w, |env| {
            player::player_from_weenie(env, Class::Player, weenie, ObjectGuid::new(g), account_id)
        });
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(PropertyInt::Level, level);
        o.set_property(PropertyInt::Gender, 1);
        o.set_property(PropertyInt::HeritageGroup, 1);
        let p = o.player.as_mut().expect("a player");
        p.player.character = Some(Character {
            id: g,
            account_id,
            name: name.to_owned(),
            ..Default::default()
        });
        p.player.account = Some(account.clone());
        w.objects.insert(o).expect("fresh");
        let mut s = SessionData {
            state: SessionState::WorldConnected,
            ..Default::default()
        };
        s.set_account(account_id, name.to_lowercase(), AccessLevel::Player);
        s.set_player(Some(ObjectGuid::new(g)));
        w.sessions.insert(
            SessionId {
                client_id: u16::try_from(account_id).unwrap(),
                generation: 1,
            },
            s,
        );
        let pmgr = &mut w.player_manager;
        assert!(pmgr.online_players.try_add(
            g,
            OnlinePlayer {
                guid: ObjectGuid::new(g),
                account: Some(account)
            }
        ));
        pmgr.player_names.insert(
            OrdinalIgnoreCase(name.to_owned()),
            IPlayer::Online(ObjectGuid::new(g)),
        );
        let character = Character {
            id: g,
            account_id,
            name: name.to_owned(),
            ..Default::default()
        };
        assert!(w.shard.base_database().save_character(&character));
    }
    let (patron, vassal, cleric) = (
        ObjectGuid::new(0x5000_0001),
        ObjectGuid::new(0x5000_0002),
        ObjectGuid::new(0x5000_0003),
    );

    // Vassal swears to Patron; the patron answers the confirmation
    start_capture();
    pa::swear_allegiance(&mut w, vassal, patron.full(), true, false);
    let session = player_manager::player_session(&w, patron).unwrap();
    let request = take_sent()
        .into_iter()
        .filter(|(s, _, b)| {
            *s == session
                && b.len() >= 16
                && u32::from_le_bytes(b[12..16].try_into().unwrap()) == 0x0274
        })
        .map(|(_, _, b)| b)
        .next()
        .expect("the patron is asked");
    let split = split_ui_blob(&request).expect("a blob");
    let mut body = split.body;
    let request = CharacterConfirmationRequest::read(&mut body).expect("decodes");
    assert_eq!(
        request.confirmation_type,
        ConfirmationType::SwearAllegiance.0.cast_signed()
    );
    assert!(
        empyrean_world::world_objects::managers::confirmation_manager::handle_response(
            &mut w,
            patron,
            ConfirmationType::SwearAllegiance,
            request.context_id,
            true,
            false
        )
    );
    assert!(pa::has_allegiance(&w, vassal));

    // the fellowship: Vassal and Cleric
    let f = fellowship::new(&mut w, vassal, "Company", true);
    for g in [vassal, cleric] {
        f.get_mut(&mut w).fellowship_members.try_add(g.full(), g);
        player_fellowship::set_fellowship(&mut w, g, Some(f));
    }

    // quest XP (a flat share, no distance rule): 2,000 -> 1,000 each
    fellowship::split_xp(
        &mut w,
        &f,
        2_000,
        XpType::Quest,
        ShareType::Fellowship,
        vassal,
    );
    run_actions(&mut w, Actor::World);
    assert_eq!(
        w.objects.get(vassal).unwrap().allegiance_xp_generated(),
        0,
        "no allegiance bit: no pass-up"
    );

    fellowship::split_xp(&mut w, &f, 2_000, XpType::Quest, ShareType::All, vassal);
    run_actions(&mut w, Actor::World);
    assert!(
        w.objects.get(vassal).unwrap().allegiance_xp_generated() > 0,
        "the allegiance bit passes the vassal's share up"
    );
    assert!(w.objects.get(patron).unwrap().allegiance_xp_received() > 0);
    let _ = take_sent();
}

/// A kill task credit is shared with fellows in range.
#[test]
fn a_kill_task_credit_is_shared_with_fellows_in_range() {
    use empyrean_entity::enums::DamageType;
    use empyrean_world::entity::damage_history;
    use empyrean_world::world_objects::creature_death;

    let mut h = H::small();
    h.w.content = Arc::new(
        MemContent::new().quest(empyrean_content::models::world::Quest {
            id: 1,
            name: "KillTaskRats".to_owned(),
            max_solves: 10,
            message: Some(String::new()),
            ..Default::default()
        }),
    );
    h.player(A, "Alpha", 3);
    h.player(B, "Bravo", 4);
    h.player(C, "Charlie", 4);
    auto_accept(&mut h, B);
    auto_accept(&mut h, C);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(C));
    for g in [A, B] {
        quest_manager::set_quest_completions(
            &mut h.w,
            &mut QuestOwner::Creature(g),
            "KillTaskRats",
            0,
        );
    }

    // a rat Alpha has hurt
    let rat = ObjectGuid::new(0x8000_0100);
    let mut o = WorldObject::allocate(Class::Creature);
    o.guid = rat;
    o.biota.id = rat.full();
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let mut vitals = empyrean_common::dotnet::DotNetDict::new();
    vitals.insert(
        PropertyAttribute2nd::MaxHealth,
        PropertiesAttribute2nd {
            init_level: 20,
            level_from_cp: 0,
            cp_spent: 0,
            current_level: 20,
        },
    );
    o.biota.properties_attribute_2nd = Some(vitals);
    let cv = CreatureVital::new(&mut o, PropertyAttribute2nd::MaxHealth);
    o.vitals_mut().insert(PropertyAttribute2nd::MaxHealth, cv);
    o.set_property(PropertyString::Name, "Rat".to_owned());
    o.set_location(Some(at(21.0, 20.0)));
    h.w.objects.insert(o).expect("fresh guid");
    damage_history::add(&mut h.w, rat, A, DamageType::Slash, 5);

    creature_death::on_death_handle_kill_task(&mut h.w, rat, "KillTaskRats@rat");
    let count = |w: &World, g: ObjectGuid| {
        quest_manager::get_quest(w, &QuestOwner::Creature(g), "KillTaskRats")
            .map(|q| q.num_times_completed)
    };
    assert_eq!(count(&h.w, A), Some(1), "the killer");
    assert_eq!(count(&h.w, B), Some(1), "the fellow with the task");
    assert_eq!(count(&h.w, C), None, "the fellow without it");

    // `fellow_kt_killer` as its description says (ACE c5f16b44; before it, ACE had the test
    // inverted): off, a killer without the task still passes credit to fellows holding
    // it; on, it does not. Its default is off, as on retail (V377: the retail pcaps credited such
    // fellows); ACE's default is on.
    let charlie_kill = |h: &mut H, guid: u32| {
        let rat = ObjectGuid::new(guid);
        let mut o = WorldObject::allocate(Class::Creature);
        o.guid = rat;
        o.biota.id = rat.full();
        o.biota.properties_enchantment_registry = Some(Vec::new());
        let mut vitals = empyrean_common::dotnet::DotNetDict::new();
        vitals.insert(
            PropertyAttribute2nd::MaxHealth,
            PropertiesAttribute2nd {
                init_level: 20,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 20,
            },
        );
        o.biota.properties_attribute_2nd = Some(vitals);
        let cv = CreatureVital::new(&mut o, PropertyAttribute2nd::MaxHealth);
        o.vitals_mut().insert(PropertyAttribute2nd::MaxHealth, cv);
        o.set_property(PropertyString::Name, "Rat".to_owned());
        o.set_location(Some(at(21.0, 20.0)));
        h.w.objects.insert(o).expect("fresh guid");
        damage_history::add(&mut h.w, rat, C, DamageType::Slash, 5);
        creature_death::on_death_handle_kill_task(&mut h.w, rat, "KillTaskRats@rat");
    };
    assert!(
        !empyrean_world::managers::property_manager::get_bool(
            &h.w,
            "fellow_kt_killer",
            false,
            true
        )
        .item,
        "off by default (retail)"
    );
    charlie_kill(&mut h, 0x8000_0101);
    assert_eq!(
        (count(&h.w, A), count(&h.w, B), count(&h.w, C)),
        (Some(2), Some(2), None),
        "the fellows with the task share"
    );
    empyrean_world::managers::property_manager::modify_bool(&h.w, "fellow_kt_killer", true);
    charlie_kill(&mut h, 0x8000_0102);
    assert_eq!(
        (count(&h.w, A), count(&h.w, B), count(&h.w, C)),
        (Some(2), Some(2), None),
        "on: the killer lacks the task, no credit"
    );
    empyrean_world::managers::property_manager::modify_bool(&h.w, "fellow_kt_killer", false);
}

/// The emote managers fellowship callees reach the fellowship.
#[test]
fn the_emote_managers_fellowship_callees_reach_the_fellowship() {
    use empyrean_world::world_objects::managers::emote_manager::shims as em;
    let mut h = H::small();
    h.player(A, "Alpha", 3);
    h.player(B, "Bravo", 4);
    auto_accept(&mut h, B);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));

    assert!(em::player_has_fellowship(&h.w, B));
    assert!(!em::fellowship_quest_manager_has_quest(
        &h.w,
        B,
        Some("CryptCleared")
    ));
    em::fellowship_quest_manager_stamp(&mut h.w, A, Some("CryptCleared@fellow"));
    assert!(em::fellowship_quest_manager_has_quest(
        &h.w,
        B,
        Some("CryptCleared")
    ));
    assert_eq!(em::fellowship_get_fellowship_members_count(&mut h.w, A), 2);
    assert_eq!(em::fellowship_fellowship_name(&h.w, B), "Company");
    start_capture();
    em::fellowship_broadcast_to_fellow(&mut h.w, A, "Onward");
    let msgs = sent();
    let sa = empyrean_world::managers::player_manager::player_session(&h.w, A).unwrap();
    let sb = empyrean_world::managers::player_manager::player_session(&h.w, B).unwrap();
    // `GameEventChannelBroadcast(FellowBroadcast)` to each member
    assert_eq!(events_to(&msgs, sa), [0x0147]);
    assert_eq!(events_to(&msgs, sb), [0x0147]);
}

/// A fellows death is told to the fellowship.
#[test]
fn a_fellows_death_is_told_to_the_fellowship() {
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    auto_accept(&mut h, B);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));

    start_capture();
    let _ = empyrean_world::world_objects::player_death::player_on_death(
        &mut h.w,
        B,
        None,
        empyrean_entity::enums::DamageType::Slash,
        false,
    );
    let msgs = sent();
    assert!(
        chats_to(&msgs, sa).contains(&"Your fellow Bravo has died!".to_owned()),
        "{:?}",
        chats_to(&msgs, sa)
    );
    let _ = sb;
}

/// The fellow channel reaches the fellowship.
#[test]
fn the_fellow_channel_reaches_the_fellowship() {
    use dereth_protocol::actions::pack_action;
    use dereth_protocol::comms::CommunicationChannelBroadcast;
    use empyrean_world::network::managers::inbound_message_manager::{
        handle_client_message, run_inbound_message_queue,
    };
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    let sc = h.player(C, "Charlie", 4);
    for (i, s) in [sa, sb, sc].into_iter().enumerate() {
        let d = h.w.sessions.get_mut(s).unwrap();
        d.state = empyrean_net::SessionState::WorldConnected;
        // an authenticated session (the squelch check reads `Session.AccountId`)
        d.set_account(
            u32::try_from(i + 1).unwrap(),
            format!("acct{i}"),
            empyrean_entity::enums::AccessLevel::Player,
        );
    }
    let say = |h: &mut H, s: SessionId| {
        start_capture();
        let m = CommunicationChannelBroadcast {
            channel: empyrean_entity::enums::Channel::Fellow.0.cast_unsigned(),
            message: "hello".to_owned(),
        };
        handle_client_message(
            &mut h.w,
            empyrean_net::ClientMessage::new(pack_action(0x10, &m).unwrap()).unwrap(),
            s,
        );
        run_inbound_message_queue(&mut h.w);
        sent()
    };
    // no fellowship: WeenieError
    let msgs = say(&mut h, sa);
    assert_eq!(events_to(&msgs, sa), [0x028A]);

    auto_accept(&mut h, B);
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let msgs = say(&mut h, sa);
    assert_eq!(events_to(&msgs, sb), [0x0147], "the fellow hears it");
    assert_eq!(events_to(&msgs, sa), [0x0147], "the sender's own echo");
    assert!(events_to(&msgs, sc).is_empty(), "not a member");
}

/// V264/V265/V266/V284: a sender squelched by several fellows gets its own line on
/// the Fellow channel once, not once per squelching fellow (ACE echoes it for every member that
/// is the sender or squelches the sender). The squelching fellows still hear nothing.
#[test]
fn a_sender_squelched_by_fellows_gets_one_fellow_echo() {
    use dereth_protocol::actions::pack_action;
    use dereth_protocol::comms::CommunicationChannelBroadcast;
    use empyrean_world::network::managers::inbound_message_manager::{
        handle_client_message, run_inbound_message_queue,
    };
    use empyrean_world::world_objects::managers::squelch_manager;
    const D: ObjectGuid = ObjectGuid::new(0x5000_0004);
    let mut h = H::small();
    let sa = h.player(A, "Alpha", 3);
    let sb = h.player(B, "Bravo", 4);
    let sc = h.player(C, "Charlie", 4);
    let sd = h.player(D, "Delta", 4);
    for (i, s) in [sa, sb, sc, sd].into_iter().enumerate() {
        let d = h.w.sessions.get_mut(s).unwrap();
        d.state = empyrean_net::SessionState::WorldConnected;
        d.set_account(
            u32::try_from(i + 1).unwrap(),
            format!("acct{i}"),
            empyrean_entity::enums::AccessLevel::Player,
        );
    }
    for g in [B, C, D] {
        auto_accept(&mut h, g);
    }
    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    for g in [B, C, D] {
        player_fellowship::fellowship_recruit(&mut h.w, A, Some(g));
    }
    // Bravo and Charlie squelch Alpha's fellowship chat; Delta does not
    for g in [B, C] {
        squelch_manager::handle_action_modify_character_squelch(
            &mut h.w,
            g,
            true,
            A.full(),
            "",
            empyrean_entity::enums::ChatMessageType::Fellowship,
        );
    }

    start_capture();
    let m = CommunicationChannelBroadcast {
        channel: empyrean_entity::enums::Channel::Fellow.0.cast_unsigned(),
        message: "hello".to_owned(),
    };
    handle_client_message(
        &mut h.w,
        empyrean_net::ClientMessage::new(pack_action(0x10, &m).unwrap()).unwrap(),
        sa,
    );
    run_inbound_message_queue(&mut h.w);
    let msgs = sent();
    assert_eq!(events_to(&msgs, sa), [0x0147], "one echo for the sender");
    assert!(events_to(&msgs, sb).is_empty(), "Bravo squelches Alpha");
    assert!(events_to(&msgs, sc).is_empty(), "Charlie squelches Alpha");
    assert_eq!(events_to(&msgs, sd), [0x0147], "Delta hears it");
}

/// Fellowships live in the world store while referenced.
#[test]
fn fellowships_live_in_the_world_store_while_referenced() {
    let mut h = H::small();
    h.player(A, "Alpha", 3);
    h.player(B, "Bravo", 4);
    auto_accept(&mut h, B);
    assert!(h.w.fellowships.is_empty());

    player_fellowship::fellowship_create(&mut h.w, A, "Company", true);
    player_fellowship::fellowship_recruit(&mut h.w, A, Some(B));
    let first = h.fellowship(A).expect("created");
    assert_eq!(h.w.fellowships.len(), 1);

    player_fellowship::fellowship_quit(&mut h.w, A, true);
    assert_eq!(
        (h.fellowship(A), h.fellowship(B)),
        (None, None),
        "disbanded"
    );
    assert_eq!(h.w.fellowships.len(), 1, "kept until the next one is made");

    player_fellowship::fellowship_create(&mut h.w, B, "Second", true);
    let second = h.fellowship(B).expect("created");
    assert_ne!(first, second);
    assert_eq!(h.w.fellowships.len(), 1, "the disbanded one is gone");
    assert_eq!(second.get(&h.w).fellowship_name, "Second");
}
