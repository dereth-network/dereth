// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SpellProjectile.cs
//! Port of `Source/ACE.Server/WorldObjects/SpellProjectile.cs`.
//!
//! The spell projectile: its setup after creation (physics state, scripts, spin), the
//! projectile type of a spell, the launch physics state, the impact, and the hit: the damage roll
//! (resist, critical, absorb, elemental, slayer, resistances; war/void and life projectiles) and
//! its application (ratings, cloak, messages, death).
//!
//! Members are free functions `(w, this, ..)` over a `SpellProjectile`; `ProjectileSource`,
//! `ProjectileTarget` and `ProjectileLauncher` are the object's `ProjectileLinks` guids, read as
//! ACE's `null` when gone. Members of classes other units own are pointers at the bottom of this
//! file, named after ACE's member.
//!
//! Physics: ACE's server `PhysicsObj.ProjectileTarget` (which lets a missile pass through
//! creatures other than its target) is `phys_ext::set_projectile_target`, which the shared
//! crate's missile-ignore test reads (A11).

use empyrean_common::dotnet::{math, to_string, CsCast, Vector3};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::properties::PropertyBool;
use empyrean_entity::enums::{
    ChatMessageType, CombatMode, DamageType, PhysicsState, PlayScript, ResistanceType, Skill,
    SkillAdvancementClass, SpellCategory, SpellType, WeenieErrorWithString,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::entity::spell::Spell;
use crate::entity::spell_projectile_info::SpellProjectileInfo;
use crate::entity::spell_projectile_type::ProjectileSpellType;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::game_messages::messages::game_message_set_state::game_message_set_state;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::game_messages::messages::game_message_vector_update::game_message_vector_update;
use crate::physics::phys_ext;
use crate::world_objects::creature_combat::{self, CombatType, DebugDamageType};
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_weapon::{self as weapon, SkillOf};
use crate::world_objects::{
    creature_equipment, creature_rating, creature_vitals, world_object_magic,
    world_object_networking,
};
use crate::World;

/// Non-property fields declared in `SpellProjectile.cs`.
#[derive(Debug, Default)]
pub struct SpellProjectileFields {
    // ACE: SpellProjectile.Spell
    pub spell: Option<Spell>,
    // ACE: SpellProjectile.SpellType
    pub spell_type: ProjectileSpellType,

    // ACE: SpellProjectile.SpawnPos
    pub spawn_pos: Option<Position>,
    // ACE: SpellProjectile.DistanceToTarget
    pub distance_to_target: f32,
    // ACE: SpellProjectile.LifeProjectileDamage
    pub life_projectile_damage: u32,

    // ACE: SpellProjectile.Info
    pub info: Option<SpellProjectileInfo>,

    /// Only set to true when this spell was launched by using the built-in spell on a caster.
    // ACE: SpellProjectile.IsWeaponSpell
    pub is_weapon_spell: bool,

    /// If a spell projectile is from a proc source, make sure there is no attempt to re-proc again
    /// when the spell projectile hits.
    // ACE: SpellProjectile.FromProc
    pub from_proc: bool,

    // ACE: SpellProjectile.DebugVelocity
    pub debug_velocity: i32,

    // ACE: SpellProjectile.WorldEntryCollision
    pub world_entry_collision: bool,
}

// ------------------------------------------------------------------------------------ helpers

/// The projectile's `SpellProjectile` fields.
///
/// # Panics
/// When `this` is not a live `SpellProjectile`.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &SpellProjectileFields {
    match w.objects.get(this).map(|o| &o.kind) {
        Some(crate::world_objects::kinds::KindData::SpellProjectile(d)) => &d.spell_projectile,
        _ => panic!("ACE: {this:?} is not a SpellProjectile"),
    }
}

/// The projectile's `SpellProjectile` fields, mutably.
///
/// # Panics
/// When `this` is not a live `SpellProjectile`.
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut SpellProjectileFields {
    match w.objects.get_mut(this).map(|o| &mut o.kind) {
        Some(crate::world_objects::kinds::KindData::SpellProjectile(d)) => &mut d.spell_projectile,
        _ => panic!("ACE: {this:?} is not a SpellProjectile"),
    }
}

/// `Spell` (set by `Setup`).
fn spell_of(w: &World, this: ObjectGuid) -> Spell {
    fields(w, this)
        .spell
        .clone()
        .expect("ACE: SpellProjectile.Spell is null (NullReferenceException)")
}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .unwrap_or_else(|| panic!("ACE: {g:?} is null (NullReferenceException)"))
}

/// A guid that still resolves (ACE: a non-null reference).
fn live(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    g.filter(|&g| w.objects.get(g).is_some())
}

fn is_player(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_player)
}

fn is_creature(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_creature)
}

fn name_of(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `ProjectileSource`, `ProjectileTarget`, `ProjectileLauncher`.
fn links(
    w: &World,
    this: ObjectGuid,
) -> (Option<ObjectGuid>, Option<ObjectGuid>, Option<ObjectGuid>) {
    let l = obj(w, this).projectile.unwrap_or_default();
    (live(w, l.source), live(w, l.target), live(w, l.launcher))
}

fn session_of(w: &World, player: ObjectGuid) -> SessionId {
    crate::managers::player_manager::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let s = session_of(w, player);
    enqueue_send(w, s, msg);
}

/// `EnqueueBroadcast(msg)`.
fn enqueue_broadcast(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let _ = world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

/// `PhysicsObj` (ACE dereferences it without a check).
fn physics_obj(w: &World, this: ObjectGuid) -> dereth_physics::PhysHandle {
    obj(w, this)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)")
}

fn dotnet_bool(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

// ------------------------------------------------------------------------------------ members

/// Performs additional set up of the spell projectile based on the spell id or its derived type.
// ACE: SpellProjectile.Setup
pub fn setup(w: &mut World, this: ObjectGuid, spell: &Spell, spell_type: ProjectileSpellType) {
    {
        let f = fields_mut(w, this);
        f.spell = Some(spell.clone());
        f.spell_type = spell_type;
    }

    crate::dispatch::init_physics_obj::init_physics_obj(w, this);

    // Not ACE (retail, V314): the weenie's stored PhysicsState. Retail flies a projectile in the state
    // its weenie stores when that is a flight state (it has Missile), so AlignPath and a ring
    // piece's ScriptedCollision keep the stored bits below.
    let stored_state = obj(w, this)
        .get_property(empyrean_entity::enums::PropertyInt::PhysicsState)
        .map(PhysicsState);
    let stored_flight_state = stored_state.filter(|s| s.contains(PhysicsState::Missile));

    // Runtime changes to default state
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::ReportCollisions,
        PhysicsState::ReportCollisions,
        Some(true),
    );
    phys_ext::set_physics_state(w, this, PhysicsState::Missile, Some(true));
    // Not ACE (retail, V314): ACE turns AlignPath on for every projectile; a weenie stored in flight
    // without it (the eggs, snowballs, grenades and nanners) keeps it off, as on retail.
    let align_path = stored_flight_state.is_none_or(|s| s.contains(PhysicsState::AlignPath));
    phys_ext::set_physics_state(w, this, PhysicsState::AlignPath, Some(align_path));
    phys_ext::set_physics_state(w, this, PhysicsState::PathClipped, Some(true));
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::IgnoreCollisions,
        PhysicsState::IgnoreCollisions,
        Some(false),
    );

    // FIXME: use data here
    if spell.name() != "Rolling Death" {
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::Ethereal,
            PhysicsState::Ethereal,
            Some(false),
        );
    }

    let wcid = obj(w, this).biota.weenie_class_id;
    if spell_type == ProjectileSpellType::Bolt
        || spell_type == ProjectileSpellType::Streak
        || spell_type == ProjectileSpellType::Arc
        || spell_type == ProjectileSpellType::Volley
        || spell_type == ProjectileSpellType::Blast
        || wcid == 7276
        || wcid == 7277
        || wcid == 7279
        || wcid == 7280
    {
        let o = w.objects.get_mut(this).expect("ACE: this is null");
        o.set_default_script_id(Some(PlayScript::ProjectileCollision.0));
        o.set_default_script_intensity(Some(1.0));
    }

    // Some wall spells don't have scripted collisions
    if wcid == 7278 || wcid == 7281 || wcid == 7282 || wcid == 23144 {
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::ScriptedCollision,
            PhysicsState::ScriptedCollision,
            Some(false),
        );
    }

    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::AllowEdgeSlide,
        PhysicsState::EdgeSlide,
        Some(false),
    );

    // No need to send an ObjScale of 1.0f over the wire since that is the default value
    #[allow(clippy::float_cmp)]
    if obj(w, this).obj_scale() == Some(1.0) {
        w.objects
            .get_mut(this)
            .expect("ACE: this is null")
            .set_obj_scale(None);
    }

    if spell_type == ProjectileSpellType::Ring {
        if spell.id() == 3818 {
            let o = w.objects.get_mut(this).expect("ACE: this is null");
            o.set_default_script_id(Some(PlayScript::Explode.0));
            o.set_default_script_intensity(Some(1.0));
            phys_ext::set_physics_property_state(
                w,
                this,
                PropertyBool::ScriptedCollision,
                PhysicsState::ScriptedCollision,
                Some(true),
            );
        } else {
            // Not ACE (retail, V314): ACE turns ScriptedCollision off for every other ring piece; a
            // weenie whose stored state has it (the mana and spore clouds, the rabbit) keeps it,
            // as on retail.
            let scripted_collision =
                stored_state.is_some_and(|s| s.contains(PhysicsState::ScriptedCollision));
            phys_ext::set_physics_property_state(
                w,
                this,
                PropertyBool::ScriptedCollision,
                PhysicsState::ScriptedCollision,
                Some(scripted_collision),
            );
        }
    }

    // Projectiles with RotationSpeed get omega values and "align path" turned off which
    // creates the nice swirling animation
    let rotation_speed = obj(w, this).rotation_speed().unwrap_or(0.0);
    #[allow(clippy::float_cmp)]
    if rotation_speed != 0.0 {
        phys_ext::set_physics_state(w, this, PhysicsState::AlignPath, Some(false));
        #[allow(clippy::cast_possible_truncation)] // `(float)(Math.PI * 2 * RotationSpeed)`
        let omega_x = (std::f64::consts::PI * 2.0 * rotation_speed) as f32;
        let h = physics_obj(w, this);
        if let Some(b) = w.physics.get_mut(h) {
            b.omega_vector = dereth_primitives::Vec3::new(omega_x, 0.0, 0.0);
        }
    }
}

