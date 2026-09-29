//! Shared virtual-time server fixture and message helpers.

#![allow(unused_imports)]

pub(crate) use std::sync::Arc;

pub(crate) use dereth_protocol::comms::{CommunicationTalk, CommunicationTextboxString};
pub(crate) use dereth_protocol::movement::{InterpretedMotionState, MovementBody};
pub(crate) use dereth_protocol::objects::{ItemCreateObject, ItemDeleteObject};
pub(crate) use dereth_protocol::Reader;
pub(crate) use empyrean_content::models::world::{Event, Weenie, WeeniePropertiesGenerator};
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    ChatMessageType, GameEventState, GeneratorDestruct, GeneratorTimeType, MotionCommand,
    MotionStance, PropertyDataId, PropertyInt, PropertyString, WeenieType,
};
pub(crate) use empyrean_entity::{LandblockId, ObjectGuid, Position};
pub(crate) use empyrean_net::{SessionId, SessionState};
pub(crate) use empyrean_testkit::land::{self, TEST_SETUP};
pub(crate) use empyrean_testkit::{ClientId, TestServer};
pub(crate) use empyrean_world::managers::event_manager;
pub(crate) use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
pub(crate) use empyrean_world::managers::landblock_manager;
pub(crate) use empyrean_world::world_objects::creature_death;
pub(crate) use empyrean_world::world_objects::world_object::CtorEnv;
pub(crate) use empyrean_world::World;

pub(crate) const LB: u32 = 0xA9B4_0000;

pub(crate) const PLAYER_WCID: u32 = 1;
pub(crate) const DRUDGE: u32 = 7;
pub(crate) const CORPSE: u32 = 21;
pub(crate) const SPAWN: u32 = 2001;
pub(crate) const GENERATOR: u32 = 1000;

pub(crate) const ALPHA: u32 = 0x5000_0001;
pub(crate) const BRAVO: u32 = 0x5000_0002;

pub(crate) const EVENT: &str = "EventI15Festival";

pub(crate) fn weenie(wcid: u32, class_name: &str, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, class_name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

/// Players, a drudge, the corpse weenie, and an event generator (RegenerationInterval 0, Destroy on
/// disable, as the world DB's seasonal generators are) for `EVENT`, stored `Off`.
pub(crate) fn content() -> MemContent {
    let mut generator = weenie(
        GENERATOR,
        "i15gen",
        "Festival Generator",
        WeenieType::Generic,
    )
    .with_int(PropertyInt::InitGeneratedObjects, 1)
    .with_int(PropertyInt::MaxGeneratedObjects, 1)
    .with_int(PropertyInt::GeneratorTimeType, GeneratorTimeType::Event.0)
    .with_int(
        PropertyInt::GeneratorEndDestructionType,
        GeneratorDestruct::Destroy.0,
    )
    .with_string(PropertyString::GeneratorEvent, EVENT);
    generator.weenie_properties_generator = vec![WeeniePropertiesGenerator {
        id: 1,
        object_id: GENERATOR,
        probability: -1.0,
        weenie_class_id: SPAWN,
        init_create: 1,
        max_create: 1,
        when_create: 1,  // RegenerationType.Destruction
        where_create: 2, // RegenLocationType.Scatter
        ..Default::default()
    }];
    MemContent::new()
        .weenie(
            weenie(PLAYER_WCID, "human", "human", WeenieType::Creature).with_did(
                empyrean_entity::enums::PropertyDataId::CombatTable,
                0x3000_0000,
            ),
        )
        .weenie(weenie(DRUDGE, "drudge", "Drudge", WeenieType::Creature))
        .weenie(weenie(CORPSE, "corpse", "Corpse", WeenieType::Corpse))
        .weenie(weenie(
            SPAWN,
            "i15buffer",
            "Festival Buffer",
            WeenieType::Creature,
        ))
        .weenie(generator)
        .event(Event {
            id: 1,
            name: EVENT.to_owned(),
            start_time: -1,
            end_time: -1,
            state: GameEventState::Off.0,
            ..Default::default()
        })
}

pub(crate) struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

pub(crate) fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

/// The content is installed before start-up, so `EventManager.Initialize` loads its events.
pub(crate) fn server() -> TestServer {
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .build()
        .expect("fake dats");
    let mut ts = TestServer::with_setup(dats, |w| {
        w.content = Arc::new(content());
        guid_manager::initialize(w, &mut EmptyShard);
    });
    land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
    empyrean_command::command_manager::initialize(None);
    ts
}

/// A client whose session plays `guid`, a player standing at `pos` (as `DoPlayerEnterWorld` binds
/// it), made an admin when `admin`.
pub(crate) fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
    pos: Position,
    admin: bool,
) -> ClientId {
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
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
    o.set_location(Some(pos));
    if admin {
        o.set_is_admin_prop(true);
    }
    w.objects.insert(o).expect("fresh");
    let online = empyrean_world::managers::player_manager::OnlinePlayer {
        guid: ObjectGuid::new(guid),
        account: None,
    };
    assert!(w.player_manager.online_players.try_add(guid, online));

    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(ObjectGuid::new(guid)));
    assert!(
        landblock_manager::add_object(w, ObjectGuid::new(guid), false),
        "the player joins its landblock"
    );
    ts.advance(0.1);
    id
}

