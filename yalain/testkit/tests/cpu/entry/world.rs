//! Entering the world: what the world connection sends, the scene, moving, the objects around,
//! options kept, leaving and entering again.

use auberean_primitives::Vec3;
use auberean_protocol::login::{
    CharacterCreated, CharacterRef, CreateCharacter, DataHeld, Empty, PlayCharacter,
};
use auberean_protocol::physics::ClientPosition;
use auberean_protocol::script::{Arg, FunctionId, ScriptArgs, ServerCall};
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::text::{NarrowString, WideString};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject};
use yalain_content::{ContentObject, ContentPack, Description, Kind};
use yalain_store::MemoryStore;
use yalain_testkit::{Harness, Received};
use yalain_world::graph::{hash, Builder};
use yalain_world::options::Options;
use yalain_world::player::{PLAYER_ENTITY, PLAYER_PACKAGE, PLAYER_QUALITIES};
use yalain_world::session::{function, request};
use yalain_world::world::{rotation_of_heading, ARRIVAL_FX, CONTENT_ID_BASE, PORTAL_FX};
use yalain_world::ServerConfig;

const STEPS: usize = 400;

fn at_character_list(h: &mut Harness, account: &str) -> usize {
    let b = h.add_bot();
    let now = h.now;
    h.bots[b].log_in(account, b"ticket", now);
    assert!(h.run_until(STEPS, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::DataQuery(_)))));
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files: Vec::new(),
    });
    h.bots[b].send(false, queue::DATABASE, &held);
    assert!(h.run_until(STEPS, |h| h.bots[b]
        .messages(false)
        .iter()
        .any(|m| matches!(m, ServerMessage::CharacterList(_)))));
    b
}

fn create(h: &mut Harness, b: usize, name: &str) -> u64 {
    let before = h.bots[b].inbox.len();
    let m = ClientMessage::CreateCharacter(CreateCharacter {
        account: NarrowString::from("tester"),
        name: WideString::from(name),
        entity_did: 0x4700_110F,
        start_area: 0x4000_0001,
        race: 1,
        sex: 0x1000,
        class: 2,
        implement: 1,
        physique: Table::from_inserted(SMALL, [(1, 1.0), (2, 1.0)]),
    });
    h.bots[b].send(false, queue::LOGON, &m);
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() > before));
    match h.bots[b].inbox[before].message() {
        Some(ServerMessage::CharacterCreated(CharacterCreated { identity, .. })) => identity.id,
        other => panic!("not a creation answer: {other:?}"),
    }
}

/// Is a received message the player's object being made?
fn is_player_object(r: &Received, id: u64) -> bool {
    matches!(r.message(), Some(ServerMessage::CreateObject(o)) if o.id == id)
}

/// Enter the world with a character; returns once the welcome has arrived. `world` says which
/// connection carries the world.
fn enter(h: &mut Harness, b: usize, id: u64, world: bool) {
    let before = h.bots[b].inbox.len();
    let m = ClientMessage::PlayCharacter(PlayCharacter {
        character: id,
        account: NarrowString::from("tester"),
    });
    h.bots[b].send(false, queue::LOGON, &m);
    let welcome = move |r: &Received| {
        r.world == world
            && matches!(r.message(), Some(ServerMessage::PlayerCall(c)) if c.function.0 == function::SHOW_TEXT)
    };
    assert!(
        h.run_until(STEPS, |h| h.bots[b].inbox[before..].iter().any(welcome)),
        "the entry arrives"
    );
    h.run(10);
}

/// The non-ephemeral messages of one connection since `from`, in the order the server sent them.
fn sent_order(h: &Harness, b: usize, from: usize, world: bool) -> Vec<Received> {
    let mut v: Vec<Received> = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.world == world && r.blob_id >> 63 == 0)
        .cloned()
        .collect();
    v.sort_by_key(|r| r.blob_id & 0xFFFF_FFFF);
    v
}

fn ephemeral(h: &Harness, b: usize, from: usize) -> Vec<ServerMessage> {
    h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.blob_id >> 63 == 1)
        .filter_map(Received::message)
        .collect()
}

fn private_stamps(v: &[Received]) -> Vec<u16> {
    #[allow(clippy::cast_possible_truncation)]
    v.iter()
        .filter(|r| r.blob_id >> 56 == 0x01)
        .map(|r| (r.blob_id >> 32) as u16)
        .collect()
}

