// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/ProjectileCollisionHelper.cs
//! Port of `Source/ACE.Server/WorldObjects/ProjectileCollisionHelper.cs`.
//!
//! Helper class for arrows / bolts / thrown weapons outside of the WorldObject hierarchy: the
//! projectile hits its target (damage through the source's `DamageTarget` or the damage event)
//! or the environment, and leaves the world.
//!
//! DIVERGE (arch, the store owns objects): ACE only removes the spent projectile from its
//! landblock and lets it be garbage-collected (its guid is not recycled); here it also leaves
//! `World.objects` then, as other unclaimed objects do.

use empyrean_entity::enums::{ChatMessageType, Skill, SkillAdvancementClass, Sound};
use empyrean_entity::ObjectGuid;

use crate::entity::damage_event::DamageEvent;
use crate::entity::landblock;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::physics::phys_ext;
use crate::world_objects::creature_combat::{self, CombatType};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_weapon::SkillOf;
use crate::world_objects::{
    monster_combat, monster_missile, player_combat, world_object_networking,
};
use crate::{dispatch, World};

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// `worldObject.PhysicsObj.is_active()`.
///
/// # Panics
/// Without a physics body (ACE: `NullReferenceException`).
fn physics_obj_is_active(w: &World, g: ObjectGuid) -> bool {
    let h = phys_ext::physics_obj(w, g).expect("ACE: PhysicsObj is null (NullReferenceException)");
    phys_ext::is_active(w, h)
}

/// `worldObject.PhysicsObj.entering_world`.
fn physics_obj_entering_world(w: &World, g: ObjectGuid) -> bool {
    phys_ext::physics_obj(w, g)
        .and_then(|h| phys_ext::server_record(w, h))
        .is_some_and(|e| e.entering_world)
}

/// `worldObject.ProjectileSource` / `ProjectileTarget` / `ProjectileLauncher`.
fn links(w: &World, g: ObjectGuid) -> crate::world_objects::world_object::ProjectileLinks {
    object(w, g).projectile.unwrap_or_default()
}

/// `Proficiency.OnSuccessUse(player, skill, difficulty)` (`Entity/Proficiency.cs`).
fn proficiency_on_success_use(w: &mut World, player: ObjectGuid, skill: SkillOf, difficulty: u32) {
    crate::entity::proficiency::on_success_use(w, player, skill.skill, difficulty);
}

/// `worldObject.TryProcEquippedItems(attacker, target, selfTarget, weapon)` (`WorldObject_Combat.cs`).
fn try_proc_equipped_items(
    w: &mut World,
    this: ObjectGuid,
    attacker: ObjectGuid,
    target: ObjectGuid,
    self_target: bool,
    weapon: Option<ObjectGuid>,
) {
    crate::world_objects::world_object_combat::try_proc_equipped_items(
        w,
        this,
        attacker,
        target,
        self_target,
        weapon,
    );
}

/// `worldObject.CurrentLandblock?.RemoveWorldObject(worldObject.Guid, showError:
/// !worldObject.PhysicsObj.entering_world)` then `PhysicsObj.set_active(false)`.
fn leave_world(w: &mut World, world_object: ObjectGuid) {
    let show_error = !physics_obj_entering_world(w, world_object);
    let body = phys_ext::physics_obj(w, world_object);
    if let Some(current_landblock) = object(w, world_object).current_landblock {
        landblock::remove_world_object(
            w,
            current_landblock,
            world_object,
            false,
            false,
            show_error,
        );
    }
    // the landblock may already have destroyed the body; a body it did not reach stops here
    if let Some(h) = body.filter(|&h| w.physics.get(h).is_some()) {
        let _ = phys_ext::set_active(w, h, false);
    }
    // Not ACE: a launcher or ammo destroyed while this projectile flew was kept for its hit
    // (`world_object::destroy`); out of the world, the projectile lets go of it
    if let Some(links) = w.objects.get(world_object).and_then(|o| o.projectile) {
        crate::world_objects::world_object::release_projectile_links(w, &links);
    }
}