pub(crate) fn new_object(w: &mut World, wcid: u32) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie),
            guid,
        )
    })
    .expect("constructible");
    w.objects.insert(o).expect("fresh");
    guid
}

pub(crate) fn on_ground(w: &mut World, wcid: u32, pos: Position) -> ObjectGuid {
    let g = new_object(w, wcid);
    w.objects.get_mut(g).unwrap().set_location(Some(pos));
    assert!(landblock_manager::add_object(w, g, false));
    g
}

/// Types `line` in chat; the chat lines received for it.
pub(crate) fn say(ts: &mut TestServer, id: ClientId, line: &str) -> Vec<(String, u32)> {
    let before = ts.received::<CommunicationTextboxString>(id).len();
    ts.send_game_action(
        id,
        &CommunicationTalk {
            message: line.to_owned(),
        },
    );
    ts.advance(0.5);
    ts.received::<CommunicationTextboxString>(id)[before..]
        .iter()
        .map(|m| (m.text.clone(), m.text_type))
        .collect()
}

pub(crate) fn broadcast(text: &str) -> (String, u32) {
    (text.to_owned(), ChatMessageType::Broadcast.0)
}

pub(crate) fn wcid_of(ts: &TestServer, id: u32) -> Option<u32> {
    ts.world
        .objects
        .get(ObjectGuid::new(id))
        .map(|o| o.biota.weenie_class_id)
}

/// The live objects of `wcid` on `0xA9B4` and the landblocks around it.
pub(crate) fn live(ts: &TestServer, wcid: u32) -> Vec<ObjectGuid> {
    let mut v = Vec::new();
    for x in 0xA8..=0xAAu32 {
        for y in 0xB3..=0xB5u32 {
            let Some(l) = ts
                .world
                .landblock_manager
                .landblocks
                .get(LandblockId::new((x << 24) | (y << 16) | 0xFFFF))
            else {
                continue;
            };
            v.extend(
                l.get_all_world_objects_for_diagnostics()
                    .into_iter()
                    .filter(|&g| {
                        ts.world
                            .objects
                            .get(g)
                            .is_some_and(|o| o.biota.weenie_class_id == wcid)
                    }),
            );
        }
    }
    v.sort_unstable();
    v
}

/// The interpreted motion state a CreateObject's PhysicsDesc movement data carries (movement type
/// Invalid), with the style it names.
pub(crate) fn create_motion(c: &ItemCreateObject) -> (u16, InterpretedMotionState) {
    let (buf, _autonomous) =
        c.0.physicsdesc
            .movement
            .as_ref()
            .expect("PhysicsDescriptionFlag.Movement");
    assert!(!buf.is_empty(), "a movement buffer");
    let body = MovementBody::read(&mut Reader::new(buf)).expect("the movement data decodes");
    assert_eq!(
        body.movement_type, 0,
        "MovementType.Invalid: an interpreted state"
    );
    (
        body.current_style,
        body.interpreted.expect("an interpreted state"),
    )
}

