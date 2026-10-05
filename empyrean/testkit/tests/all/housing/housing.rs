//! ACE: Source/ACE.Server/Managers/HouseManager.cs::HouseManager
//! Buy, guest access, hook item, rent paid then lapsed and evicted, relog; deleted guest dropped
//! from access list; barrier stops strangers; alt recalls to account house; house select.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::CommunicationTextboxString;
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::items::InventoryPutItemInContainer;
use dereth_protocol::login::{
    LoginExecuteLogOff, LoginExecuteLogOffRequest, LoginSendEnterWorld, LoginSendEnterWorldRequest,
};
use dereth_protocol::trade::{
    HouseAddPermanentGuest, HouseBuyHouse, HouseDataMessage, HouseProfileMessage, HouseRentHouse,
    HouseUpdateRestrictions,
};
use dereth_protocol::Message;
use empyrean_content::models::world::{
    LandblockInstance, LandblockInstanceLink, Weenie, WeeniePropertiesCreateList,
};
use empyrean_content::MemContent;
use empyrean_dat::fake::sample;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AccessLevel, DestinationType, HouseType, ItemType, PositionType, PropertyDataId, PropertyFloat,
    PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::{Biota, ObjectGuid};
use empyrean_net::SessionState;
use empyrean_testkit::land;
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::world_manager::WorldStatusState;
use empyrean_world::managers::{house_manager, player_manager};
use empyrean_world::physics::phys_ext;
use empyrean_world::physics::weenie_object::WeenieObject;
use empyrean_world::world_objects::world_object::CtorEnv;
use empyrean_world::world_objects::{container, house, player_house};
use empyrean_world::World;

const PLAYER_SETUP: u32 = 0x0200_0001;
const START: u16 = 0xA9B4;
const HEIGHT: u8 = 47;
const CELL: u32 = 0xA9B4_0019;

// characters (synthetic names)
const OWNER: u32 = 0x5000_0001;
const GUEST: u32 = 0x5000_0002;
const STRANGER: u32 = 0x5000_0003;
// a second character on the owner's account
const ALT: u32 = 0x5000_0004;

// weenies
const HUMAN: u32 = 1;
const COTTAGE: u32 = 20;
const SLUMLORD: u32 = 21;
const HOOK: u32 = 22;
const STORAGE: u32 = 23;
const BOOTSPOT: u32 = 24;
const CANDLESTICK: u32 = 25;
const COINSTACK: u32 = 273;
const DEED: u32 = 9549;

// world instances of the cottage and its links
const HOUSE_GUID: u32 = 0x7A9B_4001;
const SLUMLORD_GUID: u32 = 0x7A9B_4002;
const HOOK_GUID: u32 = 0x7A9B_4003;
const STORAGE_GUID: u32 = 0x7A9B_4004;
const BOOTSPOT_GUID: u32 = 0x7A9B_4005;

const HOUSE_ID: u32 = 1234;
const BUY_PRICE: i32 = 100;
const RENT_PRICE: i32 = 50;

// game-event kinds (inside 0xF7B0) and opcodes
const HOUSE_PROFILE: u32 = 0x021D;
const HOUSE_DATA: u32 = 0x0225;
const HOUSE_STATUS: u32 = 0x0226;
const UPDATE_RESTRICTIONS: u32 = 0x0248;

use empyrean_testkit::EmptyShard;

fn thing(wcid: u32, class_name: &str, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, class_name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

fn content() -> MemContent {
    let row = |id: u32, destination: DestinationType, wcid: u32, stack_size: i32| {
        WeeniePropertiesCreateList {
            id,
            object_id: SLUMLORD,
            destination_type: i8::try_from(destination.0).unwrap(),
            weenie_class_id: wcid,
            stack_size,
            ..Default::default()
        }
    };
    let mut slumlord = thing(
        SLUMLORD,
        "slumlordcottage1234",
        "Cottage",
        WeenieType::SlumLord,
    )
    .with_did(PropertyDataId::HouseId, HOUSE_ID);
    slumlord.weenie_properties_create_list = vec![
        row(1, DestinationType::HouseBuy, COINSTACK, BUY_PRICE),
        row(2, DestinationType::HouseRent, COINSTACK, RENT_PRICE),
    ];

    MemContent::new()
        .weenie(
            Weenie::new(HUMAN, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7)
                .with_float(PropertyFloat::HeartbeatInterval, 5.0),
        )
        .weenie(
            thing(COTTAGE, "housecottage1234", "Cottage", WeenieType::House)
                .with_int(PropertyInt::HouseType, HouseType::Cottage.0)
                .with_did(PropertyDataId::HouseId, HOUSE_ID)
                .with_did(
                    PropertyDataId::RestrictionEffect,
                    empyrean_entity::enums::PlayScript::RestrictionEffectBlue.0,
                ),
        )
        .weenie(slumlord)
        .weenie(
            thing(HOOK, "hook", "Floor Hook", WeenieType::Hook)
                .with_int(PropertyInt::ItemsCapacity, 1)
                .with_int(PropertyInt::HookType, 1)
                .with_float(PropertyFloat::UseRadius, 5.0),
        )
        .weenie(
            thing(STORAGE, "storage", "Chest", WeenieType::Storage)
                .with_int(PropertyInt::ItemsCapacity, 24),
        )
        .weenie(thing(
            BOOTSPOT,
            "bootspot",
            "Boot Spot",
            WeenieType::BootSpot,
        ))
        .weenie(
            thing(
                CANDLESTICK,
                "candlestick",
                "Candlestick",
                WeenieType::Generic,
            )
            .with_int(PropertyInt::ItemType, 0x80),
        )
        .weenie(
            thing(DEED, "deed", "Deed", WeenieType::Generic).with_int(PropertyInt::ItemType, 0x80),
        )
        .weenie(
            thing(COINSTACK, "coinstack", "Pyreal", WeenieType::Coin)
                .with_string(PropertyString::PluralName, "Pyreals")
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::Money.0).unwrap(),
                )
                .with_int(PropertyInt::MaxStackSize, 25000)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::StackUnitValue, 1)
                .with_int(PropertyInt::Value, 1),
        )
        .landblock_instance(house_instance())
        .landblock_instance(child(SLUMLORD_GUID, SLUMLORD, [82.0, 12.0]))
        .landblock_instance(child(HOOK_GUID, HOOK, [84.0, 12.0]))
        .landblock_instance(child(STORAGE_GUID, STORAGE, [86.0, 12.0]))
        .landblock_instance(child(BOOTSPOT_GUID, BOOTSPOT, [60.0, 60.0]))
}

