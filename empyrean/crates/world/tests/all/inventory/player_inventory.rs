//! Vectors: fixtures/vectors/inventory/
//! Burden arithmetic and CheckWieldRequirement replay ACE inventory vectors; move/stack/wield
//! flows expected from Player_Inventory.cs.
//! Divergence: V407
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::vectors::{self, i64_of, Case};
use empyrean_content::models::world::Weenie as ContentWeenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    CombatStyle, EquipMask, HeritageGroup, PropertyAttribute, PropertyBool, PropertyInstanceId,
    PropertyInt, PropertyString, Skill, SkillAdvancementClass, WeenieError, WeenieType,
    WieldRequirement,
};
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_store::MemShard;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::items_to_receive::ItemsToReceive;
use empyrean_world::entity::put_item_in_container_event::PutItemInContainerEvent;
use empyrean_world::entity::unique_table::UniqueTable;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::player_inventory::{self as pi, SearchLocations};
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{container, creature_equipment as ce};
use empyrean_world::World;
use serde_json::Value;

// ------------------------------------------------------------------ fixtures

const PLAYER_WCID: u32 = 1;
const PACK: u32 = 2;
const ITEM: u32 = 3; // burden 5, value 7
const COIN: u32 = 4; // stackable, max 100, unit burden 2, unit value 5
const SWORD: u32 = 5;
const GREATSWORD: u32 = 6;
const SHIELD: u32 = 7;
const WAND: u32 = 8;
const UNIQUE: u32 = 9; // Unique = 1
const ARROW: u32 = 10;
const OTHER_COIN: u32 = 11;

const PLAYER: u32 = 0x5000_0001;
const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

// opcodes and game-event types
const GAME_EVENT: u32 = 0xF7B0;
const SAVE_FAILED: u32 = 0x00A0;
const CONTAIN_ID: u32 = 0x0022;
const WIELD_ITEM: u32 = 0x0023;
const WEENIE_ERROR: u32 = 0x028A;
const TRANSIENT: u32 = 0x02EB;
const STACK_SIZE: u32 = 0x0197;
const PRIVATE_INT: u32 = 0x02CD;
const PUBLIC_INT: u32 = 0x02CE;
const INSTANCE_ID: u32 = 0x02DA;
const PICKUP_EVENT: u32 = 0xF74A;
const SOUND: u32 = 0xF750;
const REMOVE_OBJECT: u32 = 0x0024;

fn weapon(wcid: u32, name: &str, weenie_type: WeenieType, loc: EquipMask) -> ContentWeenie {
    ContentWeenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_int(PropertyInt::ValidLocations, i32::try_from(loc.0).unwrap())
        .with_int(PropertyInt::EncumbranceVal, 100)
        .with_int(PropertyInt::Value, 50)
}

fn coin(wcid: u32) -> ContentWeenie {
    ContentWeenie::new(wcid, "coins", WeenieType::Coin)
        .with_string(PropertyString::Name, "Pyreal")
        .with_int(PropertyInt::MaxStackSize, 100)
        .with_int(PropertyInt::StackSize, 10)
        .with_int(PropertyInt::StackUnitEncumbrance, 2)
        .with_int(PropertyInt::StackUnitValue, 5)
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
        .weenie(
            ContentWeenie::new(PACK, "pack", WeenieType::Container)
                .with_string(PropertyString::Name, "Pack")
                .with_int(PropertyInt::ItemsCapacity, 24)
                .with_int(PropertyInt::EncumbranceVal, 50)
                .with_int(PropertyInt::Value, 60),
        )
        .weenie(
            ContentWeenie::new(ITEM, "item", WeenieType::Generic)
                .with_string(PropertyString::Name, "Gem")
                .with_int(PropertyInt::EncumbranceVal, 5)
                .with_int(PropertyInt::Value, 7),
        )
        .weenie(coin(COIN))
        .weenie(coin(OTHER_COIN).with_string(PropertyString::Name, "Trade Note"))
        .weenie(
            weapon(
                SWORD,
                "Sword",
                WeenieType::MeleeWeapon,
                EquipMask::MeleeWeapon,
            )
            .with_int(PropertyInt::DefaultCombatStyle, CombatStyle::OneHanded.0),
        )
        .weenie(
            weapon(
                GREATSWORD,
                "Greatsword",
                WeenieType::MeleeWeapon,
                EquipMask::TwoHanded,
            )
            .with_int(PropertyInt::WeaponSkill, Skill::TwoHandedCombat.0)
            .with_int(PropertyInt::DefaultCombatStyle, CombatStyle::TwoHanded.0),
        )
        .weenie(
            weapon(SHIELD, "Shield", WeenieType::Generic, EquipMask::Shield)
                .with_int(PropertyInt::CombatUse, 4)
                .with_int(PropertyInt::ItemType, 2),
        )
        .weenie(
            weapon(WAND, "Wand", WeenieType::Caster, EquipMask::Held)
                .with_int(PropertyInt::DefaultCombatStyle, CombatStyle::Magic.0),
        )
        .weenie(
            ContentWeenie::new(UNIQUE, "unique", WeenieType::Generic)
                .with_string(PropertyString::Name, "Orb")
                .with_string(PropertyString::PluralName, "Orbs")
                .with_int(PropertyInt::Unique, 1)
                .with_int(PropertyInt::EncumbranceVal, 1),
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
}

use empyrean_testkit::EmptyShard;

/// A world on synthetic content with ACE's default server properties, one session S whose
/// player is PLAYER (strength 100, empty-handed, burden 0).
fn world() -> World {
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
            .expect("fake dats with the stat tables"),
    );
    w.content = Arc::new(content());
    guid_manager::initialize(&mut w, &mut EmptyShard);
    pm::install_shard_config(&mut w, pm::shard_config_handle(Box::new(MemShard::new())));
    pm::initialize(&mut w, true);

    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(&w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            Class::Player,
            weenie,
            ObjectGuid::new(PLAYER),
            1,
        )
    });
    set_attribute(&mut o, PropertyAttribute::Strength, 100, 0);
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    // `Player.Character` (the ObjDesc an equip broadcasts reads the hair textures)
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
    w
}

