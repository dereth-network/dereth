//! ACE: Source/ACE.Server/WorldObjects/Player_Inventory.cs::HandleActionGetAndWieldItem
//! Pickup walks then picks up (never-reached cancelled), drop in front, move between packs,
//! split/merge stacks, wield/dequip, give to player, over-burden refused, every refusal answered,
//! item mana query; plus a drudge kill-task/loot scenario.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

pub(crate) use std::sync::Arc;

pub(crate) use dereth_primitives::ObjectId;
pub(crate) use dereth_protocol::comms::{CommunicationTextboxString, CommunicationTransientString};
pub(crate) use dereth_protocol::items::{
    InventoryDropItem, InventoryGetAndWieldItem, InventoryGiveObjectRequest,
    InventoryPutItemInContainer, InventoryStackableMerge, InventoryStackableSplitToContainer,
    ItemUpdateStackSize,
};
pub(crate) use dereth_protocol::objects::{
    CharacterServerSaysAttemptFailed, EffectsSoundEvent, ItemServerSaysContainId,
    ItemServerSaysMoveItem, ItemWearItem,
};
pub(crate) use dereth_protocol::qualities::{QualitiesPrivateUpdateInt, QualitiesUpdateInstanceId};
pub(crate) use empyrean_content::models::world::Weenie;
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    CharacterOptions1, EquipMask, PropertyAttribute, PropertyDataId, PropertyInt, PropertyString,
    Sound, WeenieError, WeenieType,
};
pub(crate) use empyrean_entity::{ObjectGuid, Position};
pub(crate) use empyrean_net::{SessionId, SessionState};
pub(crate) use empyrean_testkit::land::{self, TEST_SETUP};
pub(crate) use empyrean_testkit::{ClientId, TestServer};
pub(crate) use empyrean_world::managers::guid_manager;
pub(crate) use empyrean_world::managers::landblock_manager;
pub(crate) use empyrean_world::physics::phys_ext;
pub(crate) use empyrean_world::world_objects::world_object::{self, CtorEnv, WorldObject};
pub(crate) use empyrean_world::world_objects::{container, creature_equipment as ce};
pub(crate) use empyrean_world::World;

pub(crate) const LB: u32 = 0xA9B4_0000;

pub(crate) const PLAYER_WCID: u32 = 1;
pub(crate) const PACK: u32 = 2;
pub(crate) const GEM: u32 = 3; // burden 5
pub(crate) const COIN: u32 = 4; // stackable: max 100, unit burden 2
pub(crate) const SWORD: u32 = 5;
pub(crate) const ANVIL: u32 = 6; // burden 5000
pub(crate) const ORB: u32 = 20; // item mana 50 of 200

pub(crate) const ALPHA: u32 = 0x5000_0001;
pub(crate) const BRAVO: u32 = 0x5000_0002;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
pub(crate) const SAVE_FAILED: u32 = 0x00A0;
pub(crate) const CONTAIN_ID: u32 = 0x0022;
pub(crate) const WIELD_ITEM: u32 = 0x0023;
pub(crate) const REMOVE_OBJECT: u32 = 0x0024;
pub(crate) const MOVE_ITEM: u32 = 0x019A;
pub(crate) const STACK_SIZE: u32 = 0x0197;
pub(crate) const TRANSIENT: u32 = 0x02EB;
pub(crate) const PRIVATE_INT: u32 = 0x02CD;
pub(crate) const PUBLIC_INT: u32 = 0x02CE;
pub(crate) const INSTANCE_ID: u32 = 0x02DA;
pub(crate) const CREATE_OBJECT: u32 = 0xF745;
pub(crate) const DELETE_OBJECT: u32 = 0xF747;
pub(crate) const POSITION: u32 = 0xF748;
pub(crate) const PICKUP_EVENT: u32 = 0xF74A;
pub(crate) const OBJ_DESC: u32 = 0xF625;
pub(crate) const PARENT_EVENT: u32 = 0xF749;
pub(crate) const MOVEMENT: u32 = 0xF74C;
pub(crate) const SOUND: u32 = 0xF750;
pub(crate) const CHAT: u32 = 0xF7E0;

pub(crate) fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

