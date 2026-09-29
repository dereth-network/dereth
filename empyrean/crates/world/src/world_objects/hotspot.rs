// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Hotspot.cs
//! Port of `Source/ACE.Server/WorldObjects/Hotspot.cs`: a volume that periodically damages (or
//! drains, or heals) the creatures touching it.

use dereth_physics::PhysHandle;
use empyrean_common::dotnet::numerics::{Quaternion, Vector3};
use empyrean_common::dotnet::{math, CsCast, DotNetHashSet};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{ActivationResponse, ChatMessageType, DamageType, Sound};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::body_part::BodyPart;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::physics::phys_ext;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `Hotspot.cs`.
#[derive(Debug, Default)]
pub struct HotspotFields {
    /// The creatures touching the hotspot.
    // ACE: Hotspot.Creatures
    pub creatures: DotNetHashSet<ObjectGuid>,
    /// Whether an action loop is running (ACE's `ActionLoop != null`; the chain itself lives in
    /// the action queues).
    // ACE: Hotspot.ActionLoop
    pub action_loop: bool,
}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut HotspotFields {
    match &mut w.objects.get_mut(this).expect("ACE: this is null").kind {
        crate::world_objects::kinds::KindData::Hotspot(d) => &mut d.hotspot,
        _ => panic!("InvalidCastException: not a Hotspot"),
    }
}

fn fields(w: &World, this: ObjectGuid) -> &HotspotFields {
    match &object(w, this).kind {
        crate::world_objects::kinds::KindData::Hotspot(d) => &d.hotspot,
        _ => panic!("InvalidCastException: not a Hotspot"),
    }
}

// ACE: Hotspot.OnCollideObject
/// A creature (or, unless `AffectsAis`, only a player) that touches the hotspot joins its set, and
/// the action loop starts if it is not running.
pub fn hotspot_on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    let Some(wo) = w.objects.get(target) else {
        return;
    };
    if !wo.is_creature() {
        return;
    }

    if !object(w, this).affects_ais() && !wo.is_player() {
        return;
    }

    if !fields(w, this).creatures.contains(&target) {
        //Console.WriteLine($"{Name} ({Guid}).OnCollideObject({creature.Name})");
        fields_mut(w, this).creatures.insert(target);
    }

    if !fields(w, this).action_loop {
        // Not ACE's (a fix, V342): one loop is built and enqueued, with one
        // random cycle time. ACE built a loop, kept it, and enqueued a second one, so each start
        // drew the cycle time twice and discarded the first draw.
        next_action_loop(w, this).enqueue_chain(w);
    }
}

// ACE: Hotspot.OnCollideObjectEnd
/// Empty in ACE (the leaving creature is dropped by the next activation's touching check).
#[allow(unused_variables)]
pub fn hotspot_on_collide_object_end(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    /*if (!(wo is Player player))
        return;

    if (Players.Contains(player.Guid))
        Players.Remove(player.Guid);*/
}

// ACE: Hotspot.NextActionLoop
/// A new loop: wait `CycleTimeNext`, then activate on every creature and loop again while any are
/// left.
fn next_action_loop(w: &mut World, this: ObjectGuid) -> ActionChain {
    fields_mut(w, this).action_loop = true;
    let mut action_loop = ActionChain::new();
    let delay = cycle_time_next(w, this);
    action_loop.add_delay_seconds(w, delay);
    action_loop.add_action(Actor::Object(this), move |w: &mut World| {
        if w.objects.get(this).is_none() {
            return;
        }
        if !fields(w, this).creatures.is_empty() {
            activate(w, this);
            next_action_loop(w, this).enqueue_chain(w);
        } else {
            fields_mut(w, this).action_loop = false;
        }
    });
    action_loop
}

// ACE: Hotspot.CycleTimeNext
/// A random time between `CycleTime * (1 - CycleTimeVariance)` and `CycleTime`.
///
/// # Panics
/// With a null `CycleTime` (ACE: `InvalidOperationException`; the constructor sets it to at least 1).
fn cycle_time_next(w: &World, this: ObjectGuid) -> f64 {
    let o = object(w, this);
    let max = o.cycle_time();
    // `max * (1.0f - CycleTimeVariance ?? 0.0f)`: `??` binds looser than `-`
    let min = max.map(|m| m * (1.0 - o.cycle_time_variance().unwrap_or(0.0)));

    #[allow(clippy::cast_possible_truncation)]
    let (min, max) = (
        min.expect("ACE: CycleTime is null (InvalidOperationException)") as f32,
        max.expect("ACE: CycleTime is null (InvalidOperationException)") as f32,
    );
    ThreadSafeRandom::next_float(min, max)
}

