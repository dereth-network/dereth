//! ACE: Source/ACE.Server/WorldObjects/Player_Use.cs::HandleActionUseItem
//! Door opens for all and closes on its timer; chest walked to and opened; locked chest opens
//! with its key only; apple raises health; healing kit; corpse opened and looted; vendor via Use.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_primitives::ObjectId;
use dereth_protocol::comms::CommunicationTextboxString;
use dereth_protocol::items::{
    InventoryPutItemInContainer, InventoryUseEvent, InventoryUseWithTargetEvent,
};
use dereth_protocol::movement::MovementSetObjectMovement;
use dereth_protocol::objects::{
    EffectsSoundEvent, ItemOnViewContents, ItemServerSaysContainId, ItemUseDone,
};
use dereth_protocol::qualities::QualitiesUpdateBool;
use dereth_protocol::trade::VendorInfo;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CharacterOptions1, DestinationType, ItemType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyString, Skill,
    SkillAdvancementClass, Sound, WeenieError, WeenieType,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::landblock_manager;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::world_object::{self, CtorEnv, WorldObject};
use empyrean_world::world_objects::{container, player_use};
use empyrean_world::World;

const LB: u32 = 0xA9B4_0000;

const PLAYER_WCID: u32 = 1;
const DOOR: u32 = 2;
const CHEST: u32 = 3;
const KEY: u32 = 4;
const GEM: u32 = 5;
const APPLE: u32 = 6;
const KIT: u32 = 7;
const CORPSE: u32 = 8;
const SHOPKEEPER: u32 = 9;
const DRUDGE: u32 = 10;

const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
const CONTAIN_ID: u32 = 0x0022;
const REMOVE_OBJECT: u32 = 0x0024;
const VENDOR_INFO: u32 = 0x0062;
const VIEW_CONTENTS: u32 = 0x0196;
const USE_DONE: u32 = 0x01C7;
const PUBLIC_BOOL: u32 = 0x02D2;
const PRIVATE_VITAL: u32 = 0x02E9;
const CREATE_OBJECT: u32 = 0xF745;
const MOVEMENT: u32 = 0xF74C;
const SOUND: u32 = 0xF750;
const CHAT: u32 = 0xF7E0;

fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

fn int(t: ItemType) -> i32 {
    i32::try_from(t.0).unwrap()
}

