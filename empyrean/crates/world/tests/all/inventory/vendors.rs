//! Vectors: fixtures/vectors/vendors/
//! Vendor price formulas and change-making replay ACE vendor vectors; buy/sell flows from
//! Player_Commerce.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::vectors::{self, f64_of, i64_of, Case};
use empyrean_content::models::world::{
    Weenie as ContentWeenie, WeeniePropertiesCreateList, WeeniePropertiesGenerator,
};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    DestinationType, ItemType, ObjectDescriptionFlag, PropertyAttribute, PropertyBool,
    PropertyFloat, PropertyInt, PropertyString, RegenLocationType, WeenieType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_store::MemShard;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::item_profile::ItemProfile;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{container, player_commerce as pc, vendor};
use empyrean_world::World;
use serde_json::Value;

// ------------------------------------------------------------------ fixtures

const PLAYER_WCID: u32 = 1;
const VENDOR_WCID: u32 = 2;
const GEM: u32 = 3; // stackable, max 100, unit value 10, unit burden 1
const CUP: u32 = 4; // generic, value 100, burden 5
const PACK: u32 = 5;
const NOTE: u32 = 6; // a promissory note, value 1000
const BROKE_VENDOR: u32 = 7; // no BuyPrice
const COINSTACK: u32 = 273; // `coinStackWcid`: max 25000, unit value 1

const PLAYER: u32 = 0x5000_0001;
const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

// opcodes and game-event types
const GAME_EVENT: u32 = 0xF7B0;
const CONTAIN_ID: u32 = 0x0022;
const VENDOR_INFO: u32 = 0x0062;
const TRANSIENT: u32 = 0x02EB;
const STACK_SIZE: u32 = 0x0197;
const PRIVATE_INT: u32 = 0x02CD;
const REMOVE_OBJECT: u32 = 0x0024;
const CREATE_OBJECT: u32 = 0xF745;
const SOUND: u32 = 0xF750;
const INSTANCE_ID: u32 = 0x02DA;

fn vendor_weenie(wcid: u32, name: &str) -> ContentWeenie {
    let mut v = ContentWeenie::new(wcid, name, WeenieType::Vendor)
        .with_string(PropertyString::Name, name)
        .with_int(
            PropertyInt::MerchandiseItemTypes,
            i32::try_from(ItemType::Misc.0 | ItemType::Gem.0).unwrap(),
        )
        .with_int(PropertyInt::MerchandiseMinValue, 0)
        .with_int(PropertyInt::MerchandiseMaxValue, 100_000)
        .with_float(PropertyFloat::BuyPrice, 0.9)
        .with_float(PropertyFloat::SellPrice, 1.5);
    let row = |id: u32,
               destination: DestinationType,
               wcid: u32,
               stack_size: i32,
               palette: i8,
               shade: f32| WeeniePropertiesCreateList {
        id,
        object_id: wcid,
        destination_type: i8::try_from(destination.0).unwrap(),
        weenie_class_id: wcid,
        stack_size,
        palette,
        shade,
        ..Default::default()
    };
    v.weenie_properties_create_list = vec![
        row(1, DestinationType::Shop, GEM, -1, 0, 0.0),
        row(2, DestinationType::Contain, CUP, 1, 0, 0.0),
        row(3, DestinationType::Shop, CUP, 5, 3, 0.5),
        row(4, DestinationType::Wield, CUP, 1, 0, 0.0),
    ];
    v.weenie_properties_generator = vec![
        WeeniePropertiesGenerator {
            id: 1,
            object_id: wcid,
            probability: -1.0,
            weenie_class_id: CUP,
            where_create: RegenLocationType::Shop.0,
            ..Default::default()
        },
        WeeniePropertiesGenerator {
            id: 2,
            object_id: wcid,
            probability: -1.0,
            weenie_class_id: GEM,
            where_create: RegenLocationType::Scatter.0,
            ..Default::default()
        },
    ];
    v
}

fn content() -> MemContent {
    MemContent::new()
        .weenie(
            ContentWeenie::new(PLAYER_WCID, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_string(PropertyString::Name, "Tester")
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(vendor_weenie(VENDOR_WCID, "Shopkeeper"))
        .weenie(
            ContentWeenie::new(BROKE_VENDOR, "broke", WeenieType::Vendor)
                .with_string(PropertyString::Name, "Broke"),
        )
        .weenie(
            ContentWeenie::new(GEM, "gem", WeenieType::Stackable)
                .with_string(PropertyString::Name, "Gem")
                .with_string(PropertyString::PluralName, "Gems")
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::Gem.0).unwrap(),
                )
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 1)
                .with_int(PropertyInt::StackUnitValue, 10)
                .with_int(PropertyInt::StackUnitEncumbrance, 1)
                .with_int(PropertyInt::Value, 10)
                .with_int(PropertyInt::EncumbranceVal, 1),
        )
        .weenie(
            ContentWeenie::new(CUP, "cup", WeenieType::Generic)
                .with_string(PropertyString::Name, "Cup")
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::Misc.0).unwrap(),
                )
                .with_int(PropertyInt::Value, 100)
                .with_int(PropertyInt::EncumbranceVal, 5),
        )
        .weenie(
            ContentWeenie::new(PACK, "pack", WeenieType::Container)
                .with_string(PropertyString::Name, "Pack")
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::Misc.0).unwrap(),
                )
                .with_int(PropertyInt::ItemsCapacity, 24)
                .with_int(PropertyInt::Value, 60),
        )
        .weenie(
            ContentWeenie::new(NOTE, "note", WeenieType::Generic)
                .with_string(PropertyString::Name, "Note")
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::PromissoryNote.0).unwrap(),
                )
                .with_int(PropertyInt::Value, 1000),
        )
        .weenie(
            ContentWeenie::new(COINSTACK, "coinstack", WeenieType::Coin)
                .with_string(PropertyString::Name, "Pyreal")
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
}

