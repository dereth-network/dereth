//! ACE: Source/ACE.Server/WorldObjects/Player.cs::FinalizeLogout
//! WorldObject_Database/Player_Database saves, WorldObject_Links and Player logout members follow
//! ACE.
//! Fixture: synthetic dats, isolated world state.

use std::sync::Arc;

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::not_ported;
use empyrean_content::models::world::{LandblockInstance, LandblockInstanceLink};
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    PositionType, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::models::PropertiesGenerator;
use empyrean_entity::{Biota, ObjectGuid, Position};
use empyrean_store::{MemShard, ShardHandle};
use empyrean_world::dispatch::Class;
use empyrean_world::entity::timers::TimersState;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{
    player, player_database as pdb, player_tick, world_object_database as wodb,
    world_object_links as links,
};
use empyrean_world::World;

const PLAYER: u32 = 0x5000_0001;

fn world() -> (World, VirtualClock) {
    let clock = VirtualClock::default();
    let timers = TimersState::new(&clock);
    let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
    let mut w = World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .unwrap(),
    );
    w.timers = timers;
    (w, clock)
}

fn with_shard(w: &mut World, shard: MemShard) {
    w.shard = ShardHandle::synchronous(Box::new(shard), Arc::new(VirtualClock::default()));
}

fn object(w: &mut World, guid: u32, class: Class, weenie_type: WeenieType) -> ObjectGuid {
    let mut o = WorldObject::allocate(class);
    o.guid = ObjectGuid::new(guid);
    o.biota = Biota {
        id: guid,
        weenie_class_id: 50,
        weenie_type,
        ..Default::default()
    };
    o.biota.properties_enchantment_registry = Some(Vec::new());
    w.objects.insert(o).unwrap();
    ObjectGuid::new(guid)
}

fn o(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap()
}

fn loc(x: f32) -> Position {
    Position::from_components(0xA9B4_0019, x, 10.0, 94.0, 0.0, 0.0, 0.0, 1.0, false)
}

fn saved(w: &World, id: u32) -> Option<empyrean_store::models::shard::Biota> {
    w.shard.base_database().get_biota(id, false)
}

fn saved_x(w: &World, id: u32) -> Option<f32> {
    saved(w, id)?
        .biota_properties_position
        .iter()
        .find(|p| p.position_type == PositionType::Location.0)
        .map(|p| p.origin_x)
}

// ---- WorldObject_Database ----------------------------------------------------------------------

