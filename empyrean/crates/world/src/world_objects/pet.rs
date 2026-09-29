// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Pet.cs
//! Port of `Source/ACE.Server/WorldObjects/Pet.cs`: a passive summonable creature (and the base of
//! `CombatPet`).

use dereth_physics::PhysHandle;
use dereth_primitives::Vec3;
use empyrean_entity::enums::{MovementType, PhysicsState, WeenieError};
use empyrean_entity::{LandblockId, ObjectGuid};

use crate::network::motion::move_to_parameters::RetailMoveTo;
use crate::network::motion::movement_data::Motion;
use crate::physics::phys_ext;
use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{monster, monster_navigation, monster_tick};
use crate::World;

/// Non-property fields declared in `Pet.cs`.
#[derive(Debug, Default)]
pub struct PetFields {
    // ACE: Pet.P_PetOwner
    pub p_pet_owner: Option<ObjectGuid>,
    // ACE: Pet.P_PetDevice
    pub p_pet_device: Option<ObjectGuid>,
    // ACE: Pet.nextSlowTickTime
    pub next_slow_tick_time: f64,
}

// ACE: Pet.slowTickSeconds
const SLOW_TICK_SECONDS: f64 = 1.0;

// if the passive pet is between min-max distance to owner,
// it will turn and start running torwards its owner

// ACE: Pet.MinDistance
pub const MIN_DISTANCE: f32 = 2.0;
// ACE: Pet.MaxDistance
pub const MAX_DISTANCE: f32 = 192.0;

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn object_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// The `Pet.cs` fields of a Pet or a CombatPet, or `None` for any other object.
#[must_use]
pub fn fields_of(o: &WorldObject) -> Option<&PetFields> {
    match &o.kind {
        KindData::Pet(d) => Some(&d.pet),
        KindData::CombatPet(d) => Some(&d.pet),
        _ => None,
    }
}

