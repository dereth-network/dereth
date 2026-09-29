//! ACE: Source/ACE.Server/WorldObjects/Vendor.cs::Vendor
//! Use/buy/sell at a vendor and a refused buy; vendor out of reach, closed or out of value range
//! refused.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::sync::Arc;

use dereth_primitives::ObjectId;
use dereth_protocol::comms::CommunicationHearDirectSpeech;
use dereth_protocol::objects::{
    CharacterServerSaysAttemptFailed, ItemServerSaysContainId, ItemUseDone,
};
use dereth_protocol::trade::{ItemProfile, VendorBuy, VendorInfo, VendorSell};
use empyrean_content::models::world::{Weenie, WeeniePropertiesCreateList};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CharacterOptions1, DestinationType, EmoteCategory, EmoteType, ItemType, PropertyAttribute,
    PropertyDataId, PropertyFloat, PropertyInt, PropertyString, VendorType, WeenieError,
    WeenieType,
};
use empyrean_entity::models::properties_emote::PropertiesEmote;
use empyrean_entity::models::properties_emote_action::PropertiesEmoteAction;
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land::{self, TEST_SETUP};
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::landblock_manager;
use empyrean_world::world_objects::world_object::CtorEnv;
use empyrean_world::world_objects::{container, player_commerce, vendor};

const LB: u32 = 0xA9B4_0000;

const PLAYER_WCID: u32 = 1;
const SHOPKEEPER: u32 = 2;
const GEM: u32 = 3; // stackable, max 100, unit value 10
const CUP: u32 = 4; // value 100
const COINSTACK: u32 = 273;

const ALPHA: u32 = 0x5000_0001;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
const SAVE_FAILED: u32 = 0x00A0;
const CONTAIN_ID: u32 = 0x0022;
const VENDOR_INFO: u32 = 0x0062;
/// `Vendor.Rotate(player)`'s TurnToObject (`ApproachVendor` and `ActOnUse`).
const MOVEMENT: u32 = 0xF74C;
const USE_DONE: u32 = 0x01C7;
const STACK_SIZE: u32 = 0x0197;
const PRIVATE_INT: u32 = 0x02CD;
const TELL: u32 = 0x02BD;
const CREATE_OBJECT: u32 = 0xF745;
const SOUND: u32 = 0xF750;

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
    let row = |id: u32, wcid: u32, stack_size: i32| WeeniePropertiesCreateList {
        id,
        object_id: SHOPKEEPER,
        destination_type: i8::try_from(DestinationType::Shop.0).unwrap(),
        weenie_class_id: wcid,
        stack_size,
        ..Default::default()
    };
    shopkeeper.weenie_properties_create_list = vec![row(1, GEM, -1), row(2, CUP, 5)];

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
        .weenie(shopkeeper)
        .weenie(
            weenie(GEM, "Gem", WeenieType::Stackable)
                .with_string(PropertyString::PluralName, "Gems")
                .with_int(PropertyInt::ItemType, int(ItemType::Gem))
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::StackUnitValue, 10)
                .with_int(PropertyInt::StackUnitEncumbrance, 1)
                .with_int(PropertyInt::Value, 10)
                .with_int(PropertyInt::EncumbranceVal, 1),
        )
        .weenie(
            weenie(CUP, "Cup", WeenieType::Generic)
                .with_int(PropertyInt::ItemType, int(ItemType::Misc))
                .with_int(PropertyInt::Value, 100)
                .with_int(PropertyInt::EncumbranceVal, 5),
        )
        .weenie(
            Weenie::new(COINSTACK, "coinstack", WeenieType::Coin)
                .with_string(PropertyString::Name, "Pyreal")
                .with_did(PropertyDataId::Setup, TEST_SETUP)
                .with_string(PropertyString::PluralName, "Pyreals")
                .with_int(PropertyInt::ItemType, int(ItemType::Money))
                .with_int(PropertyInt::MaxStackSize, 25000)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::StackUnitValue, 1)
                .with_int(PropertyInt::Value, 1),
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

/// A client logged in as `account` whose session plays `guid` at `pos` (as `emotes.rs` joins).
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

fn tell(message: &str) -> PropertiesEmoteAction {
    PropertiesEmoteAction {
        r#type: EmoteType::Tell.0.cast_unsigned(),
        message: Some(message.to_owned()),
        ..PropertiesEmoteAction::default()
    }
}

fn vendor_set(vendor_type: VendorType, message: &str) -> PropertiesEmote {
    PropertiesEmote {
        category: EmoteCategory::Vendor,
        probability: 1.0,
        vendor_type: Some(vendor_type),
        properties_emote_action: vec![tell(message)],
        ..PropertiesEmote::default()
    }
}