/// The projectile type of a spell, from its projectile count, category, tracking, spread and name.
// ACE: SpellProjectile.GetProjectileSpellType
#[must_use]
pub fn get_projectile_spell_type(w: &World, spell_id: u32) -> ProjectileSpellType {
    let spell = Spell::new(w, spell_id, true);

    if spell.wcid() == 0 {
        return ProjectileSpellType::Undef;
    }

    let category = spell.category();

    if spell.num_projectiles() == 1 {
        return if category >= SpellCategory::AcidStreak && category <= SpellCategory::SlashingStreak
            || category == SpellCategory::NetherStreak
            || category == SpellCategory::Fireworks
        {
            ProjectileSpellType::Streak
        } else if spell.non_tracking() {
            ProjectileSpellType::Arc
        } else {
            ProjectileSpellType::Bolt
        };
    }

    #[allow(clippy::float_cmp)]
    if category >= SpellCategory::AcidRing && category <= SpellCategory::SlashingRing
        || spell.spread_angle() == 360.0
    {
        return ProjectileSpellType::Ring;
    }

    if category >= SpellCategory::AcidBurst && category <= SpellCategory::SlashingBurst
        || category == SpellCategory::NetherDamageOverTimeRaising3
    {
        return ProjectileSpellType::Blast;
    }

    // 1481 - Flaming Missile Volley
    if category >= SpellCategory::AcidVolley && category <= SpellCategory::BladeVolley
        || spell.name().contains("Volley")
    {
        return ProjectileSpellType::Volley;
    }

    if category >= SpellCategory::AcidWall && category <= SpellCategory::SlashingWall {
        return ProjectileSpellType::Wall;
    }

    if category >= SpellCategory::AcidStrike && category <= SpellCategory::SlashingStrike {
        return ProjectileSpellType::Strike;
    }

    ProjectileSpellType::Undef
}

// ACE: SpellProjectile.GetProjectileScriptIntensity
#[must_use]
pub fn get_projectile_script_intensity(
    w: &World,
    this: ObjectGuid,
    spell_type: ProjectileSpellType,
) -> f32 {
    let spell = spell_of(w, this);
    get_projectile_script_intensity_of(&spell, spell_type)
}

/// The body of [`get_projectile_script_intensity`] over the projectile's spell.
#[must_use]
pub fn get_projectile_script_intensity_of(spell: &Spell, spell_type: ProjectileSpellType) -> f32 {
    if spell_type == ProjectileSpellType::Wall {
        return 0.4;
    }
    if spell_type == ProjectileSpellType::Ring {
        if spell.level() == 6 || spell.id() == 3818 {
            return 0.4;
        }
        if spell.level() == 7 {
            return 1.0;
        }
    }

    // Bolt, Blast, Volley, Streak and Arc all seem to use this scale
    // TODO: should this be based on spell level, or power of first scarab?
    // ie. can this use Spell.Formula.ScarabScale?
    match spell.level() {
        2 => 0.2,
        3 => 0.4,
        4 => 0.6,
        5 => 0.8,
        6..=8 => 1.0,
        _ => 0.0, // 1, default
    }
}

/// The projectile stops: ethereal, hidden and inactive; an explosion (and, still entering the
/// world, the launch) script; zero velocity; destroyed 5 s later.
// ACE: SpellProjectile.ProjectileImpact
pub fn projectile_impact(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.ProjectileImpact()");

    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::ReportCollisions,
        PhysicsState::ReportCollisions,
        Some(false),
    );
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::Ethereal,
        PhysicsState::Ethereal,
        Some(true),
    );
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::IgnoreCollisions,
        PhysicsState::IgnoreCollisions,
        Some(true),
    );
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::NoDraw,
        PhysicsState::NoDraw,
        Some(true),
    );
    phys_ext::set_physics_state(w, this, PhysicsState::Cloaked, Some(true));
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::LightsStatus,
        PhysicsState::LightingOn,
        Some(false),
    );

    let h = physics_obj(w, this);
    phys_ext::set_active(w, h, false);

    let spell_type = fields(w, this).spell_type;

    if phys_ext::server_record(w, h).is_some_and(|e| e.entering_world) {
        // this path should only happen if spell_projectile_ethereal = false
        let intensity = get_projectile_script_intensity(w, this, spell_type);
        enqueue_broadcast(
            w,
            this,
            game_message_script(this, PlayScript::Launch, intensity),
        );
        fields_mut(w, this).world_entry_collision = true;
    }

    let state = phys_ext::state(w, h);
    let set_state =
        game_message_set_state(w.objects.get_mut(this).expect("ACE: this is null"), state);
    enqueue_broadcast(w, this, set_state);
    let intensity = get_projectile_script_intensity(w, this, spell_type);
    enqueue_broadcast(
        w,
        this,
        game_message_script(this, PlayScript::Explode, intensity),
    );

    // this should only be needed for spell_projectile_ethereal = true,
    // however it can also fix a display issue on client in default mode,
    // where GameMessageSetState updates projectile to ethereal before it has actually collided on client,
    // causing a 'ghost' projectile to continue to sail through the target

    phys_ext::set_velocity_field(w, h, Vector3::ZERO);
    let vector_update = game_message_vector_update(w, this);
    enqueue_broadcast(w, this, vector_update);

    let mut self_destruct_chain = ActionChain::new();
    self_destruct_chain.add_delay_seconds(w, 5.0);
    self_destruct_chain.add_action(Actor::Object(this), move |w| {
        crate::world_objects::world_object::destroy(w, this, true, false)
    });
    self_destruct_chain.enqueue_chain(w);
}

/// Handles collision with scenery or other static objects that would block a projectile from
/// reaching its target, in which case the projectile should be removed with no further processing.
// ACE: SpellProjectile.OnCollideEnvironment
pub fn spell_projectile_on_collide_environment(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.OnCollideEnvironment()");

    let (source, _, _) = links(w, this);
    if let Some(player) = source.filter(|&s| is_player(w, s)) {
        if fields(w, this).info.is_some()
            && crate::world_objects::player_magic::fields(w, player).debug_spell
        {
            let msg = format!("{}.OnCollideEnvironment()", name_of(w, this));
            send(
                w,
                player,
                game_message_system_chat(&msg, ChatMessageType::Broadcast),
            );
            let info = fields(w, this)
                .info
                .clone()
                .map(|i| i.to_string(w))
                .unwrap_or_default();
            send(
                w,
                player,
                game_message_system_chat(&info, ChatMessageType::Broadcast),
            );
        }
    }

    projectile_impact(w, this);
}