fn content() -> MemContent {
    let mut shopkeeper = weenie(SHOPKEEPER, "Shopkeeper", WeenieType::Vendor)
        .with_int(
            PropertyInt::MerchandiseItemTypes,
            int(ItemType::Misc) | int(ItemType::Gem),
        )
        .with_int(PropertyInt::MerchandiseMinValue, 0)
        .with_int(PropertyInt::MerchandiseMaxValue, 100_000)
        .with_float(PropertyFloat::BuyPrice, 0.9)
        .with_float(PropertyFloat::SellPrice, 1.5)
        .with_float(PropertyFloat::UseRadius, 3.0);
    shopkeeper.weenie_properties_create_list = vec![
        empyrean_content::models::world::WeeniePropertiesCreateList {
            id: 1,
            object_id: SHOPKEEPER,
            destination_type: i8::try_from(DestinationType::Shop.0).unwrap(),
            weenie_class_id: GEM,
            stack_size: -1,
            ..Default::default()
        },
    ];

    MemContent::new()
        .weenie(
            weenie(PLAYER_WCID, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemType, int(ItemType::Creature))
                .with_int(PropertyInt::Level, 1)
                .with_int64(empyrean_entity::enums::PropertyInt64::TotalExperience, 0)
                .with_int64(
                    empyrean_entity::enums::PropertyInt64::AvailableExperience,
                    0,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(weenie(DOOR, "Door", WeenieType::Door).with_float(PropertyFloat::UseRadius, 3.0))
        .weenie(
            weenie(CHEST, "Chest", WeenieType::Chest)
                .with_int(PropertyInt::ItemType, int(ItemType::Container))
                .with_float(PropertyFloat::UseRadius, 1.0),
        )
        .weenie(
            weenie(KEY, "Key", WeenieType::Key)
                .with_int(PropertyInt::Structure, 3)
                .with_int(PropertyInt::MaxStructure, 3)
                .with_int(PropertyInt::ItemType, int(ItemType::Key))
                .with_int(
                    PropertyInt::TargetType,
                    int(ItemType::Container) | int(ItemType::Portal) | int(ItemType::Misc),
                ),
        )
        .weenie(
            weenie(GEM, "Gem", WeenieType::Generic)
                .with_int(PropertyInt::ItemType, int(ItemType::Gem))
                .with_int(PropertyInt::Value, 10),
        )
        .weenie(
            weenie(APPLE, "Apple", WeenieType::Food)
                .with_int(
                    PropertyInt::BoosterEnum,
                    i32::from(PropertyAttribute2nd::Health.0),
                )
                .with_int(PropertyInt::BoostValue, 10)
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 1),
        )
        .weenie(
            weenie(KIT, "Healing Kit", WeenieType::Healer)
                .with_int(
                    PropertyInt::BoosterEnum,
                    i32::from(PropertyAttribute2nd::Health.0),
                )
                .with_int(PropertyInt::BoostValue, 5000)
                .with_float(PropertyFloat::HealkitMod, 1.0)
                .with_int(PropertyInt::Structure, 2)
                .with_int(PropertyInt::MaxStructure, 10)
                .with_int(PropertyInt::TargetType, int(ItemType::Creature)),
        )
        .weenie(
            weenie(CORPSE, "corpse", WeenieType::Corpse).with_int(PropertyInt::ItemsCapacity, 120),
        )
        .weenie(drudge())
        .weenie(shopkeeper)
        // the vendor's currency (Vendor.ValidateVendorRequirements)
        .weenie(
            Weenie::new(273, "coinstack", WeenieType::Coin)
                .with_string(PropertyString::Name, "Pyreal")
                .with_did(PropertyDataId::Setup, TEST_SETUP)
                .with_int(PropertyInt::ItemType, int(ItemType::Money))
                .with_int(PropertyInt::MaxStackSize, 25000)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::StackUnitValue, 1)
                .with_int(PropertyInt::Value, 1),
        )
}

fn drudge() -> Weenie {
    use empyrean_content::models::world::weenie_properties_attribute::WeeniePropertiesAttribute;
    use empyrean_content::models::world::weenie_properties_attribute_2nd::WeeniePropertiesAttribute2nd;
    use empyrean_content::models::world::WeeniePropertiesCreateList;

    let mut d =
        weenie(DRUDGE, "Drudge", WeenieType::Creature).with_bool(PropertyBool::Attackable, true);
    d.weenie_properties_attribute = [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ]
    .into_iter()
    .map(|a| WeeniePropertiesAttribute {
        object_id: DRUDGE,
        r#type: a.0,
        init_level: 60,
        ..Default::default()
    })
    .collect();
    d.weenie_properties_attribute_2nd = [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ]
    .into_iter()
    .map(|v| WeeniePropertiesAttribute2nd {
        object_id: DRUDGE,
        r#type: v.0,
        init_level: 50,
        current_level: 50,
        ..Default::default()
    })
    .collect();
    d.weenie_properties_create_list = vec![WeeniePropertiesCreateList {
        object_id: DRUDGE,
        destination_type: i8::try_from(DestinationType::Treasure.0).unwrap(),
        weenie_class_id: GEM,
        stack_size: 1,
        ..Default::default()
    }];
    d
}

fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

