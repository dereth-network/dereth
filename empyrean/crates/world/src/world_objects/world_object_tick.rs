// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Tick.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Tick.cs`.
//!
//! `Time.GetUnixTime()` and `DateTime.UtcNow` read the tick snapshot (`w.now`, or the
//! constructor's `env.w.now`). The landblock's tick lists order objects by the three `Next*Time`
//! fields, read through [`next_heartbeat_time`], [`next_generator_update_time`] and
//! [`next_generator_regeneration_time`].

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::time::Time;
use empyrean_entity::enums::{PropertyFloat, PropertyString, WeenieType};
use empyrean_entity::models::properties_enchantment_registry_extensions::has_enchantments;
use empyrean_entity::ObjectGuid;

use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::world_objects::world_object_generators;
use crate::World;

/// Non-property fields declared in `WorldObject_Tick.cs`.
///
/// Not here: `stopwatch` (the `ServerPerformanceMonitor` timing of `UpdateObjectPhysics`, which
/// is physics).
#[derive(Debug, Default)]
pub struct WorldObjectTickFields {
    // ACE: WorldObject.CachedHeartbeatInterval
    pub cached_heartbeat_interval: f64,
    /// A value of Double.MaxValue indicates that there is no NextHeartbeat.
    // ACE: WorldObject.NextHeartbeatTime
    pub next_heartbeat_time: f64,
    // ACE: WorldObject.cachedRegenerationInterval
    pub cached_regeneration_interval: f64,
    /// A value of Double.MaxValue indicates that there is no NextGeneratorHeartbeat.
    // ACE: WorldObject.NextGeneratorUpdateTime
    pub next_generator_update_time: f64,
    /// A value of Double.MaxValue indicates that there is no NextGeneratorRegeneration.
    // ACE: WorldObject.NextGeneratorRegenerationTime
    pub next_generator_regeneration_time: f64,
    // ACE: WorldObject.lastDist
    pub last_dist: f64,
    /// `PhysicsTimer.CurrentTime` when the object was built (a field initializer in ACE). Read only
    /// by `UpdateObjectPhysics`; set by the physics unit (3.5) with `PhysicsTimer`.
    // ACE: WorldObject.physicsCreationTime
    pub physics_creation_time: f64,
    // ACE: WorldObject.LastPhysicsUpdate
    pub last_physics_update: f64,
    // ACE: WorldObject.slowUpdateObjectPhysicsHits
    pub slow_update_object_physics_hits: i32,
}

// ACE: WorldObject.heartbeatSpreadInterval
const HEARTBEAT_SPREAD_INTERVAL: i32 = 5;

/// `public static double ProjectileTimeout = 30.0f;`
// ACE: WorldObject.ProjectileTimeout
pub const PROJECTILE_TIMEOUT: f64 = 30.0;

/// `public static double UpdateRate_Creature = 0.2f;`: the float literal widened to double.
// ACE: WorldObject.UpdateRate_Creature
#[allow(clippy::cast_lossless)]
pub const UPDATE_RATE_CREATURE: f64 = 0.2f32 as f64;

/// `wo.NextHeartbeatTime`: the key of the landblock's heartbeat list.
#[must_use]
pub fn next_heartbeat_time(o: &WorldObject) -> f64 {
    o.wo.world_object_tick.next_heartbeat_time
}

/// `wo.NextGeneratorUpdateTime`: the key of the landblock's generator-update list.
#[must_use]
pub fn next_generator_update_time(o: &WorldObject) -> f64 {
    o.wo.world_object_tick.next_generator_update_time
}

/// `wo.NextGeneratorRegenerationTime`: the key of the landblock's regeneration list.
#[must_use]
pub fn next_generator_regeneration_time(o: &WorldObject) -> f64 {
    o.wo.world_object_tick.next_generator_regeneration_time
}

/// The constructor's call: `InitializeHeartbeats()` with `Time.GetUnixTime()` from the
/// construction environment.
pub fn initialize_heartbeats(o: &mut WorldObject, env: &CtorEnv<'_>) {
    world_object_initialize_heartbeats(o, env.w.now.unix_time);
}

