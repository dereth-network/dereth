//! Vectors: fixtures/vectors/allegiance/
//! Allegiance init/officer order/DoPassXP replay ACE vectors; retail tithe, effective
//! loyalty/leadership (uncapped, rounded) and pass-through rates fit recorded sessions; hierarchy
//! record carries times sworn.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use std::sync::Arc;
use std::time::Duration;

use dereth_primitives::ObjectId;
use dereth_protocol::actions::pack_action;
use dereth_protocol::comms::{
    CharacterConfirmationRequest, CommunicationTextboxString, CommunicationWeenieError,
};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::social::{
    AllegianceAddAllegianceBan, AllegianceBreakAllegiance, AllegianceChatBoot, AllegianceChatGag,
    AllegianceClearAllegianceName, AllegianceClearAllegianceOfficerTitles,
    AllegianceClearAllegianceOfficers, AllegianceClearMotd, AllegianceDoAllegianceLockAction,
    AllegianceInfoRequest, AllegianceInfoResponse, AllegianceListAllegianceBans,
    AllegianceListAllegianceOfficerTitles, AllegianceListAllegianceOfficers,
    AllegianceQueryAllegianceName, AllegianceQueryMotd, AllegianceRemoveAllegianceBan,
    AllegianceRemoveAllegianceOfficer, AllegianceSetAllegianceApprovedVassal,
    AllegianceSetAllegianceName, AllegianceSetAllegianceOfficer,
    AllegianceSetAllegianceOfficerTitle, AllegianceSetMotd, AllegianceUpdate,
    AllegianceUpdateRequest,
};
use dereth_protocol::Message;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::vectors::{self, u64_of};
use empyrean_content::models::world::Weenie as ContentWeenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AccessLevel, AllegianceLockAction, CharacterOption, ConfirmationType, PropertyBool,
    PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString, ShareType,
    WeenieError, WeenieType, XpType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::{ClientMessage, SessionId, SessionState};
use empyrean_store::models::auth::Account;
use empyrean_store::models::shard::Character;
use empyrean_store::MemShard;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::actions::action_queue::run_actions;
use empyrean_world::entity::actions::i_actor::Actor;
use empyrean_world::entity::allegiance_node::NodeRef;
use empyrean_world::entity::i_player::IPlayer;
use empyrean_world::entity::offline_player::OfflinePlayer;
use empyrean_world::managers::allegiance_manager;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::player_manager::{self, OnlinePlayer, OrdinalIgnoreCase};
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::managers::inbound_message_manager::{
    handle_client_message, run_inbound_message_queue,
};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::managers::confirmation_manager;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{allegiance, player, player_allegiance as pa};
use empyrean_world::World;

const M: u32 = 0x5000_0001; // Mona, the monarch-to-be (level 50)
const V: u32 = 0x5000_0002; // Vass (level 20)
const C: u32 = 0x5000_0003; // Cora (level 10)
const D: u32 = 0x5000_0004; // Dora, offline (level 5)
const SM: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const SV: SessionId = SessionId {
    client_id: 2,
    generation: 1,
};
const SC: SessionId = SessionId {
    client_id: 3,
    generation: 1,
};

const PLAYER_WCID: u32 = 1;
const ALLEGIANCE_WCID: u32 = 1149;

const GAME_EVENT: u32 = 0xF7B0;
const CHAT: u32 = 0xF7E0;
const WEENIE_ERROR: u32 = 0x028A;
const ALLEGIANCE_UPDATE: u32 = 0x0020;
const UPDATE_DONE: u32 = 0x01C8;
const CONFIRMATION_REQUEST: u32 = 0x0274;
const CONFIRMATION_DONE: u32 = 0x0276;
const INFO_RESPONSE: u32 = 0x027C;
const PRIVATE_UPDATE_INT: u32 = 0x02CD;

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn account(account_id: u32, name: &str) -> Account {
    Account {
        account_id,
        account_name: name.to_owned(),
        ..Account::default()
    }
}

fn guid(g: u32) -> ObjectGuid {
    ObjectGuid::new(g)
}

/// An online player (as `PlayerEnterWorld` leaves it, without a landblock).
fn add_online(w: &mut World, g: u32, name: &str, level: i32, account_id: u32, session: SessionId) {
    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        player::player_from_weenie(env, Class::Player, weenie, guid(g), account_id)
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
    p.player.account = Some(account(account_id, name));
    w.objects.insert(o).expect("fresh");

    let mut s = SessionData {
        state: SessionState::WorldConnected,
        ..Default::default()
    };
    s.set_account(account_id, name.to_lowercase(), AccessLevel::Player);
    s.set_player(Some(guid(g)));
    w.sessions.insert(session, s);

    let pmgr = &mut w.player_manager;
    assert!(pmgr.online_players.try_add(
        g,
        OnlinePlayer {
            guid: guid(g),
            account: Some(account(account_id, name))
        }
    ));
    pmgr.player_names
        .insert(OrdinalIgnoreCase(name.to_owned()), IPlayer::Online(guid(g)));
    pmgr.player_accounts
        .get_or_insert_with(account_id, DotNetDict::new)
        .insert(g, IPlayer::Online(guid(g)));
}

fn add_offline(w: &mut World, g: u32, name: &str, level: i32, account_id: u32) {
    let mut biota = empyrean_entity::Biota {
        id: g,
        weenie_class_id: PLAYER_WCID,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, name.to_owned());
    biota.set_property(PropertyInt::Level, level);
    biota.set_property(PropertyInt::Gender, 2);
    biota.set_property(PropertyInt::HeritageGroup, 2);
    let offline = OfflinePlayer {
        biota,
        guid: guid(g),
        account: Some(account(account_id, name)),
        last_requested_database_save: DotNetDateTime::MIN_VALUE,
        changes_detected: false,
        allegiance: None,
        allegiance_node: None,
    };
    let pmgr = &mut w.player_manager;
    pmgr.offline_players.insert(g, offline);
    pmgr.player_names.insert(
        OrdinalIgnoreCase(name.to_owned()),
        IPlayer::Offline(guid(g)),
    );
    pmgr.player_accounts
        .get_or_insert_with(account_id, DotNetDict::new)
        .insert(g, IPlayer::Offline(guid(g)));
}

fn bare_world() -> World {
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
                ALLEGIANCE_WCID,
                "allegiance",
                WeenieType::Allegiance,
            )),
    );
    guid_manager::initialize(&mut w, &mut EmptyShard);
    pm::install_shard_config(&mut w, pm::shard_config_handle(Box::new(MemShard::new())));
    pm::initialize(&mut w, true);
    w
}

fn world() -> World {
    let mut w = bare_world();
    add_online(&mut w, M, "Mona", 50, 1, SM);
    add_online(&mut w, V, "Vass", 20, 2, SV);
    add_online(&mut w, C, "Cora", 10, 3, SC);
    add_offline(&mut w, D, "Dora", 5, 4);
    // the characters exist in the shard (the allegiance rows reference them)
    for (g, name, account_id) in [
        (M, "Mona", 1),
        (V, "Vass", 2),
        (C, "Cora", 3),
        (D, "Dora", 4),
    ] {
        let character = Character {
            id: g,
            account_id,
            name: name.to_owned(),
            ..Default::default()
        };
        assert!(w.shard.base_database().save_character(&character));
    }
    w
}

type Sent = Vec<(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)>;

fn u32_at(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
}

fn kind(b: &[u8]) -> u32 {
    if u32_at(b, 0) == GAME_EVENT {
        u32_at(b, 12)
    } else {
        u32_at(b, 0)
    }
}

fn to(sent: &Sent, session: SessionId) -> Vec<Vec<u8>> {
    sent.iter()
        .filter(|(s, _, _)| *s == session)
        .map(|(_, _, b)| b.clone())
        .collect()
}

fn kinds_to(sent: &Sent, session: SessionId) -> Vec<u32> {
    to(sent, session).iter().map(|b| kind(b)).collect()
}

fn decode<M2: Message>(blob: &[u8]) -> M2 {
    let split = split_ui_blob(blob).expect("a blob");
    let mut body = split.body;
    M2::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", kind(blob)))
}

/// Every chat line `session` got, in order.
fn chats(sent: &Sent, session: SessionId) -> Vec<String> {
    to(sent, session)
        .iter()
        .filter(|b| kind(b) == CHAT)
        .map(|b| decode::<CommunicationTextboxString>(b).text)
        .collect()
}

/// Every weenie error `session` got, in order.
fn errors(sent: &Sent, session: SessionId) -> Vec<u32> {
    to(sent, session)
        .iter()
        .filter(|b| kind(b) == WEENIE_ERROR)
        .map(|b| decode::<CommunicationWeenieError>(b).error_type)
        .collect()
}

fn we(e: WeenieError) -> u32 {
    e.0.cs_cast()
}

/// Runs a client game action from `session` and returns everything sent meanwhile.
fn act<M2: Message>(w: &mut World, session: SessionId, m: &M2) -> Sent {
    start_capture();
    let data = pack_action(0x10, m).expect("encode");
    handle_client_message(w, ClientMessage::new(data).expect("opcode"), session);
    run_inbound_message_queue(w);
    take_sent()
}

fn capture(w: &mut World, f: impl FnOnce(&mut World)) -> Sent {
    start_capture();
    f(w);
    take_sent()
}

fn obj(w: &World, g: u32) -> &WorldObject {
    w.objects.get(guid(g)).expect("present")
}

fn allegiance_of(w: &World, g: u32) -> Option<ObjectGuid> {
    pa::i_player_allegiance(w, IPlayer::Online(guid(g)))
}

fn rank_of(w: &World, alleg: ObjectGuid, g: u32) -> u32 {
    let id = allegiance::member_node(
        w,
        NodeRef {
            allegiance: alleg,
            player: guid(g),
        },
    )
    .expect("a member");
    allegiance::tree(w, alleg).unwrap().node(id).rank
}

/// `vassal` swears to `patron` through the confirmation: the patron is asked, then answers yes.
/// Returns what the answer sent.
fn swear(w: &mut World, vassal: u32, patron: u32) -> Sent {
    let asked = capture(w, |w| {
        pa::swear_allegiance(w, guid(vassal), patron, true, false)
    });
    let session = player_manager::player_session(w, guid(patron)).unwrap();
    let request = to(&asked, session)
        .into_iter()
        .find(|b| kind(b) == CONFIRMATION_REQUEST)
        .expect("the patron is asked");
    let request: CharacterConfirmationRequest = decode(&request);
    assert_eq!(
        request.confirmation_type,
        ConfirmationType::SwearAllegiance.0.cast_signed()
    );
    capture(w, |w| {
        assert!(confirmation_manager::handle_response(
            w,
            guid(patron),
            ConfirmationType::SwearAllegiance,
            request.context_id,
            true,
            false
        ));
    })
}

// ------------------------------------------------------------------ vectors (ACE's compiled code)

