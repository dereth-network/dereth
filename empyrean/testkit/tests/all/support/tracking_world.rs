//! Shared virtual-time server fixture and message helpers.

#![allow(unused_imports)]

pub(crate) use std::sync::Arc;

pub(crate) use dereth_primitives::ObjectId;
pub(crate) use dereth_protocol::comms::{
    CharacterConfirmationRequest, CharacterConfirmationResponse, CommunicationTextboxString,
    CommunicationWeenieErrorWithString,
};
pub(crate) use dereth_protocol::events::split_ui_blob;
pub(crate) use dereth_protocol::items::InventoryPutItemInContainer;
pub(crate) use dereth_protocol::objects::{
    EffectsPlayScriptType, InventoryPickupEvent, ItemDeleteObject,
};
pub(crate) use dereth_protocol::qualities::{
    QualitiesPrivateUpdateAttribute, QualitiesPrivateUpdateInt, QualitiesPrivateUpdateInt64,
};
pub(crate) use dereth_protocol::Message;
pub(crate) use empyrean_content::models::world::weenie_properties_attribute::WeeniePropertiesAttribute;
pub(crate) use empyrean_content::models::world::Weenie;
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    ConfirmationType, EquipMask, PropertyAttribute, PropertyDataId, PropertyInt, PropertyInt64,
    PropertyString, WeenieErrorWithString, WeenieType,
};
pub(crate) use empyrean_entity::{ObjectGuid, Position};
pub(crate) use empyrean_net::{SessionId, SessionState};
pub(crate) use empyrean_testkit::land::{self, TEST_SETUP};
pub(crate) use empyrean_testkit::{ClientId, TestServer};
pub(crate) use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
pub(crate) use empyrean_world::managers::landblock_manager;
pub(crate) use empyrean_world::physics::phys_ext;
pub(crate) use empyrean_world::world_objects::world_object::{self, CtorEnv};
pub(crate) use empyrean_world::world_objects::{container, creature_equipment as ce};
pub(crate) use empyrean_world::World;

pub(crate) const LB: u32 = 0xA9B4_0000;

pub(crate) const PLAYER_WCID: u32 = 1;
pub(crate) const GEM: u32 = 3;
pub(crate) const SWORD: u32 = 5;
pub(crate) const DRUDGE: u32 = 7;
pub(crate) const STRENGTH_GEM: u32 = 8;
pub(crate) const TRANSFER_DEVICE: u32 = 9;

pub(crate) const ALPHA: u32 = 0x5000_0001;
pub(crate) const BRAVO: u32 = 0x5000_0002;

pub(crate) const DELETE_OBJECT: u32 = 0xF747;
pub(crate) const PICKUP_EVENT: u32 = 0xF74A;
pub(crate) const CONFIRM: u32 = 0x0274;
pub(crate) const PRIVATE_ATTRIBUTE: u32 = 0x02E3;
pub(crate) const PRIVATE_INT: u32 = 0x02CD;
pub(crate) const PRIVATE_INT64: u32 = 0x02CF;
pub(crate) const ERROR_WITH_STRING: u32 = 0x028B;
pub(crate) const PLAY_EFFECT: u32 = 0xF755;
pub(crate) const CHAT: u32 = 0xF7E0;

pub(crate) fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

/// A level 1 human with 5,000 unassigned experience, innate Strength 50 and the other attributes 60.
pub(crate) fn player_weenie() -> Weenie {
    let mut d = weenie(PLAYER_WCID, "human", WeenieType::Creature)
        .with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        )
        .with_int(PropertyInt::ItemsCapacity, 102)
        .with_int(PropertyInt::ContainersCapacity, 7)
        .with_int(PropertyInt::Level, 1)
        .with_int64(PropertyInt64::TotalExperience, 5000)
        .with_int64(PropertyInt64::AvailableExperience, 5000);
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
        object_id: PLAYER_WCID,
        r#type: a.0,
        init_level: if a == PropertyAttribute::Strength {
            50
        } else {
            60
        },
        ..Default::default()
    })
    .collect();
    d
}

pub(crate) fn content() -> MemContent {
    MemContent::new()
        .weenie(player_weenie())
        .weenie(
            weenie(GEM, "Gem", WeenieType::Generic)
                .with_int(PropertyInt::EncumbranceVal, 5)
                .with_int(PropertyInt::Value, 7),
        )
        .weenie(
            weenie(SWORD, "Sword", WeenieType::MeleeWeapon)
                .with_int(
                    PropertyInt::ValidLocations,
                    i32::try_from(EquipMask::MeleeWeapon.0).unwrap(),
                )
                .with_int(PropertyInt::EncumbranceVal, 100),
        )
        .weenie(weenie(DRUDGE, "Drudge", WeenieType::Creature))
        .weenie(
            weenie(
                STRENGTH_GEM,
                "Might of the Seventh Mule",
                WeenieType::AugmentationDevice,
            )
            .with_int(PropertyInt::AugmentationStat, 1) // AugmentationType.Strength
            .with_int64(PropertyInt64::AugmentationCost, 1000)
            .with_int(PropertyInt::EncumbranceVal, 5),
        )
        .weenie(
            weenie(
                TRANSFER_DEVICE,
                "Endurance to Strength Transferal",
                WeenieType::AttributeTransferDevice,
            )
            .with_int(
                PropertyInt::TransferFromAttribute,
                i32::from(PropertyAttribute::Endurance.0),
            )
            .with_int(
                PropertyInt::TransferToAttribute,
                i32::from(PropertyAttribute::Strength.0),
            )
            .with_int(PropertyInt::EncumbranceVal, 5),
        )
}