/// `InitializeHeartbeats()` at `current_unix_time`. Draws `ThreadSafeRandom` once when the
/// object heartbeats (always, unless its `HeartbeatInterval` is set to 0 or less).
// ACE: WorldObject.InitializeHeartbeats
pub fn world_object_initialize_heartbeats(o: &mut WorldObject, current_unix_time: f64) {
    if o.biota.weenie_type == WeenieType::GamePiece {
        o.set_heartbeat_interval(Some(1.0));
    }

    if o.heartbeat_interval().is_none() {
        o.set_heartbeat_interval(Some(5.0));
    }

    if o.regeneration_interval() < 0.0 {
        log::warn!(
            "{} ({}).InitializeHeartBeats() - RegenerationInterval {}, setting to 0",
            o.get_property(PropertyString::Name).unwrap_or_default(),
            o.guid,
            o.regeneration_interval()
        );
        o.set_regeneration_interval(0.0);
    }

    let cached_heartbeat_interval = o.heartbeat_interval().unwrap_or(0.0);
    let t = &mut o.wo.world_object_tick;
    t.cached_heartbeat_interval = cached_heartbeat_interval;

    if t.cached_heartbeat_interval > 0.0 {
        // The intention of this code was just to spread the heartbeat ticks out a little over a 0-5s range,
        let delay = ThreadSafeRandom::next_float(0.0, HEARTBEAT_SPREAD_INTERVAL as f32);

        t.next_heartbeat_time = current_unix_time + delay;
    } else {
        t.next_heartbeat_time = f64::MAX; // Disable future HeartBeats
    }

    let regeneration_interval = o.regeneration_interval();
    let is_generator = o.is_generator();
    let t = &mut o.wo.world_object_tick;
    t.cached_regeneration_interval = regeneration_interval;

    if is_generator {
        t.next_generator_update_time = current_unix_time; // Generators start right away
                                                          //NextGeneratorUpdateTime = Time.GetFutureUnixTime(CachedHeartbeatInterval);
                                                          //NextGeneratorRegenerationTime = Time.GetFutureUnixTime(CachedHeartbeatInterval);
        if t.cached_regeneration_interval == 0.0 {
            t.next_generator_regeneration_time = f64::MAX;
        }
    } else {
        t.next_generator_update_time = f64::MAX; // Disable future GeneratorHeartBeats
        t.next_generator_regeneration_time = f64::MAX;
    }
}

/// Should only be used by Landblocks.
// ACE: WorldObject.ReinitializeHeartbeats
pub fn reinitialize_heartbeats(w: &mut World, this: ObjectGuid) {
    let now = w.now.unix_time;
    if let Some(o) = w.objects.get_mut(this) {
        world_object_initialize_heartbeats(o, now);
    }
}

/// Called every 5 seconds for WorldObject base.
// ACE: WorldObject.GeneratorUpdate
pub fn generator_update(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    world_object_generators::generator_update(w, this);

    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    o.set_property(PropertyFloat::GeneratorUpdateTimestamp, current_unix_time);

    o.wo.world_object_tick.next_generator_update_time = current_unix_time + 5.0;
}

/// Called every `RegenerationInterval` seconds for WorldObject base.
// ACE: WorldObject.GeneratorRegeneration
pub fn generator_regeneration(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    //Console.WriteLine($"{Name}.GeneratorRegeneration({currentUnixTime})");

    world_object_generators::generator_generate(w, this);

    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    o.set_property(PropertyFloat::RegenerationTimestamp, current_unix_time);

    let t = &mut o.wo.world_object_tick;
    if t.cached_regeneration_interval > 0.0 {
        t.next_generator_regeneration_time = current_unix_time + t.cached_regeneration_interval;
    }

    //Console.WriteLine($"{Name}.NextGeneratorRegenerationTime({NextGeneratorRegenerationTime})");
}

impl WorldObject {
    // ACE: WorldObject.IsLifespanSpent
    #[must_use]
    pub fn is_lifespan_spent(&self, utc_now: DotNetDateTime) -> bool {
        self.lifespan().is_some() && self.get_remaining_lifespan(utc_now) <= 0
    }

