//! ACE: Source/ACE.Server/Entity/Fellowship.cs::Fellowship
//! Three players create, recruit, share XP, quit and dismiss with ACE's fellowship messages.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::qualities::QualitiesPrivateUpdateInt64;
use dereth_protocol::social::{
    FellowshipCreate, FellowshipDismiss, FellowshipFullUpdate, FellowshipQuitNotice,
    FellowshipQuitRequest, FellowshipRecruit, FellowshipUpdateFellow, FellowshipUpdateRequest,
};
use dereth_protocol::Message;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::XpTable;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CharacterOption, PropertyAttribute, PropertyDataId, PropertyInt64, PropertyString, ShareType,
    WeenieType, XpType,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::landblock_manager;
use empyrean_world::managers::quest_manager::{self, QuestOwner};
use empyrean_world::world_objects::world_object::CtorEnv;
use empyrean_world::world_objects::{player_fellowship, player_xp};

const LB: u32 = 0xA9B4_0000;
const PLAYER_WCID: u32 = 1;

const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;
const CHARLIE: u32 = 0x5000_0003;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
const FULL_UPDATE: u32 = 0x02BE;
const UPDATE_FELLOW: u32 = 0x02C0;
const UPDATE_DONE: u32 = 0x01C9;
const FELLOW_QUIT: u32 = 0x00A3;
const FELLOW_DISMISS: u32 = 0x00A4;
const PRIVATE_INT64: u32 = 0x02CF;
const SERVER_MESSAGE: u32 = 0xF7E0;

fn content() -> MemContent {
    MemContent::new().weenie(
        Weenie::new(PLAYER_WCID, "human", WeenieType::Creature)
            .with_did(
                empyrean_entity::enums::PropertyDataId::CombatTable,
                0x3000_0000,
            )
            .with_string(PropertyString::Name, "human")
            .with_did(PropertyDataId::Setup, TEST_SETUP),
    )
}

/// Level 2 at 1,000,000 total XP, and on up to level 10: no grant here levels anyone up.
fn xp_table() -> XpTable {
    let level_xp: Vec<u64> = (0u64..=10)
        .map(|l| if l < 2 { 0 } else { l * 1_000_000 })
        .collect();
    XpTable {
        id: DataId(file_id::XP_TABLE),
        level_credits: vec![0; level_xp.len()],
        level_xp,
        ..empyrean_dat::fake::sample::xp_table()
    }
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

fn server() -> TestServer {
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new().with_xp_table(xp_table()))
        .build()
        .expect("fake dats");
    let mut ts = TestServer::with_setup(dats, |w| {
        w.content = Arc::new(content());
    });
    land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
    ts
}

/// A client logged in as `account` whose session plays `guid` at `pos` (as `vendors.rs` joins),
/// accepting fellowship invitations automatically.
fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
    level: i32,
    pos: Position,
) -> (ClientId, SessionId) {
    let before: Vec<SessionId> = ts.world.sessions.iter().map(|(id, _)| id).collect();
    let id = ts.connect(account, "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .find(|s| !before.contains(s))
        .expect("the new session");

    let w = &mut ts.world;
    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            empyrean_world::dispatch::Class::Player,
            weenie,
            ObjectGuid::new(guid),
            1,
        )
    });
    o.set_property(PropertyString::Name, name.to_owned());
    let rec = o
        .biota
        .properties_attribute
        .get_or_insert_with(Default::default)
        .get_or_insert_with(PropertyAttribute::Strength, Default::default);
    rec.init_level = 100;
    o.set_level(Some(level));
    o.set_property(PropertyInt64::TotalExperience, 0);
    o.set_property(PropertyInt64::AvailableExperience, 0);
    let option = CharacterOption::AutomaticallyAcceptFellowshipRequests;
    let mut character = empyrean_store::models::shard::Character::default();
    if let Some(bit) = option.character_options1() {
        character.character_options_1 = bit.0.cast_signed();
    } else {
        character.character_options_2 = option
            .character_options2()
            .expect("an options flag")
            .0
            .cast_signed();
    }
    o.player.as_mut().expect("a player").player.character = Some(character);
    o.set_location(Some(pos));
    w.objects.insert(o).expect("fresh");
    w.player_manager.online_players.insert(
        guid,
        empyrean_world::managers::player_manager::OnlinePlayer {
            guid: ObjectGuid::new(guid),
            account: None,
        },
    );

    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(ObjectGuid::new(guid)));
    assert!(
        landblock_manager::add_object(w, ObjectGuid::new(guid), false),
        "the player joins its landblock"
    );
    ts.advance(0.1);
    (id, session)
}