// ACE: Hotspot.DamageNext
/// A random amount between the base damage's min and max.
fn damage_next(w: &mut World, this: ObjectGuid) -> f32 {
    let r = crate::dispatch::get_base_damage::get_base_damage(w, this);
    #[allow(clippy::cast_precision_loss)]
    let max = r.max_damage as f32;
    let p: f32 = ThreadSafeRandom::next_float(r.min_damage(), max).cs_cast();
    p
}

// ACE: Hotspot.DamageType
/// `(DamageType)_DamageType`.
///
/// # Panics
/// Without a `DamageType` property (ACE: `InvalidOperationException` on the null cast).
#[must_use]
pub fn damage_type(o: &WorldObject) -> DamageType {
    DamageType(
        o.damage_type_raw()
            .expect("ACE: _DamageType is null (InvalidOperationException)")
            .cs_cast(),
    )
}

/// `PhysicsObj.is_touching(obj)` (ACE's custom helper for hotspots in `Physics/PhysicsObj.cs`):
/// on the same landblock, one of this body's spheres meets one of `obj`'s spheres or
/// cylinder-spheres, in landblock coordinates.
fn is_touching(w: &World, this: PhysHandle, obj: PhysHandle) -> bool {
    // ensure same landblock
    // no cross-landblock collision detection here,
    // although it could be added if needed
    if phys_ext::cur_landblock(w, this) != phys_ext::cur_landblock(w, obj) {
        return false;
    }

    let (Some(a), Some(b)) = (w.physics.get(this), w.physics.get(obj)) else {
        return false;
    };

    let p_spheres = &a.geometry.spheres;
    let spheres = &b.geometry.spheres;
    let cylspheres = &b.geometry.cyl_spheres;

    if p_spheres.is_empty() || (spheres.is_empty() && cylspheres.is_empty()) {
        return false;
    }

    let local_to_global =
        |frame: &dereth_primitives::Frame, p: dereth_primitives::Vec3| -> Vector3 {
            let q = Quaternion::new(
                frame.rotation.x,
                frame.rotation.y,
                frame.rotation.z,
                frame.rotation.w,
            );
            let o = Vector3::new(frame.origin.x, frame.origin.y, frame.origin.z);
            o + Vector3::transform(Vector3::new(p.x, p.y, p.z), q)
        };

    for p_sphere in p_spheres {
        for sphere in spheres {
            // convert to landblock coordinates
            let player_center = local_to_global(&a.position.frame, p_sphere.center);
            let glob_center = local_to_global(&b.position.frame, sphere.center);

            // `Sphere.Intersects`
            let delta = glob_center - player_center;
            let rad_sum = p_sphere.radius + sphere.radius;
            if delta.length_squared() < rad_sum * rad_sum {
                return true;
            }
        }

        for cylsphere in cylspheres {
            // convert to landblock coordinates
            let center = local_to_global(&a.position.frame, p_sphere.center);
            let lowpoint = local_to_global(&b.position.frame, cylsphere.low_pt);

            let disp = center - lowpoint;
            let radsum = p_sphere.radius + cylsphere.radius - dereth_physics::globals::EPSILON;

            // `CylSphere.CollidesWithSphere(pSphere, disp, radsum)`
            if disp.x * disp.x + disp.y * disp.y <= radsum * radsum
                && p_sphere.radius - dereth_physics::globals::EPSILON + cylsphere.height * 0.5
                    >= (cylsphere.height * 0.5 - disp.z).abs()
            {
                return true;
            }
        }
    }
    false
}

// ACE: Hotspot.Activate
/// Every creature still touching the hotspot is affected; the others leave the set.
fn activate(w: &mut World, this: ObjectGuid) {
    let creatures: Vec<ObjectGuid> = fields(w, this).creatures.iter().copied().collect();
    for creature_guid in creatures {
        // `CurrentLandblock.GetObject(creatureGuid) as Creature`
        let landblock = object(w, this)
            .current_landblock
            .expect("ACE: CurrentLandblock is null (NullReferenceException)");
        let creature = crate::entity::landblock::get_object(w, landblock, creature_guid, true)
            .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_creature));

        // verify current state of collision here
        let touching = creature.is_some_and(|c| {
            let (Some(ch), Some(h)) = (phys_ext::physics_obj(w, c), phys_ext::physics_obj(w, this))
            else {
                panic!("ACE: PhysicsObj is null (NullReferenceException)")
            };
            is_touching(w, ch, h)
        });
        let Some(creature) = creature.filter(|_| touching) else {
            //Console.WriteLine($"{Name} ({Guid}).OnCollideObjectEnd({creature?.Name})");
            fields_mut(w, this).creatures.remove(&creature_guid);
            continue;
        };
        activate_creature(w, this, creature);
    }
}

