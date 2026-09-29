// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Missile.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Missile.cs`.
//!
//! Missile attacks shared by players and monsters: the ammo reload, aiming (the projectile speed,
//! the aim velocity and aim motion, the spawn origin, the ballistic velocity through ACE's
//! `Trajectory` solvers) and the launch (a new projectile object with its physics state).

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use std::cell::RefCell;
use std::collections::HashMap;

use dereth_primitives::{Frame, Position as PPosition, Quat, Vec3};
use empyrean_common::dotnet::numerics::{Quaternion, Vector3};
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, CombatMode, CombatStyle, MotionCommand, ParentLocation,
    PhysicsState, Placement, PlayScript, PlayerKillerStatus, PropertyBool, PropertyDataId,
    PropertyFloat, PropertyInt, Sound, WeenieError,
};
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::position_extensions;
use crate::factories::world_object_factory;
use crate::managers::{guid_manager, landblock_manager, property_manager};
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::messages::game_message_parent_event::game_message_parent_event;
use crate::network::game_messages::messages::game_message_pickup_event::game_message_pickup_event;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::physics::{motion_table, phys_ext, trajectory, trajectory2};
use crate::world_objects::creature_combat;
use crate::world_objects::world_object::{self, CtorEnv, WorldObject};
use crate::world_objects::{
    creature_equipment, player_combat, player_melee, player_networking, player_tracking,
    world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Creature_Missile.cs`.
#[derive(Debug, Default)]
pub struct CreatureMissileFields {}

// ============================================================================== helpers

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

/// `EnqueueBroadcast(msgs)`.
fn enqueue_broadcast(w: &mut World, this: ObjectGuid, msgs: &[GameMessage]) {
    let _ = world_object_networking::enqueue_broadcast(w, this, true, msgs);
}

/// `Location`.
///
/// # Panics
/// Without one (ACE: `NullReferenceException`).
fn location(w: &World, g: ObjectGuid) -> Position {
    object(w, g)
        .location()
        .expect("System.NullReferenceException: Location")
}

/// `Height` (`WorldObject_Properties.cs`): `PhysicsObj?.GetHeight() ?? 0`. A body the landblock
/// destroyed still answers in ACE (`destroyed_physics_obj`).
fn height(w: &World, g: ObjectGuid) -> f32 {
    match phys_ext::physics_obj(w, g).and_then(|h| w.physics.get(h)) {
        Some(o) => o.height(),
        None => destroyed_physics_obj(w, g).map_or(0.0, |d| d.height),
    }
}

/// The destroyed body ACE's reference still holds after the landblock removed the object.
fn destroyed_physics_obj(w: &World, g: ObjectGuid) -> Option<world_object::DestroyedPhysicsObj> {
    w.objects
        .get(g)
        .and_then(|o| o.wo.world_object.destroyed_physics_obj)
}

/// `PhysicsObj.Position`; a body the landblock destroyed answers with its cell-0 position.
///
/// # Panics
/// Without a physics body (ACE: `NullReferenceException`).
fn physics_position(w: &World, g: ObjectGuid) -> PPosition {
    match phys_ext::physics_obj(w, g) {
        Some(h) => {
            phys_ext::position(w, h).expect("ACE: PhysicsObj is null (NullReferenceException)")
        }
        None => {
            destroyed_physics_obj(w, g)
                .expect("ACE: PhysicsObj is null (NullReferenceException)")
                .position
        }
    }
}

/// `PhysicsObj.CachedVelocity`; a body the landblock destroyed answers with its last one.
///
/// # Panics
/// Without a physics body (ACE: `NullReferenceException`).
fn cached_velocity(w: &World, g: ObjectGuid) -> Vector3 {
    let v = match phys_ext::physics_obj(w, g) {
        Some(h) => {
            w.physics
                .get(h)
                .expect("ACE: target.PhysicsObj is null (NullReferenceException)")
                .cached_velocity
        }
        None => {
            destroyed_physics_obj(w, g)
                .expect("ACE: target.PhysicsObj is null (NullReferenceException)")
                .cached_velocity
        }
    };
    Vector3::new(v.x, v.y, v.z)
}

/// `PhysicsPosition.ACEPosition()`: `new Position(ObjCellID, Frame.Origin, Frame.Orientation)`.
fn ace_position(p: &PPosition) -> Position {
    let o = p.frame.origin;
    let r = p.frame.rotation;
    Position::from_vectors(
        p.cell.0,
        Vector3::new(o.x, o.y, o.z),
        Quaternion::new(r.x, r.y, r.z, r.w),
    )
}

const UNIT_X: Vector3 = Vector3::new(1.0, 0.0, 0.0);
const UNIT_Z: Vector3 = Vector3::new(0.0, 0.0, 1.0);

/// `float.ToRadians()` (ACE.Server's physics extension): `(float)(Math.PI / 180.0f * angle)`.
fn to_radians(angle: f32) -> f32 {
    (std::f64::consts::PI / f64::from(180.0f32) * f64::from(angle)) as f32
}

/// `Vector3.IsValid()` (ACE.Server's physics extension): no NaN component.
fn is_valid(v: Vector3) -> bool {
    !v.x.is_nan() && !v.y.is_nan() && !v.z.is_nan()
}

// ACE: PhysicsGlobals.EPSILON
const PHYSICS_EPSILON: f32 = 0.0002;
// ACE: PhysicsGlobals.MaxVelocity
const PHYSICS_MAX_VELOCITY: f32 = 50.0;
// ACE: PhysicsGlobals.Gravity
const PHYSICS_GRAVITY: f32 = -9.8;

thread_local! {
    /// ACE's static `ProjectileRadiusCache` (`WorldObject_Magic.cs`), shared there with the spell
    /// projectiles; DIVERGE: a cache of its own here, per world thread (the values are the same;
    /// only `ClearSpellCache` would clear ACE's).
    static PROJECTILE_RADIUS_CACHE: RefCell<HashMap<u32, f32>> = RefCell::new(HashMap::new());
}

// ============================================================================== members

/// Queues the reload motions (for an ammo launcher) and the ammo's re-appearance in the right
/// hand on `action_chain` (a new chain, enqueued here, when `None`); answers the reload time.
// ACE: Creature.ReloadMissileAmmo
pub fn reload_missile_ammo(
    w: &mut World,
    this: ObjectGuid,
    action_chain: Option<&mut ActionChain>,
) -> f32 {
    let weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let ammo = creature_equipment::get_equipped_ammo(w, this);

    let (Some(weapon), Some(ammo)) = (weapon, ammo) else {
        return 0.0;
    };

    let mut own_chain = ActionChain::new();
    let new_chain = action_chain.is_none();
    let action_chain = match action_chain {
        Some(chain) => chain,
        None => &mut own_chain,
    };

    let mut anim_length = 0.0f32;
    if object(w, weapon).is_ammo_launcher() {
        let anim_speed = creature_combat::get_anim_speed(w, this);
        //Console.WriteLine($"AnimSpeed: {animSpeed}");

        anim_length = world_object_networking::enqueue_motion_persist(
            w,
            this,
            action_chain,
            MotionCommand::Reload,
            anim_speed,
            true,
            None,
            false,
            false,
        ); // start pulling out next arrow
        world_object_networking::enqueue_motion_persist(
            w,
            this,
            action_chain,
            MotionCommand::Ready,
            1.0,
            true,
            None,
            false,
            false,
        ); // finish reloading
    }

    // ensure ammo visibility for players
    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        if creature_combat::combat_mode(w, this) != CombatMode::Missile {
            return;
        }

        world_object_networking::enqueue_action_broadcast(
            w,
            this,
            move |w: &mut World, p: ObjectGuid| {
                player_tracking::track_equipped_object(w, p, this, ammo)
            },
            false,
        );

        let mut delay_chain = ActionChain::new();
        delay_chain.add_delay_seconds(w, f64::from(0.001f32)); // ensuring this message gets sent after player broadcasts above...
        delay_chain.add_action(Actor::Object(this), move |w: &mut World| {
            broadcast_ammo_parent_event(w, this, ammo);
        });
        delay_chain.enqueue_chain(w);
    });

    if new_chain {
        own_chain.enqueue_chain(w);
    }

    let stance = player_melee::current_stance(w, this);
    let anim_length2 = motion_table::get_animation_length_between(
        w,
        object(w, this).motion_table_id(),
        stance,
        MotionCommand::Reload,
        MotionCommand::Ready,
        1.0,
    );
    //Console.WriteLine($"AnimLength: {animLength} + {animLength2}");

    anim_length + anim_length2
}

/// `EnqueueBroadcast(new GameMessageParentEvent(this, ammo, ParentLocation.RightHand,
/// Placement.RightHandCombat))`; nothing if either object is gone.
pub(crate) fn broadcast_ammo_parent_event(w: &mut World, this: ObjectGuid, ammo: ObjectGuid) {
    let Some((creature, item)) = w.objects.get2_mut(this, ammo) else {
        return;
    };
    let msg = game_message_parent_event(
        creature,
        item,
        Some(ParentLocation::RightHand),
        Some(Placement::RightHandCombat),
    );
    enqueue_broadcast(w, this, &[msg]);
}

/// The horizontal direction from `source` to `dest`.
// ACE: Creature.GetDir2D
#[must_use]
pub fn get_dir_2d(source: Vector3, dest: Vector3) -> Vector3 {
    let mut diff = dest - source;
    diff.z = 0.0;
    Vector3::normalize(diff)
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)` into the store (a new dynamic guid; recycled
/// when the weenie cannot be built).
fn create_new_world_object(w: &mut World, weenie_class_id: u32) -> Option<ObjectGuid> {
    let weenie = w.content.get_cached_weenie(weenie_class_id)?;
    let guid = guid_manager::new_dynamic_guid(w);
    let Some(wo) = CtorEnv::with_world(w, |env| {
        world_object_factory::create_world_object(env, Some(weenie), guid)
    }) else {
        guid_manager::recycle_dynamic_guid(w, guid);
        return None;
    };
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        panic!("two live objects with guid {:?}", dup.guid);
    }
    Some(guid)
}

/// Launches a projectile from this creature to `target`: a new object of the ammo's weenie, at
/// `origin` facing `orientation`, flying at `velocity`, linked to its source, target, launcher and
/// ammo. `None` when the velocity is invalid, the projectile cannot enter the world, or it cannot
/// see the shooter from where it spawns.
// ACE: Creature.LaunchProjectile
#[allow(clippy::too_many_arguments)] // ACE's parameters
pub fn launch_projectile(
    w: &mut World,
    this: ObjectGuid,
    weapon: Option<ObjectGuid>,
    ammo: ObjectGuid,
    target: ObjectGuid,
    origin: Vector3,
    orientation: Quaternion,
    velocity: Vector3,
) -> Option<ObjectGuid> {
    let player = object(w, this).is_player();

    if !is_valid(velocity) {
        if player {
            player_networking::send_weenie_error(w, this, WeenieError::YourAttackMisfired);
        }

        return None;
    }

    let ammo_wcid = object(w, ammo).biota.weenie_class_id;
    let proj = create_new_world_object(w, ammo_wcid)
        .expect("System.NullReferenceException: WorldObjectFactory.CreateNewWorldObject");

    {
        let links = object_mut(w, proj)
            .projectile
            .get_or_insert_with(Default::default);
        links.source = Some(this);
        links.target = Some(target);

        links.launcher = weapon;
        links.ammo = Some(ammo);
    }

    let mut loc = location(w, this);
    loc.set_pos(origin);
    loc.set_rotation(orientation);
    object_mut(w, proj).set_location(Some(loc));

    set_projectile_physics_state(w, this, proj, target, velocity);

    let success = landblock_manager::add_object(w, proj, false);

    if !success || phys_ext::physics_obj(w, proj).is_none() {
        // DIVERGE: a projectile that hit something while entering the world has left the store
        // already (ProjectileCollisionHelper's DIVERGE), with HitMsg set.
        let hit_msg = w
            .objects
            .get(proj)
            .is_none_or(|o| o.wo.world_object.hit_msg);
        if !hit_msg && player {
            let msg = game_message_system_chat(
                "Your missile attack hit the environment.",
                ChatMessageType::Broadcast,
            );
            player_combat::send(w, this, msg);
        }

        world_object::destroy(w, proj, true, false);
        return None;
    }

    if !world_object::is_projectile_visible(w, this, proj) {
        dispatch::on_collide_environment::on_collide_environment(w, proj);

        world_object::destroy(w, proj, true, false);
        return None;
    }

    let pk_status = if player {
        object(w, this).player_killer_status()
    } else {
        PlayerKillerStatus::Creature
    };

    let msg = game_message_public_update_property_int(
        object_mut(w, proj),
        PropertyInt::PlayerKillerStatus,
        pk_status.0.cast_signed(),
    );
    enqueue_broadcast(w, proj, &[msg]);
    enqueue_broadcast(
        w,
        proj,
        &[game_message_script(proj, PlayScript::Launch, 0.0)],
    );

    // detonate point-blank projectiles immediately
    /*var radsum = target.PhysicsObj.GetRadius() + proj.PhysicsObj.GetRadius();
    var dist = Vector3.Distance(origin, dest);
    if (dist < radsum)
    {
        Console.WriteLine($"Point blank");
        proj.OnCollideObject(target);
    }*/

    Some(proj)
}

// ACE: Creature.ProjSpawnHeight
pub const PROJ_SPAWN_HEIGHT: f32 = 0.8454;

/// Returns the origin to spawn the projectile in the attacker's local space: in front of both
/// bodies, rotated up or down by the aim angle, at 84.54% of the attacker's height.
// ACE: Creature.GetProjectileSpawnOrigin
pub fn get_projectile_spawn_origin(
    w: &World,
    this: ObjectGuid,
    projectile_wcid: u32,
    motion: MotionCommand,
) -> Vector3 {
    let h =
        phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)");
    let attacker_radius = phys_ext::get_physics_radius(w, h);
    let projectile_radius = get_projectile_radius(w, projectile_wcid);

    //Console.WriteLine($"{Name} radius: {attackerRadius}");
    //Console.WriteLine($"Projectile {projectileWcid} radius: {projectileRadius}");

    let radsum = attacker_radius * 2.0 + projectile_radius * 2.0 + PHYSICS_EPSILON;

    let mut origin = Vector3::new(0.0, radsum, 0.0);

    // rotate by aim angle
    let angle = to_radians(motion.get_aim_angle());
    let z_rotation = Quaternion::create_from_axis_angle(UNIT_X, angle);

    origin = Vector3::transform(origin, z_rotation);

    origin.z += height(w, this) * PROJ_SPAWN_HEIGHT;

    origin
}

/// Returns the cached physics radius for a projectile wcid: its setup's first sphere times its
/// default scale.
// ACE: Creature.GetProjectileRadius
#[must_use]
pub fn get_projectile_radius(w: &World, projectile_wcid: u32) -> f32 {
    if let Some(radius) =
        PROJECTILE_RADIUS_CACHE.with(|c| c.borrow().get(&projectile_wcid).copied())
    {
        return radius;
    }

    let weenie = w.content.get_cached_weenie(projectile_wcid);

    let Some(weenie) = weenie else {
        log::error!("Creature_Missile.GetProjectileRadius(): couldn't find projectile weenie {projectile_wcid}");
        return 0.0;
    };

    let Some(setup_id) = weenie
        .properties_did
        .as_ref()
        .and_then(|d| d.get(&PropertyDataId::Setup).copied())
    else {
        log::error!(
            "Creature_Missile.GetProjectileRadius(): couldn't find SetupId for {} - {}",
            weenie.weenie_class_id,
            weenie.class_name.as_deref().unwrap_or_default()
        );
        return 0.0;
    };

    let setup = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::SetupModel>(setup_id);

    let scale = weenie
        .properties_float
        .as_ref()
        .and_then(|f| f.get(&PropertyFloat::DefaultScale).copied())
        .unwrap_or(1.0);

    let sphere_radius = setup
        .as_ref()
        .and_then(|s| s.spheres.first().map(|s| s.radius))
        .expect("ACE: setup.Spheres[0] (NullReferenceException / ArgumentOutOfRangeException)");
    let result = (f64::from(sphere_radius) * scale) as f32;

    PROJECTILE_RADIUS_CACHE.with(|c| {
        c.borrow_mut().entry(projectile_wcid).or_insert(result);
    });

    result
}

// ACE: Creature.DefaultProjectileSpeed
/// The lowest value found in data (for starter bows).
pub const DEFAULT_PROJECTILE_SPEED: f32 = 20.0;

/// The launcher's `MaximumVelocity` (20 without one, or when 0), times `fast_missile_modifier` for
/// a player with Use Fast Missiles, capped at the physics engine's 50.
// ACE: Creature.GetProjectileSpeed
pub fn get_projectile_speed(w: &World, this: ObjectGuid) -> f32 {
    let missile_launcher = creature_equipment::get_equipped_missile_weapon(w, this);

    let mut max_velocity = missile_launcher
        .and_then(|g| object(w, g).maximum_velocity())
        .unwrap_or(f64::from(DEFAULT_PROJECTILE_SPEED));

    #[allow(clippy::float_cmp)] // C#'s `== 0.0f`
    if max_velocity == 0.0 {
        let launcher = missile_launcher.expect("System.NullReferenceException: missileLauncher");
        log::warn!(
            "{}.GetMissileSpeed() - {} ({}) has speed 0",
            creature_combat::shim::name(w, this),
            creature_combat::shim::name(w, launcher),
            launcher
        );

        max_velocity = f64::from(DEFAULT_PROJECTILE_SPEED);
    }

    if object(w, this).is_player()
        && player_melee::get_character_option(w, this, CharacterOption::UseFastMissiles)
    {
        max_velocity *= property_manager::get_double(w, "fast_missile_modifier", 0.0, true).item;
    }

    // hard cap in physics engine
    max_velocity = max_velocity.min(f64::from(PHYSICS_MAX_VELOCITY));

    //Console.WriteLine($"MaxVelocity: {maxVelocity}");

    max_velocity as f32
}

/// The launch velocity from eye level to the target's aim point, for the aim motion.
// ACE: Creature.GetAimVelocity
pub fn get_aim_velocity(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    projectile_speed: f32,
) -> Vector3 {
    let loc = location(w, this);
    let target_loc = location(w, target);
    let cross_landblock = loc.landblock() != target_loc.landblock();

    // eye level -> target point
    let mut origin = if cross_landblock {
        position_extensions::to_global(&loc, false)
    } else {
        loc.pos()
    };
    origin.z += height(w, this) * PROJ_SPAWN_HEIGHT;

    let mut dest = if cross_landblock {
        position_extensions::to_global(&target_loc, false)
    } else {
        target_loc.pos()
    };
    dest.z += height(w, target) / dispatch::get_aim_height::get_aim_height(w, this, target);

    let dir = Vector3::normalize(dest - origin);

    let (velocity, _time) =
        get_projectile_velocity(w, this, target, origin, dir, dest, projectile_speed, true);

    velocity
}

/// Whether the target has left the world: gone from the store, or its body destroyed by the
/// landblock (a player whose log-off finished). A missile is not fired at such a target.
#[must_use]
pub fn target_left_world(w: &World, target: ObjectGuid) -> bool {
    w.objects.get(target).is_none() || phys_ext::physics_obj(w, target).is_none()
}

/// The global launch velocity from the attacker's body plus the local spawn origin to the target;
/// also answers the spawn origin (in the attacker's landblock frame) and the facing rotation.
// ACE: Creature.CalculateProjectileVelocity
pub fn calculate_projectile_velocity(
    w: &mut World,
    this: ObjectGuid,
    local_origin: Vector3,
    target: ObjectGuid,
    projectile_speed: f32,
) -> (Vector3, Vector3, Quaternion) {
    let source_loc = ace_position(&physics_position(w, this));
    // Not ACE's (a fix, V325): the callers check `target_left_world` first and
    // fire nothing at a target whose log-off finished; ACE aimed at the destroyed body's frame in
    // landblock 0.
    let target_loc = ace_position(&physics_position(w, target));

    let cross_landblock = source_loc.landblock() != target_loc.landblock();

    let mut start_pos = if cross_landblock {
        position_extensions::to_global(&source_loc, false)
    } else {
        source_loc.pos()
    };
    let mut end_pos = if cross_landblock {
        position_extensions::to_global(&target_loc, false)
    } else {
        target_loc.pos()
    };

    let dir = Vector3::normalize(end_pos - start_pos);

    let angle = empyrean_common::math::atan2(f64::from(-dir.x), f64::from(dir.y));

    let rotation = Quaternion::create_from_axis_angle(UNIT_Z, angle as f32);

    let origin = source_loc.pos() + Vector3::transform(local_origin, rotation);

    start_pos = start_pos + Vector3::transform(local_origin, rotation);
    end_pos.z += height(w, target) / dispatch::get_aim_height::get_aim_height(w, this, target);

    let (velocity, _time) = get_projectile_velocity(
        w,
        this,
        target,
        start_pos,
        dir,
        end_pos,
        projectile_speed,
        true,
    );

    (velocity, origin, rotation)
}

/// Hides the launched ammo; monsters have infinite ammo.
// ACE: Creature.UpdateAmmoAfterLaunch
pub fn update_ammo_after_launch(w: &mut World, this: ObjectGuid, ammo: ObjectGuid) {
    // hide previously held ammo
    let msg = game_message_pickup_event(object_mut(w, ammo));
    enqueue_broadcast(w, this, &[msg]);

    // monsters have infinite ammo?

    /*if (ammo.StackSize == null || ammo.StackSize <= 1)
    {
        TryUnwieldObjectWithBroadcasting(ammo.Guid, out _, out _);
        ammo.Destroy();
    }
    else
    {
        ammo.SetStackSize(ammo.StackSize - 1);
        EnqueueBroadcast(new GameMessageSetStackSize(ammo));
    }*/
}

/// Calculates the velocity to launch the projectile from `origin` to `dest` (leading a moving
/// target unless a player turned Lead Missile Targets off), and its flight time. ACE's default:
/// `use_gravity = true`.
// ACE: Creature.GetProjectileVelocity
#[allow(clippy::too_many_arguments)]
pub fn get_projectile_velocity(
    w: &World,
    this: ObjectGuid,
    target: ObjectGuid,
    origin: Vector3,
    _dir: Vector3,
    dest: Vector3,
    speed: f32,
    use_gravity: bool,
) -> (Vector3, f32) {
    let mut time = 0.0f32;

    let gravity = if use_gravity {
        -PHYSICS_GRAVITY
    } else {
        0.00001f32
    };

    let target_velocity = cached_velocity(w, target);

    let alt_solver = property_manager::get_bool(w, "trajectory_alt_solver", false, true).item;

    if !target_velocity.equals(Vector3::ZERO) {
        if object(w, this).is_player()
            && !player_melee::get_character_option(w, this, CharacterOption::LeadMissileTargets)
        {
            // fall through
        } else {
            // use movement quartic solver
            if !alt_solver {
                let arc = trajectory::solve_ballistic_arc_moving(
                    origin,
                    speed,
                    dest,
                    target_velocity,
                    gravity,
                );
                time = arc.time;

                if arc.num > 0 {
                    return (arc.s0, time);
                }
            } else {
                return (
                    trajectory2::calculate_trajectory(
                        origin,
                        dest,
                        target_velocity,
                        speed,
                        use_gravity,
                    ),
                    time,
                );
            }
        }
    }

    // use stationary solver
    if !alt_solver {
        let arc = trajectory::solve_ballistic_arc(origin, speed, dest, gravity);

        time = arc.t0;
        (arc.s0, time)
    } else {
        (
            trajectory2::calculate_trajectory(origin, dest, Vector3::ZERO, speed, use_gravity),
            time,
        )
    }
}

/// Sets the physics state for a launched projectile: a missile that reports collisions, placed
/// at its `Location`, moving at `velocity` (spinning instead of aligning to its path when it has a
/// rotation speed), active.
// ACE: Creature.SetProjectilePhysicsState
pub fn set_projectile_physics_state(
    w: &mut World,
    _this: ObjectGuid,
    obj: ObjectGuid,
    target: ObjectGuid,
    velocity: Vector3,
) {
    dispatch::init_physics_obj::init_physics_obj(w, obj);

    phys_ext::set_physics_property_state(
        w,
        obj,
        PropertyBool::ReportCollisions,
        PhysicsState::ReportCollisions,
        Some(true),
    );
    phys_ext::set_physics_state(w, obj, PhysicsState::Missile, Some(true));
    phys_ext::set_physics_state(w, obj, PhysicsState::AlignPath, Some(true));
    phys_ext::set_physics_state(w, obj, PhysicsState::PathClipped, Some(true));
    phys_ext::set_physics_property_state(
        w,
        obj,
        PropertyBool::Ethereal,
        PhysicsState::Ethereal,
        Some(false),
    );
    phys_ext::set_physics_property_state(
        w,
        obj,
        PropertyBool::IgnoreCollisions,
        PhysicsState::IgnoreCollisions,
        Some(false),
    );

    let loc = location(w, obj);
    let (pos, rotation) = (loc.pos(), loc.rotation());
    let h =
        phys_ext::physics_obj(w, obj).expect("ACE: PhysicsObj is null (NullReferenceException)");
    if let Some(body) = w.physics.get_mut(h) {
        body.position.frame = Frame {
            origin: Vec3::new(pos.x, pos.y, pos.z),
            rotation: Quat {
                w: rotation.w,
                x: rotation.x,
                y: rotation.y,
                z: rotation.z,
            },
        };
    }

    let placement = if has_missile_flight_placement(w, obj) {
        Some(Placement::MissileFlight)
    } else {
        None
    };
    object_mut(w, obj).set_placement(placement);

    object_mut(w, obj)
        .wo
        .world_object_properties
        .current_motion_state = None;

    phys_ext::set_velocity_field(w, h, velocity);
    let target_body = phys_ext::physics_obj(w, target);
    phys_ext::set_projectile_target(w, h, target_body);

    // Projectiles with RotationSpeed get omega values and "align path" turned off which
    // creates the nice swirling animation
    let rotation_speed = object(w, obj).rotation_speed().unwrap_or(0.0);
    #[allow(clippy::float_cmp)] // C#'s `!= 0`
    if rotation_speed != 0.0 {
        phys_ext::set_physics_state(w, obj, PhysicsState::AlignPath, Some(false));
        let omega = (std::f64::consts::PI * 2.0 * rotation_speed) as f32;
        if let Some(body) = w.physics.get_mut(h) {
            body.omega_vector = Vec3::new(omega, 0.0, 0.0);
        }
    }

    let _ = phys_ext::set_active(w, h, true);
}

/// `HasMissileFlightPlacement` (`WorldObject_Properties.cs`).
fn has_missile_flight_placement(w: &World, obj: ObjectGuid) -> bool {
    crate::world_objects::world_object_properties::has_missile_flight_placement(w, obj)
}

/// The launch sound for the weapon's combat style.
// ACE: Creature.GetLaunchMissileSound
#[must_use]
pub fn get_launch_missile_sound(w: &World, weapon: ObjectGuid) -> Sound {
    match object(w, weapon).default_combat_style() {
        Some(CombatStyle::Bow) => Sound::BowRelease,
        Some(CombatStyle::Crossbow) => Sound::CrossbowRelease,
        _ => Sound::ThrownWeaponRelease1,
    }
}

// ACE: Creature.MetersToYards
pub const METERS_TO_YARDS: f32 = 1.094; // 1.09361
                                        // ACE: Creature.MissileRangeCap
pub const MISSILE_RANGE_CAP: f32 = 85.0 / METERS_TO_YARDS; // 85 yards = ~77.697 meters w/ ac formula
                                                           // ACE: Creature.DefaultMaxVelocity
pub const DEFAULT_MAX_VELOCITY: f32 = 20.0; // ?

/// The missile weapon's range: `MaximumVelocity^2 / 9.8`, capped at 85 yards.
// ACE: Creature.GetMaxMissileRange
#[must_use]
#[allow(clippy::excessive_precision)] // ACE's literal, `0.1020408163265306f`
pub fn get_max_missile_range(w: &World, this: ObjectGuid) -> f32 {
    let weapon = creature_equipment::get_equipped_missile_weapon(w, this);
    let max_velocity = weapon
        .and_then(|g| object(w, g).maximum_velocity())
        .unwrap_or(f64::from(DEFAULT_MAX_VELOCITY));

    let missile_range = (empyrean_common::math::pow(max_velocity, f64::from(2.0f32)) as f32)
        * 0.102_040_816_326_530_6f32;
    //var missileRange = (float)Math.Pow(maxVelocity, 2.0f) * 0.0682547266398198f;

    //var strengthMod = SkillFormula.GetAttributeMod((int)Strength.Current);
    //var maxRange = Math.Min(missileRange * strengthMod, MissileRangeCap);
    let max_range = missile_range.min(MISSILE_RANGE_CAP);

    // any kind of other caps for monsters specifically?
    // throwing lugian rocks @ 85 yards seems a bit far...

    //Console.WriteLine($"{Name}.GetMaxMissileRange(): maxVelocity={maxVelocity}, strengthMod={strengthMod}, maxRange={maxRange}");

    // for client display
    /*var maxRangeYards = maxRange * MetersToYards;
    if (maxRangeYards >= 10.0f)
        maxRangeYards -= maxRangeYards % 5.0f;
    else
        maxRangeYards = (float)Math.Ceiling(maxRangeYards);

    Console.WriteLine($"Max range: {maxRange} ({maxRangeYards} yds.)");*/

    max_range
}

/// The aim motion for a launch velocity: the nearest 15-degree step of its elevation.
// ACE: Creature.GetAimLevel
#[must_use]
pub fn get_aim_level(velocity: Vector3) -> MotionCommand {
    // get z-angle?
    let z_angle = Vector3::normalize(velocity).z * 90.0f32;

    #[allow(unused_assignments)]
    let mut aim_level = MotionCommand::AimLevel;

    if z_angle >= 82.5 {
        aim_level = MotionCommand::AimHigh90;
    } else if z_angle >= 67.5 {
        aim_level = MotionCommand::AimHigh75;
    } else if z_angle >= 52.5 {
        aim_level = MotionCommand::AimHigh60;
    } else if z_angle >= 37.5 {
        aim_level = MotionCommand::AimHigh45;
    } else if z_angle >= 22.5 {
        aim_level = MotionCommand::AimHigh30;
    } else if z_angle >= 7.5 {
        aim_level = MotionCommand::AimHigh15;
    } else if z_angle > -7.5 {
        aim_level = MotionCommand::AimLevel;
    } else if z_angle > -22.5 {
        aim_level = MotionCommand::AimLow15;
    } else if z_angle > -37.5 {
        aim_level = MotionCommand::AimLow30;
    } else if z_angle > -52.5 {
        aim_level = MotionCommand::AimLow45;
    } else if z_angle > -67.5 {
        aim_level = MotionCommand::AimLow60;
    } else if z_angle > -82.5 {
        aim_level = MotionCommand::AimLow75;
    } else {
        aim_level = MotionCommand::AimLow90;
    }

    //Console.WriteLine($"Z Angle: {aimLevel.GetAimAngle()}");

    aim_level
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Creature.UpdateAmmoAfterLaunch
pub fn creature_update_ammo_after_launch(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    ammo: empyrean_entity::ObjectGuid,
) {
    update_ammo_after_launch(w, this, ammo);
}