pub(crate) fn content() -> MemContent {
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
            weenie(PACK, "Pack", WeenieType::Container)
                .with_int(PropertyInt::ItemsCapacity, 24)
                .with_int(PropertyInt::EncumbranceVal, 50),
        )
        .weenie(
            weenie(GEM, "Gem", WeenieType::Generic)
                .with_int(PropertyInt::EncumbranceVal, 5)
                .with_int(PropertyInt::Value, 7),
        )
        .weenie(
            weenie(COIN, "Pyreal", WeenieType::Coin)
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 10)
                .with_int(PropertyInt::StackUnitEncumbrance, 2)
                .with_int(PropertyInt::StackUnitValue, 1),
        )
        .weenie(
            weenie(SWORD, "Sword", WeenieType::MeleeWeapon)
                .with_int(
                    PropertyInt::ValidLocations,
                    i32::try_from(EquipMask::MeleeWeapon.0).unwrap(),
                )
                .with_int(PropertyInt::DefaultCombatStyle, 1)
                .with_int(PropertyInt::EncumbranceVal, 100),
        )
        .weenie(
            weenie(ANVIL, "Anvil", WeenieType::Generic).with_int(PropertyInt::EncumbranceVal, 5000),
        )
        .weenie(
            weenie(ORB, "Orb", WeenieType::Generic)
                .with_int(PropertyInt::ItemCurMana, 50)
                .with_int(PropertyInt::ItemMaxMana, 200),
        )
}

pub(crate) fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

