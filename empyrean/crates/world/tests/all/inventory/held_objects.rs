//! ACE: Source/ACE.Server/WorldObjects/WorldObject.cs::Destroy
//! Destroying a container/creature destroys contents; loot still listed outlives destroy inside
//! corpse; login ctor fills inventory/equipped/children; new character title and options.
//! Fixture: isolated world state.

use empyrean_entity::enums::{
    CharacterOption, EquipMask, ParentLocation, PhysicsDescriptionFlag, Placement, PositionType,
    PropertyInstanceId, PropertyInt,
};
use empyrean_entity::models::PropertiesPosition;
use empyrean_entity::ObjectGuid;
use empyrean_store::adapter::biota_converter::BiotaConverter;
use empyrean_store::models::shard::Character;
use empyrean_world::dispatch::Class;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::guid_manager;
use empyrean_world::world_objects::world_object::{self as wo, CtorEnv};
use empyrean_world::world_objects::world_object_equipment::HeldItem;
use empyrean_world::world_objects::{
    container, creature_equipment as ce, player, player_character, world_object_networking,
};
use empyrean_world::World;

use super::containers::{
    add, obj, spawn, world, COIN, CREATURE, ITEM, PACK, PLAYER, SIDE, STORAGE, SWORD,
};

/// The number of guids `GuidManager.RecycleDynamicGuid` has taken back.
fn recycled(w: &mut World) -> usize {
    let info = guid_manager::get_dynamic_guid_debug_info(w);
    let n = info
        .rsplit("recycled GUIDs available: ")
        .next()
        .expect("the count");
    n.trim().parse().unwrap_or_else(|_| panic!("{info}"))
}

// ---- WorldObject.Destroy ------------------------------------------------------------------------

/// `Destroy`: "If this is a container or a creature, all of the inventory and/or equipped objects
/// will also be destroyed" (`foreach (var item in container.Inventory.Values) item.Destroy();`,
/// then the same over `creature.EquippedObjects.Values`), each recursively, each recycling its
/// dynamic guid.
#[test]
fn destroying_a_container_or_creature_destroys_what_it_holds() {
    let mut w = world();
    let pack = spawn(&mut w, PACK);
    let side = spawn(&mut w, SIDE);
    let (a, b) = (spawn(&mut w, ITEM), spawn(&mut w, ITEM));
    assert!(add(&mut w, pack, a));
    assert!(add(&mut w, pack, side));
    assert!(add(&mut w, side, b));

    wo::destroy(&mut w, pack, true, false);
    for g in [pack, side, a, b] {
        assert!(w.objects.get(g).is_none(), "0x{g} destroyed");
    }
    assert_eq!(recycled(&mut w), 4);

    let creature = spawn(&mut w, CREATURE);
    let sword = spawn(&mut w, SWORD);
    let carried = spawn(&mut w, ITEM);
    assert!(ce::try_equip_object(
        &mut w,
        creature,
        sword,
        EquipMask::MeleeWeapon
    ));
    assert!(add(&mut w, creature, carried));
    let bystander = spawn(&mut w, ITEM);

    wo::destroy(&mut w, creature, true, false);
    for g in [creature, sword, carried] {
        assert!(w.objects.get(g).is_none(), "0x{g} destroyed");
    }
    assert!(
        w.objects.get(bystander).is_some(),
        "an unrelated item stays"
    );
    assert_eq!(recycled(&mut w), 7);
}