    /// Seconds until `CreationTimestamp + Lifespan`, truncated; `int.MaxValue` without a lifespan.
    // ACE: WorldObject.GetRemainingLifespan
    #[must_use]
    pub fn get_remaining_lifespan(&self, utc_now: DotNetDateTime) -> i32 {
        let Some(lifespan) = self.lifespan() else {
            return i32::MAX;
        };

        let creation_timestamp = self.creation_timestamp().unwrap_or(0);
        let expiration_timestamp =
            Time::get_date_time_from_timestamp(f64::from(creation_timestamp))
                .add_seconds(f64::from(lifespan));
        let time_to_expiration = expiration_timestamp - utc_now;

        time_to_expiration.total_seconds().cs_cast()
    }
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

/// Called every ~5 seconds for WorldObject base.
// ACE: WorldObject.Heartbeat
pub fn world_object_heartbeat(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    current_unix_time: f64,
) {
    let utc_now = w.now.utc;
    let Some(o) = w.objects.get(this) else { return };

    // `EnchantmentManager.HasEnchantments`: the caching manager's value is the registry's.
    if has_enchantments(o.biota.properties_enchantment_registry.as_ref()) {
        // `EnchantmentManager.HeartBeat(CachedHeartbeatInterval);`
        // Not ACE's (retail captures, V280): a player's enchantments are
        // credited the seconds since its previous heartbeat (4 to 6 s); others ACE's interval.
        let interval = crate::world_objects::player_tick::heartbeat_credit(w, this);
        crate::world_objects::managers::enchantment_manager::heart_beat(w, this, interval);
    }

    let Some(o) = w.objects.get(this) else { return };
    if o.is_lifespan_spent(utc_now) {
        crate::world_objects::world_object_decay::delete_object(w, this, None);
    }

    let Some(o) = w.objects.get_mut(this) else {
        return;
    };
    o.set_property(PropertyFloat::HeartbeatTimestamp, current_unix_time);
    o.wo.world_object_tick.next_heartbeat_time =
        current_unix_time + o.wo.world_object_tick.cached_heartbeat_interval;
}

/// ACE's rules are ported as `i_actor::world_object_enqueue_action` (the routing
/// rule, next to the actors); this is the dispatch target that runs them.
// ACE: WorldObject.EnqueueAction
pub fn world_object_enqueue_action(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    action: crate::entity::actions::i_action::Action,
) {
    crate::entity::actions::i_actor::world_object_enqueue_action(w, this, action);
}

/// Handles calling the physics engine for non-player objects: a creature at most every
/// `UpdateRate_Creature` seconds, a missile always (until `ProjectileTimeout`), anything else only
/// while animating or in its first updates. Syncs `Location` from the body when it moved, and
/// answers whether the body changed landblock.
// ACE: WorldObject.UpdateObjectPhysics
pub fn world_object_update_object_physics(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> bool {
    use crate::physics::phys_ext;

    let Some(h) = phys_ext::physics_obj(w, this) else {
        return false;
    };
    if !phys_ext::is_active(w, h) {
        return false;
    }

    let mut is_dying = false;
    let mut cached_velocity_fix = false;
    let now = phys_ext::physics_timer_current_time(w);
    let Some(o) = w.objects.get(this) else {
        return false;
    };
    let (is_creature, is_spell_projectile) = (o.is_creature(), o.is_spell_projectile());
    let Some(initial_updates) = phys_ext::server_record(w, h).map(|e| e.initial_updates) else {
        return false;
    };

    if is_creature {
        let last_physics_update =
            phys_ext::server_record(w, h).map_or(0.0, |e| e.last_physics_update);
        if last_physics_update + UPDATE_RATE_CREATURE > now {
            return false;
        }

        if let Some(e) = phys_ext::ext_mut(w, h) {
            e.last_physics_update = now;
        }

        // monsters have separate physics updates,
        // except during the first frame of spawning, idle emotes, and dying
        is_dying = crate::world_objects::monster_combat::is_dead(
            w.objects.get(this).expect("checked above"),
        );

        // determine if updates should be run for object
        let is_monster = crate::world_objects::monster::fields(w, this).is_monster;
        let is_awake = crate::world_objects::monster_awareness::is_awake(w, this);
        let run_update = phys_ext::is_animating(w, h) && (!is_monster || !is_awake)
            || is_dying
            || initial_updates <= 1;

        if !run_update {
            return false;
        }

        if is_monster && !is_awake {
            cached_velocity_fix = true;
        }
    } else {
        // arrows / spell projectiles
        let is_missile =
            (phys_ext::state(w, h).0 & empyrean_entity::enums::PhysicsState::Missile.0) != 0;
        if is_missile {
            let physics_creation_time =
                phys_ext::server_record(w, h).map_or(0.0, |e| e.physics_creation_time);
            if physics_creation_time + PROJECTILE_TIMEOUT <= now {
                // only for projectiles?
                phys_ext::set_active(w, h, false);
                crate::world_objects::world_object::destroy(w, this, true, false);
                return false;
            }

            // missiles always run an update
        } else {
            // determine if updates should be run for object
            let run_update = phys_ext::is_animating(w, h) || initial_updates <= 1;

            if !run_update {
                return false;
            }
        }
    }

    // get position before
    let prev_pos = phys_ext::position(w, h).map(|p| p.frame.origin);
    let cell_before = phys_ext::cur_cell(w, h).map_or(0, |c| c.0);

    phys_ext::update_object(w, h);

    // get position after
    let Some(new_position) = phys_ext::position(w, h) else {
        return false;
    };
    let new_pos = new_position.frame.origin;

    // handle landblock / cell change
    let is_moved = prev_pos != Some(new_pos);

    let Some(cur_cell) = phys_ext::cur_cell(w, h) else {
        phys_ext::set_active(w, h, false);
        crate::world_objects::world_object::destroy(w, this, true, false);
        return false;
    };

    let landblock_update = (cell_before >> 16) != (cur_cell.0 >> 16);

    if is_moved || is_dying {
        let Some(o) = w.objects.get_mut(this) else {
            return landblock_update;
        };
        if let Some(location) = o.get_position_mut(empyrean_entity::enums::PositionType::Location) {
            if cur_cell.0 != cell_before {
                location.set_landblock_id(empyrean_entity::LandblockId::new(cur_cell.0));
            }

            // skip ObjCellID check when updating from physics
            location.position_x = new_pos.x;
            location.position_y = new_pos.y;
            location.position_z = new_pos.z;

            let r = new_position.frame.rotation;
            location.set_rotation(empyrean_common::dotnet::Quaternion::new(r.x, r.y, r.z, r.w));
        }
    }

    if is_spell_projectile {
        use crate::world_objects::spell_projectile;

        let velocity = crate::world_objects::world_object_properties::velocity(w, this);
        if velocity == empyrean_common::dotnet::Vector3::ZERO
            && spell_projectile::fields(w, this).debug_velocity < 30
        {
            // todo: ensure this doesn't produce any false positives, then add mitigation code until fully debugged
            let f = spell_projectile::fields_mut(w, this);
            f.debug_velocity += 1;

            if f.debug_velocity == 30 {
                let location = w
                    .objects
                    .get(this)
                    .and_then(WorldObject::location)
                    .map(|l| l.to_loc_string())
                    .unwrap_or_default();
                let source = w
                    .objects
                    .get(this)
                    .and_then(|o| o.projectile)
                    .and_then(|l| l.source)
                    .filter(|&s| w.objects.get(s).is_some());
                let source_name = source
                    .and_then(|s| crate::dispatch::name::name(w, s))
                    .unwrap_or_default();
                let source_guid = source.map(|s| s.to_string()).unwrap_or_default();
                let spell = spell_projectile::fields(w, this).spell.as_ref();
                log::error!(
                    "Spell projectile w/ zero velocity detected @ {location}, launched by {source_name} ({source_guid}), spell ID {} - {}",
                    spell.map(|s| s.id().to_string()).unwrap_or_default(),
                    spell.map(|s| s.name().to_owned()).unwrap_or_default()
                );
            }
        }

        if spell_projectile::fields(w, this).spell_type
            == crate::entity::spell_projectile_type::ProjectileSpellType::Ring
        {
            let f = spell_projectile::fields(w, this);
            let location = w.objects.get(this).and_then(WorldObject::location);
            let dist = f
                .spawn_pos
                .as_ref()
                .expect("ACE: SpawnPos is null (NullReferenceException)")
                .distance_to(location.as_ref());
            let max_range = f
                .spell
                .as_ref()
                .expect("ACE: Spell is null (NullReferenceException)")
                .base_range_constant();
            //Console.WriteLine("Max range: " + maxRange);
            if dist > max_range {
                phys_ext::set_active(w, h, false);
                spell_projectile::projectile_impact(w, this);
                return false;
            }
        }
    }

    if cached_velocity_fix {
        if let Some(b) = w.physics.get_mut(h) {
            b.cached_velocity = dereth_primitives::Vec3::new(0.0, 0.0, 0.0);
        }
    }

    // ServerPerformanceMonitor's timing (the `finally` block) is not measured here.
    landblock_update
}
