// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Combat.cs`.
//!
//! Combat mode and stance changes, the attack and defence skill getters, the shield, sneak attack,
//! recklessness, Dirty Fighting and overpower rolls, faction and foe tests, and the retaliate
//! targets. Members that touch only one object's data are still free functions `(w, this, ..)`
//! because the enchanted stat getters read through the caching `EnchantmentManager`.
//!
//! The generated virtual-dispatch targets call the typed ports [`get_combat_type`],
//! [`get_damage_type`], [`on_evade`] and [`on_damage_target`].
//!
//! [`shim`] holds faithful, unanchored ports of the members of other ACE files that the
//! damage pipeline needs (`Player_Combat`, `Monster_Melee`, `Monster_Combat`, `Creature_Rating`'s
//! getters, `Creature_Properties`' resistances, ...). Each names its ACE member, so it can give way
//! to that member's anchored port.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    AttackHeight, AttackType, ChatMessageType, CombatMode, CombatStyle, DamageType, FactionBits,
    ImbuedEffectType, MotionCommand, MotionStance, PlayScript, PlayerKillerStatus,
    PropertyAttribute, PropertyInt, ResistanceType, Skill, SkillAdvancementClass, SpellId,
    TargetingTactic, Tolerance,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::spell::Spell;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::messages::game_message_pickup_event::game_message_pickup_event;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::motion::movement_data::Motion;
use crate::physics::motion_table;
use crate::world_objects::creature_equipment;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::monster::{self, State};
use crate::world_objects::skill_formula;
use crate::world_objects::world_object::{self, WorldObject};
use crate::world_objects::world_object_weapon::{self as weapon_mod, SkillOf};
use crate::world_objects::{monster_awareness, monster_combat, monster_melee};
use crate::World;

pub use crate::world_objects::world_object_magic::DebugDamageType;

/// ACE's `CombatType` (melee / missile / magic). ACE declares it in `Player_Combat.cs`; it lives
/// here until that file is ported, because the damage pipeline is built on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CombatType {
    #[default]
    Melee,
    Missile,
    Magic,
}

impl CombatType {
    /// The enum member's name (`ToString()`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            CombatType::Melee => "Melee",
            CombatType::Missile => "Missile",
            CombatType::Magic => "Magic",
        }
    }
}

/// Non-property fields declared in `Creature_Combat.cs`.
///
/// `AttackTarget`, `AttackHeight` and `CurrentAttack` are declared in `Monster_Combat.cs` and live in
/// `MonsterCombatFields`. `DamageHistory` is held in `CreatureDeathFields`.
#[derive(Debug)]
pub struct CreatureCombatFields {
    // ACE: Creature.CombatTable
    pub combat_table: Option<std::sync::Arc<dereth_assets::CombatManeuverTable>>,
    // ACE: Creature.DebugDamage
    pub debug_damage: DebugDamageType,
    // ACE: Creature.DebugDamageTarget
    pub debug_damage_target: ObjectGuid,
    /// `CombatMode { get; protected set; }`, set to `NonCombat` by `Creature.SetEphemeralValues`.
    // ACE: Creature.CombatMode
    pub combat_mode: CombatMode,
    // ACE: Creature.AttackType
    pub attack_type: AttackType,
    /// Handles queueing up multiple animation sequences between packets (a bow to sword switch
    /// queues peace mode, unarmed combat, peace mode, bow combat).
    // ACE: Creature.LastWeaponSwap
    pub last_weapon_swap: f64,
    /// Storage for the Player fields of files not ported yet ([`shim`]).
    pub player: PlayerCombatShimFields,
}

impl Default for CreatureCombatFields {
    fn default() -> Self {
        Self {
            combat_table: None,
            debug_damage: DebugDamageType::None,
            debug_damage_target: ObjectGuid::default(),
            combat_mode: CombatMode::NonCombat,
            attack_type: AttackType::Undef,
            last_weapon_swap: 0.0,
            player: PlayerCombatShimFields::default(),
        }
    }
}

/// SHIM storage: Player fields declared in other ACE files, which the damage pipeline reads.
/// (`_powerLevel` and `_accuracyLevel` are in `PlayerMeleeFields`/`PlayerMissileFields`.)
#[derive(Debug, Default)]
pub struct PlayerCombatShimFields {
    /// `Player.stance` (`Player.cs`, initialised to `NonCombat`).
    pub stance: Option<MotionStance>,
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

fn object_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// This creature's `Creature_Combat` fields.
///
/// # Panics
/// When `o` is not a Creature (ACE: `InvalidCastException`).
#[must_use]
pub fn fields(o: &WorldObject) -> &CreatureCombatFields {
    &o.creature
        .as_ref()
        .expect("InvalidCastException: not a Creature")
        .creature_combat
}

/// This creature's `Creature_Combat` fields, mutably.
///
/// # Panics
/// When `o` is not a Creature (ACE: `InvalidCastException`).
pub fn fields_mut(o: &mut WorldObject) -> &mut CreatureCombatFields {
    &mut o
        .creature
        .as_mut()
        .expect("InvalidCastException: not a Creature")
        .creature_combat
}

// ACE: Creature.CombatMode
/// # Panics
/// When `this` is missing or not a Creature.
#[must_use]
pub fn combat_mode(w: &World, this: ObjectGuid) -> CombatMode {
    fields(object(w, this)).combat_mode
}

// ACE: Creature.CombatMode
/// # Panics
/// When `this` is missing or not a Creature.
pub fn set_combat_mode_field(w: &mut World, this: ObjectGuid, value: CombatMode) {
    fields_mut(object_mut(w, this)).combat_mode = value;
}

// ACE: Creature.AttackType
/// # Panics
/// When `this` is missing or not a Creature.
#[must_use]
pub fn attack_type(w: &World, this: ObjectGuid) -> AttackType {
    fields(object(w, this)).attack_type
}

/// `Creature.AttackHeight` (`Monster_Combat.cs`, held in `MonsterCombatFields`).
#[must_use]
pub fn attack_height(w: &World, this: ObjectGuid) -> Option<AttackHeight> {
    crate::world_objects::monster_combat::fields(w, this).attack_height
}

/// `this is Player`.
fn is_player(w: &World, g: ObjectGuid) -> bool {
    object(w, g).is_player()
}

/// `x is Creature`.
fn is_creature(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some_and(WorldObject::is_creature)
}

/// `CurrentMotionState.Stance`.
///
/// # Panics
/// Without a `CurrentMotionState` (ACE: `NullReferenceException`).
fn current_stance(w: &World, this: ObjectGuid) -> MotionStance {
    object(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .expect("System.NullReferenceException: CurrentMotionState")
        .stance
}

/// `Creature.GetCreatureSkill(skill)` (adds the record if missing).
fn skill_of(w: &mut World, this: ObjectGuid, skill: Skill) -> SkillOf {
    SkillOf::get(w, this, skill)
}

/// `GetCreatureSkill(skill).Current`.
fn skill_current(w: &mut World, this: ObjectGuid, skill: Skill) -> u32 {
    skill_of(w, this, skill).current(w)
}

/// `<attribute>.Current` (the enchanted, cached value).
pub(crate) fn attribute_current(
    w: &mut World,
    this: ObjectGuid,
    attribute: PropertyAttribute,
) -> u32 {
    use crate::world_objects::entity::creature_attribute::StatCtx;
    let a = object(w, this)
        .attributes()
        .get(&attribute)
        .copied()
        .expect("Creature.Attributes: every attribute is present");
    a.current(&mut StatCtx::in_world(w, this))
}

/// `<attribute>.Base`.
pub(crate) fn attribute_base(w: &World, this: ObjectGuid, attribute: PropertyAttribute) -> u32 {
    let o = object(w, this);
    let a = o
        .attributes()
        .get(&attribute)
        .copied()
        .expect("Creature.Attributes: every attribute is present");
    a.base(o)
}

/// `(uint)Math.Round(x)` for a `float` expression (widened to `double`, rounded half to even).
fn round_to_uint(x: f32) -> u32 {
    math::round(f64::from(x)).cs_cast()
}

/// `Time.GetUnixTime()`: the tick's clock.
fn unix_time(w: &World) -> f64 {
    w.now.unix_time
}

/// `player.Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    crate::world_objects::player_skills::send(w, player, [msg]);
}

/// `EnqueueBroadcast(msgs)`.
fn enqueue_broadcast(w: &mut World, this: ObjectGuid, msgs: &[GameMessage]) {
    let _ = crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, msgs);
}

