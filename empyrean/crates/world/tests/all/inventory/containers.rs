//! Vectors: fixtures/vectors/containers/
//! Container (+tick), Stackable, equipment and monster inventory replay ACE containers vectors
//! and code.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::sort::{compare_to, list_sort};
use empyrean_common::not_ported::take_local;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f32_of, i64_of, same_f32, Case};
use empyrean_content::models::world::treasure_wielded::TreasureWielded;
use empyrean_content::models::world::Weenie as ContentWeenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CombatStyle, CombatUse, CoverageMask, DestinationType, EquipMask, ItemType, ParentLocation,
    Placement, PropertyAttribute, PropertyBool, PropertyDataId, PropertyInt, PropertyString, Skill,
    WeenieType,
};
use empyrean_entity::models::PropertiesCreateList;
use empyrean_entity::ObjectGuid;
use empyrean_world::dispatch::{self, Class};
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::world_objects::container;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{
    creature_equipment as ce, monster_inventory as mi, world_object_equipment as woe,
};
use empyrean_world::World;
use serde_json::Value;

// ------------------------------------------------------------------ fixtures

pub(crate) const PACK: u32 = 10; // main container: 3 items, 2 packs
pub(crate) const SIDE: u32 = 11; // side pack: 2 items
pub(crate) const ITEM: u32 = 12; // generic, burden 5, value 7
const HEAVY: u32 = 13; // generic, burden 5000
pub(crate) const COIN: u32 = 14; // stackable
pub(crate) const CREATURE: u32 = 15;
pub(crate) const SWORD: u32 = 16;
const GREATSWORD: u32 = 17;
const SHIELD: u32 = 18;
const DAGGER_LEFT: u32 = 19; // off-hand weapon
const BOW: u32 = 20;
const ARROW: u32 = 21;
const SHIRT: u32 = 22;
const PANTS: u32 = 23;
const COAT: u32 = 24; // clothing overlapping the shirt
const HELM: u32 = 25; // outerwear
const RING: u32 = 26; // gear rating
pub(crate) const STORAGE: u32 = 27;
const NOTE: u32 = 28; // "tradenote250"
pub(crate) const PLAYER: u32 = 29;

fn generic(wcid: u32, name: &str, burden: i32, value: i32) -> ContentWeenie {
    ContentWeenie::new(wcid, name, WeenieType::Generic)
        .with_string(PropertyString::Name, name)
        .with_int(PropertyInt::EncumbranceVal, burden)
        .with_int(PropertyInt::Value, value)
}

fn clothing(wcid: u32, name: &str, loc: EquipMask, coverage: CoverageMask) -> ContentWeenie {
    ContentWeenie::new(wcid, name, WeenieType::Clothing)
        .with_int(PropertyInt::ValidLocations, i32::try_from(loc.0).unwrap())
        .with_int(
            PropertyInt::ClothingPriority,
            i32::try_from(coverage.0).unwrap(),
        )
        .with_int(PropertyInt::EncumbranceVal, 10)
        .with_int(PropertyInt::Value, 20)
        .with_int(
            PropertyInt::ItemType,
            i32::try_from(ItemType::Clothing.0).unwrap(),
        )
}

fn weapon(wcid: u32, name: &str, weenie_type: WeenieType, loc: EquipMask) -> ContentWeenie {
    ContentWeenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_int(PropertyInt::ValidLocations, i32::try_from(loc.0).unwrap())
        .with_int(PropertyInt::EncumbranceVal, 100)
        .with_int(PropertyInt::Value, 50)
}