fn player() -> ObjectGuid {
    ObjectGuid::new(PLAYER)
}

fn set_attribute(o: &mut WorldObject, attribute: PropertyAttribute, init: u32, ranks: u32) {
    let rec = o
        .biota
        .properties_attribute
        .get_or_insert_with(Default::default)
        .get_or_insert_with(attribute, Default::default);
    rec.init_level = init;
    rec.level_from_cp = ranks;
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

/// A game event's payload (after the 16-byte header and type).
fn payload(bytes: &[u8]) -> &[u8] {
    &bytes[16..]
}

fn u32_at(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
}

/// A transient string event's text (a `String16L` after the event header).
fn transient_text(bytes: &[u8]) -> String {
    let p = payload(bytes);
    let len = usize::from(u16::from_le_bytes([p[0], p[1]]));
    String::from_utf8(p[2..2 + len].to_vec()).unwrap()
}

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("inventory", name);
    assert!(!file.cases.is_empty(), "inventory/{name}: no cases");
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
        "inventory/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(10)].join("\n  ")
    );
}

fn int(v: &Value) -> i64 {
    i64_of(v).expect("int")
}

fn opt_i32(v: &Value) -> Option<i32> {
    v.as_i64().map(|x| i32::try_from(x).expect("an int"))
}

// ------------------------------------------------------------------ vectors