/// The shopkeeper at `pos`, with a Tell for each of Open, Buy and Sell.
fn shopkeeper(ts: &mut TestServer, pos: Position) -> ObjectGuid {
    let g = new_object(ts, SHOPKEEPER);
    let o = ts.world.objects.get_mut(g).unwrap();
    o.set_location(Some(pos));
    o.biota.properties_emote = Some(Arc::new(vec![
        vendor_set(VendorType::Open, "Welcome, %s."),
        vendor_set(VendorType::Buy, "A fine purchase."),
        vendor_set(VendorType::Sell, "I'll take that."),
    ]));
    assert!(landblock_manager::add_object(&mut ts.world, g, false));
    ts.advance(0.1);
    g
}

fn all(g: &[Got], kind: u32) -> Vec<&Got> {
    g.iter().filter(|m| m.kind == kind).collect()
}

/// The item profiles of a VendorInfo as (amount, guid).
fn listed(info: &VendorInfo) -> Vec<(i32, u32)> {
    info.items.iter().map(|i| (i.amount, i.iid.0)).collect()
}

// ------------------------------------------------------------------ the scenario

/// A player uses a vendor and sees its inventory; buys three gems (the pyreals leave, the stack
/// arrives); sells a cup back (the pyreals arrive, and the cup shows in the vendor's list); and a
/// buy it cannot afford is refused with ACE's answer (InventoryServerSaveFailed, then UseDone).
#[test]
fn use_buy_sell_and_a_refused_buy() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let alpha = ObjectGuid::new(ALPHA);
    let npc = shopkeeper(&mut ts, at(21.0, 20.0));

    let coins = new_object(&mut ts, COINSTACK);
    ts.world
        .objects
        .get_mut(coins)
        .unwrap()
        .set_stack_size(Some(100));
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        alpha,
        coins,
        0,
        false,
        true
    ));
    let my_cup = new_object(&mut ts, CUP);
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        alpha,
        my_cup,
        0,
        false,
        true
    ));
    player_commerce::update_coin_value(&mut ts.world, alpha, false);
    ts.advance(0.1);

    // ---- use: LoadInventory 1 ms later, then ApproachVendor(Open): VendorInfo, then the Open emote
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, npc, alpha)
    });
    // `Vendor.ApproachVendor`: the vendor turns to the player (`Rotate`) before VendorInfo, and
    // again (`Rotate`) with the Open emote
    assert_eq!(sent, [MOVEMENT, VENDOR_INFO, MOVEMENT, TELL], "{sent:04X?}");
    let info: VendorInfo = first(&g, VENDOR_INFO).decode();
    let shop = vendor::default_items_for_sale(&ts.world, npc);
    assert_eq!(info.merchant_id.0, npc.full());
    assert_eq!(
        (
            info.profile.item_types,
            info.profile.min_value,
            info.profile.max_value,
            info.profile.magic
        ),
        (ItemType::Misc.0 | ItemType::Gem.0, 0, 100_000, 0)
    );
    assert_eq!(
        (info.profile.buy_price, info.profile.sell_price),
        (0.9, 1.5)
    );
    assert_eq!(
        (
            info.profile.trade_id,
            info.profile.trade_num,
            info.profile.trade_name.as_str()
        ),
        (0, 0, "")
    );
    // the gem row's -1 (unlimited) packs as 0xFFFFFF, which reads back sign-extended; each item
    // carries its PublicWeenieDesc
    assert_eq!(
        listed(&info),
        vec![(-1, shop[0].full()), (5, shop[1].full())]
    );
    assert!(info.items.iter().all(|i| i.pwd.is_some()));
    let greeting: CommunicationHearDirectSpeech = first(&g, TELL).decode();
    assert_eq!(greeting.message, "Welcome, Alpha.");
    assert_eq!(
        ts.world
            .objects
            .get(alpha)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player_use
            .last_opened_container_id,
        npc
    );

    // ---- buy 3 gems: 1.5 x 30 = 45 pyreals. SpendCurrency consumes them (SetStackSize, burden,
    // CoinValue); the stack is created in the pack (CreateObject, ContainId, burden); the
    // PickUpItem sound; ApproachVendor(Buy): VendorInfo and the Buy emote; then UseDone.
    let buy = VendorBuy {
        vendor_id: ObjectId(npc.full()),
        items: vec![ItemProfile {
            amount: 3,
            iid: ObjectId(shop[0].full()),
            pwd: None,
        }],
        alternate_currency_id: 0,
    };
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &buy)
    });
    assert_eq!(
        sent,
        [
            STACK_SIZE,
            PRIVATE_INT,
            PRIVATE_INT,
            CREATE_OBJECT,
            CONTAIN_ID,
            PRIVATE_INT,
            SOUND,
            VENDOR_INFO,
            MOVEMENT,
            TELL,
            USE_DONE
        ],
        "{sent:04X?}"
    );
    assert_eq!(
        ts.world.objects.get(coins).unwrap().stack_size(),
        Some(55),
        "the pyreals leave"
    );
    assert_eq!(ts.world.objects.get(alpha).unwrap().coin_value(), Some(55));
    let gems: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
    assert_eq!(gems.container.0, ALPHA);
    let bought = ObjectGuid::new(gems.item.0);
    assert_eq!(
        (
            ts.world.objects.get(bought).unwrap().biota.weenie_class_id,
            ts.world.objects.get(bought).unwrap().stack_size()
        ),
        (GEM, Some(3))
    );
    assert!(
        container::inventory_values(&ts.world, alpha).contains(&bought),
        "the items arrive"
    );
    let thanks: CommunicationHearDirectSpeech = first(&g, TELL).decode();
    assert_eq!(thanks.message, "A fine purchase.");
    let done: ItemUseDone = first(&g, USE_DONE).decode();
    assert_eq!(done.failure_type, 0);

    // ---- sell the cup: 0.9 x 100 = 90 pyreals. The cup leaves the pack (burden; a sale sends no
    // Container clear), ContainId into the vendor; ProcessItemsForPurchase keeps it for resale and
    // ApproachVendor(Sell) lists it (VendorInfo, the Sell emote); the payout stack is created
    // (CreateObject, ContainId, burden, CoinValue); the sound; UseDone.
    let sell = VendorSell {
        vendor_id: ObjectId(npc.full()),
        items: vec![ItemProfile {
            amount: 1,
            iid: ObjectId(my_cup.full()),
            pwd: None,
        }],
    };
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &sell)
    });
    assert_eq!(
        sent,
        [
            PRIVATE_INT,
            CONTAIN_ID,
            VENDOR_INFO,
            MOVEMENT,
            TELL,
            CREATE_OBJECT,
            CONTAIN_ID,
            PRIVATE_INT,
            PRIVATE_INT,
            SOUND,
            USE_DONE
        ],
        "{sent:04X?}"
    );
    let moved: ItemServerSaysContainId = first(&g, CONTAIN_ID).decode();
    assert_eq!(
        (moved.item.0, moved.container.0),
        (my_cup.full(), npc.full())
    );
    let info: VendorInfo = first(&g, VENDOR_INFO).decode();
    assert_eq!(
        listed(&info),
        vec![
            (-1, shop[0].full()),
            (5, shop[1].full()),
            (1, my_cup.full())
        ],
        "the cup shows in the vendor's list"
    );
    let payout: ItemServerSaysContainId = all(&g, CONTAIN_ID)[1].decode();
    let payout = ObjectGuid::new(payout.item.0);
    assert_eq!(
        ts.world.objects.get(payout).unwrap().stack_size(),
        Some(90),
        "the pyreals arrive"
    );
    assert_eq!(ts.world.objects.get(alpha).unwrap().coin_value(), Some(145));
    assert!(!container::inventory_values(&ts.world, alpha).contains(&my_cup));
    let taken: CommunicationHearDirectSpeech = first(&g, TELL).decode();
    assert_eq!(taken.message, "I'll take that.");

    // ---- a buy it cannot afford: the cup back costs 1.5 x 100 = 150 > 145
    let rebuy = VendorBuy {
        vendor_id: ObjectId(npc.full()),
        items: vec![ItemProfile {
            amount: 1,
            iid: ObjectId(my_cup.full()),
            pwd: None,
        }],
        alternate_currency_id: 0,
    };
    let (sent, g) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &rebuy)
    });
    assert_eq!(sent, [SAVE_FAILED, USE_DONE], "{sent:04X?}");
    let failed: CharacterServerSaysAttemptFailed = first(&g, SAVE_FAILED).decode();
    assert_eq!(
        (failed.object.0, failed.reason),
        (ALPHA, u32::try_from(WeenieError::None.0).unwrap())
    );
    let done: ItemUseDone = first(&g, USE_DONE).decode();
    assert_eq!(done.failure_type, 0);
    assert_eq!(
        ts.world.objects.get(alpha).unwrap().coin_value(),
        Some(145),
        "nothing is taken"
    );
    assert_eq!(
        vendor::unique_items_for_sale(&ts.world, npc),
        vec![my_cup],
        "the cup is still for sale"
    );
}