pub(crate) use empyrean_testkit::EmptyShard;

pub(crate) fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

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

/// A client whose session plays `guid`, a player standing at `pos` (as `DoPlayerEnterWorld` binds
/// it: the session's player is set before it joins its landblock).
pub(crate) fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
    pos: Position,
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
    w.objects.insert(o).expect("fresh");
    // `PlayerManager.AddPlayerToOnlinePlayers`: the Confirmation targets find the player there
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

/// A new object of `wcid` placed at `pos` in its landblock.
pub(crate) fn on_ground(w: &mut World, wcid: u32, pos: Position) -> ObjectGuid {
    let g = new_object(w, wcid);
    w.objects.get_mut(g).unwrap().set_location(Some(pos));
    assert!(landblock_manager::add_object(w, g, false));
    g
}

/// Every message `id` received from index `from` on, as (kind, whole blob).
pub(crate) fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<(u32, Vec<u8>)> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            (m.opcode, blob)
        })
        .collect()
}

pub(crate) fn decode<M: Message>(blob: &[u8]) -> M {
    let split = split_ui_blob(blob).expect("a blob");
    let mut body = split.body;
    M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", M::OPCODE.0))
}

/// The ids of the DeleteObjects in `g`, in arrival order.
pub(crate) fn deleted(g: &[(u32, Vec<u8>)]) -> Vec<u32> {
    g.iter()
        .filter(|(k, _)| *k == DELETE_OBJECT)
        .map(|(_, b)| decode::<ItemDeleteObject>(b).id.0)
        .collect()
}

/// Whether the player's `ObjectMaint` knows `g`.
pub(crate) fn knows(w: &World, player: u32, g: ObjectGuid) -> bool {
    let me = phys_ext::physics_obj(w, ObjectGuid::new(player)).expect("a body");
    let it = phys_ext::physics_obj(w, g).expect("a body");
    empyrean_world::physics::object_maint::known_objects_contains_value(w, me, it)
}

/// A new `wcid` in `player`'s main pack.
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

pub(crate) fn innate(w: &World, player: u32, a: PropertyAttribute) -> u32 {
    let o = w.objects.get(ObjectGuid::new(player)).expect("the player");
    o.attributes()
        .get(&a)
        .expect("the attribute")
        .starting_value(o)
}

/// The one message of `kind` in `g`, decoded.
pub(crate) fn one<M: Message>(g: &[(u32, Vec<u8>)], kind: u32) -> M {
    let found: Vec<&(u32, Vec<u8>)> = g.iter().filter(|(k, _)| *k == kind).collect();
    assert_eq!(
        found.len(),
        1,
        "one 0x{kind:04X} in {:04X?}",
        g.iter().map(|m| m.0).collect::<Vec<_>>()
    );
    decode(&found[0].1)
}

/// `(kind, blob)` with a game event's type as its kind.
pub(crate) fn events(g: Vec<(u32, Vec<u8>)>) -> Vec<(u32, Vec<u8>)> {
    g.into_iter()
        .map(|(k, b)| {
            if k == 0xF7B0 {
                (u32::from_le_bytes(b[12..16].try_into().unwrap()), b)
            } else {
                (k, b)
            }
        })
        .collect()
}

/// Uses `device` (its `ActOnUse`), then answers the question yes over the wire; returns the
/// question and what the answer brought.
pub(crate) fn use_and_confirm(
    ts: &mut TestServer,
    id: ClientId,
    device: ObjectGuid,
) -> (CharacterConfirmationRequest, Vec<(u32, Vec<u8>)>) {
    let n = ts.received_raw(id).len();
    empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, device, ObjectGuid::new(ALPHA));
    ts.advance(0.2);
    let asked = events(got(ts, id, n));
    let ask: CharacterConfirmationRequest = one(&asked, CONFIRM);

    let n = ts.received_raw(id).len();
    ts.send_game_action(
        id,
        &CharacterConfirmationResponse {
            confirmation_type: ask.confirmation_type,
            context_id: ask.context_id,
            accepted: 1,
        },
    );
    ts.advance(0.2);
    (ask, events(got(ts, id, n)))
}

// ------------------------------------------------------------------ cloak and teleport visibility