// ============================================================================== members

/// Switches a player or creature to a new combat stance (`SetCombatMode(CombatMode)`).
// ACE: Creature.SetCombatMode
pub fn set_combat_mode(w: &mut World, this: ObjectGuid, combat_mode: CombatMode) -> f32 {
    set_combat_mode_with(w, this, combat_mode, false, false).0
}

/// Switches a player or creature to a new combat stance. Returns `(queueTime + animLength,
/// queueTime)`. ACE's defaults: `force_hand_combat = false`, `anim_only = false`.
// ACE: Creature.SetCombatMode
pub fn set_combat_mode_with(
    w: &mut World,
    this: ObjectGuid,
    combat_mode: CombatMode,
    force_hand_combat: bool,
    anim_only: bool,
) -> (f32, f32) {
    // check if combat stance actually needs switching
    let combat_stance = if force_hand_combat {
        MotionStance::HandCombat
    } else {
        get_combat_stance(w, this)
    };

    //Console.WriteLine($"{Name}.SetCombatMode({combatMode}), CombatStance: {combatStance}");

    if combat_mode != CombatMode::NonCombat && current_stance(w, this) == combat_stance {
        return (0.0, 0.0);
    }

    if self::combat_mode(w, this) == CombatMode::Missile {
        hide_ammo(w, this);
    }

    if !anim_only {
        set_combat_mode_field(w, this, combat_mode);
    }

    let anim_length = match combat_mode {
        CombatMode::NonCombat => handle_switch_to_peace_mode(w, this),
        CombatMode::Melee => handle_switch_to_melee_combat_mode(w, this, force_hand_combat),
        CombatMode::Magic => handle_switch_to_magic_combat_mode(w, this),
        CombatMode::Missile => handle_switch_to_missile_combat_mode(w, this),
        _ => {
            log::info!(
                "Unknown combat mode {} for {}",
                self::combat_mode(w, this).to_dotnet_string(),
                shim::name(w, this)
            );
            0.0
        }
    };

    let queue_time = handle_stance_queue(w, this, anim_length);

    //Console.WriteLine($"SetCombatMode(): queueTime({queueTime}) + animLength({animLength})");
    (queue_time + anim_length, queue_time)
}

/// `new GameMessagePrivateUpdatePropertyInt(this, PropertyInt.CombatMode, (int)mode)`.
fn combat_mode_message(w: &mut World, this: ObjectGuid, mode: CombatMode) -> GameMessage {
    game_message_private_update_property_int(object_mut(w, this), PropertyInt::CombatMode, mode.0)
}

/// Switches a player or creature to non-combat mode.
// ACE: Creature.HandleSwitchToPeaceMode
pub fn handle_switch_to_peace_mode(w: &mut World, this: ObjectGuid) -> f32 {
    let anim_length = motion_table::get_animation_length_between(
        w,
        object(w, this).motion_table_id(),
        current_stance(w, this),
        MotionCommand::Ready,
        MotionCommand::NonCombat,
        1.0,
    );

    let motion = Motion::from_stance(MotionStance::NonCombat);
    world_object::execute_motion_persist(w, this, motion, true, None);

    if is_player(w, this) {
        fields_mut(object_mut(w, this)).player.stance = Some(MotionStance::NonCombat);
        let msg = combat_mode_message(w, this, CombatMode::NonCombat);
        send(w, this, msg);
    }

    //Console.WriteLine("HandleSwitchToPeaceMode() - animLength: " + animLength);
    anim_length
}

/// Handles switching between combat stances: old style -> peace mode -> hand combat (weapon swap)
/// -> peace mode -> new style.
// ACE: Creature.SwitchCombatStyles
pub fn switch_combat_styles(w: &mut World, this: ObjectGuid) -> f32 {
    let stance = current_stance(w, this);
    if stance == MotionStance::NonCombat
        || stance == MotionStance::Invalid
        || crate::world_objects::monster::fields(w, this).is_monster
    {
        return 0.0;
    }

    let _combat_stance = get_combat_stance(w, this);

    let (unarmed, peace2) = (0.0f32, 0.0f32);

    // this is now handled as a proper 2-step process in HandleActionChangeCombatMode / NextUseTime

    // FIXME: just call generic method to switch to HandCombat first
    let peace1 = motion_table::get_animation_length_between(
        w,
        object(w, this).motion_table_id(),
        stance,
        MotionCommand::Ready,
        MotionCommand::NonCombat,
        1.0,
    );
    /*if (CurrentMotionState.Stance != MotionStance.HandCombat && combatStance != MotionStance.HandCombat)
    {
        unarmed = MotionTable.GetAnimationLength(MotionTableId, MotionStance.NonCombat, MotionCommand.Ready, MotionCommand.HandCombat);
        peace2 = MotionTable.GetAnimationLength(MotionTableId, MotionStance.HandCombat, MotionCommand.Ready, MotionCommand.NonCombat);
    }*/

    world_object::set_stance(w, this, MotionStance::NonCombat, false);

    //Console.WriteLine($"SwitchCombatStyle() - animLength: {animLength}");
    peace1 + unarmed + peace2
}

/// Switches a player or creature to melee attack stance. ACE's default: `force_hand_combat =
/// false`.
// ACE: Creature.HandleSwitchToMeleeCombatMode
pub fn handle_switch_to_melee_combat_mode(
    w: &mut World,
    this: ObjectGuid,
    force_hand_combat: bool,
) -> f32 {
    // get appropriate combat stance for currently wielded items
    let combat_stance = if force_hand_combat {
        MotionStance::HandCombat
    } else {
        get_combat_stance(w, this)
    };

    let mut anim_length = switch_combat_styles(w, this);
    anim_length += motion_table::get_animation_length_between(
        w,
        object(w, this).motion_table_id(),
        current_stance(w, this),
        MotionCommand::Ready,
        MotionCommand(combat_stance.0),
        1.0,
    );

    let motion = Motion::from_stance(combat_stance);
    world_object::execute_motion_persist(w, this, motion, true, None);

    if is_player(w, this) {
        crate::world_objects::player_trade::handle_action_trade_switch_to_combat_mode(w, this);
        let msg = combat_mode_message(w, this, CombatMode::Melee);
        send(w, this, msg);
    }

    //Console.WriteLine("HandleSwitchToMeleeCombatMode() - animLength: " + animLength);
    anim_length
}

/// Switches a player or creature to magic casting stance.
// ACE: Creature.HandleSwitchToMagicCombatMode
pub fn handle_switch_to_magic_combat_mode(w: &mut World, this: ObjectGuid) -> f32 {
    let wand = creature_equipment::get_equipped_wand(w, this);
    if wand.is_none() {
        return 0.0;
    }

    let mut anim_length = switch_combat_styles(w, this);
    anim_length += motion_table::get_animation_length_between(
        w,
        object(w, this).motion_table_id(),
        current_stance(w, this),
        MotionCommand::Ready,
        MotionCommand::Magic,
        1.0,
    );

    let motion = Motion::from_stance(MotionStance::Magic);
    world_object::execute_motion_persist(w, this, motion, true, None);

    if is_player(w, this) {
        crate::world_objects::player_trade::handle_action_trade_switch_to_combat_mode(w, this);
        let msg = combat_mode_message(w, this, CombatMode::Magic);
        send(w, this, msg);
    }

    //Console.WriteLine("HandleSwitchToMagicCombatMode() - animLength: " + animLength);
    anim_length
}

/// Switches a player or creature to a missile combat stance.
// ACE: Creature.HandleSwitchToMissileCombatMode
pub fn handle_switch_to_missile_combat_mode(w: &mut World, this: ObjectGuid) -> f32 {
    // get appropriate combat stance for currently wielded items
    let Some(weapon) = creature_equipment::get_equipped_missile_weapon(w, this) else {
        return 0.0;
    };

    let combat_stance = get_combat_stance(w, this);

    let swap_time = switch_combat_styles(w, this);

    let motion = Motion::from_stance(combat_stance);
    let stance_time = world_object::execute_motion_persist(w, this, motion, true, None);

    let ammo = creature_equipment::get_equipped_ammo(w, this);
    let mut reload_time = 0.0f32;
    if ammo.is_some() && object(w, weapon).is_ammo_launcher() {
        // bug for bow-wielding skeletons starting from decomposed state:
        // sleep -> wakeup anim time must be passed in here
        let mut action_chain = ActionChain::new();

        let current_time = unix_time(w);
        let mut queue_time = 0.0f32;
        let last_weapon_swap = fields(object(w, this)).last_weapon_swap;
        if current_time < last_weapon_swap {
            queue_time += (last_weapon_swap - current_time) as f32;
        }

        action_chain.add_delay_seconds(w, f64::from(queue_time + swap_time + stance_time));
        reload_time = crate::world_objects::creature_missile::reload_missile_ammo(
            w,
            this,
            Some(&mut action_chain),
        );
        action_chain.enqueue_chain(w);
    }

    if is_player(w, this) {
        crate::world_objects::player_trade::handle_action_trade_switch_to_combat_mode(w, this);
        let msg = combat_mode_message(w, this, CombatMode::Missile);
        send(w, this, msg);
    }
    //Console.WriteLine("HandleSwitchToMissileCombatMode() - animLength: " + animLength);
    swap_time + stance_time + reload_time
}