fn kind_of(m: &ServerMessage) -> String {
    match m {
        ServerMessage::PlayerCall(c) => format!("call {:08X}", c.function.0),
        other => format!("{:#x}", other.message_type()),
    }
}

/// As a character enters, its world connection carries, in this order: the player's object, the
/// message that makes it the client's player, its description, a new character's conflict
/// status, the arrival effect, a new character's movement ethereality, a freeze of its movement,
/// the end of the portal effect, the session start and the welcome, a new character's craft
/// skills and the craft registry, then (after the objects around) its allegiance's profile and
/// its chat channels twice; the private ones
/// numbered from 1, the object messages unordered, the effects ordered by the object's visual
/// stamps after the one it was made with.
/// Behaviour: server.world.entry-in-the-recorded-order
#[test]
fn entering_sends_the_player_its_object_description_and_session_start_in_the_recorded_order() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Elara");
    let from = h.bots[b].inbox.len();
    enter(&mut h, b, id, true);
    let v = sent_order(&h, b, from, true);
    let kinds: Vec<String> = v
        .iter()
        .map(|r| kind_of(&r.message().expect("reads")))
        .collect();
    assert_eq!(
        kinds,
        [
            "0x10058".to_string(),
            "0x10060".into(),
            "0x10068".into(),
            format!("call {:08X}", yalain_world::kingdoms::STATUS),
            "0x100a0".into(),
            "0x1006a".into(),
            format!("call {:08X}", function::MOVEMENT_FROZEN),
            "0x100a1".into(),
            format!("call {:08X}", function::SESSION_START),
            format!("call {:08X}", function::SHOW_TEXT),
            format!("call {:08X}", yalain_world::craft::function::SKILLS),
            format!("call {:08X}", yalain_world::craft::function::REFRESH),
            "call 8203002E".into(),
            "call 8205001E".into(),
            "call 8205001E".into(),
        ]
    );
    let tops: Vec<u64> = v.iter().map(|r| r.blob_id >> 56).collect();
    assert_eq!(
        tops,
        [
            0x03, 0x03, 0x01, 0x01, 0x02, 0x02, 0x01, 0x02, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
            0x01
        ]
    );
    assert_eq!(private_stamps(&v), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    let Some(ServerMessage::CreateObject(o)) = v[0].message() else {
        panic!()
    };
    assert_eq!(
        (o.id, o.weenie.entity_did, o.weenie.package_id),
        (id, PLAYER_ENTITY, PLAYER_PACKAGE)
    );
    assert_eq!(
        o.weenie.name.literal.map(|w| w.text()),
        Some("Elara [Mv]".to_string())
    );
    assert_eq!(
        o.physics.position.cell, 0x0299_0109,
        "at the start area's first place"
    );
    assert_eq!(
        (o.physics.instance_stamp, o.physics.visual_ordering_stamp),
        (1, 8)
    );
    let Some(ServerMessage::PlayerObject(p)) = v[1].message() else {
        panic!()
    };
    assert_eq!((p.id, p.region), (id, 1));
    let Some(ServerMessage::PlayerQualities(q)) = v[2].message() else {
        panic!()
    };
    assert_eq!(q.0.did, PLAYER_QUALITIES);
    assert_eq!(
        (q.0.ints.get(290), q.0.ints.get(291), q.0.ints.get(292)),
        (Some(&1), Some(&0x1000), Some(&2))
    );
    let Some(ServerMessage::DoFx(fx)) = v[4].message() else {
        panic!()
    };
    assert_eq!((fx.action.fx, fx.sender.other_stamp), (ARRIVAL_FX, 9));
    let Some(ServerMessage::StopFx(stop)) = v[7].message() else {
        panic!()
    };
    assert_eq!((stop.action.0, stop.sender.other_stamp), (PORTAL_FX, 11));
    let Some(ServerMessage::PlayerCall(frozen)) = v[6].message() else {
        panic!()
    };
    assert_eq!(
        frozen.args,
        Some(ScriptArgs::from_values(&[Arg::Int(1)]).expect("packs"))
    );
    let skills = vec![(835, 0), (96, 0), (176, 0), (207, 0), (1, 1), (7, 1)];
    let options = Options::new_character(&skills).to_packed();
    assert!(
        v[8].bytes.windows(options.len()).any(|w| w == options),
        "the session start holds a new character's options"
    );
}