/// A server on flat land with the synthetic content.
pub(crate) fn server() -> TestServer {
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

pub(crate) fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
    pos: Position,
    strength: u32,
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
    rec.init_level = strength;
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    let character = empyrean_store::models::shard::Character {
        character_options_1: i32::try_from(CharacterOptions1::AllowGive.0).unwrap(),
        ..Default::default()
    };
    o.player.as_mut().expect("a player").player.character = Some(character);
    o.set_location(Some(pos));
    w.objects.insert(o).expect("fresh");

    // `session.SetPlayer(player)` comes before the player enters its landblock (as in
    // `DoPlayerEnterWorld`): tracking the objects it then sees sends through its session.
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

/// A new object of `wcid` on the ground at `pos`.
pub(crate) fn drop_on_ground(w: &mut World, wcid: u32, pos: Position) -> ObjectGuid {
    let g = new_object(w, wcid);
    w.objects.get_mut(g).unwrap().set_location(Some(pos));
    assert!(landblock_manager::add_object(w, g, false));
    g
}

/// A new object of `wcid` in `player`'s main pack.
pub(crate) fn in_pack(w: &mut World, player: u32, wcid: u32) -> ObjectGuid {
    let g = new_object(w, wcid);
    assert!(container::try_add_to_inventory(
        w,
        ObjectGuid::new(player),
        g,
        0,
        false,
        true
    ));
    g
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

pub(crate) fn obj(ts: &TestServer, g: ObjectGuid) -> &WorldObject {
    ts.world.objects.get(g).expect("live object")
}

/// The one `InventoryServerSaveFailed` answer, as `(item, WeenieError)`.
pub(crate) fn save_failed(g: &[Got]) -> (u32, u32) {
    let all: Vec<&Got> = g.iter().filter(|m| m.kind == SAVE_FAILED).collect();
    assert_eq!(all.len(), 1, "one answer in {:04X?}", kinds(g));
    let m: CharacterServerSaysAttemptFailed = all[0].decode();
    (m.object.0, m.reason)
}

/// Moves a player's body and `Location` (what the client's autonomous position updates do).
pub(crate) fn walk_to(ts: &mut TestServer, player: u32, pos: Position) {
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

// ------------------------------------------------------------------ scenarios

/// Picking up an item on the ground (`HandleActionPutItemInContainer`, world to player): the
/// MoveTo, then the pickup motion, then the inventory messages. `CreateMoveToChain` polls
/// `WithinUseRadius` every 0.1 s while the client walks.
#[test]
pub(crate) fn picking_up_an_item_on_the_ground_walks_to_it_then_picks_it_up() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let gem = drop_on_ground(&mut ts.world, GEM, at(26.0, 20.0));
    ts.advance(0.1);

    let (sent, _) = exchange(&mut ts, id, session, 0.5, |ts| {
        ts.send_game_action(
            id,
            &InventoryPutItemInContainer {
                item: ObjectId(gem.full()),
                container: ObjectId(ALPHA),
                slot: 0,
            },
        );
    });
    assert_eq!(
        sent,
        [MOVEMENT],
        "only the MoveToObject motion while out of reach"
    );
    assert!(
        obj(&ts, gem).current_landblock.is_some(),
        "still on the ground"
    );

    // the client arrives (its autonomous positions move the body)
    let (sent, g) = exchange(&mut ts, id, session, 0.5, |ts| {
        walk_to(ts, ALPHA, at(25.4, 20.0))
    });
    assert_eq!(
        sent,
        [
            POSITION,
            MOVEMENT,
            INSTANCE_ID,
            CONTAIN_ID,
            PRIVATE_INT,
            SOUND,
            MOVEMENT,
            PICKUP_EVENT
        ],
        "{sent:04X?}"
    );

    let container_update: QualitiesUpdateInstanceId = first(&g, INSTANCE_ID).decode();
    assert_eq!(
        (container_update.0.object.0, container_update.0.value.0),
        (gem.full(), ALPHA)
    );
    let contain: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
    assert_eq!(
        (contain.item.0, contain.container.0, contain.slot),
        (gem.full(), ALPHA, 0)
    );
    let burden: QualitiesPrivateUpdateInt = first(&g, PRIVATE_INT).decode();
    assert_eq!(
        (burden.0.property_id, burden.0.value),
        (u32::from(PropertyInt::EncumbranceVal.0), 5)
    );
    let sound: EffectsSoundEvent = first(&g, SOUND).decode();
    assert_eq!(
        (sound.id.0, sound.sound_type),
        (ALPHA, i32::try_from(Sound::PickUpItem.0).unwrap())
    );

    assert!(obj(&ts, gem).current_landblock.is_none());
    assert_eq!(
        container::inventory_values(&ts.world, ObjectGuid::new(ALPHA)),
        vec![gem]
    );
    assert_eq!(obj(&ts, ObjectGuid::new(ALPHA)).encumbrance_val(), Some(5));
}

/// A pickup the client never walks to times out after `defaultMoveToTimeout` (15 s) and is
/// answered with `ActionCancelled`.
#[test]
pub(crate) fn a_pickup_never_reached_is_cancelled_and_answered() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let gem = drop_on_ground(&mut ts.world, GEM, at(40.0, 20.0));
    ts.advance(0.1);
    let (sent, _) = exchange(&mut ts, id, session, 14.5, |ts| {
        ts.send_game_action(
            id,
            &InventoryPutItemInContainer {
                item: ObjectId(gem.full()),
                container: ObjectId(ALPHA),
                slot: 0,
            },
        );
    });
    assert_eq!(sent, [MOVEMENT], "still waiting at 14.5 s");
    let (sent, g) = exchange(&mut ts, id, session, 1.0, |_| {});
    assert_eq!(sent, [SAVE_FAILED]);
    assert_eq!(
        save_failed(&g),
        (
            gem.full(),
            u32::try_from(WeenieError::ActionCancelled.0).unwrap()
        )
    );
    assert!(obj(&ts, gem).current_landblock.is_some());
}

/// Dropping an item (`HandleActionDropItem` over `StartPickupChain`): the pickup motion, the item
/// leaves the pack (`TryRemoveFromInventoryWithNetworking(DropItem)`), lands in front of the
/// player (`TryDropItem`), and the client is told.
#[test]
pub(crate) fn dropping_an_item_puts_it_on_the_ground_in_front() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let gem = in_pack(&mut ts.world, ALPHA, GEM);
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(
            id,
            &InventoryDropItem {
                item: ObjectId(gem.full()),
            },
        )
    });
    // TryDropItem's `CurrentLandblock.AddWorldObject(item)` enters the item into physics; the
    // player's ObjectMaint newly sees it and `Player.TrackObject` sends its CreateObject (unit
    // 4.3's port of ACE's `enqueue_obj`). The slide's `item.SendUpdatePosition(true)` then
    // broadcasts to the item's known players (`ObjectMaint.GetKnownPlayersValuesAsPlayer`, real
    // since 4.11), which include the dropper, then `item.EnqueueBroadcastPhysicsState()` (SetState);
    // the direct send after MoveItem follows.
    assert_eq!(
        sent,
        [
            POSITION,
            MOVEMENT,
            INSTANCE_ID,
            PRIVATE_INT,
            CREATE_OBJECT,
            POSITION,
            0xF74B,
            INSTANCE_ID,
            MOVE_ITEM,
            POSITION,
            SOUND,
            MOVEMENT
        ],
        "{sent:04X?}"
    );
    let moved: ItemServerSaysMoveItem = first(&g, MOVE_ITEM).decode();
    assert_eq!(moved.item.0, gem.full());
    let sound: EffectsSoundEvent = first(&g, SOUND).decode();
    assert_eq!(sound.sound_type, i32::try_from(Sound::DropItem.0).unwrap());

    assert!(container::inventory_values(&ts.world, ObjectGuid::new(ALPHA)).is_empty());
    assert_eq!(obj(&ts, ObjectGuid::new(ALPHA)).encumbrance_val(), Some(0));
    assert!(obj(&ts, gem).current_landblock.is_some(), "on the ground");
    let loc = obj(&ts, gem).location().expect("a location");
    let d = ((loc.position_x - 20.0).powi(2) + (loc.position_y - 20.0).powi(2)).sqrt();
    assert!((d - 1.1).abs() < 0.05, "1.1 m in front, got {d}");
}