/// Sends the message to hide the current equipped ammo.
// ACE: Creature.HideAmmo
pub fn hide_ammo(w: &mut World, this: ObjectGuid) {
    if let Some(ammo) = creature_equipment::get_equipped_ammo(w, this) {
        let msg = game_message_pickup_event(object_mut(w, ammo));
        enqueue_broadcast(w, this, &[msg]);
    }
}

/// Returns the combat stance for the currently wielded items.
// ACE: Creature.GetCombatStance
#[must_use]
pub fn get_combat_stance(w: &World, this: ObjectGuid) -> MotionStance {
    let caster = creature_equipment::get_equipped_wand(w, this);

    if caster.is_some() {
        return MotionStance::Magic;
    }

    let weapon = creature_equipment::get_equipped_weapon(w, this, true);
    let dual_wield = creature_equipment::get_dual_wield_weapon(w, this);
    let shield = creature_equipment::get_equipped_shield(w, this);

    let mut combat_stance = MotionStance::HandCombat;

    if let Some(weapon) = weapon {
        combat_stance = get_weapon_stance(w, this, weapon);
    }

    if dual_wield.is_some() {
        combat_stance = MotionStance::DualWieldCombat;
    }

    if shield.is_some() {
        combat_stance = add_shield_stance(combat_stance);
    }

    combat_stance
}

/// Translates the default combat style for a weapon into a combat motion stance.
// ACE: Creature.GetWeaponStance
#[must_use]
pub fn get_weapon_stance(w: &World, this: ObjectGuid, weapon: ObjectGuid) -> MotionStance {
    let style = object(w, weapon).default_combat_style();
    match style {
        Some(CombatStyle::Atlatl) => MotionStance::AtlatlCombat,
        Some(CombatStyle::Bow) => MotionStance::BowCombat,
        Some(CombatStyle::Crossbow) => MotionStance::CrossbowCombat,
        Some(CombatStyle::DualWield) => MotionStance::DualWieldCombat,
        Some(CombatStyle::Magic) => MotionStance::Magic,
        Some(CombatStyle::OneHanded) => MotionStance::SwordCombat,
        Some(CombatStyle::OneHandedAndShield) => MotionStance::SwordShieldCombat,
        Some(CombatStyle::Sling) => MotionStance::SlingCombat,
        Some(CombatStyle::ThrownShield) => MotionStance::ThrownShieldCombat,
        Some(CombatStyle::ThrownWeapon) => MotionStance::ThrownWeaponCombat,
        // MotionStance.TwoHandedStaffCombat doesn't appear to do anything
        // Additionally, PropertyInt.WeaponType isn't always included, and the 2handed weapons that do appear to use WeaponType.TwoHanded
        Some(CombatStyle::TwoHanded) => MotionStance::TwoHandedSwordCombat,
        Some(CombatStyle::Unarmed) => MotionStance::HandCombat,
        _ => {
            // `Console.WriteLine($"{Name}.GetCombatStance() - {weapon.DefaultCombatStyle}")`
            log::info!(
                "{}.GetCombatStance() - {}",
                shim::name(w, this),
                style.map(CombatStyle::to_dotnet_string).unwrap_or_default()
            );
            MotionStance::HandCombat
        }
    }
}

/// Adds the shield stance to an existing combat stance.
// ACE: Creature.AddShieldStance
#[must_use]
pub fn add_shield_stance(combat_stance: MotionStance) -> MotionStance {
    match combat_stance {
        MotionStance::SwordCombat => MotionStance::SwordShieldCombat,
        MotionStance::ThrownWeaponCombat => MotionStance::ThrownShieldCombat,
        other => other,
    }
}

/// Adds queued weapon swaps to the current animation time.
// ACE: Creature.HandleStanceQueue
pub fn handle_stance_queue(w: &mut World, this: ObjectGuid, anim_length: f32) -> f32 {
    let current_time = unix_time(w);
    let f = fields_mut(object_mut(w, this));
    if current_time >= f.last_weapon_swap {
        f.last_weapon_swap = current_time + f64::from(anim_length);
        0.0
    } else {
        f.last_weapon_swap += f64::from(anim_length);
        (f.last_weapon_swap - current_time - f64::from(anim_length)) as f32
    }
}

/// Returns the attack type for non-player creatures.
// ACE: Creature.GetCombatType
#[must_use]
pub fn get_combat_type(w: &World, this: ObjectGuid) -> CombatType {
    crate::world_objects::monster_combat::fields(w, this)
        .current_attack
        .unwrap_or(CombatType::Melee)
}

