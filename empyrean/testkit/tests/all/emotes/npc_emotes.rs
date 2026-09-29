//! ACE: Source/ACE.Server/WorldObjects/Managers/EmoteManager.cs::ExecuteEmote
//! Using an NPC runs its use emotes in order and in time; giving an item runs its give or refuse
//! emote.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_primitives::ObjectId;
use dereth_protocol::comms::{
    CommunicationHearDirectSpeech, CommunicationHearSpeech, CommunicationTextboxString,
};
use dereth_protocol::items::InventoryGiveObjectRequest;
use dereth_protocol::movement::MovementSetObjectMovement;
use dereth_protocol::objects::{CharacterServerSaysAttemptFailed, EffectsSoundEvent};
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CharacterOptions1, ChatMessageType, EmoteCategory, EmoteType, MotionCommand, PropertyAttribute,
    PropertyBool, PropertyDataId, PropertyInt, PropertyString, Sound, WeenieError, WeenieType,
};
use empyrean_entity::models::properties_emote::PropertiesEmote;
use empyrean_entity::models::properties_emote_action::PropertiesEmoteAction;
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::landblock_manager;
use empyrean_world::world_objects::container;
use empyrean_world::world_objects::managers::emote_manager;
use empyrean_world::world_objects::world_object::CtorEnv;

const LB: u32 = 0xA9B4_0000;

const PLAYER_WCID: u32 = 1;
const GUARD: u32 = 2;
const GEM: u32 = 3;
const COIN: u32 = 4;
const SWORD: u32 = 5;

const ALPHA: u32 = 0x5000_0001;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
const SAVE_FAILED: u32 = 0x00A0;
const CONTAIN_ID: u32 = 0x0022;
const REMOVE_OBJECT: u32 = 0x0024;
const PRIVATE_INT: u32 = 0x02CD;
const HEAR_SPEECH: u32 = 0x02BB;
const TELL: u32 = 0x02BD;
const CREATE_OBJECT: u32 = 0xF745;
const MOVEMENT: u32 = 0xF74C;
const SOUND: u32 = 0xF750;
const CHAT: u32 = 0xF7E0;

fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