#[test]
fn save_biota_writes_the_cached_positions_stamps_the_save_and_snapshots_the_biota() {
    let (mut w, _) = world();
    let g = object(
        &mut w,
        0x8000_0001,
        Class::GenericObject,
        WeenieType::Generic,
    );
    o(&mut w, g).set_location(Some(loc(10.0)));
    o(&mut w, g).wo.world_object_database.changes_detected = false;
    // A move changes the cached position only (ACE callers mutate GetPosition's reference).
    let p = o(&mut w, g)
        .get_position_mut(PositionType::Location)
        .unwrap();
    let mut xyz = p.pos();
    xyz.x = 12.5;
    p.set_pos(xyz);
    assert_eq!(
        o(&mut w, g)
            .biota
            .properties_position
            .as_ref()
            .unwrap()
            .get(&PositionType::Location)
            .unwrap()
            .position_x,
        10.0
    );

    // enqueueSave = false: the routines, not the save.
    empyrean_world::dispatch::save_biota_to_database::save_biota_to_database(&mut w, g, false);
    let now = w.now;
    let d = &o(&mut w, g).wo.world_object_database;
    assert_eq!(
        (d.last_requested_database_save, d.changes_detected),
        (now.utc, false)
    );
    assert!(saved(&w, 0x8000_0001).is_none(), "nothing enqueued");
    assert_eq!(
        o(&mut w, g)
            .biota
            .properties_position
            .as_ref()
            .unwrap()
            .get(&PositionType::Location)
            .unwrap()
            .position_x,
        12.5,
        "cache written back"
    );

    // enqueueSave = true: CheckpointTimestamp, then the snapshot to the shard. The checkpoint is
    // stamped before the changed flag is cleared, so the saved object is left unchanged (V343,
    // a fix: ACE's stamp re-marked it, and it was saved again).
    o(&mut w, g).set_location(Some(loc(14.0)));
    empyrean_world::dispatch::save_biota_to_database::save_biota_to_database(&mut w, g, true);
    assert_eq!(saved_x(&w, 0x8000_0001), Some(14.0));
    assert!(
        !o(&mut w, g).wo.world_object_database.changes_detected,
        "a save leaves the object unchanged"
    );
    assert_eq!(
        saved(&w, 0x8000_0001)
            .unwrap()
            .biota_properties_float
            .iter()
            .find(|r| r.r#type == PropertyFloat::CheckpointTimestamp.0)
            .map(|r| r.value),
        Some(now.unix_time),
        "the snapshot carries the checkpoint"
    );
    assert_eq!(
        o(&mut w, g).get_property(PropertyFloat::CheckpointTimestamp),
        Some(now.unix_time)
    );
    assert!(o(&mut w, g).biota_originated_from_or_has_been_saved_to_database());

    // A later change does not reach the saved snapshot.
    o(&mut w, g).set_property(PropertyInt::StackSize, 9);
    assert!(saved(&w, 0x8000_0001)
        .unwrap()
        .biota_properties_int
        .iter()
        .all(|r| r.r#type != PropertyInt::StackSize.0));

    // RemoveBiotaFromDatabase: back to MinValue, changed, removed from the shard.
    wodb::remove_biota_from_database(&mut w, g, true);
    assert!(saved(&w, 0x8000_0001).is_none());
    let d = &o(&mut w, g).wo.world_object_database;
    assert_eq!(
        (d.last_requested_database_save, d.changes_detected),
        (DotNetDateTime::MIN_VALUE, true)
    );
}

#[test]
fn a_failed_biota_save_flags_the_player_and_its_next_tick_boots_it() {
    let (mut w, _) = world();
    let mut shard = MemShard::new();
    shard.fail_writes_of(PLAYER, true);
    with_shard(&mut w, shard);
    let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
    empyrean_world::dispatch::save_biota_to_database::save_biota_to_database(&mut w, g, true);
    for callback in w.shard.take_completed() {
        callback(&mut w);
    }
    assert!(
        o(&mut w, g)
            .player
            .as_ref()
            .unwrap()
            .player_database
            .biota_save_failed
    );

    // Logging out: the tick ends there, the flag stays.
    o(&mut w, g).player.as_mut().unwrap().player.is_logging_out = true;
    o(&mut w, g).set_property(PropertyInt::Age, 5);
    let t = w.now.unix_time;
    player_tick::player_tick(&mut w, g, t);
    assert!(
        o(&mut w, g)
            .player
            .as_ref()
            .unwrap()
            .player_database
            .biota_save_failed
    );
    assert_eq!(
        o(&mut w, g).get_property(PropertyInt::Age),
        Some(5),
        "the age update did not run"
    );

    // Otherwise the session is terminated and the flag cleared (still no age update).
    o(&mut w, g).player.as_mut().unwrap().player.is_logging_out = false;
    w.sessions.insert(
        empyrean_net::SessionId {
            client_id: 1,
            generation: 1,
        },
        SessionData {
            player: Some(g),
            ..SessionData::default()
        },
    );
    player_tick::player_tick(&mut w, g, t);
    assert!(
        !o(&mut w, g)
            .player
            .as_ref()
            .unwrap()
            .player_database
            .biota_save_failed
    );
    assert_eq!(o(&mut w, g).get_property(PropertyInt::Age), Some(5));
}

#[test]
fn static_and_dynamic_objects_persist_by_aces_rules() {
    let (mut w, _) = world();
    let st = |w: &World, g: ObjectGuid| wodb::is_static_that_should_persist_to_shard(w, g);
    let dy = |w: &World, g: ObjectGuid| wodb::is_dynamic_that_should_persist_to_shard(w, g);

    // Statics: only when saved, an owned house, a slumlord of an owned house, or a hook/storage
    // with something in it.
    let plain = object(
        &mut w,
        0x7A9B_4001,
        Class::GenericObject,
        WeenieType::Generic,
    );
    assert!(!st(&w, plain) && !dy(&w, plain));
    let now = w.now.utc;
    o(&mut w, plain)
        .wo
        .world_object_database
        .last_requested_database_save = now;
    assert!(st(&w, plain));
    let house = object(&mut w, 0x7A9B_4002, Class::House, WeenieType::House);
    assert!(!st(&w, house));
    o(&mut w, house).set_property(PropertyInstanceId::HouseOwner, PLAYER);
    assert!(st(&w, house));
    let slumlord = object(&mut w, 0x7A9B_4003, Class::SlumLord, WeenieType::SlumLord);
    assert!(!st(&w, slumlord), "no House (ParentLink)");
    o(&mut w, slumlord).wo.world_object_links.parent_link = Some(house);
    assert!(st(&w, slumlord), "its house is owned");
    let hook = object(&mut w, 0x7A9B_4004, Class::Hook, WeenieType::Hook);
    assert!(!st(&w, hook));
    o(&mut w, hook)
        .container
        .as_mut()
        .unwrap()
        .container
        .inventory
        .insert(ObjectGuid::new(0x8000_0099), ());
    assert!(st(&w, hook));

    // Dynamics: everything else, except generators and their spawns, missiles, ammunition, spell
    // projectiles, game pieces, pets, monster corpses and gateways.
    let item = object(
        &mut w,
        0x8000_0001,
        Class::GenericObject,
        WeenieType::Generic,
    );
    assert!(dy(&w, item) && !st(&w, item));
    o(&mut w, item).wo.world_object_generators.generator = Some(plain);
    assert!(!dy(&w, item), "spawned by a generator");
    o(&mut w, item)
        .wo
        .world_object_database
        .biota_originated_from_database = true;
    assert!(dy(&w, item), "from the database, whatever it is");
    for (guid, class, weenie_type) in [
        (0x8000_0002, Class::Ammunition, WeenieType::Ammunition),
        (0x8000_0003, Class::Missile, WeenieType::Missile),
        (
            0x8000_0004,
            Class::SpellProjectile,
            WeenieType::ProjectileSpell,
        ),
        (0x8000_0005, Class::GamePiece, WeenieType::GamePiece),
        (0x8000_0006, Class::Pet, WeenieType::Pet),
        (0x8000_0007, Class::CombatPet, WeenieType::CombatPet),
    ] {
        let g = object(&mut w, guid, class, weenie_type);
        assert!(!dy(&w, g), "{weenie_type:?}");
    }
    let corpse = object(&mut w, 0x8000_0008, Class::Corpse, WeenieType::Corpse);
    assert!(dy(&w, corpse), "a player's corpse");
    empyrean_world::world_objects::corpse::set_is_monster(&mut w, corpse, true);
    assert!(!dy(&w, corpse), "a monster's corpse");
    let portal = object(&mut w, 0x8000_0009, Class::Portal, WeenieType::Portal);
    assert!(dy(&w, portal));
    o(&mut w, portal).biota.weenie_class_id = 1955;
    assert!(!dy(&w, portal), "a gateway");
    assert!(!dy(&w, ObjectGuid::new(0x8000_00FF)), "gone from the store");
}

// ---- Player_Database ---------------------------------------------------------------------------

#[test]
fn rush_next_player_save_brings_the_save_forward_only() {
    let (mut w, _) = world();
    let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
    let now = w.now.utc;
    o(&mut w, g)
        .wo
        .world_object_database
        .last_requested_database_save = now;
    // Due in 300 s; rushing to 5 s: LastRequestedDatabaseSave = now + 5 - 300.
    pdb::rush_next_player_save(&mut w, g, 5);
    assert_eq!(
        o(&mut w, g)
            .wo
            .world_object_database
            .last_requested_database_save,
        now.add_seconds(5.0).add_seconds(-300.0)
    );
    // Already due sooner than 10 s from now: unchanged.
    pdb::rush_next_player_save(&mut w, g, 10);
    assert_eq!(
        o(&mut w, g)
            .wo
            .world_object_database
            .last_requested_database_save,
        now.add_seconds(-295.0)
    );
    assert_eq!(pdb::player_save_interval_secs(&w), 300);
}

#[test]
fn save_player_saves_the_character_when_changed_and_the_player_with_its_changed_possessions() {
    let (mut w, _) = world();
    let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
    o(&mut w, g).player.as_mut().unwrap().player.character =
        Some(empyrean_store::models::shard::Character {
            id: PLAYER,
            name: "Aldric".into(),
            ..Default::default()
        });
    let changed = object(
        &mut w,
        0x8000_0001,
        Class::GenericObject,
        WeenieType::Generic,
    );
    let unchanged = object(
        &mut w,
        0x8000_0002,
        Class::GenericObject,
        WeenieType::Generic,
    );
    let worn = object(
        &mut w,
        0x8000_0003,
        Class::GenericObject,
        WeenieType::Generic,
    );
    for i in [changed, unchanged] {
        o(&mut w, g)
            .container
            .as_mut()
            .unwrap()
            .container
            .inventory
            .insert(i, ());
    }
    o(&mut w, g)
        .creature
        .as_mut()
        .unwrap()
        .creature_equipment
        .equipped_objects
        .insert(worn, ());
    o(&mut w, changed).wo.world_object_database.changes_detected = true;
    o(&mut w, worn).wo.world_object_database.changes_detected = true;
    assert_eq!(
        pdb::player_get_all_possessions(&w, g),
        [changed, unchanged, worn]
    );

    // Character unchanged: not saved.
    pdb::save_player_to_database(&mut w, g);
    assert!(w.shard.base_database().get_character(PLAYER).is_none());
    assert!(
        saved(&w, PLAYER).is_some()
            && saved(&w, 0x8000_0001).is_some()
            && saved(&w, 0x8000_0003).is_some()
    );
    assert!(
        saved(&w, 0x8000_0002).is_none(),
        "no ChangesDetected: not saved"
    );
    assert!(!o(&mut w, changed).wo.world_object_database.changes_detected);

    // Character changed: saved first, and the flag cleared.
    o(&mut w, g)
        .player
        .as_mut()
        .unwrap()
        .player_database
        .character_changes_detected = true;
    pdb::save_player_to_database(&mut w, g);
    assert_eq!(
        w.shard
            .base_database()
            .get_character(PLAYER)
            .map(|c| c.name),
        Some("Aldric".to_owned())
    );
    let now = w.now.utc;
    let pd = &o(&mut w, g).player.as_ref().unwrap().player_database;
    assert!(!pd.character_changes_detected);
    assert_eq!(pd.character_last_requested_database_save, now);
}

#[test]
fn set_properties_at_log_out_stamps_the_time_and_the_passup_skills() {
    let (mut w, _) = world();
    let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
    pdb::set_properties_at_log_out(&mut w, g);
    let now = w.now.unix_time;
    let p = o(&mut w, g);
    assert_eq!(p.get_property(PropertyFloat::LogoffTimestamp), Some(now));
    assert_eq!(
        p.get_property(PropertyInt::CurrentLoyaltyAtLastLogoff),
        Some(0),
        "untrained: 0"
    );
    assert_eq!(
        p.get_property(PropertyInt::CurrentLeadershipAtLastLogoff),
        Some(0)
    );
}

// ---- Player.cs log-out ---------------------------------------------------------------------------

#[test]
fn log_out_off_a_landblock_finalizes_at_once_and_saves() {
    let (mut w, _) = world();
    let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
    o(&mut w, g).player.as_mut().unwrap().player.character = Some(Default::default());
    o(&mut w, g).set_property(PropertyString::Name, "Aldric".to_owned());
    not_ported::take_local();
    assert!(player::log_out(&mut w, g, false, false));
    let p = o(&mut w, g);
    assert!(p.player.as_ref().unwrap().player.is_logging_out && p.wo.world_object.is_busy);
    assert!(
        w.player_manager.players_pending_final_logoff.is_empty(),
        "queued, then removed by FinalizeLogout"
    );
    assert!(saved(&w, PLAYER).is_some_and(|b| b
        .biota_properties_float
        .iter()
        .any(|f| f.r#type == PropertyFloat::LogoffTimestamp.0)));
}

#[test]
fn a_player_killer_in_battle_gets_the_delayed_log_out() {
    let (mut w, _) = world();
    let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
    o(&mut w, g).set_property(
        PropertyInt::PlayerKillerStatus,
        empyrean_entity::enums::PlayerKillerStatus::PK
            .0
            .cast_signed(),
    );
    let now = w.now.unix_time;
    o(&mut w, g).set_property(PropertyFloat::LastPkAttackTimestamp, now - 60.0);
    assert!(!player::log_out(&mut w, g, false, false), "delayed");
    let p = o(&mut w, g);
    assert!(
        p.player.as_ref().unwrap().player.pk_logout
            && !p.player.as_ref().unwrap().player.is_logging_out
    );
    assert_eq!(
        p.get_property(PropertyFloat::LogoffTimestamp),
        Some(now + 20.0),
        "pk_timer"
    );
    assert_eq!(w.player_manager.players_pending_logoff.len(), 1);
    // forceImmediate skips the delay.
    o(&mut w, g).player.as_mut().unwrap().player.character = Some(Default::default());
    assert!(player::log_out(&mut w, g, false, true));
    // Two minutes after the last attack, no delay either.
    o(&mut w, g).set_property(PropertyFloat::LastPkAttackTimestamp, now - 120.0);
    assert!(player::log_out(&mut w, g, false, false));
}

// ---- WorldObject_Links -------------------------------------------------------------------------

fn instance(guid: u32, wcid: u32, x: f32, children: &[u32]) -> LandblockInstance {
    let mut i = LandblockInstance::new(guid, wcid, 0xA9B4_0001, [x, 2.0, 3.0]);
    i.is_link_child = true;
    for &c in children {
        i.landblock_instance_link.push(LandblockInstanceLink {
            parent_guid: guid,
            child_guid: c,
            ..Default::default()
        });
    }
    i
}

#[test]
fn activate_links_turns_a_generators_links_into_profiles() {
    let (mut w, _) = world();
    let biota = Biota {
        id: 0x7A9B_4001,
        weenie_class_id: 60,
        weenie_type: WeenieType::Generic,
        properties_generator: Some(Arc::new(vec![PropertiesGenerator {
            probability: -1.0,
            init_create: 1,
            max_create: 1,
            ..Default::default()
        }])),
        ..Default::default()
    };
    let g = w
        .objects
        .insert(
            factory::create_world_object_from_biota(&CtorEnv::without_content(&w), biota).unwrap(),
        )
        .map(|()| ObjectGuid::new(0x7A9B_4001))
        .unwrap();
    assert!(o(&mut w, g).is_generator());
    let a = instance(0x7A9B_4002, 70, 5.0, &[]);
    let b = instance(0x7A9B_4003, 71, 6.0, &[]);
    o(&mut w, g).wo.world_object_links.linked_instances = vec![a.clone(), b.clone()];
    let count = w.objects.len();
    links::activate_links(&mut w, g, &[a, b], &[], None);
    let profiles = &o(&mut w, g).wo.world_object_generators.generator_profiles;
    assert_eq!(
        profiles
            .iter()
            .map(|p| p.biota.weenie_class_id)
            .collect::<Vec<_>>(),
        [0, 70, 71],
        "one profile per link, after the weenie's"
    );
    assert_eq!(profiles[1].biota.obj_cell_id, Some(0xA9B4_0001));
    assert_eq!(
        w.objects.len(),
        count,
        "no objects: the generator spawns them"
    );
}

#[test]
fn activate_links_creates_children_links_them_both_ways_and_follows_nested_links() {
    let (mut w, _) = world();
    let content = empyrean_content::MemContent::new()
        .weenie(empyrean_content::models::world::Weenie::new(
            70,
            "switch",
            WeenieType::Generic,
        ))
        .weenie(empyrean_content::models::world::Weenie::new(
            71,
            "door",
            WeenieType::Generic,
        ));
    w.content = Arc::new(content);
    let parent = object(&mut w, 0x7A9B_4001, Class::House, WeenieType::House);
    let child = instance(0x7A9B_4002, 70, 5.0, &[0x7A9B_4003]);
    let grandchild = instance(0x7A9B_4003, 71, 6.0, &[]);
    let missing = instance(0x7A9B_4004, 99, 7.0, &[]);
    let source = [child.clone(), grandchild.clone(), missing.clone()];
    o(&mut w, parent).wo.world_object_links.linked_instances = vec![child, missing];
    // The shard has a biota for the grandchild: it is restored rather than built from the weenie.
    let mut restored = Biota {
        id: 0x7A9B_4003,
        weenie_class_id: 71,
        weenie_type: WeenieType::Generic,
        ..Default::default()
    };
    restored.set_property(PropertyString::Name, "restored".to_owned());
    not_ported::take_local();
    links::activate_links(&mut w, parent, &source, &[restored], None);

    let (c, gc) = (ObjectGuid::new(0x7A9B_4002), ObjectGuid::new(0x7A9B_4003));
    assert!(
        w.objects.get(ObjectGuid::new(0x7A9B_4004)).is_none(),
        "no weenie: skipped"
    );
    assert_eq!(o(&mut w, parent).wo.world_object_links.child_links, [c]);
    assert_eq!(o(&mut w, c).wo.world_object_links.parent_link, Some(parent));
    assert_eq!(
        o(&mut w, c).location().unwrap().pos().x,
        5.0,
        "at the link's position"
    );
    // Nested: the child is the grandchild's parent.
    assert_eq!(o(&mut w, c).wo.world_object_links.child_links, [gc]);
    assert_eq!(o(&mut w, gc).wo.world_object_links.parent_link, Some(c));
    assert_eq!(
        o(&mut w, gc).get_property(PropertyString::Name).as_deref(),
        Some("restored")
    );
    assert!(
        o(&mut w, gc)
            .wo
            .world_object_database
            .biota_originated_from_database
    );
    let hits = not_ported::take_local();
    assert!(
        !hits.keys().any(|k| k.starts_with("ACE: House.")),
        "House links set their properties: {hits:?}"
    );
    // the house sets its child's link properties: its HouseId and HouseOwner (none here)
    o(&mut w, parent).set_house_id(Some(77));
    o(&mut w, c).set_house_id(None);

    // UpdateLinks: the parent's UpdateLinkProperties for each child and each child's children.
    links::update_links(&mut w, parent);
    assert!(not_ported::take_local()
        .keys()
        .all(|k| !k.starts_with("ACE: House.")));
    assert_eq!(
        (o(&mut w, c).house_id(), o(&mut w, gc).house_id()),
        (Some(77), Some(77)),
        "UpdateLinkProperties set both"
    );
}

#[test]
fn a_logged_off_player_is_released_with_its_possessions_once_offline() {
    let (mut w, _) = world();
    let g = object(&mut w, PLAYER, Class::Player, WeenieType::Creature);
    let item = object(
        &mut w,
        0x8000_0001,
        Class::GenericObject,
        WeenieType::Generic,
    );
    o(&mut w, g)
        .container
        .as_mut()
        .unwrap()
        .container
        .inventory
        .insert(item, ());
    o(&mut w, g).current_landblock = Some(empyrean_entity::LandblockId::new(0xA9B4_FFFF));
    player::release_logged_off_player(&mut w, g);
    assert!(w.objects.get(g).is_some(), "still on a landblock: kept");
    o(&mut w, g).current_landblock = None;
    player::release_logged_off_player(&mut w, g);
    assert!(w.objects.get(g).is_none() && w.objects.get(item).is_none());
}

#[test]
fn activate_links_with_a_parent_links_the_children_to_that_parent() {
    // The mansion case (`Landblock.CreateWorldObjects`): a linked house's children belong to the
    // first house of the mansion.
    let (mut w, _) = world();
    w.content = Arc::new(empyrean_content::MemContent::new().weenie(
        empyrean_content::models::world::Weenie::new(70, "portal", WeenieType::Generic),
    ));
    let root = object(&mut w, 0x7A9B_4001, Class::House, WeenieType::House);
    let linked = object(&mut w, 0x7A9B_4005, Class::House, WeenieType::House);
    let child = instance(0x7A9B_4006, 70, 5.0, &[]);
    o(&mut w, linked).wo.world_object_links.linked_instances = vec![child.clone()];
    links::activate_links(&mut w, linked, &[child], &[], Some(root));
    let c = ObjectGuid::new(0x7A9B_4006);
    assert_eq!(o(&mut w, c).wo.world_object_links.parent_link, Some(root));
    assert_eq!(o(&mut w, root).wo.world_object_links.child_links, [c]);
    assert!(o(&mut w, linked)
        .wo
        .world_object_links
        .child_links
        .is_empty());
}