/// Returns the attribute damage bonus for a physical attack: Coordination for bows and finesse
/// weapons, else Strength.
// ACE: Creature.GetAttributeMod
pub fn get_attribute_mod(w: &mut World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> f32 {
    let is_bow = weapon.is_some_and(|g| object(w, g).is_bow());

    // DIVERGE: melee damage before the weapon-skill consolidation
    // (`EraFormulas::older_melee_damage`): daggers take Coordination, and the factor follows the
    // attack's skill, an unarmed humanoid's being the lower one (ClassicACE's `GetAttributeMod` at
    // its Infiltration ruleset).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
    if w.era.formulas.older_melee_damage {
        let attribute =
            if is_bow || weapon.map(|g| object(w, g).weapon_skill()) == Some(Skill::Dagger) {
                PropertyAttribute::Coordination
            } else {
                PropertyAttribute::Strength
            };
        let mut skill =
            crate::dispatch::get_current_weapon_skill::get_current_weapon_skill(w, this);
        if is_bow {
            // bows and crossbows together, thrown weapons apart
            skill = Skill::Bow;
        } else if skill == Skill::UnarmedCombat && !is_humanoid(w, this) {
            // a creature that cannot wield weapons keeps the usual factor
            skill = Skill::None;
        }
        let current: i32 = attribute_current(w, this, attribute).cs_cast();
        return skill_formula::get_attribute_mod_for_skill(current, skill);
    }

    //var attribute = isBow || GetCurrentWeaponSkill() == Skill.FinesseWeapons ? Coordination : Strength;
    let attribute =
        if is_bow || weapon.map(|g| object(w, g).weapon_skill()) == Some(Skill::FinesseWeapons) {
            PropertyAttribute::Coordination
        } else {
            PropertyAttribute::Strength
        };

    let current: i32 = attribute_current(w, this, attribute).cs_cast();
    skill_formula::get_attribute_mod(current, is_bow)
}

/// Not ACE: whether the creature can wield weapons: a player, or a creature allowed a combat
/// style (ClassicACE's `IsHumanoid`).
// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature.cs
#[must_use]
pub fn is_humanoid(w: &World, this: ObjectGuid) -> bool {
    is_player(w, this) || object(w, this).ai_allowed_combat_style().0 != 0
}

/// Not ACE: the damage an unarmed humanoid adds to its maximum from its Unarmed Combat skill, a
/// twentieth of it, before the weapon-skill consolidation (`EraFormulas::older_melee_damage`);
/// 0 otherwise (ClassicACE's `GetUnarmedSkillDamageBonus` at its Infiltration ruleset).
// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
pub fn get_unarmed_skill_damage_bonus(w: &mut World, this: ObjectGuid) -> i32 {
    if !w.era.formulas.older_melee_damage || !is_humanoid(w, this) {
        return 0;
    }
    if crate::dispatch::get_current_weapon_skill::get_current_weapon_skill(w, this)
        != Skill::UnarmedCombat
    {
        return 0;
    }
    let skill = skill_of(w, this, Skill::UnarmedCombat).current(w);
    (skill / 20).cs_cast()
}

/// Returns the current weapon skill for non-player creatures (converted to the post-MoA skill
/// when the creature has none of the weapon's own).
// ACE: Creature.GetCurrentWeaponSkill
pub fn get_current_weapon_skill(w: &mut World, this: ObjectGuid) -> Skill {
    let weapon = creature_equipment::get_equipped_weapon(w, this, false);

    let mut skill = weapon.map_or(Skill::UnarmedCombat, |g| object(w, g).weapon_skill());

    // DIVERGE: an era before the 2012 weapon-skill consolidation (`EraFeatures::
    // consolidated_weapon_skills`) attacks with the creature's highest old skill of the weapon's
    // kind, missile or melee (ClassicACE's `GetCurrentWeaponSkill` for its older rulesets).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
    if !w.era.features.consolidated_weapon_skills {
        let candidates: &[Skill] = if weapon.is_some_and(|g| object(w, g).is_ranged()) {
            &[Skill::Bow, Skill::Crossbow, Skill::ThrownWeapon]
        } else {
            &[
                Skill::Axe,
                Skill::Dagger,
                Skill::Mace,
                Skill::Spear,
                Skill::Staff,
                Skill::Sword,
                Skill::UnarmedCombat,
            ]
        };
        let mut best = candidates[0];
        let mut best_current = skill_of(w, this, best).current(w);
        for &s in &candidates[1..] {
            let current = skill_of(w, this, s).current(w);
            if current > best_current {
                best = s;
                best_current = current;
            }
        }
        return best;
    }

    let creature_skill = skill_of(w, this, skill);

    if creature_skill.init_level(w) == 0 {
        // convert to post-MoA skill
        skill = if weapon.is_some_and(|g| object(w, g).is_ranged()) {
            Skill::MissileWeapons
        } else if skill == Skill::Sword {
            Skill::HeavyWeapons
        } else if skill == Skill::Dagger {
            Skill::FinesseWeapons
        } else {
            Skill::LightWeapons
        };
    }

    //Console.WriteLine("Monster weapon skill: " + skill);

    skill
}

/// Returns the effective attack skill for a non-player creature, ie. with Heart Seeker bonus.
// ACE: Creature.GetEffectiveAttackSkill
pub fn get_effective_attack_skill(w: &mut World, this: ObjectGuid) -> u32 {
    let attack = crate::dispatch::get_current_attack_skill::get_current_attack_skill(w, this);
    let attack_skill = skill_current(w, this, attack);

    // TODO: don't use for bow?
    // https://asheron.fandom.com/wiki/Developer_Chat_-_2002/09/23
    let offense_mod = weapon_mod::get_weapon_offense_modifier(w, Some(this));

    // monsters don't use accuracy mod?

    round_to_uint(attack_skill as f32 * offense_mod)
}

/// Returns the effective defense skill for a player or creature, ie. with Defender bonus and
/// imbues.
// ACE: Creature.GetEffectiveDefenseSkill
pub fn get_effective_defense_skill(
    w: &mut World,
    this: ObjectGuid,
    combat_type: CombatType,
) -> u32 {
    let defense_skill = if combat_type == CombatType::Missile {
        Skill::MissileDefense
    } else {
        Skill::MeleeDefense
    };
    let defense_mod = if defense_skill == Skill::MissileDefense {
        weapon_mod::get_weapon_missile_defense_modifier(w, this)
    } else {
        weapon_mod::get_weapon_melee_defense_modifier(w, Some(this))
    };
    let burden_mod = crate::dispatch::get_burden_mod::get_burden_mod(w, this);

    let imbued_effect_type = if defense_skill == Skill::MissileDefense {
        ImbuedEffectType::MissileDefense
    } else {
        ImbuedEffectType::MeleeDefense
    };
    let defense_imbues = get_defense_imbues(w, this, imbued_effect_type);

    let stance_mod = if is_player(w, this) {
        crate::world_objects::player_combat::get_defense_stance_mod(w, this)
    } else {
        1.0f32
    };

    //if (this is Player)
    //Console.WriteLine($"StanceMod: {stanceMod}");

    let current = skill_current(w, this, defense_skill);
    let mut effective_defense = round_to_uint(
        current as f32 * defense_mod * burden_mod * stance_mod + defense_imbues as f32,
    );

    if crate::world_objects::creature::is_exhausted(w.objects.get(this).expect("ACE: this is null"))
    {
        effective_defense = 0;
    }

    effective_defense
}

// ACE: Creature.MinAttackSpeed
const MIN_ATTACK_SPEED: f64 = 0.5;
// ACE: Creature.MaxAttackSpeed
const MAX_ATTACK_SPEED: f64 = 2.0;

/// Returns the animation speed for an attack, based on the current quickness and weapon speed.
// ACE: Creature.GetAnimSpeed
pub fn get_anim_speed(w: &mut World, this: ObjectGuid) -> f32 {
    let quickness = attribute_current(w, this, PropertyAttribute::Quickness);
    let weapon_speed = weapon_mod::get_weapon_speed(w, Some(this));

    let divisor = 1.0 - (f64::from(quickness) / 300.0) + (f64::from(weapon_speed) / 150.0);
    if divisor <= 0.0 {
        return MAX_ATTACK_SPEED as f32;
    }

    math_clamp_f64(1.0 / divisor, MIN_ATTACK_SPEED, MAX_ATTACK_SPEED) as f32
}

/// Called when a creature evades an attack (the empty base for non-player creatures).
// ACE: Creature.OnEvade
pub fn on_evade(
    _w: &mut World,
    _this: ObjectGuid,
    _attacker: ObjectGuid,
    _attack_type: CombatType,
) {
    // empty base for non-player creatures?
}

/// Called when a creature hits a target (the empty base for non-player creatures).
// ACE: Creature.OnDamageTarget
pub fn on_damage_target(
    _w: &mut World,
    _this: ObjectGuid,
    _target: ObjectGuid,
    _attack_type: CombatType,
    _critical: bool,
) {
    // empty base for non-player creatures?
}

/// Returns the current attack height as an enumerable string.
// ACE: Creature.GetAttackHeight
#[must_use]
pub fn get_attack_height(w: &World, this: ObjectGuid) -> Option<&'static str> {
    attack_height(w, this).and_then(AttackHeight::get_string)
}

/// Returns the splatter height for the current attack height.
// ACE: Creature.GetSplatterHeight
#[must_use]
pub fn get_splatter_height(w: &World, this: ObjectGuid) -> &'static str {
    match attack_height(w, this) {
        None | Some(AttackHeight::Medium) => "Mid",
        Some(AttackHeight::Low) => "Low",
        Some(_) => "Up",
    }
}

/// Returns the splatter direction quadrant string.
// ACE: Creature.GetSplatterDir
#[must_use]
pub fn get_splatter_dir(w: &World, this: ObjectGuid, target: ObjectGuid) -> String {
    use empyrean_entity::enums::Quadrant;
    let quadrant = world_object::get_relative_dir(w, this, target);

    let mut splatter_dir = String::from(if quadrant.contains(Quadrant::Left) {
        "Left"
    } else {
        "Right"
    });
    splatter_dir += if quadrant.contains(Quadrant::Front) {
        "Front"
    } else {
        "Back"
    };

    splatter_dir
}

// ACE: Creature.GetLifeResistance
pub fn get_life_resistance(w: &mut World, this: ObjectGuid, damage_type: DamageType) -> f64 {
    match damage_type {
        DamageType::Slash
        | DamageType::Pierce
        | DamageType::Bludgeon
        | DamageType::Fire
        | DamageType::Cold
        | DamageType::Acid
        | DamageType::Electric
        | DamageType::Nether => {
            crate::world_objects::creature_properties::resist_mod(w, this, damage_type)
        }
        _ => 1.0,
    }
}

/// Reduces a creatures's attack skill while exhausted.
// ACE: Creature.GetExhaustedSkill
#[must_use]
pub fn get_exhausted_skill(attack_skill: u32) -> u32 {
    let half_skill = round_to_uint(attack_skill as f32 / 2.0f32);

    let max_penalty: u32 = 50;
    let reduced_skill = attack_skill.saturating_sub(max_penalty); // `attackSkill >= maxPenalty ? attackSkill - maxPenalty : 0`

    reduced_skill.max(half_skill)
}

/// Returns a divisor for the target height for aiming projectiles (the base).
// ACE: Creature.GetAimHeight
#[must_use]
pub fn get_aim_height(_w: &World, _this: ObjectGuid, _target: ObjectGuid) -> f32 {
    2.0
}