/// A received message: its kind and the whole blob.
#[derive(Debug, Clone)]
struct Got {
    kind: u32,
    blob: Vec<u8>,
}

impl Got {
    fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }

    /// A system chat's text.
    fn text(&self) -> String {
        let len = usize::from(u16::from_le_bytes([self.blob[4], self.blob[5]]));
        String::from_utf8(self.blob[6..6 + len].to_vec()).unwrap()
    }
}

/// What the client received since message `from`, without the Age heartbeat.
fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let kind = if m.opcode == 0xF7B0 {
                u32::from_le_bytes(m.body[8..12].try_into().unwrap())
            } else {
                m.opcode
            };
            Got { kind, blob }
        })
        .filter(|g| g.kind != 0x02CD)
        .collect()
}

fn kinds(g: &[Got]) -> Vec<u32> {
    g.iter().map(|g| g.kind).collect()
}

fn last(g: &[Got], kind: u32) -> &Got {
    g.iter()
        .rev()
        .find(|m| m.kind == kind)
        .unwrap_or_else(|| panic!("no 0x{kind:04X} in {:04X?}", kinds(g)))
}

fn chats(g: &[Got]) -> Vec<String> {
    g.iter()
        .filter(|m| m.kind == SERVER_MESSAGE)
        .map(Got::text)
        .collect()
}

/// The `TotalExperience` updates a client got.
fn total_xp_updates(g: &[Got]) -> Vec<i64> {
    g.iter()
        .filter(|m| m.kind == PRIVATE_INT64)
        .map(|m| m.decode::<QualitiesPrivateUpdateInt64>().0)
        .filter(|u| u.property_id == u32::from(PropertyInt64::TotalExperience.0))
        .map(|u| u.value)
        .collect()
}