fn content() -> MemContent {
    MemContent::new()
        .weenie(
            weenie(PLAYER_WCID, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(
            weenie(GUARD, "Guard", WeenieType::Creature)
                .with_did(PropertyDataId::MotionTable, 0x0900_0001)
                .with_bool(PropertyBool::AllowGive, true),
        )
        .weenie(
            weenie(GEM, "Gem", WeenieType::Generic)
                .with_int(PropertyInt::EncumbranceVal, 5)
                .with_int(PropertyInt::Value, 7),
        )
        .weenie(
            weenie(COIN, "Pyreal", WeenieType::Coin)
                .with_string(PropertyString::PluralName, "Pyreals")
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::StackUnitEncumbrance, 1)
                .with_int(PropertyInt::StackUnitValue, 1),
        )
        .weenie(
            weenie(SWORD, "Sword", WeenieType::MeleeWeapon)
                .with_int(PropertyInt::EncumbranceVal, 100),
        )
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

fn server() -> TestServer {
    let mut ts = TestServer::with_setup(
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
        |w| {
            w.content = Arc::new(content());
            guid_manager::initialize(w, &mut EmptyShard);
        },
    );
    land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
    ts
}

/// A client logged in as `account` whose session plays `guid` at `pos` (as `inventory.rs` joins).
fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
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
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    let character = empyrean_store::models::shard::Character {
        character_options_1: i32::try_from(CharacterOptions1::AllowGive.0).unwrap(),
        ..Default::default()
    };
    o.player.as_mut().expect("a player").player.character = Some(character);
    o.set_location(Some(pos));
    w.objects.insert(o).expect("fresh");

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

/// A new object of `wcid`, in the world's store.
fn new_object(ts: &mut TestServer, wcid: u32) -> ObjectGuid {
    let w = &mut ts.world;
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

/// The Guard NPC standing at `pos` with the emote table `sets`.
fn guard(ts: &mut TestServer, pos: Position, sets: Vec<PropertiesEmote>) -> ObjectGuid {
    let g = new_object(ts, GUARD);
    let o = ts.world.objects.get_mut(g).unwrap();
    o.set_location(Some(pos));
    o.biota.properties_emote = Some(Arc::new(sets));
    assert!(landblock_manager::add_object(&mut ts.world, g, false));
    ts.advance(0.1);
    g
}

fn act(t: EmoteType, delay: f32, message: &str) -> PropertiesEmoteAction {
    PropertiesEmoteAction {
        r#type: t.0.cast_unsigned(),
        delay,
        message: Some(message.to_owned()),
        ..PropertiesEmoteAction::default()
    }
}

fn set(
    category: EmoteCategory,
    weenie_class_id: Option<u32>,
    actions: Vec<PropertiesEmoteAction>,
) -> PropertiesEmote {
    PropertiesEmote {
        category,
        probability: 1.0,
        weenie_class_id,
        properties_emote_action: actions,
        ..PropertiesEmote::default()
    }
}

// ------------------------------------------------------------------ scenarios

/// Using an NPC with a Use emote set: the Tell goes out at once; the Motion waits for its 0.5 s
/// pre-delay and is broadcast (`ExecuteMotion`: UpdateMotion to the players who know the NPC); the
/// Say follows it with no delay (the Motion returns its animation length, 0 without a motion table
/// in the fake dats) as a HearSpeech within `LocalBroadcastRange`; the last Tell waits its 2 s.
///
#[test]
fn using_an_npc_runs_its_use_emotes_in_order_and_in_time() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let mut wave = act(EmoteType::Motion, 0.5, "");
    wave.motion = Some(MotionCommand::Wave);
    let npc = guard(
        &mut ts,
        at(21.0, 20.0),
        vec![set(
            EmoteCategory::Use,
            None,
            vec![
                act(EmoteType::Tell, 0.0, "Greetings, %s."),
                wave,
                act(EmoteType::Say, 0.0, "Move along!"),
                act(EmoteType::Tell, 2.0, "Walk safely, %tn."),
            ],
        )],
    );

    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        emote_manager::on_use(&mut ts.world, npc, ObjectGuid::new(ALPHA))
    });
    assert_eq!(sent, [TELL], "{sent:04X?}");
    let tell: CommunicationHearDirectSpeech = first(&g, TELL).decode();
    assert_eq!(
        (
            tell.message.as_str(),
            tell.sender_name.as_str(),
            tell.sender_id.0,
            tell.target_id.0,
            tell.text_type
        ),
        (
            "Greetings, Alpha.",
            "Guard",
            npc.full(),
            ALPHA,
            ChatMessageType::Tell.0
        )
    );
    assert!(emote_manager::is_busy(&ts.world, npc));

    // the Motion's pre-delay ends at 0.5 s; the Say runs right after it
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |_| {});
    assert_eq!(sent, [MOVEMENT, HEAR_SPEECH], "{sent:04X?}");
    {
        let motion: MovementSetObjectMovement = first(&g, MOVEMENT).decode();
        assert_eq!(motion.id.0, npc.full());
        let buffer = motion.decoded_movement().expect("a movement buffer");
        let state = buffer
            .body
            .interpreted
            .expect("an interpreted motion state");
        assert_eq!(
            state.forward_command,
            Some(u16::try_from(MotionCommand::Wave.0 & 0xFFFF).unwrap())
        );
        let say: CommunicationHearSpeech = first(&g, HEAR_SPEECH).decode();
        assert_eq!(
            (
                say.message.as_str(),
                say.sender_name.as_str(),
                say.sender_id.0,
                say.text_type
            ),
            ("Move along!", "Guard", npc.full(), ChatMessageType::Emote.0)
        );
    }
    assert!(emote_manager::is_busy(&ts.world, npc));

    // nothing until the last Tell's 2 s pre-delay has passed (at 2.5 s)
    let (sent, _) = exchange(&mut ts, id, session, 1.8, |_| {});
    assert_eq!(sent, Vec::<u32>::new());
    let (sent, g) = exchange(&mut ts, id, session, 0.2, |_| {});
    assert_eq!(sent, [TELL]);
    let tell: CommunicationHearDirectSpeech = first(&g, TELL).decode();
    assert_eq!(tell.message, "Walk safely, Alpha.");
    assert!(!emote_manager::is_busy(&ts.world, npc), "the set is done");
}