/// Return the scalar damage absorbed by a shield.
// ACE: Creature.GetShieldMod
pub fn get_shield_mod(
    w: &mut World,
    this: ObjectGuid,
    attacker: ObjectGuid,
    damage_type: DamageType,
    weapon: Option<ObjectGuid>,
) -> f32 {
    // ensure combat stance
    // DIVERGE: before the Shield skill (`EraFormulas::shields_without_skill`) a shield guards in
    // any stance (ClassicACE's `GetShieldMod` outside its end-of-retail ruleset).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
    let without_skill = w.era.formulas.shields_without_skill;
    if !without_skill && combat_mode(w, this) == CombatMode::NonCombat {
        return 1.0;
    }

    // does the player have a shield equipped?
    let Some(shield) = creature_equipment::get_equipped_shield(w, this) else {
        return 1.0;
    };

    // phantom weapons ignore all armor and shields
    if weapon.is_some_and(|g| {
        weapon_mod::has_imbued_effect(object(w, g), ImbuedEffectType::IgnoreAllArmor)
    }) {
        return 1.0;
    }

    // is monster in front of player,
    // within shield effectiveness area?
    let effective_angle = 180.0f32;
    let angle = crate::world_objects::creature_navigation::get_angle(w, this, attacker);
    if angle.abs() > effective_angle / 2.0 {
        return 1.0;
    }

    // get base shield AL
    let base_sl = object(w, shield)
        .get_property(PropertyInt::ArmorLevel)
        .map_or(0.0f32, |v| v as f32);

    // shield AL item enchantment additives:
    // impenetrability, brittlemail
    let ignore_magic_armor = weapon.is_some_and(|g| object(w, g).ignore_magic_armor())
        || object(w, attacker).ignore_magic_armor();

    let mut mod_sl = emc::get_armor_mod(w, shield);

    if ignore_magic_armor {
        mod_sl = if is_player(w, attacker) {
            math::round(f64::from(monster_melee::ignore_magic_armor_scaled(
                w,
                this,
                mod_sl as f32,
            )))
            .cs_cast()
        } else {
            0
        };
    }

    let effective_sl = base_sl + mod_sl as f32;

    // get shield RL against damage type
    let base_rl = monster_melee::get_resistance(object(w, shield), damage_type);

    // shield RL item enchantment additives:
    // banes, lures
    let mut mod_rl = emc::get_armor_mod_vs_type(w, shield, damage_type);

    if ignore_magic_armor {
        mod_rl = if is_player(w, attacker) {
            monster_melee::ignore_magic_armor_scaled(w, this, mod_rl)
        } else {
            0.0
        };
    }

    let mut effective_rl = (base_rl + f64::from(mod_rl)) as f32;

    // resistance clamp
    effective_rl = math_clamp_f32(effective_rl, -2.0, 2.0);

    // handle negative SL
    //if (effectiveSL < 0 && effectiveRL != 0)
    //effectiveRL = 1.0f / effectiveRL;

    let mut effective_level = effective_sl * effective_rl;

    // SL cap:
    // Trained / untrained: 1/2 shield skill
    // Spec: shield skill
    // SL cap is applied *after* item enchantments
    // DIVERGE: before the Shield skill (`EraFormulas::shields_without_skill`) nothing caps it
    // (ClassicACE's `GetSkillModifiedShieldLevel` at its Infiltration ruleset).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Melee.cs
    if !without_skill {
        let shield_skill = skill_of(w, this, Skill::Shield);
        let mut shield_cap = shield_skill.current(w);
        if shield_skill.advancement_class(w) != SkillAdvancementClass::Specialized {
            shield_cap = round_to_uint(shield_cap as f32 / 2.0f32);
        }

        effective_level = math::min_f32(effective_level, shield_cap as f32);
    }

    let ignore_shield_mod = weapon_mod::get_ignore_shield_mod(w, attacker, weapon);
    //Console.WriteLine($"IgnoreShieldMod: {ignoreShieldMod}");

    effective_level *= ignore_shield_mod;

    // SL is multiplied by existing AL
    //Console.WriteLine("ShieldMod: " + shieldMod);
    skill_formula::calc_armor_mod(effective_level)
}

/// Returns the total applicable Recklessness modifier, taking into account both attacker and
/// defender players.
// ACE: Creature.GetRecklessnessMod
pub fn get_recklessness_mod(w: &mut World, attacker: ObjectGuid, defender: ObjectGuid) -> f32 {
    let mut recklessness_mod = 1.0f32;

    // multiplicative or additive?
    // defender is a negative Damage Reduction Rating
    // 20 DR combined with 20 DRR = 1.2 * 0.8333... = 1.0
    // 20 DR combined with -20 DRR = 1.2 * 1.2 = 1.44
    if is_player(w, attacker) {
        recklessness_mod *= crate::world_objects::player_combat::get_recklessness_mod(w, attacker);
    }

    if is_player(w, defender) {
        recklessness_mod *= crate::world_objects::player_combat::get_recklessness_mod(w, defender);
    }

    recklessness_mod
}

/// The sneak attack damage modifier: always from behind, a Deception-scaled chance from the
/// front, reduced by the target's Assess Person.
// ACE: Creature.GetSneakAttackMod
pub fn get_sneak_attack_mod(w: &mut World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    // ensure trained
    let sneak_attack = skill_of(w, this, Skill::SneakAttack);
    if sneak_attack.advancement_class(w) < SkillAdvancementClass::Trained {
        return 1.0;
    }

    // ensure creature target
    if !is_creature(w, target) {
        return 1.0;
    }
    let creature_target = target;

    // Effects:
    // General Sneak Attack effects:
    //   - 100% chance to sneak attack from behind an opponent.
    //   - Deception trained: 10% chance to sneak attack from the front of an opponent
    //   - Deception specialized: 15% chance to sneak attack from the front of an opponent
    let angle = crate::world_objects::creature_navigation::get_angle(w, creature_target, this);
    let behind = angle.abs() > 90.0;
    let mut chance = 0.0f32;
    if behind {
        chance = 1.0;
    } else {
        let deception = skill_of(w, this, Skill::Deception);
        let sac = deception.advancement_class(w);
        if sac == SkillAdvancementClass::Trained {
            chance = 0.1;
        } else if sac == SkillAdvancementClass::Specialized {
            chance = 0.15;
        }

        // if Deception is below 306 skill, these chances are reduced proportionately.
        // this is in addition to proprtional reduction if your Sneak Attack skill is below your attack skill.
        let deception_cap = 306u32;
        if deception.current(w) < deception_cap {
            let current = deception.current(w);
            chance *= math::min_f32(current as f32 / deception_cap as f32, 1.0);
        }
    }
    //Console.WriteLine($"Sneak attack {(behind ? "behind" : "front")}, chance {Math.Round(chance * 100)}%");

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);
    if rng >= f64::from(chance) {
        return 1.0;
    }

    // Damage Rating:
    // Sneak Attack Trained:
    //   + 10 Damage Rating when Sneak Attack activates
    // Sneak Attack Specialized:
    //   + 20 Damage Rating when Sneak Attack activates
    let mut damage_rating =
        if sneak_attack.advancement_class(w) == SkillAdvancementClass::Specialized {
            20.0f32
        } else {
            10.0f32
        };

    // Sneak Attack works for melee, missile, and magic attacks.

    // if the Sneak Attack skill is lower than your attack skill (as determined by your equipped weapon)
    // then the damage rating is reduced proportionately. Because the damage rating caps at 10 for trained
    // and 20 for specialized, there is no reason to raise the skill above your attack skill
    let attack = crate::dispatch::get_current_attack_skill::get_current_attack_skill(w, this);
    let attack_skill = skill_of(w, this, attack);
    if sneak_attack.current(w) < attack_skill.current(w) {
        if attack_skill.current(w) > 0 {
            let s = sneak_attack.current(w);
            let a = attack_skill.current(w);
            damage_rating *= s as f32 / a as f32;
        } else {
            damage_rating = 0.0;
        }
    }

    // if the defender has Assess Person, they reduce the extra Sneak Attack damage Deception can add
    // from the front by up to 100%.
    // this percent is reduced proportionately if your buffed Assess Person skill is below the deception cap.
    // this reduction does not apply to attacks from behind.
    if !behind {
        // compare to assess person or deception??
        // wiki info is confusing here, it says 'your buffed Assess Person'
        // which sounds like its scaling sourceAssess / targetAssess,
        // but i think it should be targetAssess / deceptionCap?
        let target_assess = skill_current(w, creature_target, Skill::AssessPerson);

        let deception_cap = 306u32;
        damage_rating *= 1.0 - math::min_f32(target_assess as f32 / deception_cap as f32, 1.0);
    }

    //Console.WriteLine("SneakAttackMod: " + sneakAttackMod);
    (100.0 + damage_rating) / 100.0f32
}