/// The projectile hit `target`: its intended target takes the source's damage (a player through
/// `DamageTarget`, a monster through the damage event against its attack target), with the
/// collision sound, Dirty Fighting and the target procs; anything else counts as the environment.
/// The projectile then leaves the world.
// ACE: ProjectileCollisionHelper.OnCollideObject
#[allow(clippy::too_many_lines)]
pub fn on_collide_object(w: &mut World, world_object: ObjectGuid, target: ObjectGuid) {
    if !physics_obj_is_active(w, world_object) {
        return;
    }

    //Console.WriteLine($"Projectile.OnCollideObject - {WorldObject.Name} ({WorldObject.Guid}) -> {target.Name} ({target.Guid})");

    let links = links(w, world_object);
    if links.target.is_none_or(|t| t != target) {
        //Console.WriteLine("Unintended projectile target! (should be " + ProjectileTarget.Guid.Full.ToString("X8") + " - " + ProjectileTarget.Name + ")");
        on_collide_environment(w, world_object);
        return;
    }

    // take damage
    let is = |w: &World, g: Option<ObjectGuid>, f: fn(&WorldObject) -> bool| {
        g.filter(|&g| w.objects.get(g).is_some_and(f))
    };
    let source_creature = is(w, links.source, WorldObject::is_creature);
    let source_player = is(w, links.source, WorldObject::is_player);
    let target_creature = is(w, Some(target), WorldObject::is_creature);

    let mut damage_event: Option<DamageEvent> = None;

    if let Some(target_creature) =
        target_creature.filter(|&t| !monster_combat::is_dead(object(w, t)))
    {
        if let Some(source_player) = source_player {
            // player damage monster or player
            damage_event =
                player_combat::damage_target(w, source_player, target_creature, Some(world_object));

            if damage_event.as_ref().is_some_and(DamageEvent::has_damage) {
                let msg = game_message_sound(world_object, Sound::Collision, 1.0);
                let _ = world_object_networking::enqueue_broadcast(w, world_object, true, &[msg]);
            }
        } else if let Some(source_creature) =
            source_creature.filter(|&s| monster_combat::attack_target(w, s).is_some())
        {
            // todo: clean this up
            let target_player = monster_combat::attack_target(w, source_creature)
                .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player));

            let event = DamageEvent::calculate_damage(
                w,
                source_creature,
                target_creature,
                Some(world_object),
                None,
                None,
            );

            if let Some(target_player) = target_player {
                // monster damage player
                if event.has_damage() {
                    player_combat::take_damage_event(
                        w,
                        target_player,
                        Some(source_creature),
                        &event,
                    );

                    // blood splatter?

                    #[allow(clippy::float_cmp)] // C#'s `!= 1.0f`
                    if event.shield_mod != 1.0 {
                        let shield_skill = SkillOf::get(w, target_player, Skill::Shield);
                        let current = shield_skill.current(w);
                        proficiency_on_success_use(w, target_player, shield_skill, current);
                        // ??
                    }

                    // handle Dirty Fighting
                    if SkillOf::get(w, source_creature, Skill::DirtyFighting).advancement_class(w)
                        >= SkillAdvancementClass::Trained
                    {
                        creature_combat::fight_dirty(
                            w,
                            source_creature,
                            target_player,
                            event.weapon,
                        );
                    }
                } else {
                    player_combat::on_evade(w, target_player, source_creature, CombatType::Missile);
                }
            } else {
                // monster damage pet
                if event.has_damage() {
                    dispatch::take_damage::take_damage(
                        w,
                        target_creature,
                        source_creature,
                        event.damage_type,
                        event.damage,
                        false,
                    );

                    // blood splatter?

                    // handle Dirty Fighting
                    if SkillOf::get(w, source_creature, Skill::DirtyFighting).advancement_class(w)
                        >= SkillAdvancementClass::Trained
                    {
                        creature_combat::fight_dirty(
                            w,
                            source_creature,
                            target_creature,
                            event.weapon,
                        );
                    }
                }

                if !object(w, target_creature).is_combat_pet() {
                    // faction mobs and foetype
                    creature_combat::monster_on_attack_monster(w, source_creature, target_creature);
                }
            }

            damage_event = Some(event);
        }

        // handle target procs
        if damage_event.as_ref().is_some_and(DamageEvent::has_damage) {
            let mut thread_safe = true;

            if w.landblock_manager
                .currently_ticking_landblock_groups_multi_threaded
            {
                // Ok... if we got here, we're likely in the parallel landblock physics processing.
                let group = |w: &World, g: Option<ObjectGuid>| {
                    g.and_then(|g| w.objects.get(g))
                        .and_then(|o| o.current_landblock)
                        .map(|id| {
                            w.landblock_manager
                                .landblocks
                                .expect(id)
                                .current_landblock_group
                        })
                };
                let (a, b, c) = (
                    group(w, Some(world_object)),
                    group(w, source_creature),
                    group(w, Some(target_creature)),
                );
                if a.is_none() || b.is_none() || c.is_none() || a != b || b != c {
                    thread_safe = false;
                }
            }

            if thread_safe {
                // This can result in spell projectiles being added to either sourceCreature or targetCreature landblock.
                // worldObject is hitting targetCreature, so they should almost always be in the same landblock
                try_proc_equipped_items(
                    w,
                    world_object,
                    source_creature.expect("System.NullReferenceException: sourceCreature"),
                    target_creature,
                    false,
                    links.launcher,
                );
            } else {
                // sourceCreature and creatureTarget are now in different landblock groups.
                // What has likely happened is that sourceCreature sent a projectile toward creatureTarget. Before impact, sourceCreature was teleported away.
                // To perform this fully thread safe, we would enqueue the work onto worldManager.
                // WorldManager.EnqueueAction(new ActionEventDelegate(() => sourceCreature.TryProcEquippedItems(targetCreature, false)));
                // But, to keep it simple, we will just ignore it and not bother with TryProcEquippedItems for this particular impact.
            }
        }
    }

    leave_world(w, world_object);

    if let Some(o) = w.objects.get_mut(world_object) {
        o.wo.world_object.hit_msg = true;
    }

    w.objects.remove(world_object);
}