/// Moving an item between packs: the self-contained branch answers at once.
#[test]
pub(crate) fn moving_an_item_between_packs() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let pack = in_pack(&mut ts.world, ALPHA, PACK);
    let gem = in_pack(&mut ts.world, ALPHA, GEM);
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.1, |ts| {
        ts.send_game_action(
            id,
            &InventoryPutItemInContainer {
                item: ObjectId(gem.full()),
                container: ObjectId(pack.full()),
                slot: 0,
            },
        );
    });
    assert_eq!(sent, [INSTANCE_ID, CONTAIN_ID]);
    let contain: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
    assert_eq!(
        (contain.item.0, contain.container.0, contain.slot),
        (gem.full(), pack.full(), 0)
    );
    assert_eq!(
        obj(&ts, gem).wo.world_object_properties.container,
        Some(pack)
    );
}

/// Splitting a stack in the pack, then merging it back (`HandleActionStackableSplitToContainer`,
/// `HandleActionStackableMerge`, self-contained).
#[test]
pub(crate) fn splitting_and_merging_stacks() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let coins = in_pack(&mut ts.world, ALPHA, COIN);
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.1, |ts| {
        ts.send_game_action(
            id,
            &InventoryStackableSplitToContainer {
                stack: ObjectId(coins.full()),
                container: ObjectId(ALPHA),
                slot: 0,
                amount: 4,
            },
        );
    });
    assert_eq!(sent, [CREATE_OBJECT, CONTAIN_ID, STACK_SIZE]);
    let contain: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
    let new_stack = ObjectGuid::new(contain.item.0);
    let size: ItemUpdateStackSize = first(&g, STACK_SIZE).decode();
    assert_eq!((size.item.0, size.amount), (coins.full(), 6));
    assert_eq!(obj(&ts, new_stack).stack_size(), Some(4));

    let (sent, g) = exchange(&mut ts, id, session, 0.1, |ts| {
        ts.send_game_action(
            id,
            &InventoryStackableMerge {
                merge_from: ObjectId(new_stack.full()),
                merge_to: ObjectId(coins.full()),
                amount: 4,
            },
        );
    });
    assert_eq!(sent, [REMOVE_OBJECT, STACK_SIZE]);
    let size: ItemUpdateStackSize = first(&g, STACK_SIZE).decode();
    assert_eq!((size.item.0, size.amount), (coins.full(), 10));
    assert!(
        ts.world.objects.get(new_stack).is_none(),
        "the merged-away stack is destroyed"
    );
    assert_eq!(obj(&ts, ObjectGuid::new(ALPHA)).encumbrance_val(), Some(20));
}