/// The projectile hit `target`: the impact, then for a creature (not the caster) the PK check,
/// the damage roll and its application (or the enchantment of an enchantment projectile), the
/// target effect, the proficiency, the procs, and the combat reactions.
// ACE: SpellProjectile.OnCollideObject
pub fn spell_projectile_on_collide_object(w: &mut World, this: ObjectGuid, target: ObjectGuid) {
    //Console.WriteLine($"{Name}.OnCollideObject({target.Name})");

    let (projectile_source, projectile_target, projectile_launcher) = links(w, this);
    let player = projectile_source.filter(|&s| is_player(w, s));

    if let Some(player) = player {
        if fields(w, this).info.is_some()
            && crate::world_objects::player_magic::fields(w, player).debug_spell
        {
            let msg = format!(
                "{}.OnCollideObject({} ({}))",
                name_of(w, this),
                name_of(w, target),
                target
            );
            send(
                w,
                player,
                game_message_system_chat(&msg, ChatMessageType::Broadcast),
            );
            let info = fields(w, this)
                .info
                .clone()
                .map(|i| i.to_string(w))
                .unwrap_or_default();
            send(
                w,
                player,
                game_message_system_chat(&info, ChatMessageType::Broadcast),
            );
        }
    }

    projectile_impact(w, this);

    // ensure valid creature target
    if !is_creature(w, target) || Some(target) == projectile_source {
        return;
    }
    let creature_target = target;

    let spell = spell_of(w, this);

    if let Some(player) = player {
        crate::world_objects::player_magic::fields_mut(w, player).last_hit_spell_projectile =
            Some(spell.clone());
    }

    // ensure caster can damage target
    let source_creature = projectile_source.filter(|&s| is_creature(w, s));
    if let Some(source_creature) = source_creature {
        if !creature_can_damage(w, source_creature, creature_target) {
            return;
        }
    }

    // if player target, ensure matching PK status
    let target_player = Some(creature_target).filter(|&t| is_player(w, t));

    let pk_error =
        projectile_source.and_then(|s| check_pk_status_vs_target(w, s, creature_target, &spell));
    if let Some(pk_error) = pk_error {
        if let Some(player) = player {
            let target_name = name_of(w, creature_target);
            send_weenie_error_with_string(w, player, pk_error[0], &target_name);
        }

        if let Some(target_player) = target_player {
            let source_name = projectile_source
                .map(|s| name_of(w, s))
                .expect("ACE: ProjectileSource is null (NullReferenceException)");
            send_weenie_error_with_string(w, target_player, pk_error[1], &source_name);
        }

        return;
    }

    let mut critical = false;
    let mut crit_defended = false;
    let mut overpower = false;

    let damage = calculate_damage(
        w,
        this,
        projectile_source,
        creature_target,
        &mut critical,
        &mut crit_defended,
        &mut overpower,
    );

    if let Some(damage) = damage {
        if spell.meta_spell_type() == SpellType::EnchantmentProjectile {
            // handle EnchantmentProjectile successfully landing on target
            let source =
                projectile_source.expect("ACE: ProjectileSource is null (NullReferenceException)");
            let from_proc = fields(w, this).from_proc;
            world_object_magic::create_enchantment(
                w,
                source,
                creature_target,
                source,
                projectile_launcher,
                &spell,
                false,
                from_proc,
                false,
            );
        } else {
            damage_target(
                w,
                this,
                creature_target,
                damage,
                critical,
                crit_defended,
                overpower,
            );
        }

        // if this SpellProjectile has a TargetEffect, play it on successful hit
        // (the caster effect is skipped for a projectile hit, so a gone caster is never read)
        world_object_magic::do_spell_effects(
            w,
            this,
            &spell,
            projectile_source.unwrap_or(this),
            Some(creature_target),
            true,
        );

        if let Some(player) = player {
            proficiency_on_success_use(w, player, spell.get_magic_skill(), spell.power_mod());
        }

        // handle target procs
        // note that for untargeted multi-projectile spells,
        // ProjectileTarget will be null here, so procs will not apply

        // TODO: instead of ProjectileLauncher is Caster, perhaps a SpellProjectile.CanProc bool that defaults to true,
        // but is set to false if the source of a spell is from a proc, to prevent multi procs?

        if let Some(source_creature) = source_creature {
            if projectile_target.is_some() && !fields(w, this).from_proc {
                // TODO figure out why cross-landblock group operations are happening here. We shouldn't need this code Mag-nus 2021-02-09
                // (LandblockManager.CurrentlyTickingLandblockGroupsMultiThreaded: the world runs
                // its landblocks on one thread, so the operation is always thread safe)

                // This can result in spell projectiles being added to either sourceCreature or creatureTargets landblock.
                world_object_try_proc_equipped_items(
                    w,
                    source_creature,
                    source_creature,
                    creature_target,
                    false,
                    projectile_launcher,
                );
            }
        }
    }

    // also called on resist
    if let Some(player) = player {
        if target_player.is_none() {
            player_on_attack_monster(w, player, creature_target);
        }
    }

    if player.is_none() && target_player.is_none() {
        // check for faction combat
        if let Some(source_creature) = source_creature {
            if creature_combat::allow_faction_combat(w, source_creature, creature_target)
                || creature_combat::potential_foe(w, source_creature, creature_target)
            {
                creature_combat::monster_on_attack_monster(w, source_creature, creature_target);
            }
        }
    }
}