fn content() -> MemContent {
    MemContent::new()
        .weenie(
            ContentWeenie::new(PACK, "pack", WeenieType::Container)
                .with_string(PropertyString::Name, "Pack")
                .with_int(PropertyInt::ItemsCapacity, 3)
                .with_int(PropertyInt::ContainersCapacity, 2)
                .with_int(PropertyInt::EncumbranceVal, 50)
                .with_int(PropertyInt::Value, 60),
        )
        .weenie(
            ContentWeenie::new(SIDE, "sidepack", WeenieType::Container)
                .with_int(PropertyInt::ItemsCapacity, 2)
                .with_int(PropertyInt::EncumbranceVal, 30)
                .with_int(PropertyInt::Value, 40),
        )
        .weenie(generic(ITEM, "item", 5, 7))
        .weenie(generic(HEAVY, "heavy", 5000, 1))
        .weenie(generic(NOTE, "tradenote250", 1, 250))
        .weenie(
            ContentWeenie::new(COIN, "coinstack", WeenieType::Coin)
                .with_int(PropertyInt::MaxStackSize, 100)
                .with_int(PropertyInt::StackSize, 10)
                .with_int(PropertyInt::StackUnitEncumbrance, 2)
                .with_int(PropertyInt::StackUnitValue, 5),
        )
        .weenie(
            ContentWeenie::new(CREATURE, "drudge", WeenieType::Creature)
                .with_string(PropertyString::Name, "Drudge")
                .with_int(PropertyInt::ItemsCapacity, 20)
                .with_int(PropertyInt::ContainersCapacity, 2)
                .with_bool(PropertyBool::Attackable, true),
        )
        .weenie(
            weapon(
                SWORD,
                "Sword",
                WeenieType::MeleeWeapon,
                EquipMask::MeleeWeapon,
            )
            .with_int(PropertyInt::GearDamage, 0),
        )
        .weenie(
            weapon(
                GREATSWORD,
                "Greatsword",
                WeenieType::MeleeWeapon,
                EquipMask::TwoHanded,
            )
            .with_int(PropertyInt::WeaponSkill, Skill::TwoHandedCombat.0),
        )
        .weenie(
            weapon(SHIELD, "Shield", WeenieType::Generic, EquipMask::Shield)
                .with_int(
                    PropertyInt::ItemType,
                    i32::try_from(ItemType::Armor.0).unwrap(),
                )
                .with_int(PropertyInt::CombatUse, i32::from(CombatUse::Shield.0)),
        )
        .weenie(
            weapon(
                DAGGER_LEFT,
                "Dagger",
                WeenieType::MeleeWeapon,
                EquipMask::MeleeWeapon,
            )
            .with_bool(PropertyBool::AutowieldLeft, true),
        )
        .weenie(
            weapon(
                BOW,
                "Bow",
                WeenieType::MissileLauncher,
                EquipMask::MissileWeapon,
            )
            .with_int(PropertyInt::DefaultCombatStyle, CombatStyle::Bow.0)
            .with_int(PropertyInt::AmmoType, 1),
        )
        .weenie(
            ContentWeenie::new(ARROW, "arrow", WeenieType::Ammunition)
                .with_int(
                    PropertyInt::ValidLocations,
                    i32::try_from(EquipMask::MissileAmmo.0).unwrap(),
                )
                .with_int(PropertyInt::AmmoType, 1)
                .with_int(PropertyInt::MaxStackSize, 250),
        )
        .weenie(clothing(
            SHIRT,
            "shirt",
            EquipMask::ChestWear,
            CoverageMask::UnderwearChest,
        ))
        .weenie(clothing(
            PANTS,
            "pants",
            EquipMask::UpperLegWear,
            CoverageMask::UnderwearUpperLegs,
        ))
        .weenie(clothing(
            COAT,
            "coat",
            EquipMask::ChestWear | EquipMask::UpperArmWear,
            CoverageMask::UnderwearChest | CoverageMask::UnderwearUpperArms,
        ))
        .weenie(
            clothing(HELM, "helm", EquipMask::HeadWear, CoverageMask::Head)
                .with_int(PropertyInt::ArmorLevel, 40),
        )
        .weenie(
            generic(RING, "ring", 1, 1)
                .with_int(PropertyInt::GearDamage, 3)
                .with_int(PropertyInt::GearCrit, 2),
        )
        .weenie(
            ContentWeenie::new(STORAGE, "storage", WeenieType::Storage)
                .with_int(PropertyInt::ItemsCapacity, 10),
        )
        .weenie(
            ContentWeenie::new(PLAYER, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .treasure_wielded(TreasureWielded {
            id: 1,
            treasure_type: 7,
            weenie_class_id: SWORD,
            probability: 0.0,
            set_start: true,
            has_sub_set: true,
            ..Default::default()
        })
        .treasure_wielded(TreasureWielded {
            id: 2,
            treasure_type: 7,
            weenie_class_id: SHIELD,
            probability: 1.0,
            ..Default::default()
        })
        .treasure_wielded(TreasureWielded {
            id: 3,
            treasure_type: 7,
            weenie_class_id: COIN,
            probability: 1.0,
            stack_size: 42,
            continues_previous_set: true,
            ..Default::default()
        })
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

pub(crate) fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1_767_225_600.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("empty fake dats"),
    );
    w.content = Arc::new(content());
    guid_manager::initialize(&mut w, &mut EmptyShard);
    w
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)`, added to `World.objects`.
pub(crate) fn spawn(w: &mut World, wcid: u32) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), guid)
    })
    .expect("constructible");
    assert!(w.objects.insert(o).is_ok());
    guid
}

/// A `Player` with `strength` (base), in `World.objects`.
fn spawn_player(w: &mut World, strength: u32) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(PLAYER).expect("test weenie");
    let guid = ObjectGuid::new(0x5000_0001);
    let mut o = CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            Class::Player,
            weenie,
            guid,
            1,
        )
    });
    o.biota
        .properties_attribute
        .get_or_insert_with(Default::default)
        .get_or_insert_with(PropertyAttribute::Strength, Default::default)
        .init_level = strength;
    assert!(w.objects.insert(o).is_ok());
    guid
}

pub(crate) fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).expect("live object")
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).expect("live object")
}

pub(crate) fn add(w: &mut World, c: ObjectGuid, item: ObjectGuid) -> bool {
    container::try_add_to_inventory(w, c, item, 0, false, true)
}

fn positions(w: &World, items: &[ObjectGuid]) -> Vec<Option<i32>> {
    items
        .iter()
        .map(|&g| obj(w, g).placement_position())
        .collect()
}

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("containers", name);
    assert!(!file.cases.is_empty(), "containers/{name}: no cases");
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
        "containers/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(10)].join("\n  ")
    );
}

fn ints(v: &Value) -> Vec<i64> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|x| i64_of(x).expect("int"))
        .collect()
}

// ------------------------------------------------------------------ vectors

/// `List<T>.Sort(Comparison<T>)` is .NET's unstable introsort: the order of equal keys matches.
#[test]
fn list_sort_matches_dotnet_introsort() {
    replay("list_sort", |c| {
        let keys = ints(&c.input["keys"]);
        let mut list: Vec<(i64, i64)> = keys.iter().copied().zip(0..).collect();
        list_sort(&mut list, |a, b| compare_to(a.0, b.0));
        let got: Vec<i64> = list.iter().map(|e| e.1).collect();
        if got == ints(&c.output) {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

/// `Creature.CreateListSelect`: one roll per treasure set, drawn from `ThreadSafeRandom`.
#[test]
fn create_list_select_matches_ace() {
    let mut w = world();
    empyrean_world::managers::property_manager::initialize(&mut w, true);
    replay("create_list_select", |c| {
        let entries: Vec<PropertiesCreateList> = c.input["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .enumerate()
            .map(|(i, e)| PropertiesCreateList {
                database_record_id: u32::try_from(i).unwrap(),
                destination_type: DestinationType(i32::try_from(i64_of(&e[0]).unwrap()).unwrap()),
                weenie_class_id: u32::try_from(i64_of(&e[1]).unwrap()).unwrap(),
                shade: f32_of(&e[2]).unwrap(),
                ..Default::default()
            })
            .collect();
        ThreadSafeRandom::seed(u64::try_from(i64_of(&c.input["seed"]).unwrap()).unwrap());
        let got: Vec<i64> = ce::create_list_select(&w, &entries)
            .iter()
            .map(|e| i64::from(e.database_record_id))
            .collect();
        if got == ints(&c.output) {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

/// `Creature.GetTotalProbability`: LINQ's float `Sum` (a double accumulator) minus the float product.
#[test]
fn get_total_probability_matches_ace() {
    replay("get_total_probability", |c| {
        let items: Vec<TreasureWielded> = c.input["probabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| TreasureWielded {
                probability: f32_of(p).unwrap(),
                ..Default::default()
            })
            .collect();
        let got = mi::get_total_probability(Some(&items));
        if same_f32(got, f32_of(&c.output).unwrap()) {
            Ok(())
        } else {
            Err(format!("{got}"))
        }
    });
}

// ------------------------------------------------------------------ Container: add, remove, move

/// `TryAddToInventory` at placement 0 pushes the others up (`PlacementPosition >= 0` ++), sets the
/// ownership properties, and adds burden and value; `TryRemoveFromInventory` pulls the later ones
/// down and a re-add reuses the freed `Dictionary` slot (.NET enumeration order).
#[test]
fn add_remove_and_readd_follow_ace_order() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let [a, b, c, d] = [0; 4].map(|_| spawn(&mut w, ITEM));

    for i in [a, b, c] {
        assert!(add(&mut w, pack, i));
    }
    assert_eq!(container::inventory_values(&w, pack), [a, b, c]);
    assert_eq!(positions(&w, &[a, b, c]), [Some(2), Some(1), Some(0)]);
    let o = obj(&w, a);
    assert_eq!(
        (o.container_id(), o.owner_id(), o.placement()),
        (
            Some(pack.full()),
            Some(pack.full()),
            Some(Placement::Resting)
        )
    );
    assert_eq!(o.wo.world_object_properties.container, Some(pack));
    assert_eq!(
        (obj(&w, pack).encumbrance_val(), obj(&w, pack).value()),
        (Some(50 + 15), Some(60 + 21))
    );

    // Full main pack, no side pack: refused; a duplicate add is refused too.
    assert!(!add(&mut w, pack, d));
    assert!(!container::try_add_to_inventory(
        &mut w, pack, a, 0, true, true
    ));

    assert_eq!(
        container::try_remove_from_inventory_with_item(&mut w, pack, b, false),
        Some(b)
    );
    assert_eq!(positions(&w, &[a, c]), [Some(1), Some(0)]);
    let o = obj(&w, b);
    assert_eq!(
        (
            o.container_id(),
            o.owner_id(),
            o.placement_position(),
            o.wo.world_object_properties.container
        ),
        (None, None, None, None)
    );
    assert_eq!(obj(&w, pack).encumbrance_val(), Some(50 + 10));
    assert!(!container::try_remove_from_inventory(
        &mut w, pack, b, false
    ));

    assert!(container::try_add_to_inventory(
        &mut w, pack, d, 1, false, true
    ));
    assert_eq!(
        container::inventory_values(&w, pack),
        [a, d, c],
        "d reuses b's slot"
    );
    assert_eq!(positions(&w, &[a, d, c]), [Some(2), Some(1), Some(0)]);
    assert_eq!(
        container::get_inventory_item_with_container(&w, pack, d),
        Some((d, pack))
    );
}

/// A full main pack spills into the side packs, sorted by `Placement` with .NET's sort; the root
/// counts the burden; `limitToMainPackOnly` stops it; side slots have their own capacity.
#[test]
fn capacity_limits_and_side_pack_spill() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let (s1, s2, s3) = (
        spawn(&mut w, SIDE),
        spawn(&mut w, SIDE),
        spawn(&mut w, SIDE),
    );
    obj_mut(&mut w, s1).set_placement(Some(Placement::Belt));
    obj_mut(&mut w, s2).set_placement(Some(Placement::RightHandCombat));
    assert!(add(&mut w, pack, s1));
    assert!(add(&mut w, pack, s2));
    assert!(!add(&mut w, pack, s3), "ContainersCapacity 2");
    // TryAddToInventory sets Placement = Resting on the packs themselves: both sort equal now,
    // so the introsort's two-element case keeps s1 first.
    assert_eq!(container::get_free_container_slots(&w, pack), 0);
    assert_eq!(
        container::get_free_inventory_slots(&w, pack, true),
        3 + 2 + 2
    );
    assert_eq!(container::get_free_inventory_slots(&w, pack, false), 3);

    let items: Vec<ObjectGuid> = (0..6).map(|_| spawn(&mut w, ITEM)).collect();
    for &i in &items[..3] {
        assert!(add(&mut w, pack, i));
    }
    assert!(
        !container::try_add_to_inventory(&mut w, pack, items[3], 0, true, true),
        "main pack only"
    );
    assert_eq!(
        container::try_add_to_inventory_with_container(&mut w, pack, items[3], 0, false, true),
        Some(s1)
    );
    assert_eq!(
        container::try_add_to_inventory_with_container(&mut w, pack, items[4], 0, false, true),
        Some(s1)
    );
    assert_eq!(
        container::try_add_to_inventory_with_container(&mut w, pack, items[5], 0, false, true),
        Some(s2)
    );
    assert_eq!(
        obj(&w, pack).encumbrance_val(),
        Some(50 + 30 + 30 + 6 * 5),
        "the packs and every item, wherever it went"
    );
    assert_eq!(obj(&w, s1).encumbrance_val(), Some(30 + 10));
    assert_eq!(container::get_free_inventory_slots(&w, pack, true), 1);

    assert!(container::has_inventory_item(&w, pack, items[4]));
    assert_eq!(
        container::get_inventory_item_with_container(&w, pack, items[4]),
        Some((items[4], s1))
    );
    // Removing through the root finds it in the side pack and takes the burden off both.
    assert_eq!(
        container::try_remove_from_inventory_with_item(&mut w, pack, items[4], false),
        Some(items[4])
    );
    assert_eq!(
        (
            obj(&w, pack).encumbrance_val(),
            obj(&w, s1).encumbrance_val()
        ),
        (Some(50 + 60 + 5 * 5), Some(30 + 5))
    );

    // CanAddToInventory / CanAddToContainer.
    let extra = spawn(&mut w, ITEM);
    assert!(container::can_add_to_inventory(&w, pack, extra));
    assert!(!container::can_add_to_container(&w, pack, extra, false));
    assert!(container::can_add_to_inventory_counts(&w, pack, 0, 2, 0));
    assert!(!container::can_add_to_inventory_counts(&w, pack, 0, 3, 0));
    assert!(!container::can_add_to_inventory_counts(&w, pack, 1, 0, 0));
    assert_eq!(
        container::can_add_to_inventory_list_with_reasons(&w, pack, &[]),
        (true, false, false)
    );
    assert_eq!(
        container::can_add_to_inventory_list_with_reasons(&w, pack, &[extra, s3]),
        (false, false, true)
    );
}

/// A player's adds check `EncumbranceVal + item <= (150 * Strength + augs) * 3`; a side pack of the
/// player does not ("bug: should be root owner").
#[test]
fn player_burden_limits_adds() {
    let mut w = world();
    let player = spawn_player(&mut w, 10);
    assert_eq!(
        container::player_get_encumbrance_capacity(&w, obj(&w, player)),
        1500
    );
    let heavy = spawn(&mut w, HEAVY);
    assert!(!add(&mut w, player, heavy), "5000 > 4500");
    assert!(!container::can_add_to_inventory(&w, player, heavy));
    assert!(
        container::try_add_to_inventory(&mut w, player, heavy, 0, false, false),
        "burdenCheck false"
    );
    assert_eq!(obj(&w, player).encumbrance_val(), Some(5000));

    let side = spawn(&mut w, SIDE);
    obj_mut(&mut w, side).set_encumbrance_val(Some(0));
    assert!(
        !add(&mut w, player, side),
        "the pack alone is within the limit, the load is not"
    );
    let item = spawn(&mut w, ITEM);
    assert!(!container::can_merge_to_inventory(
        &w, player, item, heavy, 1
    ));
}

/// `SortWorldObjectsIntoInventory` walks the list backwards, renumbers placements by
/// `OrderBy(PlacementPosition)`, then sorts each side pack and adds its burden.
#[test]
fn sort_world_objects_into_inventory_order() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let side = spawn(&mut w, SIDE);
    let (x1, x2, p1, orphan) = (
        spawn(&mut w, ITEM),
        spawn(&mut w, ITEM),
        spawn(&mut w, ITEM),
        spawn(&mut w, ITEM),
    );
    for (g, parent, pos) in [
        (x1, pack, 7),
        (x2, pack, 3),
        (side, pack, 9),
        (p1, side, 4),
        (orphan, ObjectGuid::new(0x8000_FFFF), 0),
    ] {
        let o = obj_mut(&mut w, g);
        o.set_container_id(Some(parent.full()));
        o.set_placement_position(Some(pos));
    }

    let mut list = vec![x1, side, x2, p1, orphan];
    container::sort_world_objects_into_inventory(&mut w, pack, &mut list);

    assert_eq!(list, [orphan], "only the unclaimed object is left");
    assert_eq!(
        container::inventory_values(&w, pack),
        [x2, side, x1],
        "added from the end of the list"
    );
    assert_eq!(
        positions(&w, &[x2, x1, side, p1]),
        [Some(0), Some(1), Some(0), Some(0)]
    );
    assert_eq!(container::inventory_values(&w, side), [p1]);
    assert!(
        obj(&w, pack)
            .container
            .as_ref()
            .unwrap()
            .container
            .inventory_loaded
    );
    assert!(
        obj(&w, side)
            .container
            .as_ref()
            .unwrap()
            .container
            .inventory_loaded
    );
    assert_eq!(obj(&w, pack).encumbrance_val(), Some(50 + 5 + 5 + (30 + 5)));
    assert_eq!(obj(&w, pack).value(), Some(60 + 7 + 7 + (40 + 7)));
}

/// `SortBiotasIntoInventory` builds the objects, keeps what it sorts and drops the rest.
#[test]
fn sort_biotas_into_inventory_drops_the_unclaimed() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let (a, stray) = (spawn(&mut w, ITEM), spawn(&mut w, ITEM));
    obj_mut(&mut w, a).set_container_id(Some(pack.full()));
    let biotas: Vec<empyrean_entity::Biota> = [a, stray]
        .iter()
        .map(|&g| w.objects.remove(g).unwrap().biota)
        .collect();

    container::sort_biotas_into_inventory(&mut w, pack, biotas);
    assert_eq!(container::inventory_values(&w, pack), [a]);
    assert!(w.objects.contains(a));
    assert!(!w.objects.contains(stray));
}

/// The lookups: local items by `PlacementPosition`, then side packs in placement order; counts
/// sum `StackSize ?? 1`; class names compare ignoring case; trade notes by prefix.
#[test]
fn inventory_lookups_by_wcid_class_and_type() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let side = spawn(&mut w, SIDE);
    let (i1, i2, note) = (
        spawn(&mut w, ITEM),
        spawn(&mut w, ITEM),
        spawn(&mut w, NOTE),
    );
    let coin = spawn(&mut w, COIN);
    for g in [i1, side, i2] {
        assert!(add(&mut w, pack, g));
    }
    assert!(add(&mut w, side, note));
    assert!(add(&mut w, side, coin));

    assert_eq!(
        container::get_inventory_items_of_wcid(&w, pack, ITEM),
        [i2, i1],
        "placement 0 is the newest"
    );
    assert_eq!(
        container::get_num_inventory_items_of_wcid(&w, pack, COIN),
        10
    );
    assert_eq!(
        container::get_inventory_items_of_weenie_class(&w, pack, "ITEM"),
        [i2, i1]
    );
    assert_eq!(
        container::get_num_inventory_items_of_weenie_class(&w, pack, "CoinStack"),
        10
    );
    assert_eq!(container::get_trade_notes(&w, pack), [note]);
    assert_eq!(
        container::get_inventory_items_of_type_weenie_type(&w, pack, WeenieType::Coin),
        [coin]
    );
    assert_eq!(container::get_inventory_item(&w, pack, coin), Some(coin));
}

// ------------------------------------------------------------------ Stackable

/// `SetStackSize`: burden and value follow the unit values; a non-stackable ignores it.
/// `CanMergeToInventory`: lifted `StackSize + amount <= MaxStackSize`.
#[test]
fn stack_size_merge_and_split_arithmetic() {
    let mut w = world();
    let coin = spawn(&mut w, COIN);
    let o = obj(&w, coin);
    assert_eq!(
        (o.stack_size(), o.encumbrance_val(), o.value()),
        (Some(10), Some(20), Some(50)),
        "Stackable.SetEphemeralValues"
    );
    obj_mut(&mut w, coin).set_stack_size(Some(37));
    let o = obj(&w, coin);
    assert_eq!(
        (o.stack_size(), o.encumbrance_val(), o.value()),
        (Some(37), Some(74), Some(185))
    );
    obj_mut(&mut w, coin).set_stack_size(None);
    assert_eq!(
        (obj(&w, coin).stack_size(), obj(&w, coin).encumbrance_val()),
        (None, Some(2)),
        "null counts as 1"
    );

    let item = spawn(&mut w, ITEM);
    obj_mut(&mut w, item).set_stack_size(Some(5));
    assert_eq!(obj(&w, item).stack_size(), None, "not a Stackable");

    let pack = spawn(&mut w, PACK);
    obj_mut(&mut w, coin).set_stack_size(Some(90));
    assert!(container::can_merge_to_inventory(&w, pack, item, coin, 10));
    assert!(!container::can_merge_to_inventory(&w, pack, item, coin, 11));
    assert!(
        !container::can_merge_to_inventory(&w, pack, coin, item, 1),
        "null StackSize: lifted comparison is false"
    );
}

/// `MergeAllStackables` pours later stacks into earlier ones of the same wcid; an emptied stack is
/// removed and destroyed.
#[test]
fn merge_all_stackables_pours_later_into_earlier() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let (a, other, c) = (
        spawn(&mut w, COIN),
        spawn(&mut w, ITEM),
        spawn(&mut w, COIN),
    );
    obj_mut(&mut w, a).set_stack_size(Some(60));
    obj_mut(&mut w, c).set_stack_size(Some(70));
    for g in [a, other, c] {
        assert!(add(&mut w, pack, g));
    }
    container::merge_all_stackables(&mut w, pack);
    assert_eq!(
        (obj(&w, a).stack_size(), obj(&w, c).stack_size()),
        (Some(100), Some(30))
    );

    obj_mut(&mut w, a).set_stack_size(Some(50));
    container::merge_all_stackables(&mut w, pack);
    assert_eq!(obj(&w, a).stack_size(), Some(80));
    assert!(!w.objects.contains(c), "the emptied stack is destroyed");
    assert_eq!(container::inventory_values(&w, pack), [a, other]);
}

// ------------------------------------------------------------------ open, close, reset

#[test]
fn open_close_and_clear_unmanaged_inventory() {
    let mut w = world();
    let chest = spawn(&mut w, PACK);
    let player = spawn_player(&mut w, 100);
    obj_mut(&mut w, chest).set_property(empyrean_entity::enums::PropertyFloat::ResetInterval, 5.0);
    take_local();

    dispatch::act_on_use::act_on_use(&mut w, chest, player);
    let o = obj(&w, chest);
    assert!(o.is_open());
    assert_eq!(o.viewer(), player.full());
    assert!(
        o.reset_message_pending(),
        "a reset is scheduled (at least 15 s)"
    );
    assert_eq!(
        container::in_use_message(&w, chest),
        "The Pack is already in use by someone else!"
    );

    dispatch::act_on_use::act_on_use(&mut w, chest, player);
    assert!(
        !obj(&w, chest).is_open(),
        "the viewer's second use closes it"
    );
    assert_eq!(obj(&w, chest).viewer(), 0);

    // An owned container does nothing.
    obj_mut(&mut w, chest).set_owner_id(Some(player.full()));
    dispatch::act_on_use::act_on_use(&mut w, chest, player);
    assert!(!obj(&w, chest).is_open());

    // ClearUnmanagedInventory keeps generated items and destroys the rest.
    let (managed, loose) = (spawn(&mut w, ITEM), spawn(&mut w, ITEM));
    obj_mut(&mut w, managed).set_generator_id(Some(chest.full()));
    assert!(add(&mut w, chest, managed));
    assert!(add(&mut w, chest, loose));
    assert!(container::clear_unmanaged_inventory(&mut w, chest, false));
    assert_eq!(container::inventory_values(&w, chest), [managed]);
    assert!(!w.objects.contains(loose));

    let storage = spawn(&mut w, STORAGE);
    assert!(
        !container::clear_unmanaged_inventory(&mut w, storage, false),
        "storage is never cleared"
    );
}

/// `GenerateContainList` marks the items as managed by the container and sets stack sizes.
#[test]
fn generate_contain_list_places_managed_items() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    obj_mut(&mut w, pack).biota.properties_create_list = Some(Arc::new(vec![
        PropertiesCreateList {
            destination_type: DestinationType::Contain,
            weenie_class_id: COIN,
            stack_size: 33,
            ..Default::default()
        },
        PropertiesCreateList {
            destination_type: DestinationType::Wield,
            weenie_class_id: ITEM,
            ..Default::default()
        },
        PropertiesCreateList {
            destination_type: DestinationType::ContainTreasure,
            weenie_class_id: 9999,
            ..Default::default()
        },
        PropertiesCreateList {
            destination_type: DestinationType::ContainTreasure,
            weenie_class_id: ITEM,
            ..Default::default()
        },
    ]));
    container::generate_contain_list(&mut w, pack);
    let inv = container::inventory_values(&w, pack);
    assert_eq!(inv.len(), 2);
    let (coin, item) = (obj(&w, inv[0]), obj(&w, inv[1]));
    assert_eq!(
        (
            coin.biota.weenie_class_id,
            coin.stack_size(),
            coin.generator_id()
        ),
        (COIN, Some(33), Some(pack.full()))
    );
    assert_eq!(item.biota.weenie_class_id, ITEM);
}

// ------------------------------------------------------------------ Creature_Equipment

/// Equip and dequip across the slot masks: weapon slots compare parent locations, clothing
/// compares coverage, anything else compares `CurrentWieldedLocation`.
#[test]
fn equip_and_dequip_across_slot_masks() {
    let mut w = world();
    let creature = spawn(&mut w, CREATURE);
    let (sword, great, shield, dagger, bow, arrow) = (
        spawn(&mut w, SWORD),
        spawn(&mut w, GREATSWORD),
        spawn(&mut w, SHIELD),
        spawn(&mut w, DAGGER_LEFT),
        spawn(&mut w, BOW),
        spawn(&mut w, ARROW),
    );
    let (shirt, coat, ring) = (
        spawn(&mut w, SHIRT),
        spawn(&mut w, COAT),
        spawn(&mut w, RING),
    );

    assert!(ce::try_equip_object(
        &mut w,
        creature,
        sword,
        EquipMask::MeleeWeapon
    ));
    let o = obj(&w, sword);
    assert_eq!(
        (o.parent_location(), o.placement(), o.wielder),
        (
            Some(ParentLocation::RightHand),
            Some(Placement::RightHandCombat),
            Some(creature)
        )
    );
    assert_eq!(o.wielder_id(), Some(creature.full()));
    assert_eq!(
        obj(&w, creature).wo.world_object_properties.children.len(),
        1
    );
    assert!(
        !ce::try_equip_object(&mut w, creature, great, EquipMask::TwoHanded),
        "two-handed also parents to the right hand"
    );

    assert!(ce::try_equip_object(
        &mut w,
        creature,
        shield,
        EquipMask::Shield
    ));
    assert_eq!(
        obj(&w, shield).parent_location(),
        Some(ParentLocation::Shield)
    );
    // An off-hand weapon parents to LeftWeapon, so the shield does not block it (ACE's check).
    assert!(ce::try_equip_object(
        &mut w,
        creature,
        dagger,
        EquipMask::Shield
    ));
    assert_eq!(
        obj(&w, dagger).placement(),
        Some(Placement::RightHandNonCombat)
    );
    assert_eq!(ce::get_equipped_shield(&w, creature), Some(shield));
    assert_eq!(
        ce::get_equipped_off_hand(&w, creature),
        Some(shield),
        "first in EquippedObjects order"
    );
    assert_eq!(ce::get_dual_wield_weapon(&w, creature), Some(dagger));
    assert_eq!(
        ce::get_equipped_melee_weapon(&w, creature, false),
        Some(sword)
    );

    assert!(ce::try_equip_object(
        &mut w,
        creature,
        bow,
        EquipMask::MissileWeapon
    ));
    assert_eq!(
        obj(&w, bow).parent_location(),
        Some(ParentLocation::LeftHand)
    );
    assert!(ce::try_equip_object(
        &mut w,
        creature,
        arrow,
        EquipMask::MissileAmmo
    ));
    assert_eq!(
        obj(&w, arrow).parent_location(),
        None,
        "ammo is a child only in missile combat mode"
    );
    assert_eq!(ce::get_missile_ammo(&w, creature), Some(arrow));
    assert_eq!(ce::get_equipped_missile_launcher(&w, creature), Some(bow));
    assert_eq!(ce::get_equipped_weapon(&w, creature, false), Some(sword));

    assert!(ce::try_equip_object(
        &mut w,
        creature,
        shirt,
        EquipMask::ChestWear
    ));
    assert!(
        !ce::try_equip_object(
            &mut w,
            creature,
            coat,
            EquipMask::ChestWear | EquipMask::UpperArmWear
        ),
        "coverage overlap"
    );
    assert!(ce::try_equip_object(
        &mut w,
        creature,
        ring,
        EquipMask::FingerWearLeft
    ));
    assert_eq!(
        ce::get_equipped_items_rating_sum(&w, creature, PropertyInt::GearDamage),
        3
    );
    assert_eq!(
        ce::get_equipped_items_rating_sum(&w, creature, PropertyInt::GearCrit),
        2
    );
    assert_eq!(
        ce::get_equipped_items_rating_sum(&w, creature, PropertyInt::Value),
        0,
        "not a rating"
    );

    let burden = obj(&w, creature).encumbrance_val().unwrap();
    assert_eq!(
        burden,
        100 * 4 + 10 + 1,
        "sword, shield, dagger, bow, shirt, ring (the arrow weighs 0)"
    );
    assert_eq!(
        ce::try_dequip_object(&mut w, creature, sword),
        Some((sword, EquipMask::MeleeWeapon))
    );
    let o = obj(&w, sword);
    assert_eq!(
        (
            o.current_wielded_location(),
            o.wielder_id(),
            o.wielder,
            o.placement(),
            o.parent_location()
        ),
        (None, None, None, Some(Placement::Resting), None)
    );
    assert_eq!(obj(&w, creature).encumbrance_val(), Some(burden - 100));
    assert!(!obj(&w, creature)
        .wo
        .world_object_properties
        .children
        .iter()
        .any(|h| h.guid == sword.full()));
    assert_eq!(ce::try_dequip_object(&mut w, creature, sword), None);
    assert_eq!(
        ce::try_dequip_object(&mut w, creature, ring).map(|r| r.1),
        Some(EquipMask::FingerWearLeft)
    );
    assert_eq!(
        ce::get_equipped_items_rating_sum(&w, creature, PropertyInt::GearDamage),
        0
    );
    assert!(
        ce::try_equip_object(&mut w, creature, great, EquipMask::TwoHanded),
        "the right hand is free again"
    );
}

// ------------------------------------------------------------------ Monster_Inventory

/// A monster wears its underwear, then picks a weapon (one candidate: the shuffle draws nothing)
/// and a shield (`Next(0, 0)`), equipping them in that order.
#[test]
fn monster_wields_its_inventory() {
    let mut w = world();
    let creature = spawn(&mut w, CREATURE);
    let items = [SHIRT, PANTS, HELM, SWORD, SHIELD].map(|wcid| spawn(&mut w, wcid));
    for g in items {
        assert!(add(&mut w, creature, g));
    }
    ThreadSafeRandom::seed(7);
    mi::equip_inventory_items(&mut w, creature, false);
    let equipped = ce::equipped_objects_values(&w, creature);
    assert_eq!(
        equipped,
        [items[1], items[0], items[2], items[3], items[4]],
        "pants, shirt, helm, sword, shield"
    );
    assert!(container::inventory_values(&w, creature).is_empty());
}

/// V316: a second helm cannot be worn (the head is covered) and goes
/// back into the inventory; ACE left it in neither the inventory nor the equipped items.
#[test]
fn a_monsters_armor_that_cannot_be_worn_stays_in_its_inventory() {
    let mut w = world();
    let creature = spawn(&mut w, CREATURE);
    let helms = [spawn(&mut w, HELM), spawn(&mut w, HELM)];
    for g in helms {
        assert!(add(&mut w, creature, g));
    }
    ThreadSafeRandom::seed(7);
    mi::equip_inventory_items(&mut w, creature, false);
    let equipped = ce::equipped_objects_values(&w, creature);
    let inventory = container::inventory_values(&w, creature);
    assert_eq!(equipped.len(), 1, "one helm is worn");
    assert_eq!(inventory.len(), 1, "the other stays in the inventory");
    assert!(
        helms.contains(&equipped[0])
            && helms.contains(&inventory[0])
            && equipped[0] != inventory[0]
    );
}

/// A launcher needs matching ammo; an off-hand weapon goes with a one-handed weapon when the AI
/// may dual wield.
#[test]
fn monster_weapon_choice_ammo_and_dual_wield() {
    let mut w = world();
    let creature = spawn(&mut w, CREATURE);
    let (bow, arrow) = (spawn(&mut w, BOW), spawn(&mut w, ARROW));
    assert!(add(&mut w, creature, bow));
    assert_eq!(
        mi::select_wielded_weapons(&mut w, creature),
        Vec::<ObjectGuid>::new(),
        "no ammo: the bow is dropped from the choice"
    );
    assert!(add(&mut w, creature, arrow));
    assert_eq!(mi::select_wielded_weapons(&mut w, creature), [bow, arrow]);

    let dualist = spawn(&mut w, CREATURE);
    obj_mut(&mut w, dualist)
        .set_property(PropertyInt::AiAllowedCombatStyle, CombatStyle::DualWield.0);
    let (sword, dagger) = (spawn(&mut w, SWORD), spawn(&mut w, DAGGER_LEFT));
    assert!(add(&mut w, dualist, sword));
    assert!(add(&mut w, dualist, dagger));
    ThreadSafeRandom::seed(1);
    assert_eq!(mi::select_wielded_weapons(&mut w, dualist), [sword, dagger]);
}

/// The wielded-treasure sets: a set that misses skips its subset, and the next entry continuing
/// the set is rolled on its own (probability 1).
#[test]
fn generate_wielded_treasure_follows_the_set_structure() {
    let mut w = world();
    let creature = spawn(&mut w, CREATURE);
    obj_mut(&mut w, creature).set_property(PropertyDataId::WieldedTreasureType, 7);
    ThreadSafeRandom::seed(3);
    ce::generate_wielded_treasure(&mut w, creature);
    let inv = container::inventory_values(&w, creature);
    assert_eq!(inv.len(), 1);
    assert_eq!(
        (
            obj(&w, inv[0]).biota.weenie_class_id,
            obj(&w, inv[0]).stack_size()
        ),
        (COIN, Some(42))
    );

    let table = w.content.get_cached_wielded_treasure(7);
    let rolled = woe::generate_wielded_treasure_sets(&mut w, &table).expect("one item");
    assert_eq!(
        rolled
            .iter()
            .map(|o| o.biota.weenie_class_id)
            .collect::<Vec<_>>(),
        [COIN]
    );
    assert!(
        woe::generate_wielded_treasure_sets(&mut w, &[]).is_none(),
        "nothing rolled: null"
    );
}

// Cross-checks against the shared client rules.

/// `Player.GetEncumbranceCapacity` against the shared rules' `burden::encumbrance_capacity`:
/// they agree for Strength >= 1 and up to five augmentations (the most a player can buy).
#[test]
fn rule3_encumbrance_capacity_agrees_with_dere_rules_up_to_five_augs() {
    let mut w = world();
    let player = spawn_player(&mut w, 1);
    for strength in 1..=500u32 {
        for augs in 0..=5 {
            let o = obj_mut(&mut w, player);
            o.biota
                .properties_attribute
                .as_mut()
                .unwrap()
                .get_mut(&PropertyAttribute::Strength)
                .unwrap()
                .init_level = strength;
            o.set_property(PropertyInt::AugmentationIncreasedCarryingCapacity, augs);
            let ours = container::player_get_encumbrance_capacity(&w, obj(&w, player));
            let theirs =
                dereth_rules::burden::encumbrance_capacity(i32::try_from(strength).unwrap(), augs);
            assert_eq!(ours, theirs, "strength {strength} augs {augs}");
        }
    }
}

/// V246: above five augmentations the bonus is capped at +150 per point
/// (and a negative count gives none); ACE's `Player.GetEncumbranceCapacity` has no cap.
#[test]
fn rule3_encumbrance_capacity_above_five_augs() {
    let mut w = world();
    let player = spawn_player(&mut w, 100);
    obj_mut(&mut w, player).set_property(PropertyInt::AugmentationIncreasedCarryingCapacity, 6);
    assert_eq!(
        container::player_get_encumbrance_capacity(&w, obj(&w, player)),
        dereth_rules::burden::encumbrance_capacity(100, 6)
    );
}

/// `GetFreeInventorySlots(includeSidePacks: false)` against `num_empty_item_slots` for a main pack
/// of plain items: they agree for every capacity byte (V247, the signed reading).
#[test]
fn rule3_main_pack_free_slots_agree_with_dereth_client_model() {
    use dereth_primitives::ObjectId;
    use dereth_protocol::types::PublicWeenieDesc;

    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let items: Vec<ObjectGuid> = (0..3).map(|_| spawn(&mut w, ITEM)).collect();
    let mut disagreements = Vec::new();
    for capacity in 0..=255u8 {
        for count in 0..=3usize {
            obj_mut(&mut w, pack).set_property(PropertyInt::ItemsCapacity, i32::from(capacity));
            for &i in &items {
                container::try_remove_from_inventory(&mut w, pack, i, false);
            }
            let room = match dereth_rules::capacity::capacity(capacity) {
                dereth_rules::capacity::UNLIMITED => count,
                c => count.min(usize::try_from(c.max(0)).unwrap()),
            };
            for &i in &items[..room] {
                assert!(add(&mut w, pack, i));
            }
            let n = container::inventory_values(&w, pack).len();
            let ours = container::get_free_inventory_slots(&w, pack, false);

            let mut g = dereth_client_model::world::World::new();
            let mut wn = dereth_client_model::weenie::Weenie::new(ObjectId(1));
            wn.pwd = PublicWeenieDesc {
                items_capacity: Some(capacity),
                ..PublicWeenieDesc::default()
            };
            wn.valid = true;
            g.tables.weenies.insert(ObjectId(1), wn);
            g.tables.inventories.insert(
                ObjectId(1),
                dereth_client_model::objects::ObjectInventory {
                    container: ObjectId(1),
                    items: (0..n)
                        .map(|k| ObjectId(100 + u32::try_from(k).unwrap()))
                        .collect(),
                    ..Default::default()
                },
            );
            // The client answers -1 for an unlimited pack; the server counts it as `i32::MAX` free.
            let theirs = match g.num_empty_item_slots(ObjectId(1)) {
                -1 => i32::MAX,
                n => n,
            };
            if ours != theirs {
                disagreements.push(capacity);
            }
        }
    }
    disagreements.dedup();
    assert_eq!(
        disagreements,
        Vec::<u8>::new(),
        "V247: the server reads the capacity byte as the client does"
    );
}

/// `CanMergeToInventory(item, target, 1)` against `is_merge_attempt_legal` on their shared domain
/// (same wcid, both stackable): both allow exactly `StackSize < MaxStackSize`.
#[test]
fn rule3_merge_legality_agrees_with_dereth_client_model() {
    use dereth_primitives::ObjectId;
    use dereth_protocol::types::PublicWeenieDesc;

    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let (src, dst) = (spawn(&mut w, COIN), spawn(&mut w, COIN));
    for max in 2..=6u16 {
        for stack in 0..=max + 1 {
            obj_mut(&mut w, dst).set_property(PropertyInt::MaxStackSize, i32::from(max));
            obj_mut(&mut w, dst).set_stack_size(Some(i32::from(stack)));
            let ours = container::can_merge_to_inventory(&w, pack, src, dst, 1);

            let mut g = dereth_client_model::world::World::new();
            for (id, n) in [(1, 1u16), (2, stack)] {
                let mut wn = dereth_client_model::weenie::Weenie::new(ObjectId(id));
                wn.pwd = PublicWeenieDesc {
                    wcid: COIN,
                    stack_size: Some(n),
                    max_stack_size: Some(max),
                    ..PublicWeenieDesc::default()
                };
                wn.valid = true;
                g.tables.weenies.insert(ObjectId(id), wn);
            }
            let theirs = g.is_merge_attempt_legal(ObjectId(1), ObjectId(2)).is_ok();
            assert_eq!(ours, theirs, "stack {stack} max {max}");
        }
    }
}

mod created_contents {
    use crate::support::position_and_inventory::*;

    /// `Container(Weenie)` -> `SetEphemeralValues(false)` -> `GenerateContainList`: a sack built from
    /// its weenie holds its `Contain` create-list items (the coin stack of 5, marked as managed by the
    /// sack's GeneratorId; the missing weenie is skipped) once it is in the world, generated once. A
    /// creature's constructor does not generate it.
    #[test]
    fn a_container_built_from_its_weenie_holds_its_contain_list() {
        let mut w = world();
        let sack =
            factory::create_new_world_object_by_name_in_world(&mut w, "sack").expect("a sack");
        let inventory: Vec<ObjectGuid> = container::inventory(w.objects.get(sack).unwrap())
            .keys()
            .copied()
            .collect();
        assert_eq!(inventory.len(), 1, "one coin stack");
        let coin = w.objects.get(inventory[0]).unwrap();
        assert_eq!(
            (
                coin.biota.weenie_class_id,
                coin.stack_size(),
                coin.generator_id()
            ),
            (COIN, Some(5), Some(sack.full()))
        );
        assert_eq!(coin.container_id(), Some(sack.full()));

        empyrean_world::world_objects::creature::post_insert(&mut w, sack);
        assert_eq!(
            container::inventory(w.objects.get(sack).unwrap()).len(),
            1,
            "not generated twice"
        );

        let drudge =
            factory::create_new_world_object_by_name_in_world(&mut w, "drudge").expect("a drudge");
        assert!(
            container::inventory(w.objects.get(drudge).unwrap()).is_empty(),
            "creatures skip it"
        );
    }
}

#[cfg(feature = "real-content")]
mod created_contents_real {
    mod real_content {
        use crate::support::position_and_inventory::real_content::*;

        /// `Container.GenerateContainList` on a real weenie: "sackherbs25" (a Container) holds its 18
        /// `Contain` create-list herbs, 25 each, in the list's order (ACE's world database).
        #[test]
        fn a_real_herb_sack_holds_its_herbs() {
            let path = empyrean_common::test_paths::world_pack();
            let content = empyrean_content::PackContent::open(&path).unwrap_or_else(|e| {
                    panic!(
                        "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                        path.display()
                    )
                });
            let mut w = real_world();
            w.content = Arc::new(content);
            let sack = factory::create_new_world_object_by_name_in_world(&mut w, "sackherbs25")
                .expect("the sack");
            let herbs: Vec<(u32, Option<i32>)> = container::inventory(w.objects.get(sack).unwrap())
                .keys()
                .map(|&g| {
                    w.objects
                        .get(g)
                        .map(|o| (o.biota.weenie_class_id, o.stack_size()))
                        .unwrap()
                })
                .collect();
            let wcids: Vec<u32> = herbs.iter().map(|h| h.0).collect();
            assert_eq!(
                wcids,
                [
                    774, 775, 778, 768, 776, 766, 780, 765, 625, 772, 770, 771, 769, 773, 767, 781,
                    779, 777
                ]
            );
            assert!(herbs.iter().all(|h| h.1 == Some(25)), "{herbs:?}");
        }
    }
}