fn server() -> TestServer {
    let dats = FakeDats::new().with_xp_table(empyrean_dat::fake::sample::xp_table());
    let mut ts = TestServer::with_setup(
        empyrean_testkit::dats::with_stat_tables(dats)
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

/// A client logged in as `account` whose session plays `guid` at `pos`: every attribute 100, the
/// three vitals at 100 (their records' initial level), Healing trained.
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
    for a in [
        PropertyAttribute::Strength,
        PropertyAttribute::Endurance,
        PropertyAttribute::Coordination,
        PropertyAttribute::Quickness,
        PropertyAttribute::Focus,
        PropertyAttribute::Self_,
    ] {
        o.biota
            .properties_attribute
            .get_or_insert_with(Default::default)
            .get_or_insert_with(a, Default::default)
            .init_level = 100;
    }
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        let rec = o
            .biota
            .properties_attribute_2nd
            .get_or_insert_with(Default::default)
            .get_or_insert_with(v, Default::default);
        rec.init_level = 100;
        rec.current_level = 100;
    }
    let s = o.get_creature_skill(Skill::Healing, true).unwrap();
    s.set_advancement_class(&mut o, SkillAdvancementClass::Trained);
    s.set_init_level(&mut o, 10);
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

fn new_object(w: &mut World, wcid: u32) -> ObjectGuid {
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

/// A new object of `wcid` on the ground at `pos`.
fn on_ground(ts: &mut TestServer, wcid: u32, pos: Position) -> ObjectGuid {
    let g = new_object(&mut ts.world, wcid);
    ts.world.objects.get_mut(g).unwrap().set_location(Some(pos));
    assert!(landblock_manager::add_object(&mut ts.world, g, false));
    g
}

/// A new object of `wcid` in `owner`'s main pack (a player's or a container's).
fn inside(ts: &mut TestServer, owner: ObjectGuid, wcid: u32) -> ObjectGuid {
    let g = new_object(&mut ts.world, wcid);
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        owner,
        g,
        0,
        false,
        true
    ));
    g
}

fn obj(ts: &TestServer, g: ObjectGuid) -> &WorldObject {
    ts.world.objects.get(g).expect("live object")
}

fn obj_mut(ts: &mut TestServer, g: ObjectGuid) -> &mut WorldObject {
    ts.world.objects.get_mut(g).expect("live object")
}

/// Every message the client received from index `from` on (the Age heartbeat left out).
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
        .filter(|g| !is_age_update(&g.blob))
        .collect()
}

/// Runs `act`, advances the server `secs`, and returns what each listed client received meanwhile.
fn during(
    ts: &mut TestServer,
    ids: &[ClientId],
    secs: f64,
    act: impl FnOnce(&mut TestServer),
) -> Vec<Vec<Got>> {
    let from: Vec<usize> = ids.iter().map(|&id| ts.received_raw(id).len()).collect();
    act(ts);
    ts.advance(secs);
    ids.iter()
        .zip(from)
        .map(|(&id, n)| got(ts, id, n))
        .collect()
}

fn all(g: &[Got], kind: u32) -> Vec<&Got> {
    g.iter().filter(|m| m.kind == kind).collect()
}

/// The UseDone failure codes.
fn use_dones(g: &[Got]) -> Vec<u32> {
    all(g, USE_DONE)
        .iter()
        .map(|m| m.decode::<ItemUseDone>().failure_type)
        .collect()
}

/// The chat lines.
fn chats(g: &[Got]) -> Vec<String> {
    all(g, CHAT)
        .iter()
        .map(|m| m.decode::<CommunicationTextboxString>().text)
        .collect()
}

/// The motions of `object`, as their decoded movement buffers.
fn motions_of(g: &[Got], object: ObjectGuid) -> Vec<dereth_protocol::movement::MovementBuffer> {
    all(g, MOVEMENT)
        .iter()
        .map(|m| m.decode::<MovementSetObjectMovement>())
        .filter(|m| m.id.0 == object.full())
        .map(|m| m.decoded_movement().expect("a movement buffer"))
        .collect()
}

fn we(e: WeenieError) -> u32 {
    u32::try_from(e.0).unwrap()
}

/// Moves a player's body and `Location` (what the client's autonomous position updates do).
fn walk_to(ts: &mut TestServer, player: u32, pos: Position) {
    let w = &mut ts.world;
    let g = ObjectGuid::new(player);
    let h = phys_ext::physics_obj(w, g).expect("a body");
    assert!(phys_ext::set_position(
        w,
        h,
        &phys_ext::to_physics_position(&pos)
    ));
    world_object::sync_location(w, g);
}

