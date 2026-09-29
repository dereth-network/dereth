//! ACE: Source/ACE.Server/WorldObjects/Player_Allegiance.cs::HandleActionSwearAllegiance
//! Swear through ConfirmationResponse, XP passes up, relog keeps it, break; an unanswered offer
//! times out.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::{
    CharacterConfirmationDone, CharacterConfirmationRequest, CharacterConfirmationResponse,
    CommunicationTextboxString,
};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::login::{
    LoginCharacterSet, LoginExecuteLogOff, LoginExecuteLogOffRequest, LoginSendEnterWorld,
    LoginSendEnterWorldRequest,
};
use dereth_protocol::social::{
    AllegianceBreakAllegiance, AllegianceSwearAllegiance, AllegianceUpdate, AllegianceUpdateRequest,
};
use dereth_protocol::Message;
use empyrean_common::dotnet::DotNetDict;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, ConfirmationType, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyInt, PropertyString, ShareType, WeenieType, XpType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_store::models::shard::Character;
use empyrean_testkit::{land, ClientId, ClientStatus, TestServer};
use empyrean_world::entity::i_player::IPlayer;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::{allegiance_manager, player_manager};
use empyrean_world::world_objects::{allegiance, player_allegiance as pa, player_location};

const HOME: u16 = 0xA9B4;
const BLOCKS: [u16; 9] = [
    0xA8B3, 0xA8B4, 0xA8B5, 0xA9B3, 0xA9B4, 0xA9B5, 0xAAB3, 0xAAB4, 0xAAB5,
];

const MONA: u32 = 0x5000_0001;
const VASS: u32 = 0x5000_0002;

fn outdoor(lb: u16, x: f32, y: f32, z: f32) -> Position {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = (x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1;
    Position::from_components(
        u32::from(lb) << 16 | cell,
        x,
        y,
        z,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

fn dats() -> Arc<empyrean_dat::DatManager> {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    let table = SecondaryAttributeTable {
        id: dereth_primitives::DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    };
    // a level curve past the players' levels (50 and 20), so their XP only grows
    let xp = empyrean_dat::file_types::XpTable {
        level_xp: (0..=60u64).map(|l| l * l * 1_000_000).collect(),
        level_credits: vec![0; 61],
        ..empyrean_dat::fake::sample::xp_table()
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .with_xp_table(xp)
        .build()
        .expect("fake dats")
}

/// An account with one character standing at `at`.
fn seed(ts: &TestServer, account: &str, guid: u32, name: &str, level: i32, at: Position) {
    let account_id = ts
        .auth()
        .create_account(
            account,
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let mut biota = empyrean_entity::Biota {
        id: guid,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, name.to_owned());
    biota.set_property(PropertyInt::Level, level);
    biota.set_property(PropertyInt::Gender, 1);
    biota.set_property(PropertyInt::HeritageGroup, 1);
    // UpdateXpAndLevel adds nothing to a player with no TotalExperience (4.9's ACE-BUG)
    biota.set_property(
        empyrean_entity::enums::PropertyInt64::TotalExperience,
        1_000,
    );
    biota.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    biota.set_property(PropertyBool::ReportCollisions, true);
    biota.set_property(PropertyBool::IgnoreCollisions, false);
    let attributes = biota
        .properties_attribute
        .get_or_insert_with(DotNetDict::new);
    for a in &PropertyAttribute::ALL[1..] {
        attributes.insert(
            *a,
            PropertiesAttribute {
                init_level: 100,
                ..PropertiesAttribute::default()
            },
        );
    }
    let vitals = biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        vitals.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                current_level: 100,
                ..PropertiesAttribute2nd::default()
            },
        );
    }
    let position = PropertiesPosition {
        obj_cell_id: at.cell(),
        position_x: at.position_x,
        position_y: at.position_y,
        position_z: at.position_z,
        rotation_w: at.rotation_w,
        rotation_x: at.rotation_x,
        rotation_y: at.rotation_y,
        rotation_z: at.rotation_z,
    };
    biota
        .properties_position
        .get_or_insert_with(DotNetDict::new)
        .insert(PositionType::Location, position);
    let character = Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
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

fn seeded() -> TestServer {
    let mut ts = TestServer::with_dats(dats());
    land::use_flat_land_with_test_setup(&mut ts.world, &BLOCKS, 10);
    ts.world.content = Arc::new(
        MemContent::new()
            .weenie(Weenie::new(1, "human", WeenieType::Creature).with_did(
                empyrean_entity::enums::PropertyDataId::CombatTable,
                0x3000_0000,
            ))
            .weenie(Weenie::new(1149, "allegiance", WeenieType::Allegiance)),
    );
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    seed(
        &ts,
        "mona",
        MONA,
        "Mona",
        50,
        outdoor(HOME, 100.0, 100.0, 20.0),
    );
    seed(
        &ts,
        "vass",
        VASS,
        "Vass",
        20,
        outdoor(HOME, 101.0, 100.0, 20.0),
    );
    player_manager::initialize(&mut ts.world);
    ts
}

fn enter(ts: &mut TestServer, account: &str, guid: u32, id: Option<ClientId>) -> ClientId {
    let id = id.unwrap_or_else(|| {
        let id = ts.connect(account, "pw");
        assert_eq!(
            ts.client(id).status(),
            ClientStatus::Connected,
            "{account} logs in"
        );
        assert!(
            ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
            "the character list arrives"
        );
        id
    });
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(guid),
            account: account.to_owned(),
        },
    );
    let g = ObjectGuid::new(guid);
    assert!(
        ts.run_until(1.0, |ts| ts
            .world
            .objects
            .get(g)
            .is_some_and(|o| o.current_landblock.is_some())),
        "{account} enters the world"
    );
    player_location::on_teleport_complete(&mut ts.world, g);
    ts.advance(0.5);
    let _ = TestServer::take_not_ported();
    id
}

/// Every game event of type `M` the client received (inside a `0xF7B0`), decoded, oldest first.
fn events<M: Message>(ts: &TestServer, id: ClientId) -> Vec<M> {
    ts.received_raw(id)
        .iter()
        .filter(|m| {
            m.opcode == 0xF7B0
                && u32::from_le_bytes(m.body[8..12].try_into().unwrap()) == M::OPCODE.0
        })
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let split = split_ui_blob(&blob).expect("a blob");
            let mut body = split.body;
            M::read(&mut body).expect("the event decodes")
        })
        .collect()
}