/// When the client says it has drawn the world, the server frees the player's movement and ends
/// the arrival effect.
/// Behaviour: server.world.scene-drawn-frees-movement
#[test]
fn the_drawn_scene_frees_the_player_and_ends_the_arrival_effect() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare");
    enter(&mut h, b, id, true);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(Empty),
    );
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() >= from + 2));
    let v = sent_order(&h, b, from, true);
    let Some(ServerMessage::PlayerCall(free)) = v[0].message() else {
        panic!("{:?}", v[0].message())
    };
    assert_eq!(free.function, FunctionId(function::MOVEMENT_FROZEN));
    assert_eq!(
        free.args,
        Some(ScriptArgs::from_values(&[Arg::Int(0)]).expect("packs"))
    );
    let Some(ServerMessage::StopFx(stop)) = v[1].message() else {
        panic!()
    };
    assert_eq!((stop.action.0, stop.sender.other_stamp), (ARRIVAL_FX, 12));
    assert_eq!(private_stamps(&v), [11]);
}

fn position(cell: u32, origin: Vec3, heading: u8) -> ClientMessage {
    ClientMessage::Position(ClientPosition {
        time: 1.0,
        cell,
        origin,
        heading_motion: u32::from(heading) << 24,
        flags: 0,
        instance_stamp: 1,
        teleport_stamp: 0,
        force_position_stamp: 0,
        move_to_stamp: 0,
        jump_velocity: Vec3::ZERO,
    })
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

/// The place the client reports for its player is sent back to it as the player's position (a
/// cell position when it is in a new cell), on the player's own position stream, each newer than
/// the last; when the character leaves and enters again it stands where it was, facing as it
/// faced.
/// Behaviour: server.world.position-sent-back-and-kept
#[test]
fn a_reported_place_is_sent_back_and_the_character_enters_there_next_time() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare");
    enter(&mut h, b, id, true);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &position(0x0299_0109, Vec3::new(61.0, 238.5, -44.6), 0x40),
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &position(0x0299_010A, Vec3::new(64.0, 236.0, -44.6), 0x80),
    );
    assert!(h.run_until(STEPS, |h| ephemeral(h, b, from).len() >= 2));
    let echoes: Vec<(u32, u16)> = ephemeral(&h, b, from)
        .into_iter()
        .filter_map(|m| {
            if let ServerMessage::Position(p) | ServerMessage::PositionCell(p) = m {
                Some((p.cell, p.position_stamp))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(echoes, [(0x0299_0109, 2), (0x0299_010A, 3)]);
    log_off(&mut h, b, id);
    let from = h.bots[b].inbox.len();
    enter(&mut h, b, id, true);
    let r = h.bots[b].inbox[from..]
        .iter()
        .find(|r| is_player_object(r, id))
        .expect("the player's object");
    let Some(ServerMessage::CreateObject(o)) = r.message() else {
        panic!()
    };
    let p = o.physics.position;
    assert_eq!(
        (p.cell, p.frame.origin),
        (0x0299_010A, Vec3::new(64.0, 236.0, -44.6))
    );
    let want = rotation_of_heading(0x80);
    assert!(
        (p.frame.rotation.w - want[0]).abs() < 1e-6 && (p.frame.rotation.z - want[3]).abs() < 1e-6
    );
    assert_eq!(
        o.physics.instance_stamp, 2,
        "a new instance of the player's object"
    );
}

fn object(template: u32, object: Option<u64>, cell: u32) -> ContentObject {
    ContentObject {
        object,
        template,
        description: Description {
            name_text: None,
            package: 0x1169,
            look: 0x1F00_0ED5,
            enter_fx: 0x4100_05B4,
            scale: Some(1.0),
            name: None,
            numbers: [0x40_0020, 0, 0x4220_0010, 0, 0x42A0_0090, 0],
            source: "authored for a test".into(),
        },
        cell,
        origin: [80.0, 80.0, 90.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Creature,
        label: "Crawlspace Vermin".into(),
        placed: "authored for a test".into(),
        destination: None,
        facts: yalain_content::Facts::default(),
        walks: None,
        gate: None,
    }
}

fn created(h: &Harness, b: usize, from: usize) -> Vec<u64> {
    h.bots[b].inbox[from..]
        .iter()
        .filter_map(|r| {
            if let Some(ServerMessage::CreateObject(o)) = r.message() {
                Some(o.id)
            } else {
                None
            }
        })
        .collect()
}

/// The objects of the world in view are made for the player as it enters (a static object under
/// its own id, the others under the server's): indoors, those of its own landblock; outdoors,
/// those within three land cells; those coming into view as it walks are made as they do; each
/// is described as the pack says.
/// Behaviour: server.world.objects-around-the-player
#[test]
fn objects_in_range_are_made_as_the_player_enters_and_as_it_walks_into_their_landblock() {
    let mut h = Harness::new();
    let pack = ContentPack {
        objects: vec![
            object(0x4700_2D31, None, 0x0299_0010),
            object(0x4700_114D, Some(0x4002_9900_0000_0012), 0x0299_0011),
            object(0x4700_2D31, None, 0x029A_0012),
            object(0x4700_2225, None, 0x029B_0013),
        ],
        ..ContentPack::default()
    };
    h.server.content = pack;
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare");
    let from = h.bots[b].inbox.len();
    enter(&mut h, b, id, true);
    let ids = created(&h, b, from);
    assert_eq!(
        ids,
        [id, CONTENT_ID_BASE | 1, 0x4002_9900_0000_0012],
        "indoors: its own landblock only"
    );
    let r = h.bots[b].inbox[from..]
        .iter()
        .find(|r| is_player_object(r, CONTENT_ID_BASE | 1))
        .expect("made");
    let Some(ServerMessage::CreateObject(o)) = r.message() else {
        panic!()
    };
    assert_eq!(
        (o.visual.parent, o.weenie.package_id, o.weenie.entity_did),
        (0x1F00_0ED5, 0x1169, 0x4700_2D31)
    );
    assert!(
        o.physics.fx.is_empty(),
        "already in the world: no entry effect"
    );
    assert_eq!(
        o.weenie.numbers[5..11],
        [0x40_0020, 0, 0x4220_0010, 0, 0x42A0_0090, 0]
    );
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &position(0x029A_0012, Vec3::new(75.0, 75.0, 90.0), 0),
    );
    assert!(h.run_until(STEPS, |h| !created(h, b, from).is_empty()));
    assert_eq!(
        created(&h, b, from),
        [CONTENT_ID_BASE | 3],
        "outdoors: what is within three land cells"
    );
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &position(0x029B_0012, Vec3::new(75.0, 20.0, 90.0), 0),
    );
    assert!(h.run_until(STEPS, |h| !created(h, b, from).is_empty()));
    assert_eq!(
        created(&h, b, from),
        [CONTENT_ID_BASE | 4],
        "the next landblock's as it comes within three cells"
    );
}

fn call(f: u32, args: &[Arg]) -> ClientMessage {
    ClientMessage::ServerCall(ServerCall {
        function: FunctionId(f),
        args: ScriptArgs::from_values(args).expect("packs"),
    })
}

fn native(ty: u16, n: NativeObject) -> Arg {
    let mut g = Builder::new();
    g.native(ty, n);
    Arg::Object(g.graph())
}

/// The options the client sends (its game options, window places and opacities, a shortcut)
/// come back in the session start of the character's next entry, with every option it did not
/// change as before.
/// Behaviour: server.world.options-kept
#[test]
fn options_the_client_sends_come_back_in_the_next_session_start() {
    let mut h = Harness::new();
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare");
    enter(&mut h, b, id, true);
    let range = 0.5f32;
    h.bots[b].send(
        true,
        queue::WEENIE,
        &call(
            request::GAME_OPTIONS,
            &[
                Arg::Int(0x3F1),
                Arg::Int(0x7F),
                Arg::Int(2),
                Arg::Float(range.to_bits()),
            ],
        ),
    );
    let places: Vec<u8> = [1u32, 0x10, 1, 0x0200_0001, 7, 10, 20, 300, 200]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let opacities = hash(
        Elem::Word,
        Elem::Word,
        2,
        &[(Item::Word(0x10), Item::Word(0.75f32.to_bits()))],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &call(
            request::WINDOW_SETTINGS,
            &[
                native(1, opacities),
                native(
                    111,
                    NativeObject::Packed {
                        ty: 111,
                        bytes: places.clone(),
                    },
                ),
            ],
        ),
    );
    let shortcut: Vec<u8> = [0x4000_0001u32, 1]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &call(
            request::SHORTCUT,
            &[
                native(
                    172,
                    NativeObject::Packed {
                        ty: 172,
                        bytes: shortcut.clone(),
                    },
                ),
                Arg::Int(5),
            ],
        ),
    );
    h.run(20);
    log_off(&mut h, b, id);
    let from = h.bots[b].inbox.len();
    enter(&mut h, b, id, true);
    let mut want = Options::new_character(&[(835, 0), (96, 0), (176, 0), (207, 0), (1, 1), (7, 1)]);
    want.set_game_options(range, 2, 0x7F, 0x3F1);
    want.window_places = Some(places);
    want.opacities = Some(yalain_world::options::Pairs {
        size_index: 2,
        pairs: vec![(0x10, 0.75f32.to_bits())],
    });
    want.set_shortcut(5, shortcut);
    let packed = want.to_packed();
    let start = sent_order(&h, b, from, true)
        .into_iter()
        .find(|r| matches!(r.message(), Some(ServerMessage::PlayerCall(c)) if c.function.0 == function::SESSION_START))
        .expect("a session start");
    assert!(
        start.bytes.windows(packed.len()).any(|w| w == packed),
        "the kept options"
    );
    assert!(
        !h.server
            .unhandled
            .counts()
            .keys()
            .any(|k| k.contains("8212")),
        "every option call kept"
    );
}