/// Three players: Alpha creates a fellowship and recruits Bravo and Charlie (both auto-accept);
/// each sees the full roster; a kill's 1,000 XP earned by Alpha is shared evenly (all within 5
/// levels: 60% of it for three, 600 each, all inside 600 units); a fellowship quest stamp is seen
/// by every member; Charlie quits; Alpha dismisses Bravo.
#[test]
fn create_recruit_share_quit_and_dismiss() {
    let mut ts = server();
    let (a, _) = join(&mut ts, "alpha", ALPHA, "Alpha", 3, at(20.0, 20.0));
    let (b, _) = join(&mut ts, "bravo", BRAVO, "Bravo", 4, at(30.0, 20.0));
    let (c, _) = join(&mut ts, "charlie", CHARLIE, "Charlie", 2, at(40.0, 20.0));
    let (alpha, bravo, charlie) = (
        ObjectGuid::new(ALPHA),
        ObjectGuid::new(BRAVO),
        ObjectGuid::new(CHARLIE),
    );

    // ---- create: the full update (Alpha alone) and the update-done bracket
    let n = ts.received_raw(a).len();
    ts.send_game_action(
        a,
        &FellowshipCreate {
            name: "Company".to_owned(),
            share_xp: 1,
        },
    );
    ts.advance(0.3);
    let g = got(&ts, a, n);
    assert_eq!(kinds(&g), [FULL_UPDATE, UPDATE_DONE]);
    let full: FellowshipFullUpdate = last(&g, FULL_UPDATE).decode();
    assert_eq!(
        (
            full.0.name.as_str(),
            full.0.leader.0,
            full.0.share_xp,
            full.0.even_xp_split
        ),
        ("Company", ALPHA, 1, 0)
    );
    assert_eq!(full.0.members.entries.len(), 1);

    // ---- recruit Bravo, then Charlie
    let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
    ts.send_game_action(
        a,
        &FellowshipRecruit {
            target: ObjectId(BRAVO),
        },
    );
    ts.advance(0.3);
    let (ga, gb) = (got(&ts, a, na), got(&ts, b, nb));
    assert_eq!(
        kinds(&ga)
            .iter()
            .filter(|k| [UPDATE_FELLOW, FULL_UPDATE].contains(k))
            .copied()
            .collect::<Vec<_>>(),
        [UPDATE_FELLOW, FULL_UPDATE]
    );
    let fellow: FellowshipUpdateFellow = last(&ga, UPDATE_FELLOW).decode();
    assert_eq!(
        (
            fellow.fellow_id.0,
            fellow.fellow.name.as_str(),
            fellow.fellow.level,
            fellow.update_type
        ),
        (BRAVO, "Bravo", 4, 1)
    );
    // V258 (a fix): the share-loot bit carries ShareLoot (off here); ACE sent ShareXP (on)
    assert_eq!(fellow.fellow.share_loot, 0);
    let full: FellowshipFullUpdate = last(&gb, FULL_UPDATE).decode();
    assert_eq!(
        full.0
            .members
            .entries
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        [ALPHA, BRAVO]
    );
    assert_eq!(full.0.even_xp_split, 1, "levels 3 and 4: even share");

    let nc = ts.received_raw(c).len();
    ts.send_game_action(
        a,
        &FellowshipRecruit {
            target: ObjectId(CHARLIE),
        },
    );
    ts.advance(0.3);
    let full: FellowshipFullUpdate = last(&got(&ts, c, nc), FULL_UPDATE).decode();
    // HashComparer(16): the three guids fall in buckets 1, 2, 3
    assert_eq!(
        full.0
            .members
            .entries
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        [ALPHA, BRAVO, CHARLIE]
    );
    let names: Vec<&str> = full
        .0
        .members
        .entries
        .iter()
        .map(|(_, f)| f.name.as_str())
        .collect();
    assert_eq!(names, ["Alpha", "Bravo", "Charlie"]);
    let f = player_fellowship::fellowship(&ts.world, alpha).expect("a fellowship");
    assert_eq!(player_fellowship::fellowship(&ts.world, charlie), Some(f));

    // ---- a kill's XP, earned by Alpha: GetMemberSharePercent(3) = 0.6 of 1,000, each fellow at
    // distance <= 600 gets all of it
    let (na, nb, nc) = (
        ts.received_raw(a).len(),
        ts.received_raw(b).len(),
        ts.received_raw(c).len(),
    );
    player_xp::earn_xp(&mut ts.world, alpha, 1_000, XpType::Kill, ShareType::All);
    ts.advance(0.3);
    for (id, from, g) in [(a, na, alpha), (b, nb, bravo), (c, nc, charlie)] {
        assert_eq!(total_xp_updates(&got(&ts, id, from)), [600], "{g:?}");
        assert_eq!(
            ts.world.objects.get(g).unwrap().total_experience(),
            Some(600)
        );
    }

    // ---- a fellowship quest stamp (the fellowship's own QuestManager) reaches every member
    f.with_quest_manager(&mut ts.world, |w, qm| {
        quest_manager::stamp(w, &mut QuestOwner::Fellowship(qm), "SharedTask")
    });
    for g in [alpha, bravo, charlie] {
        let mine = player_fellowship::fellowship(&ts.world, g).expect("still a member");
        assert!(
            mine.read_quest_manager(&ts.world, |qm| quest_manager::has_quest(
                &ts.world,
                &QuestOwner::Fellowship(qm),
                "SharedTask"
            ))
        );
    }

    // ---- Charlie quits: FellowshipQuit(Charlie) to Charlie and to the others
    let (na, nc) = (ts.received_raw(a).len(), ts.received_raw(c).len());
    ts.send_game_action(c, &FellowshipQuitRequest { disband: 0 });
    ts.advance(0.3);
    let quit: FellowshipQuitNotice = last(&got(&ts, c, nc), FELLOW_QUIT).decode();
    assert_eq!(quit.member.0, CHARLIE);
    let quit: FellowshipQuitNotice = last(&got(&ts, a, na), FELLOW_QUIT).decode();
    assert_eq!(quit.member.0, CHARLIE);
    assert!(player_fellowship::fellowship(&ts.world, charlie).is_none());

    // ---- Bravo opens the panel (a full update), then Alpha dismisses Bravo
    let nb = ts.received_raw(b).len();
    ts.send_game_action(b, &FellowshipUpdateRequest { on: 1 });
    ts.advance(0.3);
    let full: FellowshipFullUpdate = last(&got(&ts, b, nb), FULL_UPDATE).decode();
    assert_eq!(
        full.0
            .members
            .entries
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        [ALPHA, BRAVO]
    );

    let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
    ts.send_game_action(
        a,
        &FellowshipDismiss {
            target: ObjectId(BRAVO),
        },
    );
    ts.advance(0.3);
    let (ga, gb) = (got(&ts, a, na), got(&ts, b, nb));
    for g in [&ga, &gb] {
        let dismissed: FellowshipDismiss = last(g, FELLOW_DISMISS).decode();
        assert_eq!(dismissed.target.0, BRAVO);
        assert!(chats(g).contains(&"Bravo dismissed from fellowship".to_owned()));
    }
    let full: FellowshipFullUpdate = last(&ga, FULL_UPDATE).decode();
    assert_eq!(
        full.0
            .members
            .entries
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        [ALPHA]
    );
    assert!(player_fellowship::fellowship(&ts.world, bravo).is_none());
    assert_eq!(player_fellowship::fellowship(&ts.world, alpha), Some(f));
}