fn chats(ts: &TestServer, id: ClientId, from: usize) -> Vec<String> {
    ts.received::<CommunicationTextboxString>(id)
        .into_iter()
        .skip(from)
        .map(|c| c.text)
        .collect()
}

/// Vass's oath: the client's action goes through the dispatch and `HandleActionSwearAllegiance`
/// (the pledge checks pass), and the move-to chain (5.14's real MoveTo and use radius) arrives and
/// runs its success callback, `SwearAllegiance(patron, true)`.
fn oath(ts: &mut TestServer, vass: ClientId) {
    ts.send_game_action(
        vass,
        &AllegianceSwearAllegiance {
            target: ObjectId(MONA),
        },
    );
    ts.advance(0.3);
}

/// Vass swears to Mona: she is asked, and the answer comes back yes.
fn swear(ts: &mut TestServer, mona: ClientId, vass: ClientId) {
    let asked_before = events::<CharacterConfirmationRequest>(ts, mona).len();
    oath(ts, vass);
    assert!(
        ts.run_until(5.0, |ts| events::<CharacterConfirmationRequest>(ts, mona)
            .len()
            > asked_before),
        "Mona is asked"
    );
    let request = events::<CharacterConfirmationRequest>(ts, mona)
        .pop()
        .unwrap();
    assert_eq!(
        request.confirmation_type,
        ConfirmationType::SwearAllegiance.0.cast_signed()
    );
    assert_eq!(request.text, "Vass");
    ts.send_game_action(
        mona,
        &CharacterConfirmationResponse {
            confirmation_type: request.confirmation_type,
            context_id: request.context_id,
            accepted: 1,
        },
    );
    ts.advance(0.3);
    let manager = &ts
        .world
        .objects
        .get(ObjectGuid::new(MONA))
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player
        .confirmation_manager;
    assert!(
        !manager.contains(ConfirmationType::SwearAllegiance),
        "answered through the one ConfirmationManager"
    );
}