fn health(ts: &TestServer, player: u32) -> u32 {
    let o = obj(ts, ObjectGuid::new(player));
    o.health().current(o)
}

// ------------------------------------------------------------------ scenarios

/// A player uses a door (`HandleActionUseItem` -> the MoveTo chain, already in reach -> `TryUseItem`
/// -> `OnActivate` -> `Door.ActOnUse` -> `Door.Open`): the open motion reaches the player and an
/// onlooker, UseDone follows; `ResetInterval` (30 s) later the door closes itself and both see the
/// close motion (`Door.Reset` -> `Door.Close`).
#[test]
fn a_door_opens_for_all_to_see_and_closes_on_its_timer() {
    let mut ts = server();
    let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let (bravo, _) = join(&mut ts, "bravo", BRAVO, "Bravo", at(24.0, 20.0));
    let door = on_ground(&mut ts, DOOR, at(21.5, 20.0));
    ts.advance(0.5);

    let got = during(&mut ts, &[alpha, bravo], 1.0, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseEvent {
                object: ObjectId(door.full()),
            },
        )
    });
    let (a, b) = (&got[0], &got[1]);
    assert!(obj(&ts, door).is_open());
    let opened = motions_of(a, door);
    assert_eq!(
        opened.len(),
        1,
        "the player sees the door open: {:04X?}",
        kinds(a)
    );
    assert_eq!(motions_of(b, door), opened, "so does the onlooker");
    assert_eq!(opened[0].body.movement_type, 0, "an interpreted motion");
    assert_eq!(use_dones(a), [0]);
    assert!(use_dones(b).is_empty());

    let got = during(&mut ts, &[alpha, bravo], 30.0, |_| {});
    assert!(!obj(&ts, door).is_open(), "closed by its reset timer");
    let closed = motions_of(&got[1], door);
    assert_eq!(closed.len(), 1, "the onlooker sees it close");
    assert_ne!(
        closed[0]
            .body
            .interpreted
            .as_ref()
            .and_then(|i| i.forward_command),
        opened[0]
            .body
            .interpreted
            .as_ref()
            .and_then(|i| i.forward_command)
    );
    assert_eq!(motions_of(&got[0], door), closed);
}

/// Opening a chest from out of range: the MoveToObject motion, the client walks, then
/// `Chest.Open` -> `Container.Open`: ViewContents with the chest's item, its CreateObject, and
/// UseDone (`CreateMoveToChain`, `Container.SendInventory`).
#[test]
fn a_chest_out_of_reach_is_walked_to_then_opened() {
    let mut ts = server();
    let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let chest = on_ground(&mut ts, CHEST, at(27.0, 20.0));
    let gem = inside(&mut ts, chest, GEM);
    ts.advance(0.5);

    let got = during(&mut ts, &[alpha], 0.5, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseEvent {
                object: ObjectId(chest.full()),
            },
        )
    });
    let a = &got[0];
    assert_eq!(
        kinds(a),
        [MOVEMENT],
        "only the MoveToObject motion while out of reach"
    );
    assert_eq!(
        motions_of(a, ObjectGuid::new(ALPHA))[0].body.movement_type,
        6,
        "MoveToObject"
    );
    assert!(!obj(&ts, chest).is_open());

    let got = during(&mut ts, &[alpha], 0.5, |ts| {
        walk_to(ts, ALPHA, at(26.0, 20.0))
    });
    let a = &got[0];
    assert!(
        obj(&ts, chest).is_open(),
        "opened on arrival: {:04X?}",
        kinds(a)
    );
    let view: ItemOnViewContents = first(a, VIEW_CONTENTS).decode();
    assert_eq!(view.container.0, chest.full());
    assert_eq!(
        view.contents.iter().map(|c| c.iid.0).collect::<Vec<_>>(),
        [gem.full()],
        "ViewContents lists the chest's item"
    );
    assert!(
        all(a, CREATE_OBJECT)
            .iter()
            .any(|m| m.blob[4..8] == gem.full().to_le_bytes()),
        "the chest's item is created for the viewer"
    );
    assert_eq!(use_dones(a), [0]);
    assert_eq!(
        player_use::fields(&ts.world, ObjectGuid::new(ALPHA)).last_opened_container_id,
        chest
    );
}