fn house_instance() -> LandblockInstance {
    let mut h = LandblockInstance::new(HOUSE_GUID, COTTAGE, CELL, [80.0, 10.0, 94.0]);
    for (i, c) in [SLUMLORD_GUID, HOOK_GUID, STORAGE_GUID, BOOTSPOT_GUID]
        .into_iter()
        .enumerate()
    {
        h.landblock_instance_link.push(LandblockInstanceLink {
            id: u32::try_from(i).unwrap() + 1,
            parent_guid: HOUSE_GUID,
            child_guid: c,
            ..Default::default()
        });
    }
    h
}

fn child(guid: u32, wcid: u32, xy: [f32; 2]) -> LandblockInstance {
    let cell = if xy[0] > 72.0 { CELL } else { 0xA9B4_0013 };
    LandblockInstance {
        is_link_child: true,
        ..LandblockInstance::new(guid, wcid, cell, [xy[0], xy[1], 94.0])
    }
}

fn blocks() -> Vec<u16> {
    let mut v = Vec::new();
    for dx in [-1i32, 0, 1] {
        for dy in [-1i32, 0, 1] {
            let x = u16::try_from(i32::from(START >> 8) + dx).expect("on the map");
            let y = u16::try_from(i32::from(START & 0xFF) + dy).expect("on the map");
            v.push(x << 8 | y);
        }
    }
    v
}

fn pos(x: f32, y: f32) -> empyrean_entity::models::PropertiesPosition {
    empyrean_entity::models::PropertiesPosition {
        obj_cell_id: CELL,
        position_x: x,
        position_y: y,
        position_z: 94.005,
        rotation_w: 1.0,
        ..Default::default()
    }
}

fn give(ts: &mut TestServer, player: u32, wcid: u32, stack_size: Option<i32>) -> ObjectGuid {
    let w = &mut ts.world;
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let g = guid_manager::new_dynamic_guid(w);
    let mut o = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(env, Some(weenie), g)
    })
    .expect("constructible");
    if stack_size.is_some() {
        o.set_stack_size(stack_size);
    }
    w.objects.insert(o).expect("fresh");
    assert!(container::try_add_to_inventory(
        w,
        guid(player),
        g,
        0,
        false,
        true
    ));
    empyrean_world::world_objects::player_commerce::update_coin_value(w, guid(player), false);
    g
}

/// Seeds a character (15-day account flag set, so purchase is allowed) on its own account.
fn seed(w: &mut World, account: &str, guid: u32, name: &str, x: f32, possessions: &mut [Biota]) {
    let account_id = w
        .auth
        .lock()
        .create_account(
            account,
            "pw",
            AccessLevel::Player,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    seed_on(w, account_id, guid, name, x, possessions);
}

/// Seeds a character on an existing account.
fn seed_on(
    w: &mut World,
    account_id: u32,
    guid: u32,
    name: &str,
    x: f32,
    possessions: &mut [Biota],
) {
    let mut biota = Biota {
        id: guid,
        weenie_class_id: HUMAN,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, name.to_owned());
    biota.set_property(PropertyDataId::Setup, PLAYER_SETUP);
    biota.set_property(PropertyInt::HeritageGroup, 1);
    biota.set_property(PropertyInt::Level, 20);
    biota.set_property(PropertyInt::ItemsCapacity, 102);
    biota.set_property(PropertyInt::ContainersCapacity, 7);
    biota.set_property(PropertyFloat::HeartbeatInterval, 5.0);
    biota.set_property(empyrean_entity::enums::PropertyBool::Account15Days, true);
    biota.set_property_position(PositionType::Location, pos(x, 10.0));
    biota.properties_enchantment_registry = Some(Vec::new());
    let character = empyrean_store::models::shard::Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Default::default()
    };
    assert!(w
        .shard
        .base_database()
        .add_character_in_parallel(&mut biota, possessions, &character));
}

fn server() -> TestServer {
    server_with(false)
}