/// Calculates the damage for a spell projectile. Used by war magic, void magic, and life magic
/// projectiles. `None`: no damage (a dead or invincible target, lifestone protection, a resist
/// without overpower). The draws, in ACE's order: the resist (`TryResistSpell`), the critical,
/// the critical defense (a player target with the augmentation), the base damage (war/void).
// ACE: SpellProjectile.CalculateDamage
#[allow(clippy::too_many_lines)]
pub fn calculate_damage(
    w: &mut World,
    this: ObjectGuid,
    source: Option<ObjectGuid>,
    target: ObjectGuid,
    critical_hit: &mut bool,
    crit_defended: &mut bool,
    overpower: &mut bool,
) -> Option<f32> {
    let source = live(w, source);
    let source_player = source.filter(|&s| is_player(w, s));
    let target_player = Some(target).filter(|&t| is_player(w, t));

    let source = source?;
    // Any invincible target, not only a player.
    if !creature_is_alive(w, target) || obj(w, target).invincible() {
        return None;
    }

    // check lifestone protection
    if let Some(target_player) = target_player {
        if obj(w, target_player).under_lifestone_protection() {
            if let Some(source_player) = source_player {
                let msg = format!(
                    "The Lifestone's magic protects {} from the attack!",
                    name_of(w, target_player)
                );
                send(
                    w,
                    source_player,
                    game_message_system_chat(&msg, ChatMessageType::Magic),
                );
            }

            crate::world_objects::player_death::handle_lifestone_protection(w, target_player);
            return None;
        }
    }

    let spell = spell_of(w, this);

    let mut crit_damage_bonus = 0.0f32;
    let mut weapon_crit_damage_mod = 1.0f32;
    let weapon_resistance_mod: f32;
    let mut resistance_mod: f32;

    // life magic
    let mut life_magic_damage = 0.0f32;

    // war/void magic
    let mut base_damage = 0i32;
    let mut skill_bonus = 0.0f32;
    let resistance_type = creature_combat::get_resistance_type(spell.damage_type());

    let source_creature = Some(source).filter(|&s| is_creature(w, s));
    if let Some(sc) = source_creature {
        if obj(w, sc).overpower().is_some() {
            *overpower = creature_combat::get_overpower(w, sc, target);
        }
    }

    let (_, _, weapon) = links(w, this);

    let resist_source = if fields(w, this).is_weapon_spell {
        weapon
    } else {
        Some(source)
    };

    let resisted =
        world_object_magic::try_resist_spell(w, source, Some(target), &spell, resist_source, true);
    if resisted && !*overpower {
        return None;
    }

    let attack_skill = source_creature.map(|sc| {
        let skill = w
            .objects
            .get_mut(sc)
            .expect("the source")
            .get_creature_skill_school(spell.school());
        skill.map(|skill| SkillOf {
            creature: sc,
            skill,
        })
    });
    let attack_skill: Option<SkillOf> = attack_skill.flatten();

    // critical hit
    let critical_chance = match source_creature {
        Some(sc) => weapon::get_weapon_magic_crit_frequency(w, weapon, sc, attack_skill, target),
        // with no weapon ACE answers the default before reading the (null) wielder
        None if weapon.is_none() => {
            weapon::get_weapon_magic_crit_frequency(w, None, target, None, target)
        }
        None => {
            panic!("ACE: GetWeaponMagicCritFrequency with a null wielder (NullReferenceException)")
        }
    };

    if ThreadSafeRandom::next_float(0.0, 1.0) < f64::from(critical_chance) {
        if let Some(target_player) = target_player {
            let aug = obj(w, target_player).augmentation_critical_defense();
            if aug > 0 {
                let critical_defense_mod = if source_player.is_some() {
                    0.05f32
                } else {
                    0.25f32
                };
                let aug_f: f32 = aug.cs_cast();
                let critical_defense_chance = aug_f * critical_defense_mod;

                if f64::from(critical_defense_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
                    *crit_defended = true;
                }
            }
        }

        if !*crit_defended {
            *critical_hit = true;
        }
    }

    let mut absorb_mod = get_absorb_mod(w, this, target);

    let is_pvp = source_player.is_some() && target_player.is_some();

    //http://acpedia.org/wiki/Announcements_-_2014/01_-_Forces_of_Nature - Aegis is 72% effective in PvP
    let target_combat_mode = creature_combat::combat_mode(w, target);
    if is_pvp
        && (target_combat_mode == CombatMode::Melee || target_combat_mode == CombatMode::Missile)
    {
        absorb_mod = 1.0 - absorb_mod;
        absorb_mod *= 0.72;
        absorb_mod = 1.0 - absorb_mod;
    }

    if is_pvp && spell.is_harmful() {
        player_update_pk_timers(w, source_player.expect("pvp"), target_player.expect("pvp"));
    }

    let elemental_damage_mod = weapon::get_caster_elemental_damage_modifier(
        w,
        weapon,
        source_creature,
        Some(target),
        spell.damage_type(),
    );

    // Possible 2x + damage bonus for the slayer property
    let slayer_mod =
        weapon::get_weapon_creature_slayer_modifier(w, weapon, source_creature, Some(target));

    let life_projectile_damage = fields(w, this).life_projectile_damage;

    // life magic projectiles: ie., martyr's hecatomb
    let final_damage: f32 = if spell.meta_spell_type() == SpellType::LifeProjectile {
        let lpd: f32 = life_projectile_damage.cs_cast();
        life_magic_damage = lpd * spell.damage_ratio();

        // could life magic projectiles crit?
        // if so, did they use the same 1.5x formula as war magic, instead of 2.0x?
        if *critical_hit {
            // verify: CriticalMultiplier only applied to the additional crit damage,
            // whereas CD/CDR applied to the total damage (base damage + additional crit damage)
            weapon_crit_damage_mod = weapon::get_weapon_crit_damage_mod(
                w,
                weapon,
                source_creature,
                attack_skill,
                target,
            );

            crit_damage_bonus = life_magic_damage * 0.5 * weapon_crit_damage_mod;
        }

        weapon_resistance_mod = weapon::get_weapon_resistance_modifier(
            w,
            weapon,
            source_creature,
            attack_skill,
            spell.damage_type(),
        );

        // if attacker/weapon has IgnoreMagicResist directly, do not transfer to spell projectile
        // only pass if SpellProjectile has it directly, such as 2637 - Invoking Aun Tanua

        #[allow(clippy::cast_possible_truncation)] // `(float)Math.Max(0.0f, double)`
        {
            resistance_mod = math::max(
                0.0,
                creature_get_resistance_mod(
                    w,
                    target,
                    resistance_type,
                    this,
                    weapon_resistance_mod,
                ),
            ) as f32;
        }

        (life_magic_damage + crit_damage_bonus)
            * elemental_damage_mod
            * slayer_mod
            * resistance_mod
            * absorb_mod
    }
    // war/void magic projectiles
    else {
        if *critical_hit {
            // Original:
            // http://acpedia.org/wiki/Announcements_-_2002/08_-_Atonement#Letter_to_the_Players

            // Critical Strikes: In addition to the skill-based damage bonus, each projectile spell has a 2% chance of causing a critical hit on the target and doing increased damage.
            // A magical critical hit is similar in some respects to melee critical hits (although the damage calculation is handled differently).
            // While a melee critical hit automatically does twice the maximum damage of the weapon, a magical critical hit will do an additional half the minimum damage of the spell.
            // For instance, a magical critical hit from a level 7 spell, which does 110-180 points of damage, would add an additional 55 points of damage to the spell.

            // Later updated for PvE only:

            // http://acpedia.org/wiki/Announcements_-_2004/07_-_Treaties_in_Stone#Letter_to_the_Players

            // Currently when a War Magic spell scores a critical hit, it adds a multiple of the base damage of the spell to a normal damage roll.
            // Starting in July, War Magic critical hits will instead add a multiple of the maximum damage of the spell.
            // No more crits that do less damage than non-crits!

            let (min, max): (f32, f32) =
                (spell.min_damage().cs_cast(), spell.max_damage().cs_cast());
            crit_damage_bonus = if is_pvp {
                // PvP: 50% of the MIN damage added to normal damage roll
                min * 0.5
            } else {
                // PvE: 50% of the MAX damage added to normal damage roll
                max * 0.5
            };

            // verify: CriticalMultiplier only applied to the additional crit damage,
            // whereas CD/CDR applied to the total damage (base damage + additional crit damage)
            weapon_crit_damage_mod = weapon::get_weapon_crit_damage_mod(
                w,
                weapon,
                source_creature,
                attack_skill,
                target,
            );

            crit_damage_bonus *= weapon_crit_damage_mod;
        }

        /* War Magic skill-based damage bonus
         * http://acpedia.org/wiki/Announcements_-_2002/08_-_Atonement#Letter_to_the_Players
         */
        if let Some(source_player) = source_player {
            let magic_skill = {
                let skill = w
                    .objects
                    .get_mut(source_player)
                    .expect("the source")
                    .get_creature_skill_school(spell.school())
                    .expect("ACE: GetCreatureSkill(school) is null (NullReferenceException)");
                skill.current(w, source_player)
            };

            if magic_skill > spell.power() {
                // `(magicSkill - Spell.Power) / 1000.0f`: the uint difference, then float
                let diff: f32 = (magic_skill - spell.power()).cs_cast();
                let percentage_bonus = diff / 1000.0f32;

                let min: f32 = spell.min_damage().cs_cast();
                skill_bonus = min * percentage_bonus;
            }
        }
        base_damage = ThreadSafeRandom::next(spell.min_damage(), spell.max_damage());

        // DIVERGE: in an era whose creature projectiles do half damage
        // (`EraFormulas::creature_projectiles_halved`), any source but a player (a creature or a
        // trap) does half (ClassicACE's `CalculateDamage` at its Infiltration ruleset).
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/SpellProjectile.cs
        if w.era.formulas.creature_projectiles_halved && source_player.is_none() {
            base_damage /= 2;
        }

        weapon_resistance_mod = weapon::get_weapon_resistance_modifier(
            w,
            weapon,
            source_creature,
            attack_skill,
            spell.damage_type(),
        );

        // if attacker/weapon has IgnoreMagicResist directly, do not transfer to spell projectile
        // only pass if SpellProjectile has it directly, such as 2637 - Invoking Aun Tanua

        #[allow(clippy::cast_possible_truncation)] // `(float)Math.Max(0.0f, double)`
        {
            resistance_mod = math::max(
                0.0,
                creature_get_resistance_mod(
                    w,
                    target,
                    resistance_type,
                    this,
                    weapon_resistance_mod,
                ),
            ) as f32;
        }

        if source_player.is_some()
            && target_player.is_some()
            && spell.damage_type() == DamageType::Nether
        {
            // for direct damage from void spells in pvp,
            // apply void_pvp_modifier *on top of* the player's natural resistance to nether

            // this supposedly brings the direct damage from void spells in pvp closer to retail
            #[allow(clippy::cast_possible_truncation)]
            {
                resistance_mod *=
                    property_manager::get_double(w, "void_pvp_modifier", 0.0, true).item as f32;
            }
        }

        let base: f32 = base_damage.cs_cast();
        let mut d = base + crit_damage_bonus + skill_bonus;

        d *= elemental_damage_mod * slayer_mod * resistance_mod * absorb_mod;
        d
    };

    // show debug info
    let info = ShowInfo1 {
        spell: spell.clone(),
        skill: attack_skill,
        critical_chance,
        critical_hit: *critical_hit,
        crit_defended: *crit_defended,
        overpower: *overpower,
        weapon_crit_damage_mod,
        magic_skill_bonus: skill_bonus,
        base_damage,
        crit_damage_bonus,
        elemental_damage_mod,
        slayer_mod,
        weapon_resistance_mod,
        resistance_mod,
        absorb_mod,
        life_projectile_damage: life_projectile_damage.cs_cast(),
        life_magic_damage,
        final_damage,
    };
    if let Some(sc) = source_creature {
        if creature_combat::fields(obj(w, sc))
            .debug_damage
            .has_flag(DebugDamageType::Attacker)
        {
            show_info(w, sc, &info);
        }
    }
    if creature_combat::fields(obj(w, target))
        .debug_damage
        .has_flag(DebugDamageType::Defender)
    {
        show_info(w, target, &info);
    }
    Some(final_damage)
}