/// A locked chest (`Chest.CheckUseRequirements` refuses with the locked sound); the wrong key
/// (`UseWithTarget` -> `Key.HandleActionUseOnTarget` -> `UseUnlocker` -> `LockHelper.Unlock`) is
/// answered KeyDoesntFitThisLock; the right one unlocks it (Locked false and the LockSuccess
/// sound to everyone near, the uses-left line, the key's Structure, UseDone), and the chest
/// then opens.
#[test]
fn a_locked_chest_opens_with_its_key_and_not_another() {
    let mut ts = server();
    let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let chest = on_ground(&mut ts, CHEST, at(20.8, 20.0));
    obj_mut(&mut ts, chest).set_lock_code(Some("oak".to_owned()));
    obj_mut(&mut ts, chest).set_is_locked(true);
    let right = inside(&mut ts, ObjectGuid::new(ALPHA), KEY);
    obj_mut(&mut ts, right).set_property(PropertyString::KeyCode, "OAK".to_owned());
    let wrong = inside(&mut ts, ObjectGuid::new(ALPHA), KEY);
    obj_mut(&mut ts, wrong).set_property(PropertyString::KeyCode, "elm".to_owned());
    ts.advance(0.5);

    // locked
    let got = during(&mut ts, &[alpha], 0.5, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseEvent {
                object: ObjectId(chest.full()),
            },
        )
    });
    let sound: EffectsSoundEvent = first(&got[0], SOUND).decode();
    assert_eq!(
        (sound.id.0, sound.sound_type),
        (
            chest.full(),
            i32::try_from(Sound::OpenFailDueToLock.0).unwrap()
        )
    );
    assert_eq!(use_dones(&got[0]), [0]);
    assert!(!obj(&ts, chest).is_open());

    // the wrong key
    let got = during(&mut ts, &[alpha], 0.5, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseWithTargetEvent {
                object: ObjectId(wrong.full()),
                target: ObjectId(chest.full()),
            },
        );
    });
    assert_eq!(
        use_dones(&got[0]),
        [we(WeenieError::KeyDoesntFitThisLock)],
        "{:04X?}",
        kinds(&got[0])
    );
    assert!(obj(&ts, chest).is_locked());

    // the right key
    let got = during(&mut ts, &[alpha], 0.5, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseWithTargetEvent {
                object: ObjectId(right.full()),
                target: ObjectId(chest.full()),
            },
        );
    });
    let a = &got[0];
    assert!(!obj(&ts, chest).is_locked());
    let locked: QualitiesUpdateBool = first(a, PUBLIC_BOOL).decode();
    assert_eq!(
        (locked.0.object.0, locked.0.property_id, locked.0.value),
        (chest.full(), u32::from(PropertyBool::Locked.0), 0)
    );
    let sound: EffectsSoundEvent = first(a, SOUND).decode();
    assert_eq!(
        (sound.id.0, sound.sound_type),
        (chest.full(), i32::try_from(Sound::LockSuccess.0).unwrap())
    );
    assert_eq!(
        chats(a),
        ["The Chest has been unlocked.\nYour key has 2 uses left."]
    );
    assert_eq!(use_dones(a), [0]);
    assert_eq!(obj(&ts, right).structure(), Some(2));

    // now it opens
    let got = during(&mut ts, &[alpha], 0.5, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseEvent {
                object: ObjectId(chest.full()),
            },
        )
    });
    assert!(obj(&ts, chest).is_open());
    let view: ItemOnViewContents = first(&got[0], VIEW_CONTENTS).decode();
    assert_eq!(view.container.0, chest.full());
}