/// The server, with `alt` a second character (`ALT`) on the owner's account.
fn server_with(alt: bool) -> TestServer {
    let setup = move |w: &mut World| {
        seed(w, "owneracct", OWNER, "Owner", 83.0, &mut []);
        if alt {
            let account_id = w
                .auth
                .lock()
                .get_account_by_name("owneracct")
                .expect("the owner's account")
                .account_id;
            seed_on(w, account_id, ALT, "Alt", 90.0, &mut []);
        }
        seed(w, "guestacct", GUEST, "Guest", 85.0, &mut []);
        seed(w, "strangeracct", STRANGER, "Stranger", 87.0, &mut []);
    };
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_skill_table(sample::skill_table())
        .build()
        .expect("fake dats");
    let mut ts = TestServer::with_setup(dats, setup);
    ts.world.content = Arc::new(content());
    ts.world.world_manager.world_status = WorldStatusState::Open;
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    land::use_flat_land_with_setup(
        &mut ts.world,
        &blocks(),
        HEIGHT,
        PLAYER_SETUP,
        land::test_setup_geometry(),
    );
    phys_ext::register_setup(&mut ts.world, land::TEST_SETUP, land::test_setup_geometry());
    // `HouseManager.Initialize()` again, over this test's content (TestServer ran it over none)
    house_manager::initialize(&mut ts.world);

    ts
}

fn login(ts: &mut TestServer, account: &str, guid: u32) -> ClientId {
    let before: Vec<_> = ts.world.sessions.iter().map(|(s, _)| s).collect();
    let id = ts.connect(account, "pw");
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(s, _)| s)
        .find(|s| !before.contains(s))
        .expect("session");
    assert!(ts.run_until(1.0, |ts| ts
        .world
        .sessions
        .get(session)
        .is_some_and(|s| s.state == SessionState::AuthConnected)));
    enter(ts, id, account, guid);
    id
}

fn enter(ts: &mut TestServer, id: ClientId, account: &str, guid: u32) {
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
    ts.advance(0.5);
    assert!(
        player_manager::get_online_player(&ts.world, guid).is_some(),
        "in the world"
    );
}

/// A received game event of `kind` (inside `0xF7B0`), decoded.
fn events<M: Message>(ts: &TestServer, id: ClientId, from: usize, kind: u32) -> Vec<M> {
    ts.received_raw(id)[from..]
        .iter()
        .filter(|m| {
            m.opcode == 0xF7B0 && u32::from_le_bytes(m.body[8..12].try_into().unwrap()) == kind
        })
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let split = split_ui_blob(&blob).expect("a blob");
            let mut body = split.body;
            M::read(&mut body).unwrap_or_else(|e| panic!("0x{kind:04X} decodes: {e:?}"))
        })
        .collect()
}

/// The chat lines the client received since `from`.
fn chat(ts: &TestServer, id: ClientId, from: usize) -> Vec<String> {
    ts.received_raw(id)[from..]
        .iter()
        .filter(|m| m.opcode == CommunicationTextboxString::OPCODE.0)
        .map(|m| {
            dereth_protocol::read_body_padded::<CommunicationTextboxString>(&m.body)
                .expect("decodes")
                .text
        })
        .collect()
}

fn mark(ts: &TestServer, id: ClientId) -> usize {
    ts.received_raw(id).len()
}

fn guid(g: u32) -> ObjectGuid {
    ObjectGuid::new(g)
}

/// The owner's purchase and rent-due times set back, and the rent queue entry rebuilt from them
/// (`RemoveRentQueue`, then `AddRentQueue` as `SetHouseOwner` does).
fn backdate(ts: &mut TestServer, purchase: i32, rent_due: i32) {
    let w = &mut ts.world;
    let p = w.objects.get_mut(guid(OWNER)).unwrap();
    p.set_house_purchase_timestamp(Some(purchase));
    p.set_house_rent_timestamp(Some(rent_due));
    house_manager::remove_rent_queue(w, HOUSE_GUID);
    house_manager::decrement_total_owned_housing_by_type(w, HouseType::Cottage);
    house_manager::add_rent_queue(
        w,
        empyrean_world::entity::i_player::IPlayer::Online(guid(OWNER)),
        HOUSE_GUID,
    );
}

fn can_move_into(ts: &TestServer, mover: u32) -> bool {
    let barrier = WeenieObject::new(&ts.world, guid(HOUSE_GUID));
    let mover = WeenieObject::new(&ts.world, guid(mover));
    barrier.can_move_into(&ts.world, &mover)
}

