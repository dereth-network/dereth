//! Vectors: fixtures/vectors/position/
//! WorldObject constructors per class, WorldObjectFactory and pure PositionExtensions members
//! replay ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_common::vectors::{self, f32_of, same_f32, u64_of, Case};
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    ActivationResponse, AttunedStatus, Channel, CombatStyle, CombatUse, ContainerType,
    DestinationType, ItemType, ObjectDescriptionFlag, Placement, PropertyBool, PropertyFloat,
    PropertyInt, PropertyString, Quadrant, RadarBehavior, Skill, Usable, WeenieType,
};
use empyrean_entity::models::properties_create_list::PropertiesCreateList;
use empyrean_entity::{Biota, LandblockId, ObjectGuid, Position, Quaternion, Weenie};
use empyrean_world::dispatch::{self, Class};
use empyrean_world::entity::position_extensions as pe;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::world_objects::player::{player_from_biota_with_character, player_from_weenie};
use empyrean_world::world_objects::world_object::{self as wo, CtorEnv, WorldObject};
use empyrean_world::World;
use serde_json::Value;

// ------------------------------------------------------------------ vector replay helpers

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) {
    let file = vectors::load_named("position", name);
    let mut failures = Vec::new();
    for case in &file.cases {
        if let Err(got) = each(case) {
            failures.push(format!(
                "in {} expected {} got {got}",
                case.input, case.output
            ));
        }
    }
    assert!(!file.cases.is_empty(), "position/{name}: no cases");
    assert!(
        failures.is_empty(),
        "position/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn f(v: &Value, key: &str) -> f32 {
    f32_of(&v[key]).unwrap_or_else(|| panic!("{key}: not a float in {v}"))
}

fn u(v: &Value, key: &str) -> u32 {
    u32::try_from(u64_of(&v[key]).unwrap_or_else(|| panic!("{key}: not an integer in {v}")))
        .expect("fits u32")
}

fn quat(v: &Value) -> Quaternion {
    Quaternion::new(f(v, "x"), f(v, "y"), f(v, "z"), f(v, "w"))
}

fn same_quat(a: Quaternion, b: Quaternion) -> bool {
    same_f32(a.x, b.x) && same_f32(a.y, b.y) && same_f32(a.z, b.z) && same_f32(a.w, b.w)
}

/// `new Position { LandblockId = new LandblockId(cell), PositionX = x, PositionY = y, PositionZ = z }`.
fn pos(v: &Value) -> Position {
    let mut p = Position::new();
    p.set_landblock_id(LandblockId::new(u(v, "cell")));
    p.position_x = f(v, "x");
    p.position_y = f(v, "y");
    p.position_z = v.get("z").map_or(0.0, |_| f(v, "z"));
    p
}

// ------------------------------------------------------------------ PositionExtensions (F13)

#[test]
fn position_is_rotation_valid_matches_ace() {
    replay("is_rotation_valid", |c| {
        let got = pe::is_rotation_valid(quat(&c.input));
        (got == c.output["valid"].as_bool().expect("valid"))
            .then_some(())
            .ok_or_else(|| got.to_string())
    });
}

#[test]
fn position_attempt_to_fix_rotation_matches_ace() {
    let wo = empyrean_world::world_objects::world_object::WorldObject::default();
    replay("attempt_to_fix_rotation", |c| {
        let mut p = Position::new();
        p.set_rotation(quat(&c.input));
        let success = pe::attempt_to_fix_rotation(
            &mut p,
            &wo,
            empyrean_entity::enums::PositionType::Location,
        );
        let ok = success == c.output["success"].as_bool().expect("success")
            && same_quat(p.rotation(), quat(&c.output["rotation"]));
        ok.then_some(())
            .ok_or_else(|| format!("{success} {:?}", p.rotation()))
    });
}

#[test]
fn position_to_global_matches_ace() {
    replay("to_global", |c| {
        let v = pe::to_global(
            &pos(&c.input),
            c.input["skip_indoors"].as_bool().expect("skip"),
        );
        let o = &c.output;
        let ok = same_f32(v.x, f(o, "x")) && same_f32(v.y, f(o, "y")) && same_f32(v.z, f(o, "z"));
        ok.then_some(()).ok_or_else(|| format!("{v:?}"))
    });
}

#[test]
fn position_get_outdoor_cell_matches_ace() {
    replay("get_outdoor_cell", |c| {
        let got = pe::get_outdoor_cell(&pos(&c.input));
        (u64::from(got) == c.output.as_u64().expect("u32"))
            .then_some(())
            .ok_or_else(|| format!("{got:#X}"))
    });
}

#[test]
fn position_get_map_coords_matches_ace() {
    replay("get_map_coords", |c| {
        let p = pos(&c.input);
        let coords = pe::get_map_coords(&p);
        let s = pe::get_map_coord_str(&p);
        let coords_ok = match (&coords, &c.output["coords"]) {
            (None, Value::Null) => true,
            (Some(m), o) if !o.is_null() => same_f32(m.x, f(o, "x")) && same_f32(m.y, f(o, "y")),
            _ => false,
        };
        let str_ok = s.as_deref() == c.output["str"].as_str();
        (coords_ok && str_ok)
            .then_some(())
            .ok_or_else(|| format!("{coords:?} {s:?}"))
    });
}

#[test]
fn position_translate_matches_ace() {
    replay("translate", |c| {
        let mut p = pos(&c.input);
        pe::translate(&mut p, u(&c.input, "block_cell"));
        let o = &c.output;
        let ok = p.cell() == u(o, "cell")
            && same_f32(p.position_x, f(o, "x"))
            && same_f32(p.position_y, f(o, "y"))
            && same_f32(p.position_z, f(o, "z"));
        ok.then_some(()).ok_or_else(|| {
            format!(
                "{:#X} {} {} {}",
                p.cell(),
                p.position_x,
                p.position_y,
                p.position_z
            )
        })
    });
}

#[test]
fn position_cell_dist_matches_ace() {
    // no dungeon cells in the fake dats: `GetIndoorCell` answers the position's own cell
    let w = world();
    replay("cell_dist", |c| {
        let got = pe::cell_dist(&w, &pos(&c.input["p1"]), &pos(&c.input["p2"]));
        (u64::from(got) == c.output.as_u64().expect("u32"))
            .then_some(())
            .ok_or_else(|| got.to_string())
    });
}

#[test]
fn position_load_repairs_bad_rotation_without_not_ported() {
    use empyrean_entity::enums::PositionType;
    let mut wo = empyrean_world::world_objects::world_object::WorldObject::default();
    let mut p = Position::new();
    p.set_landblock_id(LandblockId::new(0x7D64_0001));
    p.set_rotation(Quaternion::new(0.0, 0.0, 0.0, 2.0));
    wo.biota.set_position(PositionType::Location, &p);
    empyrean_common::not_ported::take_local();

    let loaded = wo.get_position(PositionType::Location).expect("location");
    assert!(
        same_quat(loaded.rotation(), Quaternion::IDENTITY),
        "{:?}",
        loaded.rotation()
    );
    // An already valid rotation is left alone.
    p.set_rotation(Quaternion::new(0.0, 0.0, 0.6, 0.8));
    wo.biota.set_position(PositionType::Home, &p);
    let home = wo.get_position_mut(PositionType::Home).expect("home");
    assert!(same_quat(
        home.rotation(),
        Quaternion::new(0.0, 0.0, 0.6, 0.8)
    ));
    assert!(empyrean_common::not_ported::take_local().is_empty());
}

// ------------------------------------------------------------------ construction helpers

const UNIX_TIME: f64 = 1_790_000_000.75;

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: UNIX_TIME,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("empty fake dats"),
    )
}