/// Eating (`Food.ActOnUse` -> `Player.ApplyConsumable`): the eat motion, then `Food.ApplyConsumable`
/// (the Health update, the line, the eat sound, the apple removed), then UseDone after the
/// sequence (`TryUseItem`'s `LastUseTime`).
#[test]
fn eating_an_apple_raises_health() {
    let mut ts = server();
    let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let apple = inside(&mut ts, ObjectGuid::new(ALPHA), APPLE);
    let o = obj_mut(&mut ts, ObjectGuid::new(ALPHA));
    let h = o.health();
    h.set_current(o, 40);
    ts.advance(0.5);

    let got = during(&mut ts, &[alpha], 1.0, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseEvent {
                object: ObjectId(apple.full()),
            },
        )
    });
    let a = &got[0];
    assert_eq!(health(&ts, ALPHA), 50);
    assert_eq!(chats(a), ["The Apple restores 10 points of your Health."]);
    let sound: EffectsSoundEvent = first(a, SOUND).decode();
    assert_eq!(
        (sound.id.0, sound.sound_type),
        (ALPHA, i32::try_from(Sound::Eat1.0).unwrap())
    );
    assert_eq!(all(a, REMOVE_OBJECT).len(), 1, "the apple is eaten");
    assert!(!ts.world.objects.contains(apple));
    let vital = all(a, PRIVATE_VITAL);
    assert_eq!(vital.len(), 1, "the Health update");
    assert_eq!(
        motions_of(a, ObjectGuid::new(ALPHA)).len(),
        2,
        "the eat motion and the return to Ready"
    );
    // (the fake dats have no motion table: every animation is 0 s, so TryUseItem's UseDone,
    // after LastUseTime 0, need not wait for the chain; the client's queues drain separately)
    assert_eq!(use_dones(a), [0]);
    assert!(!obj(&ts, ObjectGuid::new(ALPHA)).wo.world_object.is_busy);
}

/// A healing kit on yourself (`UseWithTarget` with yourself as the target -> `Healer`'s
/// `HandleActionUseOnTarget` -> `DoHealMotion` -> `DoHealing`): the heal motion, the heal line
/// with the uses left, the Health rise it names, the kit's Structure, UseDone. (The kit's large
/// BoostValue makes the skill check a certainty.)
#[test]
fn a_healing_kit_heals_yourself() {
    let mut ts = server();
    let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let kit = inside(&mut ts, ObjectGuid::new(ALPHA), KIT);
    let o = obj_mut(&mut ts, ObjectGuid::new(ALPHA));
    let h = o.health();
    h.set_current(o, 10);
    ts.advance(0.5);

    let got = during(&mut ts, &[alpha], 1.0, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseWithTargetEvent {
                object: ObjectId(kit.full()),
                target: ObjectId(ALPHA),
            },
        );
    });
    let a = &got[0];
    let lines = chats(a);
    assert_eq!(lines.len(), 1, "{:04X?}", kinds(a));
    let healed = health(&ts, ALPHA) - 10;
    assert!(healed > 0);
    let expert = format!(
        "You expertly heal yourself for {healed} Health points. Your Healing Kit has 1 use left."
    );
    let plain =
        format!("You heal yourself for {healed} Health points. Your Healing Kit has 1 use left.");
    assert!(lines[0] == plain || lines[0] == expert, "{lines:?}");
    assert_eq!(obj(&ts, kit).structure(), Some(1));
    assert!(
        !motions_of(a, ObjectGuid::new(ALPHA)).is_empty(),
        "the heal motion"
    );
    assert_eq!(use_dones(a), [0]);
}