/// Dirty Fighting: melee and missile attacks have a chance to weaken the opponent (low: defense
/// debuff, medium: bleed, high: attack and healing debuffs; doubled when specialized). A 25%
/// chance, reduced proportionally when the skill is below the weapon skill.
// ACE: Creature.FightDirty
pub fn fight_dirty(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    weapon: Option<ObjectGuid>,
) {
    // dirty fighting skill must be at least trained
    let dirty_skill = skill_of(w, this, Skill::DirtyFighting);
    if dirty_skill.advancement_class(w) < SkillAdvancementClass::Trained {
        return;
    }

    // ensure creature target
    if !is_creature(w, target) {
        return;
    }
    let creature_target = target;

    let mut chance = 0.25f32;

    let weapon_skill = crate::dispatch::get_current_weapon_skill::get_current_weapon_skill(w, this);
    let attack_skill = skill_of(w, this, weapon_skill);
    if dirty_skill.current(w) < attack_skill.current(w) {
        let d = dirty_skill.current(w);
        let a = attack_skill.current(w);
        chance *= d as f32 / a as f32;
    }

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);
    if rng >= f64::from(chance) {
        return;
    }

    match attack_height(w, this) {
        Some(AttackHeight::Low) => fight_dirty_apply_low_attack(w, this, creature_target, weapon),
        Some(AttackHeight::Medium) => {
            fight_dirty_apply_medium_attack(w, this, creature_target, weapon)
        }
        Some(AttackHeight::High) => fight_dirty_apply_high_attack(w, this, creature_target, weapon),
        _ => {}
    }
}

/// The DF spell for this creature's Dirty Fighting class.
fn dirty_fighting_spell(
    w: &mut World,
    this: ObjectGuid,
    specialized: SpellId,
    trained: SpellId,
) -> Spell {
    let dirty = skill_of(w, this, Skill::DirtyFighting);
    let spell_id = if dirty.advancement_class(w) == SkillAdvancementClass::Specialized {
        specialized
    } else {
        trained
    };
    Spell::from_spell_id(w, spell_id, true)
}

/// `target.EnchantmentManager.Add(spell, this, weapon)` and, for a player target, the
/// `MagicUpdateEnchantment` event.
fn add_and_notify(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    spell: &Spell,
    weapon: Option<ObjectGuid>,
) {
    let add_result = emc::add(w, target, spell, Some(this), weapon, false, false);

    if is_player(w, target) {
        shim::send_update_enchantment(w, target, add_result.enchantment.as_ref());
    }
}

/// Reduces the defense skills of the opponent by -10 if trained, or -20 if specialized.
// ACE: Creature.FightDirty_ApplyLowAttack
pub fn fight_dirty_apply_low_attack(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    weapon: Option<ObjectGuid>,
) {
    let spell = dirty_fighting_spell(
        w,
        this,
        SpellId::DF_Specialized_DefenseDebuff,
        SpellId::DF_Trained_DefenseDebuff,
    );
    if spell.not_found() {
        return; // TODO: friendly message to install DF patch
    }

    add_and_notify(w, this, target, &spell, weapon);

    let msg = game_message_script(target, PlayScript::DirtyFightingDefenseDebuff, 1.0);
    enqueue_broadcast(w, target, &[msg]);

    fight_dirty_send_message(w, this, target, &spell);
}

/// Applies bleed ticks for 60 damage per 20 seconds if trained, 120 damage per 20 seconds if
/// specialized.
// ACE: Creature.FightDirty_ApplyMediumAttack
pub fn fight_dirty_apply_medium_attack(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    weapon: Option<ObjectGuid>,
) {
    let spell = dirty_fighting_spell(
        w,
        this,
        SpellId::DF_Specialized_Bleed,
        SpellId::DF_Trained_Bleed,
    );
    if spell.not_found() {
        return; // TODO: friendly message to install DF patch
    }

    add_and_notify(w, this, target, &spell, weapon);

    // only send if not already applied?
    let msg = game_message_script(target, PlayScript::DirtyFightingDamageOverTime, 1.0);
    enqueue_broadcast(w, target, &[msg]);

    fight_dirty_send_message(w, this, target, &spell);
}

/// Reduces the attack skills and healing rating for opponent by -10 if trained, or -20 if
/// specialized.
// ACE: Creature.FightDirty_ApplyHighAttack
pub fn fight_dirty_apply_high_attack(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    weapon: Option<ObjectGuid>,
) {
    // attack debuff
    let spell = dirty_fighting_spell(
        w,
        this,
        SpellId::DF_Specialized_AttackDebuff,
        SpellId::DF_Trained_AttackDebuff,
    );
    if spell.not_found() {
        return; // TODO: friendly message to install DF patch
    }

    add_and_notify(w, this, target, &spell, weapon);

    let msg = game_message_script(target, PlayScript::DirtyFightingAttackDebuff, 1.0);
    enqueue_broadcast(w, target, &[msg]);

    fight_dirty_send_message(w, this, target, &spell);

    // healing resistance rating
    let spell = dirty_fighting_spell(
        w,
        this,
        SpellId::DF_Specialized_HealingDebuff,
        SpellId::DF_Trained_HealingDebuff,
    );
    if spell.not_found() {
        return; // TODO: friendly message to install DF patch
    }

    add_and_notify(w, this, target, &spell, weapon);

    let msg = game_message_script(target, PlayScript::DirtyFightingHealDebuff, 1.0);
    enqueue_broadcast(w, target, &[msg]);

    fight_dirty_send_message(w, this, target, &spell);
}

// ACE: Creature.FightDirty_SendMessage
pub fn fight_dirty_send_message(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    spell: &Spell,
) {
    // Dirty Fighting! <Player> delivers a <sic> Unbalancing Blow to <target>!
    //var article = spellBase.Name.StartsWithVowel() ? "an" : "a";

    let msg = format!(
        "Dirty Fighting! {} delivers a {} to {}!",
        shim::name(w, this),
        spell.name(),
        shim::name(w, target)
    );

    if is_player(w, this) {
        crate::world_objects::player::send_message_from(
            w,
            this,
            &msg,
            ChatMessageType::Combat,
            Some(this),
        );
    }
    if is_player(w, target) {
        crate::world_objects::player::send_message_from(
            w,
            target,
            &msg,
            ChatMessageType::Combat,
            Some(this),
        );
    }
}

/// Returns TRUE if the creature receives a +5 DR bonus for this weapon type (players only).
// ACE: Creature.GetHeritageBonus
#[must_use]
pub fn get_heritage_bonus(_w: &World, _this: ObjectGuid, _weapon: Option<ObjectGuid>) -> bool {
    // only for players
    false
}

// ACE: Creature.GetResistanceType
/// Returns a ResistanceType for a DamageType.
#[must_use]
pub fn get_resistance_type(damage_type: DamageType) -> ResistanceType {
    match damage_type {
        DamageType::Slash => ResistanceType::Slash,
        DamageType::Pierce => ResistanceType::Pierce,
        DamageType::Bludgeon => ResistanceType::Bludgeon,
        DamageType::Fire => ResistanceType::Fire,
        DamageType::Cold => ResistanceType::Cold,
        DamageType::Acid => ResistanceType::Acid,
        DamageType::Electric => ResistanceType::Electric,
        DamageType::Nether => ResistanceType::Nether,
        DamageType::Health => ResistanceType::HealthDrain,
        DamageType::Stamina => ResistanceType::StaminaDrain,
        DamageType::Mana => ResistanceType::ManaDrain,
        _ => ResistanceType::Undef,
    }
}

/// Whether this (non-player) creature can damage `target`: always a player; pets and PK-only
/// creatures, faction mobs and foe types otherwise.
// ACE: Creature.CanDamage
#[must_use]
pub fn can_damage(w: &World, this: ObjectGuid, target: ObjectGuid) -> bool {
    if is_player(w, target) {
        // monster attacking player
        return true; // other checks handled elsewhere
    }

    // monster attacking monster
    let source_pet = object(w, this).is_combat_pet();
    let target_pet = object(w, target).is_combat_pet();

    if source_pet || target_pet {
        if source_pet && target_pet {
            // combat pets can't damage other pets
            return false;
        } else if source_pet && object(w, target).player_killer_status() == PlayerKillerStatus::PK
            || target_pet && object(w, this).player_killer_status() == PlayerKillerStatus::PK
        {
            // combat pets can't damage pk-only creatures (ie. faction banners)
            return false;
        }
        return true;
    }

    // faction mobs
    if (object(w, this).faction1_bits().is_some() || object(w, target).faction1_bits().is_some())
        && allow_faction_combat(w, this, target)
    {
        return true;
    }

    // handle FoeType
    if potential_foe(w, this, target) {
        return true;
    }

    false
}