/// Loot the creature still lists outlives its destroy inside the corpse.
/// V291.
#[test]
fn loot_the_creature_still_lists_outlives_its_destroy_inside_the_corpse() {
    let mut w = world();
    let creature = spawn(&mut w, CREATURE);
    let corpse = spawn(&mut w, STORAGE); // any container stands for the corpse
    let loot = spawn(&mut w, ITEM);
    assert!(add(&mut w, creature, loot));
    assert!(
        add(&mut w, corpse, loot),
        "added without leaving the creature's Inventory"
    );

    wo::destroy(&mut w, creature, true, false);
    assert!(w.objects.get(creature).is_none());
    assert!(
        obj(&w, loot).wo.world_object.is_destroyed,
        "destroyed with the creature"
    );
    assert_eq!(recycled(&mut w), 2, "its guid is recycled, as ACE's is");
    assert_eq!(
        container::inventory_values(&w, corpse),
        vec![loot],
        "the corpse still lists it"
    );

    wo::destroy(&mut w, corpse, true, false);
    assert!(w.objects.get(corpse).is_none());
    assert!(w.objects.get(loot).is_none(), "gone with the corpse");
    assert_eq!(recycled(&mut w), 3, "recycled once only");
}

// ---- the login constructor ----------------------------------------------------------------------

const ME: u32 = 0x5000_0001;

/// A possession of `ME` from `wcid`: its shard biota, with the object's guid.
fn possession(
    w: &World,
    wcid: u32,
    guid: u32,
    set: impl FnOnce(&mut wo::WorldObject),
) -> empyrean_store::models::shard::Biota {
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), ObjectGuid::new(guid))
    })
    .expect("an item");
    set(&mut o);
    BiotaConverter::convert_from_entity_biota(&o.biota, false)
}