/// Builds the vector case's players as offline players, in input order.
fn vector_world(members: &serde_json::Value, xp: bool) -> World {
    let mut w = bare_world();
    for m in members.as_array().unwrap() {
        let g = u32::try_from(u64_of(&m["guid"]).unwrap()).unwrap();
        let mut biota = empyrean_entity::Biota {
            id: g,
            weenie_class_id: PLAYER_WCID,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        biota.set_property(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        );
        let u = |k: &str| u64_of(&m[k]).map(|v| u32::try_from(v).unwrap());
        let i = |k: &str| m[k].as_i64();
        if let Some(p) = u("patron") {
            biota.set_property(PropertyInstanceId::Patron, p);
        }
        if let Some(p) = u("monarch") {
            biota.set_property(PropertyInstanceId::Monarch, p);
        }
        if let Some(r) = i("officer_rank") {
            biota.set_property(
                PropertyInt::AllegianceOfficerRank,
                i32::try_from(r).unwrap(),
            );
        }
        if xp {
            if let Some(v) = i("loyalty") {
                biota.set_property(
                    PropertyInt::CurrentLoyaltyAtLastLogoff,
                    i32::try_from(v).unwrap(),
                );
            }
            if let Some(v) = i("leadership") {
                biota.set_property(
                    PropertyInt::CurrentLeadershipAtLastLogoff,
                    i32::try_from(v).unwrap(),
                );
            }
            if let Some(v) = m["existed"].as_bool() {
                biota.set_property(PropertyBool::ExistedBeforeAllegianceXpChanges, v);
            }
            if let Some(v) = i("cached") {
                biota.set_property(PropertyInt64::AllegianceXPCached, v);
            }
            if let Some(v) = i("generated") {
                biota.set_property(PropertyInt64::AllegianceXPGenerated, v);
            }
            // V285: every member sworn long past both caps, the time term ACE pins
            biota.set_property(
                PropertyFloat::AllegianceSwearTimestamp,
                w.now.unix_time - 800.0 * 86_400.0,
            );
            biota.set_property(PropertyInt::AllegianceSwearTimestamp, -800 * 3_600);
        }
        let offline = OfflinePlayer {
            biota,
            guid: guid(g),
            account: None,
            last_requested_database_save: DotNetDateTime::MIN_VALUE,
            changes_detected: false,
            allegiance: None,
            allegiance_node: None,
        };
        w.player_manager.offline_players.insert(g, offline);
    }
    w
}

#[test]
fn allegiance_init_matches_ace() {
    let file = vectors::load_named("allegiance", "trees");
    assert!(file.cases.len() >= 250);
    for (n, c) in file.cases.iter().enumerate() {
        let w = vector_world(&c.input["members"], false);
        let a = allegiance::allegiance_from_monarch(&w, guid(M));
        let f = allegiance::fields(&a).unwrap();

        let got: Vec<(u64, u64, u64, i64, i64)> = f
            .members()
            .iter()
            .map(|(g, &id)| {
                let node = f.tree.node(id);
                let patron = node
                    .patron
                    .map_or(0, |p| u64::from(f.tree.node(p).player_guid.full()));
                (
                    u64::from(g.full()),
                    u64::from(node.rank),
                    patron,
                    i64::from(f.tree.total_vassals(id)),
                    i64::from(f.tree.total_followers(id)),
                )
            })
            .collect();
        let want: Vec<(u64, u64, u64, i64, i64)> = c.output["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    u64_of(&m["guid"]).unwrap(),
                    u64_of(&m["rank"]).unwrap(),
                    u64_of(&m["patron"]).unwrap(),
                    m["total_vassals"].as_i64().unwrap(),
                    m["total_followers"].as_i64().unwrap(),
                )
            })
            .collect();
        assert_eq!(got, want, "case {n}: members");

        let officers: Vec<u64> = f.officers().keys().map(|g| u64::from(g.full())).collect();
        let want: Vec<u64> = c.output["officers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| u64_of(o).unwrap())
            .collect();
        assert_eq!(officers, want, "case {n}: officers");
    }
}

#[test]
fn do_pass_xp_matches_ace() {
    let file = vectors::load_named("allegiance", "pass_xp");
    assert!(file.cases.len() >= 600);
    let (mut passed_up, mut ace_kept) = (0, 0);
    for (n, c) in file.cases.iter().enumerate() {
        let mut w = vector_world(&c.input["members"], true);
        pm::modify_bool(
            &w,
            "offline_xp_passup_limit",
            c.input["limit"].as_bool().unwrap(),
        );

        // the allegiance object, as GetAllegiance leaves it (any guid: DoPassXP never reads it)
        let mut a = WorldObject::allocate(Class::Allegiance);
        a.guid = guid(0x7000_0001);
        allegiance::init(&w, &mut a, guid(M));
        w.objects.insert(a).expect("fresh");

        let from = guid(u32::try_from(u64_of(&c.input["from"]).unwrap()).unwrap());
        let amount = u64_of(&c.input["amount"]).unwrap();
        allegiance_manager::do_pass_xp(
            &mut w,
            NodeRef {
                allegiance: guid(0x7000_0001),
                player: from,
            },
            amount,
            c.input["direct"].as_bool().unwrap(),
        );

        // V285: the ruled chain, every member sworn past both
        // caps (the time term ACE pins). ACE's recorded vassal tithe stays the record on the
        // direct step it shares with retail (Loyalty within 291, both passing something up: the
        // same percentage, truncated in float where retail rounds at double precision, so within
        // 1 XP, or float's error for amounts past 2^24); the rest is retail's (`retail_pass_xp`).
        let want = retail_pass_xp(&w, guid(0x7000_0001), &c.input);
        for m in c.output.as_array().unwrap() {
            let g = u32::try_from(u64_of(&m["guid"]).unwrap()).unwrap();
            let p = IPlayer::Offline(guid(g));
            let (generated, cached) = want[&g];
            assert_eq!(
                pa::i_player_allegiance_xp_generated(&w, p),
                generated,
                "case {n}: {g:08X} generated"
            );
            assert_eq!(
                empyrean_world::entity::i_player::allegiance_xp_cached(&w, p),
                cached,
                "case {n}: {g:08X} cached"
            );
        }
        let from_in = c.input["members"]
            .as_array()
            .unwrap()
            .iter()
            .position(|m| u64_of(&m["guid"]) == Some(u64::from(from.full())))
            .unwrap();
        let from_loyalty = c.input["members"][from_in]["loyalty"].as_i64().unwrap_or(0);
        let before: u64 = c.input["members"][from_in]["generated"]
            .as_i64()
            .unwrap_or(0)
            .cs_cast();
        let ace = u64_of(&c.output[from_in]["generated"])
            .unwrap()
            .wrapping_sub(before);
        let got =
            pa::i_player_allegiance_xp_generated(&w, IPlayer::Offline(from)).wrapping_sub(before);
        if c.input["direct"].as_bool().unwrap() && from_loyalty <= 291 && ace > 0 && got > 0 {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let float_error = ((amount as f64) * 4e-7) as u64;
            assert!(
                got.abs_diff(ace) <= 1.max(float_error),
                "case {n}: the vassal's tithe {got} against ACE's {ace}"
            );
            ace_kept += 1;
        }
        if c.output
            .as_array()
            .unwrap()
            .iter()
            .zip(c.input["members"].as_array().unwrap())
            .any(|(o, i)| {
                CsCast::<u64>::cs_cast(i["cached"].as_i64().unwrap_or(0))
                    != u64_of(&o["cached"]).unwrap()
            })
        {
            passed_up += 1;
        }
    }
    assert!(
        passed_up > 200,
        "the grid passes XP up in most cases ({passed_up})"
    );
    assert!(
        ace_kept > 200,
        "ACE's record checks the shared direct step in most cases ({ace_kept})"
    );
}

// ------------------------------------------------------------------ V285: the retail pcaps

/// An offline allegiance for the V285 cases: `(guid, patron, loyalty, leadership, real seconds
/// sworn, in-game seconds sworn)` per member, the first the monarch `M`. Returns the world and
/// the allegiance guid.
#[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
fn sworn_world(members: &[(u32, Option<u32>, u32, u32, u32, u32)]) -> (World, ObjectGuid) {
    let json: Vec<serde_json::Value> = members
        .iter()
        .map(|&(g, patron, loyalty, leadership, _, _)| {
            serde_json::json!({ "guid": g, "patron": patron, "monarch": patron.map(|_| M), "loyalty": loyalty, "leadership": leadership, "existed": true })
        })
        .collect();
    let mut w = vector_world(&serde_json::Value::Array(json), true);
    for &(g, _, _, _, real, game) in members {
        let p = IPlayer::Offline(guid(g));
        let age = 5_000_000;
        empyrean_world::entity::i_player::set_property(&mut w, p, PropertyInt::Age, age);
        empyrean_world::entity::i_player::set_property(
            &mut w,
            p,
            PropertyInt::AllegianceSwearTimestamp,
            age - i32::try_from(game).unwrap(),
        );
        let now = w.now.unix_time;
        empyrean_world::entity::i_player::set_property(
            &mut w,
            p,
            PropertyFloat::AllegianceSwearTimestamp,
            now - f64::from(real),
        );
    }
    let mut a = WorldObject::allocate(Class::Allegiance);
    a.guid = guid(0x7000_0001);
    allegiance::init(&w, &mut a, guid(M));
    w.objects.insert(a).expect("fresh");
    (w, guid(0x7000_0001))
}

const DAY: u32 = 86_400;
const HOUR: f64 = 3_600.0;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn hours(h: f64) -> u32 {
    (h * HOUR).round() as u32
}

/// The tithe (`AllegianceXPGenerated`) `g` has after XP awards of `amounts` from its own node.
fn tithe_after(w: &mut World, alleg: ObjectGuid, g: u32, amounts: &[u64], direct: bool) -> u64 {
    for &amount in amounts {
        allegiance_manager::do_pass_xp(
            w,
            NodeRef {
                allegiance: alleg,
                player: guid(g),
            },
            amount,
            direct,
        );
    }
    pa::i_player_allegiance_xp_generated(w, IPlayer::Offline(guid(g)))
}

/// Stargren (session 15989): Loyalty 142, 91.27 in-game hours, 2,795 real days (capped), so
/// `E = round(142 * 1.12676) = 160`, Generated 62.37%. 2 x 130,000,000 + 8 x 13,000,000 XP
/// tithed 227,030,934.
#[test]
fn stargren_tithes_as_recorded() {
    let (mut w, a) = sworn_world(&[
        (M, None, 0, 0, 0, 0),
        (V, Some(M), 142, 0, 2795 * DAY, hours(91.27)),
    ]);
    let mut awards = vec![130_000_000u64; 2];
    awards.extend([13_000_000u64; 8]);
    assert_eq!(awards.iter().sum::<u64>(), 364_000_000);
    assert_eq!(tithe_after(&mut w, a, V, &awards, true), 227_030_934);
}

/// Russet (session 4478): Loyalty 5, 59.57 in-game hours, 1,600 real days, so
/// `E = round(5.41) = 5`; every 2 x 80,000 XP tithed 80,618.
#[test]
fn russet_tithes_as_recorded() {
    let (mut w, a) = sworn_world(&[
        (M, None, 0, 0, 0, 0),
        (V, Some(M), 5, 0, 1600 * DAY, hours(59.57)),
    ]);
    assert_eq!(tithe_after(&mut w, a, V, &[80_000, 80_000], true), 80_618);
    assert_eq!(
        tithe_after(&mut w, a, V, &[80_000, 80_000], true),
        2 * 80_618,
        "and again"
    );
}

/// The generated fraction for a vassal's own XP at effective Loyalty `e`.
#[allow(clippy::cast_possible_truncation)]
fn generated_at(e: f64) -> f32 {
    ((50.0 + 22.5 * e / 291.0) * 0.01) as f32
}

fn factors(loyalty: u32, real: u32, game: u32) -> allegiance_manager::PassupFactors {
    let t = allegiance_manager::SwornDays::from(pa::SwornTime {
        time_online: game,
        allegiance_age: real,
    });
    allegiance_manager::passup_factors(
        loyalty,
        0,
        1,
        true,
        t,
        allegiance_manager::SwornDays::default(),
    )
}