// ACE: SpellProjectile.GetAbsorbMod
#[must_use]
pub fn get_absorb_mod(w: &mut World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    match creature_combat::combat_mode(w, target) {
        CombatMode::Melee => {
            // does target have shield equipped?
            let shield = creature_equipment::get_equipped_shield(w, target);
            if let Some(shield) =
                shield.filter(|&s| world_object_magic::get_absorb_magic_damage(obj(w, s)).is_some())
            {
                // DIVERGE: before the Shield skill (`EraFormulas::shields_without_skill`) a shield
                // absorbs as a missile launcher does (ClassicACE's `GetAbsorbMod` at its
                // Infiltration ruleset).
                // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/SpellProjectile.cs
                if w.era.formulas.shields_without_skill {
                    return absorb_magic(w, target, shield);
                }
                return get_shield_mod(w, this, target, shield);
            }
        }

        CombatMode::Missile => {
            let missile_launcher_or_shield =
                creature_equipment::get_equipped_missile_launcher(w, target)
                    .or_else(|| creature_equipment::get_equipped_shield(w, target));
            if let Some(item) = missile_launcher_or_shield
                .filter(|&s| world_object_magic::get_absorb_magic_damage(obj(w, s)).is_some())
            {
                return absorb_magic(w, target, item);
            }
        }

        CombatMode::Magic => {
            let caster = creature_equipment::get_equipped_wand(w, target);
            if let Some(caster) =
                caster.filter(|&s| world_object_magic::get_absorb_magic_damage(obj(w, s)).is_some())
            {
                return absorb_magic(w, target, caster);
            }
        }

        _ => {}
    }
    1.0
}

/// Calculates the amount of damage a shield absorbs from magic projectile.
// ACE: SpellProjectile.GetShieldMod
#[must_use]
pub fn get_shield_mod(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    shield: ObjectGuid,
) -> f32 {
    // is spell projectile in front of creature target,
    // within shield effectiveness area?
    let effective_angle = 180.0f32;
    let angle = crate::world_objects::creature_navigation::get_angle(w, target, this);
    if angle.abs() > effective_angle / 2.0 {
        return 1.0;
    }

    // https://asheron.fandom.com/wiki/Shield
    // The formula to determine magic absorption for shields is:
    // Reduction Percent = (cap * specMod * baseSkill * 0.003f) - (cap * specMod * 0.3f)
    // Cap = Maximum reduction
    // SpecMod = 1.0 for spec, 0.8 for trained
    // BaseSkill = 100 to 433 (above 433 base shield you always achieve the maximum %)

    let shield_skill = get_creature_skill(w, target, Skill::Shield);
    let (advancement_class, base) = {
        let o = obj(w, target);
        (shield_skill.advancement_class(o), shield_skill.base(w, o))
    };
    // ensure trained?
    if advancement_class < SkillAdvancementClass::Trained || base < 100 {
        return 1.0;
    }

    let base_skill: f32 = base.min(433).cs_cast();
    let spec_mod = if advancement_class == SkillAdvancementClass::Specialized {
        1.0f32
    } else {
        0.8f32
    };
    #[allow(clippy::cast_possible_truncation)] // `(float)(shield.GetAbsorbMagicDamage() ?? 0.0f)`
    let cap = world_object_magic::get_absorb_magic_damage(obj(w, shield)).unwrap_or(0.0) as f32;

    // speced, 100 skill = 0%
    // trained, 100 skill = 0%
    // speced, 200 skill = 30%
    // trained, 200 skill = 24%
    // speced, 300 skill = 60%
    // trained, 300 skill = 48%
    // speced, 433 skill = 100%
    // trained, 433 skill = 80%

    let reduction = (cap * spec_mod * base_skill * 0.003) - (cap * spec_mod * 0.3);

    math::min_f32(1.0, 1.0 - reduction)
}

/// Calculates the damage reduction modifier for bows and casters with 'Magic Absorbing' property.
// ACE: SpellProjectile.AbsorbMagic
#[must_use]
pub fn absorb_magic(w: &mut World, target: ObjectGuid, item: ObjectGuid) -> f32 {
    // https://asheron.fandom.com/wiki/Category:Magic_Absorbing

    // Tomes and Bows
    // The formula to determine magic absorption for Tomes and the Fetish of the Dark Idols:
    // - For a 25% maximum item: (magic absorbing %) = 25 - (0.1 * (319 - base magic defense))
    // - For a 10% maximum item: (magic absorbing %) = 10 - (0.04 * (319 - base magic defense))

    // wiki currently has what is likely a typo for the 10% formula,
    // where it has a factor of 0.4 instead of 0.04
    // with 0.4, the 10% items would not start to become effective until base magic defense 294
    // with 0.04, both formulas start to become effective at base magic defense 69

    // using an equivalent formula that produces the correct results for 10% and 25%,
    // and also produces the correct results for any %

    let Some(max_percent) = world_object_magic::get_absorb_magic_damage(obj(w, item)) else {
        return 1.0;
    };

    let base_cap: i64 = 319;
    let magic_def_base = {
        let skill = get_creature_skill(w, target, Skill::MagicDefense);
        skill.base(w, obj(w, target))
    };
    // `Math.Max(0, baseCap - magicDefBase)`: int - uint is long
    let diff = (base_cap - i64::from(magic_def_base)).max(0);

    #[allow(clippy::cast_precision_loss)]
    let percent = max_percent - max_percent * diff as f64 * f64::from(0.004f32);

    #[allow(clippy::cast_possible_truncation)]
    math::min_f32(1.0, 1.0 - percent as f32)
}