/// Giving an item to an NPC (`GiveObjectToNPC`): with a Give emote for the item's wcid (and
/// `AllowGive`), the NPC takes it ("You give Guard Gem.", the NPC's ReceiveItem sound), runs the
/// Give set (a Tell, then an emote Give of 5 Pyreals: "Guard gives you 5 Pyreals."), and the gem
/// is destroyed. With a Refuse emote the item is examined and refused
/// (`TradeAiRefuseEmote`) and the Refuse set runs.
#[test]
fn giving_an_item_to_an_npc_runs_its_give_or_refuse_emote() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let mut pay = act(EmoteType::Give, 0.0, "");
    pay.weenie_class_id = Some(COIN);
    pay.stack_size = Some(5);
    let npc = guard(
        &mut ts,
        at(20.5, 20.0),
        vec![
            set(
                EmoteCategory::Give,
                Some(GEM),
                vec![act(EmoteType::Tell, 0.0, "A fine gem."), pay],
            ),
            set(
                EmoteCategory::Refuse,
                Some(SWORD),
                vec![act(EmoteType::Tell, 0.0, "Keep your blade.")],
            ),
        ],
    );
    let gem = new_object(&mut ts, GEM);
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        ObjectGuid::new(ALPHA),
        gem,
        0,
        false,
        true
    ));
    let sword = new_object(&mut ts, SWORD);
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        ObjectGuid::new(ALPHA),
        sword,
        0,
        false,
        true
    ));
    ts.advance(0.1);

    // the refused sword
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(
            id,
            &InventoryGiveObjectRequest {
                target: ObjectId(npc.full()),
                item: ObjectId(sword.full()),
                amount: 1,
            },
        );
    });
    // the player, already in reach, first turns to the NPC (`CreateMoveToChain` -> `Rotate`)
    assert_eq!(sent, [MOVEMENT, CHAT, SAVE_FAILED, TELL], "{sent:04X?}");
    let examine: CommunicationTextboxString = first(&g, CHAT).decode();
    assert_eq!(examine.text, "You allow Guard to examine your Sword.");
    let refused: CharacterServerSaysAttemptFailed = first(&g, SAVE_FAILED).decode();
    assert_eq!(
        (refused.object.0, refused.reason),
        (
            sword.full(),
            u32::try_from(WeenieError::TradeAiRefuseEmote.0).unwrap()
        )
    );
    let tell: CommunicationHearDirectSpeech = first(&g, TELL).decode();
    assert_eq!(tell.message, "Keep your blade.");
    assert_eq!(
        container::inventory_values(&ts.world, ObjectGuid::new(ALPHA)),
        vec![gem, sword]
    );

    // the accepted gem
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(
            id,
            &InventoryGiveObjectRequest {
                target: ObjectId(npc.full()),
                item: ObjectId(gem.full()),
                amount: 1,
            },
        );
    });
    assert_eq!(
        sent,
        [
            MOVEMENT,
            REMOVE_OBJECT,
            PRIVATE_INT,
            CONTAIN_ID,
            CHAT,
            SOUND,
            TELL,
            MOVEMENT,
            CREATE_OBJECT,
            CONTAIN_ID,
            PRIVATE_INT,
            PRIVATE_INT,
            CHAT,
            SOUND
        ],
        "{sent:04X?}"
    );
    assert_eq!(
        ts.world
            .objects
            .get(ObjectGuid::new(ALPHA))
            .unwrap()
            .coin_value(),
        Some(5)
    );
    let sounds: Vec<(u32, i32)> = g
        .iter()
        .filter(|m| m.kind == SOUND)
        .map(|m| m.decode::<EffectsSoundEvent>())
        .map(|e| (e.id.0, e.sound_type))
        .collect();
    let receive_item = i32::try_from(Sound::ReceiveItem.0).unwrap();
    assert_eq!(sounds, [(npc.full(), receive_item), (ALPHA, receive_item)]);
    let chats: Vec<String> = g
        .iter()
        .filter(|m| m.kind == CHAT)
        .map(|m| m.decode::<CommunicationTextboxString>().text)
        .collect();
    assert_eq!(chats, ["You give Guard Gem.", "Guard gives you 5 Pyreals."]);
    let tell: CommunicationHearDirectSpeech = first(&g, TELL).decode();
    assert_eq!(tell.message, "A fine gem.");
    assert!(ts.world.objects.get(gem).is_none(), "the gem is destroyed");
    let inventory = container::inventory_values(&ts.world, ObjectGuid::new(ALPHA));
    assert_eq!(inventory.len(), 2, "the sword and the Pyreals");
    let coins = inventory.iter().find(|&&g| g != sword).copied().unwrap();
    assert_eq!(ts.world.objects.get(coins).unwrap().stack_size(), Some(5));
}

pub(crate) use crate::support::messages::{exchange, first};

pub(crate) use crate::support::empty_shard::EmptyShard;