/// `GetEncumbranceCapacity`, `HasEnoughBurdenToAddToInventory(int)` and `GetAvailableBurden`:
/// uint/int/long arithmetic, the lifted null `EncumbranceVal`, the attribute floor.
#[test]
fn burden_arithmetic_matches_ace() {
    replay("burden", |c| {
        let mut w = world();
        let o = obj_mut(&mut w, player());
        // The harness's player is an uninitialised object: no ephemeral dictionaries, so its
        // EncumbranceVal is the biota's (a constructed Container keeps it ephemeral).
        o.wo.world_object_properties.ephemeral_property_ints = None;
        o.remove_property(PropertyInt::EncumbranceVal);
        let strength = c.input["strength"].as_array().unwrap();
        set_attribute(
            o,
            PropertyAttribute::Strength,
            u32::try_from(int(&strength[0])).unwrap(),
            u32::try_from(int(&strength[1])).unwrap(),
        );
        match opt_i32(&c.input["augs"]) {
            Some(a) => o.set_property(PropertyInt::AugmentationIncreasedCarryingCapacity, a),
            None => o.remove_property(PropertyInt::AugmentationIncreasedCarryingCapacity),
        }
        o.set_encumbrance_val(opt_i32(&c.input["encumbrance"]));
        let total = opt_i32(&c.input["total"]).unwrap();
        let o = obj(&w, player());
        let capacity = pi::get_encumbrance_capacity(&w, o);
        let got = serde_json::json!([
            capacity,
            pi::has_enough_burden_to_add_to_inventory_total(&w, o, total),
            pi::get_available_burden(&w, o)
        ]);
        // V246: retail caps the bonus at five augmentations and saturates. ACE's
        // recorded tuple is the record wherever its uncapped `int` capacity agrees; elsewhere the
        // aug count is outside 0..=5 or ACE's arithmetic overflowed, and the tuple follows the
        // ruled capacity.
        let expected = if int(&c.output[0]) == i64::from(capacity) {
            c.output.clone()
        } else {
            let augs = opt_i32(&c.input["augs"]).unwrap_or(0);
            if (0..=5).contains(&augs) && capacity != i32::MAX {
                return Err(format!(
                    "capacity {capacity} differs from ACE with {augs} augs and no overflow"
                ));
            }
            let limit = capacity.wrapping_mul(3);
            let encumbrance = opt_i32(&c.input["encumbrance"]);
            serde_json::json!([
                capacity,
                encumbrance.is_some_and(|e| e.wrapping_add(total) <= limit),
                encumbrance.map_or(0, |e| limit.wrapping_sub(e))
            ])
        };
        if got == expected {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

/// `CheckWieldRequirement` for the kinds that read only the player's own records.
#[test]
fn wield_requirement_matches_ace() {
    replay("wield_requirement", |c| {
        let mut w = world();
        let p = &c.input["player"];
        let o = obj_mut(&mut w, player());
        o.remove_property(PropertyInt::EncumbranceVal);
        for a in p["attributes"].as_array().unwrap() {
            set_attribute(
                o,
                PropertyAttribute(u16::try_from(int(&a[0])).unwrap()),
                u32::try_from(int(&a[1])).unwrap(),
                u32::try_from(int(&a[2])).unwrap(),
            );
        }
        for s in p["skills"].as_array().unwrap() {
            let rec = PropertiesSkill {
                sac: SkillAdvancementClass(u32::try_from(int(&s[1])).unwrap()),
                init_level: u32::try_from(int(&s[2])).unwrap(),
                level_from_pp: u16::try_from(int(&s[3])).unwrap(),
                ..Default::default()
            };
            o.biota
                .properties_skill
                .get_or_insert_with(Default::default)
                .insert(Skill(i32::try_from(int(&s[0])).unwrap()), rec);
        }
        for i in p["ints"].as_array().unwrap() {
            o.set_property(
                PropertyInt(u16::try_from(int(&i[0])).unwrap()),
                i32::try_from(int(&i[1])).unwrap(),
            );
        }
        for b in p["bools"].as_array().unwrap() {
            o.set_property(
                PropertyBool(u16::try_from(int(&b[0])).unwrap()),
                b[1].as_bool().unwrap(),
            );
        }
        let req = c.input["req"].as_array().unwrap();
        let got = pi::check_wield_requirement(
            &mut w,
            player(),
            WieldRequirement(i32::try_from(int(&req[0])).unwrap()),
            opt_i32(&req[1]),
            opt_i32(&req[2]),
        );
        let got = serde_json::json!(got.0);
        if got == c.output {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

// ------------------------------------------------------------------ hand-derived: lookups

/// `FindObject`: self, then the main pack and side packs (with the pack found in), then the
/// equipped items; `LocationsICanMove` without a landblock finds nothing else.
#[test]
fn find_object_searches_in_aces_order() {
    let mut w = world();
    let pack = give(&mut w, PACK);
    let loose = give(&mut w, ITEM);
    let packed = spawn(&mut w, ITEM);
    assert!(container::try_add_to_inventory(
        &mut w, pack, packed, 0, false, true
    ));
    let sword = spawn(&mut w, SWORD);
    assert!(ce::try_equip_object(
        &mut w,
        player(),
        sword,
        EquipMask::MeleeWeapon
    ));
    let stranger = spawn(&mut w, ITEM);

    let f = pi::find_object(&w, player(), player(), SearchLocations::None);
    assert_eq!(
        (f.result, f.root_owner),
        (Some(player()), Some(player())),
        "the player itself"
    );

    let f = pi::find_object(&w, player(), loose, SearchLocations::LocationsICanMove);
    assert_eq!(
        (f.result, f.found_in_container, f.root_owner, f.was_equipped),
        (Some(loose), Some(player()), Some(player()), false)
    );

    let f = pi::find_object(&w, player(), packed, SearchLocations::LocationsICanMove);
    assert_eq!(
        (f.result, f.found_in_container, f.root_owner),
        (Some(packed), Some(pack), Some(player())),
        "a side pack's item"
    );

    let f = pi::find_object(&w, player(), sword, SearchLocations::LocationsICanMove);
    assert_eq!(
        (f.result, f.found_in_container, f.root_owner, f.was_equipped),
        (Some(sword), None, Some(player()), true)
    );

    assert_eq!(
        pi::find_object(&w, player(), sword, SearchLocations::MyInventory).result,
        None,
        "equipped items only with MyEquippedItems"
    );
    assert_eq!(
        pi::find_object(&w, player(), stranger, SearchLocations::Everywhere).result,
        None
    );

    assert_eq!(
        pi::get_all_possessions(&w, player()),
        vec![pack, loose, packed, sword]
    );
}

/// `AdjustStack`: the stack's size, burden and value, then its container and its root owner
/// (once when they are the same); an amount that empties or overfills the stack is refused.
#[test]
fn adjust_stack_updates_the_stack_its_container_and_its_root() {
    let mut w = world();
    let pack = give(&mut w, PACK);
    let coins = spawn(&mut w, COIN);
    assert!(container::try_add_to_inventory(
        &mut w, pack, coins, 0, false, true
    ));
    // 10 coins: burden 20, value 50; the pack 50+20, the player 50+20 (pack) ...
    let burden = |w: &World, g| (obj(w, g).encumbrance_val(), obj(w, g).value());
    assert_eq!(burden(&w, coins), (Some(20), Some(50)));
    let (pack0, player0) = (burden(&w, pack), burden(&w, player()));

    assert!(pi::adjust_stack(
        &mut w,
        player(),
        coins,
        -4,
        Some(pack),
        Some(player())
    ));
    assert_eq!(obj(&w, coins).stack_size(), Some(6));
    assert_eq!(burden(&w, coins), (Some(12), Some(30)));
    assert_eq!(
        burden(&w, pack),
        (pack0.0.map(|e| e - 8), pack0.1.map(|v| v - 20))
    );
    assert_eq!(
        burden(&w, player()),
        (player0.0.map(|e| e - 8), player0.1.map(|v| v - 20))
    );

    // the container is the root: changed once
    let loose = give(&mut w, COIN);
    let before = burden(&w, player());
    assert!(pi::adjust_stack(
        &mut w,
        player(),
        loose,
        5,
        Some(player()),
        Some(player())
    ));
    assert_eq!(
        burden(&w, player()),
        (before.0.map(|e| e + 10), before.1.map(|v| v + 25))
    );

    // to 0 or past MaxStackSize: refused, nothing changes
    assert!(!pi::adjust_stack(
        &mut w,
        player(),
        loose,
        -15,
        Some(player()),
        Some(player())
    ));
    assert!(!pi::adjust_stack(
        &mut w,
        player(),
        loose,
        86,
        Some(player()),
        Some(player())
    ));
    assert!(pi::adjust_stack(&mut w, player(), loose, 85, None, None));
    assert_eq!(obj(&w, loose).stack_size(), Some(100));
}

// ------------------------------------------------------------------ hand-derived: checks

/// `CheckWeaponCollision` for a new item: a shield beside a two-handed weapon or a caster, a
/// missile launcher beside anything, a two-hander beside anything, ammo of another type.
#[test]
fn check_weapon_collision_refuses_aces_conflicts() {
    let mut w = world();
    let shield = spawn(&mut w, SHIELD);
    let sword = spawn(&mut w, SWORD);
    let greatsword = spawn(&mut w, GREATSWORD);
    let wand = spawn(&mut w, WAND);

    // empty-handed: everything fits
    for (item, loc) in [
        (shield, EquipMask::Shield),
        (sword, EquipMask::MeleeWeapon),
        (greatsword, EquipMask::TwoHanded),
        (wand, EquipMask::Held),
    ] {
        assert!(
            pi::check_weapon_collision(&w, player(), Some(item), Some(loc), None),
            "{loc:?}"
        );
    }

    assert!(ce::try_equip_object(
        &mut w,
        player(),
        greatsword,
        EquipMask::TwoHanded
    ));
    assert!(
        !pi::check_weapon_collision(&w, player(), Some(shield), Some(EquipMask::Shield), None),
        "a shield beside a two-hander"
    );
    assert!(
        !pi::check_weapon_collision(
            &w,
            player(),
            Some(sword),
            Some(EquipMask::MeleeWeapon),
            None
        ),
        "a sword beside a two-hander"
    );
    assert!(
        !pi::check_weapon_collision(&w, player(), Some(wand), Some(EquipMask::Held), None),
        "a wand beside anything in the main hand"
    );
    assert!(ce::try_dequip_object(&mut w, player(), greatsword).is_some());

    assert!(ce::try_equip_object(
        &mut w,
        player(),
        sword,
        EquipMask::MeleeWeapon
    ));
    assert!(
        pi::check_weapon_collision(&w, player(), Some(shield), Some(EquipMask::Shield), None),
        "a shield beside a one-hander"
    );
    assert!(
        !pi::check_weapon_collision(
            &w,
            player(),
            Some(greatsword),
            Some(EquipMask::TwoHanded),
            None
        ),
        "a two-hander needs empty hands"
    );

    // Held with no DefaultCombatStyle: only at peace
    obj_mut(&mut w, wand).remove_property(PropertyInt::DefaultCombatStyle);
    assert!(ce::try_dequip_object(&mut w, player(), sword).is_some());
    assert!(pi::check_weapon_collision(
        &w,
        player(),
        Some(wand),
        Some(EquipMask::Held),
        Some(empyrean_entity::enums::CombatMode::NonCombat)
    ));
    assert!(!pi::check_weapon_collision(
        &w,
        player(),
        Some(wand),
        Some(EquipMask::Held),
        Some(empyrean_entity::enums::CombatMode::Melee)
    ));

    // changing combat mode: a main-hand item with no DefaultCombatStyle is illegal
    assert!(ce::try_equip_object(
        &mut w,
        player(),
        wand,
        EquipMask::Held
    ));
    assert!(!pi::check_weapon_collision(&w, player(), None, None, None));
}

/// `CheckWieldRequirements`: the heritage armor rule, `AllowedWielder`, then the four
/// requirement slots in order (the first failing one answers).
#[test]
fn check_wield_requirements_in_aces_order() {
    let mut w = world();
    obj_mut(&mut w, player()).set_property(PropertyInt::HeritageGroup, HeritageGroup::Aluvian.0);
    let item = spawn(&mut w, SWORD);
    assert_eq!(
        pi::check_wield_requirements(&mut w, player(), item),
        WeenieError::None
    );

    obj_mut(&mut w, item).set_property(PropertyInt::HeritageSpecificArmor, HeritageGroup::Sho.0);
    assert_eq!(
        pi::check_wield_requirements(&mut w, player(), item),
        WeenieError::ArmorRequiresSpecificHeritage
    );
    obj_mut(&mut w, item)
        .set_property(PropertyInt::HeritageSpecificArmor, HeritageGroup::Aluvian.0);

    obj_mut(&mut w, item).set_property(PropertyInstanceId::AllowedWielder, 0x5000_0002);
    assert_eq!(
        pi::check_wield_requirements(&mut w, player(), item),
        WeenieError::YouDoNotOwnThatItem
    );
    obj_mut(&mut w, item).set_property(PropertyInstanceId::AllowedWielder, PLAYER);

    // slot 2 (Level 50) fails before slot 3 (a missing attribute record would throw)
    obj_mut(&mut w, player()).set_property(PropertyInt::Level, 10);
    let i = obj_mut(&mut w, item);
    i.set_property(PropertyInt::WieldRequirements, WieldRequirement::Attrib.0);
    i.set_property(
        PropertyInt::WieldSkillType,
        i32::from(PropertyAttribute::Strength.0),
    );
    i.set_property(PropertyInt::WieldDifficulty, 100);
    i.set_property(PropertyInt::WieldRequirements2, WieldRequirement::Level.0);
    i.set_property(PropertyInt::WieldDifficulty2, 50);
    assert_eq!(
        pi::check_wield_requirements(&mut w, player(), item),
        WeenieError::LevelTooLow
    );

    // with the server property off, nothing is checked
    assert!(pm::modify_bool(&w, "use_wield_requirements", false));
    assert_eq!(
        pi::check_wield_requirements(&mut w, player(), item),
        WeenieError::None
    );
}

/// `PutItemInContainerEvent.IsDoubleSend`: same item, container and placement, the newer one
/// less than 0.5 s old and less than 0.5 s after the older.
#[test]
fn a_double_send_is_the_same_request_within_half_a_second() {
    let t0 = DotNetDateTime::new(2026, 1, 1);
    let older = PutItemInContainerEvent::new(1, 2, 0, t0);
    let newer = PutItemInContainerEvent::new(1, 2, 0, t0 + TimeSpan::from_seconds(0.2));
    assert!(newer.is_double_send(&older, t0 + TimeSpan::from_seconds(0.3)));
    assert!(
        !newer.is_double_send(&older, t0 + TimeSpan::from_seconds(0.7)),
        "the newer one is too old"
    );
    let late = PutItemInContainerEvent::new(1, 2, 0, t0 + TimeSpan::from_seconds(0.5));
    assert!(
        !late.is_double_send(&older, t0 + TimeSpan::from_seconds(0.6)),
        "exactly 0.5 s apart is not less"
    );
    let other = PutItemInContainerEvent::new(1, 3, 0, t0 + TimeSpan::from_seconds(0.1));
    assert!(!other.is_double_send(&older, t0 + TimeSpan::from_seconds(0.2)));
}

/// `CheckUniques` over `UniqueTable`: a second unique is refused with the client's
/// TooManyUniqueItems error (V296).
#[test]
fn check_uniques_refuses_a_second_unique_with_the_unique_error() {
    let mut w = world();
    let first = give(&mut w, UNIQUE);
    let second = spawn(&mut w, UNIQUE);
    let table = UniqueTable::new(&w, &[first, second]);
    assert_eq!(
        table.entries.get(&UNIQUE).map(|e| (e.count, e.max)),
        Some((2, 1))
    );

    start_capture();
    assert!(!pi::check_uniques(&mut w, player(), &[second], None));
    let sent = take_sent();
    assert_eq!(kinds(&sent), [WEENIE_ERROR]);
    assert_eq!(
        u32_at(payload(&sent[0].2), 0),
        u32::try_from(WeenieError::TooManyUniqueItems.0).unwrap()
    );

    assert!(container::try_remove_from_inventory(
        &mut w,
        player(),
        first,
        false
    ));
    assert!(pi::check_uniques(&mut w, player(), &[second], None));
}

/// `ItemsToReceive`: slots per stack (rounded up), burden per unit, and the limits.
#[test]
fn items_to_receive_counts_stacks_slots_and_burden() {
    let w = world();
    let mut r = ItemsToReceive::new(&w, player());
    assert!(r.add(&w, COIN, 250));
    assert_eq!(
        (
            r.required_inventory_slots,
            r.required_container_slots,
            r.required_burden,
            r.required_slots()
        ),
        (3, 0, 500, 3)
    );
    assert!(r.add(&w, PACK, 2));
    assert_eq!((r.required_container_slots, r.required_burden), (2, 600));
    // strength 100: capacity 15000, available 45000
    assert!(!r.add(&w, ITEM, 10_000), "50,000 burden exceeds 45,000");
    assert!(r.player_exceeds_available_burden());

    // a whole number of stacks takes no extra slot
    let mut r = ItemsToReceive::new(&w, player());
    r.add(&w, COIN, 200);
    assert_eq!(r.required_inventory_slots, 2);
}

/// `HasEnoughBurdenToAddToInventory`: exactly three times the capacity still fits.
#[test]
fn burden_up_to_three_times_capacity_fits() {
    let mut w = world();
    // strength 100: capacity 15,000
    obj_mut(&mut w, player()).set_encumbrance_val(Some(40_000));
    let p = obj(&w, player());
    assert_eq!(pi::get_encumbrance_capacity(&w, p), 15_000);
    assert!(pi::has_enough_burden_to_add_to_inventory_total(
        &w, p, 5_000
    ));
    assert!(!pi::has_enough_burden_to_add_to_inventory_total(
        &w, p, 5_001
    ));
    assert_eq!(pi::get_available_burden(&w, p), 5_000);
}

// ------------------------------------------------------------------ hand-derived: handlers

/// A move within the player's own packs (`HandleActionPutItemInContainer`, the self-contained
/// branch): removed and re-added, then `PublicUpdateInstanceID(Container)` and `ContainId`.
#[test]
fn moving_an_item_between_packs_answers_container_and_contain_id() {
    let mut w = world();
    let pack = give(&mut w, PACK);
    let item = give(&mut w, ITEM);
    let before = obj(&w, player()).encumbrance_val();

    start_capture();
    pi::handle_action_put_item_in_container(&mut w, player(), item.full(), pack.full(), 0);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [INSTANCE_ID, CONTAIN_ID]);
    let contain = payload(&sent[1].2);
    assert_eq!(
        (u32_at(contain, 0), u32_at(contain, 4), u32_at(contain, 8)),
        (item.full(), pack.full(), 0)
    );
    assert_eq!(
        obj(&w, item).wo.world_object_properties.container,
        Some(pack)
    );
    assert_eq!(
        obj(&w, player()).encumbrance_val(),
        before,
        "the player's burden is unchanged"
    );
    assert_eq!(
        pi::find_object(&w, player(), item, SearchLocations::MyInventory).found_in_container,
        Some(pack)
    );
}

/// Every failure answers the client: an unknown item, a stuck one, an unknown container.
#[test]
fn a_refused_move_is_always_answered() {
    let mut w = world();
    let item = give(&mut w, ITEM);

    start_capture();
    pi::handle_action_put_item_in_container(&mut w, player(), 0x8000_7777, PLAYER, 0);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [TRANSIENT, SAVE_FAILED]);
    assert_eq!(transient_text(&sent[0].2), "Source item not found!");
    assert_eq!(
        (
            u32_at(payload(&sent[1].2), 0),
            u32_at(payload(&sent[1].2), 4)
        ),
        (0x8000_7777, 0)
    );

    pi::handle_action_put_item_in_container(&mut w, player(), item.full(), 0x8000_7777, 0);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [TRANSIENT, SAVE_FAILED]);
    assert_eq!(transient_text(&sent[0].2), "Target container not found!");

    obj_mut(&mut w, item).set_property(PropertyBool::Stuck, true);
    pi::handle_action_put_item_in_container(&mut w, player(), item.full(), PLAYER, 0);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [WEENIE_ERROR, SAVE_FAILED]);
    assert_eq!(
        u32_at(payload(&sent[0].2), 0),
        u32::try_from(WeenieError::Stuck.0).unwrap()
    );
}

/// Split within the pack (`HandleActionStackableSplitToContainer`, self-contained): the new stack
/// is created and contained, then the old one shrinks; merging it back consumes it.
#[test]
fn split_then_merge_within_the_pack() {
    let mut w = world();
    let coins = give(&mut w, COIN);
    let burden = obj(&w, player()).encumbrance_val();

    start_capture();
    pi::handle_action_stackable_split_to_container(&mut w, player(), coins.full(), PLAYER, 0, 4);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [0xF745, CONTAIN_ID, STACK_SIZE]);
    assert_eq!(obj(&w, coins).stack_size(), Some(6));
    let new_stack = ObjectGuid::new(u32_at(payload(&sent[1].2), 0));
    assert_eq!(obj(&w, new_stack).stack_size(), Some(4));
    assert_eq!(
        obj(&w, player()).encumbrance_val(),
        burden,
        "the same 10 coins in two stacks"
    );
    assert_eq!(
        container::inventory_values(&w, player()),
        vec![coins, new_stack]
    );

    // a part merge: both sizes change
    pi::handle_action_stackable_merge(&mut w, player(), new_stack.full(), coins.full(), 1);
    assert_eq!(kinds(&take_sent()), [STACK_SIZE, STACK_SIZE]);
    assert_eq!(
        (obj(&w, new_stack).stack_size(), obj(&w, coins).stack_size()),
        (Some(3), Some(7))
    );

    // the whole source: InventoryRemoveObject, the source is destroyed, the target grows
    pi::handle_action_stackable_merge(&mut w, player(), new_stack.full(), coins.full(), 3);
    assert_eq!(kinds(&take_sent()), [REMOVE_OBJECT, STACK_SIZE]);
    assert!(w.objects.get(new_stack).is_none());
    assert_eq!(obj(&w, coins).stack_size(), Some(10));
    assert_eq!(obj(&w, player()).encumbrance_val(), burden);

    // a whole source that exactly fills the target is still consumed
    let full = give(&mut w, COIN);
    obj_mut(&mut w, full).set_stack_size(Some(90));
    take_sent();
    pi::handle_action_stackable_merge(&mut w, player(), coins.full(), full.full(), 10);
    assert_eq!(kinds(&take_sent()), [REMOVE_OBJECT, STACK_SIZE]);
    assert!(w.objects.get(coins).is_none());
    assert_eq!(obj(&w, full).stack_size(), Some(100));
    let coins = give(&mut w, COIN);

    // refusals: different stacks, too many, a bad amount
    let notes = give(&mut w, OTHER_COIN);
    pi::handle_action_stackable_merge(&mut w, player(), notes.full(), coins.full(), 1);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [TRANSIENT, SAVE_FAILED]);
    assert_eq!(
        u32_at(payload(&sent[1].2), 4),
        u32::try_from(WeenieError::YouCannotMergeDifferentStacks.0).unwrap()
    );
    pi::handle_action_stackable_split_to_container(&mut w, player(), coins.full(), PLAYER, 0, 10);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [TRANSIENT, SAVE_FAILED]);
    assert_eq!(transient_text(&sent[0].2), "Split amount not valid!");
}

/// Wield from the pack (`HandleActionGetAndWieldItem`, own item) and dequip back into it
/// (`HandleActionPutItemInContainer` of an equipped item).
#[test]
fn wield_from_the_pack_and_dequip_back() {
    let mut w = world();
    let sword = give(&mut w, SWORD);
    let burden = obj(&w, player()).encumbrance_val();

    start_capture();
    pi::handle_action_get_and_wield_item(&mut w, player(), sword.full(), EquipMask::MeleeWeapon);
    let sent = take_sent();
    assert_eq!(kinds(&sent), [INSTANCE_ID, PUBLIC_INT, WIELD_ITEM, SOUND]);
    assert_eq!(
        (
            u32_at(payload(&sent[2].2), 0),
            u32_at(payload(&sent[2].2), 4)
        ),
        (sword.full(), EquipMask::MeleeWeapon.0)
    );
    assert_eq!(
        obj(&w, sword).current_wielded_location(),
        Some(EquipMask::MeleeWeapon)
    );
    assert_eq!(obj(&w, sword).wielder, Some(player()));
    assert!(container::inventory_values(&w, player()).is_empty());
    assert_eq!(
        obj(&w, player()).encumbrance_val(),
        burden,
        "equipped items still count"
    );

    pi::handle_action_put_item_in_container(&mut w, player(), sword.full(), PLAYER, 0);
    let sent = take_sent();
    assert_eq!(
        kinds(&sent),
        [
            INSTANCE_ID,
            PUBLIC_INT,
            PICKUP_EVENT,
            SOUND,
            INSTANCE_ID,
            CONTAIN_ID
        ]
    );
    assert_eq!(obj(&w, sword).current_wielded_location(), None);
    assert_eq!(container::inventory_values(&w, player()), vec![sword]);
    assert_eq!(obj(&w, player()).encumbrance_val(), burden);

    // an item that cannot go in that slot is refused with the plain answer
    pi::handle_action_get_and_wield_item(
        &mut w,
        player(),
        sword.full(),
        EquipMask::Shield | EquipMask::HeadWear,
    );
    assert_eq!(kinds(&take_sent()), [SAVE_FAILED]);
}

/// `TryConsumeFromInventoryWithNetworking`: part of a stack, then all of it.
#[test]
fn consuming_part_then_all_of_a_stack() {
    let mut w = world();
    let coins = give(&mut w, COIN);
    start_capture();
    assert!(pi::try_consume_from_inventory_with_networking(
        &mut w,
        player(),
        coins,
        3
    ));
    assert_eq!(kinds(&take_sent()), [STACK_SIZE, PRIVATE_INT, PRIVATE_INT]);
    assert_eq!(obj(&w, coins).stack_size(), Some(7));
    assert_eq!(obj(&w, player()).coin_value(), Some(35));
    assert!(pi::try_consume_from_inventory_with_networking_wcid(
        &mut w,
        player(),
        COIN,
        i32::MAX
    ));
    assert_eq!(
        kinds(&take_sent()),
        [REMOVE_OBJECT, PRIVATE_INT, PRIVATE_INT]
    );
    assert_eq!(obj(&w, player()).coin_value(), Some(0));
    assert!(w.objects.get(coins).is_none());
    assert_eq!(obj(&w, player()).encumbrance_val(), Some(0));
}

/// Divergence: V407
/// A one-handed sword that may go in the off hand is wielded there at the end of retail and
/// refused in an era without dual wield.
#[test]
fn an_era_without_dual_wield_refuses_a_weapon_in_the_off_hand() {
    for (era, wielded) in [
        (empyrean_common::era::EraId::Eor, true),
        (empyrean_common::era::EraId::Infiltration, false),
    ] {
        let mut w = world();
        w.era = era.rules();
        let sword = give(&mut w, SWORD);
        w.objects.get_mut(sword).expect("the sword").set_property(
            PropertyInt::ValidLocations,
            i32::try_from((EquipMask::MeleeWeapon | EquipMask::Shield).0).expect("mask"),
        );
        start_capture();
        pi::handle_action_get_and_wield_item(&mut w, player(), sword.full(), EquipMask::Shield);
        let sent = kinds(&take_sent());
        assert_eq!(sent.contains(&WIELD_ITEM), wielded, "{era}: {sent:?}");
        assert_eq!(
            obj(&w, sword).current_wielded_location() == Some(EquipMask::Shield),
            wielded,
            "{era}"
        );
        if !wielded {
            assert_eq!(sent, [SAVE_FAILED]);
        }
    }
}

/// Divergence: V424, V428
/// A world without aetheria, cloaks or trinkets refuses wielding into the sigil slot, the cloak
/// slot or the trinket slot, telling the player why; a world with them wields there as ACE does.
#[test]
fn a_world_without_aetheria_cloaks_or_trinkets_refuses_their_slots() {
    use empyrean_common::era::{with_features, EraFeatures, EraId};
    for (slot, system) in [
        (EquipMask::SigilOne, "aetheria"),
        (EquipMask::Cloak, "cloaks"),
        (EquipMask::TrinketOne, "trinkets"),
    ] {
        for has in [true, false] {
            let mut w = world();
            let mut features = EraFeatures::ALL;
            features.set(system, has);
            w.era = with_features(EraId::Eor.rules(), features);
            // The first sigil slot unlocked, as the aetheria quest leaves it.
            w.objects
                .get_mut(player())
                .expect("the player")
                .set_property(PropertyInt::AetheriaBitfield, 1);
            let item = give(&mut w, ITEM);
            w.objects.get_mut(item).expect("the item").set_property(
                PropertyInt::ValidLocations,
                i32::try_from(slot.0).expect("mask"),
            );
            start_capture();
            pi::handle_action_get_and_wield_item(&mut w, player(), item.full(), slot);
            let sent = kinds(&take_sent());
            assert_eq!(
                obj(&w, item).current_wielded_location() == Some(slot),
                has,
                "{system} {has}: {sent:?}"
            );
            assert_eq!(sent.contains(&WIELD_ITEM), has, "{system} {has}: {sent:?}");
            if !has {
                assert!(sent.contains(&SAVE_FAILED), "{system}: {sent:?}");
            }
        }
    }
}

/// Divergence: V429
/// At login an item worn in a slot the world lacks comes off into the pack, as an unwield does;
/// with no room in the pack the unwield is refused, the item stays worn and the player is told
/// to make room. A full main pack sends it to a side pack with room. A world with the slot leaves
/// it worn.
#[test]
fn at_login_an_item_in_a_slot_the_world_lacks_goes_to_the_pack() {
    use empyrean_common::era::{with_features, EraFeatures, EraId};
    let eor = EraId::Eor.rules();
    let no_cloaks = with_features(
        eor,
        EraFeatures {
            cloaks: false,
            ..eor.features
        },
    );
    let wear_cloak = |w: &mut World| {
        let cloak = give(w, ITEM);
        obj_mut(w, cloak).set_property(
            PropertyInt::ValidLocations,
            i32::try_from(EquipMask::Cloak.0).expect("mask"),
        );
        pi::handle_action_get_and_wield_item(w, player(), cloak.full(), EquipMask::Cloak);
        assert_eq!(
            obj(w, cloak).current_wielded_location(),
            Some(EquipMask::Cloak)
        );
        cloak
    };

    // The world has cloaks: the audit leaves it on.
    let mut w = world();
    let cloak = wear_cloak(&mut w);
    pi::audit_equipped_items(&mut w, player());
    assert_eq!(
        obj(&w, cloak).current_wielded_location(),
        Some(EquipMask::Cloak)
    );

    // Restarted without cloaks: the next login's audit puts it in the pack.
    w.era = no_cloaks;
    start_capture();
    pi::audit_equipped_items(&mut w, player());
    let sent = take_sent();
    assert_eq!(obj(&w, cloak).current_wielded_location(), None);
    assert!(container::inventory_values(&w, player()).contains(&cloak));
    assert!(!ce::equipped_objects_values(&w, player()).contains(&cloak));
    assert!(
        kinds(&sent).contains(&0x0022),
        "the client is told where it went: {sent:?}"
    );

    // A full pack: the unwield is refused and the cloak stays on.
    let mut w = world();
    let cloak = wear_cloak(&mut w);
    while container::get_free_inventory_slots(&w, player(), true) > 0 {
        give(&mut w, ITEM);
    }
    w.era = no_cloaks;
    pi::audit_equipped_items(&mut w, player());
    assert_eq!(
        obj(&w, cloak).current_wielded_location(),
        Some(EquipMask::Cloak)
    );
    assert!(
        ce::equipped_objects_values(&w, player()).contains(&cloak),
        "not lost"
    );

    // A full main pack and a side pack with room: it goes to the side pack.
    let mut w = world();
    let pack = give(&mut w, PACK);
    let cloak = wear_cloak(&mut w);
    while container::get_free_inventory_slots(&w, player(), false) > 0 {
        give(&mut w, ITEM);
    }
    w.era = no_cloaks;
    pi::audit_equipped_items(&mut w, player());
    assert_eq!(obj(&w, cloak).current_wielded_location(), None);
    assert!(container::inventory_values(&w, pack).contains(&cloak));
}
