//! Shared virtual-time server fixture and message helpers.

#![allow(unused_imports)]

pub(crate) use std::sync::Arc;

pub(crate) use dereth_primitives::ObjectId;
pub(crate) use dereth_protocol::comms::CommunicationTextboxString;
pub(crate) use dereth_protocol::events::split_ui_blob;
pub(crate) use dereth_protocol::items::{InventoryUseEvent, InventoryUseWithTargetEvent};
pub(crate) use dereth_protocol::objects::ItemUseDone;
pub(crate) use dereth_protocol::trade::{
    BookPageDataResponse, WritingBookAddPage, WritingBookAddPageResponse, WritingBookData,
    WritingBookDeletePage, WritingBookDeletePageResponse, WritingBookModifyPage,
    WritingBookModifyPageResponse, WritingBookOpen, WritingBookPageData,
};
pub(crate) use dereth_protocol::Message;
pub(crate) use empyrean_content::models::world::weenie_properties_book::WeeniePropertiesBook;
pub(crate) use empyrean_content::models::world::Weenie;
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    ItemType, PKLevel, PlayerKillerStatus, PropertyAttribute, PropertyAttribute2nd, PropertyDataId,
    PropertyFloat, PropertyInt, PropertyString, UiEffects, WeenieType,
};
pub(crate) use empyrean_entity::{ObjectGuid, Position};
pub(crate) use empyrean_net::{SessionId, SessionState};
pub(crate) use empyrean_testkit::land::{self, TEST_SETUP};
pub(crate) use empyrean_testkit::{ClientId, TestServer};
pub(crate) use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
pub(crate) use empyrean_world::managers::landblock_manager;
pub(crate) use empyrean_world::world_objects::container;
pub(crate) use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
pub(crate) use empyrean_world::World;

pub(crate) const LB: u32 = 0xA9B4_0000;

pub(crate) const PLAYER_WCID: u32 = 1;
pub(crate) const BOOK: u32 = 2;
pub(crate) const PK_ALTAR: u32 = 3;
pub(crate) const FANE: u32 = 4;
pub(crate) const LIFESTONE: u32 = 5;
pub(crate) const STONE: u32 = 6;
pub(crate) const WAND: u32 = 7;

pub(crate) const ALPHA: u32 = 0x5000_0001;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
pub(crate) const BOOK_OPEN: u32 = 0x00B4;
pub(crate) const BOOK_MODIFY_PAGE_RESPONSE: u32 = 0x00B5;
pub(crate) const BOOK_ADD_PAGE_RESPONSE: u32 = 0x00B6;
pub(crate) const BOOK_DELETE_PAGE_RESPONSE: u32 = 0x00B7;
pub(crate) const BOOK_PAGE_DATA_RESPONSE: u32 = 0x00B8;
pub(crate) const USE_DONE: u32 = 0x01C7;
pub(crate) const PRIVATE_INT: u32 = 0x02CD;
pub(crate) const PUBLIC_INT: u32 = 0x02CE;
pub(crate) const CHAT: u32 = 0xF7E0;

pub(crate) fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

pub(crate) fn int(t: ItemType) -> i32 {
    i32::try_from(t.0).unwrap()
}