/// Wielding from the pack (`HandleActionGetAndWieldItem`) and dequipping to the pack.
#[test]
pub(crate) fn wielding_from_inventory_and_dequipping() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let sword = in_pack(&mut ts.world, ALPHA, SWORD);
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.1, |ts| {
        ts.send_game_action(
            id,
            &InventoryGetAndWieldItem {
                item: ObjectId(sword.full()),
                slot: EquipMask::MeleeWeapon.0,
            },
        );
    });
    assert_eq!(
        sent,
        [
            PARENT_EVENT,
            OBJ_DESC,
            INSTANCE_ID,
            PUBLIC_INT,
            WIELD_ITEM,
            SOUND
        ]
    );
    let wield: ItemWearItem = first(&g, WIELD_ITEM).decode();
    assert_eq!(
        (wield.item.0, wield.slot),
        (sword.full(), EquipMask::MeleeWeapon.0)
    );
    assert_eq!(
        ce::get_equipped_main_hand(&ts.world, ObjectGuid::new(ALPHA)),
        Some(sword)
    );

    let (sent, g) = exchange(&mut ts, id, session, 0.1, |ts| {
        ts.send_game_action(
            id,
            &InventoryPutItemInContainer {
                item: ObjectId(sword.full()),
                container: ObjectId(ALPHA),
                slot: 0,
            },
        );
    });
    // TryDequipObjectWithNetworking: TryDequipObjectWithBroadcasting's ObjDescEvent comes first
    assert_eq!(
        sent,
        [
            OBJ_DESC,
            INSTANCE_ID,
            PUBLIC_INT,
            PICKUP_EVENT,
            SOUND,
            INSTANCE_ID,
            CONTAIN_ID
        ]
    );
    let sound: EffectsSoundEvent = first(&g, SOUND).decode();
    assert_eq!(
        sound.sound_type,
        i32::try_from(Sound::UnwieldObject.0).unwrap()
    );
    assert_eq!(
        ce::get_equipped_main_hand(&ts.world, ObjectGuid::new(ALPHA)),
        None
    );
    assert_eq!(
        container::inventory_values(&ts.world, ObjectGuid::new(ALPHA)),
        vec![sword]
    );
}

/// Giving an item to another player (`HandleActionGiveObjectRequest` → `GiveObjectToPlayer`):
/// the giver loses it, the receiver is sent the object, and both are told.
#[test]
pub(crate) fn giving_an_item_to_another_player() {
    let mut ts = server();
    let (alpha, alpha_session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let (bravo, _) = join(&mut ts, "bravo", BRAVO, "Bravo", at(20.5, 20.0), 100);
    let gem = in_pack(&mut ts.world, ALPHA, GEM);
    ts.advance(0.1);
    let nb = ts.received_raw(bravo).len();

    let (sent_a, a) = exchange(&mut ts, alpha, alpha_session, 0.3, |ts| {
        ts.send_game_action(
            alpha,
            &InventoryGiveObjectRequest {
                target: ObjectId(BRAVO),
                item: ObjectId(gem.full()),
                amount: 1,
            },
        );
    });
    // Bravo's share, as the client received it (its queues drain separately)
    let b = got(&ts, bravo, nb);
    // The last is Bravo's `target.EnqueueBroadcast(new GameMessageSound(target.Guid, Sound.ReceiveItem))`,
    // which reaches Alpha standing next to it.
    // First the giver, already in reach, turns to Bravo (`CreateMoveToChain` -> `Rotate`).
    assert_eq!(
        sent_a,
        [
            MOVEMENT,
            REMOVE_OBJECT,
            PRIVATE_INT,
            CONTAIN_ID,
            CHAT,
            DELETE_OBJECT,
            SOUND
        ],
        "giver: {sent_a:04X?}"
    );
    let mut kb = kinds(&b);
    kb.sort_unstable();
    // (and the giver's turn toward Bravo, which Bravo sees)
    let mut want = vec![
        CREATE_OBJECT,
        CONTAIN_ID,
        PRIVATE_INT,
        CHAT,
        SOUND,
        MOVEMENT,
    ];
    want.sort_unstable();
    assert_eq!(kb, want, "receiver");

    let give_text: CommunicationTextboxString = first(&a, CHAT).decode();
    assert_eq!(give_text.text, "You give Bravo Gem.");
    let receive_text: CommunicationTextboxString = first(&b, CHAT).decode();
    assert_eq!(receive_text.text, "Alpha gives you Gem.");
    let contain: ItemServerSaysContainId = first(&b, CONTAIN_ID).decode();
    assert_eq!((contain.item.0, contain.container.0), (gem.full(), BRAVO));
    let sound: EffectsSoundEvent = first(&b, SOUND).decode();
    assert_eq!(
        (sound.id.0, sound.sound_type),
        (BRAVO, i32::try_from(Sound::ReceiveItem.0).unwrap())
    );

    assert_eq!(
        container::inventory_values(&ts.world, ObjectGuid::new(BRAVO)),
        vec![gem]
    );
    assert_eq!(
        (
            obj(&ts, ObjectGuid::new(ALPHA)).encumbrance_val(),
            obj(&ts, ObjectGuid::new(BRAVO)).encumbrance_val()
        ),
        (Some(0), Some(5))
    );
}

/// An over-burdened pickup is refused with ACE's message, and the client is answered.
#[test]
pub(crate) fn an_over_burdened_pickup_is_refused_and_answered() {
    let mut ts = server();
    // strength 10: capacity 1500, so 4500 burden at most; the anvil weighs 5000
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 10);
    let anvil = drop_on_ground(&mut ts.world, ANVIL, at(20.5, 20.0));
    ts.advance(0.1);

    let (sent, g) = exchange(&mut ts, id, session, 0.1, |ts| {
        ts.send_game_action(
            id,
            &InventoryPutItemInContainer {
                item: ObjectId(anvil.full()),
                container: ObjectId(ALPHA),
                slot: 0,
            },
        );
    });
    assert_eq!(sent, [TRANSIENT, SAVE_FAILED]);
    let text: CommunicationTransientString = first(&g, TRANSIENT).decode();
    assert_eq!(text.text, "You are too encumbered to carry that!");
    assert_eq!(save_failed(&g), (anvil.full(), 0));
    assert!(
        obj(&ts, anvil).current_landblock.is_some(),
        "still on the ground"
    );
}