#[test]
fn swear_pass_xp_relog_and_break() {
    let mut ts = seeded();
    let mona = enter(&mut ts, "mona", MONA, None);
    let vass = enter(&mut ts, "vass", VASS, None);
    ts.advance(1.0);
    let (cm, cv) = (
        ts.received::<CommunicationTextboxString>(mona).len(),
        ts.received::<CommunicationTextboxString>(vass).len(),
    );
    let (um, uv) = (
        events::<AllegianceUpdate>(&ts, mona).len(),
        events::<AllegianceUpdate>(&ts, vass).len(),
    );

    // ---- swear: both are told, both get their allegiance panel
    swear(&mut ts, mona, vass);
    assert_eq!(chats(&ts, mona, cm), ["Vass has sworn Allegiance to you."]);
    assert_eq!(
        chats(&ts, vass, cv),
        ["Mona has accepted your oath of Allegiance!"]
    );
    let mona_panel = events::<AllegianceUpdate>(&ts, mona)
        .into_iter()
        .skip(um)
        .last()
        .expect("Mona's panel");
    let vass_panel = events::<AllegianceUpdate>(&ts, vass)
        .into_iter()
        .skip(uv)
        .last()
        .expect("Vass's panel");
    for panel in [&mona_panel, &vass_panel] {
        assert_eq!(panel.rank, 1);
        assert_eq!(panel.profile.total_members, 2);
        let members: Vec<(Option<u32>, u32)> = panel
            .profile
            .hierarchy
            .members
            .iter()
            .map(|(p, d)| (p.map(|p| p.0), d.id.0))
            .collect();
        // the monarch first; then Mona sees her vassal, Vass sees himself under his patron
        assert_eq!(members, [(None, MONA), (Some(MONA), VASS)]);
        assert!(panel
            .profile
            .hierarchy
            .members
            .iter()
            .all(|(_, d)| d.is_logged_in()));
    }
    assert_eq!(mona_panel.profile.total_vassals, 1);
    let a =
        pa::i_player_allegiance(&ts.world, IPlayer::Online(ObjectGuid::new(VASS))).expect("sworn");
    assert_eq!(
        allegiance::monarch_player_guid(&ts.world, a),
        ObjectGuid::new(MONA)
    );

    // ---- XP Vass earns passes up to Mona per ACE's formula
    let loyalty =
        pa::i_player_get_current_loyalty(&mut ts.world, IPlayer::Online(ObjectGuid::new(VASS)));
    let leadership =
        pa::i_player_get_current_leadership(&mut ts.world, IPlayer::Online(ObjectGuid::new(MONA)));
    // V285: both times sworn are those of the oath just taken (a second or so)
    let times = allegiance_manager::SwornDays::from(pa::i_player_sworn_time(
        &ts.world,
        IPlayer::Online(ObjectGuid::new(VASS)),
    ));
    let f = allegiance_manager::passup_factors(loyalty, leadership, 1, true, times, times);
    let (want_generated, want_passup) = f.amounts(50_000);
    let mona_xp = ts
        .world
        .objects
        .get(ObjectGuid::new(MONA))
        .unwrap()
        .total_experience()
        .unwrap_or(0);
    empyrean_world::world_objects::player_xp::grant_xp(
        &mut ts.world,
        ObjectGuid::new(VASS),
        50_000,
        XpType::Kill,
        ShareType::All,
    );
    ts.advance(0.5);
    let v = ts.world.objects.get(ObjectGuid::new(VASS)).unwrap();
    assert_eq!(v.allegiance_xp_generated(), want_generated);
    let m = ts.world.objects.get(ObjectGuid::new(MONA)).unwrap();
    assert_eq!(m.allegiance_xp_received(), want_passup);
    assert_eq!(
        m.allegiance_xp_cached(),
        0,
        "an online patron takes it at once"
    );
    assert_eq!(
        m.total_experience().unwrap_or(0),
        mona_xp + i64::try_from(want_passup).unwrap(),
        "and it lands in her XP"
    );

    // ---- Vass logs out and back in: the allegiance is still there (MemShard)
    ts.send_message(
        vass,
        NetQueue::Logon,
        &LoginExecuteLogOffRequest {
            character: ObjectId(VASS),
        },
    );
    assert!(
        ts.run_until(3.0, |ts| player_manager::get_online_player(&ts.world, VASS)
            .is_none()),
        "offline"
    );
    let offline_vass = IPlayer::Offline(ObjectGuid::new(VASS));
    assert_eq!(
        pa::i_player_allegiance(&ts.world, offline_vass),
        Some(a),
        "the offline player keeps its allegiance"
    );
    assert!(
        ts.run_until(10.0, |ts| !ts
            .received::<LoginExecuteLogOff>(vass)
            .is_empty()),
        "logged off"
    );
    ts.advance(0.5);
    let saved = ts.shard().get_biota(VASS, false).expect("saved");
    let saved = empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
        &saved, false,
    );
    assert_eq!(
        saved.get_property(empyrean_entity::enums::PropertyInstanceId::Patron),
        Some(MONA)
    );
    assert_eq!(
        saved.get_property(empyrean_entity::enums::PropertyInstanceId::Monarch),
        Some(MONA)
    );
    assert_eq!(saved.get_property(PropertyInt::AllegianceRank), Some(1));
    // V285: the oath's stamps (real time, and the character's played age) are saved with it
    let sworn_at = saved
        .get_property(empyrean_entity::enums::PropertyFloat::AllegianceSwearTimestamp)
        .expect("the real-time stamp is saved");
    assert!(
        saved
            .get_property(PropertyInt::AllegianceSwearTimestamp)
            .is_some(),
        "the in-game stamp is saved"
    );

    enter(&mut ts, "vass", VASS, Some(vass));
    let uv = events::<AllegianceUpdate>(&ts, vass).len();
    ts.send_game_action(vass, &AllegianceUpdateRequest { on_off: 1 });
    ts.advance(0.3);
    let panel = events::<AllegianceUpdate>(&ts, vass)
        .into_iter()
        .skip(uv)
        .last()
        .expect("the panel after the relog");
    assert_eq!((panel.rank, panel.profile.total_members), (1, 2));
    // V285: Vass's record carries the real seconds since the saved oath; the monarch's none
    let record = |g: u32| {
        panel
            .profile
            .hierarchy
            .members
            .iter()
            .find(|(_, d)| d.id.0 == g)
            .expect("a member")
            .1
            .clone()
    };
    #[allow(clippy::cast_possible_truncation)]
    let age = (ts.world.now.unix_time - sworn_at).floor() as i32;
    assert!(
        (1..=age).contains(&record(VASS).allegiance_age),
        "sworn {} s before the panel (now {age} s)",
        record(VASS).allegiance_age
    );
    assert_eq!(
        (record(MONA).time_online, record(MONA).allegiance_age),
        (0, 0)
    );
    assert_eq!(
        ts.world
            .objects
            .get(ObjectGuid::new(VASS))
            .unwrap()
            .allegiance_rank(),
        Some(1),
        "PlayerEnterWorld sets the rank from the node"
    );

    // ---- Vass breaks: both are told, the allegiance dissolves
    let (cm, cv) = (
        ts.received::<CommunicationTextboxString>(mona).len(),
        ts.received::<CommunicationTextboxString>(vass).len(),
    );
    let uv = events::<AllegianceUpdate>(&ts, vass).len();
    ts.send_game_action(
        vass,
        &AllegianceBreakAllegiance {
            target: ObjectId(MONA),
        },
    );
    ts.advance(0.3);
    assert_eq!(
        chats(&ts, mona, cm),
        ["Vass has broken their Allegiance to you!"]
    );
    assert_eq!(
        chats(&ts, vass, cv),
        ["You have broken your Allegiance to Mona!"]
    );
    let panel = events::<AllegianceUpdate>(&ts, vass)
        .into_iter()
        .skip(uv)
        .last()
        .expect("the emptied panel");
    assert_eq!((panel.rank, panel.profile.total_members), (0, 0));
    for g in [MONA, VASS] {
        let o = ts.world.objects.get(ObjectGuid::new(g)).unwrap();
        assert_eq!(
            (o.patron_id(), o.monarch_id(), o.allegiance_rank()),
            (None, None, None)
        );
        assert_eq!(
            pa::i_player_allegiance(&ts.world, IPlayer::Online(ObjectGuid::new(g))),
            None
        );
    }
}

#[test]
fn an_unanswered_offer_times_out() {
    let mut ts = seeded();
    let mona = enter(&mut ts, "mona", MONA, None);
    let vass = enter(&mut ts, "vass", VASS, None);
    ts.advance(1.0);
    let cv = ts.received::<CommunicationTextboxString>(vass).len();
    oath(&mut ts, vass);
    assert!(
        ts.run_until(1.0, |ts| !events::<CharacterConfirmationRequest>(ts, mona)
            .is_empty()),
        "Mona is asked"
    );
    ts.advance(25.0);
    assert!(
        events::<CharacterConfirmationDone>(&ts, mona).is_empty(),
        "still open"
    );
    ts.advance(6.0);
    let done = events::<CharacterConfirmationDone>(&ts, mona);
    assert_eq!(done.len(), 1, "the dialog is closed after 30 s");
    assert_eq!(
        chats(&ts, vass, cv),
        ["Mona did not respond to your offer of allegiance."]
    );
    assert_eq!(
        ts.world
            .objects
            .get(ObjectGuid::new(VASS))
            .unwrap()
            .patron_id(),
        None
    );
}
