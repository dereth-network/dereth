//! Vectors: fixtures/vectors/housing/
//! Rent period, hook-group limits, HouseManager coords/guids, purchase/rent replay ACE housing
//! vectors.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::vectors::{self, f64_of, i64_of, Case};
use empyrean_content::models::world::{
    LandblockInstance, LandblockInstanceLink, Weenie, WeeniePropertiesCreateList,
};
use empyrean_content::MemContent;
use empyrean_entity::enums::{
    DestinationType, HookGroupType, HouseType, ItemType, PropertyDataId, PropertyInstanceId,
    PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_testkit::{land, TestServer};
use empyrean_world::entity::player_house::PlayerHouse;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::{house_manager, landblock_manager};
use empyrean_world::network::structure::house_payment::HousePayment;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{container, house, player_house};
use empyrean_world::World;
use serde_json::Value;

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("housing", name);
    assert!(!file.cases.is_empty(), "housing/{name}: no cases");
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
        "housing/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(10)].join("\n  ")
    );
}

fn u32_of(v: &Value) -> u32 {
    u32::try_from(i64_of(v).expect("an integer")).expect("a uint")
}

fn i32_of(v: &Value) -> i32 {
    i32::try_from(i64_of(v).expect("an integer")).expect("an int")
}

fn opt_i32(v: &Value) -> Option<i32> {
    i64_of(v).map(|x| i32::try_from(x).expect("an int"))
}

fn throws(v: &Value) -> Option<&str> {
    vectors::throws(v)
}

// ------------------------------------------------------------------ static vectors