/// E rounds to nearest: the captures' decisive cases, where floor gives one less (or more).
#[test]
fn effective_loyalty_rounds_to_nearest() {
    for (who, loyalty, ig, rt, e) in [
        ("Chat Mule", 50, 42.41, 1025, 53.0), // 52.95
        ("Taisara", 70, 6.22, 3699, 71.0),    // 70.60
        ("Akea", 105, 252.8, 4657, 142.0),    // 141.87
        ("Branor", 215, 200.6, 4722, 275.0),  // 274.89
        ("Ripley", 130, 155.9, 4817, 158.0),  // 158.14
        ("Corran Li", 10, 22.2, 4175, 10.0),  // 10.31
        ("Hydroptic", 50, 261.6, 30, 51.0),   // RT under 730 days: 50.75
        ("Deletorious", 50, 53.6, 323, 52.0), // 51.65
    ] {
        assert_eq!(
            factors(loyalty, rt * DAY, hours(ig)).generated,
            generated_at(e),
            "{who}"
        );
    }
    // no time sworn: E is the Loyalty (ACE's pinned maximum would double it)
    assert_eq!(factors(100, 0, 0).generated, generated_at(100.0));
    // a half rounds up: 2 * (1 + 1/4) = 2.5 -> 3 (RT capped, IG a quarter of the cap)
    assert_eq!(
        factors(2, 800 * DAY, 180 * 3_600).generated,
        generated_at(3.0)
    );
}

/// Neither Loyalty nor E is capped at 291 (the captures fit 38 intervals past it uncapped).
#[test]
fn effective_loyalty_is_not_capped() {
    // Loyalty 250 sworn past both caps: E = 500, Generated 88.66%
    assert_eq!(
        factors(250, 800 * DAY, 800 * 3_600).generated,
        generated_at(500.0)
    );
    // a buffed Loyalty past 291 with no time sworn
    assert_eq!(factors(350, 0, 0).generated, generated_at(350.0));
    let (mut w, a) = sworn_world(&[
        (M, None, 0, 0, 0, 0),
        (V, Some(M), 281, 0, 800 * DAY, 800 * 3_600),
    ]);
    assert_eq!(
        tithe_after(&mut w, a, V, &[1_000_000], true),
        allegiance_manager::passup_amount(1_000_000, generated_at(562.0))
    );
}

/// The pass-through: a patron passes on `5.5% * E/291` of what its vassals passed to it (the
/// captures' offline vassals, to the unit).
#[test]
fn pass_through_is_five_and_a_half_percent_of_effective_loyalty() {
    // Vistar: Loyalty 50, 15.4 in-game hours, RT capped: E = 51; +515,488 cached tithed +4,969
    // (4,968.9: floor would give 4,968)
    let (mut w, a) = sworn_world(&[
        (M, None, 0, 0, 0, 0),
        (V, Some(M), 50, 0, 2000 * DAY, hours(15.4)),
    ]);
    assert_eq!(tithe_after(&mut w, a, V, &[515_488], false), 4_969);
    // Might of the Bow (session 3837): Loyalty 150, 86.9 in-game hours: E = 168
    let (mut w, a) = sworn_world(&[
        (M, None, 0, 0, 0, 0),
        (V, Some(M), 150, 0, 2000 * DAY, hours(86.9)),
    ]);
    assert_eq!(tithe_after(&mut w, a, V, &[196_084], false), 6_226);
    assert_eq!(tithe_after(&mut w, a, V, &[191_954], false), 6_226 + 6_095);
    // zero at E = 0 (ACE's recursion passes 16% at any Loyalty)
    let t = allegiance_manager::SwornDays::default();
    assert_eq!(
        allegiance_manager::passup_factors(0, 0, 1, false, t, t).generated,
        0.0
    );
}

/// The whole chain: a vassal's XP reaches its patron, and the patron's pass-through its own
/// patron, each at the ruled percentages.
#[test]
fn the_chain_passes_through_at_the_retail_rate() {
    const P: u32 = 0x5000_0002;
    const X: u32 = 0x5000_0003;
    let (mut w, a) = sworn_world(&[
        (M, None, 0, 100, 0, 0),
        (P, Some(M), 150, 100, 2000 * DAY, hours(86.9)),
        (X, Some(P), 100, 0, 2000 * DAY, 800 * 3_600),
    ]);
    allegiance_manager::do_pass_xp(
        &mut w,
        NodeRef {
            allegiance: a,
            player: guid(X),
        },
        1_000_000,
        true,
    );
    let times = |real: u32, game: u32| {
        allegiance_manager::SwornDays::from(pa::SwornTime {
            time_online: game,
            allegiance_age: real,
        })
    };
    let direct = allegiance_manager::passup_factors(
        100,
        100,
        1,
        true,
        times(2000 * DAY, 800 * 3_600),
        times(2000 * DAY, 800 * 3_600),
    );
    let (tithe_x, to_p) = direct.amounts(1_000_000);
    let offline = |g: u32| IPlayer::Offline(guid(g));
    assert_eq!(
        tithe_x,
        allegiance_manager::passup_amount(1_000_000, direct.generated)
    );
    assert_eq!(
        to_p,
        allegiance_manager::passup_amount(tithe_x, direct.received),
        "the patron's share of the rounded tithe"
    );
    assert_eq!(
        pa::i_player_allegiance_xp_generated(&w, offline(X)),
        tithe_x
    );
    assert_eq!(
        empyrean_world::entity::i_player::allegiance_xp_cached(&w, offline(P)),
        to_p
    );
    let through = allegiance_manager::passup_factors(
        150,
        100,
        1,
        false,
        times(2000 * DAY, hours(86.9)),
        times(2000 * DAY, hours(86.9)),
    );
    #[allow(clippy::cast_possible_truncation)] // the f32 the server stores
    let expected = ((5.5 * 168.0 / 291.0) * 0.01f64) as f32;
    assert_eq!(through.generated, expected);
    // V285/V299: the grandpatron receives the pass-through at the ordinary Received %:
    // Leadership 100, one vassal (P) at RT's cap and 86.9 h: round(100 * 1.1594) = 116
    assert_eq!(through.received, generated_at(116.0));
    let (tithe_p, to_m) = through.amounts(to_p);
    assert_eq!(
        pa::i_player_allegiance_xp_generated(&w, offline(P)),
        tithe_p
    );
    assert_eq!(
        empyrean_world::entity::i_player::allegiance_xp_cached(&w, offline(M)),
        to_m
    );
}

// ------------------------------------------------------------------ V285/V299: the patron side

/// `(in-game hours, real days)` of a vassal, as a time sworn in whole seconds.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn sworn(ig: f64, rt: f64) -> pa::SwornTime {
    pa::SwornTime {
        time_online: hours(ig),
        allegiance_age: (rt * f64::from(DAY)).round() as u32,
    }
}

/// The patron's factors for Leadership `l` over vassals sworn `(in-game hours, real days)`.
fn patron_factors(
    l: u32,
    vassals: &[(f64, f64)],
    direct: bool,
) -> allegiance_manager::PassupFactors {
    let times: Vec<pa::SwornTime> = vassals.iter().map(|&(ig, rt)| sworn(ig, rt)).collect();
    let n = i32::try_from(vassals.len()).unwrap();
    allegiance_manager::passup_factors(
        0,
        l,
        n,
        direct,
        allegiance_manager::SwornDays::default(),
        allegiance_manager::sworn_average(&times),
    )
}

/// The effective Leadership `round(L * (1 + 0.3 * V + 0.7 * RT2/730 * IG2/720))` of the
/// captures' patrons (live buffed Leadership, the vassals' times), on both steps.
#[test]
fn effective_leadership_fits_the_recorded_sessions() {
    for (who, l, vassals, e) in [
        (
            "Ferah Palacost",
            240,
            &[(86.94, 1257.0), (297.93, 1330.0)][..],
            321.0,
        ), // 320.90
        ("Cheleth", 45, &[(24.07, 847.0), (17.67, 845.0)][..], 53.0), // 52.66
        (
            "Ujiio",
            5,
            &[(15.89, 7.1), (1.51, 0.6), (3.90, 653.0), (0.75, 652.0)][..],
            7.0,
        ), // 6.51
        ("Xanxin", 5, &[(88.29, 1058.0)][..], 6.0),                   // 5.80
        ("Llllllllllllllllllll", 5, &[(0.0, 0.0)][..], 5.0),          // 5.375
        (
            "Celeth",
            145,
            &[(106.8, 2000.0), (42.41, 1025.0)][..],
            177.0,
        ), // 177.25
        ("Zech Tinker", 245, &[(20.91, 69.9)][..], 264.0),            // 263.85
        ("Hallaniel", 45, &[(59.57, 1600.0)][..], 51.0),              // 50.98
        ("Cor Overload", 40, &[(50.58, 2409.0)][..], 45.0),           // 44.97
        ("Darth Klatu", 35, &[(11.06, 4.7)][..], 38.0),               // 37.63
        ("Blvit", 45, &[(2.27, 828.0)][..], 48.0),                    // 48.47
        ("Svet", 5, &[(542.18, 338.0)][..], 7.0),                     // 6.60
        ("Kronos Lord", 40, &[(7.88, 1.9)][..], 43.0),                // 43.00
    ] {
        assert_eq!(
            patron_factors(l, vassals, true).received,
            generated_at(e),
            "{who}"
        );
        assert_eq!(
            patron_factors(l, vassals, false).received,
            generated_at(e),
            "{who}: the pass-through step"
        );
    }
    // Leadership 0 receives exactly half (the captures' Billy Herrington and Tunzi)
    assert_eq!(patron_factors(0, &[(100.0, 100.0)], true).received, 0.5);
    // R(321) as the captures show it
    assert_eq!(generated_at(321.0), 0.748_195_9);
}

/// Neither Leadership nor E is capped at 291, and the vassal count counts up to 4.
#[test]
fn effective_leadership_is_not_capped() {
    // Archie Summons: Leadership 448, two vassals, a time term near zero: round(448 * 1.15) = 515
    assert_eq!(
        patron_factors(448, &[(0.0, 0.0), (0.0, 0.0)], true).received,
        generated_at(515.0)
    );
    // at the ceiling: 4 vassals at both caps doubles the Leadership
    assert_eq!(
        patron_factors(400, &[(800.0, 800.0); 4], true).received,
        generated_at(800.0)
    );
    // 8 vassals count as 4 (Deran Dark, X-force): Leadership 45 with no time sworn, 58.5 -> 59
    assert_eq!(
        patron_factors(45, &[(0.0, 0.0); 8], true).received,
        generated_at(59.0)
    );
    assert_eq!(
        patron_factors(45, &[(0.0, 0.0); 4], true).received,
        generated_at(59.0)
    );
    // three vassals: 0.225
    assert_eq!(
        patron_factors(200, &[(0.0, 0.0); 3], true).received,
        generated_at(245.0)
    );
}

/// The patron's gain is its Received % of the vassal's rounded tithe, rounded to nearest (the
/// captures' single tithes, to the unit).
#[test]
fn the_patron_gain_rounds_from_the_tithe() {
    for (who, e, tithe, gain) in [
        ("Ferah Palacost", 321.0, 6_153, 4_604), // 4,603.65: floor would give 4,603
        ("Ferah Palacost", 321.0, 6_095, 4_560),
        ("Ferah Palacost", 321.0, 6_226, 4_658),
        ("Deran Dark", 59.0, 102_451, 55_899),
        ("Deran Dark", 59.0, 88_736, 48_416),
        ("Might of the Bow", 52.0, 409_027, 220_959), // 220,958.92
        ("Llllllllllllllllllll", 5.0, 96_706, 48_727), // 48,726.86
        ("Xanxin", 6.0, 568_763, 287_020), // half of the +1,137,526 interval (2 x 287,020)
        ("Cheleth", 53.0, 16_667_856, 9_016_966),
        ("Zech Tinker", 264.0, 1_277_751, 899_695), // 899,694.81
        ("Hallaniel", 51.0, 75_580, 40_770),
        ("Svet", 7.0, 145_826, 73_702),
    ] {
        let f = allegiance_manager::PassupFactors {
            generated: 1.0,
            received: generated_at(e),
        };
        assert_eq!(f.amounts(tithe), (tithe, gain), "{who}");
    }
    // the tithe is rounded first: 5 XP at 50% tithes 3 (2.5 rounded) and the patron gains 2
    // (1.5 rounded), where the single product 5 * 0.25 = 1.25 would give 1
    let f = allegiance_manager::PassupFactors {
        generated: 0.5,
        received: 0.5,
    };
    assert_eq!(f.amounts(5), (3, 2));
}