// ACE: Hotspot.Activate
/// Applies the hotspot to one creature: damage (resisted) for a damage type, a drain for mana and
/// stamina, and a raw health change (a kill when it empties) for health; then the sound, the
/// `ActivationTalk` and the activation emote.
fn activate_creature(w: &mut World, this: ObjectGuid, creature: ObjectGuid) {
    use crate::world_objects::creature_vitals::update_vital_delta;
    use crate::world_objects::monster_combat::is_dead;

    if !object(w, this).is_hot() {
        return;
    }

    let mut amount = damage_next(w, this);
    let mut i_amount: i32 = math::round(f64::from(amount)).cs_cast();

    let is_player = object(w, creature).is_player();

    let damage_type = damage_type(object(w, this));
    match damage_type {
        DamageType::Mana => {
            let v = object(w, creature).mana();
            i_amount = update_vital_delta(w, creature, v, i_amount.wrapping_neg());
        }

        DamageType::Stamina => {
            let v = object(w, creature).stamina();
            i_amount = update_vital_delta(w, creature, v, i_amount.wrapping_neg());
        }

        DamageType::Health => {
            let c = object(w, creature);
            if c.invincible() || is_dead(c) {
                return;
            }

            let v = c.health();
            i_amount = update_vital_delta(w, creature, v, i_amount.wrapping_neg());

            if i_amount > 0 {
                crate::entity::damage_history::on_heal(w, creature, i_amount.cast_unsigned());
            } else {
                crate::entity::damage_history::add(
                    w,
                    creature,
                    this,
                    DamageType::Health,
                    i_amount.wrapping_neg().cast_unsigned(),
                );
            }

            if is_dead(object(w, creature)) {
                let last_damager = crate::entity::damage_history::of(w, creature).last_damager();
                crate::dispatch::on_death::on_death(
                    w,
                    creature,
                    last_damager,
                    DamageType::Health,
                    false,
                );
                crate::world_objects::creature_death::die(w, creature);

                fields_mut(w, this).creatures.remove(&creature);
            }
        }

        _ => {
            let c = object(w, creature);
            if c.invincible() || is_dead(c) {
                return;
            }

            amount *= crate::world_objects::creature_properties::get_resistance_mod_damage(
                w,
                creature,
                damage_type,
                Some(this),
                None,
                1.0,
            );

            if is_player {
                i_amount = crate::world_objects::player_combat::take_damage(
                    w,
                    creature,
                    Some(this),
                    damage_type,
                    amount,
                    BodyPart::Foot,
                    false,
                    empyrean_entity::enums::AttackConditions::None,
                );
            } else {
                i_amount = crate::dispatch::take_damage::take_damage(
                    w,
                    creature,
                    this,
                    damage_type,
                    amount,
                    false,
                )
                .cast_signed();
            }

            if is_dead(object(w, creature)) {
                fields_mut(w, this).creatures.remove(&creature);
            }
        }
    }

    if !object(w, this).visibility() {
        let msg = game_message_sound(this, Sound::TriggerActivated, 1.0);
        crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
    }

    if is_player && i_amount != 0 {
        if let Some(activation_talk) = object(w, this)
            .activation_talk()
            .filter(|t| !t.trim().is_empty())
        {
            let text = activation_talk.replace("%i", &i_amount.unsigned_abs().to_string());
            if let Some(session) = crate::managers::player_manager::player_session(w, creature) {
                enqueue_send(
                    w,
                    session,
                    game_message_system_chat(&text, ChatMessageType::Broadcast),
                );
            }
        }
    }

    // perform activation emote
    if (object(w, this).activation_response() & ActivationResponse::Emote)
        == ActivationResponse::Emote
    {
        crate::dispatch::on_emote::on_emote(w, this, creature);
    }
}

// ---- constructors and SetEphemeralValues ----

/// `new Hotspot(weenie, guid)` / `new Hotspot(biota)`: the `WorldObject` constructor, then
/// Hotspot's `SetEphemeralValues`.
// ACE: Hotspot.Hotspot
pub fn hotspot_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    hotspot_set_ephemeral_values(o, env);
}

// ACE: Hotspot.SetEphemeralValues
fn hotspot_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    // If CycleTime is less than 1, player has a very bad time.
    if o.cycle_time().unwrap_or(0.0) < 1.0 {
        o.set_cycle_time(Some(1.0));
    }
}