/// A killed creatures corpse is opened and looted.
#[test]
fn a_killed_creatures_corpse_is_opened_and_looted() {
    use empyrean_entity::enums::DamageType;

    let mut ts = server();
    let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let player = ObjectGuid::new(ALPHA);
    // a player's level and XP (the kill grants XP; CheckForLevelup loops on a null Level)
    let o = obj_mut(&mut ts, player);
    o.set_level(Some(1));
    o.set_property(empyrean_entity::enums::PropertyInt64::TotalExperience, 0);
    o.set_property(
        empyrean_entity::enums::PropertyInt64::AvailableExperience,
        0,
    );
    let drudge = on_ground(&mut ts, DRUDGE, at(20.8, 20.0));
    ts.advance(0.5);

    assert!(
        empyrean_world::dispatch::take_damage::take_damage(
            &mut ts.world,
            drudge,
            player,
            DamageType::Slash,
            200.0,
            false
        ) > 0
    );
    ts.advance(1.0);
    assert!(ts.world.objects.get(drudge).is_none(), "the drudge is gone");
    let corpse = (ObjectGuid::DYNAMIC_MIN..ObjectGuid::DYNAMIC_MIN + 0x40)
        .map(ObjectGuid::new)
        .find(|&g| {
            ts.world.objects.get(g).is_some_and(|o| {
                o.biota.weenie_class_id == CORPSE && !o.wo.world_object.is_destroyed
            })
        })
        .expect("a corpse");
    let loot = container::inventory_values(&ts.world, corpse);
    assert_eq!(
        loot.len(),
        1,
        "the drudge's gem is in the corpse's Container.Inventory"
    );
    let gem = loot[0];
    assert_eq!(obj(&ts, gem).biota.weenie_class_id, GEM);
    assert_eq!(obj(&ts, corpse).killer_id(), Some(ALPHA));
    let corpse_at = obj(&ts, corpse).location().expect("on the ground");
    walk_to(
        &mut ts,
        ALPHA,
        at(corpse_at.position_x, corpse_at.position_y + 0.5),
    );
    ts.advance(0.2);

    let got = during(&mut ts, &[alpha], 0.5, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseEvent {
                object: ObjectId(corpse.full()),
            },
        )
    });
    let view: ItemOnViewContents = first(&got[0], VIEW_CONTENTS).decode();
    assert_eq!(view.container.0, corpse.full());
    assert_eq!(
        view.contents.iter().map(|c| c.iid.0).collect::<Vec<_>>(),
        [gem.full()],
        "ViewContents lists the loot"
    );
    assert!(
        all(&got[0], CREATE_OBJECT)
            .iter()
            .any(|m| m.blob[4..8] == gem.full().to_le_bytes()),
        "the corpse's item is created for the looter"
    );
    assert_eq!(use_dones(&got[0]), [0]);
    assert_eq!(
        player_use::fields(&ts.world, player).last_opened_container_id,
        corpse
    );

    let got = during(&mut ts, &[alpha], 1.0, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryPutItemInContainer {
                item: ObjectId(gem.full()),
                container: ObjectId(ALPHA),
                slot: 0,
            },
        );
    });
    let contain: ItemServerSaysContainId = first(&got[0], CONTAIN_ID).decode();
    assert_eq!((contain.item.0, contain.container.0), (gem.full(), ALPHA));
    assert_eq!(container::inventory_values(&ts.world, player), vec![gem]);
    assert!(container::inventory_values(&ts.world, corpse).is_empty());
}

/// A vendor used through `HandleActionUseItem` (not `ActOnUse` directly): in reach, `OnActivate`
/// runs `Vendor.ActOnUse`, whose `ApproachVendor` sends VendorInfo; UseDone follows.
#[test]
fn a_vendor_is_used_through_use_item() {
    let mut ts = server();
    let (alpha, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let npc = on_ground(&mut ts, SHOPKEEPER, at(21.0, 20.0));
    ts.advance(0.5);

    let got = during(&mut ts, &[alpha], 1.0, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryUseEvent {
                object: ObjectId(npc.full()),
            },
        )
    });
    let a = &got[0];
    let info: VendorInfo = first(a, VENDOR_INFO).decode();
    assert_eq!(info.merchant_id.0, npc.full());
    assert_eq!(info.items.len(), 1, "the shop's one gem row");
    assert_eq!(use_dones(a), [0]);
    assert_eq!(
        player_use::fields(&ts.world, ObjectGuid::new(ALPHA)).last_opened_container_id,
        npc
    );
}

pub(crate) use crate::support::messages::{first, is_age_update, kinds, Got};

pub(crate) use crate::support::empty_shard::EmptyShard;