/// `GuidManager.Initialize` over an empty shard.
struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn now_at(unix: f64) -> ClockSnapshot {
    ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: unix,
        utc: DotNetDateTime::new(1970, 1, 1).add_seconds(unix),
        monotonic: Duration::ZERO,
    }
}

const T0: f64 = 1_767_225_600.0;

/// A world on `content`, with ACE's default server properties and one session S whose player is
/// PLAYER (strength 100, burden 0, no coins).
fn world_on(content: MemContent) -> World {
    let mut w = World::new(
        now_at(T0),
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
    );
    w.content = Arc::new(content);
    guid_manager::initialize(&mut w, &mut EmptyShard);
    pm::install_shard_config(&mut w, pm::shard_config_handle(Box::new(MemShard::new())));
    pm::initialize(&mut w, true);

    if let Some(weenie) = w.content.get_cached_weenie(PLAYER_WCID) {
        let mut o = CtorEnv::with_world(&w, |env| {
            empyrean_world::world_objects::player::player_from_weenie(
                env,
                Class::Player,
                weenie,
                ObjectGuid::new(PLAYER),
                1,
            )
        });
        let rec = o
            .biota
            .properties_attribute
            .get_or_insert_with(Default::default)
            .get_or_insert_with(PropertyAttribute::Strength, Default::default);
        rec.init_level = 100;
        o.set_encumbrance_val(Some(0));
        o.set_value(Some(0));
        o.player.as_mut().expect("a player").player.character =
            Some(empyrean_store::models::shard::Character::default());
        w.objects.insert(o).expect("fresh");

        let mut s = SessionData::default();
        s.set_account(
            7,
            "acct".to_owned(),
            empyrean_entity::enums::AccessLevel::Player,
        );
        s.set_player(Some(ObjectGuid::new(PLAYER)));
        w.sessions.insert(S, s);
    }
    w
}

fn world() -> World {
    world_on(content())
}