fn fields(w: &World, this: ObjectGuid) -> &PetFields {
    fields_of(object(w, this)).expect("InvalidCastException: not a Pet")
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PetFields {
    match &mut object_mut(w, this).kind {
        KindData::Pet(d) => &mut d.pet,
        KindData::CombatPet(d) => &mut d.pet,
        _ => panic!("InvalidCastException: not a Pet"),
    }
}

/// `pet.P_PetOwner`: the owner while it is live (ACE keeps the reference; a gone owner reads as
/// null here).
#[must_use]
pub fn p_pet_owner(w: &World, pet: ObjectGuid) -> Option<ObjectGuid> {
    w.objects
        .get(pet)
        .and_then(fields_of)
        .and_then(|f| f.p_pet_owner)
        .filter(|&g| w.objects.contains(g))
}

/// `pet.P_PetDevice`.
#[must_use]
pub fn p_pet_device(w: &World, pet: ObjectGuid) -> Option<ObjectGuid> {
    w.objects
        .get(pet)
        .and_then(fields_of)
        .and_then(|f| f.p_pet_device)
        .filter(|&g| w.objects.contains(g))
}

fn is_passive_pet(w: &World, this: ObjectGuid) -> bool {
    monster::fields(w, this).is_passive_pet
}

/// `player.CurrentActivePet` (`Player_Use.cs`): a destroyed pet reads as null.
fn current_active_pet(w: &World, player: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::player_use::fields(w, player)
        .current_active_pet
        .filter(|&g| w.objects.contains(g))
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `PhysicsObj.GetPhysicsRadius()`: 0 for a BSP object, else the first cylinder-sphere's (or
/// sphere's) radius times the scale.
///
/// # Panics
/// Without a physics body (ACE: `NullReferenceException`).
pub(crate) fn physics_obj_get_physics_radius(w: &World, h: PhysHandle) -> f32 {
    let body = w
        .physics
        .get(h)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    if (phys_ext::state(w, h).0 & PhysicsState::HasPhysicsBSP.0) != 0 {
        return 0.0;
    }

    if let Some(cyl) = body.geometry.cyl_spheres.first() {
        return cyl.radius * body.scale;
    }
    if let Some(sphere) = body.geometry.spheres.first() {
        return sphere.radius * body.scale;
    }
    0.0
}

// ACE: Pet.Init
/// Summons the pet in front of the player (the active pet handling first); `None` is ACE's null
/// (the device's charge is still used, the pet is not summoned).
pub fn pet_init(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    pet_device: empyrean_entity::ObjectGuid,
) -> Option<bool> {
    let result = handle_current_active_pet(w, this, player);

    if result != Some(true) {
        return result;
    }

    // get physics radius of player and pet
    let player_body = object(w, player)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let player_radius = physics_obj_get_physics_radius(w, player_body);
    let pet_radius = get_pet_radius(w, this);

    let spawn_dist = player_radius + pet_radius + MIN_DISTANCE;

    let player_location = object(w, player)
        .location()
        .expect("ACE: Location is null (NullReferenceException)");
    let mut location = if is_passive_pet(w, this) {
        let l = player_location.in_front_of(f64::from(spawn_dist), true);
        object_mut(w, this).set_time_to_rot(Some(-1.0));
        l
    } else {
        player_location.in_front_of(f64::from(spawn_dist), false)
    };

    location.set_landblock_id(LandblockId::new(
        crate::entity::position_extensions::get_cell(w, &location),
    ));
    object_mut(w, this).set_location(Some(location));

    let new_name = format!("{}'s {}", name(w, player), name(w, this));
    crate::dispatch::name::set_name(w, this, new_name);

    object_mut(w, this).set_pet_owner(Some(player.full()));
    fields_mut(w, this).p_pet_owner = Some(player);

    // All pets don't leave corpses, this maybe should have been in data, but isn't so lets make sure its true.
    object_mut(w, this).set_no_corpse(true);

    let success = crate::dispatch::enter_world::enter_world(w, this);

    if !success {
        let msg = format!("Couldn't spawn {}", name(w, this));
        crate::world_objects::player_networking::send_transient_error(w, player, &msg);
        return Some(false);
    }

    crate::world_objects::player_use::fields_mut(w, player).current_active_pet = Some(this);

    object_mut(w, pet_device).set_pet(Some(this.full()));
    object_mut(w, this).set_pet_device(Some(pet_device.full()));
    fields_mut(w, this).p_pet_device = Some(pet_device);

    if is_passive_pet(w, this) {
        fields_mut(w, this).next_slow_tick_time = w.now.unix_time;
    }

    Some(true)
}

// ACE: Pet.HandleCurrentActivePet
pub fn handle_current_active_pet(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
) -> Option<bool> {
    if crate::managers::property_manager::get_bool(w, "pet_stow_replace", false, true).item {
        Some(handle_current_active_pet_replace(w, this, player))
    } else {
        handle_current_active_pet_retail(w, this, player)
    }
}

// ACE: Pet.HandleCurrentActivePet_Replace
/// The original ACE logic: a passive pet is replaced (or stowed, with the same device).
pub fn handle_current_active_pet_replace(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
) -> bool {
    let Some(current) = current_active_pet(w, player) else {
        return true;
    };

    if object(w, current).is_combat_pet() {
        // possibly add the ability to stow combat pets with passive pet devices here?
        let msg = format!("{} is already active", name(w, current));
        crate::world_objects::player_networking::send_transient_error(w, player, &msg);
        return false;
    }

    let stow_pet = object(w, this).weenie_class_id() == object(w, current).weenie_class_id();

    // despawn passive pet
    crate::world_objects::world_object::destroy(w, current, true, false);

    !stow_pet
}

// ACE: Pet.HandleCurrentActivePet_Retail
/// Retail: the active pet is stowed, and the new one needs another use.
pub fn handle_current_active_pet_retail(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
) -> Option<bool> {
    let Some(current) = current_active_pet(w, player) else {
        return Some(true);
    };

    if is_passive_pet(w, this) {
        // using a passive pet device
        // stow currently active passive/combat pet, as per retail
        // spawning the new passive pet requires another double click
        crate::world_objects::world_object::destroy(w, current, true, false);
    } else {
        // using a combat pet device
        if object(w, current).is_combat_pet() {
            let msg = format!("{} is already active", name(w, current));
            crate::world_objects::player_networking::send_transient_error(w, player, &msg);
        } else {
            // stow currently active passive pet
            // stowing the currently active passive pet w/ a combat pet device will unfortunately start the cooldown timer (and decrease the structure?) on the combat pet device, as per retail
            // spawning the combat pet will require another double click in ~45s, as per retail
            crate::world_objects::world_object::destroy(w, current, true, false);

            return None;
        }
    }
    Some(false)
}

// ACE: Pet.Tick
/// Called 5x per second for passive pets.
pub fn tick(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    monster_tick::fields_mut(w, this).next_monster_tick_time =
        current_unix_time + crate::world_objects::creature_navigation::MONSTER_TICK_INTERVAL;

    if monster_navigation::fields(w, this).is_moving {
        let h = monster_navigation::physics_obj(w, this);
        phys_ext::update_object(w, h);

        crate::world_objects::creature_navigation::update_position_sync_location(w, this);

        crate::world_objects::world_object_networking::send_update_position(w, this, false);
    }

    if current_unix_time >= fields(w, this).next_slow_tick_time {
        slow_tick(w, this, current_unix_time);
    }
}

// ACE: Pet.SlowTick
/// Called 1x per second: a pet too far from its owner vanishes; one out of reach follows.
pub fn slow_tick(w: &mut World, this: ObjectGuid, _current_unix_time: f64) {
    //Console.WriteLine($"{Name}.HeartbeatStatic({currentUnixTime})");

    fields_mut(w, this).next_slow_tick_time += SLOW_TICK_SECONDS;

    let owner = p_pet_owner(w, this).filter(|&o| object(w, o).phys.is_some());
    let Some(owner) = owner else {
        log::error!(
            "{} ({:?}).SlowTick() - P_PetOwner: {:?}, P_PetOwner.PhysicsObj: null",
            name(w, this),
            this,
            fields(w, this).p_pet_owner
        );
        crate::world_objects::world_object::destroy(w, this, true, false);
        return;
    };

    let dist = crate::world_objects::world_object_use::get_cylinder_distance(w, this, owner);

    if dist > MAX_DISTANCE {
        crate::world_objects::world_object::destroy(w, this, true, false);
    }

    // DIVERGE: ACE goes on after the Destroy above and starts a follow on the destroyed pet (its
    // broadcast and physics calls then act on a removed object); the destroyed pet has left
    // World.objects here, so the method stops.
    if w.objects.get(this).is_none() {
        return;
    }

    if !monster_navigation::fields(w, this).is_moving && dist > MIN_DISTANCE {
        start_follow(w, this);
    }
}

// ACE: Pet.StartFollow
/// Similar to `Monster_Navigation.StartTurn()`: broadcast the move, then move on the server.
fn start_follow(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.StartFollow()");

    monster_navigation::fields_mut(w, this).is_moving = true;

    let owner = p_pet_owner(w, this).expect("ACE: P_PetOwner is null (NullReferenceException)");

    // broadcast to clients
    let run_rate = monster_navigation::fields(w, this).run_rate;
    crate::dispatch::move_to::move_to(w, this, owner, run_rate);

    // perform movement on server
    // Not ACE's (retail captures, V257): the body follows with the parameters the clients are
    // sent, retail's attack chase (0x1EFF0, threshold 15.0); ACE uses its own defaults and 0.0.
    let mut mvp = RetailMoveTo::AttackChase.movement_parameters();
    mvp.distance_to_object = MIN_DISTANCE;

    //mvp.UseFinalHeading = true;

    let h = monster_navigation::physics_obj(w, this);
    let owner_body = object(w, owner)
        .phys
        .expect("ACE: P_PetOwner.PhysicsObj is null (NullReferenceException)");
    crate::physics::motion::move_to_object(w, h, owner_body, &mvp);

    // prevent snap forward
    let now = phys_ext::physics_timer_current_time(w);
    if let Some(o) = w.physics.get_mut(h) {
        o.update_time = now;
    }
}

// ACE: Pet.MoveTo
/// Broadcasts passive pet movement to clients (a combat pet moves as a monster).
pub fn pet_move_to(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    run_rate: f32,
) {
    if !is_passive_pet(w, this) {
        crate::world_objects::creature_navigation::creature_move_to(w, this, target, run_rate);
        return;
    }

    #[allow(clippy::float_cmp)]
    if monster_navigation::fields(w, this).move_speed == 0.0 {
        monster_navigation::get_movement_speed(w, this);
    }

    let mut motion = Motion::to_object(w, this, target, MovementType::MoveToObject);

    // Not ACE's (retail captures, V257): retail's pets moved with the attack chase (0x1EFF0,
    // threshold 15.0); ACE adds only CanCharge to the defaults (0x1EE1F) with 0.0.
    RetailMoveTo::AttackChase.apply(&mut motion.move_to_parameters);
    motion.move_to_parameters.distance_to_object = MIN_DISTANCE;

    motion.run_rate = monster_navigation::fields(w, this).run_rate;

    object_mut(w, this)
        .wo
        .world_object_properties
        .current_motion_state = Some(motion.clone());

    crate::world_objects::world_object_networking::enqueue_broadcast_motion(
        w, this, &motion, None, None,
    );
}

// ACE: Pet.OnMoveComplete
/// Called when the MoveTo process has completed.
pub fn pet_on_move_complete(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    status: empyrean_entity::enums::WeenieError,
) {
    //Console.WriteLine($"{Name}.OnMoveComplete({status})");

    if !is_passive_pet(w, this) {
        crate::world_objects::monster_navigation::creature_on_move_complete(w, this, status);
        return;
    }

    if status != WeenieError::None {
        return;
    }

    let h = monster_navigation::physics_obj(w, this);
    if let Some(o) = w.physics.get_mut(h) {
        o.cached_velocity = Vec3::ZERO;
    }
    monster_navigation::fields_mut(w, this).is_moving = false;
}

// ACE: Pet.GetPetRadius
/// The first sphere's radius of the pet's setup, times its scale.
///
/// # Panics
/// For a setup without spheres (ACE: `ArgumentOutOfRangeException`).
fn get_pet_radius(w: &World, this: ObjectGuid) -> f32 {
    // ACE-BUG: the result is stored in ProjectileRadiusCache instead of PetRadiusCache, so the
    // PetRadiusCache lookup never hits and the radius is read from the setup every time (the
    // stray ProjectileRadiusCache entry is keyed by a pet wcid, which no spell projectile uses,
    // so it is not stored here).
    let o = object(w, this);
    let setup = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::SetupModel>(o.setup_table_id());

    let scale = o.obj_scale().unwrap_or(1.0);

    let radius = setup
        .as_ref()
        .and_then(|s| s.spheres.first())
        .map(|s| s.radius)
        .expect("ACE: setup.Spheres[0] (ArgumentOutOfRangeException)");
    radius * scale
}

// ---- constructors and SetEphemeralValues ----

/// `new Pet(weenie, guid)` / `new Pet(biota)`: the `Creature` constructor, then
/// Pet's `SetEphemeralValues`.
// ACE: Pet.Pet
pub fn pet_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::creature::creature_ctor(o, env, src);
    pet_set_ephemeral_values(o, env);
}

// ACE: Pet.SetEphemeralValues
fn pet_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.set_ethereal(Some(true));
    o.set_radar_behavior(Some(empyrean_entity::enums::RadarBehavior::ShowNever));
    o.set_item_useable(Some(empyrean_entity::enums::Usable::No));

    o.set_suppress_generate_effect(Some(true));
}