// ACE: Creature.GetDefenseSkill
#[must_use]
pub fn get_defense_skill(combat_type: CombatType) -> Skill {
    match combat_type {
        CombatType::Melee => Skill::MeleeDefense,
        CombatType::Missile => Skill::MissileDefense,
        CombatType::Magic => Skill::MagicDefense,
    }
}

/// If one of these fields is set, potential aggro from Player or CombatPet movement terminates
/// immediately.
// ACE: Creature.PlayerCombatPet_MoveExclude
const PLAYER_COMBAT_PET_MOVE_EXCLUDE: Tolerance = Tolerance(
    Tolerance::NoAttack.0
        | Tolerance::Appraise.0
        | Tolerance::Provoke.0
        | Tolerance::Retaliate.0
        | Tolerance::Monster.0,
);

/// If one of these fields is set, potential aggro from other monster movement terminates
/// immediately.
// ACE: Creature.Monster_MoveExclude
const MONSTER_MOVE_EXCLUDE: Tolerance = Tolerance(
    Tolerance::NoAttack.0 | Tolerance::Appraise.0 | Tolerance::Provoke.0 | Tolerance::Retaliate.0,
);

/// If one of these fields is set, potential aggro from Player or CombatPet attacks terminates
/// immediately.
// ACE: Creature.PlayerCombatPet_RetaliateExclude
pub const PLAYER_COMBAT_PET_RETALIATE_EXCLUDE: Tolerance =
    Tolerance(Tolerance::NoAttack.0 | Tolerance::Monster.0);

/// If one of these fields is set, potential aggro from monster alerts terminates immediately.
// ACE: Creature.AlertExclude
pub const ALERT_EXCLUDE: Tolerance = Tolerance(Tolerance::NoAttack.0 | Tolerance::Provoke.0);

/// Wakes up a monster if it can be alerted.
// ACE: Creature.AlertMonster
pub fn alert_monster(w: &mut World, this: ObjectGuid, monster: ObjectGuid) -> bool {
    // currently used for proximity checking exclusively:

    // Player_Monster.CheckMonsters() - player movement
    // Monster_Awareness.CheckTargets_Inner() - monster spawning in
    // Monster_Awareness.FactionMob_CheckMonsters() - faction mob scanning

    // non-attackable creatures do not get aggroed,
    // unless they have a TargetingTactic, such as the invisible archers in Oswald's Dirk Quest
    let m = object(w, monster);
    if !m.attackable() && m.targeting_tactic() == TargetingTactic::None {
        return false;
    }

    // ensure monster is currently in idle state to wake up,
    // and it has no tolerance to players running nearby
    // TODO: investigate usage for tolerance
    let tolerance = if is_player(w, this) {
        PLAYER_COMBAT_PET_MOVE_EXCLUDE
    } else {
        MONSTER_MOVE_EXCLUDE
    };

    if monster::monster_state(w, monster) != State::Idle
        || (object(w, monster).tolerance() & tolerance) != Tolerance(0)
    {
        return false;
    }

    // for faction mobs, ensure alerter doesn't belong to same faction
    if same_faction(w, this, monster) && !potential_foe(w, this, monster) {
        return false;
    }

    // add to retaliate targets?

    //Console.WriteLine($"[{Timers.RunningTime}] - {monster.Name} ({monster.Guid}) - waking up");
    monster_combat::set_attack_target(w, monster, Some(this));
    monster_awareness::wake_up(w, monster, true);

    true
}

/// Returns the damage type for the currently equipped weapon / ammo (`GetDamageType(bool
/// multiple = false, CombatType? combatType = null)`, the non-player version).
// ACE: Creature.GetDamageType
#[must_use]
pub fn get_damage_type(
    w: &World,
    this: ObjectGuid,
    multiple: bool,
    combat_type: Option<CombatType>,
) -> DamageType {
    // old method, keeping intact for monsters
    let weapon = creature_equipment::get_equipped_weapon(w, this, false);
    let ammo = creature_equipment::get_equipped_ammo(w, this);

    let Some(weapon) = weapon else {
        return DamageType::Bludgeon;
    };

    let combat_type = combat_type.unwrap_or_else(|| get_combat_type(w, this));

    let damage_source = match ammo {
        Some(ammo) if combat_type != CombatType::Melee && object(w, weapon).is_ammo_launcher() => {
            ammo
        }
        _ => weapon,
    };

    let damage_types = object(w, damage_source).w_damage_type();

    // returning multiple damage types
    if multiple {
        return damage_types;
    }

    // get single damage type
    let motion = object(w, this)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .expect("System.NullReferenceException: CurrentMotionState")
        .motion_state
        .forward_command
        .to_dotnet_string();
    for &damage_type in DamageType::ALL {
        if (damage_types & damage_type) != DamageType::Undef && !damage_type.is_multi_damage() {
            // handle multiple damage types
            if damage_type == DamageType::Slash && motion.contains("Thrust") {
                continue;
            }

            return damage_type;
        }
    }
    damage_types
}

/// Flag indicates which overpower formula is used: true = Formula A (ratings method), false =
/// Formula B (critical defense method).
// ACE: Creature.OverpowerMethod
pub const OVERPOWER_METHOD: bool = false;

// ACE: Creature.GetOverpower
#[must_use]
pub fn get_overpower(w: &World, attacker: ObjectGuid, defender: ObjectGuid) -> bool {
    if OVERPOWER_METHOD {
        get_overpower_method_a(w, attacker, defender)
    } else {
        get_overpower_method_b(w, attacker, defender)
    }
}

// ACE: Creature.GetOverpower_Method_A
#[must_use]
pub fn get_overpower_method_a(w: &World, attacker: ObjectGuid, defender: ObjectGuid) -> bool {
    // implemented similar to ratings
    let Some(mut overpower_chance) = object(w, attacker).overpower() else {
        return false;
    };

    if let Some(resist) = object(w, defender).overpower_resist() {
        overpower_chance = overpower_chance.wrapping_sub(resist);
    }

    //Console.WriteLine($"Overpower chance: {GetOverpowerChance_Method_A(attacker, defender)}");

    if overpower_chance <= 0 {
        return false;
    }

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    rng < f64::from(overpower_chance as f32 * 0.01f32)
}

// ACE: Creature.GetOverpower_Method_B
#[must_use]
pub fn get_overpower_method_b(w: &World, attacker: ObjectGuid, defender: ObjectGuid) -> bool {
    // implemented similar to critical defense
    let Some(overpower_chance) = object(w, attacker).overpower() else {
        return false;
    };

    //Console.WriteLine($"Overpower chance: {GetOverpowerChance_Method_B(attacker, defender)}");

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    if rng >= f64::from(overpower_chance as f32 * 0.01f32) {
        return false;
    }

    let Some(resist_chance) = object(w, defender).overpower_resist() else {
        return true;
    };

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    rng >= f64::from(resist_chance as f32 * 0.01f32)
}

// ACE: Creature.GetOverpowerChance
#[must_use]
pub fn get_overpower_chance(w: &World, attacker: ObjectGuid, defender: ObjectGuid) -> f32 {
    if OVERPOWER_METHOD {
        get_overpower_chance_method_a(w, attacker, defender)
    } else {
        get_overpower_chance_method_b(w, attacker, defender)
    }
}

// ACE: Creature.GetOverpowerChance_Method_A
#[must_use]
pub fn get_overpower_chance_method_a(w: &World, attacker: ObjectGuid, defender: ObjectGuid) -> f32 {
    let Some(mut overpower_chance) = object(w, attacker).overpower() else {
        return 0.0;
    };

    if let Some(resist) = object(w, defender).overpower_resist() {
        overpower_chance = overpower_chance.wrapping_sub(resist);
    }

    if overpower_chance <= 0 {
        return 0.0;
    }

    overpower_chance as f32 * 0.01f32
}