/// Called for a spell projectile to damage its target: the ratings (damage, heritage, sneak
/// attack, critical, PK), the cloak, the vital, the damage history, the messages, and death.
// ACE: SpellProjectile.DamageTarget
#[allow(clippy::too_many_lines)]
pub fn damage_target(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    damage: f32,
    critical: bool,
    crit_defended: bool,
    overpower: bool,
) {
    let mut damage = damage;
    let target_player = Some(target).filter(|&t| is_player(w, t));

    // Any invincible target, not only a player.
    if obj(w, target).invincible() || creature_is_dead(w, target) {
        return;
    }

    let (projectile_source, _, _) = links(w, this);
    let source_creature = projectile_source.filter(|&s| is_creature(w, s));
    let source_player = projectile_source.filter(|&s| is_player(w, s));

    let pk_battle = source_player.is_some() && target_player.is_some();

    let mut percent: f32;

    let mut damage_rating_mod = 1.0f32;
    let mut heritage_mod = 1.0f32;
    let mut sneak_attack_mod = 1.0f32;
    let mut crit_damage_rating_mod = 1.0f32;
    let mut pk_damage_rating_mod = 1.0f32;

    let mut damage_resist_rating_mod = 1.0f32;
    let mut crit_damage_resist_rating_mod = 1.0f32;
    let mut pk_damage_resist_rating_mod = 1.0f32;

    let mut equipped_cloak: Option<ObjectGuid> = None;

    // The vital change actually applied, which the chat lines report.
    let amount: u32;

    let spell = spell_of(w, this);

    // handle life projectiles for stamina / mana
    if spell.category() == SpellCategory::StaminaLowering {
        let stamina = obj(w, target).stamina();
        let max: f32 = stamina
            .max_value(&mut StatCtx::in_world(w, target))
            .cs_cast();
        percent = damage / max;
        let delta: i32 = math::round(f64::from(damage)).cs_cast();
        amount = creature_vitals::update_vital_delta(w, target, stamina, delta.wrapping_neg())
            .wrapping_neg()
            .cast_unsigned();
    } else if spell.category() == SpellCategory::ManaLowering {
        let mana = obj(w, target).mana();
        let max: f32 = mana.max_value(&mut StatCtx::in_world(w, target)).cs_cast();
        percent = damage / max;
        let delta: i32 = math::round(f64::from(damage)).cs_cast();
        amount = creature_vitals::update_vital_delta(w, target, mana, delta.wrapping_neg())
            .wrapping_neg()
            .cast_unsigned();
    } else {
        // for possibly applying sneak attack to magic projectiles,
        // only do this for health-damaging projectiles?
        if let Some(source_player) = source_player {
            // TODO: use target direction vs. projectile position, instead of player position
            // could sneak attack be applied to void DoTs?
            sneak_attack_mod = creature_combat::get_sneak_attack_mod(w, source_player, target);
            //Console.WriteLine("Magic sneak attack:  + sneakAttackMod);
            let wand = creature_equipment::get_equipped_wand(w, source_player);
            let heritage = wand.is_some_and(|wand| {
                crate::dispatch::get_heritage_bonus::get_heritage_bonus(w, source_player, wand)
            });
            heritage_mod = if heritage { 1.05 } else { 1.0 };
        }

        let damage_rating = source_creature.map_or(0, |sc| {
            crate::world_objects::creature_rating::get_damage_rating(w, sc)
        });
        damage_rating_mod = creature_rating::additive_combine(&[
            creature_rating::get_positive_rating_mod(damage_rating),
            heritage_mod,
            sneak_attack_mod,
        ]);

        damage_resist_rating_mod =
            crate::world_objects::creature_rating::get_damage_resist_rating_mod(
                w,
                target,
                Some(CombatType::Magic),
                true,
            );

        if critical {
            crit_damage_rating_mod =
                creature_rating::get_positive_rating_mod(source_creature.map_or(0, |sc| {
                    crate::world_objects::creature_rating::get_crit_damage_rating(w, sc)
                }));
            crit_damage_resist_rating_mod = creature_rating::get_negative_rating_mod(
                crate::world_objects::creature_rating::get_crit_damage_resist_rating(w, target),
                false,
            );

            damage_rating_mod =
                creature_rating::additive_combine(&[damage_rating_mod, crit_damage_rating_mod]);
            damage_resist_rating_mod = creature_rating::additive_combine(&[
                damage_resist_rating_mod,
                crit_damage_resist_rating_mod,
            ]);
        }

        if pk_battle {
            pk_damage_rating_mod =
                creature_rating::get_positive_rating_mod(source_creature.map_or(0, |sc| {
                    crate::world_objects::creature_rating::get_pk_damage_rating(w, sc)
                }));
            pk_damage_resist_rating_mod = creature_rating::get_negative_rating_mod(
                crate::world_objects::creature_rating::get_pk_damage_resist_rating(w, target),
                false,
            );

            damage_rating_mod =
                creature_rating::additive_combine(&[damage_rating_mod, pk_damage_rating_mod]);
            damage_resist_rating_mod = creature_rating::additive_combine(&[
                damage_resist_rating_mod,
                pk_damage_resist_rating_mod,
            ]);
        }

        damage *= damage_rating_mod * damage_resist_rating_mod;

        let health = obj(w, target).health();
        let max_health: f32 = health
            .max_value(&mut StatCtx::in_world(w, target))
            .cs_cast();
        percent = damage / max_health;

        //Console.WriteLine($"Damage rating: " + Creature.ModToRating(damageRatingMod));

        equipped_cloak = creature_equipped_cloak(w, target);

        if let Some(cloak) = equipped_cloak {
            if cloak_has_damage_proc(w, cloak) && cloak_roll_proc(w, cloak, percent) {
                let source = projectile_source
                    .expect("ACE: ProjectileSource is null (NullReferenceException)");
                let reduced_damage = cloak_get_reduced_amount(w, source, damage);

                cloak_show_message(w, target, source, damage, reduced_damage);

                damage = reduced_damage;
                let max_health: f32 = health
                    .max_value(&mut StatCtx::in_world(w, target))
                    .cs_cast();
                percent = damage / max_health;
            }
        }

        let delta: i32 = math::round(f64::from(damage)).cs_cast();
        amount = creature_vitals::update_vital_delta(w, target, health, delta.wrapping_neg())
            .wrapping_neg()
            .cast_unsigned();
        crate::entity::damage_history::add(
            w,
            target,
            projectile_source
                .expect("ACE: DamageHistory.Add(null attacker) (NullReferenceException)"),
            spell.damage_type(),
            amount,
        );

        //if (targetPlayer != null && targetPlayer.Fellowship != null)
        //targetPlayer.Fellowship.OnVitalUpdate(targetPlayer);
    }

    // Not ACE's (a fix, V272): the chat lines report the vital change actually applied,
    // not the rolled damage ACE put back here "for debugging"; in the retail captures a drain that
    // emptied the pool reported what was left (down to "drains 0"). A killing blow still sends no
    // damage line (the alive check below), as retail did.

    // show debug info
    let info = ShowInfo2 {
        heritage_mod,
        sneak_attack_mod,
        damage_rating_mod,
        damage_resist_rating_mod,
        crit_damage_rating_mod,
        crit_damage_resist_rating_mod,
        pk_damage_rating_mod,
        pk_damage_resist_rating_mod,
        damage,
    };
    if let Some(sc) = source_creature {
        if creature_combat::fields(obj(w, sc))
            .debug_damage
            .has_flag(DebugDamageType::Attacker)
        {
            show_info_ratings(w, sc, &info);
        }
    }
    if creature_combat::fields(obj(w, target))
        .debug_damage
        .has_flag(DebugDamageType::Defender)
    {
        show_info_ratings(w, target, &info);
    }

    if creature_is_alive(w, target) {
        let (verb, plural) = strings_get_attack_verb(spell.damage_type(), percent);
        let _type = spell.damage_type().to_dotnet_string().to_lowercase();

        let crit_msg = if critical { "Critical hit! " } else { "" };
        let sneak_msg = if sneak_attack_mod > 1.0 {
            "Sneak Attack! "
        } else {
            ""
        };
        let overpower_msg = if overpower { "Overpower! " } else { "" };

        let non_health = spell.category() == SpellCategory::StaminaLowering
            || spell.category() == SpellCategory::ManaLowering;

        if let Some(source_player) = source_player {
            let crit_prot = if crit_defended {
                " Your critical hit was avoided with their augmentation!"
            } else {
                ""
            };

            let target_name = name_of(w, target);
            let mut attacker_msg =
                format!("{crit_msg}{overpower_msg}{sneak_msg}You {verb} {target_name} for {amount} points with {}.{crit_prot}", spell.name());

            // could these crit / sneak attack?
            if non_health {
                let vital = if spell.category() == SpellCategory::StaminaLowering {
                    "stamina"
                } else {
                    "mana"
                };
                attacker_msg = format!(
                    "With {} you drain {amount} points of {vital} from {target_name}.",
                    spell.name()
                );
            }

            if !squelch_manager_squelches_contains(
                w,
                source_player,
                Some(target),
                ChatMessageType::Magic,
            ) {
                send(
                    w,
                    source_player,
                    game_message_system_chat(&attacker_msg, ChatMessageType::Magic),
                );
            }
        }

        if let Some(target_player) = target_player {
            let crit_prot = if crit_defended {
                " Your augmentation allows you to avoid a critical hit!"
            } else {
                ""
            };

            let source_name = projectile_source
                .map(|s| name_of(w, s))
                .expect("ACE: ProjectileSource is null (NullReferenceException)");
            let mut defender_msg =
                format!("{crit_msg}{overpower_msg}{sneak_msg}{source_name} {plural} you for {amount} points with {}.{crit_prot}", spell.name());

            if non_health {
                let vital = if spell.category() == SpellCategory::StaminaLowering {
                    "stamina"
                } else {
                    "mana"
                };
                defender_msg = format!(
                    "{source_name} casts {} and drains {amount} points of your {vital}.",
                    spell.name()
                );
            }

            if !squelch_manager_squelches_contains(
                w,
                target_player,
                projectile_source,
                ChatMessageType::Magic,
            ) {
                send(
                    w,
                    target_player,
                    game_message_system_chat(&defender_msg, ChatMessageType::Magic),
                );
            }

            if let Some(sc) = source_creature {
                crate::world_objects::player_combat::set_current_attacker(w, target_player, sc);
            }
        }

        if !non_health {
            if let Some(cloak) = equipped_cloak {
                if cloak_has_proc_spell(w, cloak) {
                    let source = projectile_source
                        .expect("ACE: ProjectileSource is null (NullReferenceException)");
                    cloak_try_proc_spell(w, target, source, cloak, percent);
                }
            }

            emote_manager_on_damage(w, target, source_player);

            if critical {
                emote_manager_on_receive_critical(w, target, source_player);
            }
        }
    } else {
        let last_damager = projectile_source.map(|s| DamageHistoryInfo::new(w, s, 0.0));
        let _ = crate::dispatch::on_death::on_death(
            w,
            target,
            last_damager,
            spell.damage_type(),
            critical,
        );
        crate::world_objects::creature_death::die(w, target);
    }
}