/// Every inventory request naming something that is not there gets exactly one
/// `InventoryServerSaveFailed` for it (the client's inventory lock has no timeout).
#[test]
pub(crate) fn every_refused_request_is_answered() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let ghost = 0x8000_7777;
    type Send = Box<dyn Fn(&mut TestServer)>;
    let requests: Vec<Send> = vec![
        Box::new(move |ts| {
            ts.send_game_action(
                id,
                &InventoryPutItemInContainer {
                    item: ObjectId(ghost),
                    container: ObjectId(ALPHA),
                    slot: 0,
                },
            )
        }),
        Box::new(move |ts| {
            ts.send_game_action(
                id,
                &InventoryDropItem {
                    item: ObjectId(ghost),
                },
            )
        }),
        Box::new(move |ts| {
            ts.send_game_action(
                id,
                &InventoryGetAndWieldItem {
                    item: ObjectId(ghost),
                    slot: EquipMask::MeleeWeapon.0,
                },
            )
        }),
        Box::new(move |ts| {
            ts.send_game_action(
                id,
                &InventoryStackableSplitToContainer {
                    stack: ObjectId(ghost),
                    container: ObjectId(ALPHA),
                    slot: 0,
                    amount: 1,
                },
            )
        }),
        Box::new(move |ts| {
            ts.send_game_action(
                id,
                &InventoryStackableMerge {
                    merge_from: ObjectId(ghost),
                    merge_to: ObjectId(ghost),
                    amount: 1,
                },
            )
        }),
        Box::new(move |ts| {
            ts.send_game_action(
                id,
                &InventoryGiveObjectRequest {
                    target: ObjectId(ALPHA),
                    item: ObjectId(ghost),
                    amount: 1,
                },
            )
        }),
    ];
    for (i, send) in requests.iter().enumerate() {
        let (sent, g) = exchange(&mut ts, id, session, 0.1, |ts| send(ts));
        assert_eq!(sent, [TRANSIENT, SAVE_FAILED], "request {i}");
        assert_eq!(save_failed(&g).0, ghost, "request {i}");
    }
}