/// The whole cycle: buy, a guest and the barrier, a hooked item, the rent paid, a relog, the rent
/// timer (paid, then lapsed: eviction).
#[test]
fn buy_guest_hook_rent_relog_and_eviction() {
    let mut ts = server();
    let owner = login(&mut ts, "owneracct", OWNER);
    let guest = login(&mut ts, "guestacct", GUEST);
    let stranger = login(&mut ts, "strangeracct", STRANGER);
    ts.advance(1.0);
    let coins = give(&mut ts, OWNER, COINSTACK, Some(500));
    let candle = give(&mut ts, OWNER, CANDLESTICK, None);

    // the cottage loaded with its landblock, linked
    let w = &ts.world;
    assert!(
        w.objects
            .get(guid(HOUSE_GUID))
            .is_some_and(|o| o.is_house()),
        "the house is in the world"
    );
    assert_eq!(
        house::slum_lord(w, guid(HOUSE_GUID)),
        Some(guid(SLUMLORD_GUID))
    );
    assert_eq!(house::hooks(w, guid(HOUSE_GUID)), vec![guid(HOOK_GUID)]);
    assert_eq!(
        house::storage(w, guid(HOUSE_GUID)),
        vec![guid(STORAGE_GUID)]
    );
    assert_eq!(
        house::boot_spot(w, guid(HOUSE_GUID)),
        Some(guid(BOOTSPOT_GUID))
    );
    assert_eq!(w.objects.get(guid(HOUSE_GUID)).unwrap().house_owner(), None);
    // nobody owns it: the barrier lets anyone in
    assert!(can_move_into(&ts, STRANGER));

    // ---- buy: 100 of the 500 pyreals
    let from = mark(&ts, owner);
    ts.send_game_action(
        owner,
        &HouseBuyHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(4.0);
    let w = &ts.world;
    assert_eq!(
        w.objects.get(coins).unwrap().stack_size(),
        Some(400),
        "the pyreals leave"
    );
    let house_o = w.objects.get(guid(HOUSE_GUID)).unwrap();
    assert_eq!(
        house_o.house_owner(),
        Some(OWNER),
        "ownership shows on the house"
    );
    assert_eq!(house_o.house_owner_name().as_deref(), Some("Owner"));
    assert_eq!(
        w.objects.get(guid(SLUMLORD_GUID)).unwrap().house_owner(),
        Some(OWNER),
        "and on its links"
    );
    assert_eq!(
        w.objects
            .get(guid(SLUMLORD_GUID))
            .unwrap()
            .get_property(PropertyString::Name)
            .as_deref(),
        Some("Owner's Cottage")
    );
    let p = w.objects.get(guid(OWNER)).unwrap();
    assert_eq!(
        (p.house_instance(), p.house_id()),
        (Some(HOUSE_GUID), Some(HOUSE_ID))
    );
    assert!(
        container::get_inventory_items_of_wcid(w, guid(OWNER), DEED).len() == 1,
        "the deed arrives"
    );
    let lines = chat(&ts, owner, from);
    assert!(
        lines.contains(&"Congratulations!  You now own this dwelling.".to_owned()),
        "{lines:?}"
    );
    let data: Vec<HouseDataMessage> = events(&ts, owner, from, HOUSE_DATA);
    assert_eq!(data.len(), 1, "the house data 3 s later");
    assert_eq!(
        data[0].house_type,
        u32::try_from(HouseType::Cottage.0).unwrap()
    );
    assert_eq!(
        (data[0].buy.len(), data[0].buy[0].num, data[0].buy[0].paid),
        (1, BUY_PRICE, BUY_PRICE),
        "bought: paid in full"
    );
    assert_eq!((data[0].rent[0].num, data[0].rent[0].paid), (RENT_PRICE, 0));
    let profile: Vec<HouseProfileMessage> = events(&ts, owner, from, HOUSE_PROFILE);
    assert_eq!(
        profile
            .last()
            .expect("the slumlord's profile")
            .profile
            .owner
            .0,
        OWNER
    );
    assert_eq!(profile.last().unwrap().profile.name, "Owner");
    // the rent queue holds it, due 30 days after the purchase
    assert_eq!(house_manager::total_owned_housing(&ts.world), 1);

    // ---- a guest, and the barrier
    assert!(
        !can_move_into(&ts, GUEST),
        "an owned, closed house fences a non-guest"
    );
    let (gfrom, sfrom) = (mark(&ts, guest), mark(&ts, stranger));
    let from = mark(&ts, owner);
    ts.send_game_action(
        owner,
        &HouseAddPermanentGuest {
            name: "Guest".to_owned(),
        },
    );
    ts.advance(0.5);
    assert!(chat(&ts, owner, from).contains(&"Guest has been added to your guest list.".to_owned()));
    assert!(chat(&ts, guest, gfrom)
        .contains(&"Owner has added you to their house guest list.".to_owned()));
    assert!(can_move_into(&ts, GUEST), "the guest passes the barrier");
    assert!(!can_move_into(&ts, STRANGER), "a stranger doesn't");
    assert!(can_move_into(&ts, OWNER));
    // the clients near the house are told: the guest is in the table, the stranger is not
    let r: Vec<HouseUpdateRestrictions> = events(&ts, stranger, sfrom, UPDATE_RESTRICTIONS);
    let table: Vec<u32> = r
        .last()
        .expect("the restrictions update")
        .restrictions
        .table
        .entries
        .iter()
        .map(|(k, _)| k.0)
        .collect();
    assert_eq!(table, vec![GUEST]);
    assert_eq!(r.last().unwrap().sender.0, HOUSE_GUID);

    // ---- @adminhouse dump of the selected house (AdminCommands.DumpHouse, AppendHouseLinkDump)
    let from = mark(&ts, owner);
    ts.world
        .objects
        .get_mut(guid(OWNER))
        .unwrap()
        .set_current_appraisal_target(Some(HOUSE_GUID));
    let session = player_manager::player_session(&ts.world, guid(OWNER)).expect("a session");
    empyrean_command::handlers::admin_commands::handle_adminhouse(
        &mut ts.world,
        Some(session),
        &["dump".to_owned()],
    );
    ts.advance(0.5);
    let dump = chat(&ts, owner, from).join("");
    for line in [
        "House Dump for Cottage (0x7A9B4001)\n",
        "HouseType: Cottage (1)\n",
        "OwnerID: 0x50000001 | OwnerName: Owner\n",
        "100 Pyreals (WCID: 273)\n",
        "50 Pyreals (WCID: 273) | Paid: 0\n",
        "===Hooks for House 0x7A9B4001==================\n",
        "===BootSpot for House 0x7A9B4001===============\n",
        "Guest (0x50000002)\n",
        "HouseOwner: Owner (0x50000001)\n",
    ] {
        assert!(dump.contains(line), "the dump has {line:?}: {dump}");
    }
    ts.world
        .objects
        .get_mut(guid(OWNER))
        .unwrap()
        .set_current_appraisal_target(None);

    // ---- hang the candlestick on the hook
    let usable = ts
        .world
        .objects
        .get(guid(HOUSE_GUID))
        .unwrap()
        .house_current_hooks_usable();
    let ok = empyrean_world::dispatch::check_use_requirements::check_use_requirements(
        &mut ts.world,
        guid(HOOK_GUID),
        guid(OWNER),
    );
    assert!(ok.success, "the owner may use the hook");
    let refused = empyrean_world::dispatch::check_use_requirements::check_use_requirements(
        &mut ts.world,
        guid(HOOK_GUID),
        guid(STRANGER),
    );
    assert!(
        !refused.success && refused.message.is_some(),
        "a stranger may not"
    );
    empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, guid(HOOK_GUID), guid(OWNER));
    ts.send_game_action(
        owner,
        &InventoryPutItemInContainer {
            item: ObjectId(candle.full()),
            container: ObjectId(HOOK_GUID),
            slot: 0,
        },
    );
    assert!(
        ts.run_until(5.0, |ts| container::inventory_values(
            &ts.world,
            guid(HOOK_GUID)
        )
        .contains(&candle)),
        "the candlestick hangs"
    );
    let w = &ts.world;
    assert_eq!(
        w.objects
            .get(guid(HOOK_GUID))
            .unwrap()
            .get_property(PropertyString::Name)
            .as_deref(),
        Some("Candlestick"),
        "the hook takes the item's look"
    );
    assert_eq!(
        w.objects
            .get(guid(HOUSE_GUID))
            .unwrap()
            .house_current_hooks_usable(),
        usable - 1
    );

    // ---- pay the rent: 50 split off the 400
    let from = mark(&ts, owner);
    ts.send_game_action(
        owner,
        &HouseRentHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(0.5);
    assert_eq!(ts.world.objects.get(coins).unwrap().stack_size(), Some(350));
    let paid: i32 = container::inventory_values(&ts.world, guid(SLUMLORD_GUID))
        .iter()
        .map(|&g| ts.world.objects.get(g).unwrap().stack_size().unwrap_or(1))
        .sum();
    assert_eq!(paid, RENT_PRICE, "the slumlord holds the rent");
    assert!(
        chat(&ts, owner, from).contains(&"Maintenance paid.".to_owned()),
        "{:?}",
        chat(&ts, owner, from)
    );
    let data: Vec<HouseDataMessage> = events(&ts, owner, from, HOUSE_DATA);
    assert_eq!(
        (
            data.last().unwrap().rent[0].num,
            data.last().unwrap().rent[0].paid
        ),
        (RENT_PRICE, RENT_PRICE)
    );

    // ---- log out and back in: the ownership is kept (MemShard)
    ts.send_message(
        owner,
        NetQueue::Logon,
        &LoginExecuteLogOffRequest {
            character: ObjectId(OWNER),
        },
    );
    assert!(
        ts.run_until(10.0, |ts| !ts
            .received::<LoginExecuteLogOff>(owner)
            .is_empty()),
        "logged off"
    );
    assert!(player_manager::get_online_player(&ts.world, OWNER).is_none());
    ts.advance(1.0);
    let from = mark(&ts, owner);
    enter(&mut ts, owner, "owneracct", OWNER);
    ts.advance(1.0);
    let p = ts.world.objects.get(guid(OWNER)).unwrap();
    assert_eq!(
        p.house_instance(),
        Some(HOUSE_GUID),
        "the house is still the owner's"
    );
    assert_eq!(
        player_house::house(&ts.world, guid(OWNER)),
        Some(guid(HOUSE_GUID)),
        "HandleHouseOnLogin found it"
    );
    assert_eq!(
        ts.world
            .objects
            .get(guid(HOUSE_GUID))
            .unwrap()
            .house_owner(),
        Some(OWNER)
    );
    let _ = from;

    // ---- the rent timer. Thirty virtual days are not ticked through: the purchase is backdated
    // (the owner bought 30 days and a minute ago, so the rent fell due a minute ago) and the queue
    // entry rebuilt. HouseManager.Tick (once a virtual minute) finds it due: the rent is paid, so
    // the payment is collected and the next period is due 30 days on.
    let now: i32 = empyrean_common::dotnet::CsCast::cs_cast(ts.world.now.unix_time);
    let purchase = now - 30 * 24 * 3600 - 60;
    backdate(&mut ts, purchase, purchase + 30 * 24 * 3600);
    ts.advance(61.0);
    let next_due = ts
        .world
        .objects
        .get(guid(OWNER))
        .unwrap()
        .house_rent_timestamp()
        .expect("a rent due time");
    assert_eq!(
        next_due,
        purchase + 60 * 24 * 3600,
        "rent paid: the next period ends 60 days after the purchase"
    );
    assert!(
        container::inventory_values(&ts.world, guid(SLUMLORD_GUID)).is_empty(),
        "the slumlord's payment is collected"
    );
    assert_eq!(
        ts.world
            .objects
            .get(guid(HOUSE_GUID))
            .unwrap()
            .house_owner(),
        Some(OWNER)
    );

    // ---- unpaid, the rent lapses: eviction
    let from = mark(&ts, owner);
    let now: i32 = empyrean_common::dotnet::CsCast::cs_cast(ts.world.now.unix_time);
    backdate(&mut ts, purchase - 30 * 24 * 3600, now - 60);
    ts.advance(64.0);
    let w = &ts.world;
    assert_eq!(
        w.objects.get(guid(HOUSE_GUID)).unwrap().house_owner(),
        None,
        "evicted"
    );
    assert_eq!(
        w.objects
            .get(guid(SLUMLORD_GUID))
            .unwrap()
            .get_property(PropertyString::Name)
            .as_deref(),
        Some("Cottage"),
        "the slumlord's name resets"
    );
    let p = w.objects.get(guid(OWNER)).unwrap();
    assert_eq!(
        (p.house_instance(), p.house_id(), p.house_rent_timestamp()),
        (None, None, None)
    );
    assert_eq!(player_house::house(w, guid(OWNER)), None);
    assert!(
        container::get_inventory_items_of_wcid(w, guid(OWNER), DEED).is_empty(),
        "the deed is taken"
    );
    assert!(
        chat(&ts, owner, from).contains(&"Your house has reverted due to non-payment of the maintenance costs.  All items stored in the house have been lost.".to_owned())
    );
    let status: Vec<dereth_protocol::trade::HouseHouseStatus> =
        events(&ts, owner, from, HOUSE_STATUS);
    assert_eq!(status.len(), 1, "the house panel is cleared 3 s later");
    assert_eq!(house_manager::total_owned_housing(&ts.world), 0);
    assert!(
        house::fields(&ts.world, guid(HOUSE_GUID)).guests.is_empty(),
        "the guest list is cleared"
    );
    assert!(
        ts.world
            .objects
            .get(guid(HOUSE_GUID))
            .unwrap()
            .open_to_everyone(),
        "and the house open"
    );
    assert!(can_move_into(&ts, STRANGER), "unowned again");
}

/// A deleted guest is left out of the access list.
/// V322.
#[test]
fn a_deleted_guest_is_left_out_of_the_access_list() {
    use empyrean_world::network::structure::house_access;
    let mut ts = server();
    let owner = login(&mut ts, "owneracct", OWNER);
    ts.advance(1.0);
    let coins = give(&mut ts, OWNER, COINSTACK, Some(500));
    ts.send_game_action(
        owner,
        &HouseBuyHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(4.0);
    ts.send_game_action(
        owner,
        &HouseAddPermanentGuest {
            name: "Guest".to_owned(),
        },
    );
    ts.advance(0.5);
    let deleted = guid(0x5000_0099);
    house::fields_mut(&mut ts.world, guid(HOUSE_GUID))
        .guests
        .insert(deleted, false);

    let har = house_access::house_access_new(&ts.world, Some(guid(HOUSE_GUID)));
    let listed: Vec<(ObjectGuid, Option<String>)> = har
        .guest_list
        .iter()
        .map(|(g, i)| (*g, i.guest_name.clone()))
        .collect();
    assert_eq!(listed, [(guid(GUEST), Some("Guest".to_owned()))]);
}

#[test]
fn the_barrier_stops_a_stranger_and_lets_a_guest_in() {
    const START_CELL: u32 = 0xA9B4_001A; // x 72..96, y 24..48
    const FENCED: u32 = 0xA9B4_001B; // x 72..96, y 48..72

    let mut ts = server();
    let owner = login(&mut ts, "owneracct", OWNER);
    let guest = login(&mut ts, "guestacct", GUEST);
    let stranger = login(&mut ts, "strangeracct", STRANGER);
    ts.advance(1.0);
    let coins = give(&mut ts, OWNER, COINSTACK, Some(500));
    ts.send_game_action(
        owner,
        &HouseBuyHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(4.0);
    ts.send_game_action(
        owner,
        &HouseAddPermanentGuest {
            name: "Guest".to_owned(),
        },
    );
    ts.advance(0.5);
    assert!(can_move_into(&ts, GUEST) && !can_move_into(&ts, STRANGER));

    phys_ext::set_cell_restriction(&mut ts.world, dereth_primitives::CellId(FENCED), HOUSE_GUID);

    // out of portal space at (84, 40), then north at 4 m/s for 6 s: across y 48 into the fence
    let walk = |ts: &mut TestServer, id: ClientId, who: u32| -> (f32, u32) {
        let start = empyrean_entity::Position::from_components(
            START_CELL, 84.0, 40.0, 94.005, 0.0, 0.0, 0.0, 1.0, false,
        );
        empyrean_world::world_objects::player_location::teleport(
            &mut ts.world,
            guid(who),
            &start,
            false,
        );
        ts.advance(0.5);
        ts.send_game_action(
            id,
            &dereth_protocol::login::CharacterLoginCompleteNotification,
        );
        ts.advance(0.5);
        let h = phys_ext::physics_obj(&ts.world, guid(who)).expect("a body");
        assert!(
            !phys_ext::get_physics_state(
                &ts.world,
                guid(who),
                empyrean_entity::enums::PhysicsState::Hidden
            ),
            "out of portal space"
        );
        assert_eq!(
            phys_ext::position(&ts.world, h).map(|p| p.cell.0),
            Some(START_CELL)
        );
        for _ in 0..60 {
            phys_ext::set_velocity(
                &mut ts.world,
                h,
                empyrean_common::dotnet::Vector3::new(0.0, 4.0, 0.0),
                false,
            );
            ts.advance(0.1);
            phys_ext::update_object(&mut ts.world, h);
        }
        let p = phys_ext::position(&ts.world, h).expect("placed");
        (p.frame.origin.y, p.cell.0)
    };

    let (y, cell) = walk(&mut ts, stranger, STRANGER);
    assert!(
        y < 48.0 && cell == START_CELL,
        "the stranger is stopped at the barrier: y {y}, cell {cell:08X}"
    );
    assert!(y > 44.0, "he walked up to it: y {y}");

    let (y, cell) = walk(&mut ts, guest, GUEST);
    assert!(
        y > 55.0 && cell == FENCED,
        "the guest walks in: y {y}, cell {cell:08X}"
    );

    let (y, cell) = walk(&mut ts, owner, OWNER);
    assert!(
        y > 55.0 && cell == FENCED,
        "and the owner: y {y}, cell {cell:08X}"
    );
}

fn x_of(ts: &TestServer, who: u32) -> f32 {
    ts.world
        .objects
        .get(guid(who))
        .unwrap()
        .location()
        .expect("placed")
        .position_x
}

fn log_off(ts: &mut TestServer, id: ClientId, who: u32) {
    let seen = ts.received::<LoginExecuteLogOff>(id).len();
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginExecuteLogOffRequest {
            character: ObjectId(who),
        },
    );
    assert!(
        ts.run_until(10.0, |ts| ts.received::<LoginExecuteLogOff>(id).len()
            > seen),
        "logged off"
    );
    ts.advance(1.0);
}

/// `/house recall` from where the character stands, then long enough for the animation and the
/// teleport: whether it ended at the slumlord (x 82) rather than where it started.
fn recalls_home(ts: &mut TestServer, who: u32) -> bool {
    let start = x_of(ts, who);
    empyrean_world::world_objects::player_location::handle_action_tele_to_house(
        &mut ts.world,
        guid(who),
    );
    ts.advance(10.0);
    let end = x_of(ts, who);
    (end - 82.0).abs() < 0.5 && (end - start).abs() > 0.5
}

/// Puts the character back where it was seeded (x 90), clear of the slumlord.
fn step_away(ts: &mut TestServer, who: u32) {
    let at = empyrean_entity::Position::from_components(
        CELL, 90.0, 10.0, 94.005, 0.0, 0.0, 0.0, 1.0, false,
    );
    empyrean_world::world_objects::player_location::teleport(&mut ts.world, guid(who), &at, false);
    ts.advance(2.0);
}

/// V295: a character without a house of its own on an account whose other character owns one.
/// Logged in, the login loads the account's house onto it and it recalls there; with its house
/// slot emptied the account-house lookup still finds that house; with one house per character it
/// gets none from either path; a character on an account with no house gets none.
#[test]
fn an_alt_recalls_to_the_account_house() {
    let mut ts = server_with(true);
    let owner = login(&mut ts, "owneracct", OWNER);
    ts.advance(1.0);
    let coins = give(&mut ts, OWNER, COINSTACK, Some(500));
    ts.send_game_action(
        owner,
        &HouseBuyHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(4.0);
    assert_eq!(
        ts.world.objects.get(guid(OWNER)).unwrap().house_instance(),
        Some(HOUSE_GUID),
        "bought"
    );
    log_off(&mut ts, owner, OWNER);

    // (1) the alt, logged in normally: the account's house is loaded onto it, and it recalls there
    enter(&mut ts, owner, "owneracct", ALT);
    ts.advance(1.0);
    assert_eq!(
        ts.world.objects.get(guid(ALT)).unwrap().house_instance(),
        None,
        "the alt owns no house"
    );
    assert_eq!(
        player_house::house(&ts.world, guid(ALT)),
        Some(guid(HOUSE_GUID)),
        "the login loaded the account's house"
    );
    assert!(
        recalls_home(&mut ts, ALT),
        "the alt recalls to the account's house"
    );

    // (2) its house slot emptied: the account-house lookup finds the account's house
    step_away(&mut ts, ALT);
    player_house::set_house(&mut ts.world, guid(ALT), None);
    assert_eq!(
        player_house::get_account_house(&mut ts.world, guid(ALT)),
        Some(guid(HOUSE_GUID)),
        "the account's house"
    );
    player_house::set_house(&mut ts.world, guid(ALT), None);
    assert!(
        recalls_home(&mut ts, ALT),
        "and /house recall takes it there"
    );

    // (3) one house per character: the alt, logged in again, gets none from either path
    step_away(&mut ts, ALT);
    log_off(&mut ts, owner, ALT);
    empyrean_world::managers::property_manager::modify_bool(&ts.world, "house_per_char", true);
    enter(&mut ts, owner, "owneracct", ALT);
    ts.advance(1.0);
    assert_eq!(
        player_house::house(&ts.world, guid(ALT)),
        None,
        "the login loads no house"
    );
    assert_eq!(
        player_house::get_account_house(&mut ts.world, guid(ALT)),
        None,
        "nor does the account-house lookup find one"
    );
    assert!(!recalls_home(&mut ts, ALT), "no recall");
    empyrean_world::managers::property_manager::modify_bool(&ts.world, "house_per_char", false);

    // (4) a character on an account with no house
    let _stranger = login(&mut ts, "strangeracct", STRANGER);
    ts.advance(1.0);
    assert_eq!(player_house::house(&ts.world, guid(STRANGER)), None);
    assert_eq!(
        player_house::get_account_house(&mut ts.world, guid(STRANGER)),
        None
    );
    assert!(!recalls_home(&mut ts, STRANGER), "no recall");
}

/// `/house-select` for an owner of two houses on one account: the kept house stays, the other is
/// evicted, and nothing is written to the console (ACE e0f9ce83 dropped a stray "OK").
#[test]
fn house_select_keeps_one_house_and_writes_nothing_to_the_console() {
    const HOUSE2_GUID: u32 = 0x7A9B_4011;
    const SLUMLORD2_GUID: u32 = 0x7A9B_4012;
    let mut ts = server();
    let mut second = LandblockInstance::new(HOUSE2_GUID, COTTAGE, CELL, [80.0, 40.0, 94.0]);
    second.landblock_instance_link.push(LandblockInstanceLink {
        id: 1,
        parent_guid: HOUSE2_GUID,
        child_guid: SLUMLORD2_GUID,
        ..Default::default()
    });
    ts.world.content = Arc::new(
        content()
            .landblock_instance(second)
            .landblock_instance(child(SLUMLORD2_GUID, SLUMLORD, [82.0, 42.0])),
    );
    house_manager::initialize(&mut ts.world);

    let owner = login(&mut ts, "owneracct", OWNER);
    ts.advance(1.0);
    let coins = give(&mut ts, OWNER, COINSTACK, Some(500));
    ts.send_game_action(
        owner,
        &HouseBuyHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(4.0);
    assert_eq!(
        ts.world.objects.get(guid(OWNER)).unwrap().house_instance(),
        Some(HOUSE_GUID),
        "bought"
    );

    // the second house on the same account (a multi-house owner)
    ts.world
        .objects
        .get_mut(guid(HOUSE2_GUID))
        .expect("the second house loaded")
        .set_house_owner_prop(Some(OWNER));
    house_manager::add_rent_queue(
        &mut ts.world,
        empyrean_world::entity::i_player::IPlayer::Online(guid(OWNER)),
        HOUSE2_GUID,
    );
    let houses = player_house::get_multi_houses(&mut ts.world, guid(OWNER));
    assert_eq!(houses.len(), 2);
    let keep = houses.iter().position(|h| h.full() == HOUSE_GUID).unwrap() + 1;

    let session = ts
        .world
        .sessions
        .iter()
        .find(|(_, s)| s.player == Some(guid(OWNER)))
        .map(|(s, _)| s)
        .expect("the owner's session");
    empyrean_command::command_manager::start_console_capture();
    empyrean_command::handlers::player_commands::handle_house_select_confirmed(
        &mut ts.world,
        Some(session),
        true,
        &[keep.to_string()],
    );
    let console = empyrean_command::command_manager::take_console_output();
    assert_eq!(console, Vec::<String>::new(), "no console output");
    assert_eq!(
        ts.world
            .objects
            .get(guid(HOUSE2_GUID))
            .unwrap()
            .house_owner(),
        None,
        "the other house is evicted"
    );
    assert_eq!(
        ts.world
            .objects
            .get(guid(HOUSE_GUID))
            .unwrap()
            .house_owner(),
        Some(OWNER),
        "the kept house stays"
    );
    assert_eq!(
        player_house::get_multi_houses(&mut ts.world, guid(OWNER)),
        vec![guid(HOUSE_GUID)]
    );
}

/// Divergence: V420
/// A world without housing shows no house profile at the slumlord and refuses buying the cottage
/// (the pyreals stay, nobody owns it); one with housing but no apartments sells the cottage but
/// refuses the house recall, which goes with apartments. Each refusal tells the player why.
#[test]
fn a_world_without_housing_refuses_the_cottage_and_one_without_apartments_the_recall() {
    use empyrean_common::era::{with_features, EraExt as _, EraFeatures, EraId};
    let mut ts = server();
    let owner = login(&mut ts, "owneracct", OWNER);
    ts.advance(1.0);
    let coins = give(&mut ts, OWNER, COINSTACK, Some(500));
    let eor = EraId::Eor.rules();
    ts.world.era = with_features(
        eor,
        EraFeatures {
            housing: false,
            apartments: false,
            ..eor.features
        },
    );

    let from = mark(&ts, owner);
    empyrean_world::world_objects::slum_lord::slum_lord_act_on_use(
        &mut ts.world,
        guid(SLUMLORD_GUID),
        guid(OWNER),
    );
    ts.send_game_action(
        owner,
        &HouseBuyHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(4.0);
    let profiles: Vec<HouseProfileMessage> = events(&ts, owner, from, HOUSE_PROFILE);
    assert!(profiles.is_empty(), "no purchase window");
    assert_eq!(
        chat(&ts, owner, from),
        ["This world has no housing.", "This world has no housing."]
    );
    assert_eq!(ts.world.objects.get(coins).unwrap().stack_size(), Some(500));
    assert_eq!(
        ts.world
            .objects
            .get(guid(HOUSE_GUID))
            .unwrap()
            .house_owner(),
        None
    );

    // Housing without apartments: the cottage sells, the recall is refused.
    ts.world.era = with_features(
        eor,
        EraFeatures {
            apartments: false,
            ..eor.features
        },
    );
    ts.send_game_action(
        owner,
        &HouseBuyHouse {
            slumlord: ObjectId(SLUMLORD_GUID),
            items: vec![ObjectId(coins.full())],
        },
    );
    ts.advance(4.0);
    assert_eq!(
        ts.world
            .objects
            .get(guid(HOUSE_GUID))
            .unwrap()
            .house_owner(),
        Some(OWNER)
    );
    step_away(&mut ts, OWNER);
    let from = mark(&ts, owner);
    assert!(!recalls_home(&mut ts, OWNER));
    assert_eq!(chat(&ts, owner, from), ["This world has no house recall."]);

    // February 2005 had apartments and their recalls.
    ts.world.era = EraId::Infiltration.rules();
    assert!(recalls_home(&mut ts, OWNER));
}
