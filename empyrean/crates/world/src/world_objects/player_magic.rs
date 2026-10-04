// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Magic.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Magic.cs`.
//!
//! The player's casting flow: the cast request handlers, the checks (spellbook, components,
//! target, range, indoor/outdoor), the fizzle roll, mana, the spell words, the windup and cast
//! gestures as a motion chain, the launch into `HandleCastSpell` and the recoil
//! (`FinishCast`), plus the full-physics (`FastTick`) turn and motion-done hooks.
//!
//! # Shape
//!
//! Members are free functions `(w, this, ..)`; `this` is a Player with a session (ACE dereferences
//! `Player.Session` without a check). `WorldObject` arguments are guids; a guid that no longer
//! resolves reads as ACE's `null`. ACE's overloads keep one name where Rust allows it; the others
//! carry a suffix and the same `// ACE:` anchor.
//!
//! **Pointers.** Members of classes ported elsewhere (the PK timers, `OnAttackMonster`, the
//! proficiency grant, fellowships, trades, the squelch list, `RecordCast`) are reached through the
//! functions at the bottom of this file, each named after ACE's member. A pointer that is not
//! ported calls `not_ported!` and answers ACE's value for the common case (noted per function).
//!
//! `Creature.CalculateManaUsage` is in `creature_magic.rs`.

use dereth_primitives::Position as PPosition;
use empyrean_common::dotnet::{math, CsCast, DotNetDateTime};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    ChatMessageType, CombatMode, ItemType, MagicSchool, MotionCommand, MotionStance, PlayScript,
    PlayerKillerStatus, Skill, SpellFlags, SpellType, WeenieError, WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::cast_queue::{CastQueue, CastQueueType};
pub use crate::entity::cast_spell_params::CastingPreCheckStatus;
use crate::entity::magic_state::{self, MagicState};
use crate::entity::spell::Spell;
use crate::entity::windup_params::WindupParams;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_use_done::game_event_use_done;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_hear_speech::game_message_hear_speech;
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::motion::movement_data::Motion;
use crate::physics::phys_ext;
use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::world_object::{WorldObject, LOCAL_BROADCAST_RANGE};
use crate::world_objects::{
    container, creature_combat, creature_equipment, world_object_magic, world_object_networking,
};
use crate::World;

/// `Player.TargetCategory`. TODO: get rid of this, only used for determining if TurnTo is required.
// ACE: Player.TargetCategory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetCategory {
    #[default]
    Undef,
    WorldObject,
    Wielded,
    Inventory,
    Self_,
    Fellowship,
}

/// `Player.RecordCast` (`Entity/RecordCast.cs`; only the `/recordcast` developer command enables
/// it).
pub use crate::entity::record_cast::RecordCast as RecordCastRef;

/// Non-property fields declared in `Player_Magic.cs`.
#[derive(Debug, Default)]
pub struct PlayerMagicFields {
    // ACE: Player.MagicState
    pub magic_state: MagicState,

    /// The last spell projectile launched by this player to successfully collided with a target.
    // ACE: Player.LastHitSpellProjectile
    pub last_hit_spell_projectile: Option<Spell>,

    /// Limiter for switching between war and void magic.
    // ACE: Player.LastSuccessCast_Time
    pub last_success_cast_time: f64,
    // ACE: Player.LastSuccessCast_School
    pub last_success_cast_school: MagicSchool,

    // ACE: Player.DebugSpell
    pub debug_spell: bool,

    // ACE: Player.DebugDamageBuffer
    pub debug_damage_buffer: Option<String>,

    // ACE: Player.RecordCast
    pub record_cast: RecordCastRef,

    // ACE: Player.TurnTarget
    pub turn_target: Option<ObjectGuid>,

    // ACE: Player.StartPos
    pub start_pos: Option<PPosition>,
}

// ------------------------------------------------------------------------------------ helpers

/// The player's `Player_Magic` fields.
///
/// # Panics
/// When `this` is not a live player.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &PlayerMagicFields {
    &w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .expect("ACE: this is a Player")
        .player_magic
}

/// The player's `Player_Magic` fields, mutably.
///
/// # Panics
/// When `this` is not a live player.
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut PlayerMagicFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: this is a Player")
        .player_magic
}

fn ms(w: &World, this: ObjectGuid) -> &MagicState {
    &fields(w, this).magic_state
}