/// Cheleth (session 10502) passes on `5.5% * E_loy/291` of what it received, per event, with its
/// Loyalty's E (Loyalty 50, 106.8 in-game hours, RT capped: 57): received 4,384,998 and
/// 4,631,968, tithed 97,141 in all (47,240.44 + 49,901.10; the sum's 97,141.54 would round to
/// 97,142).
#[test]
fn cheleth_passes_through_per_event() {
    let (mut w, a) = sworn_world(&[
        (M, None, 0, 145, 0, 0),
        (V, Some(M), 50, 45, 2000 * DAY, hours(106.8)),
    ]);
    assert_eq!(
        tithe_after(&mut w, a, V, &[4_384_998, 4_631_968], false),
        97_141
    );
}

/// Divergence: V396
/// In an era whose patrons may not be of lower level, Vass (20) cannot swear to Cora (10) and is
/// told so, while swearing to Mona (50) still works; at the end of retail the lower patron is
/// ACE's success.
#[test]
fn an_era_refuses_an_oath_to_a_lower_level_patron() {
    let mut w = world();
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    let sent = capture(&mut w, |w| {
        pa::swear_allegiance(w, guid(V), C, true, false);
    });
    assert!(chats(&sent, SV)
        .iter()
        .any(|c| c.contains("You cannot swear to a lower level character.")));
    assert_eq!(errors(&sent, SV), [we(WeenieError::AllegianceIllegalLevel)]);
    assert_eq!(allegiance_of(&w, V), None);
    swear(&mut w, V, M);
    assert!(allegiance_of(&w, V).is_some(), "a higher patron is fine");

    let mut w = world();
    swear(&mut w, V, C);
    assert!(allegiance_of(&w, V).is_some(), "ACE's rule: any level");
}

/// The hierarchy record carries the tracked times; swearing starts them at zero and a new oath
/// restarts them.
#[test]
fn the_hierarchy_record_carries_the_times_sworn() {
    use empyrean_world::network::structure::allegiance_data::{allegiance_data_new, record};
    let mut w = world();
    swear(&mut w, V, M);
    let a = allegiance_of(&w, V).expect("sworn");
    let rec = |w: &mut World, g: u32| {
        record(&allegiance_data_new(
            w,
            Some(NodeRef {
                allegiance: a,
                player: guid(g),
            }),
        ))
    };
    let r = rec(&mut w, V);
    assert_eq!((r.time_online, r.allegiance_age), (0, 0), "sworn just now");

    // three real days pass, two hours of them played
    w.now.unix_time += 3.0 * f64::from(DAY);
    let age = obj(&w, V).age().unwrap_or(0);
    w.objects
        .get_mut(guid(V))
        .unwrap()
        .set_age(Some(age + 7_200));
    let r = rec(&mut w, V);
    assert_eq!((r.time_online, r.allegiance_age), (7_200, 3 * 86_400));
    assert_ne!(r.bitfield & 0x4, 0, "HasAllegianceAge");
    let m = rec(&mut w, M);
    assert_eq!(
        (m.time_online, m.allegiance_age),
        (0, 0),
        "the monarch has no patron"
    );
    // the times feed the pass-up
    assert_eq!(
        pa::i_player_sworn_time(&w, IPlayer::Online(guid(V))),
        pa::SwornTime {
            time_online: 7_200,
            allegiance_age: 3 * 86_400
        }
    );

    // breaking stops them; a new oath restarts them at zero
    act(
        &mut w,
        SV,
        &AllegianceBreakAllegiance {
            target: ObjectId(M),
        },
    );
    assert_eq!(
        pa::i_player_sworn_time(&w, IPlayer::Online(guid(V))),
        pa::SwornTime::default()
    );
    w.now.unix_time += f64::from(DAY);
    swear(&mut w, V, M);
    assert_eq!(
        pa::i_player_sworn_time(&w, IPlayer::Online(guid(V))),
        pa::SwornTime::default(),
        "restamped"
    );
    w.now.unix_time += 60.0;
    assert_eq!(
        pa::i_player_sworn_time(&w, IPlayer::Online(guid(V))).allegiance_age,
        60
    );
}