pub(crate) const DRUDGE: u32 = 7;
pub(crate) const DRUDGE_GENERATOR: u32 = 8;
pub(crate) const CORPSE: u32 = 9;
pub(crate) const KILL_TASK: &str = "DrudgeKillTask";
pub(crate) const VIEW_CONTENTS: u32 = 0x0196;

/// The synthetic content plus a drudge (attributes, vitals, Run, the monster motion table, a kill
/// task and a Treasure create-list gem), a generator of one drudge, the corpse weenie and the kill
/// task's quest row (10 kills).
pub(crate) fn hunting_content() -> MemContent {
    use empyrean_content::models::world::weenie_properties_attribute::WeeniePropertiesAttribute;
    use empyrean_content::models::world::weenie_properties_attribute_2nd::WeeniePropertiesAttribute2nd;
    use empyrean_content::models::world::weenie_properties_create_list::WeeniePropertiesCreateList;
    use empyrean_content::models::world::weenie_properties_generator::WeeniePropertiesGenerator;
    use empyrean_content::models::world::weenie_properties_skill::WeeniePropertiesSkill;
    use empyrean_content::models::world::Quest;
    use empyrean_entity::enums::{
        DestinationType, PropertyAttribute2nd, PropertyBool, PropertyFloat, Skill,
        SkillAdvancementClass,
    };

    let mut drudge = weenie(DRUDGE, "Drudge", WeenieType::Creature)
        .with_did(PropertyDataId::MotionTable, super::monster_ai::MT)
        .with_bool(PropertyBool::Attackable, true)
        .with_string(PropertyString::KillQuest, KILL_TASK);
    drudge.weenie_properties_attribute = [
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
    drudge.weenie_properties_attribute_2nd = [
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
    drudge.weenie_properties_skill = vec![WeeniePropertiesSkill {
        object_id: DRUDGE,
        r#type: u16::try_from(Skill::Run.0).unwrap(),
        sac: SkillAdvancementClass::Trained.0,
        init_level: 100,
        ..Default::default()
    }];
    // `Treasure` without `Wield`, no shade: `CreateListSelect` always takes each, onto the corpse
    // (two gems: the player takes one, the other goes with the corpse's Destroy).
    let gem_entry = WeeniePropertiesCreateList {
        object_id: DRUDGE,
        destination_type: i8::try_from(DestinationType::Treasure.0).unwrap(),
        weenie_class_id: GEM,
        stack_size: 1,
        ..Default::default()
    };
    drudge.weenie_properties_create_list = vec![
        gem_entry.clone(),
        WeeniePropertiesCreateList { id: 1, ..gem_entry },
    ];

    let mut generator = weenie(DRUDGE_GENERATOR, "Drudge Generator", WeenieType::Generic)
        .with_int(PropertyInt::InitGeneratedObjects, 1)
        .with_int(PropertyInt::MaxGeneratedObjects, 1)
        .with_float(PropertyFloat::RegenerationInterval, 600.0);
    generator.weenie_properties_generator = vec![WeeniePropertiesGenerator {
        id: 1,
        object_id: DRUDGE_GENERATOR,
        probability: 1.0,
        weenie_class_id: DRUDGE,
        init_create: 1,
        max_create: 1,
        ..Default::default()
    }];

    content()
        .weenie(drudge)
        .weenie(generator)
        .weenie(
            weenie(CORPSE, "corpse", WeenieType::Corpse).with_int(PropertyInt::ItemsCapacity, 120),
        )
        .quest(Quest {
            id: 1,
            name: KILL_TASK.to_owned(),
            min_delta: 0,
            max_solves: 10,
            message: Some(String::new()),
            ..Default::default()
        })
}

/// The first live object of `wcid` among the dynamic guids handed out so far.
pub(crate) fn find_by_wcid(w: &World, wcid: u32) -> Option<ObjectGuid> {
    (ObjectGuid::DYNAMIC_MIN..ObjectGuid::DYNAMIC_MIN + 0x40)
        .map(ObjectGuid::new)
        .find(|&g| {
            w.objects
                .get(g)
                .is_some_and(|o| o.biota.weenie_class_id == wcid && !o.wo.world_object.is_destroyed)
        })
}

pub(crate) use crate::support::messages::{exchange, first, got, kinds, Got};

pub(crate) use crate::support::empty_shard::EmptyShard;