fn weenie(wcid: u32, weenie_type: WeenieType) -> Weenie {
    Weenie {
        weenie_class_id: wcid,
        weenie_type,
        ..Default::default()
    }
}

fn set_int(w: &mut Weenie, k: PropertyInt, v: i32) {
    w.properties_int
        .get_or_insert_with(DotNetDict::new)
        .insert(k, v);
}

fn set_bool(w: &mut Weenie, k: PropertyBool, v: bool) {
    w.properties_bool
        .get_or_insert_with(DotNetDict::new)
        .insert(k, v);
}

fn set_string(w: &mut Weenie, k: PropertyString, v: &str) {
    w.properties_string
        .get_or_insert_with(DotNetDict::new)
        .insert(k, v.to_owned());
}

/// A player weenie: `Player.SetEphemeralValues` reads `CombatTableDID.Value` (ACE's human table).
fn player_weenie() -> Weenie {
    let mut p = weenie(1, WeenieType::Creature);
    p.properties_did.get_or_insert_with(DotNetDict::new).insert(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    p
}

fn g(v: u32) -> ObjectGuid {
    ObjectGuid::new(v)
}

/// Builds `new <class>(weenie, guid)` through the factory's weenie switch.
fn build(w: &World, weenie: Weenie) -> WorldObject {
    let env = CtorEnv::without_content(w);
    factory::create_world_object(&env, Some(Arc::new(weenie)), g(0x8000_0001)).expect("a class")
}

fn flags(o: &WorldObject) -> ObjectDescriptionFlag {
    o.wo.world_object.object_description_flags
}

fn pairs<K: Copy + std::hash::Hash + Eq, V: Clone>(d: &DotNetDict<K, V>) -> Vec<(K, V)> {
    d.iter().map(|(k, v)| (*k, v.clone())).collect()
}

/// Every property the object exposes, by type, in enumeration order (ephemerals included).
fn all_properties(o: &WorldObject) -> String {
    let floats: Vec<_> = pairs(&o.get_all_property_float())
        .into_iter()
        .map(|(k, v)| (k, v.to_bits()))
        .collect();
    format!(
        "{:?}\n{:?}\n{floats:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        pairs(&o.get_all_property_bools()),
        pairs(&o.get_all_property_data_id()),
        pairs(&o.get_all_property_instance_id()),
        pairs(&o.get_all_property_int()),
        pairs(&o.get_all_property_int64()),
        pairs(&o.get_all_property_string()),
    )
}

/// `CreateWorldObject(Weenie, ObjectGuid)`'s switch, transcribed (by script) from
/// `Source/ACE.Server/Factories/WorldObjectFactory.cs`: `Undef` builds nothing and any type not
/// listed builds a `GenericObject`. `CreateWorldObject(Biota)` has the same cases except
/// `ProjectileSpell`.
const WEENIE_SWITCH: &[(WeenieType, Class)] = &[
    (WeenieType::LifeStone, Class::Lifestone),
    (WeenieType::Door, Class::Door),
    (WeenieType::Portal, Class::Portal),
    (WeenieType::Book, Class::Book),
    (WeenieType::PKModifier, Class::PKModifier),
    (WeenieType::Cow, Class::Cow),
    (WeenieType::Creature, Class::Creature),
    (WeenieType::Container, Class::Container),
    (WeenieType::Scroll, Class::Scroll),
    (WeenieType::Vendor, Class::Vendor),
    (WeenieType::Coin, Class::Coin),
    (WeenieType::Key, Class::Key),
    (WeenieType::Food, Class::Food),
    (WeenieType::Gem, Class::Gem),
    (WeenieType::Game, Class::Game),
    (WeenieType::GamePiece, Class::GamePiece),
    (WeenieType::AllegianceBindstone, Class::Bindstone),
    (WeenieType::Clothing, Class::Clothing),
    (WeenieType::MeleeWeapon, Class::MeleeWeapon),
    (WeenieType::MissileLauncher, Class::MissileLauncher),
    (WeenieType::Ammunition, Class::Ammunition),
    (WeenieType::Missile, Class::Missile),
    (WeenieType::Corpse, Class::Corpse),
    (WeenieType::Chest, Class::Chest),
    (WeenieType::Stackable, Class::Stackable),
    (WeenieType::SpellComponent, Class::SpellComponent),
    (WeenieType::Switch, Class::Switch),
    (WeenieType::AdvocateFane, Class::AdvocateFane),
    (WeenieType::AdvocateItem, Class::AdvocateItem),
    (WeenieType::Healer, Class::Healer),
    (WeenieType::Lockpick, Class::Lockpick),
    (WeenieType::Caster, Class::Caster),
    (WeenieType::ProjectileSpell, Class::SpellProjectile),
    (WeenieType::HotSpot, Class::Hotspot),
    (WeenieType::ManaStone, Class::ManaStone),
    (WeenieType::House, Class::House),
    (WeenieType::SlumLord, Class::SlumLord),
    (WeenieType::Storage, Class::Storage),
    (WeenieType::Hook, Class::Hook),
    (WeenieType::Hooker, Class::Hooker),
    (WeenieType::HousePortal, Class::HousePortal),
    (
        WeenieType::SkillAlterationDevice,
        Class::SkillAlterationDevice,
    ),
    (WeenieType::PressurePlate, Class::PressurePlate),
    (WeenieType::PetDevice, Class::PetDevice),
    (WeenieType::Pet, Class::Pet),
    (WeenieType::CombatPet, Class::CombatPet),
    (WeenieType::Allegiance, Class::Allegiance),
    (WeenieType::AugmentationDevice, Class::AugmentationDevice),
    (
        WeenieType::AttributeTransferDevice,
        Class::AttributeTransferDevice,
    ),
    (WeenieType::CraftTool, Class::CraftTool),
    (WeenieType::LightSource, Class::LightSource),
];

/// The component depth of each class chain: 1 derives from Container, 2 from Creature, 3 from
/// Player (`class X : Y` declarations in `Source/ACE.Server/WorldObjects`).
fn depth(class: Class) -> u8 {
    match class {
        Class::Container
        | Class::Chest
        | Class::Storage
        | Class::Corpse
        | Class::Hook
        | Class::SlumLord => 1,
        Class::Creature
        | Class::Cow
        | Class::GamePiece
        | Class::Pet
        | Class::CombatPet
        | Class::Vendor => 2,
        Class::Player | Class::Sentinel | Class::Admin => 3,
        _ => 0,
    }
}

fn expected(t: WeenieType, from_biota: bool) -> Option<Class> {
    if t == WeenieType::Undef {
        return None;
    }
    if from_biota && t == WeenieType::ProjectileSpell {
        return Some(Class::GenericObject);
    }
    Some(
        WEENIE_SWITCH
            .iter()
            .find(|(k, _)| *k == t)
            .map_or(Class::GenericObject, |(_, c)| *c),
    )
}

// ------------------------------------------------------------------ WorldObjectFactory

#[test]
fn factory_builds_every_weenie_type_as_ace_switches() {
    let w = world();
    let env = CtorEnv::without_content(&w);
    let mut types: Vec<WeenieType> = WeenieType::ALL.to_vec();
    types.push(WeenieType(0xFFFF));
    let mut distinct = std::collections::HashSet::new();

    for t in types {
        let built =
            factory::create_world_object(&env, Some(Arc::new(weenie(100, t))), g(0x8000_0010));
        let want = expected(t, false);
        assert_eq!(
            built.as_ref().map(Class::of),
            want,
            "weenie switch for {t:?}"
        );
        assert_eq!(factory::weenie_class(t), want);

        if let (Some(o), Some(class)) = (&built, want) {
            distinct.insert(class);
            let d = depth(class);
            let components = (
                o.container.is_some(),
                o.creature.is_some(),
                o.player.is_some(),
            );
            assert_eq!(components, (d >= 1, d >= 2, d >= 3), "{t:?}");
            assert_eq!(o.kind.class_name(), class.name());
            assert_eq!(o.guid, g(0x8000_0010));
            assert_eq!(o.biota.id, 0x8000_0010);
            assert_eq!(o.biota.weenie_type, t);
            assert!(o.weenie.is_some());
        }

        let biota = Biota {
            id: 0x7000_0010,
            weenie_class_id: 100,
            weenie_type: t,
            ..Default::default()
        };
        let restored = factory::create_world_object_from_biota(&env, biota);
        let want = expected(t, true);
        assert_eq!(
            restored.as_ref().map(Class::of),
            want,
            "biota switch for {t:?}"
        );
        assert_eq!(factory::biota_class(t), want);
        if let Some(o) = &restored {
            assert_eq!(o.guid, g(0x7000_0010));
            assert!(o.weenie.is_none());
        }
    }

    // Every non-player class the factory can reach, plus GenericObject.
    assert_eq!(distinct.len(), 52);
    assert!(factory::create_world_object(&env, None, g(1)).is_none());
}

#[test]
fn factory_create_new_world_object_by_wcid_and_create_list() {
    let w = world();
    let mut coin = weenie(273, WeenieType::Coin);
    set_int(&mut coin, PropertyInt::MaxStackSize, 25000);
    let coin = Arc::new(coin);
    let lookup = move |wcid: u32| (wcid == 273).then(|| coin.clone());
    let env = CtorEnv {
        w: &w,
        get_cached_weenie: &lookup,
    };

    assert!(factory::create_new_world_object_by_wcid(&env, 999, g(0x8000_0002)).is_none());
    let o = factory::create_new_world_object_by_wcid(&env, 273, g(0x8000_0002)).expect("coin");
    assert_eq!(Class::of(&o), Class::Coin);

    let item = PropertiesCreateList {
        destination_type: DestinationType::Contain,
        weenie_class_id: 273,
        stack_size: 1,
        palette: 3,
        shade: 0.25,
        ..Default::default()
    };
    let o = factory::create_new_world_object_from_create_list(&env, &item, g(0x8000_0003))
        .expect("coin");
    assert_eq!(o.wo.world_object.destination_type, DestinationType::Contain);
    assert_eq!(o.palette_template(), Some(3));
    assert_eq!(o.shade(), Some(0.25));

    // Treasure: the shade is a probability and is not copied.
    let treasure = PropertiesCreateList {
        destination_type: DestinationType::ContainTreasure,
        palette: 0,
        ..item
    };
    let o = factory::create_new_world_object_from_create_list(&env, &treasure, g(0x8000_0004))
        .expect("coin");
    assert_eq!(o.shade(), None);
    assert_eq!(o.palette_template(), None);
}

// ------------------------------------------------------------------ constructor chains

#[test]
fn base_constructor_sets_world_object_defaults() {
    let w = world();
    let o = build(&w, weenie(1, WeenieType::Generic));
    assert_eq!(Class::of(&o), Class::GenericObject);
    assert_eq!(flags(&o), ObjectDescriptionFlag::Attackable);
    assert_eq!(o.placement(), Some(Placement::Resting));
    // `(int)Time.GetUnixTime()`
    assert_eq!(o.creation_timestamp(), Some(1_790_000_000));
    assert!(o.biota.properties_enchantment_registry.is_some());
    assert_eq!(o.wo.world_object.listening_radius, 5.0);

    // A weenie's own Placement is kept; the (Biota) constructor sets no CreationTimestamp.
    let mut placed = weenie(1, WeenieType::Generic);
    set_int(
        &mut placed,
        PropertyInt::Placement,
        Placement::RightHandCombat.0 as i32,
    );
    assert_eq!(
        build(&w, placed).placement(),
        Some(Placement::RightHandCombat)
    );

    let env = CtorEnv::without_content(&w);
    let biota = Biota {
        id: 0x7000_0001,
        weenie_class_id: 1,
        weenie_type: WeenieType::Generic,
        ..Default::default()
    };
    let o = factory::create_world_object_from_biota(&env, biota).expect("generic");
    assert_eq!(o.creation_timestamp(), None);
    assert_eq!(o.placement(), Some(Placement::Resting));
}

#[test]
fn chest_capacity_default_is_containers_zero_not_chests_ten() {
    // Container.SetEphemeralValues runs first and sets a missing ContainerCapacity to 0, so
    // Chest's `ContainerCapacity ?? 10` keeps 0; ItemCapacity is only defaulted by Chest.
    let w = world();
    let o = build(&w, weenie(2, WeenieType::Chest));
    assert_eq!(Class::of(&o), Class::Chest);
    assert_eq!(o.container_capacity(), Some(0));
    assert_eq!(o.item_capacity(), Some(120));
    assert_eq!(o.use_radius(), Some(0.5));
    assert!(o.activation_response().0 & ActivationResponse::Use.0 != 0);
    assert!(!o.is_open());
    // Container's burden and value are ephemeral: in the enumeration, not in the biota.
    let ints = pairs(&o.get_all_property_int());
    assert!(
        ints.contains(&(PropertyInt::EncumbranceVal, 0)) && ints.contains(&(PropertyInt::Value, 0)),
        "{ints:?}"
    );
    assert!(o
        .biota
        .properties_int
        .as_ref()
        .is_none_or(|d| !d.contains_key(&PropertyInt::EncumbranceVal)));
}

#[test]
fn storage_runs_container_then_chest_then_storage() {
    let w = world();
    let mut s = weenie(3, WeenieType::Storage);
    set_bool(&mut s, PropertyBool::Locked, true);
    let o = build(&w, s);
    assert_eq!(Class::of(&o), Class::Storage);
    // Chest: IsLocked implies DefaultLocked; then Storage unlocks.
    assert!(o.default_locked());
    assert!(!o.is_locked());
    assert!(!o.is_open());
    assert!(o.wo.world_object.bump_velocity);
}

#[test]
fn corpse_overrides_container_capacity_and_bumps_only_from_biota() {
    let w = world();
    let o = build(&w, weenie(4, WeenieType::Corpse));
    assert_eq!(o.container_capacity(), Some(10));
    assert_eq!(o.item_capacity(), Some(120));
    assert_eq!(
        flags(&o),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::Corpse
    );
    assert_eq!(o.suppress_generate_effect(), Some(true));
    assert!(!o.wo.world_object.bump_velocity);
    // Corpses carry no ephemeral Value.
    assert!(!pairs(&o.get_all_property_int())
        .iter()
        .any(|(k, _)| *k == PropertyInt::Value));

    let env = CtorEnv::without_content(&w);
    let o = factory::create_world_object_from_biota(&env, o.biota.clone()).expect("corpse");
    assert!(o.wo.world_object.bump_velocity);
}

#[test]
fn combat_pet_runs_creature_then_pet_then_combat_pet() {
    let w = world();
    let o = build(&w, weenie(5, WeenieType::CombatPet));
    assert_eq!(Class::of(&o), Class::CombatPet);
    assert!(o.is_pet() && o.is_creature() && o.is_container());
    // Creature.InitializePropertyDictionaries
    assert!(o.biota.properties_attribute.is_some() && o.biota.properties_skill.is_some());
    assert!(o.biota.properties_body_part.is_some() && o.biota.properties_attribute_2nd.is_some());
    // Container: burden but no value for creatures; then Pet's defaults.
    let ints = pairs(&o.get_all_property_int());
    assert!(ints.contains(&(PropertyInt::EncumbranceVal, 0)));
    assert!(!ints.iter().any(|(k, _)| *k == PropertyInt::Value));
    assert_eq!(o.container_capacity(), Some(0));
    assert_eq!(o.ethereal(), Some(true));
    assert_eq!(o.radar_behavior(), Some(RadarBehavior::ShowNever));
    assert_eq!(o.item_useable(), Some(Usable::No));
    assert_eq!(o.suppress_generate_effect(), Some(true));
}

#[test]
fn door_defaults_follow_ace() {
    let w = world();
    let o = build(&w, weenie(6, WeenieType::Door));
    assert_eq!(
        flags(&o),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::Door
    );
    assert!(!o.is_open());
    assert_eq!(o.ethereal(), None);
    assert_eq!(o.reset_interval(), Some(30.0));
    assert_eq!(o.lock_code(), Some(String::new()));
    assert!(!o.is_locked());
    assert!(o.activation_response().0 & ActivationResponse::Use.0 != 0);

    let mut d = weenie(6, WeenieType::Door);
    set_bool(&mut d, PropertyBool::DefaultOpen, true);
    set_bool(&mut d, PropertyBool::Locked, true);
    let o = build(&w, d);
    assert!(o.is_open());
    assert_eq!(o.ethereal(), Some(true));
    // Locked without DefaultLocked: DefaultLocked is repaired, then IsLocked follows it.
    assert!(o.default_locked() && o.is_locked());
}

#[test]
fn stackable_derives_unit_values() {
    let w = world();
    let mut s = weenie(7, WeenieType::Stackable);
    set_int(&mut s, PropertyInt::EncumbranceVal, 50);
    set_int(&mut s, PropertyInt::StackSize, 5);
    set_int(&mut s, PropertyInt::MaxStackSize, 10);
    set_int(&mut s, PropertyInt::Value, 101);
    let o = build(&w, s);
    assert_eq!(o.stack_unit_encumbrance(), Some(10));
    // int division: 101 / 5 == 20, then Value = 20 * 5
    assert_eq!(o.stack_unit_value(), Some(20));
    assert_eq!(o.value(), Some(100));
    assert_eq!(o.encumbrance_val(), Some(50));

    // Nothing set: every default is 1 or 0, and MaxStackSize 1 skips the rewrite.
    let o = build(&w, weenie(8, WeenieType::Food));
    assert_eq!(
        (
            o.stack_size(),
            o.max_stack_size(),
            o.value(),
            o.encumbrance_val()
        ),
        (Some(1), Some(1), Some(0), Some(0))
    );
    assert_eq!(
        (o.stack_unit_encumbrance(), o.stack_unit_value()),
        (Some(0), Some(0))
    );
    assert_eq!(
        flags(&o),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::Food
    );
}

#[test]
fn leaf_defaults_follow_ace() {
    let w = world();
    let o = build(&w, weenie(9, WeenieType::AllegianceBindstone));
    assert_eq!(
        flags(&o),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::BindStone
    );
    assert_eq!(
        o.get_property(PropertyInt::ShowableOnRadar),
        Some(i32::from(RadarBehavior::ShowAlways.0))
    );

    assert_eq!(
        build(&w, weenie(10, WeenieType::Game)).use_radius(),
        Some(6.5)
    );
    let o = build(&w, weenie(11, WeenieType::GamePiece));
    assert_eq!((o.time_to_rot(), o.use_radius()), (Some(-1.0), Some(0.5)));
    assert_eq!(
        build(&w, weenie(12, WeenieType::HotSpot)).cycle_time(),
        Some(1.0)
    );
    assert_eq!(
        build(&w, weenie(13, WeenieType::SlumLord)).item_capacity(),
        Some(120)
    );
    let o = build(&w, weenie(14, WeenieType::Portal));
    assert_eq!(
        flags(&o),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::Portal
    );
    let o = build(&w, weenie(15, WeenieType::HousePortal));
    assert_eq!(Class::of(&o), Class::HousePortal);
    assert_eq!(
        flags(&o),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::Portal
    );
    let o = build(&w, weenie(16, WeenieType::Scroll));
    assert_eq!(
        o.use_().as_deref(),
        Some("Use this item to attempt to learn its spell.")
    );
    let o = build(&w, weenie(17, WeenieType::Book));
    assert_eq!(o.get_property(PropertyInt::AppraisalPages), Some(0));
    assert_eq!(o.get_property(PropertyInt::AppraisalMaxPages), Some(0));
    let o = build(&w, weenie(4142, WeenieType::Generic));
    assert_eq!(
        (o.max_generated_objects(), o.init_generated_objects()),
        (0, 0)
    );

    let mut pk = weenie(18, WeenieType::PKModifier);
    set_int(&mut pk, PropertyInt::PkLevelModifier, -1);
    assert_eq!(
        flags(&build(&w, pk)),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::NpkSwitch
    );

    let mut key = weenie(19, WeenieType::Key);
    set_int(&mut key, PropertyInt::Structure, 9);
    set_int(&mut key, PropertyInt::MaxStructure, 4);
    assert_eq!(build(&w, key).structure(), Some(4));
}

/// The weenie player constructor makes its character account and combat table.
#[test]
fn the_weenie_player_constructor_makes_its_character_account_and_combat_table() {
    use std::net::{IpAddr, Ipv4Addr};
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: UNIX_TIME,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let cmt = dereth_assets::CombatManeuverTable {
        id: dereth_primitives::DataId(0x3000_0000),
        maneuvers: Vec::new(),
    };
    let w = World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .with_portal(0x3000_0000, cmt)
            .build()
            .expect("fake dats"),
    );
    let account_id = w
        .auth
        .lock()
        .create_account(
            "acct",
            "pw",
            empyrean_entity::enums::AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let env = CtorEnv::without_content(&w);
    let mut p = player_weenie();
    set_string(&mut p, PropertyString::Name, "Tester");
    let o = player_from_weenie(&env, Class::Player, Arc::new(p), g(0x5000_0001), account_id);
    let pl = o.player.as_ref().expect("a player");
    let character = pl.player.character.as_ref().expect("the new Character");
    assert_eq!(
        (character.id, character.account_id, character.name.as_str()),
        (0x5000_0001, account_id, "Tester")
    );
    assert!(pl.player_database.character_changes_detected);
    assert_eq!(
        pl.player.account.as_ref().map(|a| a.account_name.as_str()),
        Some("acct")
    );
    assert!(pl.player_death.loot_permission.is_empty());
    let table = empyrean_world::world_objects::creature_combat::fields(&o)
        .combat_table
        .as_ref()
        .expect("the combat table");
    assert_eq!(table.id.0, 0x3000_0000);
}

#[test]
fn player_constructors_follow_ace() {
    let w = world();
    let env = CtorEnv::without_content(&w);
    let mut p = player_weenie();
    set_int(&mut p, PropertyInt::AugmentationExtraPackSlot, 1);
    set_string(&mut p, PropertyString::Name, "Tester");
    let o = player_from_weenie(&env, Class::Player, Arc::new(p.clone()), g(0x5000_0001), 7);
    assert_eq!(Class::of(&o), Class::Player);
    assert!(o.player.is_some() && o.creature.is_some() && o.container.is_some());
    assert_eq!(
        flags(&o),
        ObjectDescriptionFlag::Attackable | ObjectDescriptionFlag::Player
    );
    assert_eq!(o.container_capacity(), Some(8));
    assert_eq!(
        (o.available_experience(), o.total_experience()),
        (Some(0), Some(0))
    );
    assert!(o.attackable());
    assert_eq!(
        o.get_property(PropertyString::DateOfBirth).as_deref(),
        Some("01 January 2026")
    );
    assert_eq!(
        (o.player_kills_pk(), o.player_kills_pkl()),
        (Some(0), Some(0))
    );
    assert!(!o.first_enter_world_done());

    let o = player_from_weenie(&env, Class::Admin, Arc::new(p), g(0x5000_0002), 7);
    assert_eq!(Class::of(&o), Class::Admin);
    let admin_flags = ObjectDescriptionFlag::Attackable
        | ObjectDescriptionFlag::Player
        | ObjectDescriptionFlag::Admin;
    assert_eq!(flags(&o), admin_flags);
    let sentinel = Channel::Audit
        | Channel::Advocate1
        | Channel::Advocate2
        | Channel::Advocate3
        | Channel::Sentinel
        | Channel::AllBroadcast;
    assert_eq!(
        o.channels_allowed(),
        Some(sentinel | Channel::QA1 | Channel::QA2 | Channel::ValidChans)
    );

    // Restoring that biota as a Sentinel ORs Sentinel's channels into the saved ones.
    let character = empyrean_store::models::shard::Character {
        id: 0x5000_0002,
        account_id: 7,
        ..Default::default()
    };
    let o = player_from_biota_with_character(
        &env,
        Class::Sentinel,
        o.biota.clone(),
        Vec::new(),
        Vec::new(),
        character,
        None,
    );
    assert_eq!(Class::of(&o), Class::Sentinel);
    assert_eq!(o.guid, g(0x5000_0002));

    let mut fresh = player_weenie();
    set_int(&mut fresh, PropertyInt::ChannelsAllowed, Channel::Fellow.0);
    let o = player_from_weenie(&env, Class::Sentinel, Arc::new(fresh), g(0x5000_0003), 7);
    assert_eq!(o.channels_allowed(), Some(sentinel | Channel::Fellow));
}

#[test]
#[should_panic(expected = "has no (Weenie, ObjectGuid) or (Biota) constructor")]
fn player_class_needs_the_player_constructor() {
    let w = world();
    let env = CtorEnv::without_content(&w);
    let _ = WorldObject::from_weenie(
        &env,
        Class::Player,
        Arc::new(weenie(1, WeenieType::Creature)),
        g(0x5000_0001),
    );
}

#[test]
fn container_from_biota_repairs_saved_data() {
    let w = world();
    let mut chest_weenie = weenie(20, WeenieType::Chest);
    set_int(&mut chest_weenie, PropertyInt::EncumbranceVal, 75);
    let chest_weenie = Arc::new(chest_weenie);
    let lookup = move |wcid: u32| (wcid == 20).then(|| chest_weenie.clone());
    let env = CtorEnv {
        w: &w,
        get_cached_weenie: &lookup,
    };

    let mut biota = Biota {
        id: 0x7000_0020,
        weenie_class_id: 20,
        weenie_type: WeenieType::Chest,
        ..Default::default()
    };
    let mut bools = DotNetDict::new();
    bools.insert(PropertyBool::Open, true);
    biota.properties_bool = Some(bools);
    let mut ints = DotNetDict::new();
    ints.insert(PropertyInt::EncumbranceVal, 999);
    ints.insert(PropertyInt::Value, 5);
    biota.properties_int = Some(ints);
    let o = factory::create_world_object_from_biota(&env, biota).expect("chest");

    assert!(o.wo.world_object_database.changes_detected);
    assert!(o
        .biota
        .properties_bool
        .as_ref()
        .is_some_and(|b| !b.contains_key(&PropertyBool::Open)));
    let saved = o.biota.properties_int.as_ref().expect("ints");
    // EncumbranceVal reset to the weenie's; Value removed (the weenie has none).
    assert_eq!(saved.get(&PropertyInt::EncumbranceVal), Some(&75));
    assert!(!saved.contains_key(&PropertyInt::Value));
    assert_eq!(o.encumbrance_val(), Some(75));
    assert_eq!(o.value(), Some(0));
}

#[test]
fn weenie_to_biota_to_from_biota_round_trips_properties() {
    let w = world();
    let env = CtorEnv::without_content(&w);
    let types = [
        WeenieType::Door,
        WeenieType::Chest,
        WeenieType::Stackable,
        WeenieType::Creature,
        WeenieType::Portal,
        WeenieType::Book,
        WeenieType::Pet,
        WeenieType::Generic,
    ];
    for t in types {
        let mut x = weenie(30, t);
        set_int(&mut x, PropertyInt::ItemType, ItemType::Misc.0 as i32);
        set_string(&mut x, PropertyString::Name, "thing");
        x.properties_float
            .get_or_insert_with(DotNetDict::new)
            .insert(PropertyFloat::DefaultScale, 1.5);
        let a = factory::create_world_object(&env, Some(Arc::new(x)), g(0x8000_0030)).expect("a");
        let b = factory::create_world_object_from_biota(&env, a.biota.clone()).expect("b");
        assert_eq!(Class::of(&a), Class::of(&b), "{t:?}");
        assert_eq!(all_properties(&a), all_properties(&b), "{t:?}");
        assert_eq!(flags(&a), flags(&b));
    }
}

// ------------------------------------------------------------------ WorldObject.cs members

#[test]
fn weapon_and_item_predicates() {
    let w = world();
    let mut bow = weenie(40, WeenieType::MissileLauncher);
    set_int(
        &mut bow,
        PropertyInt::DefaultCombatStyle,
        CombatStyle::Crossbow.0,
    );
    let o = build(&w, bow);
    assert!(
        o.is_bow()
            && o.is_ammo_launcher()
            && o.is_ranged()
            && !o.is_atlatl()
            && !o.is_thrown_weapon()
    );
    assert!(!o.is_caster_weapon() && !o.is_shield());

    let mut shield = weenie(41, WeenieType::Generic);
    set_int(
        &mut shield,
        PropertyInt::CombatUse,
        i32::from(CombatUse::Shield.0),
    );
    set_int(&mut shield, PropertyInt::ArmorLevel, 1);
    set_int(
        &mut shield,
        PropertyInt::WieldSkillType,
        i32::from(PropertyInt::SocietyRankEldweb.0),
    );
    set_int(
        &mut shield,
        PropertyInt::ItemType,
        ItemType::PromissoryNote.0 as i32,
    );
    let o = build(&w, shield);
    assert!(o.is_shield() && o.has_armor_level() && o.is_society_armor() && o.is_trade_note());
    assert!(!o.is_bow() && !o.is_two_handed());

    let mut two = weenie(42, WeenieType::MeleeWeapon);
    set_int(&mut two, PropertyInt::WeaponSkill, Skill::TwoHandedCombat.0);
    assert!(build(&w, two).is_two_handed());

    assert_eq!(
        build(&w, weenie(43, WeenieType::Container)).container_type(),
        ContainerType::Container
    );
    assert_eq!(
        build(&w, weenie(43, WeenieType::Chest)).container_type(),
        ContainerType::NonContainer
    );
    let mut foci = weenie(44, WeenieType::Generic);
    set_bool(&mut foci, PropertyBool::RequiresBackpackSlot, true);
    assert_eq!(build(&w, foci).container_type(), ContainerType::Foci);
}

#[test]
fn virtual_bodies_attuned_unique_trade_name() {
    let mut w = world();
    let mut x = weenie(50, WeenieType::Generic);
    set_int(&mut x, PropertyInt::Attuned, AttunedStatus::Attuned.0);
    set_int(&mut x, PropertyInt::Unique, 1);
    set_string(&mut x, PropertyString::Name, "Pyreal Mote");
    let o = build(&w, x);
    let this = o.guid;
    w.objects.insert(o).expect("fresh");

    assert!(dispatch::is_attuned_or_contains_attuned::is_attuned_or_contains_attuned(&w, this));
    assert!(!dispatch::is_sticky_attuned_or_contains_sticky_attuned::is_sticky_attuned_or_contains_sticky_attuned(
        &w, this
    ));
    assert!(dispatch::is_unique_or_contains_unique::is_unique_or_contains_unique(&w, this));
    assert_eq!(
        dispatch::get_unique_objects::get_unique_objects(&w, this),
        vec![this]
    );
    let mut set = DotNetHashSet::new();
    let traded = dispatch::is_being_traded_or_contains_item_being_traded::is_being_traded_or_contains_item_being_traded;
    assert!(!traded(&w, this, &set));
    set.insert(this);
    assert!(traded(&w, this, &set));

    assert_eq!(
        dispatch::name::name(&w, this).as_deref(),
        Some("Pyreal Mote")
    );
    assert_eq!(wo::get_plural_name(&w, this), "Pyreal Motes");
    dispatch::name::set_name(&mut w, this, "Coin".to_owned());
    assert_eq!(dispatch::name::name(&w, this).as_deref(), Some("Coin"));

    let props = w.objects.get(this).expect("o").get_properties();
    let keys: Vec<u16> = props.keys().map(|k| k.0).collect();
    assert!(
        keys.windows(2).all(|p| p[0] < p[1]),
        "ascending Enum.GetValues order"
    );
    assert_eq!(props.len(), PropertyInt::ALL.len());
    assert_eq!(props.get(&PropertyInt::Unique), Some(&Some(1)));
    assert_eq!(props.get(&PropertyInt::Level), Some(&None));
}

#[test]
fn destroy_marks_once_and_stamps_release_time() {
    use empyrean_testkit::EmptyShard;
    let mut w = world();
    empyrean_world::managers::guid_manager::initialize(&mut w, &mut EmptyShard);
    let o = build(&w, weenie(60, WeenieType::Generic));
    let this = o.guid;
    w.objects.insert(o).expect("fresh");
    w.objects
        .get_mut(this)
        .expect("o")
        .wo
        .world_object
        .is_destroyed = true;
    wo::destroy(&mut w, this, true, false);
    assert!(w.objects.get(this).is_none(), "unreferenced: dropped");
    let recycled =
        |w: &mut World| empyrean_world::managers::guid_manager::get_dynamic_guid_debug_info(w);
    assert!(
        recycled(&mut w).ends_with("recycled GUIDs available: 0"),
        "{}",
        recycled(&mut w)
    );

    let o = build(&w, weenie(60, WeenieType::Generic));
    let this = o.guid;
    w.objects.insert(o).expect("fresh");
    wo::destroy(&mut w, this, true, false);
    assert!(
        w.objects.get(this).is_none(),
        "a destroyed object leaves the store"
    );
    assert!(
        recycled(&mut w).ends_with("recycled GUIDs available: 1"),
        "{}",
        recycled(&mut w)
    );

    // A second Destroy finds nothing and recycles nothing.
    wo::destroy(&mut w, this, true, false);
    assert!(recycled(&mut w).ends_with("recycled GUIDs available: 1"));
}

#[test]
fn relative_dir_quadrants() {
    let mut w = world();
    let place = |w: &mut World, guid: u32, x: f32, y: f32| {
        let mut o = WorldObject::allocate(Class::GenericObject);
        o.guid = g(guid);
        let mut p = Position::new();
        p.set_landblock_id(LandblockId::new(0x7D64_0001));
        p.position_x = x;
        p.position_y = y;
        o.set_location(Some(p));
        w.objects.insert(o).expect("fresh");
    };
    // The target, at (10, 10), faces north: the identity rotation's heading is +Y.
    place(&mut w, 1, 10.0, 10.0);
    place(&mut w, 2, 10.0, 0.0); // behind
    place(&mut w, 3, 15.0, 10.0); // east
    place(&mut w, 4, 5.0, 10.0); // west
    place(&mut w, 5, 10.0, 20.0); // ahead
    assert_eq!(
        wo::get_relative_dir(&w, g(2), g(1)),
        Quadrant::Left | Quadrant::Back
    );
    assert_eq!(
        wo::get_relative_dir(&w, g(3), g(1)),
        Quadrant::Right | Quadrant::Front
    );
    assert_eq!(
        wo::get_relative_dir(&w, g(4), g(1)),
        Quadrant::Left | Quadrant::Front
    );
    assert_eq!(
        wo::get_relative_dir(&w, g(5), g(1)),
        Quadrant::Left | Quadrant::Front
    );
}

#[test]
fn structure_unit_value_and_moa_skill() {
    let mut w = world();
    let mut x = weenie(70, WeenieType::Generic);
    set_int(&mut x, PropertyInt::Value, 1000);
    set_int(&mut x, PropertyInt::MaxStructure, 30);
    let x = Arc::new(x);
    let o = {
        let xc = x.clone();
        let lookup = move |wcid: u32| (wcid == 70).then(|| xc.clone());
        let env = CtorEnv {
            w: &w,
            get_cached_weenie: &lookup,
        };
        let o = factory::create_world_object(&env, Some(x.clone()), g(0x8000_0070)).expect("o");
        // int division: 1000 / 30
        assert_eq!(wo::structure_unit_value(&env, &o), 33);
        o
    };
    // No cached weenie: 0 / 1.
    assert_eq!(
        wo::structure_unit_value(&CtorEnv::without_content(&w), &o),
        0
    );

    let this = o.guid;
    w.objects.insert(o).expect("fresh");
    assert_eq!(
        wo::convert_to_mo_a_skill(&mut w, this, Skill::Bow),
        Skill::Bow
    );
    let p = {
        let env = CtorEnv::without_content(&w);
        player_from_weenie(
            &env,
            Class::Player,
            Arc::new(player_weenie()),
            g(0x5000_0009),
            1,
        )
    };
    w.objects.insert(p).expect("fresh");
    assert_eq!(
        wo::convert_to_mo_a_skill(&mut w, g(0x5000_0009), Skill::Bow),
        Skill::MissileWeapons
    );
    assert_eq!(
        wo::convert_to_mo_a_skill(&mut w, g(0x5000_0009), Skill::Alchemy),
        Skill::Alchemy
    );
}

/// Coordinator wiring: `CtorEnv::with_world` resolves weenies through `World.content`.
#[test]
fn ctor_env_with_world_reads_world_content() {
    use empyrean_content::models::world::Weenie as WeenieRow;
    use empyrean_entity::enums::WeenieType;
    use empyrean_world::world_objects::world_object::CtorEnv;
    let mut w = world();
    w.content = std::sync::Arc::new(empyrean_content::MemContent::new().weenie(WeenieRow::new(
        12345,
        "testthing",
        WeenieType::Generic,
    )));
    let found = CtorEnv::with_world(&w, |env| {
        (env.get_cached_weenie)(12345).map(|x| x.weenie_class_id)
    });
    assert_eq!(found, Some(12345));
    assert!(CtorEnv::with_world(&w, |env| (env.get_cached_weenie)(99).is_none()));
}

/// A Sentinel (or Admin) marks its character plussed, a change to save, whichever constructor
/// built it; a character already plussed is left unchanged. The login constructor always has the
/// character, and attaches its account.
#[test]
fn a_sentinel_marks_its_character_plussed() {
    use std::net::{IpAddr, Ipv4Addr};
    let w = world();
    let account_id = w
        .auth
        .lock()
        .create_account(
            "admin",
            "pw",
            empyrean_entity::enums::AccessLevel::Admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let env = CtorEnv::without_content(&w);
    empyrean_common::not_ported::take_local();

    let o = player_from_weenie(
        &env,
        Class::Admin,
        Arc::new(player_weenie()),
        g(0x5000_0004),
        account_id,
    );
    let pl = o.player.as_ref().expect("a player");
    assert!(
        pl.player
            .character
            .as_ref()
            .expect("its character")
            .is_plussed
    );
    assert!(pl.player_database.character_changes_detected);

    let character = empyrean_store::models::shard::Character {
        id: 0x5000_0004,
        account_id,
        ..Default::default()
    };
    let o = player_from_biota_with_character(
        &env,
        Class::Sentinel,
        o.biota.clone(),
        Vec::new(),
        Vec::new(),
        character,
        None,
    );
    let pl = o.player.as_ref().expect("a player");
    assert!(
        pl.player
            .character
            .as_ref()
            .expect("its character")
            .is_plussed
    );
    assert!(pl.player_database.character_changes_detected);
    assert_eq!(
        pl.player.account.as_ref().map(|a| a.account_name.as_str()),
        Some("admin")
    );

    let plussed = empyrean_store::models::shard::Character {
        id: 0x5000_0004,
        account_id,
        is_plussed: true,
        ..Default::default()
    };
    let o = player_from_biota_with_character(
        &env,
        Class::Admin,
        o.biota.clone(),
        Vec::new(),
        Vec::new(),
        plussed,
        None,
    );
    assert!(
        !o.player
            .as_ref()
            .expect("a player")
            .player_database
            .character_changes_detected
    );

    assert!(
        empyrean_common::not_ported::take_local().is_empty(),
        "no unported member on the way"
    );
}

/// The 275 test templates' `AddRend`: a slashing weapon gets the slash-rending imbue, the rending
/// icon underlay (`RecipeManager.IconUnderlay`) and one more tinker.
#[test]
fn add_rend_gives_the_rending_imbue_and_its_icon_underlay() {
    let w = world();
    let env = CtorEnv::without_content(&w);
    let mut sword = weenie(30, WeenieType::MeleeWeapon);
    set_int(&mut sword, PropertyInt::DamageType, 1); // Slash
    set_int(&mut sword, PropertyInt::NumTimesTinkered, 2);
    let mut o = WorldObject::from_weenie(&env, Class::MeleeWeapon, Arc::new(sword), g(0x8000_0030));
    empyrean_common::not_ported::take_local();
    empyrean_world::factories::player_factory_ex::add_rend(&mut o);
    assert!(empyrean_common::not_ported::take_local().is_empty());
    assert_eq!(
        o.get_property(PropertyInt::ImbuedEffect),
        Some(0x8),
        "SlashRending"
    );
    assert_eq!(o.icon_underlay_id(), Some(0x0600_335c));
    assert_eq!(o.get_property(PropertyInt::NumTimesTinkered), Some(3));
}

mod creature_equipment {
    use crate::support::creature_world::*;

    /// `Creature.SetEphemeralValues` for a generator's creature: GenerateWieldList creates the sword
    /// into the inventory, EquipInventoryItems (SelectWieldedTreasure) takes it out and wields it in
    /// its ValidLocations; the creature then stands on the landblock holding it.
    #[test]
    fn a_generator_spawned_creature_wields_its_wield_list() {
        let mut h = H::new();
        let g = h.place_new(GEN_WCID, GEN);
        h.regenerate(g);

        let spawned = h.spawned(g);
        assert_eq!(spawned.len(), 1);
        let drudge = spawned[0];
        assert_eq!(
            h.o(drudge).current_landblock.map(|l| l.landblock()),
            Some(LB),
            "placed"
        );

        let equipped = creature_equipment::equipped_objects_values(&h.w, drudge);
        assert_eq!(equipped.len(), 1, "one wielded item");
        let sword = h.o(equipped[0]);
        assert_eq!(sword.biota.weenie_class_id, SWORD_WCID);
        assert_eq!(sword.wielder_id(), Some(drudge.full()));
        assert_eq!(
            sword.current_wielded_location(),
            Some(EquipMask::MeleeWeapon)
        );
        assert_eq!(sword.container_id(), None, "out of the inventory");
        assert!(
            empyrean_world::world_objects::container::inventory_values(&h.w, drudge).is_empty(),
            "nothing left in the pack"
        );
    }

    /// The step runs once per object: a second call (a re-insertion) creates nothing.
    #[test]
    fn post_insert_runs_once_per_creature() {
        let mut h = H::new();
        let d = h.place_new(DRUDGE_WCID, 0x7A9B_4300);
        let before = h.w.objects.len();
        creature::post_insert(&mut h.w, d);
        assert_eq!(h.w.objects.len(), before, "no second sword");
        assert_eq!(
            creature_equipment::equipped_objects_values(&h.w, d).len(),
            1
        );
    }
}

#[cfg(feature = "real-content")]
mod retail_equipment {
    use crate::support::creature_world::real_content::*;

    /// A generator drudge wields stands and chases a player to attack.
    #[test]
    fn a_generator_drudge_wields_stands_and_chases_a_player_to_attack() {
        // The generator picks its creature at random (a drudge, a mite, a banderling...), and the
        // random stream also places scattered spawns, so the first seed from SEED on whose
        // encounter spawns a drudge is used.
        let mut found = None;
        for seed in SEED..SEED + 50 {
            let mut h = world();
            ThreadSafeRandom::seed(seed);
            lm::get_landblock(
                &mut h.w,
                LandblockId::new(u32::from(HOME) << 16 | 0xFFFF),
                false,
                false,
            );
            // the encounter at cell (2, 1): its generator stands at (48, 24); the player 8 m north
            let p = player(&mut h, 48.0, 32.0);

            let mut drudge = None;
            for _ in 0..600 {
                h.advance(0.05);
                h.tick();
                drudge = drudges(&h)
                    .into_iter()
                    .filter(|&d| distance(&h, d, p) < 18.0)
                    .min_by(|&a, &b| distance(&h, a, p).total_cmp(&distance(&h, b, p)));
                if drudge.is_some() {
                    break;
                }
            }
            if let Some(d) = drudge {
                found = Some((h, p, d));
                break;
            }
        }
        let (mut h, p, d) = found.expect("a drudge spawned near the player");

        // its wielded treasure, equipped
        let equipped = creature_equipment::equipped_objects_values(&h.w, d);
        assert!(!equipped.is_empty(), "the drudge wields something");
        for e in &equipped {
            assert_eq!(h.o(*e).wielder_id(), Some(d.full()));
            assert!(h.o(*e).current_wielded_location().is_some());
        }

        // on the ground
        let body = phys_ext::physics_obj(&h.w, d).expect("a body");
        let mut contact = false;
        for _ in 0..40 {
            h.advance(0.05);
            h.tick();
            if h.w
                .physics
                .get(body)
                .is_some_and(|o| o.transient_state.in_contact())
            {
                contact = true;
                break;
            }
        }
        assert!(contact, "the drudge stands on the ground");

        assert!(
            !monster_awareness::is_awake(&h.w, d),
            "a Retaliate drudge ignores the player"
        );
        monster_combat::set_attack_target(&mut h.w, d, Some(p));
        monster_awareness::wake_up(&mut h.w, d, true);
        let start = distance(&h, d, p);
        assert!(start > 3.0, "beyond melee range: {start}");
        let mut closed = false;
        for _ in 0..600 {
            h.advance(0.05);
            h.tick();
            if distance(&h, d, p) < 2.0 {
                closed = true;
                break;
            }
        }
        let body_at = h.w.physics.get(body).map(|o| o.position.frame.origin);
        assert!(
            closed,
            "the drudge ran to the player: {} m (body {body_at:?}, target {:?})",
            distance(&h, d, p),
            monster_combat::attack_target(&h.w, d)
        );
        assert_eq!(monster_combat::attack_target(&h.w, d), Some(p));

        // and swings: an attack sets its next attack time
        let before = monster_combat::fields(&h.w, d).next_attack_time;
        let mut swung = false;
        for _ in 0..200 {
            h.advance(0.05);
            h.tick();
            if monster_combat::fields(&h.w, d).next_attack_time > before {
                swung = true;
                break;
            }
        }
        assert!(swung, "the drudge attacks");
    }

    /// An academy npc is dressed and his create object shows it.
    #[test]
    fn an_academy_npc_is_dressed_and_his_create_object_shows_it() {
        use dereth_protocol::objects::ItemCreateObject;
        use empyrean_world::network::game_messages::messages::game_message_create_object::game_message_create_object;

        let mut h = world();
        lm::get_landblock(
            &mut h.w,
            LandblockId::new(u32::from(ACADEMY) << 16 | 0xFFFF),
            false,
            false,
        );
        h.advance(0.05);
        h.tick();
        let lb = LandblockId::new(u32::from(ACADEMY) << 16 | 0xFFFF);
        let guard =
            h.w.landblock_manager
                .landblocks
                .get(lb)
                .expect("loaded")
                .world_object_guids()
                .copied()
                .find(|&g| h.o(g).biota.weenie_class_id == SENIOR_GUARD)
                .expect("the Senior Guard");

        let equipped = creature_equipment::equipped_objects_values(&h.w, guard);
        let wcids: Vec<u32> = equipped
            .iter()
            .map(|&e| h.o(e).biota.weenie_class_id)
            .collect();
        assert!(
            wcids.contains(&SINGULARITY_SWORD),
            "the sword is wielded: {wcids:?}"
        );
        assert!(wcids.len() >= 5, "armour and clothing worn too: {wcids:?}");
        for e in &equipped {
            assert_eq!(h.o(*e).wielder_id(), Some(guard.full()));
        }

        let m = game_message_create_object(&mut h.w, guard, false, false);
        let c = dereth_protocol::read_body_padded::<ItemCreateObject>(&m.data[4..])
            .expect("the CreateObject decodes")
            .0;
        assert_eq!(c.id.0, guard.full());
        let od = &c.objdesc;
        assert!(!od.subpalettes.is_empty(), "clothing palettes: {od:?}");
        assert!(!od.texture_changes.is_empty(), "clothing textures: {od:?}");
        assert!(
            !od.anim_part_changes.is_empty(),
            "armour model parts: {od:?}"
        );
        let sword = equipped
            .iter()
            .copied()
            .find(|&e| h.o(e).biota.weenie_class_id == SINGULARITY_SWORD)
            .unwrap();
        let children = c.physicsdesc.children.clone().unwrap_or_default();
        assert!(
            children.iter().any(|ch| ch.child_id.0 == sword.full()),
            "the sword in the child list: {children:?}"
        );

        // the ObjDesc differs from the bare body's: take the gear off and compare
        let bare = {
            for e in &equipped {
                assert!(creature_equipment::try_dequip_object(&mut h.w, guard, *e).is_some());
            }
            let m = game_message_create_object(&mut h.w, guard, false, false);
            dereth_protocol::read_body_padded::<ItemCreateObject>(&m.data[4..])
                .expect("decodes")
                .0
                .objdesc
        };
        assert_ne!(bare, *od, "the worn items change the ObjDesc");
    }

    /// The Academy's NPCs are all linked children of generators (`ActivateLinks`). Holtburg's
    /// Renald the Eldest and Dwennon are plain landblock instances (`CreateWorldObjects`): they
    /// are dressed too.
    #[test]
    fn holtburgs_unlinked_npcs_are_dressed() {
        let mut h = world();
        let lb = LandblockId::new(0xA9B4_FFFF);
        lm::get_landblock(&mut h.w, lb, false, false);
        h.advance(0.05);
        h.tick();
        for wcid in [28856u32, 33970] {
            let npc =
                h.w.landblock_manager
                    .landblocks
                    .get(lb)
                    .expect("loaded")
                    .world_object_guids()
                    .copied()
                    .find(|&g| h.o(g).biota.weenie_class_id == wcid)
                    .expect("the NPC");
            let equipped = creature_equipment::equipped_objects_values(&h.w, npc);
            assert!(!equipped.is_empty(), "{wcid} wears its wield list");
        }
    }
}