/// `House.GetRentTimestamp` / `GetRentDue` over purchase times, house types and clocks.
#[test]
fn rent_period_matches_ace() {
    replay("rent_timestamps", |c| {
        let i = &c.input;
        let now = f64_of(&i["now"]).expect("now");
        let apartment = i32_of(&i["house_type"]) == HouseType::Apartment.0;
        let purchase = u32_of(&i["purchase"]);
        let got = (
            house::get_rent_timestamp_at(now, apartment, purchase),
            house::get_rent_due_at(now, apartment, purchase),
        );
        let want = (u32_of(&c.output["timestamp"]), u32_of(&c.output["due"]));
        if got == want {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

/// `House.GetHookGroupMaxCount` (the `HookGroupLimits` table), including the missing keys.
#[test]
fn hook_group_limits_match_ace() {
    replay("hook_group_max", |c| {
        let i = &c.input;
        let (t, g) = (
            HouseType(i32_of(&i["house_type"])),
            HookGroupType(i32_of(&i["hook_group"])),
        );
        let got = catch_unwind(AssertUnwindSafe(|| house::hook_group_limit(t, g)));
        match (got, throws(&c.output)) {
            (Ok(v), None) if v == i32_of(&c.output) => Ok(()),
            (Err(_), Some("System.Collections.Generic.KeyNotFoundException")) => Ok(()),
            (Ok(v), _) => Err(v.to_string()),
            (Err(_), _) => Err("a panic".to_owned()),
        }
    });
}

/// `HouseManager.GetCoords(Position)`: map coordinates outdoors, the apartment block's name
/// indoors (or none), then the position.
#[test]
fn coords_match_ace() {
    replay("coords", |c| {
        let i = &c.input;
        // the harness writes each float widened to a double: narrowing it back is exact
        #[allow(clippy::cast_possible_truncation)]
        let f = |k: &str| f64_of(&i[k]).expect("a float") as f32;
        let p = Position::from_components(
            u32_of(&i["cell"]),
            f("x"),
            f("y"),
            f("z"),
            f("qx"),
            f("qy"),
            f("qz"),
            f("qw"),
            false,
        );
        let got = house_manager::get_coords(&p);
        if Some(got.as_str()) == c.output.as_str() {
            Ok(())
        } else {
            Err(got)
        }
    });
}

/// `HouseManager.GetHouseGuid`: the house guid sharing the slumlord's landblock prefix.
#[test]
fn house_guid_matches_ace() {
    replay("house_guid", |c| {
        let houses: Vec<u32> = c.input["houses"]
            .as_array()
            .unwrap()
            .iter()
            .map(u32_of)
            .collect();
        let got = house_manager::get_house_guid(u32_of(&c.input["slumlord"]), &houses);
        if got == u32_of(&c.output) {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

const PYREAL: u32 = 273;
const GEM: u32 = 30001;
const TRADE_NOTES: [(u32, &str); 5] = [
    (2621, "tradenote100"),
    (2622, "tradenote500"),
    (2623, "tradenote1000"),
    (2624, "tradenote5000"),
    (2625, "tradenote10000"),
];

fn item_content() -> MemContent {
    let mut c = MemContent::new()
        .weenie(Weenie::new(PYREAL, "coinstack", WeenieType::Generic))
        .weenie(Weenie::new(GEM, "gemamber", WeenieType::Generic))
        .weenie(house_weenie());
    for (wcid, name) in TRADE_NOTES {
        c = c.weenie(Weenie::new(wcid, name, WeenieType::Generic));
    }
    c
}

fn house_weenie() -> Weenie {
    Weenie::new(COTTAGE, "housecottage1234", WeenieType::House)
        .with_int(PropertyInt::HouseType, HouseType::Cottage.0)
}

fn world_with(content: MemContent) -> World {
    let mut w = World::new(
        empyrean_common::clock::ClockSnapshot::take(
            &empyrean_common::clock::VirtualClock::default(),
            0.0,
        ),
        empyrean_testkit::dats::with_stat_tables(empyrean_dat::FakeDats::new())
            .build()
            .expect("fake dats"),
    );
    w.content = Arc::new(content);
    w
}

/// A generic object of `wcid` built as the harness builds one: only the listed properties.
fn detached(w: &World, guid: u32, j: &Value) -> WorldObject {
    let wcid = u32_of(&j["wcid"]);
    let weenie = w.content.get_cached_weenie(wcid).expect("a test weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), ObjectGuid::new(guid))
    })
    .expect("constructible");
    for (p, k) in [
        (PropertyInt::ItemType, "item_type"),
        (PropertyInt::StackSize, "stack_size"),
        (PropertyInt::Value, "value"),
        (PropertyInt::StackUnitValue, "stack_unit_value"),
    ] {
        match j.get(k).and_then(opt_i32) {
            Some(v) => o.set_property(p, v),
            None => o.remove_property(p),
        }
    }
    if let Some(name) = j.get("name").and_then(Value::as_str) {
        o.set_property(PropertyString::Name, name.to_owned());
    }
    o
}

/// Puts the case's sent items into the store (removing the previous case's), in order.
fn place_items(w: &mut World, items: &Value, previous: &mut Vec<ObjectGuid>) -> Vec<ObjectGuid> {
    for g in previous.drain(..) {
        w.objects.remove(g);
    }
    let mut out = Vec::new();
    for j in items.as_array().unwrap() {
        let guid = u32_of(&j["guid"]);
        let mut o = detached(w, guid, j);
        o.set_property(PropertyString::Name, format!("Item {guid:08X}"));
        w.objects.insert(o).expect("fresh");
        out.push(ObjectGuid::new(guid));
    }
    previous.extend(out.iter().copied());
    out
}

/// V334 (V334, retail): a house's CreateObject carries its restriction effect (data id 44, blue
/// 152 where the weenie gives none) as the description's script, and no physics default script.
/// ACE set data id 30 to 152 on every house and sent it in both places.
#[test]
fn a_houses_create_object_carries_its_restriction_effect_as_the_description_script() {
    use empyrean_world::network::game_messages::messages::game_message_create_object::game_message_create_object;
    // Apartments, so the writer needs no landblock for the house's permissions.
    let (plain, gold) = (0x9000, 0x9001);
    let apartment = |wcid: u32| {
        Weenie::new(wcid, "houseapartment", WeenieType::House)
            .with_int(PropertyInt::HouseType, HouseType::Apartment.0)
    };
    let mut w = world_with(
        MemContent::new()
            .weenie(apartment(plain))
            .weenie(apartment(gold).with_did(PropertyDataId::RestrictionEffect, 154)),
    );
    for (guid, wcid, want) in [(0x7A9B_4002u32, plain, 152u16), (0x7A9B_4003, gold, 154)] {
        let weenie = w.content.get_cached_weenie(wcid).expect("a test weenie");
        let o = CtorEnv::with_world(&w, |env| {
            factory::create_world_object(env, Some(weenie), ObjectGuid::new(guid))
        })
        .expect("constructible");
        assert_eq!(
            o.get_property(PropertyDataId::PhysicsScript),
            None,
            "wcid {wcid}: no data id 30"
        );
        w.objects.insert(o).expect("fresh");
        let m = game_message_create_object(&mut w, ObjectGuid::new(guid), false, false);
        let d = dereth_protocol::read_body_padded::<dereth_protocol::objects::ItemCreateObject>(
            &m.data[4..],
        )
        .expect("decodes")
        .0;
        assert_eq!(
            (d.wdesc.pscript, d.physicsdesc.default_script),
            (Some(want), None),
            "wcid {wcid}"
        );
    }
}

/// `HousePayment.GetConsumeItems`: smallest stacks first, trade notes after the pyreals by
/// denomination, the last item partly (a trade note rounded up by its unit value).
#[test]
fn consume_items_match_ace() {
    let mut w = world_with(item_content());
    let mut previous = Vec::new();
    replay("consume_items", |c| {
        let i = &c.input;
        let items = place_items(&mut w, &i["items"], &mut previous);
        let payment = HousePayment {
            num: i32_of(&i["num"]),
            paid: i32_of(&i["paid"]),
            weenie_id: u32_of(&i["wcid"]),
            ..HousePayment::default()
        };
        let got: Vec<(u32, i32)> = payment
            .get_consume_items(&w, &items)
            .iter()
            .map(|x| (x.base.guid.full(), x.value.unwrap_or(0)))
            .collect();
        let want: Vec<(u32, i32)> = c
            .output
            .as_array()
            .expect("a list")
            .iter()
            .map(|p| (u32_of(&p[0]), i32_of(&p[1])))
            .collect();
        if got == want {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

/// `Player.HasItems`: pyreals and trade notes cover a "Pyreal" row by value; other rows need the
/// wcid's total stack.
#[test]
fn has_items_matches_ace() {
    let mut w = world_with(item_content());
    let mut previous = Vec::new();
    replay("has_items", |c| {
        let sent = place_items(&mut w, &c.input["sent"], &mut previous);
        let buy: Vec<WorldObject> = c.input["buy"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(k, j)| detached(&w, 0x8010_0000 + u32::try_from(k).unwrap(), j))
            .collect();
        let got = player_house::has_items(&w, &sent, &buy);
        if Some(got) == c.output.as_bool() {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

const COTTAGE: u32 = 20;

/// `House.HasPermission`: the owner; guests (storage for storage); the open flag for guest access
/// only. PlayerManager finds no one and the player is in no allegiance, as in the vectors.
#[test]
fn has_permission_matches_ace() {
    let mut w = world_with(item_content());
    let house_guid = ObjectGuid::new(0x7A9B_4001);
    replay("has_permission", |c| {
        let i = &c.input;
        w.objects.remove(house_guid);
        let weenie = w.content.get_cached_weenie(COTTAGE).expect("house weenie");
        let mut h = CtorEnv::with_world(&w, |env| {
            factory::create_world_object(env, Some(weenie), house_guid)
        })
        .expect("a house");
        if let Some(o) = i64_of(&i["owner"]) {
            h.set_property(PropertyInstanceId::HouseOwner, u32::try_from(o).unwrap());
        }
        if let Some(m) = i64_of(&i["monarch"]) {
            h.set_property(PropertyInstanceId::Monarch, u32::try_from(m).unwrap());
        }
        h.set_open_to_everyone(i["open"].as_bool().unwrap());
        w.objects.insert(h).expect("fresh");
        let guests = &mut house::fields_mut(&mut w, house_guid).guests;
        for g in i["guests"].as_array().unwrap() {
            guests.insert(ObjectGuid::new(u32_of(&g[0])), g[1].as_bool().unwrap());
        }
        let got = house::has_permission(
            &w,
            house_guid,
            ObjectGuid::new(u32_of(&i["player"])),
            i["storage"].as_bool().unwrap(),
        );
        if Some(got) == c.output.as_bool() {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

#[test]
fn house_ids_parse_from_class_names() {
    // `uint.TryParse(Regex.Match(classname, @"\d+").Value)`
    assert_eq!(
        house_manager::parse_house_id("housecottage1234"),
        Some(1234)
    );
    assert_eq!(
        house_manager::parse_house_id("slumlord12villa345"),
        Some(12)
    );
    assert_eq!(house_manager::parse_house_id("housemansion"), None);
    assert_eq!(
        house_manager::parse_house_id("house99999999999"),
        None,
        "overflows a uint"
    );
}

// ------------------------------------------------------------------ the rent queue

#[test]
fn rent_queue_orders_by_rent_due_then_house_guid() {
    let at = |s: f64, h: u32| PlayerHouse {
        account_id: 1,
        player_guid: 0x5000_0001,
        player_name: None,
        house: ObjectGuid::new(h),
        rent_due: DotNetDateTime::UNIX_EPOCH.add_seconds(s),
    };
    let (a, b, c) = (
        at(100.0, 0x7000_0002),
        at(100.0, 0x7000_0001),
        at(50.0, 0x7000_0009),
    );
    assert_eq!(
        a.compare_to(&b),
        std::cmp::Ordering::Greater,
        "equal due times: the house guid decides"
    );
    assert_eq!(c.compare_to(&b), std::cmp::Ordering::Less);
    assert_eq!(a.compare_to(&a.clone()), std::cmp::Ordering::Equal);
}

// ------------------------------------------------------------------ offline copies

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

const LB: u16 = 0xA9B5;
const CELL: u32 = 0xA9B5_0019;
const SLUMLORD: u32 = 21;
const HOOK: u32 = 22;
const HOUSE_GUID: u32 = 0x7A9B_5001;
const SLUMLORD_GUID: u32 = 0x7A9B_5002;
const HOOK_GUID: u32 = 0x7A9B_5003;
const OWNER: u32 = 0x5000_0007;

fn house_content() -> MemContent {
    let thing = |wcid: u32, class: &str, t: WeenieType| {
        Weenie::new(wcid, class, t)
            .with_string(PropertyString::Name, class)
            .with_did(PropertyDataId::Setup, land::TEST_SETUP)
    };
    let mut slumlord = thing(SLUMLORD, "slumlordcottage1234", WeenieType::SlumLord)
        .with_did(PropertyDataId::HouseId, 1234);
    slumlord.weenie_properties_create_list = vec![WeeniePropertiesCreateList {
        id: 1,
        object_id: SLUMLORD,
        destination_type: i8::try_from(DestinationType::HouseRent.0).unwrap(),
        weenie_class_id: PYREAL,
        stack_size: 50,
        ..Default::default()
    }];
    let mut h = LandblockInstance::new(HOUSE_GUID, COTTAGE, CELL, [80.0, 10.0, 94.0]);
    h.landblock_instance_link = vec![
        LandblockInstanceLink {
            id: 1,
            parent_guid: HOUSE_GUID,
            child_guid: SLUMLORD_GUID,
            ..Default::default()
        },
        LandblockInstanceLink {
            id: 2,
            parent_guid: HOUSE_GUID,
            child_guid: HOOK_GUID,
            ..Default::default()
        },
    ];
    MemContent::new()
        .weenie(
            house_weenie()
                .with_did(PropertyDataId::Setup, land::TEST_SETUP)
                .with_did(PropertyDataId::HouseId, 1234),
        )
        .weenie(slumlord)
        .weenie(thing(HOOK, "hook", WeenieType::Hook))
        .weenie(
            thing(PYREAL, "coinstack", WeenieType::Coin)
                .with_string(PropertyString::Name, "Pyreal")
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::Money.0).unwrap(),
                )
                .with_int(PropertyInt::MaxStackSize, 25000)
                .with_int(PropertyInt::StackUnitValue, 1),
        )
        .landblock_instance(h)
        .landblock_instance(LandblockInstance {
            is_link_child: true,
            ..LandblockInstance::new(SLUMLORD_GUID, SLUMLORD, CELL, [82.0, 12.0, 94.0])
        })
        .landblock_instance(LandblockInstance {
            is_link_child: true,
            ..LandblockInstance::new(HOOK_GUID, HOOK, CELL, [84.0, 12.0, 94.0])
        })
}

/// `House.Load` on an unloaded landblock: the house and its links come from the world instances,
/// the slumlord from its shard biota, whose paid item loads into it; `HouseManager.GetHouse` waits
/// for that load (`RegisterCallback`), and the rent reads as paid. When the landblock loads, the
/// copy leaves the store and the landblock's own objects take the guids.
#[test]
fn an_offline_copy_loads_with_its_paid_rent_then_gives_way_to_the_landblock() {
    let mut ts = TestServer::new();
    ts.world.content = Arc::new(house_content());
    land::use_flat_land_with_test_setup(&mut ts.world, &[LB], 47);
    house_manager::initialize(&mut ts.world);
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    let w = &mut ts.world;

    // the shard: the owned house, and its slumlord holding the rent
    let weenie = w.content.get_cached_weenie(COTTAGE).unwrap();
    let mut owned = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), ObjectGuid::new(HOUSE_GUID))
    })
    .unwrap();
    owned.set_property(PropertyInstanceId::HouseOwner, OWNER);
    w.shard.save_biota(owned.biota.clone(), None);
    let weenie = w.content.get_cached_weenie(SLUMLORD).unwrap();
    let mut slumlord = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), ObjectGuid::new(SLUMLORD_GUID))
    })
    .unwrap();
    slumlord.set_property(PropertyInstanceId::HouseOwner, OWNER);
    w.shard.save_biota(slumlord.biota.clone(), None);
    let weenie = w.content.get_cached_weenie(PYREAL).unwrap();
    let mut coins = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), ObjectGuid::new(0x8000_0100))
    })
    .unwrap();
    coins.set_stack_size(Some(50));
    coins.set_property(PropertyInstanceId::Container, SLUMLORD_GUID);
    w.shard.save_biota(coins.biota.clone(), None);

    let lb_id = house::landblock_id_of_house_guid(HOUSE_GUID);
    assert!(!landblock_manager::is_loaded(w, lb_id));

    let seen: Arc<Mutex<Vec<u32>>> = Arc::default();
    let s = Arc::clone(&seen);
    house_manager::get_house(
        w,
        HOUSE_GUID,
        Box::new(move |_w, h| s.lock().unwrap().push(h.full())),
    );

    let h = ObjectGuid::new(HOUSE_GUID);
    assert!(house_manager::is_offline_copy(w, h), "an offline copy");
    assert_eq!(w.objects.get(h).unwrap().current_landblock, None);
    assert_eq!(house::slum_lord(w, h), Some(ObjectGuid::new(SLUMLORD_GUID)));
    assert_eq!(house::hooks(w, h), vec![ObjectGuid::new(HOOK_GUID)]);
    assert_eq!(
        w.objects
            .get(ObjectGuid::new(SLUMLORD_GUID))
            .unwrap()
            .house_owner(),
        Some(OWNER),
        "the house and its slumlord are the shard's"
    );
    assert!(
        seen.lock().unwrap().is_empty(),
        "the callback waits for the slumlord's inventory"
    );

    ts.advance(0.2);
    let w = &mut ts.world;
    assert_eq!(*seen.lock().unwrap(), vec![HOUSE_GUID], "then runs once");
    assert_eq!(
        container::inventory_values(w, ObjectGuid::new(SLUMLORD_GUID)),
        vec![ObjectGuid::new(0x8000_0100)]
    );
    assert!(
        empyrean_world::world_objects::slum_lord::is_rent_paid(w, ObjectGuid::new(SLUMLORD_GUID)),
        "50 of 50 paid"
    );

    // a second Load reuses the copy
    assert_eq!(house::load(w, HOUSE_GUID, false), Some(h));

    // the landblock loads: the copy gives way
    landblock_manager::get_landblock(w, lb_id, false, false);
    ts.advance(0.5);
    let w = &ts.world;
    assert!(!house_manager::is_offline_copy(w, h));
    assert!(
        w.objects
            .get(h)
            .is_some_and(|o| o.current_landblock.is_some()),
        "the landblock's house"
    );
    assert!(
        w.objects
            .get(ObjectGuid::new(HOOK_GUID))
            .is_some_and(|o| o.current_landblock.is_some()),
        "and its links"
    );
    assert_eq!(house::slum_lord(w, h), Some(ObjectGuid::new(SLUMLORD_GUID)));
}

// ------------------------------------------------------------------ real content

#[cfg(feature = "real-content")]
mod real {
    use std::time::Instant;

    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};

    use super::*;

    fn server() -> TestServer {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        let dats = DatManager::initialize(Arc::new(source)).expect("retail dats");
        let path = empyrean_common::test_paths::world_pack();
        let pack = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        });
        let mut ts = TestServer::with_dats(dats);
        ts.world.content = Arc::new(pack);
        guid_manager::initialize(&mut ts.world, &mut EmptyShard);
        ts
    }

    /// `HouseManager.Initialize` over ACE's world database, then every house of one landblock
    /// loads with its links (a slumlord each), and an unloaded house loads as an offline copy.
    #[test]
    fn real_houses_load_and_link() {
        let mut ts = server();
        let t = Instant::now();
        house_manager::initialize(&mut ts.world);
        eprintln!("HouseManager.Initialize: {:?}", t.elapsed());
        let ids = ts
            .world
            .house_manager
            .house_id_to_guid
            .as_ref()
            .expect("built");
        assert!(ids.len() > 1000, "{} house ids", ids.len());

        // the root houses of landblock 0x9DAF (two villas; see the shared crate's barrier notes)
        let roots: Vec<u32> = empyrean_tables::house_cell::ROOT_GUIDS
            .entries()
            .iter()
            .filter(|(k, v)| k == v && (k >> 12) & 0xFFFF == 0x9DAF)
            .map(|(k, _)| *k)
            .collect();
        assert!(!roots.is_empty());
        let lb = house::landblock_id_of_house_guid(roots[0]);
        landblock_manager::get_landblock(&mut ts.world, lb, false, false);
        ts.advance(1.0);
        for &r in &roots {
            let h = ObjectGuid::new(r);
            let w = &ts.world;
            assert!(
                w.objects
                    .get(h)
                    .is_some_and(|o| o.is_house() && o.current_landblock.is_some()),
                "house {r:08X} is on its landblock"
            );
            assert!(
                house::slum_lord(w, h).is_some(),
                "house {r:08X} has its slumlord"
            );
            assert!(w.objects.get(h).unwrap().house_type() != HouseType::Undef);
        }

        // a house elsewhere, not loaded: an offline copy (the house on a slumlord's landblock, as
        // GetHouseGuid finds it)
        let all: Vec<u32> = ts
            .world
            .house_manager
            .house_id_to_guid
            .as_ref()
            .unwrap()
            .values()
            .flatten()
            .copied()
            .collect();
        let other = ts
            .world
            .content
            .get_houses_all()
            .into_iter()
            .filter(|r| (r.landblock_instance.guid >> 12) & 0xFFFF != 0x9DAF)
            .map(|r| house_manager::get_house_guid(r.landblock_instance.guid, &all))
            .find(|&g| g != 0)
            .expect("a house beside a slumlord");
        let copy = house::load(&mut ts.world, other, false);
        assert_eq!(copy, Some(ObjectGuid::new(other)));
        assert!(house_manager::is_offline_copy(
            &ts.world,
            ObjectGuid::new(other)
        ));
    }
}

/// `Container(Biota)` (Container.cs): a storage chest restored from its saved biota when its
/// landblock loads loads its own saved inventory from the shard (`GetInventoryInParallel`, then
/// `SortBiotasIntoInventory` on its queue), as the insertion hook starts it.
#[test]
fn a_saved_storage_chest_reloads_its_contents() {
    const STORAGE: u32 = 23;
    const STORAGE_GUID: u32 = 0x7A9B_5010;
    let mut ts = TestServer::new();
    ts.world.content = Arc::new(
        house_content()
            .weenie(
                Weenie::new(STORAGE, "storage", WeenieType::Storage)
                    .with_string(PropertyString::Name, "storage")
                    .with_did(PropertyDataId::Setup, land::TEST_SETUP),
            )
            .landblock_instance(LandblockInstance::new(
                STORAGE_GUID,
                STORAGE,
                CELL,
                [86.0, 12.0, 94.0],
            )),
    );
    land::use_flat_land_with_test_setup(&mut ts.world, &[LB], 47);
    house_manager::initialize(&mut ts.world);
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    let w = &mut ts.world;

    // the shard: the chest, and the coins saved inside it
    let weenie = w.content.get_cached_weenie(STORAGE).unwrap();
    let chest = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), ObjectGuid::new(STORAGE_GUID))
    })
    .unwrap();
    w.shard.save_biota(chest.biota.clone(), None);
    let weenie = w.content.get_cached_weenie(PYREAL).unwrap();
    let mut coins = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), ObjectGuid::new(0x8000_0200))
    })
    .unwrap();
    coins.set_stack_size(Some(75));
    coins.set_property(PropertyInstanceId::Container, STORAGE_GUID);
    w.shard.save_biota(coins.biota.clone(), None);

    let lb_id = house::landblock_id_of_house_guid(HOUSE_GUID);
    landblock_manager::get_landblock(w, lb_id, false, false);
    ts.advance(0.5);
    let w = &ts.world;
    let chest = ObjectGuid::new(STORAGE_GUID);
    assert!(
        w.objects
            .get(chest)
            .is_some_and(|o| o.current_landblock.is_some()),
        "the chest is on its landblock"
    );
    assert!(
        w.objects
            .get(chest)
            .unwrap()
            .wo
            .world_object_database
            .biota_originated_from_database,
        "restored from the shard"
    );
    assert_eq!(
        container::inventory_values(w, chest),
        vec![ObjectGuid::new(0x8000_0200)]
    );
    assert!(
        w.objects
            .get(chest)
            .unwrap()
            .container
            .as_ref()
            .unwrap()
            .container
            .inventory_loaded
    );
    assert_eq!(
        w.objects
            .get(ObjectGuid::new(0x8000_0200))
            .unwrap()
            .stack_size(),
        Some(75)
    );
}