mod confirmation_response {
    //! ACE: Source/ACE.Server/WorldObjects/Player_Fellowship.cs::FellowshipRecruit
    use crate::inventory::trade::*;

    /// A fellowship invitation answered over the wire: Bravo (no auto-accept) gets the popup, answers
    /// yes through ConfirmationResponse, and joins.
    #[test]
    fn a_fellowship_invitation_is_answered_through_confirmation_response() {
        let mut ts = server();
        let (a, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
        let (b, _) = join(&mut ts, "bravo", BRAVO, "Bravo", at(20.5, 20.0), 100);

        ts.send_game_action(
            a,
            &FellowshipCreate {
                name: "Company".to_owned(),
                share_xp: 1,
            },
        );
        ts.advance(0.2);

        let nb = ts.received_raw(b).len();
        ts.send_game_action(
            a,
            &FellowshipRecruit {
                target: ObjectId(BRAVO),
            },
        );
        ts.advance(0.2);
        let gb = trade_events(&ts, b, nb);
        assert_eq!(kinds(&gb), [CONFIRMATION_REQUEST]);
        let request: CharacterConfirmationRequest = gb[0].decode();
        assert_eq!(
            (
                request.confirmation_type,
                request.context_id,
                request.text.as_str()
            ),
            (4, 1, "Alpha")
        );
        assert!(player_fellowship::fellowship(&ts.world, ObjectGuid::new(BRAVO)).is_none());

        let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
        ts.send_game_action(
            b,
            &CharacterConfirmationResponse {
                confirmation_type: request.confirmation_type,
                context_id: request.context_id,
                accepted: 1,
            },
        );
        ts.advance(0.2);

        let fa = player_fellowship::fellowship(&ts.world, ObjectGuid::new(ALPHA))
            .expect("Alpha's fellowship");
        let fb =
            player_fellowship::fellowship(&ts.world, ObjectGuid::new(BRAVO)).expect("Bravo joined");
        assert!(fa == fb, "the same fellowship");
        for (id, n) in [(a, na), (b, nb)] {
            let full = got(&ts, id, n)
                .into_iter()
                .rev()
                .find(|g| g.kind == FULL_UPDATE)
                .expect("a full update");
            let full: FellowshipFullUpdate = full.decode();
            assert_eq!(full.0.members.entries.len(), 2);
        }
        let manager = &ts
            .world
            .objects
            .get(ObjectGuid::new(BRAVO))
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player
            .confirmation_manager;
        assert!(!manager.contains(ConfirmationType::Fellowship), "answered");

        // the 30 s timeout finds nothing left to abort: no ConfirmationDone, no message
        let nb = ts.received_raw(b).len();
        ts.advance(31.0);
        assert!(
            got(&ts, b, nb).iter().all(|g| g.kind != 0x0276),
            "no ConfirmationDone after an answer"
        );
    }
}