fn player() -> ObjectGuid {
    ObjectGuid::new(PLAYER)
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)`, added to `World.objects`.
fn spawn(w: &mut World, wcid: u32) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), guid)
    })
    .expect("constructible");
    assert!(w.objects.insert(o).is_ok());
    guid
}

/// A new object added to the player's main pack.
fn give(w: &mut World, wcid: u32) -> ObjectGuid {
    let g = spawn(w, wcid);
    assert!(container::try_add_to_inventory(
        w,
        player(),
        g,
        0,
        false,
        true
    ));
    g
}

/// `stack` Pyreals in the player's pack, with CoinValue brought up to date.
fn give_coins(w: &mut World, stack: i32) -> ObjectGuid {
    let g = give(w, COINSTACK);
    obj_mut(w, g).set_stack_size(Some(stack));
    pc::update_coin_value(w, player(), false);
    g
}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).expect("live object")
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).expect("live object")
}

/// Each sent message's kind: the game-event type inside a `0xF7B0`, else the opcode.
fn kinds(sent: &[(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)]) -> Vec<u32> {
    sent.iter()
        .map(|(_, _, b)| {
            let word = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            if word(0) == GAME_EVENT {
                word(12)
            } else {
                word(0)
            }
        })
        .collect()
}

/// A transient string event's text (a `String16L` after the event header).
fn transient_text(bytes: &[u8]) -> String {
    let p = &bytes[16..];
    let len = usize::from(u16::from_le_bytes([p[0], p[1]]));
    String::from_utf8(p[2..2 + len].to_vec()).unwrap()
}

fn transients(sent: &[(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)]) -> Vec<String> {
    sent.iter()
        .filter(|(_, _, b)| {
            b.len() > 16
                && b[..4] == GAME_EVENT.to_le_bytes()
                && b[12..16] == TRANSIENT.to_le_bytes()
        })
        .map(|(_, _, b)| transient_text(b))
        .collect()
}

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("vendors", name);
    assert!(!file.cases.is_empty(), "vendors/{name}: no cases");
    let failures: Vec<String> = file
        .cases
        .iter()
        .filter_map(|c| {
            each(c)
                .err()
                .map(|got| format!("in {} expected {} got {got}", c.input, c.output))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "vendors/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(10)].join("\n  ")
    );
}

fn opt_i32(v: &Value) -> Option<i32> {
    i64_of(v).map(|x| i32::try_from(x).expect("an int"))
}

fn item_type(v: &Value) -> Option<ItemType> {
    i64_of(v).map(|x| ItemType(u32::try_from(x).expect("an item type")))
}

/// A Vendor object built from `wcid`, in the store (no landblock).
fn vendor_of(w: &mut World, wcid: u32) -> ObjectGuid {
    spawn(w, wcid)
}

// ------------------------------------------------------------------ ACE vectors

/// `Vendor.GetSellCost(int?, ItemType?)` over every SellPrice in ACE's world database, now priced
/// as retail (V225, V225). ACE's recorded output stays the record of what ACE did: the two may
/// differ only by ACE's single-precision rounding: one pyreal up to 2^24, and above that the
/// float's spacing at the product (see [`ace_rounding_bound`]).
#[test]
fn sell_cost_is_retail_within_ace_rounding() {
    replay("sell_cost", |c| {
        let i = &c.input;
        let got = vendor::get_sell_cost_of(
            f64_of(&i["rate"]),
            opt_i32(&i["value"]),
            None,
            item_type(&i["item_type"]),
        );
        let ace = i64_of(&c.output).expect("uint");
        let bound = ace_rounding_bound(
            effective_rate(f64_of(&i["rate"]), item_type(&i["item_type"]), 1.15),
            opt_i32(&i["value"]),
        );
        if (i64::from(got) - ace).abs() <= bound {
            Ok(())
        } else {
            Err(format!("{got} (ACE {ace}, bound {bound})"))
        }
    });
}

/// `Vendor.GetBuyCost(int?, ItemType?)` over every BuyPrice in ACE's world database, now priced as
/// retail (V225, V225), within ACE's rounding of its recorded output.
#[test]
fn buy_cost_is_retail_within_ace_rounding() {
    replay("buy_cost", |c| {
        let i = &c.input;
        let got = vendor::get_buy_cost_of(
            f64_of(&i["rate"]),
            opt_i32(&i["value"]),
            None,
            item_type(&i["item_type"]),
        );
        let ace = i64_of(&c.output).expect("int");
        let bound = ace_rounding_bound(
            effective_rate(f64_of(&i["rate"]), item_type(&i["item_type"]), 1.0),
            opt_i32(&i["value"]),
        );
        if (i64::from(got) - ace).abs() <= bound {
            Ok(())
        } else {
            Err(format!("{got} (ACE {ace}, bound {bound})"))
        }
    });
}

/// The rate ACE and the client use: a trade note's fixed rate (1.15 to buy from a vendor, 1.0 to
/// sell to one) replaces the vendor's.
fn effective_rate(
    rate: Option<f64>,
    ty: Option<empyrean_entity::enums::ItemType>,
    note_rate: f64,
) -> Option<f64> {
    if ty == Some(empyrean_entity::enums::ItemType::PromissoryNote) {
        Some(note_rate)
    } else {
        rate
    }
}

/// How far ACE's `(float)rate * value` can land from the double product: the float's spacing at
/// the product, plus the one pyreal a boundary crossing costs.
fn ace_rounding_bound(rate: Option<f64>, value: Option<i32>) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let product = (rate.unwrap_or(1.0) as f32 * value.unwrap_or(0) as f32).abs();
    let spacing = f64::from(product) * f64::from(f32::EPSILON);
    #[allow(clippy::cast_possible_truncation)]
    let bound = 1 + spacing.ceil() as i64;
    bound
}

/// A stack is priced as the client prices it: its value over its size (integer division) is the
/// unit value, times the stack size. For a stack whose value is an exact multiple of the unit
/// value (what ACE keeps) that is the unit price chain, not the whole value times the rate.
#[test]
fn a_stack_is_priced_per_unit_times_count_as_the_client_does() {
    use dereth_rules::vendor::{buy_price, sell_price};
    for &(rate, unit, n) in &[
        (0.9_f64, 81, 7),
        (0.1, 19, 3),
        (1.35, 17, 100),
        (0.75, 1, 250),
    ] {
        let r: f32 = empyrean_common::dotnet::CsCast::cs_cast(rate);
        assert_eq!(
            vendor::get_buy_cost_of(Some(rate), Some(unit * n), Some(n), None),
            buy_price(unit, 0, r, n)
        );
        assert_eq!(
            i64::from(vendor::get_sell_cost_of(
                Some(rate),
                Some(unit * n),
                Some(n),
                None
            )),
            i64::from(sell_price(unit, 0, r, n))
        );
    }
    // A value that is not a multiple of the stack size loses the remainder, as in the client.
    assert_eq!(
        vendor::get_buy_cost_of(Some(1.0), Some(10), Some(3), None),
        buy_price(3, 0, 1.0, 3)
    );
}

/// `Vendor.CalculatePayoutCoinAmount` over item lists (the int sum wraps).
#[test]
fn payout_matches_ace() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    replay("payout", |c| {
        let i = &c.input;
        obj_mut(&mut w, v).set_buy_price(f64_of(&i["rate"]));
        let items: Vec<ObjectGuid> = i["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|it| {
                let g = spawn(&mut w, CUP);
                let o = obj_mut(&mut w, g);
                o.set_value(opt_i32(&it[0]));
                o.set_property(PropertyInt::ItemType, opt_i32(&it[1]).unwrap());
                g
            })
            .collect();
        let got = vendor::calculate_payout_coin_amount(&w, v, &items);
        for g in items {
            w.objects.remove(g);
        }
        // Each item at retail's price (V225), summed with ACE's wrapping int addition.
        let want = i["items"]
            .as_array()
            .unwrap()
            .iter()
            .fold(0_i32, |sum, it| {
                sum.wrapping_add(vendor::get_buy_cost_of(
                    f64_of(&i["rate"]),
                    opt_i32(&it[0]),
                    None,
                    item_type(&it[1]),
                ))
            });
        if got == want {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

/// The weenies the change-making vectors were generated against.
fn change_content() -> MemContent {
    let file = vectors::load_named("vendors", "payout_coin_stacks");
    let mut content = MemContent::new();
    for wj in file.cases[0].input["weenies"].as_array().unwrap() {
        let wcid = u32::try_from(i64_of(&wj["wcid"]).unwrap()).unwrap();
        let name = wj["class_name"].as_str().unwrap();
        let ty = WeenieType(u32::try_from(i64_of(&wj["type"]).unwrap()).unwrap());
        let mut weenie = ContentWeenie::new(wcid, name, ty).with_string(PropertyString::Name, name);
        for p in wj["ints"].as_array().unwrap() {
            weenie = weenie.with_int(
                PropertyInt(i64_of(&p[0]).unwrap().try_into().unwrap()),
                opt_i32(&p[1]).unwrap(),
            );
        }
        content = content.weenie(weenie);
    }
    content
}

/// One created stack as the vectors write it.
fn stack_of(w: &World, g: ObjectGuid) -> String {
    let o = obj(w, g);
    format!(
        "[{:?},{:?},{:?},{:?},{:?},{}]",
        o.stack_size(),
        o.value(),
        o.encumbrance_val(),
        o.palette_template(),
        o.shade(),
        o.biota.weenie_class_id
    )
}

fn stacks_json(v: &Value) -> Vec<String> {
    let opt = |x: &Value| {
        if x.is_null() {
            "None".to_owned()
        } else {
            format!("Some({})", x)
        }
    };
    let shade = |x: &Value| f64_of(x).map_or("None".to_owned(), |f| format!("Some({f:?})"));
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| {
            format!(
                "[{},{},{},{},{},{}]",
                opt(&s[0]),
                opt(&s[1]),
                opt(&s[2]),
                opt(&s[3]),
                shade(&s[4]),
                s[5]
            )
        })
        .collect()
}

/// `Player.CreatePayoutCoinStacks`: the payout split into coinstacks of at most MaxStackSize.
#[test]
fn payout_coin_stacks_match_ace() {
    let mut w = world_on(change_content());
    replay("payout_coin_stacks", |c| {
        let amount = opt_i32(&c.input["amount"]).unwrap();
        let got: Vec<String> = pc::create_payout_coin_stacks(&mut w, amount)
            .into_iter()
            .map(|g| stack_of(&w, g))
            .collect();
        let want = stacks_json(&c.output);
        if got == want {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

/// `Vendor.ItemProfileToWorldObjects`: a purchase split into stacks (or single items).
#[test]
fn item_profile_stacks_match_ace() {
    let mut w = world_on(change_content());
    replay("item_profile_stacks", |c| {
        let i = &c.input;
        let profile = ItemProfile {
            amount: opt_i32(&i["amount"]).unwrap(),
            object_guid: 0x8000_0001,
            weenie_class_id: u32::try_from(i64_of(&i["wcid"]).unwrap()).unwrap(),
            palette: opt_i32(&i["palette"]),
            shade: f64_of(&i["shade"]),
        };
        let got: Vec<String> = vendor::item_profile_to_world_objects(&mut w, &profile)
            .into_iter()
            .map(|g| stack_of(&w, g))
            .collect();
        let want = stacks_json(&c.output);
        if got == want {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

// ------------------------------------------------------------------ Vendor.cs

/// `Vendor.SetEphemeralValues`: the Vendor description flag, the Shop generator profiles dropped
/// (`vendor_shop_uses_generator` is false by default), and `OpenForBusiness` from
/// `ValidateVendorRequirements` (a vendor with no prices is closed, and says so in the property).
#[test]
fn construction_flags_validates_and_drops_shop_generators() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    let o = obj(&w, v);
    assert!(o
        .wo
        .world_object
        .object_description_flags
        .contains(ObjectDescriptionFlag::Vendor));
    assert!(o.open_for_business());
    assert_eq!(
        o.get_property(PropertyBool::OpenForBusiness),
        None,
        "true is stored as absent"
    );
    let profiles: Vec<u32> =
        o.wo.world_object_generators
            .generator_profiles
            .iter()
            .map(|p| p.biota.weenie_class_id)
            .collect();
    assert_eq!(
        profiles,
        vec![GEM],
        "the Shop profile is removed, the Scatter one kept"
    );

    let b = vendor_of(&mut w, BROKE_VENDOR);
    assert!(!obj(&w, b).open_for_business());
    assert_eq!(
        obj(&w, b).get_property(PropertyBool::OpenForBusiness),
        Some(false)
    );
}

/// `Vendor.LoadInventory` / `LoadInventoryItem`: one default item per Shop create-list row, in
/// row order, contained by the vendor, with the row's palette and shade when positive and its
/// stack size as `VendorShopCreateListStackSize`; loading twice changes nothing.
#[test]
fn load_inventory_creates_the_shop_items_once() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    vendor::load_inventory(&mut w, v);
    let items = vendor::default_items_for_sale(&w, v);
    assert_eq!(items.len(), 2);
    let wcids: Vec<u32> = items
        .iter()
        .map(|&g| obj(&w, g).biota.weenie_class_id)
        .collect();
    assert_eq!(wcids, vec![GEM, CUP]);
    for &g in &items {
        assert_eq!(obj(&w, g).container_id(), Some(v.full()));
    }
    assert_eq!(
        (
            obj(&w, items[0]).palette_template(),
            obj(&w, items[0]).shade()
        ),
        (None, None)
    );
    assert_eq!(
        (
            obj(&w, items[1]).palette_template(),
            obj(&w, items[1]).shade()
        ),
        (Some(3), Some(0.5))
    );
    assert_eq!(
        obj(&w, items[0])
            .wo
            .world_object_properties
            .vendor_shop_create_list_stack_size,
        Some(-1)
    );
    assert_eq!(
        obj(&w, items[1])
            .wo
            .world_object_properties
            .vendor_shop_create_list_stack_size,
        Some(5)
    );
    assert_eq!(
        vendor::try_get_item_for_sale(&w, v, items[1]),
        Some(items[1])
    );
    assert_eq!(vendor::try_get_item_for_sale(&w, v, player()), None);

    vendor::load_inventory(&mut w, v);
    assert_eq!(vendor::default_items_for_sale(&w, v), items);
}

/// `Vendor.AddDefaultItem` (a Shop generator spawn): tops up the first default stack of the wcid
/// that has room, else adds the item.
#[test]
fn add_default_item_tops_up_a_stack_or_adds() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    let first = spawn(&mut w, GEM);
    vendor::add_default_item(&mut w, v, first);
    assert_eq!(vendor::default_items_for_sale(&w, v), vec![first]);
    assert_eq!(obj(&w, first).container_id(), Some(v.full()));

    let second = spawn(&mut w, GEM);
    vendor::add_default_item(&mut w, v, second);
    assert_eq!(
        vendor::default_items_for_sale(&w, v),
        vec![first],
        "merged into the first stack"
    );
    assert_eq!(obj(&w, first).stack_size(), Some(2));

    let cup = spawn(&mut w, CUP);
    vendor::add_default_item(&mut w, v, cup);
    let cup2 = spawn(&mut w, CUP);
    vendor::add_default_item(&mut w, v, cup2);
    assert_eq!(
        vendor::default_items_for_sale(&w, v),
        vec![first, cup, cup2],
        "no stack: StackSize 1 is not below MaxStackSize ?? 1"
    );
}

/// `BuyItems_ValidateTransaction` with too little money: nothing is sent (the handler answers),
/// the generic items it created are destroyed and no coins are taken.
#[test]
fn a_buy_with_too_little_money_is_refused() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    vendor::load_inventory(&mut w, v);
    let gem_for_sale = vendor::default_items_for_sale(&w, v)[0];
    let coins = give_coins(&mut w, 40);
    let live = w.objects.len();

    start_capture();
    // 3 gems: one stack of value 30, SellPrice 1.5: ceil(45 - 0.1) = 45 > 40
    let mut profiles = vec![ItemProfile::new(3, gem_for_sale.full())];
    assert!(!vendor::buy_items_validate_transaction(
        &mut w,
        v,
        &mut profiles,
        player()
    ));
    assert!(take_sent().is_empty());
    assert_eq!(w.objects.len(), live, "the created stack is destroyed");
    assert_eq!(obj(&w, coins).stack_size(), Some(40));
    assert_eq!(obj(&w, v).money_income(), 0);
}

/// `BuyItems_ValidateTransaction` rejects the whole request at the first bad amount.
#[test]
fn a_buy_with_a_bad_amount_is_refused_at_once() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    vendor::load_inventory(&mut w, v);
    let gem_for_sale = vendor::default_items_for_sale(&w, v)[0];
    give_coins(&mut w, 1000);
    start_capture();
    let mut profiles = vec![
        ItemProfile::new(1, gem_for_sale.full()),
        ItemProfile::new(0, gem_for_sale.full()),
    ];
    assert!(!vendor::buy_items_validate_transaction(
        &mut w,
        v,
        &mut profiles,
        player()
    ));
    let sent = take_sent();
    assert_eq!(transients(&sent), ["Invalid amount"]);
}

/// A buy that fits: `SpendCurrency(coinstack, cost, destroy)` consumes the coins
/// (`TryConsumeFromInventoryWithNetworking`: SetStackSize, burden, CoinValue), each bought stack
/// is created in the pack (CreateObject, ContainId, burden), then the PickUpItem sound and
/// `ApproachVendor(Buy)`'s VendorInfo; the vendor's income and sold count go up.
#[test]
fn a_buy_takes_the_coins_and_gives_the_items() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    vendor::load_inventory(&mut w, v);
    let gem_for_sale = vendor::default_items_for_sale(&w, v)[0];
    let coins = give_coins(&mut w, 100);

    start_capture();
    let mut profiles = vec![ItemProfile::new(3, gem_for_sale.full())];
    assert!(vendor::buy_items_validate_transaction(
        &mut w,
        v,
        &mut profiles,
        player()
    ));
    let sent = take_sent();
    assert_eq!(
        kinds(&sent),
        [
            STACK_SIZE,
            PRIVATE_INT,
            PRIVATE_INT,
            CREATE_OBJECT,
            CONTAIN_ID,
            PRIVATE_INT,
            SOUND,
            VENDOR_INFO
        ]
    );
    assert_eq!(obj(&w, coins).stack_size(), Some(55));
    assert_eq!(obj(&w, player()).coin_value(), Some(55));
    let inventory = container::inventory_values(&w, player());
    assert_eq!(inventory.len(), 2);
    let bought = inventory[1];
    assert_eq!(
        (
            obj(&w, bought).biota.weenie_class_id,
            obj(&w, bought).stack_size()
        ),
        (GEM, Some(3))
    );
    assert_eq!(
        (obj(&w, v).money_income(), obj(&w, v).num_items_sold()),
        (45, 1)
    );
    assert_eq!(
        vendor::default_items_for_sale(&w, v).len(),
        2,
        "the shop's own item stays"
    );
    let p = obj(&w, player()).player.as_ref().unwrap();
    assert_eq!(p.player_use.last_opened_container_id, v);
    assert!(obj(&w, v).reset_timestamp().is_some(), "PrepareResetToHome");
}

/// `Player.VerifySellItems`: each refusal and its text, in ACE's order of checks; only the
/// accepted item is returned.
#[test]
fn verify_sell_items_refusals() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    let cup = give(&mut w, CUP);
    let note = give(&mut w, NOTE); // PromissoryNote is not in the vendor's merchandise types
    let worthless = give(&mut w, CUP);
    obj_mut(&mut w, worthless).set_value(Some(0));
    let pack = give(&mut w, PACK);
    let inside = spawn(&mut w, CUP);
    assert!(container::try_add_to_inventory(
        &mut w, pack, inside, 0, false, true
    ));
    let gems = give(&mut w, GEM);
    obj_mut(&mut w, gems).set_stack_size(Some(4));

    start_capture();
    let profiles = vec![
        ItemProfile::new(1, 0x7000_0000), // not in the player's possession
        ItemProfile::new(1, cup.full()),
        ItemProfile::new(1, cup.full()), // duplicate
        ItemProfile::new(0, gems.full()),
        ItemProfile::new(5, gems.full()), // more than the stack
        ItemProfile::new(1, note.full()),
        ItemProfile::new(1, worthless.full()),
        ItemProfile::new(1, pack.full()),
    ];
    let verified = pc::verify_sell_items(&mut w, player(), &profiles, v);
    assert_eq!(verified.values().copied().collect::<Vec<_>>(), vec![cup]);
    assert_eq!(
        transients(&take_sent()),
        [
            "The Note is unsellable.",
            "The Cup has no value and cannot be sold.",
            "You cannot sell that! The Pack must be empty."
        ]
    );
}

/// `Vendor.ProcessItemsForPurchase`: a plain item is kept for resale (contained by the vendor,
/// stamped `SoldTimestamp`, removed from the shard); a stackable or DestroyOnSell item is
/// destroyed; each counts in NumItemsBought; then `ApproachVendor(Sell)`.
#[test]
fn process_items_for_purchase_keeps_uniques_and_destroys_the_rest() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    let cup = spawn(&mut w, CUP);
    let gems = spawn(&mut w, GEM);
    let doomed = spawn(&mut w, CUP);
    obj_mut(&mut w, doomed).set_property(PropertyBool::DestroyOnSell, true);

    start_capture();
    vendor::process_items_for_purchase(&mut w, v, player(), &[cup, gems, doomed]);
    assert_eq!(kinds(&take_sent()), [VENDOR_INFO]);
    assert_eq!(vendor::unique_items_for_sale(&w, v), vec![cup]);
    assert_eq!(obj(&w, cup).container_id(), Some(v.full()));
    assert_eq!(obj(&w, cup).sold_timestamp(), Some(T0));
    assert!(w.objects.get(gems).is_none() && w.objects.get(doomed).is_none());
    assert_eq!(obj(&w, v).num_items_bought(), 3);
}

/// `Vendor.RotUniques` (run by `ApproachVendor`): a unique item older than
/// `vendor_unique_rot_time` (300 s) is taken off the list and destroyed; a younger one stays.
#[test]
fn unique_items_rot_after_the_rot_time() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    let old = spawn(&mut w, CUP);
    let young = spawn(&mut w, CUP);
    vendor::process_items_for_purchase(&mut w, v, player(), &[old]);
    w.now = now_at(T0 + 100.0);
    vendor::process_items_for_purchase(&mut w, v, player(), &[young]);
    assert_eq!(vendor::unique_items_for_sale(&w, v), vec![old, young]);

    w.now = now_at(T0 + 300.0);
    vendor::rot_uniques(&mut w, v);
    assert_eq!(
        vendor::unique_items_for_sale(&w, v),
        vec![young],
        "300 s after the sale: rotted (>=)"
    );
    assert!(w.objects.get(old).is_none());
}

/// A unique item bought back: it moves from the vendor's list into the pack, loses its
/// SoldTimestamp, and costs its own value at the SellPrice.
#[test]
fn a_unique_item_can_be_bought_back() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    let cup = spawn(&mut w, CUP);
    vendor::process_items_for_purchase(&mut w, v, player(), &[cup]);
    give_coins(&mut w, 200);

    let mut profiles = vec![ItemProfile::new(1, cup.full())];
    assert!(vendor::buy_items_validate_transaction(
        &mut w,
        v,
        &mut profiles,
        player()
    ));
    assert!(vendor::unique_items_for_sale(&w, v).is_empty());
    assert!(container::inventory_values(&w, player()).contains(&cup));
    assert_eq!(obj(&w, cup).sold_timestamp(), None);
    assert_eq!(
        obj(&w, player()).coin_value(),
        Some(50),
        "ceil(1.5 * 100 - 0.1) = 150"
    );
}

/// `Vendor.CheckClose` when the player is out of UseRadius (no physics bodies here: the distance
/// is unbounded): LastOpenedContainerId is cleared and the tracking stops.
#[test]
fn check_close_forgets_a_player_who_walked_away() {
    let mut w = world();
    let v = vendor_of(&mut w, VENDOR_WCID);
    obj_mut(&mut w, v).set_property(PropertyFloat::UseRadius, 3.0);
    // a player on no landblock is one who logged out: forgotten silently
    vendor::fields_mut(&mut w, v).last_player_info =
        Some(empyrean_world::entity::world_object_info::WorldObjectInfo::new(&w, player()));
    vendor::check_close(&mut w, v);
    assert!(vendor::fields(&w, v).last_player_info.is_none());

    // on a landblock and out of range
    obj_mut(&mut w, player()).current_landblock =
        Some(empyrean_entity::LandblockId::new(0xA9B4_FFFF));
    obj_mut(&mut w, player())
        .player
        .as_mut()
        .unwrap()
        .player_use
        .last_opened_container_id = v;
    vendor::fields_mut(&mut w, v).last_player_info =
        Some(empyrean_world::entity::world_object_info::WorldObjectInfo::new(&w, player()));
    vendor::check_close(&mut w, v);
    assert!(vendor::fields(&w, v).last_player_info.is_none());
    assert_eq!(
        obj(&w, player())
            .player
            .as_ref()
            .unwrap()
            .player_use
            .last_opened_container_id,
        ObjectGuid::INVALID
    );
}

// ------------------------------------------------------------------ Player_Commerce.cs

/// `Player.UpdateCoinValue`: the sum of the coin stacks' Value; the private update only when it
/// changed (and never with `sendUpdateMessageIfChanged = false`).
#[test]
fn update_coin_value_sends_only_on_change() {
    let mut w = world();
    let coins = give(&mut w, COINSTACK);
    obj_mut(&mut w, coins).set_stack_size(Some(30));
    start_capture();
    pc::update_coin_value(&mut w, player(), true);
    assert_eq!(kinds(&take_sent()), [PRIVATE_INT]);
    assert_eq!(obj(&w, player()).coin_value(), Some(30));
    pc::update_coin_value(&mut w, player(), true);
    assert!(take_sent().is_empty(), "unchanged");
    obj_mut(&mut w, coins).set_stack_size(Some(31));
    pc::update_coin_value(&mut w, player(), false);
    assert!(take_sent().is_empty());
    assert_eq!(obj(&w, player()).coin_value(), Some(31));
}

/// `Player.SpendCurrency(wcid, amount)` without destroy (the death drop): whole stacks are taken
/// and removed; the last is split (`CollectCurrencyStacks`: a new stack of the remainder, the old
/// one adjusted: SetStackSize and burden), and the split stack, never in the inventory, updates
/// CoinValue. More than CoinValue answers null.
#[test]
fn spend_currency_takes_whole_stacks_and_splits_the_last() {
    let mut w = world();
    give_coins(&mut w, 10);
    give_coins(&mut w, 10);
    assert_eq!(obj(&w, player()).coin_value(), Some(20));
    // `GetInventoryItemsOfWCID` walks the pack by PlacementPosition (the newest stack is at 0)
    let order = container::get_inventory_items_of_wcid(&w, player(), COINSTACK);

    start_capture();
    assert_eq!(
        pc::spend_currency(&mut w, player(), COINSTACK, 21, false),
        None
    );
    let taken = pc::spend_currency(&mut w, player(), COINSTACK, 15, false).expect("a list");
    assert_eq!(taken.len(), 2);
    assert_eq!(taken[0], order[0], "the first stack, whole");
    assert_eq!(
        obj(&w, taken[1]).stack_size(),
        Some(5),
        "a new stack of the remaining 5"
    );
    assert_eq!(
        obj(&w, order[1]).stack_size(),
        Some(5),
        "the second stack, adjusted"
    );
    assert_eq!(container::inventory_values(&w, player()), vec![order[1]]);
    assert_eq!(obj(&w, player()).coin_value(), Some(5));
    // CollectCurrencyStacks' split (SetStackSize, burden), then the whole stack's
    // TryRemoveFromInventoryWithNetworking(SpendItem) (Container cleared, InventoryRemoveObject,
    // burden, CoinValue 20 -> 5), then
    // the split stack's failed removal: UpdateCoinValue, unchanged, sends nothing
    assert_eq!(
        kinds(&take_sent()),
        [
            STACK_SIZE,
            PRIVATE_INT,
            INSTANCE_ID,
            REMOVE_OBJECT,
            PRIVATE_INT,
            PRIVATE_INT
        ]
    );
    assert_eq!(pc::spend_currency(&mut w, player(), 0, 5, false), None);
    assert_eq!(
        pc::spend_currency(&mut w, player(), COINSTACK, 0, false),
        None
    );
}

/// `UpdateCoinValue(false)` in the `Player` constructor: CoinValue is the sum of the coin stacks
/// sorted into the inventory (a stored CoinValue is overwritten), wielded items and other items
/// aside, and nothing is sent.
#[test]
fn the_player_constructor_sums_its_coins() {
    let mut w = world();
    let mut shard = Vec::new();
    for (wcid, stack) in [(COINSTACK, 30_i32), (GEM, 4), (COINSTACK, 12)] {
        let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
        let mut o = CtorEnv::with_world(&w, |env| {
            factory::create_world_object(
                env,
                Some(weenie),
                ObjectGuid::new(0x8000_0100 + stack.cast_unsigned()),
            )
        })
        .expect("an item");
        o.set_stack_size(Some(stack));
        o.set_container_id(Some(PLAYER));
        shard.push(
            empyrean_store::adapter::biota_converter::BiotaConverter::convert_from_entity_biota(
                &o.biota, false,
            ),
        );
    }
    let mut biota = obj(&w, player()).biota.clone();
    biota
        .properties_int
        .get_or_insert_with(Default::default)
        .insert(PropertyInt::CoinValue, 999);
    start_capture();
    let p = CtorEnv::with_world(&w, |env| {
        empyrean_world::world_objects::player::player_from_biota_with_character(
            env,
            Class::Player,
            biota,
            shard,
            Vec::new(),
            empyrean_store::models::shard::Character::default(),
            None,
        )
    });
    // The constructor's tail runs once the player is in the store (it replaces the fixture's).
    w.objects.remove(player());
    w.objects.insert(p).expect("fresh");
    empyrean_world::world_objects::player::player_ctor_load_possessions(&mut w, player());
    assert!(take_sent().is_empty());
    assert_eq!(obj(&w, player()).coin_value(), Some(42));
}

// Cross-checks against the shared client rules.

/// the server's `GetBuyCost`/`GetSellCost` against the client's price formulas
/// (`dereth_rules::vendor`), at every rate in ACE's world database and unit values 1..=100,000.
/// Under ACE's single-precision product they disagreed by one pyreal in 19,215 buy and 15,403
/// sell cases (the first: rate 0.1, value 19, ACE 1, the client 2). The server uses retail rounding
/// (V225, 2026-09-23; DIVERGENCES V225), so the window's price and the charge now always agree.
#[test]
fn rule3_vendor_prices_agree_with_dereth_rules() {
    use dereth_rules::vendor::{buy_price, sell_price};
    let rates = [
        0.0, 0.001, 0.1, 0.2, 0.5, 0.6, 0.7, 0.75, 0.8, 0.85, 0.9, 0.95, 1.0, 1.05, 1.1, 1.15, 1.2,
        1.25, 1.35, 1.4, 1.45, 1.5, 1.55, 1.6, 1.7, 1.8, 1.9, 2.0, 10.0, 20.0, 50.0,
    ];
    let mut bad = Vec::new();
    for &r in &rates {
        let rate: f32 = empyrean_common::dotnet::CsCast::cs_cast(r);
        for v in 1..=100_000 {
            let (ace, client) = (
                vendor::get_buy_cost_of(Some(r), Some(v), None, None),
                buy_price(v, 0, rate, 1),
            );
            if ace != client {
                bad.push(format!(
                    "buy rate {r} value {v}: ACE {ace}, client {client}"
                ));
            }
            let (ace, client) = (
                i64::from(vendor::get_sell_cost_of(Some(r), Some(v), None, None)),
                i64::from(sell_price(v, 0, rate, 1)),
            );
            if ace != client {
                bad.push(format!(
                    "sell rate {r} value {v}: ACE {ace}, client {client}"
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} disagreements:\n  {}",
        bad.len(),
        bad.iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