pub(crate) fn content() -> MemContent {
    let mut book = weenie(BOOK, "Journal", WeenieType::Book);
    book.weenie_properties_book = Some(WeeniePropertiesBook {
        object_id: BOOK,
        max_num_pages: 2,
        max_num_chars_per_page: 1000,
        ..Default::default()
    });

    MemContent::new()
        .weenie(
            weenie(PLAYER_WCID, "human", WeenieType::Creature).with_did(empyrean_entity::enums::PropertyDataId::CombatTable, 0x3000_0000)
                .with_int(PropertyInt::ItemType, int(ItemType::Creature))
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(book)
        .weenie(
            weenie(PK_ALTAR, "Altar of Bael'Zharon", WeenieType::PKModifier)
                .with_int(PropertyInt::PkLevelModifier, 1)
                .with_float(PropertyFloat::UseRadius, 3.0)
                .with_string(PropertyString::UseMessage, "You feel a harsh dissonance, and you sense that an act of killing is now possible."),
        )
        .weenie(
            weenie(FANE, "Advocate Fane", WeenieType::AdvocateFane)
                .with_float(PropertyFloat::UseRadius, 3.0)
                .with_string(PropertyString::UseMessage, "You are now an Advocate."),
        )
        .weenie(
            weenie(LIFESTONE, "Life Stone", WeenieType::LifeStone)
                .with_float(PropertyFloat::UseRadius, 3.0)
                .with_string(PropertyString::UseMessage, "You have attuned your spirit to this Lifestone."),
        )
        .weenie(
            weenie(STONE, "Mana Stone", WeenieType::ManaStone)
                .with_float(PropertyFloat::ItemEfficiency, 0.5)
                .with_float(PropertyFloat::ManaStoneDestroyChance, 0.0)
                .with_int(PropertyInt::TargetType, int(ItemType::Caster)),
        )
        .weenie(weenie(WAND, "Wand", WeenieType::Caster).with_int(PropertyInt::ItemType, int(ItemType::Caster)))
        .weenie(weenie(42645, "Aetheria Mana Stone", WeenieType::CraftTool).with_int(PropertyInt::TargetType, int(ItemType::Gem)))
        .weenie(weenie(42635, "Coalesced Aetheria", WeenieType::Gem).with_int(PropertyInt::ItemType, int(ItemType::Gem)))
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

/// A client logged in as `account` whose session plays `guid` at `pos`: every attribute 100, the
/// three vitals at 100.
pub(crate) fn join(
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
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
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

/// A new object of `wcid` on the ground at `pos`.
pub(crate) fn on_ground(ts: &mut TestServer, wcid: u32, pos: Position) -> ObjectGuid {
    let g = new_object(&mut ts.world, wcid);
    ts.world.objects.get_mut(g).unwrap().set_location(Some(pos));
    assert!(landblock_manager::add_object(&mut ts.world, g, false));
    g
}

/// A new object of `wcid` in the player's main pack.
pub(crate) fn in_pack(ts: &mut TestServer, wcid: u32) -> ObjectGuid {
    let g = new_object(&mut ts.world, wcid);
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        ObjectGuid::new(ALPHA),
        g,
        0,
        false,
        true
    ));
    g
}

pub(crate) fn obj(ts: &TestServer, g: ObjectGuid) -> &WorldObject {
    ts.world.objects.get(g).expect("live object")
}

pub(crate) fn obj_mut(ts: &mut TestServer, g: ObjectGuid) -> &mut WorldObject {
    ts.world.objects.get_mut(g).expect("live object")
}

/// A received message: its kind (the game-event type inside a 0xF7B0, else the opcode) and blob.
#[derive(Debug, Clone)]
pub(crate) struct Got {
    pub(crate) kind: u32,
    pub(crate) blob: Vec<u8>,
}

impl Got {
    pub(crate) fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }
}

/// `PrivateUpdatePropertyInt(PropertyInt.Age)`: the player's own heartbeat, left out.
pub(crate) fn is_age_update(blob: &[u8]) -> bool {
    blob.len() >= 9
        && u32::from_le_bytes(blob[0..4].try_into().unwrap()) == PRIVATE_INT
        && u32::from_le_bytes(blob[5..9].try_into().unwrap()) == u32::from(PropertyInt::Age.0)
}

/// Runs `act`, advances the server `secs`, and returns what the client received meanwhile.
pub(crate) fn during(
    ts: &mut TestServer,
    id: ClientId,
    secs: f64,
    act: impl FnOnce(&mut TestServer),
) -> Vec<Got> {
    let from = ts.received_raw(id).len();
    act(ts);
    ts.advance(secs);
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

pub(crate) fn kinds(g: &[Got]) -> Vec<u32> {
    g.iter().map(|g| g.kind).collect()
}

pub(crate) fn all(g: &[Got], kind: u32) -> Vec<&Got> {
    g.iter().filter(|m| m.kind == kind).collect()
}

pub(crate) fn first(g: &[Got], kind: u32) -> &Got {
    g.iter()
        .find(|m| m.kind == kind)
        .unwrap_or_else(|| panic!("no 0x{kind:04X} in {:04X?}", kinds(g)))
}

pub(crate) fn chats(g: &[Got]) -> Vec<String> {
    all(g, CHAT)
        .iter()
        .map(|m| m.decode::<CommunicationTextboxString>().text)
        .collect()
}

pub(crate) fn use_dones(g: &[Got]) -> Vec<u32> {
    all(g, USE_DONE)
        .iter()
        .map(|m| m.decode::<ItemUseDone>().failure_type)
        .collect()
}

pub(crate) fn pages(
    ts: &TestServer,
    book: ObjectGuid,
) -> Vec<(u32, Option<String>, Option<String>)> {
    obj(ts, book)
        .biota
        .properties_book_page_data
        .as_ref()
        .expect("a book's pages")
        .iter()
        .map(|p| (p.author_id, p.author_name.clone(), p.page_text.clone()))
        .collect()
}

// ------------------------------------------------------------------ scenarios