/// Without a referral (a setting), the world comes on the login connection: the same entry, the
/// private messages numbered from 1 at every entry, positions sent back; logging off returns to
/// the character list on the same connection.
/// Behaviour: server.world.entered-without-referral
#[test]
fn without_a_referral_the_world_comes_on_the_login_connection() {
    let cfg = ServerConfig {
        referral: false,
        ..ServerConfig::default()
    };
    let mut h = Harness::with(cfg, MemoryStore::default(), yalain_testkit::test_rules());
    let b = at_character_list(&mut h, "tester");
    let id = create(&mut h, b, "Babyfare");
    for round in 0..2 {
        let from = h.bots[b].inbox.len();
        enter(&mut h, b, id, false);
        assert!(
            h.bots[b].referrals.is_empty() && !h.bots[b].in_world(),
            "no referral, no world connection"
        );
        let v: Vec<Received> = sent_order(&h, b, from, false)
            .into_iter()
            .filter(|r| r.blob_id >> 56 != 0x23)
            .collect();
        assert!(is_player_object(&v[0], id), "round {round}");
        // A new character's craft skills come once, before its first registry.
        // A new character is told its conflict status too.
        let privates: Vec<u16> = (1..=if round == 0 { 10 } else { 8 }).collect();
        assert_eq!(private_stamps(&v), privates, "round {round}");
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            false,
            queue::WEENIE,
            &position(0x0299_0109, Vec3::new(61.0, 238.5, -44.6), 0x40),
        );
        assert!(h.run_until(STEPS, |h| !ephemeral(h, b, from).is_empty()));
        log_off(&mut h, b, id);
    }
}