/// `Player(Biota, inventory, wieldedItems, character, session)`: `SortBiotasIntoInventory`
/// (walked from the end), `AddBiotasToEquippedObjects` (the burden, the gear rating cache, then
/// `SetChildren`: the wielded sword becomes a `Children` entry in the right hand, placed at
/// `RightHandCombat` at the player's `Location`), then `UpdateCoinValue(false)`. The player's
/// CreateObject then carries the `Children` physics flag.
#[test]
fn the_login_constructor_fills_inventory_equipped_objects_and_children() {
    let mut w = world();
    let (coins, item, sword) = (0x8000_0101, 0x8000_0102, 0x8000_0103);
    let inventory = vec![
        possession(&w, COIN, coins, |o| o.set_container_id(Some(ME))),
        possession(&w, ITEM, item, |o| o.set_container_id(Some(ME))),
    ];
    let wielded = vec![possession(&w, SWORD, sword, |o| {
        o.set_property(PropertyInstanceId::Wielder, ME);
        o.set_property(
            PropertyInt::CurrentWieldedLocation,
            EquipMask::MeleeWeapon.0.cast_signed(),
        );
        o.set_property(PropertyInt::GearDamage, 3);
    })];

    let mut biota = empyrean_entity::Biota {
        id: ME,
        weenie_class_id: PLAYER,
        weenie_type: empyrean_entity::enums::WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    let here = PropertiesPosition {
        obj_cell_id: 0xA9B4_0019,
        position_x: 84.0,
        position_y: 7.0,
        position_z: 94.0,
        rotation_w: 1.0,
        ..Default::default()
    };
    biota.set_property_position(PositionType::Location, here);
    let character = Character {
        id: ME,
        name: "Tester".to_owned(),
        ..Default::default()
    };
    let p = CtorEnv::with_world(&w, |env| {
        player::player_from_biota_with_character(
            env,
            Class::Player,
            biota,
            inventory,
            wielded,
            character,
            None,
        )
    });
    assert!(
        p.player
            .as_ref()
            .unwrap()
            .player
            .login_possessions
            .is_some(),
        "carried until the player is in the store"
    );
    let me = p.guid;
    w.objects.insert(p).expect("fresh");
    player::player_ctor_load_possessions(&mut w, me);

    let g = ObjectGuid::new;
    assert_eq!(container::inventory_values(&w, me), [g(item), g(coins)]);
    assert_eq!(ce::equipped_objects_values(&w, me), [g(sword)]);
    assert_eq!(obj(&w, g(sword)).wo.world_object_properties.container, None);
    assert_eq!(
        obj(&w, g(item)).wo.world_object_properties.container,
        Some(me)
    );

    let held = &obj(&w, me).wo.world_object_properties.children;
    assert_eq!(
        held,
        &[HeldItem {
            guid: sword,
            location_id: ParentLocation::RightHand.0,
            equip_mask: EquipMask::MeleeWeapon
        }]
    );
    let s = obj(&w, g(sword));
    assert_eq!(
        (s.placement(), s.parent_location()),
        (
            Some(Placement::RightHandCombat),
            Some(ParentLocation::RightHand)
        )
    );
    assert_eq!(
        s.location().map(|l| l.to_string()),
        obj(&w, me).location().map(|l| l.to_string())
    );
    assert!(s.location().is_some());
    assert_eq!(
        ce::get_equipped_items_rating_sum(&w, me, PropertyInt::GearDamage),
        3
    );

    // the stack of 10 at StackUnitValue 5
    assert_eq!(obj(&w, g(coins)).value(), Some(50));
    assert_eq!(obj(&w, me).coin_value(), Some(50));
    let flags = world_object_networking::calculated_physics_description_flag(&w, me);
    assert!(
        (flags & PhysicsDescriptionFlag::Children) == PhysicsDescriptionFlag::Children,
        "{flags:?}"
    );
    assert!(obj(&w, me)
        .player
        .as_ref()
        .unwrap()
        .player
        .login_possessions
        .is_none());
}

// ---- PlayerFactory: titles and options on the new character --------------------------------------

/// `AddTitle` and `SetCharacterOption` on a new character (the player `PlayerFactory.Create`
/// builds, staged in the store for the call): the title goes in the registry and becomes the
/// display title, `NumCharacterTitles` counts it, the option bit is set, and
/// `CharacterChangesDetected` is set (ACE's bodies); an undefined title changes nothing. No message
/// is due before the first enter-world (the player has no session).
#[test]
fn a_new_characters_title_and_options_use_the_player_character_bodies() {
    let mut w = world();
    let weenie = w.content.get_cached_weenie(PLAYER).expect("test weenie");
    let p = CtorEnv::with_world(&w, |env| {
        player::player_from_weenie(env, Class::Player, weenie, ObjectGuid::new(ME), 1)
    });
    w.objects.insert(p).expect("fresh");
    let me = ObjectGuid::new(ME);
    let changed = |w: &World| {
        w.objects
            .get(me)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player_database
            .character_changes_detected
    };
    let character = |w: &World| {
        w.objects
            .get(me)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player
            .character
            .clone()
            .expect("the constructor's Character")
    };
    w.objects
        .get_mut(me)
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_database
        .character_changes_detected = false;

    player_character::add_title(&mut w, me, 0x7FFF_FFFF, true);
    let p = w.objects.get(me).unwrap();
    assert_eq!(
        (
            p.num_character_titles(),
            p.character_title_id(),
            changed(&w)
        ),
        (None, None, false),
        "undefined: nothing"
    );

    player_character::add_title(&mut w, me, 1, true);
    assert_eq!(
        character(&w)
            .character_properties_title_book
            .iter()
            .map(|t| t.title_id)
            .collect::<Vec<_>>(),
        [1]
    );
    let p = w.objects.get(me).unwrap();
    assert_eq!(
        (p.num_character_titles(), p.character_title_id()),
        (Some(1), Some(1))
    );
    assert!(changed(&w));

    w.objects
        .get_mut(me)
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_database
        .character_changes_detected = false;
    player_character::set_character_option(
        &mut w,
        me,
        CharacterOption::ListenToPKDeathMessages,
        true,
    );
    let bit = CharacterOption::ListenToPKDeathMessages
        .character_options2()
        .expect("an options-2 bit")
        .0
        .cast_signed();
    assert_eq!(character(&w).character_options_2 & bit, bit);
    assert!(changed(&w));
    player_character::set_character_option(
        &mut w,
        me,
        CharacterOption::ListenToPKDeathMessages,
        false,
    );
    assert_eq!(character(&w).character_options_2 & bit, 0);
}