/// `MotionCommand.Dead` and `MotionStance.NonCombat` as the interpreted state sends them (16 bits).
#[allow(clippy::cast_possible_truncation)]
pub(crate) const DEAD: u16 = MotionCommand::Dead.0 as u16;
#[allow(clippy::cast_possible_truncation)]
pub(crate) const NON_COMBAT: u16 = MotionStance::NonCombat.0 as u16;

pub(crate) fn assert_lies_dead(c: &ItemCreateObject, what: &str) {
    let (style, state) = create_motion(c);
    assert_eq!(style, NON_COMBAT, "{what}: NonCombat");
    assert_eq!(
        state.forward_command,
        Some(DEAD),
        "{what}: the corpse's CreateObject carries the Dead motion"
    );
}

// ------------------------------------------------------------------------------ real content

#[cfg(feature = "real-content")]
pub(crate) mod real {
    //! Holtburg (`0xA9B4`) on the retail dats and `world.pack`: the Pumpkin Buffer Generator
    //! (wcid 80022, `GeneratorEvent` EventFallFestival, stored `Off` in the world DB) spawns its
    //! Pumpkin Buffer (wcid 32205) only while the event is on; and a chicken's, a drudge's and a
    //! player's corpses are created lying dead.

    pub(crate) use std::sync::Arc;

    pub(crate) use dereth_protocol::objects::ItemCreateObject;
    pub(crate) use empyrean_content::PackContent;
    pub(crate) use empyrean_dat::{DatManager, RealDats};
    pub(crate) use empyrean_entity::enums::{GameEventState, PropertyDataId};
    pub(crate) use empyrean_entity::{LandblockId, ObjectGuid, Position};
    pub(crate) use empyrean_testkit::TestServer;
    pub(crate) use empyrean_world::managers::event_manager;
    pub(crate) use empyrean_world::world_objects::creature_death;

    pub(crate) use super::{
        assert_lies_dead, broadcast, join, live, on_ground, say, wcid_of, EmptyShard, ALPHA, BRAVO,
    };

    pub(crate) const HOLTBURG: u16 = 0xA9B4;
    pub(crate) const FALL_FESTIVAL: &str = "EventFallFestival";
    pub(crate) const PUMPKIN_BUFFER: u32 = 32205;
    pub(crate) const PUMPKIN_BUFFER_GENERATOR: u32 = 80022;
    /// The Pumpkin Kin setup the Pumpkin Buffer shares with the Pumpkin Kin and the Pet Pumpkin.
    pub(crate) const PUMPKIN_SETUP: u32 = 0x0200_14E0;
    /// The creature chicken (wcid 262 "Chicken" is the food item).
    pub(crate) const CHICKEN: u32 = 24937;
    pub(crate) const DRUDGE_SKULKER: u32 = 7;
    pub(crate) const CORPSE: u32 = 21;

    pub(crate) fn dats() -> Arc<DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(source)).expect("the retail dats initialize")
    }

    pub(crate) fn pack() -> PackContent {
        let path = empyrean_common::test_paths::world_pack();
        PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        })
    }

    pub(crate) fn server() -> TestServer {
        let ts = TestServer::with_setup(dats(), |w| {
            w.content = Arc::new(pack());
            empyrean_world::managers::guid_manager::initialize(w, &mut EmptyShard);
        });
        empyrean_command::command_manager::initialize(None);
        ts
    }

    /// Holtburg's outdoor cell `0xA9B40019` (the Pumpkin Buffer Generator's), near the town centre.
    pub(crate) fn holtburg(x: f32, y: f32, z: f32) -> Position {
        Position::from_components(
            u32::from(HOLTBURG) << 16 | 0x0019,
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

    pub(crate) fn landblock_id() -> LandblockId {
        LandblockId::new(u32::from(HOLTBURG) << 16 | 0xFFFF)
    }
}