/// Sets the physics state for a launched projectile.
// ACE: SpellProjectile.SetProjectilePhysicsState
pub fn set_projectile_physics_state(
    w: &mut World,
    this: ObjectGuid,
    target: Option<ObjectGuid>,
    use_gravity: bool,
) {
    if use_gravity {
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::GravityStatus,
            PhysicsState::Gravity,
            Some(true),
        );
    }

    {
        let o = w.objects.get_mut(this).expect("ACE: this is null");
        o.wo.world_object_properties.current_motion_state = None;
        o.set_placement(None);
    }

    // TODO: Physics description timestamps (sequence numbers) don't seem to be getting updated

    //Console.WriteLine("SpellProjectile PhysicsState: " + PhysicsObj.State);

    let location = obj(w, this)
        .location()
        .expect("ACE: Location is null (NullReferenceException)");
    let pos = location.pos();
    let rotation = location.rotation();
    let h = physics_obj(w, this);
    if let Some(b) = w.physics.get_mut(h) {
        b.position.frame.origin = dereth_primitives::Vec3::new(pos.x, pos.y, pos.z);
        b.position.frame.rotation =
            dereth_primitives::Quat::new(rotation.w, rotation.x, rotation.y, rotation.z);
    }

    let velocity = phys_ext::velocity(w, h);
    //velocity = Vector3.Transform(velocity, Matrix4x4.Transpose(Matrix4x4.CreateFromQuaternion(rotation)));
    phys_ext::set_velocity_field(w, h, velocity);

    if let Some(target) = target {
        let target_body = w.objects.get(target).and_then(|o| o.phys);
        phys_ext::set_projectile_target(w, h, target_body);
    }

    phys_ext::set_active(w, h, true);
}

/// `ShowInfo`'s first overload's arguments (the damage roll).
struct ShowInfo1 {
    spell: Spell,
    skill: Option<SkillOf>,
    critical_chance: f32,
    critical_hit: bool,
    crit_defended: bool,
    overpower: bool,
    weapon_crit_damage_mod: f32,
    magic_skill_bonus: f32,
    base_damage: i32,
    crit_damage_bonus: f32,
    elemental_damage_mod: f32,
    slayer_mod: f32,
    weapon_resistance_mod: f32,
    resistance_mod: f32,
    absorb_mod: f32,
    life_projectile_damage: f32,
    life_magic_damage: f32,
    final_damage: f32,
}

/// The debug-damage dump of the damage roll, buffered on the observer (`/debugdamage`).
// ACE: SpellProjectile.ShowInfo
#[allow(clippy::float_cmp)]
fn show_info(w: &mut World, observed: ObjectGuid, i: &ShowInfo1) {
    let debug_damage_target = creature_combat::fields(obj(w, observed)).debug_damage_target;
    let Some(observer) =
        crate::managers::player_manager::get_online_player(w, debug_damage_target.full())
    else {
        creature_combat::fields_mut(w.objects.get_mut(observed).expect("observed")).debug_damage =
            DebugDamageType::None;
        return;
    };

    let skill = i
        .skill
        .expect("ACE: skill is null (NullReferenceException)");
    let mut info = format!("Skill: {}\n", skill.skill.skill.to_sentence());
    info += &format!("CriticalChance: {}\n", to_string(i.critical_chance));
    info += &format!("CriticalHit: {}\n", dotnet_bool(i.critical_hit));

    if i.crit_defended {
        info += &format!("CriticalDefended: {}\n", dotnet_bool(i.crit_defended));
    }

    info += &format!("Overpower: {}\n", dotnet_bool(i.overpower));

    if i.spell.meta_spell_type() == SpellType::LifeProjectile {
        // life magic projectile
        info += &format!(
            "LifeProjectileDamage: {}\n",
            to_string(i.life_projectile_damage)
        );
        info += &format!("DamageRatio: {}\n", to_string(i.spell.damage_ratio()));
        info += &format!("LifeMagicDamage: {}\n", to_string(i.life_magic_damage));
    } else {
        // war/void projectile
        let difficulty = i.spell.power().min(350);
        info += &format!("Difficulty: {difficulty}\n");

        if i.magic_skill_bonus != 0.0 {
            info += &format!("SkillBonus: {}\n", to_string(i.magic_skill_bonus));
        }

        info += &format!(
            "BaseDamageRange: {} - {}\n",
            i.spell.min_damage(),
            i.spell.max_damage()
        );
        info += &format!("BaseDamage: {}\n", i.base_damage);
        info += &format!("DamageType: {}\n", i.spell.damage_type().to_dotnet_string());
    }

    if i.weapon_crit_damage_mod != 1.0 {
        info += &format!(
            "WeaponCritDamageMod: {}\n",
            to_string(i.weapon_crit_damage_mod)
        );
    }

    if i.crit_damage_bonus != 0.0 {
        info += &format!("CritDamageBonus: {}\n", to_string(i.crit_damage_bonus));
    }

    if i.elemental_damage_mod != 1.0 {
        info += &format!(
            "ElementalDamageMod: {}\n",
            to_string(i.elemental_damage_mod)
        );
    }

    if i.slayer_mod != 1.0 {
        info += &format!("SlayerMod: {}\n", to_string(i.slayer_mod));
    }

    if i.weapon_resistance_mod != 1.0 {
        info += &format!(
            "WeaponResistanceMod: {}\n",
            to_string(i.weapon_resistance_mod)
        );
    }

    if i.resistance_mod != 1.0 {
        info += &format!("ResistanceMod: {}\n", to_string(i.resistance_mod));
    }

    if i.absorb_mod != 1.0 {
        info += &format!("AbsorbMod: {}\n", to_string(i.absorb_mod));
    }

    let _ = i.final_damage;

    //observer.Session.Network.EnqueueSend(new GameMessageSystemChat(info, ChatMessageType.Broadcast));
    let f = crate::world_objects::player_magic::fields_mut(w, observer);
    f.debug_damage_buffer = Some(f.debug_damage_buffer.take().unwrap_or_default() + &info);
}

/// `ShowInfo`'s second overload's arguments (the ratings).
struct ShowInfo2 {
    heritage_mod: f32,
    sneak_attack_mod: f32,
    damage_rating_mod: f32,
    damage_resist_rating_mod: f32,
    crit_damage_rating_mod: f32,
    crit_damage_resist_rating_mod: f32,
    pk_damage_rating_mod: f32,
    pk_damage_resist_rating_mod: f32,
    damage: f32,
}