fn ms_mut(w: &mut World, this: ObjectGuid) -> &mut MagicState {
    &mut fields_mut(w, this).magic_state
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

/// `wo.Name` (the virtual property).
fn name_of(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `Session`: ACE's `Session.Network` throws for a player without one.
fn session_of(w: &World, this: ObjectGuid) -> SessionId {
    crate::managers::player_manager::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let s = session_of(w, this);
    enqueue_send(w, s, msg);
}

/// `Session.Network.EnqueueSend(new GameMessageSystemChat(msg, type))`.
pub(crate) fn send_system_chat(w: &mut World, this: ObjectGuid, msg: &str, type_: ChatMessageType) {
    send(w, this, game_message_system_chat(msg, type_));
}

/// `Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, msg))`.
fn send_transient_string(w: &mut World, this: ObjectGuid, msg: &str) {
    let s = session_of(w, this);
    let m = game_event_communication_transient_string(session_data(w, s), msg);
    enqueue_send(w, s, m);
}

/// `EnqueueBroadcast(new GameMessageScript(Guid, script, speed))`.
fn broadcast_script(w: &mut World, this: ObjectGuid, script: PlayScript, speed: f32) {
    let msg = game_message_script(this, script, speed);
    let _ = world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

/// The physics body's position (`PhysicsObj.Position`).
fn physics_position(w: &World, this: ObjectGuid) -> PPosition {
    let h = obj(w, this)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    phys_ext::position(w, h).expect("ACE: PhysicsObj is null (NullReferenceException)")
}

/// `Physics.Common.Position.Distance(pos)`: the length of the global offset.
fn physics_distance(a: &PPosition, b: &PPosition) -> f32 {
    let v = dereth_physics::math::get_offset(a, b);
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

/// `PhysicsObj.MovementManager.MotionInterpreter.InterpretedState` (`CurrentStyle`, `TurnCommand`).
fn interpreted_state(w: &World, this: ObjectGuid) -> (u32, u32) {
    let h = obj(w, this)
        .phys
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    crate::physics::motion::interpreted_state(w, h)
        .map_or((0, 0), |s| (s.current_style.0, s.turn_command.0))
}

/// `PhysicsObj.StopCompletely(false)`.
fn physics_stop_completely(w: &mut World, this: ObjectGuid) {
    if let Some(h) = obj(w, this).phys {
        crate::physics::motion::stop_completely(w, h, false);
    }
}

/// `GetCreatureSkill(school)` (added if absent); ACE's `null` for a non-magic school.
fn get_creature_skill_school(
    w: &mut World,
    this: ObjectGuid,
    school: MagicSchool,
) -> Option<CreatureSkill> {
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .get_creature_skill_school(school)
}

/// `GetCreatureSkill(skill)` (added if absent).
fn get_creature_skill(w: &mut World, this: ObjectGuid, skill: Skill) -> CreatureSkill {
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .get_creature_skill(skill, true)
        .expect("the skill was added")
}

/// `GetCreatureSkill(spell.School).Current`.
fn school_skill_current(w: &mut World, this: ObjectGuid, school: MagicSchool) -> u32 {
    let skill = get_creature_skill_school(w, this, school)
        .expect("ACE: GetCreatureSkill(school) is null (NullReferenceException)");
    skill.current(w, this)
}

/// `CurrentMotionState` (`WorldObject_Properties.cs`).
fn current_motion_state(w: &World, this: ObjectGuid) -> Option<&Motion> {
    obj(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
}

/// `WorldObject.IsBusy`.
#[must_use]
pub fn is_busy(w: &World, this: ObjectGuid) -> bool {
    obj(w, this).wo.world_object.is_busy
}

/// `WorldObject.IsBusy = value`.
pub fn set_is_busy(w: &mut World, this: ObjectGuid, value: bool) {
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .wo
        .world_object
        .is_busy = value;
}

/// `spell.Formula.GetPlayerFormula(this)`.
fn get_player_formula(w: &World, this: ObjectGuid, spell: &mut Spell) {
    spell
        .formula
        .as_mut()
        .expect("ACE: Spell.Formula is null (NullReferenceException)")
        .get_player_formula(w, this);
}

// ------------------------------------------------------------------------------------ members

/// Returns the magic skill associated with the magic school for the last collided spell
/// projectile.
// ACE: Player.GetCurrentMagicSkill
#[must_use]
pub fn get_current_magic_skill(w: &World, this: ObjectGuid) -> Skill {
    let Some(last) = fields(w, this).last_hit_spell_projectile.as_ref() else {
        return Skill::WarMagic; // this should never happen, but just in case
    };

    match last.school() {
        MagicSchool::LifeMagic => Skill::LifeMagic,
        MagicSchool::CreatureEnchantment => Skill::CreatureEnchantment,
        MagicSchool::ItemEnchantment => Skill::ItemEnchantment,
        MagicSchool::VoidMagic => Skill::VoidMagic,
        _ => Skill::WarMagic, // WarMagic, default
    }
}

/// Handles player targeted casting message. `caster_item` set: casting a built-in spell from a
/// weapon.
// ACE: Player.HandleActionCastTargetedSpell
pub fn handle_action_cast_targeted_spell(
    w: &mut World,
    this: ObjectGuid,
    target_guid: u32,
    spell_id: u32,
    caster_item: Option<ObjectGuid>,
) {
    //Console.WriteLine($"{Name}.HandleActionCastTargetedSpell({targetGuid:X8}, {spellId}, {builtInSpell})");

    if creature_combat::combat_mode(w, this) != CombatMode::Magic {
        log::warn!(
            "{}.HandleActionCastTargetedSpell({:08X}, {}, {}) - CombatMode mismatch {}, LastCombatMode: {}",
            name_of(w, this),
            target_guid,
            spell_id,
            caster_item.map(|c| name_of(w, c)).unwrap_or_default(),
            creature_combat::combat_mode(w, this).to_dotnet_string(),
            player_last_combat_mode(w, this).to_dotnet_string()
        );

        if player_last_combat_mode(w, this) == CombatMode::Magic {
            creature_combat::set_combat_mode_field(w, this, CombatMode::Magic);
        } else {
            send_use_done_event(w, this, WeenieError::None);
            return;
        }
    }

    if !pre_cast_checks(w, this) {
        return;
    }

    if is_busy(w, this) && ms(w, this).can_queue {
        let s = ms_mut(w, this);
        s.cast_queue = Some(CastQueue::new(
            CastQueueType::Targeted,
            target_guid,
            spell_id,
            caster_item,
        ));
        s.can_queue = false;
        return;
    }

    if !verify_busy(w, this) {
        return;
    }

    // verify spell is contained in player's spellbook,
    // or in the weapon's spellbook in the case of built-in spells
    if !verify_spell(w, this, spell_id, caster_item) {
        send_use_done_event(w, this, WeenieError::MagicInvalidSpellType);
        return;
    }

    let (target_category, target) = get_target_category(w, this, target_guid, spell_id);

    let Some(target) = target.filter(|&t| !obj(w, t).wo.world_object.teleporting) else {
        send_use_done_event(w, this, WeenieError::TargetNotAcquired);
        return;
    };

    magic_state::on_cast_start(w, this);
    magic_state::set_windup_params(w, this, target_guid, spell_id, caster_item);

    let start_pos = physics_position(w, this);
    fields_mut(w, this).start_pos = Some(start_pos);

    if record_cast_enabled(w, this) {
        let spell = Spell::new(w, spell_id, true);
        record_cast_on_cast_targeted_spell(w, this, &spell, target);
    }

    if target_category != TargetCategory::WorldObject && target_category != TargetCategory::Wielded
    {
        if !create_player_spell(w, this, target, target_category, spell_id, caster_item) {
            magic_state::on_cast_done(w, this);
        }

        return;
    }

    // start turning
    if !crate::world_objects::player_tick::fast_tick(w, this) {
        let mut rotate_target = Some(target);
        if let Some(wielder_id) = obj(w, target).wielder_id() {
            rotate_target = current_landblock_get_object(w, this, ObjectGuid::new(wielder_id));
        }

        let rotate_time = crate::dispatch::rotate::rotate(
            w,
            this,
            rotate_target.expect("ACE: rotateTarget is null (NullReferenceException)"),
        );
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, f64::from(rotate_time));

        action_chain.add_action(Actor::Object(this), move |w| {
            // ensure target still exists
            let (target_category, target) = get_target_category(w, this, target_guid, spell_id);

            let Some(target) = target else {
                send_use_done_event(w, this, WeenieError::TargetNotAcquired);
                magic_state::on_cast_done(w, this);
                return;
            };

            if !create_player_spell(w, this, target, target_category, spell_id, caster_item) {
                magic_state::on_cast_done(w, this);
            }
        });

        action_chain.enqueue_chain(w);
    } else {
        turn_to_magic(w, this, target);
    }
}

/// The checks both cast handlers make after the combat-mode check, in ACE's order: the physics
/// stance of a full-physics player, jumping, and the PK logout. False when the cast is refused.
fn pre_cast_checks(w: &mut World, this: ObjectGuid) -> bool {
    if crate::world_objects::player_tick::fast_tick(w, this)
        && interpreted_state(w, this).0 != MotionStance::Magic.0
    {
        let (style, _) = interpreted_state(w, this);
        let current =
            current_motion_state(w, this).map(|m| (m.stance, m.motion_state.forward_command));
        log::warn!(
            "{} CombatMode: {}, CurrentMotionState: {}.{}, Physics: {}",
            name_of(w, this),
            creature_combat::combat_mode(w, this).to_dotnet_string(),
            current.map(|c| c.0.to_dotnet_string()).unwrap_or_default(),
            current.map(|c| c.1.to_dotnet_string()).unwrap_or_default(),
            MotionStance(style).to_dotnet_string()
        );
        world_object_networking::apply_physics_motion(
            w,
            this,
            &Motion::from_stance(MotionStance::Magic),
        );
        send_use_done_event(w, this, WeenieError::YoureTooBusy);
        return false;
    }

    if crate::world_objects::player::is_jumping(w, this) {
        send_use_done_event(w, this, WeenieError::YouCantDoThatWhileInTheAir);
        return false;
    }

    if player_pk_logout(w, this) {
        send_use_done_event(w, this, WeenieError::YouHaveBeenInPKBattleTooRecently);
        return false;
    }

    true
}

// ACE: Player.DoWindup
pub fn do_windup(
    w: &mut World,
    this: ObjectGuid,
    windup_params: Option<WindupParams>,
    check_angle: bool,
) {
    //Console.WriteLine($"{Name}.DoWindup()");

    let windup_params = windup_params.expect("ACE: windupParams is null (NullReferenceException)");

    // ensure target still exists
    let (target_category, target) =
        get_target_category(w, this, windup_params.target_guid, windup_params.spell_id);

    let Some(target) = target else {
        send_use_done_event(w, this, WeenieError::TargetNotAcquired);
        magic_state::on_cast_done(w, this);
        return;
    };

    if !check_angle || is_within_angle(w, this, target) {
        if !create_player_spell(
            w,
            this,
            target,
            target_category,
            windup_params.spell_id,
            windup_params.caster_item,
        ) {
            magic_state::on_cast_done(w, this);
        }
    } else {
        // restart turn if required
        if interpreted_state(w, this).1 == 0 {
            turn_to_magic(w, this, target);
        } else {
            ms_mut(w, this).pending_turn_release = true;
        }
    }
}

// ACE: Player.GetTargetCategory
pub fn get_target_category(
    w: &mut World,
    this: ObjectGuid,
    target_guid: u32,
    spell_id: u32,
) -> (TargetCategory, Option<ObjectGuid>) {
    // fellowship spell
    let spell = Spell::new(w, spell_id, true);
    if spell.is_fellowship_spell() {
        return (TargetCategory::Fellowship, Some(this));
    }

    let guid = ObjectGuid::new(target_guid);

    // direct landblock object
    let target = current_landblock_get_object(w, this, guid);

    if target.is_some() {
        return (
            if target_guid == this.full() {
                TargetCategory::Self_
            } else {
                TargetCategory::WorldObject
            },
            target,
        );
    }

    // self-wielded
    let target = creature_equipment::get_equipped_item(w, this, guid);
    if target.is_some() {
        return (TargetCategory::Inventory, target);
    }

    // inventory item
    let target = container::get_inventory_item(w, this, guid);
    if target.is_some() {
        return (TargetCategory::Inventory, target);
    }

    // other selectable wielded
    let target = obj(w, this)
        .current_landblock
        .and_then(|lb| crate::entity::landblock::get_wielded_object(w, lb, guid, true));
    if target.is_some() {
        return (TargetCategory::Wielded, target);
    }

    // known trade objects
    if let Some(trade_partner) = player_get_known_trade_obj(w, this, guid) {
        let target = creature_equipment::get_equipped_item(w, trade_partner, guid);
        if target.is_some() {
            return (TargetCategory::Wielded, target);
        }

        let target = container::get_inventory_item(w, trade_partner, guid);
        if target.is_some() {
            return (TargetCategory::Inventory, target);
        }
    }

    (TargetCategory::Undef, None)
}

/// Handles player untargeted casting message.
// ACE: Player.HandleActionMagicCastUnTargetedSpell
pub fn handle_action_magic_cast_un_targeted_spell(w: &mut World, this: ObjectGuid, spell_id: u32) {
    //Console.WriteLine($"{Name}.HandleActionCastUnTargetedSpell({spellId})");

    if creature_combat::combat_mode(w, this) != CombatMode::Magic {
        log::warn!(
            "{}.HandleActionMagicCastUnTargetedSpell({}) - CombatMode mismatch {}, LastCombatMode {}",
            name_of(w, this),
            spell_id,
            creature_combat::combat_mode(w, this).to_dotnet_string(),
            player_last_combat_mode(w, this).to_dotnet_string()
        );

        if player_last_combat_mode(w, this) == CombatMode::Magic {
            creature_combat::set_combat_mode_field(w, this, CombatMode::Magic);
        } else {
            send_use_done_event(w, this, WeenieError::None);
            return;
        }
    }

    if !pre_cast_checks(w, this) {
        return;
    }

    if is_busy(w, this) && ms(w, this).can_queue {
        let s = ms_mut(w, this);
        s.cast_queue = Some(CastQueue::new(CastQueueType::Untargeted, 0, spell_id, None));
        s.can_queue = false;
        return;
    }

    if !verify_busy(w, this) {
        return;
    }

    // verify spell is contained in player's spellbook,
    // or in the weapon's spellbook in the case of built-in spells
    if !verify_spell(w, this, spell_id, None) {
        return;
    }

    if record_cast_enabled(w, this) {
        let spell = Spell::new(w, spell_id, true);
        record_cast_on_cast_untargeted_spell(w, this, &spell);
    }

    magic_state::on_cast_start(w, this);

    let start_pos = physics_position(w, this);
    fields_mut(w, this).start_pos = Some(start_pos);

    if !create_player_spell_untargeted(w, this, spell_id) {
        magic_state::on_cast_done(w, this);
    }
}

/// Verifies spell is contained in player's spellbook, or in the weapon's spellbook in the case of
/// built-in spells (`caster_item` set).
// ACE: Player.VerifySpell
#[must_use]
pub fn verify_spell(
    w: &World,
    this: ObjectGuid,
    spell_id: u32,
    caster_item: Option<ObjectGuid>,
) -> bool {
    if caster_item.is_some() {
        is_weapon_spell(w, this, spell_id, caster_item)
    } else {
        crate::world_objects::player_spells::spell_is_known(w, this, spell_id)
    }

    // send error message?
}

/// Returns TRUE if the currently equipped casting implement has a built-in spell.
// ACE: Player.IsWeaponSpell
#[must_use]
pub fn is_weapon_spell(
    w: &World,
    this: ObjectGuid,
    spell_id: u32,
    caster_item: Option<ObjectGuid>,
) -> bool {
    let mut caster = creature_equipment::get_equipped_wand(w, this);

    if let Some(caster_item) = caster_item {
        caster = Some(caster_item);
    }

    let Some(spell_did) = live(w, caster).and_then(|c| obj(w, c).spell_did()) else {
        return false;
    };

    spell_did == spell_id
}

// ACE: Player.Windup_MaxMove
pub const WINDUP_MAX_MOVE: f32 = 6.0;
// ACE: Player.Windup_MaxMoveSq
pub const WINDUP_MAX_MOVE_SQ: f32 = WINDUP_MAX_MOVE * WINDUP_MAX_MOVE;

// ACE: Player.VerifyBusy
pub fn verify_busy(w: &mut World, this: ObjectGuid) -> bool {
    let o = obj(w, this);
    if o.wo.world_object.is_busy
        || o.wo.world_object.teleporting
        || player_suicide_in_progress(w, this)
    {
        send_use_done_event(w, this, WeenieError::YoureTooBusy);
        return false;
    }
    true
}

/// The spell, or `None` (after the error messages) when it is unknown or the player lacks its
/// components.
// ACE: Player.ValidateSpell
pub fn validate_spell(
    w: &mut World,
    this: ObjectGuid,
    spell_id: u32,
    is_weapon_spell: bool,
) -> Option<Spell> {
    let mut spell = Spell::new(w, spell_id, true);

    if spell.not_found() {
        if spell.spell_base.is_none() {
            send_transient_string(w, this, &format!("SpellId {} Invalid.", spell.id()));
            send_use_done_event(w, this, WeenieError::None);
        } else {
            send_system_chat(
                w,
                this,
                &format!("{} spell not implemented, yet!", spell.name()),
                ChatMessageType::System,
            );
            send_use_done_event(w, this, WeenieError::MagicInvalidSpellType);
        }
        return None;
    }
    if !is_weapon_spell && !has_components_for_spell(w, this, &mut spell) {
        send_use_done_event(w, this, WeenieError::YouDontHaveAllTheComponents);
        return None;
    }

    Some(spell)
}

// ACE: Player.VerifySpellTarget
pub fn verify_spell_target(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: ObjectGuid,
) -> bool {
    if is_invalid_target(w, this, spell, target) {
        send_transient_string(
            w,
            this,
            &format!("{} cannot be cast on {}.", spell.name(), name_of(w, target)),
        );
        send_use_done_event(w, this, WeenieError::None);
        return false;
    }
    true
}

/// Determines whether the target for the spell being cast is invalid.
// ACE: Player.IsInvalidTarget
pub fn is_invalid_target(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: ObjectGuid,
) -> bool {
    let target_player = is_player(w, target);
    let target_creature = is_creature(w, target);

    // ensure target is enchantable
    if !world_object_networking::shims::is_enchantable(w, target) {
        return true;
    }

    // Self targeted spells should have a target of self
    if (spell.flags() & SpellFlags::SelfTargeted) == SpellFlags::SelfTargeted && target != this {
        return true;
    }

    // Invalidate non Item Enchantment spells cast against non Creatures or Players
    if spell.school() != MagicSchool::ItemEnchantment && !target_creature {
        return true;
    }

    // Invalidate beneficial spells against Creature/Non-player targets
    if target_creature && !target_player && spell.is_beneficial() {
        return true;
    }

    // check item spells
    if !target_creature {
        if let Some(wielder_id) = obj(w, target).wielder_id() {
            let lb = obj(w, this)
                .current_landblock
                .expect("ACE: CurrentLandblock is null (NullReferenceException)");
            let parent =
                crate::entity::landblock::get_object(w, lb, ObjectGuid::new(wielder_id), true)
                    .filter(|&p| is_player(w, p));

            // Invalidate beneficial spells against monster wielded items
            if parent.is_none() && spell.is_beneficial() {
                return true;
            }

            // Invalidate harmful spells against player wielded items, depending on pk status
            if let Some(parent) = parent {
                if spell.is_harmful() && check_pk_status_vs_target(w, this, parent, spell).is_some()
                {
                    return true;
                }
            }
        }
    }

    // verify target type for item enchantment
    if spell.school() == MagicSchool::ItemEnchantment
        && !verify_non_component_target_type(w, spell, Some(target))
        && (spell.dispel_school() != MagicSchool::ItemEnchantment
            || !property_manager::get_bool(w, "item_dispel", false, true).item)
    {
        return true;
    }

    // brittlemail / lure / other negative item spells cannot be cast with player as target

    // TODO: by end of retail, players couldn't cast any negative spells on themselves
    // this feature is currently in ace for dev testing...
    if target == this && spell.is_negative_redirectable() {
        return true;
    }

    if target_creature
        && target != this
        && spell.non_component_target_type() == ItemType::Creature
        && !creature_can_damage(w, this, target)
    {
        return true;
    }

    false
}

// ACE: Player.VerifySpellRange
pub fn verify_spell_range(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    target_category: TargetCategory,
    spell: &Spell,
    caster_item: Option<ObjectGuid>,
    mut magic_skill: u32,
) -> bool {
    if target_category != TargetCategory::WorldObject && target_category != TargetCategory::Wielded
        || target == this
    {
        return true;
    }

    let mut target_loc = Some(target);
    if let Some(wielder_id) = obj(w, target).wielder_id() {
        target_loc = current_landblock_get_object(w, this, ObjectGuid::new(wielder_id));
    }

    let location = obj(w, this)
        .location()
        .expect("ACE: Location is null (NullReferenceException)");
    let target_location = target_loc
        .and_then(|t| obj(w, t).location())
        .expect("ACE: targetLoc.Location is null (NullReferenceException)");
    let distance_to = location.distance_2d(&target_location);

    if caster_item.is_none() {
        // Range uses the initial skill plus trained ranks.
        // this is much lower than base, and omits things like attribute formula + base augs + enlightenment
        let player_skill = get_creature_skill_school(w, this, spell.school())
            .expect("ACE: GetCreatureSkill(school) is null (NullReferenceException)");
        let o = obj(w, this);
        magic_skill = player_skill
            .init_level(o)
            .wrapping_add(u32::from(player_skill.ranks(o)));
    }

    let magic_skill_f: f32 = magic_skill.cs_cast();
    let max_range = math::min_f32(
        spell.base_range_constant() + magic_skill_f * spell.base_range_mod(),
        PLAYER_MAX_RADAR_RANGE_OUTDOORS,
    );

    if distance_to > max_range {
        send_use_done_event(w, this, WeenieError::MissileOutOfRange);
        return false;
    }

    // bootstrapping this function for indoor/outdoor check, since it is called both before and after windup
    let target_indoors = obj(w, target).location().map(|l| l.indoors());
    if (spell.flags() & SpellFlags::NotIndoor) == SpellFlags::NotIndoor
        && (location.indoors()
            || target_indoors.expect("ACE: target.Location is null (NullReferenceException)"))
    {
        send_use_done_event(w, this, WeenieError::YourSpellCannotBeCastInside);
        return false;
    }
    if (spell.flags() & SpellFlags::NotOutdoor) == SpellFlags::NotOutdoor
        && (!location.indoors()
            || !target_indoors.expect("ACE: target.Location is null (NullReferenceException)"))
    {
        send_use_done_event(w, this, WeenieError::YourSpellCannotBeCastOutside);
        return false;
    }
    true
}

/// The fizzle roll: one `ThreadSafeRandom.Next(0, 1)` when the skill is within 50 of the spell's
/// power, then (switching between war and void) one `Next(3, 5)` for the time limit.
// ACE: Player.GetCastingPreCheckStatus
pub fn get_casting_pre_check_status(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    magic_skill: u32,
    is_weapon_spell: bool,
) -> CastingPreCheckStatus {
    let difficulty = spell.power();

    let mut casting_pre_check_status = CastingPreCheckStatus::CastFailed;

    let difficulty_i: i32 = difficulty.cs_cast();
    if magic_skill > 0 && i64::from(magic_skill) >= i64::from(difficulty_i) - 50 {
        let chance = crate::world_objects::skill_check::get_magic_skill_chance(
            magic_skill.cs_cast(),
            difficulty_i,
        );
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);
        if chance > rng {
            casting_pre_check_status = CastingPreCheckStatus::Success;
        }
    }

    // build-in spells never fizzle
    if is_weapon_spell {
        casting_pre_check_status = CastingPreCheckStatus::Success;
    }

    // limit casting time between war and void
    let last_school = fields(w, this).last_success_cast_school;
    if spell.school() == MagicSchool::VoidMagic && last_school == MagicSchool::WarMagic
        || spell.school() == MagicSchool::WarMagic && last_school == MagicSchool::VoidMagic
    {
        // roll each time?
        let time_limit = ThreadSafeRandom::next_float(3.0, 5.0);

        if w.now.unix_time - fields(w, this).last_success_cast_time < time_limit {
            let cur_type = if spell.school() == MagicSchool::WarMagic {
                "War"
            } else {
                "Void"
            };
            let prev_type = if last_school == MagicSchool::VoidMagic {
                "Nether"
            } else {
                "Elemental"
            };

            send_system_chat(
                w,
                this,
                &format!("The {prev_type} energies permeating your blood cause this {cur_type} magic to fail."),
                ChatMessageType::Magic,
            );

            casting_pre_check_status = CastingPreCheckStatus::CastFailed;
        }
    }
    casting_pre_check_status
}

/// The mana the cast will use (5 for a fizzle), or `None` (after UseDone) when the player or the
/// caster item has too little.
// ACE: Player.CalculateManaUsage
pub fn calculate_mana_usage_player(
    w: &mut World,
    this: ObjectGuid,
    casting_pre_check_status: CastingPreCheckStatus,
    spell: &Spell,
    target: Option<ObjectGuid>,
    caster_item: Option<ObjectGuid>,
) -> Option<u32> {
    let mut mana_used = 0;
    if casting_pre_check_status == CastingPreCheckStatus::Success {
        mana_used = crate::world_objects::creature_magic::calculate_mana_usage(
            w, this, this, spell, target,
        );
    } else if casting_pre_check_status == CastingPreCheckStatus::CastFailed {
        mana_used = 5; // todo: verify with retail
    }

    let o = obj(w, this);
    let mut current_mana = o.mana().current(o);
    if let Some(caster_item) = caster_item {
        //var caster = GetEquippedWand();
        current_mana = obj(w, caster_item).item_cur_mana().unwrap_or(0).cs_cast();
    }

    if mana_used > current_mana {
        send_use_done_event(w, this, WeenieError::YouDontHaveEnoughManaToCast);
        return None;
    }

    proficiency_on_success_use(w, this, Skill::ManaConversion, spell.power_mod());

    Some(mana_used)
}

// ACE: Player.DoSpellWords
pub fn do_spell_words(w: &mut World, this: ObjectGuid, spell: &mut Spell, is_weapon_spell: bool) {
    get_player_formula(w, this, spell);

    let spell_words = spell_words(w, spell);
    if !spell_words.trim().is_empty() && !is_weapon_spell {
        let msg = game_message_hear_speech(
            &spell_words,
            &player_get_name_with_suffix(w, this),
            this.full(),
            ChatMessageType::Spellcasting,
        );
        world_object_networking::enqueue_broadcast_range(
            w,
            this,
            &msg,
            LOCAL_BROADCAST_RANGE,
            None,
        );
    }

    player_on_talk(w, this, &spell_words);
}

/// from retail captures, player animation speed for windup / first half of cast gesture
// ACE: Player.CastSpeed
pub const CAST_SPEED: f32 = 2.0;

// ACE: Player.DoWindupGestures
pub fn do_windup_gestures(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    is_weapon_spell: bool,
    cast_chain: &mut ActionChain,
) {
    if (spell.flags() & SpellFlags::FastCast) == SpellFlags::FastCast || is_weapon_spell {
        return;
    }

    let fast_tick = crate::world_objects::player_tick::fast_tick(w, this);

    if fast_tick {
        cast_chain.add_action(Actor::Object(this), move |w| {
            physics_stop_completely(w, this);

            let s = ms_mut(w, this);
            s.turn_started = false;
            s.is_turning = false;
        });
    }

    let mut _windup_time = 0.0f32;

    let windup_gestures = spell.formula_ref().windup_gestures(w);
    for &windup_gesture in &windup_gestures {
        if record_cast_enabled(w, this) {
            cast_chain.add_action(Actor::Object(this), move |w| {
                let anim_length = record_cast_animation_length(w, this, windup_gesture);
                record_cast_log(
                    w,
                    this,
                    &format!(
                        "Windup Gesture: {}, Windup Time: {anim_length}",
                        windup_gesture.to_dotnet_string()
                    ),
                );
            });
        }

        // don't mess with CurrentMotionState here?
        if !fast_tick {
            _windup_time = world_object_networking::enqueue_motion_magic(
                w,
                this,
                cast_chain,
                windup_gesture,
                CAST_SPEED,
            );
        }

        /*Console.WriteLine($"{spell.Name}");
        Console.WriteLine($"Windup Gesture: " + windupGesture);
        Console.WriteLine($"Windup time: " + windupTime);
        Console.WriteLine("-------");*/
    }

    if fast_tick {
        _windup_time = world_object_networking::enqueue_motion_action(
            w,
            this,
            cast_chain,
            &windup_gestures,
            CAST_SPEED,
            Some(MotionStance::Magic),
            false,
            true,
        );
    }
}

// ACE: Player.DoCastGesture
pub fn do_cast_gesture(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    caster_item: Option<ObjectGuid>,
    cast_chain: &mut ActionChain,
) {
    let cast_gesture = spell.formula_ref().cast_gesture(w);
    ms_mut(w, this).cast_gesture = cast_gesture;

    if let Some(caster_item) = caster_item {
        //var caster = GetEquippedWand();
        let use_user_animation = obj(w, caster_item).use_user_animation();
        if use_user_animation != MotionCommand::Invalid {
            ms_mut(w, this).cast_gesture = use_user_animation;
        }
    }

    if record_cast_enabled(w, this) {
        cast_chain.add_action(Actor::Object(this), move |w| {
            let gesture = ms(w, this).cast_gesture;
            let anim_length = record_cast_animation_length(w, this, gesture);
            record_cast_log(
                w,
                this,
                &format!(
                    "Cast Gesture: {}, Cast Time: {anim_length}",
                    gesture.to_dotnet_string()
                ),
            );
        });
    }

    cast_chain.add_action(Actor::Object(this), move |w| {
        if !ms(w, this).is_casting {
            return;
        }

        let now = w.now.utc;
        ms_mut(w, this).cast_gesture_start_time = now;

        if crate::world_objects::player_tick::fast_tick(w, this) {
            physics_stop_completely(w, this);
        }
    });

    if ms(w, this).cast_gesture == MotionCommand::Invalid {
        ms_mut(w, this).cast_gesture = MotionCommand::Ready;
    }

    let cast_gesture = ms(w, this).cast_gesture;
    let _cast_time = if crate::world_objects::player_tick::fast_tick(w, this) {
        world_object_networking::enqueue_motion(
            w,
            this,
            cast_chain,
            cast_gesture,
            CAST_SPEED,
            true,
            None,
            true,
            false,
        )
    } else {
        world_object_networking::enqueue_motion_magic(w, this, cast_chain, cast_gesture, CAST_SPEED)
    };

    //Console.WriteLine($"Cast Gesture: " + MagicState.CastGesture);
    //Console.WriteLine($"Cast time: " + castTime);
}

/// 20 from MoveToManager threshold?
// ACE: Player.MaxAngle
pub const MAX_ANGLE: f32 = 5.0;

/// The `DoCastSpell(MagicState, bool checkAngle = true)` overload: the stored cast parameters, or
/// a BadCast UseDone when there are none.
// ACE: Player.DoCastSpell
pub fn do_cast_spell_state(w: &mut World, this: ObjectGuid, check_angle: bool) {
    //Console.WriteLine("DoCastSpell");

    if !ms(w, this).is_casting {
        return;
    }

    let Some(state) = ms(w, this).cast_spell_params.clone() else {
        log::warn!("{}.DoCastSpell(): null state detected", name_of(w, this));
        log::warn!("{}", magic_state::to_string(w, this));

        // send UseDone?
        send_use_done_event(w, this, WeenieError::BadCast);

        return;
    };

    do_cast_spell(
        w,
        this,
        state.spell,
        state.caster_item,
        state.magic_skill,
        state.mana_used,
        state.target,
        state.status,
        check_angle,
    );
}

// ACE: Player.IsWithinAngle
pub fn is_within_angle(w: &mut World, this: ObjectGuid, target: ObjectGuid) -> bool {
    // TODO: investigate this more, difference for GetAngle() between ACE and ac physics engine
    let mut angle = 0.0f32;
    if target != this {
        if obj(w, target).current_landblock.is_none() {
            let found = crate::world_objects::player_inventory::find_object(
                w,
                this,
                target,
                crate::world_objects::player_inventory::SearchLocations::Everywhere,
            );

            match found.root_owner {
                None => log::error!(
                    "{}.IsWithinAngle({} ({})) - couldn't find rootOwner",
                    name_of(w, this),
                    name_of(w, target),
                    target
                ),
                Some(root_owner) if root_owner != this => {
                    angle = creature_get_angle(w, this, root_owner)
                }
                Some(_) => {}
            }
        } else {
            angle = creature_get_angle(w, this, target);
        }
    }

    //Console.WriteLine($"Angle: " + angle);
    let max_angle = property_manager::get_double(w, "spellcast_max_angle", 0.0, true).item;

    if record_cast_enabled(w, this) {
        record_cast_log(
            w,
            this,
            &format!("DoCastSpell(angle={angle} vs. {max_angle})"),
        );
    }

    f64::from(angle) <= max_angle
}

// ACE: Player.DoCastSpell
#[allow(clippy::too_many_arguments)]
pub fn do_cast_spell(
    w: &mut World,
    this: ObjectGuid,
    spell: Spell,
    caster_item: Option<ObjectGuid>,
    magic_skill: u32,
    mana_used: u32,
    target: Option<ObjectGuid>,
    casting_pre_check_status: CastingPreCheckStatus,
    check_angle: bool,
) {
    let mut target = target;
    if let Some(t) = target {
        // verify target still exists
        // (a destroyed target keeps its guid in ACE; the lookup then misses)
        let (target_category, found) = get_target_category(w, this, t.full(), spell.id());
        target = found;

        let Some(t) = target else {
            send_weenie_error(w, this, WeenieError::TargetNotAcquired);
            finish_cast(w, this);
            return;
        };

        // do second rotate, if applicable
        // TODO: investigate this more, difference for GetAngle() between ACE and ac physics engine
        if check_angle && !is_within_angle(w, this, t) {
            if !crate::world_objects::player_tick::fast_tick(w, this) {
                let rotate_time = crate::dispatch::rotate::rotate(w, this, t);

                let mut action_chain = ActionChain::new();
                action_chain.add_delay_seconds(w, f64::from(rotate_time));
                action_chain.add_action(Actor::Object(this), move |w| {
                    do_cast_spell(
                        w,
                        this,
                        spell,
                        caster_item,
                        magic_skill,
                        mana_used,
                        Some(t),
                        casting_pre_check_status,
                        false,
                    );
                });
                action_chain.enqueue_chain(w);
            } else if interpreted_state(w, this).1 == 0 {
                turn_to_magic(w, this, t);
            } else {
                ms_mut(w, this).pending_turn_release = true;
            }

            return;
        }

        // verify spell range
        if !verify_spell_range(
            w,
            this,
            t,
            target_category,
            &spell,
            caster_item,
            magic_skill,
        ) {
            finish_cast(w, this);
            return;
        }
    }

    if creature_is_dead(w, this) {
        finish_cast(w, this);
        return;
    }

    do_cast_spell_inner(
        w,
        this,
        &spell,
        caster_item,
        mana_used,
        target,
        casting_pre_check_status,
        true,
    );
}

// ACE: Player.TurnTo_Magic
pub fn turn_to_magic(w: &mut World, this: ObjectGuid, target: ObjectGuid) {
    //Console.WriteLine($"{Name}.TurnTo_Magic()");
    fields_mut(w, this).turn_target = Some(target);

    {
        let s = ms_mut(w, this);
        s.turn_started = true;
        s.is_turning = true;
    }

    if crate::world_objects::player_tick::fast_tick(w, this) {
        if property_manager::get_double(w, "spellcast_max_angle", 0.0, true).item
            > f64::from(5.0f32)
            && is_within_angle(w, this, target)
        {
            // emulate current gdle TurnTo - doesn't match retail, but some players may prefer this
            on_move_complete_magic(w, this, WeenieError::None);
            return;
        }

        // verify cast radius before every automatic TurnTo after windup
        if !verify_cast_radius(w, this) {
            return;
        }

        let stop_completely = !ms(w, this).cast_motion_done;
        //var stopCompletely = true;

        let always_turn = ms(w, this).always_turn;
        crate::world_objects::player_move2::create_turn_to_chain2(
            w,
            this,
            target,
            Box::new(|_, _| {}),
            None,
            stop_completely,
            always_turn,
        );

        ms_mut(w, this).always_turn = false;
    }
}

/// `finish_cast` defaults to true in ACE.
// ACE: Player.DoCastSpell_Inner
#[allow(clippy::too_many_arguments)]
pub fn do_cast_spell_inner(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    caster_item: Option<ObjectGuid>,
    mana_used: u32,
    target: Option<ObjectGuid>,
    mut casting_pre_check_status: CastingPreCheckStatus,
    finish_cast_: bool,
) {
    if record_cast_enabled(w, this) {
        record_cast_log(w, this, "DoCastSpell_Inner()");
    }

    if ms(w, this).cast_meter {
        let gesture = ms(w, this).cast_gesture;
        let gesture_time = record_cast_animation_length(w, this, gesture);
        let cast_time = (w.now.utc - ms(w, this).cast_gesture_start_time).total_seconds();
        #[allow(clippy::cast_possible_truncation)]
        let efficiency = 1.0f32 - cast_time as f32 / gesture_time;
        let msg = format!(
            "Cast efficiency: {}%",
            empyrean_common::dotnet::to_string(efficiency * 100.0)
        );
        send_system_chat(w, this, &msg, ChatMessageType::Broadcast);
    }

    // consume mana
    let caster = live(w, caster_item).or_else(|| creature_equipment::get_equipped_wand(w, this)); // TODO: persist this from the beginning, since this is done with delay

    let is_weapon_spell = caster_item.is_some();

    let item_caster = if is_weapon_spell { caster } else { None };

    if !is_weapon_spell {
        let mana = obj(w, this).mana();
        crate::world_objects::creature_vitals::update_vital_delta(
            w,
            this,
            mana,
            (mana_used.cast_signed()).wrapping_neg(),
        );
    } else if let Some(item_caster) = item_caster {
        let o = w
            .objects
            .get_mut(item_caster)
            .expect("ACE: itemCaster is null");
        let cur = o.item_cur_mana();
        // `itemCaster.ItemCurMana -= (int)manaUsed` (null stays null)
        o.set_item_cur_mana(cur.map(|m| m.wrapping_sub(mana_used.cast_signed())));
    } else {
        casting_pre_check_status = CastingPreCheckStatus::CastFailed;
    }

    // consume spell components
    if !is_weapon_spell {
        try_burn_components(w, this, spell);
    }

    // check windup move distance cap
    let start_pos = fields(w, this)
        .start_pos
        .expect("ACE: StartPos is null (NullReferenceException)");
    let dist = physics_distance(&start_pos, &physics_position(w, this));

    // only PKs affected by these caps?
    if dist > WINDUP_MAX_MOVE && obj(w, this).player_killer_status() != PlayerKillerStatus::NPK {
        //player.Session.Network.EnqueueSend(new GameEventWeenieError(player.Session, WeenieError.YouHaveMovedTooFar));
        send_system_chat(
            w,
            this,
            "Your movement disrupted spell casting!",
            ChatMessageType::Magic,
        );

        broadcast_script(w, this, PlayScript::Fizzle, 0.5);

        if finish_cast_ {
            finish_cast(w, this);
        }

        return;
    }

    let target = live(w, target);
    let pk_error = target.and_then(|t| check_pk_status_vs_target(w, this, t, spell));
    if pk_error.is_some() {
        casting_pre_check_status = CastingPreCheckStatus::InvalidPKStatus;
    }

    match casting_pre_check_status {
        CastingPreCheckStatus::Success => {
            if !spell.is_fellowship_spell() {
                create_player_spell_on(w, this, target, spell, is_weapon_spell);
            } else {
                let fellows = get_fellowship_targets(w, this);
                for fellow in fellows {
                    create_player_spell_on(w, this, Some(fellow), spell, is_weapon_spell);
                }
            }

            // handle self procs
            if spell.is_harmful() && target != Some(this) {
                world_object_try_proc_equipped_items(w, this, this, this, true, caster);
            }
        }

        CastingPreCheckStatus::InvalidPKStatus => {
            if spell.num_projectiles() > 0 {
                let _ = world_object_magic::handle_cast_spell(
                    w,
                    this,
                    spell,
                    target,
                    item_caster,
                    caster,
                    is_weapon_spell,
                    false,
                    false,
                );
            }
        }

        _ => {
            broadcast_script(w, this, PlayScript::Fizzle, 0.5);
            send_weenie_error(w, this, WeenieError::YourSpellFizzled);
        }
    }

    if let Some(pk_error) = pk_error {
        if spell.num_projectiles() == 0 {
            let t = target.expect("ACE: target is null (NullReferenceException)");
            let target_name = name_of(w, t);
            send_weenie_error_with_string(w, this, pk_error[0], &target_name);

            if is_player(w, t) {
                let name = name_of(w, this);
                send_weenie_error_with_string(w, t, pk_error[1], &name);
            }
        }
    }

    if finish_cast_ {
        finish_cast(w, this);
    }
}

// ACE: Player.FinishCast
pub fn finish_cast(w: &mut World, this: ObjectGuid) {
    let params = ms(w, this).cast_spell_params.clone();
    let has_windup_gestures = params
        .as_ref()
        .is_none_or(crate::entity::cast_spell_params::CastSpellParams::has_windup_gestures);
    let mut cast_gesture = ms(w, this).cast_gesture;

    let fast_tick = crate::world_objects::player_tick::fast_tick(w, this);
    if fast_tick {
        cast_gesture = if has_windup_gestures {
            current_motion_state(w, this)
                .expect("ACE: CurrentMotionState is null (NullReferenceException)")
                .motion_state
                .forward_command
        } else {
            ms(w, this).cast_gesture
        };
    }

    let self_target = !has_windup_gestures
        && params
            .expect("ACE: MagicState.CastSpellParams is null (NullReferenceException)")
            .target
            == Some(this);

    magic_state::on_cast_done(w, this);

    set_is_busy(w, this, true);

    let queue = property_manager::get_bool(w, "spellcast_recoil_queue", false, true).item;

    if queue {
        ms_mut(w, this).can_queue = true;
    }

    if fast_tick {
        let fastbuff = self_target && property_manager::get_bool(w, "fastbuff", false, true).item;

        // return to magic ready stance
        let mut action_chain = ActionChain::new();
        world_object_networking::enqueue_motion(
            w,
            this,
            &mut action_chain,
            MotionCommand::Ready,
            1.0,
            true,
            Some(cast_gesture),
            false,
            fastbuff,
        );
        action_chain.add_action(Actor::Object(this), move |w| {
            set_is_busy(w, this, false);
            send_use_done_event(w, this, WeenieError::None);

            if queue {
                handle_cast_queue(w, this);
            }

            //Console.WriteLine("====================================");
        });
        action_chain.enqueue_chain(w);
    } else {
        // temporarily old version:

        // return to magic combat stance
        let return_stance = Motion::new(MotionStance::Magic, MotionCommand::Ready, 1.0);
        world_object_networking::enqueue_broadcast_motion(w, this, &return_stance, None, None);

        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, f64::from(1.0f32)); // TODO: get actual recoil timing
        action_chain.add_action(Actor::Object(this), move |w| {
            set_is_busy(w, this, false);
            send_use_done_event(w, this, WeenieError::None);

            if queue {
                handle_cast_queue(w, this);
            }
        });
        action_chain.enqueue_chain(w);
    }
}

/// Method used for handling player targeted spell casts. With `caster_item`: casting a built-in
/// spell from a weapon.
// ACE: Player.CreatePlayerSpell
pub fn create_player_spell(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    target_category: TargetCategory,
    spell_id: u32,
    caster_item: Option<ObjectGuid>,
) -> bool {
    let Some(mut spell) = validate_spell(w, this, spell_id, caster_item.is_some()) else {
        return false;
    };

    if !verify_spell_target(w, this, &spell, target) {
        return false;
    }

    // if casting implement has spell built in,
    // use spellcraft from the item, instead of player's magic skill?
    let caster = caster_item.or_else(|| creature_equipment::get_equipped_wand(w, this));
    let is_weapon_spell =
        caster_item.is_some() && is_weapon_spell(w, this, spell.id(), caster_item);

    // Grab player's skill level in the spell's Magic School
    let mut magic_skill = school_skill_current(w, this, spell.school());
    if is_weapon_spell {
        if let Some(item_spellcraft) = caster.and_then(|c| obj(w, c).item_spellcraft()) {
            magic_skill = item_spellcraft.cast_unsigned();
        }
    }

    // verify spell range
    if !verify_spell_range(
        w,
        this,
        target,
        target_category,
        &spell,
        caster_item,
        magic_skill,
    ) {
        return false;
    }

    // get casting pre-check status
    let casting_pre_check_status =
        get_casting_pre_check_status(w, this, &spell, magic_skill, is_weapon_spell);

    // calculate mana usage
    let Some(mana_used) = calculate_mana_usage_player(
        w,
        this,
        casting_pre_check_status,
        &spell,
        Some(target),
        caster_item,
    ) else {
        return false;
    };

    // spell words
    do_spell_words(w, this, &mut spell, is_weapon_spell);

    let mut spell_chain = ActionChain::new();
    //StartPos = new Physics.Common.Position(PhysicsObj.Position);

    // do wind-up gestures: fastcast has no windup (creature enchantments)
    do_windup_gestures(w, this, &spell, is_weapon_spell, &mut spell_chain);

    // cast spell
    do_cast_gesture(w, this, &spell, caster_item, &mut spell_chain);

    magic_state::set_cast_params(
        w,
        this,
        spell,
        caster_item,
        magic_skill,
        mana_used,
        Some(target),
        casting_pre_check_status,
    );

    if !crate::world_objects::player_tick::fast_tick(w, this) {
        spell_chain.add_action(Actor::Object(this), move |w| {
            do_cast_spell_state(w, this, true)
        });
    }

    spell_chain.enqueue_chain(w);

    true
}

// ACE: Player.GetFellowshipTargets
#[must_use]
pub fn get_fellowship_targets(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    player_fellowship_get_fellowship_members(w, this).unwrap_or_else(|| vec![this])
}

/// The private `CreatePlayerSpell(WorldObject target, Spell spell, bool isWeaponSpell)` overload:
/// the spell lands (after the windup).
// ACE: Player.CreatePlayerSpell
fn create_player_spell_on(
    w: &mut World,
    this: ObjectGuid,
    target: Option<ObjectGuid>,
    spell: &Spell,
    is_weapon_spell: bool,
) {
    let target = live(w, target);
    let target_creature = target.filter(|&t| is_creature(w, t));
    let target_player = target.filter(|&t| is_player(w, t));

    {
        let now = w.now.unix_time;
        let f = fields_mut(w, this);
        f.last_success_cast_school = spell.school();
        f.last_success_cast_time = now;
    }

    let caster = creature_equipment::get_equipped_wand(w, this);

    let item_caster = if is_weapon_spell { caster } else { None };

    // verify after windup, still consumes mana
    if spell.meta_spell_type() == SpellType::Dispel
        && !world_object_magic::verify_dispel_pk_status(w, Some(this), target)
    {
        return;
    }

    if spell.school() == MagicSchool::ItemEnchantment {
        world_object_magic::try_cast_item_enchantment_with_redirects(
            w,
            this,
            spell,
            target,
            item_caster,
        );

        // use target resistance?
        proficiency_on_success_use(w, this, Skill::ItemEnchantment, spell.power_mod());

        if spell.is_harmful() {
            let mut player_redirect = target_player;
            if player_redirect.is_none() {
                if let Some(wielder_id) = target.and_then(|t| obj(w, t).wielder_id()) {
                    player_redirect =
                        current_landblock_get_object(w, this, ObjectGuid::new(wielder_id))
                            .filter(|&p| is_player(w, p));
                }
            }

            if let Some(player_redirect) = player_redirect {
                player_update_pk_timers(w, this, player_redirect);
            }
        }
    } else {
        let mut resisted_or_immune = false;
        if !spell.is_projectile() {
            if target_player.is_none() {
                player_on_attack_monster(w, this, target_creature);
            }

            if world_object_magic::try_resist_spell(w, this, target, spell, item_caster, false) {
                resisted_or_immune = true;
            } else if let Some(tc) =
                target_creature.filter(|&tc| obj(w, tc).non_projectile_magic_immune())
            {
                let msg = format!(
                    "You fail to affect {} with {}",
                    name_of(w, tc),
                    spell.name()
                );
                send_system_chat(w, this, &msg, ChatMessageType::Magic);
                resisted_or_immune = true;
            }
        }

        if !resisted_or_immune {
            let _ = world_object_magic::handle_cast_spell(
                w,
                this,
                spell,
                target,
                item_caster,
                caster,
                is_weapon_spell,
                false,
                false,
            );

            if !spell.is_projectile() {
                if spell.is_harmful() {
                    if let Some(tc) = target_creature {
                        let difficulty = {
                            let skill = get_creature_skill(w, tc, Skill::MagicDefense);
                            skill.current(w, tc)
                        };
                        let school_skill = spell.get_magic_skill();
                        proficiency_on_success_use(w, this, school_skill, difficulty);
                    }

                    // handle target procs
                    if let Some(tc) = target_creature.filter(|&tc| tc != this) {
                        world_object_try_proc_equipped_items(w, this, this, tc, false, caster);
                    }

                    if let Some(tp) = target_player {
                        player_update_pk_timers(w, this, tp);
                    }
                } else {
                    proficiency_on_success_use(w, this, spell.get_magic_skill(), spell.power_mod());
                }
            }
        }
    }
}

/// Method used for handling player untargeted spell casts.
// ACE: Player.CreatePlayerSpell
pub fn create_player_spell_untargeted(w: &mut World, this: ObjectGuid, spell_id: u32) -> bool {
    let Some(mut spell) = validate_spell(w, this, spell_id, false) else {
        return false;
    };

    // get player's current magic skill
    let magic_skill = school_skill_current(w, this, spell.school());

    let casting_pre_check_status =
        get_casting_pre_check_status(w, this, &spell, magic_skill, false);

    // calculate mana usage
    let Some(mana_used) =
        calculate_mana_usage_player(w, this, casting_pre_check_status, &spell, None, None)
    else {
        return false;
    };

    // begin spellcasting
    do_spell_words(w, this, &mut spell, false);

    let mut spell_chain = ActionChain::new();

    //StartPos = new Physics.Common.Position(PhysicsObj.Position);

    // do wind-up gestures: fastcast has no windup (creature enchantments)
    do_windup_gestures(w, this, &spell, false, &mut spell_chain);

    // do cast gesture
    do_cast_gesture(w, this, &spell, None, &mut spell_chain);

    // cast untargeted spell
    magic_state::set_cast_params(
        w,
        this,
        spell,
        None,
        magic_skill,
        mana_used,
        None,
        casting_pre_check_status,
    );

    if !crate::world_objects::player_tick::fast_tick(w, this) {
        spell_chain.add_action(Actor::Object(this), move |w| {
            do_cast_spell_state(w, this, true)
        });
    }

    spell_chain.enqueue_chain(w);

    true
}

/// Rolls the formula's components for burning (`Spell.TryBurnComponents`), consumes one of each
/// burned component from the pack (last first), and tells the player which were consumed.
// ACE: Player.TryBurnComponents
pub fn try_burn_components(w: &mut World, this: ObjectGuid, spell: &Spell) {
    if obj(w, this).safe_spell_components()
        || property_manager::get_bool(w, "safe_spell_comps", false, true).item
    {
        return;
    }

    let mut burned = spell.try_burn_components(w, this);
    if burned.is_empty() {
        return;
    }

    // decrement components
    let mut i = burned.len();
    while i > 0 {
        i -= 1;
        let component = burned[i];

        let Some(spell_component_name) = crate::entity::spell_formula::spell_components_table(w)
            .components
            .get(&component)
            .map(|c| c.name.clone())
        else {
            log::error!(
                "{}.TryBurnComponents(): Couldn't find SpellComponent {component}",
                name_of(w, this)
            );
            continue;
        };

        let wcid = Spell::get_component_wcid(w, component);
        if wcid == 0 {
            continue;
        }

        let item = container::get_inventory_items_of_wcid(w, this, wcid)
            .into_iter()
            .next();
        let Some(item) = item else {
            if obj(w, this).spell_components_required()
                && property_manager::get_bool(w, "require_spell_comps", false, true).item
            {
                log::warn!(
                    "{}.TryBurnComponents({spell_component_name}): not found in inventory",
                    name_of(w, this)
                );
            } else {
                burned.remove(i);
            }

            continue;
        };
        crate::world_objects::player_inventory::try_consume_from_inventory_with_networking(
            w, this, item, 1,
        );
    }

    if burned.is_empty() {
        return;
    }

    // send message to player
    let msg = Spell::get_consume_string(w, &burned);
    send_system_chat(w, this, &msg, ChatMessageType::Magic);
}

/// Returns TRUE if the player has the required number of components to cast spell.
// ACE: Player.HasComponentsForSpell
pub fn has_components_for_spell(w: &mut World, this: ObjectGuid, spell: &mut Spell) -> bool {
    get_player_formula(w, this, spell);

    if !obj(w, this).spell_components_required()
        || !property_manager::get_bool(w, "require_spell_comps", false, true).item
    {
        return true;
    }

    let required_comps = spell.formula_ref().get_required_comps(w);

    for (&wcid, &required) in required_comps.iter() {
        let available = container::get_num_inventory_items_of_wcid(w, this, wcid);

        if required > available {
            return false;
        }
    }
    true
}

// ACE: Player.HandleMotionDone_Magic
pub fn handle_motion_done_magic(w: &mut World, this: ObjectGuid, motion_id: u32, success: bool) {
    //Console.WriteLine($"HandleMotionDone_Magic({(MotionCommand)motionID}, {success})");

    if !crate::world_objects::player_tick::fast_tick(w, this) || !ms(w, this).is_casting {
        return;
    }

    if motion_id == ms(w, this).cast_gesture.0 {
        if record_cast_enabled(w, this) {
            record_cast_log(
                w,
                this,
                &format!(
                    "{}.HandleMotionDone_Magic({}, {}) - cast gesture done",
                    name_of(w, this),
                    MotionCommand(motion_id).to_dotnet_string(),
                    if success { "True" } else { "False" }
                ),
            );
        }

        ms_mut(w, this).cast_motion_done = true;

        let mut action_chain = ActionChain::new();
        action_chain.add_delay_for_one_tick(w);
        action_chain.add_action(Actor::Object(this), move |w| {
            if !ms(w, this).is_casting {
                return;
            }

            ms_mut(w, this).always_turn = true;

            do_cast_spell_state(w, this, true);
        });
        action_chain.enqueue_chain(w);
    }
}

// ACE: Player.OnMoveComplete_Magic
pub fn on_move_complete_magic(w: &mut World, this: ObjectGuid, status: WeenieError) {
    //Console.WriteLine($"OnMoveComplete_Magic({status})");

    if !crate::world_objects::player_tick::fast_tick(w, this)
        || !ms(w, this).is_casting
        || !ms(w, this).turn_started
    {
        return;
    }

    // this occurs after the player is done turning
    // before the windup, or after the first half of the cast motion
    // either completed or cancelled

    if record_cast_enabled(w, this) {
        record_cast_log(
            w,
            this,
            &format!(
                "{}.OnMoveComplete_Magic({}) - DoCastSpell",
                name_of(w, this),
                status.to_dotnet_string()
            ),
        );
    }

    ms_mut(w, this).is_turning = false;

    let check_angle = status != WeenieError::None;

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_for_one_tick(w);
    action_chain.add_action(Actor::Object(this), move |w| {
        if !ms(w, this).is_casting {
            return;
        }

        if !ms(w, this).cast_motion_done {
            let windup = ms(w, this).windup_params;
            do_windup(w, this, windup, check_angle);
        } else {
            do_cast_spell_state(w, this, check_angle);
        }
    });
    action_chain.enqueue_chain(w);
}

/// `try_fizzle` defaults to true in ACE.
// ACE: Player.FailCast
pub fn fail_cast(w: &mut World, this: ObjectGuid, try_fizzle: bool) {
    let parms = ms(w, this).cast_spell_params.clone();

    let mut werror = WeenieError::None;

    if let Some(parms) = parms.filter(|_| try_fizzle) {
        do_cast_spell_inner(
            w,
            this,
            &parms.spell,
            parms.caster_item,
            parms.mana_used,
            parms.target,
            CastingPreCheckStatus::CastFailed,
            false,
        );

        werror = WeenieError::YourSpellFizzled;
    }
    send_use_done_event(w, this, werror);

    magic_state::on_cast_done(w, this);
}

// ACE: Player.OnTurnRelease
pub fn on_turn_release(w: &mut World, this: ObjectGuid) {
    ms_mut(w, this).pending_turn_release = false;

    if !ms(w, this).cast_motion_done {
        let windup = ms(w, this).windup_params;
        do_windup(w, this, windup, true);
    } else {
        do_cast_spell_state(w, this, true);
    }
}

// ACE: Player.VerifyCastRadius
pub fn verify_cast_radius(w: &mut World, this: ObjectGuid) -> bool {
    if ms(w, this).cast_gesture_start_time != DotNetDateTime::MIN_VALUE {
        let start_pos = fields(w, this)
            .start_pos
            .expect("ACE: StartPos is null (NullReferenceException)");
        let dist = physics_distance(&start_pos, &physics_position(w, this));

        if dist > WINDUP_MAX_MOVE && obj(w, this).player_killer_status() != PlayerKillerStatus::NPK
        {
            fail_cast(w, this, true);
            return false;
        }
    }
    true
}

// ACE: Player.CheckTurn
pub fn check_turn(w: &mut World, this: ObjectGuid) {
    // verify cast radius while manually moving after windup
    if !verify_cast_radius(w, this) {
        return;
    }

    if let Some(turn_target) = live(w, fields(w, this).turn_target) {
        if is_within_angle(w, this, turn_target) {
            if ms(w, this).pending_turn_release {
                on_turn_release(w, this);
            } else {
                physics_stop_completely(w, this);
            }
        }
    }
}

// ACE: Player.HandleCastQueue
pub fn handle_cast_queue(w: &mut World, this: ObjectGuid) {
    ms_mut(w, this).can_queue = false;

    if let Some(queue) = ms(w, this).cast_queue {
        if queue.type_ == CastQueueType::Targeted {
            handle_action_cast_targeted_spell(
                w,
                this,
                queue.target_guid,
                queue.spell_id,
                queue.caster_item,
            );
        } else {
            handle_action_magic_cast_un_targeted_spell(w, this, queue.spell_id);
        }
    }
}

// ACE: Player.VerifyNonComponentTargetType
#[must_use]
pub fn verify_non_component_target_type(
    w: &World,
    spell: &Spell,
    target: Option<ObjectGuid>,
) -> bool {
    // untargeted spell projectiles
    let Some(target) = live(w, target) else {
        return spell.non_component_target_type() == ItemType::None;
    };
    let t = obj(w, target);

    match spell.non_component_target_type() {
        ItemType::Creature => return t.is_creature(),

        // banes / lures
        ItemType::Vestements => {
            //return target is Clothing || target.IsShield;
            return t.is_creature() || t.is_clothing() || t.is_shield();
        }

        ItemType::Weapon => {
            //return target is MeleeWeapon || target is MissileLauncher;
            return t.is_creature() || t.is_melee_weapon() || t.is_missile_launcher();
        }

        ItemType::Caster => {
            //return target is Caster;
            return t.is_creature() || t.is_caster();
        }

        ItemType::WeaponOrCaster => {
            //return target is MeleeWeapon || target is MissileLauncher || target is Caster;
            return t.is_creature()
                || t.is_melee_weapon()
                || t.is_missile_launcher()
                || t.is_caster();
        }

        ItemType::Portal => {
            return if spell.meta_spell_type() == SpellType::PortalRecall
                || spell.meta_spell_type() == SpellType::PortalSummon
            {
                t.is_creature()
            } else {
                t.is_portal()
            };
        }

        ItemType::LockableMagicTarget => return t.is_door() || t.is_chest(),

        // Essence Lull?
        ItemType::Item => return !t.is_creature(),

        ItemType::LifeStone => return t.is_lifestone(),

        _ => {}
    }

    log::error!(
        "VerifyNonComponentTargetType({} - {}, {}) - unexpected NonComponentTargetType {}",
        spell.id(),
        spell.name(),
        name_of(w, target),
        spell.non_component_target_type().to_dotnet_string()
    );
    false
}

/// Sends a chat message with respect to SquelchManager.
// ACE: Player.SendChatMessage
pub fn send_chat_message(
    w: &mut World,
    this: ObjectGuid,
    source: Option<ObjectGuid>,
    msg: &str,
    msg_type: ChatMessageType,
) {
    if !squelch_manager_squelches_contains(w, this, source, msg_type) {
        send_system_chat(w, this, msg, msg_type);
    }
}

// ------------------------------------------------------------------ pointers (members ported elsewhere)

/// `Player.MaxRadarRange_Outdoors` (`Player.cs`).
const PLAYER_MAX_RADAR_RANGE_OUTDOORS: f32 = 75.0;

/// `spell._spellBase.GetSpellWords(DatManager.PortalDat.SpellComponentsTable)`.
fn spell_words(w: &World, spell: &Spell) -> String {
    use empyrean_dat::file_types::spell_table::SpellBaseExt;
    let base = spell
        .spell_base
        .as_ref()
        .expect("ACE: Spell._spellBase is null (NullReferenceException)");
    base.get_spell_words(w.dats.portal_dat().spell_components_table())
        .unwrap_or_else(|e| panic!("ACE: {e:?}"))
}

/// `CurrentLandblock?.GetObject(guid)` (searching the adjacent landblocks, ACE's default).
fn current_landblock_get_object(
    w: &World,
    this: ObjectGuid,
    guid: ObjectGuid,
) -> Option<ObjectGuid> {
    obj(w, this)
        .current_landblock
        .and_then(|lb| crate::entity::landblock::get_object(w, lb, guid, true))
}

/// `CheckPKStatusVsTarget(target, spell)` (the virtual, `WorldObject_Combat.cs`/`Player_Combat.cs`):
/// the two errors, or `None` when the cast is allowed.
fn check_pk_status_vs_target(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    _spell: &Spell,
) -> Option<Vec<WeenieErrorWithString>> {
    crate::dispatch::check_pk_status_vs_target::check_pk_status_vs_target(w, this, target, ())
}

/// `GetAngle(target)` (`Creature_Navigation.cs`).
fn creature_get_angle(w: &World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    crate::world_objects::creature_navigation::get_angle(w, this, target)
}

/// `IsDead` (`Creature_Vitals.cs`): `Health.Current <= 0`.
fn creature_is_dead(w: &World, this: ObjectGuid) -> bool {
    let o = obj(w, this);
    o.health().current(o) == 0
}

/// `SendUseDoneEvent(errorType)` (`Player_Use.cs`): `Session.Network.EnqueueSend(new
/// GameEventUseDone(Session, errorType))`. Exact.
pub(crate) fn send_use_done_event(w: &mut World, this: ObjectGuid, error_type: WeenieError) {
    let s = session_of(w, this);
    let msg = game_event_use_done(session_data(w, s), error_type);
    enqueue_send(w, s, msg);
}

/// `SendWeenieError(error)` (`Player_Networking.cs`).
fn send_weenie_error(w: &mut World, this: ObjectGuid, error: WeenieError) {
    crate::world_objects::player_networking::send_weenie_error(w, this, error);
}

/// `Session.Network.EnqueueSend(new GameEventWeenieErrorWithString(Session, error, str))`.
fn send_weenie_error_with_string(
    w: &mut World,
    this: ObjectGuid,
    error: WeenieErrorWithString,
    str: &str,
) {
    let s = session_of(w, this);
    let msg = game_event_weenie_error_with_string(session_data(w, s), error, str);
    enqueue_send(w, s, msg);
}

/// `Player.LastCombatMode` (`Player_Combat.cs`).
fn player_last_combat_mode(w: &World, this: ObjectGuid) -> CombatMode {
    crate::world_objects::player_combat::fields(w, this).last_combat_mode
}

/// `Player.PKLogout` (`Player.cs`).
fn player_pk_logout(w: &World, this: ObjectGuid) -> bool {
    obj(w, this)
        .player
        .as_ref()
        .is_some_and(|p| p.player.pk_logout)
}

/// `Player.suicideInProgress` (`Player_Death.cs`).
fn player_suicide_in_progress(w: &World, this: ObjectGuid) -> bool {
    obj(w, this)
        .player
        .as_ref()
        .is_some_and(|p| p.player_death.suicide_in_progress)
}

/// `Player.UnderLifestoneProtection` (`Player_Death.cs`).
pub(crate) fn under_lifestone_protection(w: &World, this: ObjectGuid) -> bool {
    obj(w, this).under_lifestone_protection()
}

/// `Player.LifestoneProtectionDispel()` (`Player_Death.cs`).
pub(crate) fn lifestone_protection_dispel(w: &mut World, this: ObjectGuid) {
    crate::world_objects::player_death::lifestone_protection_dispel(w, this);
}

/// `Player.GetKnownTradeObj(itemGuid)` (`Player_Trade.cs`): the trade partner who offered the
/// item.
fn player_get_known_trade_obj(
    w: &mut World,
    this: ObjectGuid,
    item_guid: ObjectGuid,
) -> Option<ObjectGuid> {
    crate::world_objects::player_trade::get_known_trade_obj(w, this, item_guid)
}

/// `Player.GetNameWithSuffix()` (`Player_Properties.cs`).
fn player_get_name_with_suffix(w: &World, this: ObjectGuid) -> String {
    crate::world_objects::player_properties::get_name_with_suffix(w, this)
}

/// `Player.OnTalk(string message)` (`Player.cs`: the emote listeners around the speaker).
fn player_on_talk(w: &mut World, this: ObjectGuid, message: &str) {
    crate::world_objects::player::on_talk(w, this, message);
}

/// `Proficiency.OnSuccessUse(this, GetCreatureSkill(skill), difficulty)`.
fn proficiency_on_success_use(w: &mut World, this: ObjectGuid, skill: Skill, difficulty: u32) {
    let skill = crate::entity::proficiency::get_creature_skill(w, this, skill);
    crate::entity::proficiency::on_success_use(w, this, skill, difficulty);
}

/// `TryProcEquippedItems(attacker, target, selfTarget, weapon)` (`WorldObject_Combat.cs`), on
/// `this`.
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

/// `Player.UpdatePKTimers(attacker, defender)` (`Player_Combat.cs`).
fn player_update_pk_timers(w: &mut World, attacker: ObjectGuid, defender: ObjectGuid) {
    crate::world_objects::player_combat::update_pk_timers(w, attacker, defender);
}

/// `Player.OnAttackMonster(monster)` (`Player_Monster.cs`).
fn player_on_attack_monster(w: &mut World, this: ObjectGuid, monster: Option<ObjectGuid>) {
    crate::world_objects::player_monster::on_attack_monster(w, this, monster);
}

/// `Fellowship?.GetFellowshipMembers().Values` (`Fellowship.cs`); `GetManaCost` reads its count
/// for `Fellowship.FellowshipMembers.Count`.
fn player_fellowship_get_fellowship_members(
    w: &World,
    this: ObjectGuid,
) -> Option<Vec<ObjectGuid>> {
    let fellowship = crate::world_objects::player_fellowship::fellowship(w, this)?;
    Some(crate::entity::fellowship::get_fellowship_members_read_only(
        w,
        &fellowship,
    ))
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

/// `RecordCast.Enabled` (set only by the `/recordcast` developer command).
pub(crate) fn record_cast_enabled(w: &World, this: ObjectGuid) -> bool {
    fields(w, this).record_cast.enabled
}

/// `RecordCast.Log(line)` (`Entity/RecordCast.cs`).
pub(crate) fn record_cast_log(w: &mut World, this: ObjectGuid, line: &str) {
    crate::entity::record_cast::log(w, this, line);
}

/// `RecordCast.Log($"{prefix}{wo.Location.ToLOCString()}")`.
pub(crate) fn record_cast_log_location(
    w: &mut World,
    this: ObjectGuid,
    prefix: &str,
    wo: ObjectGuid,
) {
    let loc = w
        .objects
        .get(wo)
        .and_then(WorldObject::location)
        .map(|l| l.to_loc_string())
        .unwrap_or_default();
    record_cast_log(w, this, &format!("{prefix}{loc}"));
}

/// `RecordCast.Flush()`.
pub(crate) fn record_cast_flush(w: &mut World, this: ObjectGuid) {
    crate::entity::record_cast::flush(w, this);
}

/// `RecordCast.OnCastTargetedSpell(spell, target)`.
fn record_cast_on_cast_targeted_spell(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: ObjectGuid,
) {
    crate::entity::record_cast::on_cast_targeted_spell(w, this, spell, target);
}

/// `RecordCast.OnCastUntargetedSpell(spell)`.
fn record_cast_on_cast_untargeted_spell(w: &mut World, this: ObjectGuid, spell: &Spell) {
    crate::entity::record_cast::on_cast_untargeted_spell(w, this, spell);
}

/// `Physics.Animation.MotionTable.GetAnimationLength(MotionTableId, CurrentMotionState.Stance,
/// motion, CastSpeed)`.
fn record_cast_animation_length(w: &World, this: ObjectGuid, motion: MotionCommand) -> f32 {
    let o = obj(w, this);
    let stance = current_motion_state(w, this)
        .expect("ACE: CurrentMotionState is null (NullReferenceException)")
        .stance;
    crate::physics::motion_table::get_animation_length(
        w,
        o.motion_table_id(),
        stance,
        motion,
        CAST_SPEED,
    )
}

/// `CanDamage(target)` (the virtual, `Creature_Combat.cs`; the Player override in
/// `Player_Combat.cs`).
fn creature_can_damage(w: &World, this: ObjectGuid, target: ObjectGuid) -> bool {
    crate::dispatch::can_damage::can_damage(w, this, target)
}