/// The projectile hit the environment (not while it is still entering the world): it leaves the
/// world, and the shooter hears of it (a player's chat line; a monster's own counter).
// ACE: ProjectileCollisionHelper.OnCollideEnvironment
pub fn on_collide_environment(w: &mut World, world_object: ObjectGuid) {
    if !physics_obj_is_active(w, world_object) {
        return;
    }

    // do not send 'Your missile attack hit the environment' messages to player,
    // if projectile is still in the process of spawning into world.
    if physics_obj_entering_world(w, world_object) {
        return;
    }

    //Console.WriteLine($"Projectile.OnCollideEnvironment({WorldObject.Name} - {WorldObject.Guid})");

    leave_world(w, world_object);

    let source = links(w, world_object)
        .source
        .filter(|&g| w.objects.get(g).is_some());
    if let Some(player) = source.filter(|&g| object(w, g).is_player()) {
        let msg = game_message_system_chat(
            "Your missile attack hit the environment.",
            ChatMessageType::Broadcast,
        );
        player_combat::send(w, player, msg);
    } else if let Some(creature) = source.filter(|&g| object(w, g).is_creature()) {
        monster_missile::monster_projectile_on_collide_environment(w, creature);
    }

    if let Some(o) = w.objects.get_mut(world_object) {
        o.wo.world_object.hit_msg = true;
    }

    w.objects.remove(world_object);
}