/// The debug-damage dump of the ratings: sent with the buffered roll, which it clears.
// ACE: SpellProjectile.ShowInfo
#[allow(clippy::float_cmp)]
fn show_info_ratings(w: &mut World, observed: ObjectGuid, i: &ShowInfo2) {
    let debug_damage_target = creature_combat::fields(obj(w, observed)).debug_damage_target;
    let Some(observer) =
        crate::managers::player_manager::get_online_player(w, debug_damage_target.full())
    else {
        creature_combat::fields_mut(w.objects.get_mut(observed).expect("observed")).debug_damage =
            DebugDamageType::None;
        return;
    };
    let mut info = String::new();

    if i.heritage_mod != 1.0 {
        info += &format!("HeritageMod: {}\n", to_string(i.heritage_mod));
    }

    if i.sneak_attack_mod != 1.0 {
        info += &format!("SneakAttackMod: {}\n", to_string(i.sneak_attack_mod));
    }

    if i.crit_damage_rating_mod != 1.0 {
        info += &format!(
            "CritDamageRatingMod: {}\n",
            to_string(i.crit_damage_rating_mod)
        );
    }

    if i.pk_damage_rating_mod != 1.0 {
        info += &format!("PkDamageRatingMod: {}\n", to_string(i.pk_damage_rating_mod));
    }

    if i.damage_rating_mod != 1.0 {
        info += &format!("DamageRatingMod: {}\n", to_string(i.damage_rating_mod));
    }

    if i.crit_damage_resist_rating_mod != 1.0 {
        info += &format!(
            "CritDamageResistRatingMod: {}\n",
            to_string(i.crit_damage_resist_rating_mod)
        );
    }

    if i.pk_damage_resist_rating_mod != 1.0 {
        info += &format!(
            "PkDamageResistRatingMod: {}\n",
            to_string(i.pk_damage_resist_rating_mod)
        );
    }

    if i.damage_resist_rating_mod != 1.0 {
        info += &format!(
            "DamageResistRatingMod: {}\n",
            to_string(i.damage_resist_rating_mod)
        );
    }

    info += &format!("Final damage: {}", to_string(i.damage));

    let buffer = crate::world_objects::player_magic::fields_mut(w, observer)
        .debug_damage_buffer
        .take()
        .unwrap_or_default();
    send(
        w,
        observer,
        game_message_system_chat(&(buffer + &info), ChatMessageType::Broadcast),
    );
}

// ---- constructors and SetEphemeralValues ----

/// `new SpellProjectile(weenie, guid)` / `new SpellProjectile(biota)`: the `WorldObject` constructor, then
/// SpellProjectile's `SetEphemeralValues`.
// ACE: SpellProjectile.SpellProjectile
pub fn spell_projectile_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    spell_projectile_set_ephemeral_values(o, env);
}

// ACE: SpellProjectile.SetEphemeralValues
fn spell_projectile_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    // Override weenie description defaults
    o.set_valid_locations(None);
    o.set_default_script_id(None);
}

// ------------------------------------------------------------------ pointers (other units' members)

/// `Creature.IsAlive` (`Creature_Vitals.cs`): `Health.Current > 0`.
fn creature_is_alive(w: &World, this: ObjectGuid) -> bool {
    let o = obj(w, this);
    o.health().current(o) > 0
}

/// `Creature.IsDead`: `Health.Current <= 0`.
fn creature_is_dead(w: &World, this: ObjectGuid) -> bool {
    !creature_is_alive(w, this)
}

/// `GetCreatureSkill(skill)` (added if absent).
fn get_creature_skill(w: &mut World, this: ObjectGuid, skill: Skill) -> CreatureSkill {
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .get_creature_skill(skill, true)
        .expect("the skill was added")
}

/// `target.GetResistanceMod(resistanceType, this, null, weaponResistanceMod)`
/// (`Creature_Properties.cs`).
fn creature_get_resistance_mod(
    w: &mut World,
    target: ObjectGuid,
    resistance: ResistanceType,
    attacker: ObjectGuid,
    weapon_resistance_mod: f32,
) -> f64 {
    crate::world_objects::creature_properties::get_resistance_mod(
        w,
        target,
        resistance,
        Some(attacker),
        None,
        weapon_resistance_mod,
    )
}

/// `CheckPKStatusVsTarget(target, spell)` (the virtual, `WorldObject_Combat.cs`/`Player_Combat.cs`).
fn check_pk_status_vs_target(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    _spell: &Spell,
) -> Option<Vec<WeenieErrorWithString>> {
    crate::dispatch::check_pk_status_vs_target::check_pk_status_vs_target(w, this, target, ())
}

/// `Session.Network.EnqueueSend(new GameEventWeenieErrorWithString(Session, error, str))`.
fn send_weenie_error_with_string(
    w: &mut World,
    player: ObjectGuid,
    error: WeenieErrorWithString,
    str: &str,
) {
    let s = session_of(w, player);
    let msg = game_event_weenie_error_with_string(session_data(w, s), error, str);
    enqueue_send(w, s, msg);
}

/// `Proficiency.OnSuccessUse(player, player.GetCreatureSkill(skill), difficulty)`.
fn proficiency_on_success_use(w: &mut World, player: ObjectGuid, skill: Skill, difficulty: u32) {
    let skill = crate::entity::proficiency::get_creature_skill(w, player, skill);
    crate::entity::proficiency::on_success_use(w, player, skill, difficulty);
}

/// `TryProcEquippedItems(attacker, target, selfTarget, weapon)` (`WorldObject_Combat.cs`).
fn world_object_try_proc_equipped_items(
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

/// `Player.OnAttackMonster(monster)` (`Player_Monster.cs`).
fn player_on_attack_monster(w: &mut World, this: ObjectGuid, monster: ObjectGuid) {
    crate::world_objects::player_monster::on_attack_monster(w, this, Some(monster));
}

/// `Player.UpdatePKTimers(attacker, defender)` (`Player_Combat.cs`).
fn player_update_pk_timers(w: &mut World, attacker: ObjectGuid, defender: ObjectGuid) {
    crate::world_objects::player_combat::update_pk_timers(w, attacker, defender);
}

/// `Strings.GetAttackVerb(damageType, percent, ref verb, ref plural)`: the monster-combat
/// stand-in, `("hit", "hits")`.
fn strings_get_attack_verb(damage_type: DamageType, percent: f32) -> (String, String) {
    crate::world_objects::monster_combat::shim::strings_get_attack_verb(damage_type, percent)
}

/// `SquelchManager.Squelches.Contains(source, msgType)`.
fn squelch_manager_squelches_contains(
    w: &World,
    this: ObjectGuid,
    source: Option<ObjectGuid>,
    msg_type: ChatMessageType,
) -> bool {
    crate::world_objects::managers::squelch_manager::squelches_contains(w, this, source, msg_type)
}

/// `Creature.EquippedCloak` (`Creature_Combat.cs`).
fn creature_equipped_cloak(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::creature_combat::equipped_cloak(w, this)
}

/// `Cloak.HasDamageProc(cloak)`.
fn cloak_has_damage_proc(w: &World, cloak: ObjectGuid) -> bool {
    crate::entity::cloak::has_damage_proc(w.objects.get(cloak))
}

/// `Cloak.RollProc(cloak, percent)`.
fn cloak_roll_proc(w: &mut World, cloak: ObjectGuid, percent: f32) -> bool {
    crate::entity::cloak::roll_proc(w, cloak, percent)
}

/// `Cloak.GetReducedAmount(source, float damage)`.
fn cloak_get_reduced_amount(w: &World, source: ObjectGuid, damage: f32) -> f32 {
    crate::entity::cloak::get_reduced_amount_float(w, Some(source), damage)
}

/// `Cloak.ShowMessage(defender, attacker, float damage, float reduced)`.
fn cloak_show_message(
    w: &mut World,
    defender: ObjectGuid,
    attacker: ObjectGuid,
    damage: f32,
    reduced: f32,
) {
    crate::entity::cloak::show_message_float(w, defender, Some(attacker), damage, reduced);
}

/// `Cloak.HasProcSpell(cloak)`.
fn cloak_has_proc_spell(w: &World, cloak: ObjectGuid) -> bool {
    crate::entity::cloak::has_proc_spell(w.objects.get(cloak))
}

/// `Cloak.TryProcSpell(defender, attacker, cloak, pct)`.
fn cloak_try_proc_spell(
    w: &mut World,
    defender: ObjectGuid,
    attacker: ObjectGuid,
    cloak: ObjectGuid,
    pct: f32,
) {
    crate::entity::cloak::try_proc_spell(w, defender, Some(attacker), Some(cloak), pct);
}

/// `target.EmoteManager.OnDamage(attacker)` (`EmoteManager.cs`).
fn emote_manager_on_damage(w: &mut World, this: ObjectGuid, attacker: Option<ObjectGuid>) {
    crate::world_objects::managers::emote_manager::on_damage(w, this, attacker);
}

/// `target.EmoteManager.OnReceiveCritical(attacker)` (`EmoteManager.cs`).
fn emote_manager_on_receive_critical(
    w: &mut World,
    this: ObjectGuid,
    attacker: Option<ObjectGuid>,
) {
    crate::world_objects::managers::emote_manager::on_receive_critical(w, this, attacker);
}

/// `CanDamage(target)` (the virtual, `Creature_Combat.cs`; the Player override in
/// `Player_Combat.cs`).
fn creature_can_damage(w: &World, this: ObjectGuid, target: ObjectGuid) -> bool {
    crate::dispatch::can_damage::can_damage(w, this, target)
}