/// V285: the ruled pass-up of a `pass_xp` vector case, written from the ruling: each member's
/// (generated, cached) afterwards, every member sworn past both caps (so each E is twice the
/// Loyalty, uncapped, and each patron's effective Leadership `round(L * (1.7 + 0.3 * V))`,
/// uncapped, V285/V299), the patron's gain rounded again from the rounded tithe.
/// The tree is the world's (`Allegiance.Init`'s, checked against ACE above).
fn retail_pass_xp(
    w: &World,
    alleg: ObjectGuid,
    input: &serde_json::Value,
) -> std::collections::HashMap<u32, (u64, u64)> {
    let members = input["members"].as_array().unwrap();
    let g = |m: &serde_json::Value| u32::try_from(u64_of(&m["guid"]).unwrap()).unwrap();
    let get = |x: u32, k: &str| {
        members
            .iter()
            .find(|m| g(m) == x)
            .and_then(|m| m[k].as_i64())
    };
    let tree = allegiance::tree(w, alleg).unwrap();
    let id = |x: u32| {
        allegiance::member_node(
            w,
            NodeRef {
                allegiance: alleg,
                player: guid(x),
            },
        )
    };
    let in_tree = |x: u32| id(x).is_some();
    let patron = |x: u32| {
        id(x)
            .and_then(|n| tree.node(n).patron)
            .map(|p| tree.node(p).player_guid.full())
    };
    let mut out: std::collections::HashMap<u32, (u64, u64)> = members
        .iter()
        .map(|m| {
            (
                g(m),
                (
                    CsCast::<u64>::cs_cast(m["generated"].as_i64().unwrap_or(0)),
                    CsCast::<u64>::cs_cast(m["cached"].as_i64().unwrap_or(0)),
                ),
            )
        })
        .collect();
    let limit = input["limit"].as_bool().unwrap();
    let (mut node, mut amount, mut direct) = (
        g(&members[members
            .iter()
            .position(|m| u64_of(&m["guid"]) == u64_of(&input["from"]))
            .unwrap()]),
        u64_of(&input["amount"]).unwrap(),
        input["direct"].as_bool().unwrap(),
    );
    while in_tree(node) {
        let Some(p) = patron(node) else { break };
        if !members.iter().find(|m| g(m) == node).unwrap()["existed"]
            .as_bool()
            .unwrap_or(true)
        {
            break;
        }
        let vassals = f64::from(tree.total_vassals(id(p).unwrap()));
        let e = (get(node, "loyalty").unwrap_or(0) as f64 * 2.0).round();
        // V285/V299: the patron's E, uncapped, with its vassals' times at both caps
        let el = (get(p, "leadership").unwrap_or(0) as f64
            * (1.0 + 0.3 * (0.25 * vassals).min(1.0) + 0.7))
            .round();
        #[allow(clippy::cast_possible_truncation)]
        let rec = ((50.0 + 22.5 * el / 291.0) * 0.01) as f32;
        #[allow(clippy::cast_possible_truncation)]
        let gen = if direct {
            ((50.0 + 22.5 * e / 291.0) * 0.01) as f32
        } else {
            (5.5 * e / 291.0 * 0.01) as f32
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let round = |x: u64, f: f32| (x as f64 * f64::from(f)).round() as u64;
        let generated = round(amount, gen);
        let passup = round(generated, rec);
        if passup == 0 {
            break;
        }
        let v = out.get_mut(&node).unwrap();
        v.0 = v.0.wrapping_add(generated);
        let q = out.get_mut(&p).unwrap();
        q.1 = q.1.wrapping_add(passup);
        if limit {
            q.1 = q.1.min(u64::from(u32::MAX));
        }
        (node, amount, direct) = (p, passup, false);
    }
    out
}

#[test]
fn officer_name_order_matches_ace() {
    let file = vectors::load_named("allegiance", "name_order");
    for c in &file.cases {
        let mut names: Vec<String> = c.input["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect();
        names.sort_by(|a, b| pa::compare_culture(a, b));
        let want: Vec<String> = c
            .output
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, want);
    }
}

// ------------------------------------------------------------------ swear, tree, ranks

#[test]
fn swearing_asks_the_patron_then_builds_the_allegiance() {
    let mut w = world();

    // the first oath asks the patron; a second one while it is pending finds the patron busy
    let asked = capture(&mut w, |w| pa::swear_allegiance(w, guid(V), M, true, false));
    assert_eq!(kinds_to(&asked, SM), vec![CONFIRMATION_REQUEST]);
    let request: CharacterConfirmationRequest = decode(&to(&asked, SM)[0]);
    assert_eq!((request.context_id, request.text.as_str()), (1, "Vass"));
    assert!(to(&asked, SV).is_empty());
    let busy = capture(&mut w, |w| pa::swear_allegiance(w, guid(C), M, true, false));
    assert_eq!(chats(&busy, SC), vec!["Mona is busy."]);
    assert!(to(&busy, SM).is_empty(), "the second request is not sent");

    // a stale context id is refused and keeps the request
    assert!(!confirmation_manager::handle_response(
        &mut w,
        guid(M),
        ConfirmationType::SwearAllegiance,
        7,
        true,
        false
    ));
    let sent = capture(&mut w, |w| {
        assert!(confirmation_manager::handle_response(
            w,
            guid(M),
            ConfirmationType::SwearAllegiance,
            1,
            true,
            false
        ))
    });

    assert_eq!(chats(&sent, SM), vec!["Vass has sworn Allegiance to you."]);
    assert_eq!(
        chats(&sent, SV),
        vec!["Mona has accepted your oath of Allegiance!"]
    );
    // Rebuild's UpdateProperties: each member's rank (both are rank 1), then the panels; the
    // vassal's own refresh follows. V255/V342: no AllegianceUpdateDone after an update.
    assert_eq!(
        kinds_to(&sent, SM),
        vec![CHAT, PRIVATE_UPDATE_INT, ALLEGIANCE_UPDATE]
    );
    assert_eq!(
        kinds_to(&sent, SV),
        vec![
            CHAT,
            PRIVATE_UPDATE_INT,
            ALLEGIANCE_UPDATE,
            ALLEGIANCE_UPDATE
        ]
    );

    let a = allegiance_of(&w, V).expect("in an allegiance");
    assert_eq!(allegiance_of(&w, M), Some(a));
    assert_eq!(allegiance::monarch_player_guid(&w, a), guid(M));
    assert_eq!(allegiance::total_members(&w, a), 2);
    assert_eq!(obj(&w, V).patron_id(), Some(M));
    assert_eq!(obj(&w, V).monarch_id(), Some(M));
    assert_eq!(
        obj(&w, M).monarch_id(),
        Some(M),
        "UpdateProperties gives the monarch its own id"
    );
    assert_eq!(
        (obj(&w, M).allegiance_rank(), obj(&w, V).allegiance_rank()),
        (Some(1), Some(1))
    );
    assert!(pa::has_allegiance(&w, guid(V)) && pa::has_allegiance(&w, guid(M)));
    // patron level 50 >= vassal level 20: the vassal passes XP up
    assert_eq!(
        obj(&w, V).get_property(PropertyBool::ExistedBeforeAllegianceXpChanges),
        None
    );

    // the allegiance object is saved with its monarch
    let stored = w.shard.base_database().get_allegiance_id(M).expect("an id");
    assert_eq!(stored, a.full());
    assert_eq!(allegiance_manager::find_allegiance(&w, a.full()), Some(a));

    // the vassal's panel: rank 1, two members, the monarch then the vassal (patron = monarch)
    let update: AllegianceUpdate = decode(&to(&sent, SV)[3]);
    assert_eq!(update.rank, 1);
    assert_eq!(
        (update.profile.total_members, update.profile.total_vassals),
        (2, 0)
    );
    let h = &update.profile.hierarchy;
    assert_eq!(h.chat_room_id, Some(a.full()));
    assert_eq!(
        h.allegiance_name.as_ref().map(|(n, _)| n.as_str()),
        Some(""),
        "V281/V289: an unnamed allegiance sends an empty name, as retail did (ACE sent the monarch's)"
    );
    let members: Vec<(Option<u32>, u32, &str)> = h
        .members
        .iter()
        .map(|(p, d)| (p.map(|p| p.0), d.id.0, d.name.as_str()))
        .collect();
    assert_eq!(members, vec![(None, M, "Mona"), (Some(M), V, "Vass")]);
    assert!(h.members[1].1.may_passup_experience() && h.members[1].1.is_logged_in());
    assert!(!h.members[0].1.may_passup_experience());
}

#[test]
fn ranks_grow_with_the_tree_and_a_lower_patron_stops_pass_up() {
    let mut w = world();
    swear(&mut w, V, M);
    // Cora (10) swears to Vass (20); Dora is offline and joins by her records
    swear(&mut w, C, V);
    let a = allegiance_of(&w, M).unwrap();
    assert_eq!(allegiance::total_members(&w, a), 3);
    // a chain does not raise ranks: a patron outranks its vassals only with two of equal rank
    assert_eq!(
        (rank_of(&w, a, M), rank_of(&w, a, V), rank_of(&w, a, C)),
        (1, 1, 1)
    );
    assert_eq!(obj(&w, C).monarch_id(), Some(M));

    // a second vassal under the monarch: the two top vassals are rank 1 and 1 -> max(1 + 1, 1) = 2
    let dora = IPlayer::Offline(guid(D));
    empyrean_world::entity::i_player::set_patron_id(&mut w, dora, Some(M));
    empyrean_world::entity::i_player::set_monarch_id(&mut w, dora, Some(M));
    allegiance_manager::rebuild(&mut w, Some(a));
    let a = allegiance_of(&w, M).unwrap();
    assert_eq!(allegiance::total_members(&w, a), 4);
    assert_eq!(rank_of(&w, a, M), 2);
    assert_eq!(
        pa::i_player_allegiance(&w, dora),
        Some(a),
        "Rebuild relinks offline members"
    );

    // a vassal of a higher level than its patron does not pass XP up until the patron catches up
    let mut w = world();
    swear(&mut w, M, C); // Mona (50) to Cora (10)
    assert_eq!(
        obj(&w, M).get_property(PropertyBool::ExistedBeforeAllegianceXpChanges),
        Some(false)
    );
    let node = pa::i_player_allegiance_node(&w, IPlayer::Online(guid(C))).unwrap();
    w.objects
        .get_mut(guid(C))
        .unwrap()
        .set_property(PropertyInt::Level, 50);
    empyrean_world::entity::allegiance_node::on_level_up(&mut w, node);
    assert_eq!(
        obj(&w, M).get_property(PropertyBool::ExistedBeforeAllegianceXpChanges),
        None
    );
}

#[test]
fn a_patron_of_equal_level_takes_pass_up() {
    let mut w = world();
    w.objects
        .get_mut(guid(V))
        .unwrap()
        .set_property(PropertyInt::Level, 50);
    swear(&mut w, V, M);
    assert_eq!(
        obj(&w, V).get_property(PropertyBool::ExistedBeforeAllegianceXpChanges),
        None,
        "(patron.Level ?? 1) >= (Level ?? 1)"
    );
}

#[test]
fn pledge_refusals() {
    let mut w = world();
    let sent = capture(&mut w, |w| pa::swear_allegiance(w, guid(V), V, true, false));
    assert_eq!(
        chats(&sent, SV),
        vec!["You cannot swear allegiance to yourself."]
    );

    pa::i_player_set_allegiance_officer_rank(&mut w, IPlayer::Online(guid(V)), None);
    empyrean_world::world_objects::player_character::set_character_option(
        &mut w,
        guid(M),
        CharacterOption::IgnoreAllegianceRequests,
        true,
    );
    let sent = capture(&mut w, |w| pa::swear_allegiance(w, guid(V), M, true, false));
    assert_eq!(
        chats(&sent, SV),
        vec!["Your offer of allegiance was ignored."]
    );
    assert_eq!(
        errors(&sent, SV),
        vec![we(WeenieError::YourOfferOfAllegianceWasIgnored)]
    );
    empyrean_world::world_objects::player_character::set_character_option(
        &mut w,
        guid(M),
        CharacterOption::IgnoreAllegianceRequests,
        false,
    );

    swear(&mut w, V, M);
    let sent = capture(&mut w, |w| pa::swear_allegiance(w, guid(V), C, true, false));
    assert_eq!(chats(&sent, SV), vec!["You've already sworn allegiance."]);
    assert_eq!(
        errors(&sent, SV),
        vec![we(WeenieError::YouveAlreadySwornAllegiance)]
    );

    // the monarch cannot swear into its own allegiance
    let sent = capture(&mut w, |w| pa::swear_allegiance(w, guid(M), V, true, false));
    assert_eq!(
        chats(&sent, SM),
        vec!["You cannot swear allegiance to Vass."]
    );

    // a declined offer tells the vassal
    let asked = capture(&mut w, |w| pa::swear_allegiance(w, guid(C), M, true, false));
    let request: CharacterConfirmationRequest = decode(&to(&asked, SM)[0]);
    let sent = capture(&mut w, |w| {
        confirmation_manager::handle_response(
            w,
            guid(M),
            ConfirmationType::SwearAllegiance,
            request.context_id,
            false,
            false,
        );
    });
    assert_eq!(
        chats(&sent, SC),
        vec!["Mona has declined your offer of allegiance."]
    );
    assert_eq!(obj(&w, C).patron_id(), None);
}

#[test]
fn an_unanswered_offer_is_aborted_as_a_timeout() {
    // The 30 s timer runs on the patron's queue (the TestServer scenario waits it out); here the
    // abort it queues runs directly.
    let mut w = world();
    capture(&mut w, |w| pa::swear_allegiance(w, guid(V), M, true, false));
    let sent = capture(&mut w, |w| {
        confirmation_manager::enqueue_abort(w, guid(M), ConfirmationType::SwearAllegiance, 2)
    });
    assert!(sent.is_empty(), "a different context id aborts nothing");
    let sent = capture(&mut w, |w| {
        confirmation_manager::enqueue_abort(w, guid(M), ConfirmationType::SwearAllegiance, 1)
    });
    assert_eq!(kinds_to(&sent, SM), vec![CONFIRMATION_DONE]);
    assert_eq!(
        chats(&sent, SV),
        vec!["Mona did not respond to your offer of allegiance."]
    );
    // the answer the client then sends finds nothing pending
    assert!(!confirmation_manager::handle_response(
        &mut w,
        guid(M),
        ConfirmationType::SwearAllegiance,
        1,
        true,
        false
    ));
    assert_eq!(obj(&w, V).patron_id(), None);
}

// ------------------------------------------------------------------ XP pass-up

#[test]
fn earned_xp_passes_up_per_ace_formula() {
    let mut w = world();
    swear(&mut w, V, M);

    let loyalty =
        empyrean_world::world_objects::creature_skills::get_current_loyalty(&mut w, guid(V));
    let leadership =
        empyrean_world::world_objects::creature_skills::get_current_leadership(&mut w, guid(M));
    // V285: sworn just now, so both of the vassal's times (and its patron's average) are zero
    let zero = allegiance_manager::SwornDays::default();
    let f = allegiance_manager::passup_factors(loyalty, leadership, 1, true, zero, zero);
    let amount = 100_000u64;
    let (want_generated, want_passup) = f.amounts(amount);
    assert!(want_passup > 0);

    empyrean_world::world_objects::player_xp::grant_xp(
        &mut w,
        guid(V),
        100_000,
        XpType::Kill,
        ShareType::All,
    );
    // PassXP runs on the world queue
    assert_eq!(obj(&w, V).allegiance_xp_generated(), 0);
    run_actions(&mut w, Actor::World);

    assert_eq!(obj(&w, V).allegiance_xp_generated(), want_generated);
    // the online patron takes the XP at once: received, not cached
    assert_eq!(obj(&w, M).allegiance_xp_cached(), 0);
    assert_eq!(obj(&w, M).allegiance_xp_received(), want_passup);
}

// ------------------------------------------------------------------ break

#[test]
fn breaking_from_the_patron_dissolves_a_two_member_allegiance() {
    let mut w = world();
    swear(&mut w, V, M);
    let sent = act(
        &mut w,
        SV,
        &AllegianceBreakAllegiance {
            target: ObjectId(M),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Vass has broken their Allegiance to you!"]
    );
    assert_eq!(
        chats(&sent, SV),
        vec!["You have broken your Allegiance to Mona!"]
    );
    assert_eq!(obj(&w, V).patron_id(), None);
    assert_eq!(obj(&w, V).monarch_id(), None);
    assert_eq!(
        obj(&w, M).monarch_id(),
        None,
        "HandleNoAllegiance clears the monarch's own id"
    );
    assert_eq!(
        (obj(&w, M).allegiance_rank(), obj(&w, V).allegiance_rank()),
        (None, None)
    );
    assert_eq!((allegiance_of(&w, M), allegiance_of(&w, V)), (None, None));
    // both panels empty
    let last: AllegianceUpdate = decode(
        &to(&sent, SV)
            .into_iter()
            .rev()
            .find(|b| kind(b) == ALLEGIANCE_UPDATE)
            .unwrap(),
    );
    assert_eq!(
        (
            last.rank,
            last.profile.total_members,
            last.profile.hierarchy.members.len()
        ),
        (0, 0, 0)
    );
}

#[test]
fn a_patron_breaking_from_a_vassal_makes_the_vassal_a_monarch() {
    let mut w = world();
    swear(&mut w, V, M);
    swear(&mut w, C, V);
    let sent = act(
        &mut w,
        SM,
        &AllegianceBreakAllegiance {
            target: ObjectId(V),
        },
    );
    assert_eq!(
        chats(&sent, SV),
        vec!["Mona has broken their Allegiance to you!"]
    );
    let a = allegiance_of(&w, V).expect("Vass leads Cora");
    assert_ne!(Some(a), allegiance_of(&w, M).or(Some(ObjectGuid::new(0))));
    assert_eq!(allegiance::monarch_player_guid(&w, a), guid(V));
    assert_eq!(
        (obj(&w, V).monarch_id(), obj(&w, C).monarch_id()),
        (Some(V), Some(V))
    );
    assert_eq!(allegiance_of(&w, M), None);
}

/// V264/V265/V266/V284: a monarch speaking on the Patron or Covassals channel gets
/// its own echo and the line reaches nobody (there is no patron and no co-vassals); ACE throws on
/// the missing patron node and the line is dropped with no reply. The channels' only guard is the
/// PatronId property, so the monarch here carries a stale one. (The retail client blocks `/p` for
/// a monarch, so a retail client cannot get here.)
#[test]
fn a_monarch_on_the_patron_or_covassals_channel_gets_its_echo_and_no_one_hears_it() {
    use dereth_protocol::comms::{
        CommunicationChannelBroadcast, CommunicationChannelBroadcastRecv,
    };
    use empyrean_entity::enums::Channel;
    const CHANNEL_BROADCAST: u32 = 0x0147;
    let mut w = world();
    swear(&mut w, V, M);
    w.objects.get_mut(guid(M)).unwrap().set_patron_id(Some(D));
    for channel in [Channel::Patron, Channel::CoVassals] {
        let sent = act(
            &mut w,
            SM,
            &CommunicationChannelBroadcast {
                channel: channel.0.cast_unsigned(),
                message: "hello".to_owned(),
            },
        );
        assert_eq!(
            kinds_to(&sent, SM),
            [CHANNEL_BROADCAST],
            "{channel}: the monarch's echo"
        );
        let own: CommunicationChannelBroadcastRecv = decode(&to(&sent, SM)[0]);
        assert_eq!(
            (own.channel, own.sender_name.as_str(), own.message.as_str()),
            (channel.0.cast_unsigned(), "", "hello")
        );
        assert!(
            kinds_to(&sent, SV).is_empty(),
            "{channel}: the vassal hears nothing"
        );
        assert_eq!(sent.len(), 1, "{channel}");
    }
}

// ------------------------------------------------------------------ officers, motd, name, lock, bans

#[test]
fn motd_name_titles_and_officers() {
    let mut w = world();
    // not in an allegiance
    let sent = act(&mut w, SV, &AllegianceQueryMotd);
    assert_eq!(
        errors(&sent, SV),
        vec![we(WeenieError::YouAreNotInAllegiance)]
    );

    swear(&mut w, V, M);
    swear(&mut w, C, M);
    let sent = act(
        &mut w,
        SV,
        &AllegianceSetMotd {
            motd: "hello".into(),
        },
    );
    assert_eq!(
        errors(&sent, SV),
        vec![we(WeenieError::YouDoNotHaveAuthorityInAllegiance)]
    );

    let sent = act(&mut w, SM, &AllegianceQueryMotd);
    assert_eq!(
        chats(&sent, SM),
        vec!["Your allegiance has not set a message of the day."]
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceSetMotd {
            motd: "hello".into(),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Your message of the day has been set."]
    );
    let sent = act(&mut w, SV, &AllegianceQueryMotd);
    // Mona is rank 2 (two rank-1 vassals), Aluvian female: Baronet? no: GetTitle(Aluvian, Male=1, 2)
    let title = empyrean_world::entity::allegiance_rank::get_title(
        empyrean_entity::enums::HeritageGroup(1),
        empyrean_entity::enums::Gender(1),
        2,
    );
    assert_eq!(chats(&sent, SV), vec![format!("\"hello\" -- {title} Mona")]);

    let sent = act(
        &mut w,
        SM,
        &AllegianceSetAllegianceName {
            name: "Order".into(),
        },
    );
    assert_eq!(chats(&sent, SM), vec!["Your allegiance name has been set."]);
    let sent = act(&mut w, SV, &AllegianceQueryAllegianceName);
    assert_eq!(chats(&sent, SV), vec!["Order"]);

    // officers: Mona makes Vass a seneschal; a seneschal may only make speakers
    let sent = act(
        &mut w,
        SM,
        &AllegianceSetAllegianceOfficer {
            name: "vass".into(),
            level: 2,
        },
    );
    assert_eq!(chats(&sent, SM), vec!["Vass is now Seneschal."]);
    let sent = act(
        &mut w,
        SV,
        &AllegianceSetAllegianceOfficer {
            name: "Cora".into(),
            level: 3,
        },
    );
    assert_eq!(
        errors(&sent, SV),
        vec![we(WeenieError::YouDoNotHaveAuthorityInAllegiance)]
    );
    let sent = act(
        &mut w,
        SV,
        &AllegianceSetAllegianceOfficer {
            name: "Cora".into(),
            level: 1,
        },
    );
    assert_eq!(chats(&sent, SV), vec!["Cora is now Speaker."]);
    let sent = act(
        &mut w,
        SM,
        &AllegianceSetAllegianceOfficerTitle {
            level: 2,
            title: "Steward".into(),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Your allegiance Seneschal title has been set."]
    );
    let sent = act(&mut w, SC, &AllegianceListAllegianceOfficers);
    assert_eq!(
        chats(&sent, SC),
        vec!["Allegiance Officers:\nMona (Monarch)\nCora (Speaker)\nVass (Steward)\n"]
    );
    let sent = act(&mut w, SC, &AllegianceListAllegianceOfficerTitles);
    assert_eq!(
        chats(&sent, SC),
        vec!["Allegiance Officer Titles:\n1. Speaker\n2. Steward\n3. Castellan\n"]
    );
    let sent = act(&mut w, SM, &AllegianceClearAllegianceOfficerTitles);
    assert_eq!(
        chats(&sent, SM),
        vec!["Your allegiance officer titles have been cleared."]
    );

    // the speaker may set the motd now; the seneschal removes the speaker; the monarch clears
    let sent = act(&mut w, SC, &AllegianceClearMotd);
    assert_eq!(
        chats(&sent, SC),
        vec!["Your message of the day has been cleared."]
    );
    let sent = act(
        &mut w,
        SV,
        &AllegianceRemoveAllegianceOfficer {
            name: "Cora".into(),
        },
    );
    assert_eq!(
        chats(&sent, SV),
        vec!["Cora has been removed from allegiance officers."]
    );
    let sent = act(&mut w, SV, &AllegianceClearAllegianceOfficers);
    assert_eq!(
        errors(&sent, SV),
        vec![we(WeenieError::YouDoNotHaveAuthorityInAllegiance)]
    );
    let sent = act(&mut w, SM, &AllegianceClearAllegianceOfficers);
    assert_eq!(
        chats(&sent, SM),
        vec!["The list of officers has been cleared."]
    );
    assert_eq!(obj(&w, V).allegiance_officer_rank(), None);
    let sent = act(&mut w, SM, &AllegianceClearAllegianceName);
    assert_eq!(
        chats(&sent, SM),
        vec!["Your allegiance name has been cleared."]
    );

    // info request is a seneschal's
    let sent = act(
        &mut w,
        SM,
        &AllegianceInfoRequest {
            name: "Vass".into(),
        },
    );
    let info: AllegianceInfoResponse = decode(
        &to(&sent, SM)
            .into_iter()
            .find(|b| kind(b) == INFO_RESPONSE)
            .unwrap(),
    );
    assert_eq!(info.target, ObjectId(V));
    assert_eq!(info.profile.total_members, 3);
}

#[test]
fn locking_approving_and_banning() {
    let mut w = world();
    swear(&mut w, V, M);
    let sent = act(
        &mut w,
        SV,
        &AllegianceDoAllegianceLockAction {
            action: AllegianceLockAction::Check.0,
        },
    );
    assert_eq!(
        chats(&sent, SV),
        vec!["The allegiance is currently unlocked."]
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceDoAllegianceLockAction {
            action: AllegianceLockAction::On.0,
        },
    );
    assert_eq!(chats(&sent, SM), vec!["The allegiance is now locked."]);
    let sent = act(
        &mut w,
        SM,
        &AllegianceDoAllegianceLockAction {
            action: AllegianceLockAction::On.0,
        },
    );
    assert_eq!(chats(&sent, SM), vec!["The allegiance is already locked."]);

    // a locked allegiance refuses a stranger unless approved
    let sent = capture(&mut w, |w| pa::swear_allegiance(w, guid(C), V, true, false));
    assert_eq!(
        chats(&sent, SC),
        vec!["Vass is not accepting allegiance requests."]
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceSetAllegianceApprovedVassal {
            name: "Cora".into(),
        },
    );
    assert_eq!(chats(&sent, SM), vec!["Cora is now an approved vassal."]);
    let sent = act(
        &mut w,
        SM,
        &AllegianceDoAllegianceLockAction {
            action: AllegianceLockAction::CheckApproved.0,
        },
    );
    assert_eq!(chats(&sent, SM), vec!["Approved vassals:\nCora"]);
    let a = allegiance_of(&w, M).unwrap();

    swear(&mut w, C, V);
    let a2 = allegiance_of(&w, M).unwrap();
    assert_eq!(a2, a);
    // V262/V263 (a fix): the approval is removed from the live allegiance and from the shard
    assert!(!allegiance::has_approved_vassal(obj(&w, a.full()), C));
    let stored = w
        .shard
        .base_database()
        .get_biota(a.full(), false)
        .expect("saved");
    let stored = empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
        &stored, false,
    );
    assert!(
        !empyrean_entity::models::properties_allegiance_extensions::get_approved_vassals(
            stored.properties_allegiance.as_ref()
        )
        .contains_key(&C)
    );

    // bans: a banned member is booted
    let sent = act(
        &mut w,
        SM,
        &AllegianceAddAllegianceBan {
            name: "Cora".into(),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec![
            "Cora has been banned from the allegiance.",
            "Cora has been removed from the allegiance."
        ]
    );
    assert_eq!(
        chats(&sent, SC),
        vec!["You have been booted from the allegiance!"]
    );
    assert_eq!(obj(&w, C).patron_id(), None);
    assert_eq!(allegiance_of(&w, C), None);
    let sent = act(&mut w, SM, &AllegianceListAllegianceBans);
    assert_eq!(chats(&sent, SM), vec!["Allegiance ban list:\nCora"]);
    let sent = act(
        &mut w,
        SM,
        &AllegianceRemoveAllegianceBan {
            name: "Cora".into(),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Cora is no longer banned from the allegiance."]
    );
    // an approved vassal who is banned and unbanned stays approved
    let sent = act(
        &mut w,
        SM,
        &AllegianceSetAllegianceApprovedVassal {
            name: "Dora".into(),
        },
    );
    assert_eq!(chats(&sent, SM), vec!["Dora is now an approved vassal."]);
    act(
        &mut w,
        SM,
        &AllegianceAddAllegianceBan {
            name: "Dora".into(),
        },
    );
    act(
        &mut w,
        SM,
        &AllegianceRemoveAllegianceBan {
            name: "Dora".into(),
        },
    );
    let a = allegiance_of(&w, M).unwrap();
    assert!(allegiance::has_approved_vassal(obj(&w, a.full()), D));
    assert!(!allegiance::is_banned(obj(&w, a.full()), D));
    let sent = act(
        &mut w,
        SM,
        &AllegianceAddAllegianceBan {
            name: "Mona".into(),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Mona cannot be banned from the allegiance!"]
    );
}

#[test]
fn chat_boot_and_gag() {
    let mut w = world();
    swear(&mut w, V, M);
    swear(&mut w, C, M);
    let sent = act(
        &mut w,
        SM,
        &AllegianceChatGag {
            name: "Vass".into(),
            gagged: 1,
        },
    );
    assert_eq!(chats(&sent, SM), vec!["Vass has been gagged."]);
    assert_eq!(
        chats(&sent, SV),
        vec!["You have been gagged in allegiance chat."]
    );
    let a = allegiance_of(&w, M).unwrap();
    assert!(allegiance::is_filtered(&mut w, a, guid(V)));
    // five minutes later the gag has run out and is dropped
    w.now.utc = w.now.utc + pa::allegiance_chat_gag_time();
    assert!(!allegiance::is_filtered(&mut w, a, guid(V)));

    let sent = act(
        &mut w,
        SM,
        &AllegianceChatBoot {
            name: "Cora".into(),
            reason: "spam".into(),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Cora has been booted from allegiance chat."]
    );
    assert_eq!(
        chats(&sent, SC),
        vec!["You have been booted from Allegiance chat (spam)"]
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceChatGag {
            name: "Cora".into(),
            gagged: 0,
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Cora has been booted from allegiance chat."]
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceChatGag {
            name: "Cora".into(),
            gagged: 1,
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Cora has already been booted from allegiance chat."],
        "a gag does not shorten a boot"
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceChatBoot {
            name: "Mona".into(),
            reason: String::new(),
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["You cannot boot yourself from allegiance chat."]
    );
    let sent = act(
        &mut w,
        SV,
        &AllegianceChatBoot {
            name: "Cora".into(),
            reason: String::new(),
        },
    );
    assert_eq!(
        errors(&sent, SV),
        vec![we(WeenieError::YouDoNotHaveAuthorityInAllegiance)]
    );
}

/// V261: a swear elsewhere in the allegiance rebuilds it, and the
/// chat gags and boots carry over the rebuild with their expiry times.
#[test]
fn chat_gags_and_boots_survive_a_rebuild() {
    const E: u32 = 0x5000_0005;
    const SE: SessionId = SessionId {
        client_id: 5,
        generation: 1,
    };
    let mut w = world();
    add_online(&mut w, E, "Eska", 5, 5, SE);
    swear(&mut w, V, M);
    swear(&mut w, C, M);
    act(
        &mut w,
        SM,
        &AllegianceChatGag {
            name: "Vass".into(),
            gagged: 1,
        },
    );
    act(
        &mut w,
        SM,
        &AllegianceChatBoot {
            name: "Cora".into(),
            reason: "spam".into(),
        },
    );
    let a = allegiance_of(&w, M).unwrap();

    // someone else swears into the same allegiance: a Rebuild
    swear(&mut w, E, V);
    assert_eq!(allegiance_of(&w, M), Some(a));
    assert!(
        allegiance::is_filtered(&mut w, a, guid(V)),
        "the gag survives the rebuild"
    );
    assert!(
        allegiance::is_filtered(&mut w, a, guid(C)),
        "the boot survives the rebuild"
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceChatGag {
            name: "Cora".into(),
            gagged: 1,
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Cora has already been booted from allegiance chat."]
    );

    // and a break
    act(
        &mut w,
        SE,
        &AllegianceBreakAllegiance {
            target: ObjectId(V),
        },
    );
    assert!(allegiance::is_filtered(&mut w, a, guid(V)));

    // the gag still runs out five minutes after it was given; the boot does not
    w.now.utc = w.now.utc + pa::allegiance_chat_gag_time();
    assert!(!allegiance::is_filtered(&mut w, a, guid(V)));
    assert!(allegiance::is_filtered(&mut w, a, guid(C)));
}

/// V262/V263: a locked allegiance's approved vassal who swears is taken off
/// the live allegiance's approved list, and a later save does not write the entry back.
#[test]
fn an_approved_vassal_who_swears_leaves_the_approved_list() {
    let mut w = world();
    swear(&mut w, V, M);
    act(
        &mut w,
        SM,
        &AllegianceDoAllegianceLockAction {
            action: AllegianceLockAction::On.0,
        },
    );
    act(
        &mut w,
        SM,
        &AllegianceSetAllegianceApprovedVassal {
            name: "Cora".into(),
        },
    );
    swear(&mut w, C, V);
    let a = allegiance_of(&w, M).unwrap();
    assert!(
        !allegiance::has_approved_vassal(obj(&w, a.full()), C),
        "the live allegiance drops the approval"
    );
    let sent = act(
        &mut w,
        SM,
        &AllegianceDoAllegianceLockAction {
            action: AllegianceLockAction::CheckApproved.0,
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["The approved vassals list is currently empty."]
    );

    // a later save of the allegiance (setting the motd) leaves the shard without the entry
    act(&mut w, SM, &AllegianceSetMotd { motd: "hi".into() });
    let stored = w
        .shard
        .base_database()
        .get_biota(a.full(), false)
        .expect("saved");
    let stored = empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
        &stored, false,
    );
    assert_eq!(
        stored
            .get_property(PropertyString::AllegianceMotd)
            .as_deref(),
        Some("hi"),
        "the save happened"
    );
    assert!(
        !empyrean_entity::models::properties_allegiance_extensions::get_approved_vassals(
            stored.properties_allegiance.as_ref()
        )
        .contains_key(&C)
    );
}

/// V262/V263: booting an offline member whose allegiance has not been
/// rebuilt since start-up (so the offline player was never linked to it) completes: the tree is
/// rebuilt without the member and the booter is told.
#[test]
fn booting_an_offline_member_never_linked_since_start_up() {
    let mut w = world();
    // the saved tree: Mona <- Vass <- Dora (offline)
    for (g, patron) in [(V, M), (D, V)] {
        let p = if g == D {
            IPlayer::Offline(guid(g))
        } else {
            IPlayer::Online(guid(g))
        };
        empyrean_world::entity::i_player::set_patron_id(&mut w, p, Some(patron));
        empyrean_world::entity::i_player::set_monarch_id(&mut w, p, Some(M));
    }
    // start-up: only the online players log in
    allegiance_manager::load_player(&mut w, Some(IPlayer::Online(guid(M))));
    allegiance_manager::load_player(&mut w, Some(IPlayer::Online(guid(V))));
    let a = allegiance_of(&w, M).expect("Mona's allegiance");
    assert!(
        allegiance::members(&w, a)
            .iter()
            .any(|(g, _)| *g == guid(D)),
        "Dora is a member"
    );
    assert_eq!(
        pa::i_player_allegiance(&w, IPlayer::Offline(guid(D))),
        None,
        "but was never linked"
    );

    let sent = act(
        &mut w,
        SM,
        &dereth_protocol::social::AllegianceBreakAllegianceBoot {
            name: "Dora".into(),
            account_boot: 0,
        },
    );
    assert_eq!(
        chats(&sent, SM),
        vec!["Dora has been removed from the allegiance."]
    );
    let dora = IPlayer::Offline(guid(D));
    assert_eq!(empyrean_world::entity::i_player::patron_id(&w, dora), None);
    assert_eq!(empyrean_world::entity::i_player::monarch_id(&w, dora), None);
    let a = allegiance_of(&w, M).expect("Mona still leads Vass");
    let members: Vec<ObjectGuid> = allegiance::members(&w, a)
        .into_iter()
        .map(|(g, _)| g)
        .collect();
    assert_eq!(
        members,
        vec![guid(M), guid(V)],
        "the tree is rebuilt without Dora"
    );
    assert_eq!(pa::i_player_allegiance(&w, dora), None);
    assert!(
        allegiance_manager::get_allegiance_node(&w, dora).is_none(),
        "Dora left the lookup table"
    );
}

/// V255/V277/V280/V281: an allegiance member is sent AllegianceUpdateDone
/// right after each of its own Age updates, which come every 4 to 6 s (uniform, drawn per
/// update); a player outside any allegiance gets the Age update alone. Health regeneration:
/// the Age update is the player's heartbeat, whose next time is the one schedule; the landblock's
/// per-frame player tick sends no Age update.
#[test]
fn allegiance_update_done_rides_the_members_own_age_update() {
    use empyrean_world::world_objects::player_tick::{
        player_heartbeat, player_tick, PLAYER_TICK_INTERVAL_MAX, PLAYER_TICK_INTERVAL_MIN,
    };
    let mut w = world();
    swear(&mut w, V, M);
    let beat = |w: &mut World, g: ObjectGuid, s, t: f64| {
        let sent = capture(w, |w| player_heartbeat(w, g, t));
        kinds_to(&sent, s)
            .into_iter()
            .filter(|&k| k == PRIVATE_UPDATE_INT || k == UPDATE_DONE)
            .collect::<Vec<_>>()
    };
    let due = |w: &World, g: ObjectGuid| {
        w.objects
            .get(g)
            .unwrap()
            .wo
            .world_object_tick
            .next_heartbeat_time
    };

    let t0 = 1_000_000.0;
    assert_eq!(
        beat(&mut w, guid(V), SV, t0),
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE],
        "the Age update, then the done"
    );
    let mut t = t0;
    let mut gaps = Vec::new();
    for _ in 0..50 {
        let next = due(&w, guid(V));
        let gap = next - t;
        assert!(
            (f64::from(PLAYER_TICK_INTERVAL_MIN)..f64::from(PLAYER_TICK_INTERVAL_MAX))
                .contains(&gap),
            "gap {gap} in [4, 6)"
        );
        gaps.push(gap);
        let sent = capture(&mut w, |w| player_tick(w, guid(V), next));
        assert!(
            !kinds_to(&sent, SV).contains(&PRIVATE_UPDATE_INT),
            "the frame tick sends no Age update"
        );
        assert_eq!(
            beat(&mut w, guid(V), SV, next),
            vec![PRIVATE_UPDATE_INT, UPDATE_DONE]
        );
        t = next;
    }
    assert!(
        gaps.windows(2).any(|g| (g[0] - g[1]).abs() > 1e-6),
        "the gap is drawn afresh each time"
    );
    assert_eq!(
        beat(&mut w, guid(M), SM, t0),
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE],
        "the monarch is a member too"
    );
    assert_eq!(
        beat(&mut w, guid(C), SC, t0),
        vec![PRIVATE_UPDATE_INT],
        "outside an allegiance: the Age update alone"
    );
}

/// V281: the tick's AllegianceUpdateDone goes only to a
/// member whose allegiance-update subscription (AllegianceUpdateRequest `on_off`) is on; a player
/// starts subscribed, `on_off = 0` stops it, `on_off = 1` starts it again (with its immediate
/// answer, the update), and rank makes no difference.
#[test]
fn the_ticks_allegiance_update_done_follows_the_members_subscription() {
    use empyrean_world::world_objects::player_tick::player_heartbeat;
    let mut w = world();
    swear(&mut w, V, M);
    let beat = |w: &mut World, g: ObjectGuid, s, t: f64| {
        let sent = capture(w, |w| player_heartbeat(w, g, t));
        kinds_to(&sent, s)
            .into_iter()
            .filter(|&k| k == PRIVATE_UPDATE_INT || k == UPDATE_DONE)
            .collect::<Vec<_>>()
    };
    let t0 = 1_000_000.0;
    assert_eq!(
        beat(&mut w, guid(V), SV, t0),
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE],
        "subscribed from the start"
    );

    let sent = act(&mut w, SV, &AllegianceUpdateRequest { on_off: 0 });
    assert!(!kinds_to(&sent, SV).contains(&UPDATE_DONE));
    for k in 1..4 {
        assert_eq!(
            beat(&mut w, guid(V), SV, t0 + 5.0 * f64::from(k)),
            vec![PRIVATE_UPDATE_INT],
            "unsubscribed: the Age update alone"
        );
    }
    // the monarch (another rank, still subscribed) is unaffected
    assert_eq!(
        beat(&mut w, guid(M), SM, t0),
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE]
    );
    let sent = act(&mut w, SM, &AllegianceUpdateRequest { on_off: 0 });
    assert!(
        !kinds_to(&sent, SM).is_empty(),
        "the monarch's request is answered"
    );
    assert_eq!(
        beat(&mut w, guid(M), SM, t0 + 5.0),
        vec![PRIVATE_UPDATE_INT],
        "rank makes no difference"
    );

    let sent = act(&mut w, SV, &AllegianceUpdateRequest { on_off: 1 });
    assert_eq!(
        kinds_to(&sent, SV)[0],
        ALLEGIANCE_UPDATE,
        "on_off = 1 is answered at once"
    );
    assert_eq!(
        beat(&mut w, guid(V), SV, t0 + 20.0),
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE],
        "subscribed again"
    );
}

/// Vass (sworn to Mona) earns kill XP and passes some up; the world queue runs the pass-up.
fn vass_earns_xp(w: &mut World) {
    empyrean_world::world_objects::player_xp::grant_xp(
        w,
        guid(V),
        100_000,
        XpType::Kill,
        ShareType::All,
    );
    run_actions(w, Actor::World);
    assert!(
        obj(w, V).allegiance_xp_generated() > 0,
        "the pass-up moved Vass's tithe"
    );
}

/// The allegiance kinds (Age, update, done) `s` was sent.
fn allegiance_kinds(sent: &Sent, s: SessionId) -> Vec<u32> {
    kinds_to(sent, s)
        .into_iter()
        .filter(|&k| k == PRIVATE_UPDATE_INT || k == UPDATE_DONE || k == ALLEGIANCE_UPDATE)
        .collect()
}

/// V281/V289: retail had no periodic AllegianceUpdate. A
/// subscribed member's heartbeat carries one in place of the done when the member's view of the
/// allegiance changed since the last update it was sent (here its vassal's tithe, moved by a
/// pass-up); with no change the heartbeat carries the done, and the clock alone (the sworn times,
/// the unnamed allegiance's name time) is no change.
#[test]
fn a_subscribed_members_changed_view_replaces_the_ticks_done_with_the_update() {
    use empyrean_world::world_objects::player_tick::player_heartbeat;
    let mut w = world();
    swear(&mut w, V, M);
    let beat = |w: &mut World, g: ObjectGuid, s, t: f64| {
        let sent = capture(w, |w| player_heartbeat(w, g, t));
        (
            allegiance_kinds(&sent, s),
            to(&sent, s)
                .into_iter()
                .filter(|b| kind(b) == ALLEGIANCE_UPDATE)
                .collect::<Vec<_>>(),
        )
    };
    let t0 = 1_000_000.0;
    assert_eq!(
        beat(&mut w, guid(M), SM, t0).0,
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE],
        "unchanged: the done"
    );

    vass_earns_xp(&mut w);
    let (kinds, updates) = beat(&mut w, guid(M), SM, t0 + 5.0);
    assert_eq!(
        kinds,
        vec![PRIVATE_UPDATE_INT, ALLEGIANCE_UPDATE],
        "changed: the update, and no done"
    );
    let update: AllegianceUpdate = decode(&updates[0]);
    let vass = update
        .profile
        .hierarchy
        .members
        .iter()
        .find(|(_, d)| d.id == ObjectId(V))
        .expect("Vass's record");
    assert_eq!(
        u64::from(vass.1.cp_tithed),
        obj(&w, V).allegiance_xp_generated(),
        "it shows the new tithe"
    );
    assert_eq!(
        beat(&mut w, guid(M), SM, t0 + 10.0).0,
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE],
        "sent once: the done again"
    );
    // the vassal's own record changed too
    assert_eq!(
        beat(&mut w, guid(V), SV, t0 + 5.0).0,
        vec![PRIVATE_UPDATE_INT, ALLEGIANCE_UPDATE]
    );

    // time passing is no change
    w.now.unix_time += 3600.0;
    assert_eq!(
        beat(&mut w, guid(M), SM, t0 + 15.0).0,
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE]
    );
    assert_eq!(
        beat(&mut w, guid(V), SV, t0 + 15.0).0,
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE]
    );

    // a request's answer is the new baseline: the change it shows is not sent again
    vass_earns_xp(&mut w);
    let sent = act(&mut w, SM, &AllegianceUpdateRequest { on_off: 1 });
    assert_eq!(
        kinds_to(&sent, SM),
        vec![ALLEGIANCE_UPDATE, UPDATE_DONE],
        "the login pair is unchanged"
    );
    assert_eq!(
        beat(&mut w, guid(M), SM, t0 + 20.0).0,
        vec![PRIVATE_UPDATE_INT, UPDATE_DONE]
    );
    // outside an allegiance: the Age update alone, as before
    assert_eq!(beat(&mut w, guid(C), SC, t0).0, vec![PRIVATE_UPDATE_INT]);
}

/// V281/V289: an unsubscribed member is still pushed the changes
/// to its view, off the heartbeat (on the landblock's frame tick), never sooner than 30 s after
/// the previous AllegianceUpdate, and never with a done; its heartbeat carries neither.
#[test]
fn an_unsubscribed_member_is_pushed_changes_off_the_tick_at_most_every_30_s() {
    use empyrean_world::world_objects::player_tick::{player_heartbeat, player_tick};
    let mut w = world();
    swear(&mut w, V, M);
    let sent = act(&mut w, SM, &AllegianceUpdateRequest { on_off: 0 });
    assert_eq!(
        kinds_to(&sent, SM),
        vec![ALLEGIANCE_UPDATE],
        "the answer, with no done"
    );
    let start = w.now.unix_time;
    let frame = |w: &mut World| {
        let t = w.now.unix_time;
        allegiance_kinds(&capture(w, |w| player_tick(w, guid(M), t)), SM)
    };
    let beat = |w: &mut World, t: f64| {
        allegiance_kinds(&capture(w, |w| player_heartbeat(w, guid(M), t)), SM)
    };

    vass_earns_xp(&mut w);
    w.now.unix_time = start + 10.0;
    assert_eq!(
        frame(&mut w),
        Vec::<u32>::new(),
        "changed, but within 30 s of the answer"
    );
    assert_eq!(
        beat(&mut w, start + 10.0),
        vec![PRIVATE_UPDATE_INT],
        "the heartbeat carries neither"
    );
    w.now.unix_time = start + 29.9;
    assert_eq!(frame(&mut w), Vec::<u32>::new());
    w.now.unix_time = start + 30.0;
    assert_eq!(
        frame(&mut w),
        vec![ALLEGIANCE_UPDATE],
        "pushed once 30 s have passed, with no done"
    );
    w.now.unix_time = start + 31.0;
    assert_eq!(frame(&mut w), Vec::<u32>::new(), "sent once");

    // unchanged: nothing, however long
    w.now.unix_time = start + 100.0;
    assert_eq!(frame(&mut w), Vec::<u32>::new());
    assert_eq!(beat(&mut w, start + 100.0), vec![PRIVATE_UPDATE_INT]);

    // a change 70 s after the last push goes out on the next frame tick past the second's check
    vass_earns_xp(&mut w);
    w.now.unix_time = start + 101.5;
    assert_eq!(frame(&mut w), vec![ALLEGIANCE_UPDATE]);
    vass_earns_xp(&mut w);
    for dt in [110.0, 120.0, 131.0] {
        w.now.unix_time = start + dt;
        assert_eq!(
            frame(&mut w),
            Vec::<u32>::new(),
            "within 30 s of the last push ({dt})"
        );
        assert_eq!(beat(&mut w, start + dt), vec![PRIVATE_UPDATE_INT]);
    }
    w.now.unix_time = start + 131.5;
    assert_eq!(frame(&mut w), vec![ALLEGIANCE_UPDATE]);

    // subscribed again: the answer, then the ticks carry changes in place of the done
    let sent = act(&mut w, SM, &AllegianceUpdateRequest { on_off: 1 });
    assert_eq!(kinds_to(&sent, SM)[0], ALLEGIANCE_UPDATE);
    vass_earns_xp(&mut w);
    assert_eq!(
        frame(&mut w),
        Vec::<u32>::new(),
        "the frame tick pushes only for the unsubscribed"
    );
    assert_eq!(
        beat(&mut w, start + 140.0),
        vec![PRIVATE_UPDATE_INT, ALLEGIANCE_UPDATE]
    );
}

/// V281/V289: an allegiance with no name carries the server's
/// current time (Unix seconds) in the name's time field.
#[test]
fn an_unnamed_allegiances_name_time_is_the_current_time() {
    let mut w = world();
    swear(&mut w, V, M);
    for dt in [0.0, 1234.0] {
        w.now.unix_time += dt;
        let sent = act(&mut w, SV, &AllegianceUpdateRequest { on_off: 1 });
        let update: AllegianceUpdate = decode(&to(&sent, SV)[0]);
        let (_, time) = update.profile.hierarchy.allegiance_name.expect("the name");
        assert_eq!(f64::from(time), w.now.unix_time.floor());
    }
}

// ------------------------------------------------------------------ log in and out

#[test]
fn update_request_login_notices_and_offline_passup() {
    let mut w = world();
    swear(&mut w, V, M);
    // V255/V277/V280/V281: the update the client asks for at login is followed by one AllegianceUpdateDone;
    // later ones are not, and a player outside an allegiance gets the update alone.
    let sent = act(&mut w, SV, &AllegianceUpdateRequest { on_off: 1 });
    assert_eq!(
        kinds_to(&sent, SV),
        vec![ALLEGIANCE_UPDATE, UPDATE_DONE],
        "the login update is paired"
    );
    let sent = act(&mut w, SV, &AllegianceUpdateRequest { on_off: 1 });
    assert_eq!(
        kinds_to(&sent, SV),
        vec![ALLEGIANCE_UPDATE],
        "a later update is not"
    );
    let sent = act(&mut w, SC, &AllegianceUpdateRequest { on_off: 1 });
    assert_eq!(
        kinds_to(&sent, SC),
        vec![ALLEGIANCE_UPDATE],
        "outside an allegiance: none"
    );

    // Mona hears Vass log in and out
    empyrean_world::world_objects::player_character::set_character_option(
        &mut w,
        guid(M),
        CharacterOption::ShowAllegianceLogons,
        true,
    );
    let sent = capture(&mut w, |w| pa::handle_allegiance_on_logout(w, guid(V)));
    let notice: dereth_protocol::social::AllegianceLoginNotification = decode(&to(&sent, SM)[0]);
    assert_eq!((notice.member, notice.now_logged_in), (ObjectId(V), 0));
    assert!(to(&sent, SV).is_empty());

    // the offline copy keeps the allegiance (PlayerManager.SwitchPlayerFromOnlineToOffline)
    let dora = IPlayer::Offline(guid(D));
    empyrean_world::entity::i_player::set_patron_id(&mut w, dora, Some(V));
    empyrean_world::entity::i_player::set_monarch_id(&mut w, dora, Some(M));
    let a = allegiance_of(&w, M);
    allegiance_manager::rebuild(&mut w, a);
    assert_eq!(pa::i_player_allegiance(&w, dora), allegiance_of(&w, M));
    assert_eq!(
        pa::i_player_allegiance_node(&w, dora).map(|n| n.player),
        Some(guid(D))
    );
}

mod bindstone {
    use crate::support::player_services::*;

    /// `Bindstone.ActOnUse`: a player in no allegiance is refused with `YouAreNotInAllegiance`, before
    /// any motion.
    #[test]
    fn a_bindstone_refuses_a_player_outside_an_allegiance() {
        let mut h = H::small();
        let sa = h.player(A, "Alpha", 3);
        let stone = ObjectGuid::new(0x8000_0200);
        let mut o = empyrean_world::world_objects::world_object::WorldObject::allocate(
            empyrean_world::dispatch::Class::Bindstone,
        );
        o.guid = stone;
        h.w.objects.insert(o).unwrap();
        assert_eq!(
            empyrean_world::world_objects::player_allegiance::allegiance_permission_level(&h.w, A),
            AllegiancePermissionLevel::None
        );
        start_capture();
        empyrean_world::world_objects::bindstone::bindstone_act_on_use(&mut h.w, stone, A);
        let msgs = sent();
        assert_eq!(events_to(&msgs, sa), [0x028A]);
        let body = &msgs.iter().find(|m| m.0 == sa).unwrap().3;
        assert_eq!(
            u32::from_le_bytes(body[16..20].try_into().unwrap()),
            WeenieError::YouAreNotInAllegiance.0.cast_unsigned()
        );
    }
}