// ACE: Creature.GetOverpowerChance_Method_B
#[must_use]
pub fn get_overpower_chance_method_b(w: &World, attacker: ObjectGuid, defender: ObjectGuid) -> f32 {
    let Some(overpower) = object(w, attacker).overpower() else {
        return 0.0;
    };

    let overpower_chance = overpower as f32 * 0.01f32;
    let overpower_resist_chance =
        object(w, defender).overpower_resist().unwrap_or(0) as f32 * 0.01f32;

    overpower_chance * (1.0 - overpower_resist_chance)
}

/// Returns the number of equipped items with a particular imbue type.
// ACE: Creature.GetDefenseImbues
#[must_use]
pub fn get_defense_imbues(
    w: &World,
    this: ObjectGuid,
    imbued_effect_type: ImbuedEffectType,
) -> i32 {
    let n = creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .filter(|&i| object(w, i).imbued_effect().contains(imbued_effect_type))
        .count();
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// Returns the cloak the creature has equipped, or `None`.
// ACE: Creature.EquippedCloak
#[must_use]
pub fn equipped_cloak(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    use empyrean_entity::enums::EquipMask;
    creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .find(|&i| object(w, i).valid_locations() == Some(EquipMask::Cloak))
}

/// Returns TRUE if creature has cloak equipped.
// ACE: Creature.HasCloakEquipped
#[must_use]
pub fn has_cloak_equipped(w: &World, this: ObjectGuid) -> bool {
    equipped_cloak(w, this).is_some()
}

/// Called when a monster attacks another monster (differing factions, or FoeType).
// ACE: Creature.MonsterOnAttackMonster
pub fn monster_on_attack_monster(w: &mut World, this: ObjectGuid, monster: ObjectGuid) {
    // when a faction mob attacks a regular mob, the regular mob will retaliate against the faction mob
    let self_bits = object(w, this).faction1_bits();
    let their_bits = object(w, monster).faction1_bits();
    if let Some(bits) = self_bits {
        if their_bits.is_none_or(|t| (bits & t) == FactionBits(0)) {
            add_retaliate_target(w, monster, this);
        }
    }

    // when a monster with a FoeType attacks a foe, the foe will retaliate
    if object(w, this).foe_type().is_some() {
        let their_foe = object(w, monster).foe_type();
        if their_foe.is_none() || their_foe != object(w, this).creature_type() {
            add_retaliate_target(w, monster, this);
        }
    }

    if monster::monster_state(w, monster) == State::Idle
        && !object(w, monster).tolerance().contains(Tolerance::NoAttack)
    {
        monster_combat::set_attack_target(w, monster, Some(this));
        monster_awareness::wake_up(w, monster, true);
    }
}

/// Returns TRUE if creatures are both in the same faction.
// ACE: Creature.SameFaction
#[must_use]
pub fn same_faction(w: &World, this: ObjectGuid, creature: ObjectGuid) -> bool {
    match (
        object(w, this).faction1_bits(),
        object(w, creature).faction1_bits(),
    ) {
        (Some(a), Some(b)) => (a & b) != FactionBits(0),
        _ => false,
    }
}

/// Returns TRUE is this creature has a FoeType that matches the input creature's CreatureType,
/// or if the input creature has a FoeType that matches this creature's CreatureType.
// ACE: Creature.PotentialFoe
#[must_use]
pub fn potential_foe(w: &World, this: ObjectGuid, creature: ObjectGuid) -> bool {
    let a = object(w, this);
    let b = object(w, creature);
    a.foe_type().is_some() && a.foe_type() == b.creature_type()
        || b.foe_type().is_some() && b.foe_type() == a.creature_type()
}

// ACE: Creature.AllowFactionCombat
#[must_use]
pub fn allow_faction_combat(w: &World, this: ObjectGuid, creature: ObjectGuid) -> bool {
    let (a, b) = (
        object(w, this).faction1_bits(),
        object(w, creature).faction1_bits(),
    );
    if a.is_none() && b.is_none() {
        return false;
    }

    let faction_self = a.unwrap_or(FactionBits::None);
    let faction_other = b.unwrap_or(FactionBits::None);

    (faction_self & faction_other) == FactionBits(0)
}

/// # Panics
/// Without physics bodies (ACE: `NullReferenceException` on `PhysicsObj`).
// ACE: Creature.AddRetaliateTarget
pub fn add_retaliate_target(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    let (Some(a), Some(b)) = (object(w, this).phys, object(w, wo).phys) else {
        panic!("System.NullReferenceException: PhysicsObj");
    };
    crate::physics::object_maint::add_retaliate_target(w, a, b);
}

/// # Panics
/// Without a physics body (ACE: `NullReferenceException` on `PhysicsObj`).
// ACE: Creature.HasRetaliateTarget
#[must_use]
pub fn has_retaliate_target(w: &World, this: ObjectGuid, wo: Option<ObjectGuid>) -> bool {
    match wo {
        Some(wo) => {
            let a = object(w, this)
                .phys
                .expect("System.NullReferenceException: PhysicsObj");
            crate::physics::object_maint::retaliate_targets_contains_key(w, a, wo.full())
        }
        None => false,
    }
}

/// # Panics
/// Without a physics body (ACE: `NullReferenceException` on `PhysicsObj`).
// ACE: Creature.ClearRetaliateTargets
pub fn clear_retaliate_targets(w: &mut World, this: ObjectGuid) {
    let a = object(w, this)
        .phys
        .expect("System.NullReferenceException: PhysicsObj");
    crate::physics::object_maint::clear_retaliate_targets(w, a);
}

/// `Math.Clamp(float, float, float)` (min <= max here): a NaN passes through.
fn math_clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `Math.Clamp(double, double, double)` (min <= max here).
fn math_clamp_f64(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

// ---- virtual-dispatch targets ----

// ACE: Creature.GetCombatType
/// The virtual target: [`get_combat_type`].
pub fn creature_get_combat_type(w: &crate::World, this: empyrean_entity::ObjectGuid) -> CombatType {
    get_combat_type(w, this)
}

// ACE: Creature.GetPowerMod
/// Returns a value between 0.5-1.5 for non-bow attacks, depending on the power bar meter (1.0
/// for non-player creatures).
#[allow(unused_variables)]
pub fn creature_get_power_mod(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    weapon: empyrean_entity::ObjectGuid,
) -> f32 {
    // doesn't apply for non-player creatures?
    1.0
}

// ACE: Creature.GetAccuracyMod
/// Returns a value between 0.6-1.6 for bow attacks, depending on the accuracy meter (1.0 for
/// non-player creatures).
#[allow(unused_variables)]
pub fn creature_get_accuracy_mod(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    weapon: empyrean_entity::ObjectGuid,
) -> f32 {
    // doesn't apply for non-player creatures?
    1.0
}

// ACE: Creature.GetCurrentAttackSkill
/// Returns the current attack skill for this monster, given their stance and wielded weapon.
pub fn creature_get_current_attack_skill(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::Skill {
    crate::dispatch::get_current_weapon_skill::get_current_weapon_skill(w, this)
}

// ACE: Creature.GetCurrentWeaponSkill
pub fn creature_get_current_weapon_skill(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::Skill {
    get_current_weapon_skill(w, this)
}

// ACE: Creature.GetEffectiveAttackSkill
pub fn creature_get_effective_attack_skill(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> u32 {
    get_effective_attack_skill(w, this)
}

// ACE: Creature.OnEvade
/// The virtual target: [`on_evade`].
pub fn creature_on_evade(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    attacker: empyrean_entity::ObjectGuid,
    attack_type: CombatType,
) {
    on_evade(w, this, attacker, attack_type);
}

// ACE: Creature.OnDamageTarget
/// The virtual target: [`on_damage_target`].
pub fn creature_on_damage_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    attack_type: CombatType,
    critical: bool,
) {
    on_damage_target(w, this, target, attack_type, critical);
}

// ACE: Creature.GetAimHeight
pub fn creature_get_aim_height(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) -> f32 {
    get_aim_height(w, this, target)
}

// ACE: Creature.GetHeritageBonus
pub fn creature_get_heritage_bonus(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    weapon: empyrean_entity::ObjectGuid,
) -> bool {
    get_heritage_bonus(w, this, Some(weapon))
}

// ACE: Creature.CanDamage
pub fn creature_can_damage(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) -> bool {
    can_damage(w, this, target)
}

// ACE: Creature.GetDamageType
/// The virtual target: [`get_damage_type`].
pub fn creature_get_damage_type(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    multiple: bool,
    combat_type: Option<CombatType>,
) -> empyrean_entity::enums::DamageType {
    get_damage_type(w, this, multiple, combat_type)
}

// ============================================================================== shims

pub mod shim;