/// V315: a buy or sell with a vendor the player is not within the use
/// radius of, or one that is not open for business, is refused as a vendor that is not there
/// (InventoryServerSaveFailed, then UseDone NoObject) and nothing changes hands; and an item above
/// the vendor's MerchandiseMaxValue is refused as unsellable. ACE checked none of these.
#[test]
fn a_vendor_out_of_reach_closed_or_out_of_its_value_range_is_refused() {
    let mut ts = server();
    let (id, session) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0));
    let alpha = ObjectGuid::new(ALPHA);
    let far = shopkeeper(&mut ts, at(40.0, 20.0));

    let coins = new_object(&mut ts, COINSTACK);
    ts.world
        .objects
        .get_mut(coins)
        .unwrap()
        .set_stack_size(Some(1000));
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        alpha,
        coins,
        0,
        false,
        true
    ));
    let my_cup = new_object(&mut ts, CUP);
    assert!(container::try_add_to_inventory(
        &mut ts.world,
        alpha,
        my_cup,
        0,
        false,
        true
    ));
    player_commerce::update_coin_value(&mut ts.world, alpha, false);
    ts.advance(0.1);

    let refused = |ts: &mut TestServer, msg: &dyn Fn(&mut TestServer)| {
        let (sent, g) = exchange(ts, id, session, 0.3, |ts| msg(ts));
        assert_eq!(sent, [SAVE_FAILED, USE_DONE], "{sent:04X?}");
        let done: ItemUseDone = first(&g, USE_DONE).decode();
        assert_eq!(
            done.failure_type,
            u32::try_from(WeenieError::NoObject.0).unwrap()
        );
    };

    // 20 m away with a use radius of 3: out of reach (the vendor's stock is loaded as its use
    // loads it)
    empyrean_world::dispatch::act_on_use::act_on_use(&mut ts.world, far, alpha);
    ts.advance(0.3);
    let shop = vendor::default_items_for_sale(&ts.world, far);
    let buy = VendorBuy {
        vendor_id: ObjectId(far.full()),
        items: vec![ItemProfile {
            amount: 1,
            iid: ObjectId(shop[0].full()),
            pwd: None,
        }],
        alternate_currency_id: 0,
    };
    refused(&mut ts, &|ts| ts.send_game_action(id, &buy));
    let sell = VendorSell {
        vendor_id: ObjectId(far.full()),
        items: vec![ItemProfile {
            amount: 1,
            iid: ObjectId(my_cup.full()),
            pwd: None,
        }],
    };
    refused(&mut ts, &|ts| ts.send_game_action(id, &sell));
    assert_eq!(
        ts.world.objects.get(alpha).unwrap().coin_value(),
        Some(1000),
        "nothing is taken"
    );
    assert!(
        container::inventory_values(&ts.world, alpha).contains(&my_cup),
        "nothing is sold"
    );

    // near, but closed for business
    let near = shopkeeper(&mut ts, at(21.0, 20.0));
    ts.world
        .objects
        .get_mut(near)
        .unwrap()
        .set_property(empyrean_entity::enums::PropertyBool::OpenForBusiness, false);
    let sell = VendorSell {
        vendor_id: ObjectId(near.full()),
        items: vec![ItemProfile {
            amount: 1,
            iid: ObjectId(my_cup.full()),
            pwd: None,
        }],
    };
    refused(&mut ts, &|ts| ts.send_game_action(id, &sell));
    assert!(container::inventory_values(&ts.world, alpha).contains(&my_cup));

    // open again, with a top value of 50: the cup (100) is refused as unsellable
    ts.world
        .objects
        .get_mut(near)
        .unwrap()
        .set_property(empyrean_entity::enums::PropertyBool::OpenForBusiness, true);
    ts.world
        .objects
        .get_mut(near)
        .unwrap()
        .set_property(PropertyInt::MerchandiseMaxValue, 50);
    let (sent, _) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &sell)
    });
    assert_eq!(
        sent,
        [0x02EB, SAVE_FAILED, USE_DONE],
        "the unsellable notice, then the refusal: {sent:04X?}"
    );
    assert!(
        container::inventory_values(&ts.world, alpha).contains(&my_cup),
        "the cup stays"
    );

    // within the range it sells
    ts.world
        .objects
        .get_mut(near)
        .unwrap()
        .set_property(PropertyInt::MerchandiseMaxValue, 100);
    let (sent, _) = exchange(&mut ts, id, session, 0.3, |ts| {
        ts.send_game_action(id, &sell)
    });
    assert!(sent.contains(&CONTAIN_ID), "sold: {sent:04X?}");
    assert!(!container::inventory_values(&ts.world, alpha).contains(&my_cup));
}

pub(crate) use crate::support::messages::{exchange, first, Got};

pub(crate) use crate::support::empty_shard::EmptyShard;
