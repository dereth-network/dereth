// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Melee.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Melee.cs`.
//!
//! The player melee attack: the targeted attack request, the MoveTo/TurnTo into range, the swing
//! (animation, stamina, one damage event per attack frame), the power bar refill and the repeat
//! attacks, and the cancel.
//!
//! ACE's closures capture the target and weapon objects; here they capture guids,
//! and a target gone from the store reads as ACE's destroyed (dead, unreachable) creature.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dereth_assets::CombatManeuverTable;
use empyrean_entity::enums::{
    AttackHeight, AttackType, CharacterOption, CombatMode, MotionCommand, MotionFlags,
    MotionStance, PowerAccuracy, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::attack_queue::AttackQueue;
use crate::entity::landblock;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_attack_done::game_event_attack_done;
use crate::network::game_event::events::game_event_combat_commence_attack::game_event_combat_commence_attack;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::motion::movement_data::Motion;
use crate::physics::{motion_table, phys_ext};
use crate::world_objects::creature_combat::{self, shim};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_weapon as weapon_mod;
use crate::world_objects::{
    creature_equipment, creature_melee, creature_vitals, monster_combat, player_combat,
    player_death, player_move, player_networking, player_tick, world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Player_Melee.cs`.
#[derive(Debug, Default)]
pub struct PlayerMeleeFields {
    /// The target this player is currently performing a melee attack on.
    // ACE: Player.MeleeTarget
    pub melee_target: Option<ObjectGuid>,
    /// `_powerLevel`: the power bar level, a value between 0-1 (read through [`power_level`]).
    pub power_level: f32,
    /// Built by the Player constructor (`new AttackQueue(this)`).
    // ACE: Player.AttackQueue
    pub attack_queue: AttackQueue,
    // ACE: Player.PrevMotionCommand
    pub prev_motion_command: MotionCommand,
}

// ============================================================================== helpers

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// This player's `Player_Melee` fields.
///
/// # Panics
/// When `this` is not a live player (ACE: `InvalidCastException`).
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerMeleeFields {
    &object(w, this)
        .player
        .as_ref()
        .expect("InvalidCastException: not a Player")
        .player_melee
}

/// This player's `Player_Melee` fields, mutably.
///
/// # Panics
/// When `this` is not a live player (ACE: `InvalidCastException`).
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerMeleeFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("InvalidCastException: not a Player")
        .player_melee
}

/// `creature.IsAlive` for a guid captured by a closure: a creature gone from the store is dead
/// to the attack loop.
fn creature_is_alive(w: &World, g: ObjectGuid) -> bool {
    w.objects
        .get(g)
        .is_some_and(|o| o.is_creature() && !monster_combat::is_dead(o))
}

/// `this.IsAlive` / `IsDead`.
fn is_dead(w: &World, this: ObjectGuid) -> bool {
    monster_combat::is_dead(object(w, this))
}

/// `WorldObject.IsBusy` (the `WorldObject.cs` field).
pub(crate) fn is_busy(w: &World, this: ObjectGuid) -> bool {
    object(w, this).wo.world_object.is_busy
}

/// `WorldObject.Teleporting`.
pub(crate) fn teleporting(w: &World, this: ObjectGuid) -> bool {
    object(w, this).wo.world_object.teleporting
}

/// `Player.suicideInProgress` (`Player_Death.cs`).
pub(crate) fn suicide_in_progress(w: &World, this: ObjectGuid) -> bool {
    object(w, this)
        .player
        .as_ref()
        .is_some_and(|p| p.player_death.suicide_in_progress)
}

/// `Player.PKLogout` (`Player.cs`).
pub(crate) fn pk_logout(w: &World, this: ObjectGuid) -> bool {
    object(w, this)
        .player
        .as_ref()
        .is_some_and(|p| p.player.pk_logout)
}

/// `Player.IsJumping` (`Player.cs`), which reads as not jumping without a physics body where ACE
/// would throw.
pub(crate) fn is_jumping(w: &World, this: ObjectGuid) -> bool {
    crate::world_objects::player::is_jumping(w, this)
}

/// `GetCharacterOption(option)` (`Player_Character.cs`).
pub(crate) fn get_character_option(w: &World, this: ObjectGuid, option: CharacterOption) -> bool {
    world_object_networking::shims::player_get_character_option(w, this, option)
}

/// `Creature.AttackTarget = value` (`Monster_Combat.cs`).
pub fn set_attack_target(w: &mut World, this: ObjectGuid, value: Option<ObjectGuid>) {
    monster_combat::fields_mut(w, this).attack_target = value;
}

/// `Creature.AttackTarget`.
pub fn attack_target(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    monster_combat::attack_target(w, this)
}

/// `Creature.AttackHeight = value` (`Monster_Combat.cs`).
pub fn set_attack_height(w: &mut World, this: ObjectGuid, value: Option<AttackHeight>) {
    monster_combat::fields_mut(w, this).attack_height = value;
}

/// `Creature.AttackHeight`.
pub fn attack_height(w: &World, this: ObjectGuid) -> Option<AttackHeight> {
    creature_combat::attack_height(w, this)
}

/// `CurrentMotionState.Stance`.
///
/// # Panics
/// Without a `CurrentMotionState` (ACE: `NullReferenceException`).
pub fn current_stance(w: &World, this: ObjectGuid) -> MotionStance {
    object(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .expect("System.NullReferenceException: CurrentMotionState")
        .stance
}

/// `Creature.CombatTable` (`Creature_Combat.cs`): the player constructor reads it from the portal
/// dat (`CombatTableDID`); that line is a pointer in `player.rs`, so it is read on first use here.
fn combat_table(w: &mut World, this: ObjectGuid) -> Option<Arc<CombatManeuverTable>> {
    if creature_combat::fields(object(w, this))
        .combat_table
        .is_none()
    {
        let did = object(w, this).combat_table_did()?;
        let table = w
            .dats
            .portal_dat()
            .read_from_dat::<CombatManeuverTable>(did);
        creature_combat::fields_mut(w.objects.get_mut(this).expect("ACE: this")).combat_table =
            table;
    }
    creature_combat::fields(object(w, this))
        .combat_table
        .clone()
}

/// `CombatManeuverTable.GetMotion(stance, attackHeight, attackType, prevMotion)` (ACE.DatLoader,
/// shared): the maneuvers for the stance, height and type in table order, or `[Invalid]`.
fn combat_table_get_motion(
    table: &CombatManeuverTable,
    stance: MotionStance,
    attack_height: AttackHeight,
    attack_type: AttackType,
) -> Vec<MotionCommand> {
    let maneuvers: Vec<MotionCommand> = table
        .maneuvers
        .iter()
        .filter(|m| {
            m.style == stance.0
                && m.attack_height == attack_height.0.cast_unsigned()
                && m.attack_type == attack_type.0.cast_unsigned()
        })
        .map(|m| MotionCommand(m.motion))
        .collect();

    if maneuvers.is_empty() {
        return vec![MotionCommand::Invalid];
    }

    // if the CMT contains > 1 entries for this lookup, return both
    // the code determines which motion to use based on the power bar
    maneuvers
}

/// `TryProcEquippedItems(attacker, target, selfTarget, weapon)` (`WorldObject_Combat.cs`), called
/// on the player itself (`this` is `attacker`).
fn try_proc_equipped_items(
    w: &mut World,
    attacker: ObjectGuid,
    target: ObjectGuid,
    self_target: bool,
    weapon: Option<ObjectGuid>,
) {
    crate::world_objects::world_object_combat::try_proc_equipped_items(
        w,
        attacker,
        attacker,
        target,
        self_target,
        weapon,
    );
}

/// `Session.Network.EnqueueSend(new GameEventAttackDone(Session, error))`.
fn send_attack_done(w: &mut World, this: ObjectGuid, error: WeenieError) {
    let s = player_combat::session(w, this);
    let msg = game_event_attack_done(session_data(w, s), error);
    enqueue_send(w, s, msg);
}

// ============================================================================== members

/// The power bar level, a value between 0-1 (0 while exhausted).
// ACE: Player.PowerLevel
#[must_use]
pub fn power_level(w: &World, this: ObjectGuid) -> f32 {
    if crate::world_objects::creature::is_exhausted(w.objects.get(this).expect("ACE: this is null"))
    {
        0.0
    } else {
        fields(w, this).power_level
    }
}

// ACE: Player.PowerLevel
pub fn set_power_level(w: &mut World, this: ObjectGuid, value: f32) {
    fields_mut(w, this).power_level = value;
}

/// The power bar's third: Low below 0.33, Medium below 0.66, else High.
// ACE: Player.GetPowerRange
#[must_use]
pub fn get_power_range(w: &World, this: ObjectGuid) -> PowerAccuracy {
    let power_level = power_level(w, this);
    if power_level < 0.33 {
        PowerAccuracy::Low
    } else if power_level < 0.66 {
        PowerAccuracy::Medium
    } else {
        PowerAccuracy::High
    }
}

/// Called when a player first initiates a melee attack (game action 0x0008): the checks, the
/// power bar queue, then the attack once the bar has refilled.
// ACE: Player.HandleActionTargetedMeleeAttack
pub fn handle_action_targeted_melee_attack(
    w: &mut World,
    this: ObjectGuid,
    target_guid: u32,
    attack_height: u32,
    power_level: f32,
) {
    //log.Info($"-");

    if creature_combat::combat_mode(w, this) != CombatMode::Melee {
        log::warn!(
            "{}.HandleActionTargetedMeleeAttack({target_guid:08X}, {attack_height}, {power_level}) - CombatMode mismatch {}, LastCombatMode {}",
            shim::name(w, this),
            creature_combat::combat_mode(w, this).to_dotnet_string(),
            player_combat::fields(w, this).last_combat_mode.to_dotnet_string()
        );

        if player_combat::fields(w, this).last_combat_mode == CombatMode::Melee {
            creature_combat::set_combat_mode_field(w, this, CombatMode::Melee);
        } else {
            on_attack_done(w, this, WeenieError::None);
            return;
        }
    }

    if is_busy(w, this) || teleporting(w, this) || suicide_in_progress(w, this) {
        player_networking::send_weenie_error(w, this, WeenieError::YoureTooBusy);
        on_attack_done(w, this, WeenieError::None);
        return;
    }

    if is_jumping(w, this) {
        player_networking::send_weenie_error(w, this, WeenieError::YouCantDoThatWhileInTheAir);
        on_attack_done(w, this, WeenieError::None);
        return;
    }

    if pk_logout(w, this) {
        player_networking::send_weenie_error(
            w,
            this,
            WeenieError::YouHaveBeenInPKBattleTooRecently,
        );
        on_attack_done(w, this, WeenieError::None);
        return;
    }

    // verify input
    let power_level = math_clamp_f32(power_level, 0.0, 1.0);

    set_attack_height(w, this, Some(AttackHeight(attack_height.cast_signed())));
    fields_mut(w, this).attack_queue.add(power_level);

    if fields(w, this).melee_target.is_none() {
        let p = fields_mut(w, this).attack_queue.fetch();
        set_power_level(w, this, p);
    }

    // already in melee loop?
    if player_combat::fields(w, this).attacking
        || fields(w, this)
            .melee_target
            .is_some_and(|t| creature_is_alive(w, t))
    {
        return;
    }

    // get world object for target creature
    let target = object(w, this)
        .current_landblock
        .and_then(|lb| landblock::get_object(w, lb, ObjectGuid::new(target_guid), true));

    let Some(target) = target else {
        //log.DebugFormat("{0}.HandleActionTargetedMeleeAttack({1:X8}, {2}, {3}) - couldn't find target guid", Name, targetGuid, AttackHeight, powerLevel);
        on_attack_done(w, this, WeenieError::None);
        return;
    };

    if !object(w, target).is_creature() {
        log::warn!(
            "{}.HandleActionTargetedMeleeAttack({target_guid:08X}, {}, {power_level}) - target guid not creature",
            shim::name(w, this),
            attack_height_name(w, this)
        );
        on_attack_done(w, this, WeenieError::None);
        return;
    }
    let creature_target = target;

    if !dispatch::can_damage::can_damage(w, this, creature_target) {
        let msg = format!("You cannot attack {}", shim::name(w, creature_target));
        player_networking::send_transient_error(w, this, &msg);
        on_attack_done(w, this, WeenieError::None);
        return;
    }

    if !creature_is_alive(w, creature_target) {
        on_attack_done(w, this, WeenieError::None);
        return;
    }

    //log.Info($"{Name}.HandleActionTargetedMeleeAttack({targetGuid:X8}, {attackHeight}, {powerLevel})");

    fields_mut(w, this).melee_target = Some(creature_target);
    set_attack_target(w, this, Some(creature_target));

    // reset PrevMotionCommand / DualWieldAlternate each time button is clicked
    fields_mut(w, this).prev_motion_command = MotionCommand::Invalid;
    set_dual_wield_alternate(w, this, false);

    let attack_sequence = {
        let f = player_combat::fields_mut(w, this);
        f.attack_sequence = f.attack_sequence.wrapping_add(1);
        f.attack_sequence
    };

    let next_refill_time = player_combat::fields(w, this).next_refill_time;
    if next_refill_time > w.now.utc {
        let delay_time = (next_refill_time - w.now.utc).total_seconds() as f32;

        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, f64::from(delay_time));
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            if !creature_is_alive(w, creature_target) {
                on_attack_done(w, this, WeenieError::None);
                return;
            }

            handle_action_targeted_melee_attack_inner(w, this, target, attack_sequence);
        });
        action_chain.enqueue_chain(w);
    } else {
        handle_action_targeted_melee_attack_inner(w, this, target, attack_sequence);
    }
}

/// `AttackHeight` as the log line prints it.
fn attack_height_name(w: &World, this: ObjectGuid) -> String {
    attack_height(w, this)
        .map(|h| h.to_dotnet_string())
        .unwrap_or_default()
}

/// `Creature.DualWieldAlternate = value` (`Creature_Melee.cs`).
fn set_dual_wield_alternate(w: &mut World, this: ObjectGuid, value: bool) {
    w.objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("InvalidCastException: not a Creature")
        .creature_melee
        .dual_wield_alternate = value;
}

/// `Math.Clamp(float, float, float)` (min <= max): a NaN passes through.
fn math_clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

// ACE: Player.MeleeDistance
pub const MELEE_DISTANCE: f32 = 0.6;
// ACE: Player.StickyDistance
pub const STICKY_DISTANCE: f32 = 4.0;
// ACE: Player.RepeatDistance
pub const REPEAT_DISTANCE: f32 = 16.0;

/// Within melee distance (or sticky distance with the target in sight), attack now, turning
/// first past `melee_max_angle`; otherwise charge or walk into range, then attack.
// ACE: Player.HandleActionTargetedMeleeAttack_Inner
pub fn handle_action_targeted_melee_attack_inner(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    attack_sequence: i32,
) {
    let dist = crate::world_objects::world_object_use::get_cylinder_distance(w, this, target);

    if dist <= MELEE_DISTANCE
        || dist <= STICKY_DISTANCE
            && crate::world_objects::world_object::is_melee_visible(w, this, target)
    {
        // sticky melee
        let angle = crate::world_objects::creature_navigation::get_angle(w, this, target);
        if f64::from(angle) > property_manager::get_double(w, "melee_max_angle", 0.0, true).item {
            let rotate_time = dispatch::rotate::rotate(w, this, target);

            let mut action_chain = ActionChain::new();
            action_chain.add_delay_seconds(w, f64::from(rotate_time));
            action_chain.add_action(Actor::Object(this), move |w: &mut World| {
                attack(w, this, target, attack_sequence, false);
            });
            action_chain.enqueue_chain(w);
        } else {
            attack(w, this, target, attack_sequence, false);
        }
    } else {
        // turn / move to required
        if get_character_option(w, this, CharacterOption::UseChargeAttack) {
            //log.Info($"{Name}.MoveTo({target.Name})");

            // charge attack
            dispatch::move_to::move_to(w, this, target, 0.0);
        } else {
            //log.Info($"{Name}.CreateMoveToChain({target.Name})");

            // Not ACE's (retail captures, V257): retail's melee approach carried the attack
            // chase (0x1EFF0) like the charge; ACE sends a use-move's defaults.
            player_move::create_move_to_chain_as(
                w,
                this,
                target,
                Box::new(move |w: &mut World, success: bool| {
                    if success {
                        attack(w, this, target, attack_sequence, false);
                    } else {
                        on_attack_done(w, this, WeenieError::None);
                    }
                }),
                None,
                true,
                crate::network::motion::move_to_parameters::RetailMoveTo::AttackChase,
            );
        }
    }
}

/// Called at the very end of an attack sequence (not between the repeat attacks): sends
/// `AttackDone(ActionCancelled)` so the client's power/accuracy meter resets and does not refill,
/// and clears the targets and the queue. ACE's default: `error = None` (unused).
// ACE: Player.OnAttackDone
pub fn on_attack_done(w: &mut World, this: ObjectGuid, _error: WeenieError) {
    // this function is called at the very end of an attack sequence,
    // and not between the repeat attacks

    // it sends action cancelled so the power / accuracy meter
    // is reset, and doesn't start refilling again

    // the werror for this network message is not displayed to the client --
    // if you wish to display a message, a separate GameEventWeenieError should also be sent

    send_attack_done(w, this, WeenieError::ActionCancelled);

    set_attack_target(w, this, None);
    fields_mut(w, this).melee_target = None;
    crate::world_objects::player_missile::fields_mut(w, this).missile_target = None;

    fields_mut(w, this).attack_queue.clear();

    player_combat::fields_mut(w, this).attack_cancelled = false;
}

/// Called when the client sends the 'Cancel attack' network message: a swing already under way
/// finishes and then stops; otherwise the attack ends now. Any move-to is cancelled.
// ACE: Player.HandleActionCancelAttack
pub fn handle_action_cancel_attack(w: &mut World, this: ObjectGuid, _error: WeenieError) {
    //Console.WriteLine($"{Name}.HandleActionCancelAttack()");

    if player_combat::fields(w, this).attacking {
        player_combat::fields_mut(w, this).attack_cancelled = true;
    } else if attack_target(w, this).is_some() {
        on_attack_done(w, this, WeenieError::None);
    }

    let h =
        phys_ext::physics_obj(w, this).expect("ACE: PhysicsObj is null (NullReferenceException)");
    phys_ext::cancel_moveto(w, h);
}

/// Performs a player melee attack against a target: the swing animation, the stamina cost, one
/// `DamageTarget` per attack frame (plus cleaves), then the power bar refill and, with repeat
/// attacks on and the target still in reach, the next swing. ACE's default: `subsequent = false`.
// ACE: Player.Attack
#[allow(clippy::too_many_lines, clippy::needless_range_loop)] // ACE's indexed loop
pub fn attack(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    attack_sequence: i32,
    subsequent: bool,
) {
    //log.Info($"{Name}.Attack({target.Name}, {attackSequence})");

    if player_combat::fields(w, this).attack_sequence != attack_sequence {
        return;
    }

    if creature_combat::combat_mode(w, this) != CombatMode::Melee
        || fields(w, this).melee_target.is_none()
        || is_busy(w, this)
        || is_dead(w, this)
        || suicide_in_progress(w, this)
    {
        on_attack_done(w, this, WeenieError::None);
        return;
    }

    if !creature_is_alive(w, target) {
        on_attack_done(w, this, WeenieError::None);
        return;
    }
    let creature = target;

    let (anim_length, attack_frames) = do_swing_motion(w, this, target);
    #[allow(clippy::float_cmp)] // C#'s `animLength == 0`
    if anim_length == 0.0 {
        on_attack_done(w, this, WeenieError::None);
        return;
    }

    // point of no return beyond this point -- cannot be cancelled
    player_combat::fields_mut(w, this).attacking = true;

    if subsequent {
        // client shows hourglass, until attack done is received
        // retail only did this for subsequent attacks w/ repeat attacks on
        let s = player_combat::session(w, this);
        let msg = game_event_combat_commence_attack(session_data(w, s));
        enqueue_send(w, s, msg);
    }

    let weapon = creature_equipment::get_equipped_melee_weapon(w, this, false);
    let attack_type = creature_melee::get_weapon_attack_type(w, weapon);
    let mut num_strikes = creature_melee::get_num_strikes_of(w, this, attack_type);
    let _swing_time = anim_length / num_strikes as f32 / 1.5f32;

    let mut action_chain = ActionChain::new();

    // stamina usage
    // TODO: ensure enough stamina for attack
    let power_range = get_power_range(w, this);
    let stamina_cost = player_combat::get_attack_stamina(w, this, power_range);
    let stamina = object(w, this).stamina();
    creature_vitals::update_vital_delta(w, this, stamina, stamina_cost.wrapping_neg());

    let frames = i32::try_from(attack_frames.len()).unwrap_or(i32::MAX);
    if num_strikes != frames {
        //log.Warn($"{Name}.GetAttackFrames(): MotionTableId: {MotionTableId:X8}, MotionStance: {CurrentMotionState.Stance}, Motion: {GetSwingAnimation()}, AttackFrames.Count({attackFrames.Count}) != NumStrikes({numStrikes})");
        num_strikes = frames;
    }

    // handle self-procs
    try_proc_equipped_items(w, this, this, true, weapon);

    let mut prev_time = 0.0f32;
    let target_proc: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    for i in 0..usize::try_from(num_strikes).unwrap_or(0) {
        // are there animation hooks for damage frames?
        //if (numStrikes > 1 && !TwoHandedCombat)
        //actionChain.AddDelaySeconds(swingTime);
        action_chain.add_delay_seconds(w, f64::from(attack_frames[i].0 * anim_length - prev_time));
        prev_time = attack_frames[i].0 * anim_length;

        let target_proc = Arc::clone(&target_proc);
        action_chain.add_action(Actor::Object(this), move |w: &mut World| {
            if is_dead(w, this) {
                player_combat::fields_mut(w, this).attacking = false;
                on_attack_done(w, this, WeenieError::None);
                return;
            }

            let damage_event = player_combat::damage_target(w, this, creature, weapon);

            // handle target procs
            if damage_event
                .as_ref()
                .is_some_and(crate::entity::damage_event::DamageEvent::has_damage)
                && !target_proc.load(Ordering::Relaxed)
            {
                try_proc_equipped_items(w, this, creature, false, weapon);
                target_proc.store(true, Ordering::Relaxed);
            }

            if let Some(weapon) =
                weapon.filter(|&g| w.objects.get(g).is_some_and(|o| o.is_cleaving()))
            {
                let cleave = creature_melee::get_cleave_target(w, this, creature, weapon)
                    .unwrap_or_default();

                for cleave_hit in cleave {
                    // target procs don't happen for cleaving
                    player_combat::damage_target(w, this, cleave_hit, Some(weapon));
                }
            }
        });

        //if (numStrikes == 1 || TwoHandedCombat)
        //actionChain.AddDelaySeconds(swingTime);
    }

    //actionChain.AddDelaySeconds(animLength - swingTime * numStrikes);
    action_chain.add_delay_seconds(w, f64::from(anim_length - prev_time));

    action_chain.add_action(Actor::Object(this), move |w: &mut World| {
        player_combat::fields_mut(w, this).attacking = false;

        // powerbar refill timing
        let refill_mod = if creature_melee::is_dual_wield_attack(w, this) {
            0.8f32
        } else {
            1.0f32
        }; // dual wield powerbar refills 20% faster

        let p = fields_mut(w, this).attack_queue.fetch();
        set_power_level(w, this, p);

        let next_refill_time = power_level(w, this) * refill_mod;
        let t = w.now.utc.add_seconds(f64::from(next_refill_time));
        player_combat::fields_mut(w, this).next_refill_time = t;

        let alive = creature_is_alive(w, creature);
        let in_reach = alive && {
            let dist =
                crate::world_objects::world_object_use::get_cylinder_distance(w, this, target);
            dist <= MELEE_DISTANCE
                || dist <= STICKY_DISTANCE
                    && crate::world_objects::world_object::is_melee_visible(w, this, target)
        };

        if alive
            && get_character_option(w, this, CharacterOption::AutoRepeatAttacks)
            && in_reach
            && !is_busy(w, this)
            && !player_combat::fields(w, this).attack_cancelled
        {
            // client starts refilling power meter
            send_attack_done(w, this, WeenieError::None);

            let mut next_attack = ActionChain::new();
            next_attack.add_delay_seconds(w, f64::from(next_refill_time));
            next_attack.add_action(Actor::Object(this), move |w: &mut World| {
                attack(w, this, target, attack_sequence, true);
            });
            next_attack.enqueue_chain(w);
        } else {
            on_attack_done(w, this, WeenieError::None);
        }
    });

    action_chain.enqueue_chain(w);

    if object(w, this).under_lifestone_protection() {
        player_death::lifestone_protection_dispel(w, this);
    }
}

/// Performs the player melee swing animation: its length at the weapon and Quickness speed, the
/// attack frames, and the sticky swing motion broadcast.
// ACE: Player.DoSwingMotion
pub fn do_swing_motion(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
) -> (f32, Vec<(f32, dereth_assets::AnimHook)>) {
    // get the proper animation speed for this attack,
    // based on weapon speed and player quickness
    let base_speed = creature_combat::get_anim_speed(w, this);
    let anim_speed_mod = if creature_melee::is_dual_wield_attack(w, this) {
        1.2f32
    } else {
        1.0f32
    }; // dual wield swing animation 20% faster
    let anim_speed = base_speed * anim_speed_mod;

    let swing_animation = get_swing_animation(w, this);
    let motion_table_id = object(w, this).motion_table_id();
    let stance = current_stance(w, this);
    let anim_length =
        motion_table::get_animation_length(w, motion_table_id, stance, swing_animation, anim_speed);
    //Console.WriteLine($"AnimSpeed: {animSpeed}, AnimLength: {animLength}");

    let attack_frames =
        motion_table::get_attack_frames(w, motion_table_id, stance, swing_animation);
    //Console.WriteLine($"Attack frames: {string.Join(",", attackFrames)}");

    // broadcast player swing animation to clients
    let mut motion = Motion::from_world_object(w, this, swing_animation, anim_speed);
    if property_manager::get_bool(w, "persist_movement", false, true).item {
        let current = object(w, this)
            .wo
            .world_object_properties
            .current_motion_state
            .clone()
            .expect("System.NullReferenceException: CurrentMotionState");
        motion.persist(&current);
    }
    motion.motion_state.turn_speed = 2.25;
    motion.motion_flags |= MotionFlags::StickToObject;
    motion.target_guid = target;
    w.objects
        .get_mut(this)
        .expect("ACE: this")
        .wo
        .world_object_properties
        .current_motion_state = Some(motion.clone());

    world_object_networking::enqueue_broadcast_motion(w, this, &motion, None, None);

    if player_tick::fast_tick(w, this) {
        let h = phys_ext::physics_obj(w, this)
            .expect("ACE: PhysicsObj is null (NullReferenceException)");
        phys_ext::stick_to_object(w, h, target.full());
    }

    (anim_length, attack_frames)
}

// ACE: Player.KickThreshold
pub const KICK_THRESHOLD: f32 = 0.75;

/// Returns the melee swing animation, from the weapon, the current stance, the power bar and the
/// attack height (and sets `AttackType`).
// ACE: Player.GetSwingAnimation
pub fn get_swing_animation(w: &mut World, this: ObjectGuid) -> MotionCommand {
    if creature_melee::is_dual_wield_attack(w, this) {
        let alternate = creature_melee::dual_wield_alternate(w, this);
        set_dual_wield_alternate(w, this, !alternate);
    }

    let offhand = creature_melee::is_dual_wield_attack(w, this)
        && !creature_melee::dual_wield_alternate(w, this);

    let weapon = creature_equipment::get_equipped_melee_weapon(w, this, false);

    // for reference: https://www.youtube.com/watch?v=MUaD53D9c74
    // a player with 1/2 power bar, or slightly below half
    // doing the backswing, well above 33%
    let mut subdivision = 0.33f32;

    let stance = current_stance(w, this);
    let power_level = power_level(w, this);

    let attack_type = if let Some(weapon) = weapon {
        let attack_type = weapon_mod::get_attack_type(w, weapon, stance, power_level, offhand);
        if weapon_mod::is_thrust_slash(object(w, weapon)) {
            subdivision = 0.66;
        }
        attack_type
    } else if power_level > KICK_THRESHOLD && !creature_melee::is_dual_wield_attack(w, this) {
        AttackType::Kick
    } else {
        AttackType::Punch
    };
    creature_combat::fields_mut(w.objects.get_mut(this).expect("ACE: this")).attack_type =
        attack_type;

    let attack_height = attack_height(w, this).expect(
        "System.InvalidOperationException: Nullable object must have a value (AttackHeight)",
    );
    let table = combat_table(w, this).expect("System.NullReferenceException: CombatTable");
    let motions = combat_table_get_motion(&table, stance, attack_height, attack_type);

    // higher-powered animation always in first slot ?
    let motion = if motions.len() > 1 && power_level < subdivision {
        motions[1]
    } else {
        motions[0]
    };

    fields_mut(w, this).prev_motion_command = motion;

    //Console.WriteLine($"{motion}");

    motion
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: Player.GetPowerRange
pub fn player_get_power_range(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::PowerAccuracy {
    get_power_range(w, this)
}