/// The line that the character could not be logged into the authorization server, so its
/// friends list is out of reach, comes as it enters only when it is true: the world serves no
/// friends service, or the account has no stream on it (the client presents its ticket before the
/// game login). An account with a stream enters without it.
/// Behaviour: server.friends.entry-line-only-without-a-stream
#[test]
fn the_line_that_friends_are_out_of_reach_comes_at_entry_only_without_a_friends_stream() {
    use std::collections::HashSet;
    use yalain_store::Store as _;
    // (the world's friends service: none, or whether the account has a stream; the line shown)
    for (service, shown) in [(None, true), (Some(false), true), (Some(true), false)] {
        let mut h = Harness::new();
        let b = at_character_list(&mut h, "tester");
        let id = create(&mut h, b, "Elara");
        let account = h
            .server
            .store
            .account_by_name("tester")
            .expect("reads")
            .expect("an account")
            .id;
        h.server.friends_streams = service.map(|has: bool| {
            if has {
                HashSet::from([account])
            } else {
                HashSet::new()
            }
        });
        let from = h.bots[b].inbox.len();
        let m = ClientMessage::PlayCharacter(PlayCharacter {
            character: id,
            account: NarrowString::from("tester"),
        });
        h.bots[b].send(false, queue::LOGON, &m);
        assert!(
            h.run_until(STEPS, |h| h.bots[b].inbox[from..].iter().any(|r| matches!(
                r.message(),
                Some(ServerMessage::PlayerCall(c)) if c.function.0 == function::SESSION_START
            ))),
            "the session starts"
        );
        h.run(10);
        let lines = h.bots[b].inbox[from..]
            .iter()
            .filter(|r| r.message() == Some(yalain_world::session::welcome()))
            .count();
        assert_eq!(
            lines,
            usize::from(shown),
            "friends service {service:?}: the line {}",
            if shown { "once" } else { "never" }
        );
    }
}
