// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Magic.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Magic.cs`.
//!
//! The spell-effect engine: the instant casts (`TryCastSpell*`), the resist checks,
//! `HandleCastSpell` and its dispatch by `MetaSpellType` (enchantments through the caching
//! `EnchantmentManager`, boosts, transfers, projectiles, the portal spells and dispels), the
//! projectile launch geometry, the item-spell redirects and the DoT enchantment value.
//!
//! # Shape
//!
//! Every member is a free function `(w, this, ..)`. ACE's `WorldObject`
//! arguments are guids; a nullable one is `Option<ObjectGuid>`, and a guid that no longer resolves
//! reads as ACE's `null`. `this as Player` / `target is Creature` are the kind predicates.
//!
//! **Pointers.** Members of other ACE classes (creature vitals and skills, the player's
//! PK timer and teleports, `Cloak`, `DamageHistory`, `EmoteManager`, the `SpellProjectile` leaf,
//! ...) are reached through the functions at the bottom of this file, each named after ACE's
//! member; one whose member is not ported yet calls `not_ported!` with that name and answers the
//! value noted.
//!
//! **Static caches.** ACE's `ProjectileRadiusCache` and `ProjectileSpeedCache` are process-wide
//! `ConcurrentDictionary`s of pure functions of the world database; here they are per world
//! thread (arch, V143), so tests with different content never share entries.

use std::cell::RefCell;
use std::collections::HashMap;

use dereth_primitives::Position as PPosition;
use empyrean_common::dotnet::{math, to_string, CsCast, Quaternion, Vector3};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::properties::{
    PositionType, PropertyAttribute2nd, PropertyBool, PropertyDataId, PropertyFloat,
};
use empyrean_entity::enums::{
    ChatMessageType, DamageType, DispelType, EquipMask, ImbuedEffectType, ItemType, MagicSchool,
    PortalBitmask, ResistanceType, Skill, Sound, SpellCategory, SpellId, SpellType, TransferFlags,
    WeenieError, WeenieType,
};
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::add_enchantment_result::AddEnchantmentResult;
use crate::entity::aetheria;
use crate::entity::damage_history_info::DamageHistoryInfo;
use crate::entity::position_extensions;
use crate::entity::spell::Spell;
use crate::entity::spell_enchantment::SpellEnchantment;
use crate::entity::spell_projectile_type::ProjectileSpellType;
use crate::managers::player_manager::{self, player_session};
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_magic_update_enchantment::game_event_magic_update_enchantment;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::physics::trajectory;
use crate::physics::trajectory2;
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::entity::creature_vital::CreatureVital;
use crate::world_objects::managers::enchantment_manager as em;
use crate::world_objects::managers::enchantment_manager::StackType;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::world_object::{WorldObject, LOCAL_BROADCAST_RANGE};
use crate::World;

/// Non-property fields declared in `WorldObject_Magic.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectMagicFields {
    /// `GetMaxSpellLevel`'s cache.
    // ACE: WorldObject._maxSpellLevel
    pub max_spell_level: Option<i32>,
    // ACE: WorldObject.ItemManaRateAccumulator
    pub item_mana_rate_accumulator: f32,
    // ACE: WorldObject.ItemManaDepletionMessage
    pub item_mana_depletion_message: bool,
}

// ------------------------------------------------------------------------------------ helpers

fn obj(w: &World, g: ObjectGuid) -> Option<&WorldObject> {
    w.objects.get(g)
}

/// A guid that still resolves (ACE: a non-null reference).
fn live(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    g.filter(|&g| w.objects.get(g).is_some())
}

/// The target, if it still has a body in the world: one whose body the landblock destroyed (a
/// player whose log-off finished) counts as no target.
fn in_world(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    live(w, g).filter(|&g| obj(w, g).is_some_and(|o| o.phys.is_some()))
}

fn is_player(w: &World, g: ObjectGuid) -> bool {
    obj(w, g).is_some_and(WorldObject::is_player)
}

fn is_creature(w: &World, g: ObjectGuid) -> bool {
    obj(w, g).is_some_and(WorldObject::is_creature)
}

/// `x as Player`.
fn as_player(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    g.filter(|&g| is_player(w, g))
}

/// `x as Creature`.
fn as_creature(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    g.filter(|&g| is_creature(w, g))
}

/// `wo.Name` (the virtual property); an object that is gone reads as an empty name.
fn name_of(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `player.Session`. ACE's `player.Session.Network` throws `NullReferenceException` for a player
/// without a session.
fn session_of(w: &World, player: ObjectGuid) -> SessionId {
    player_session(w, player).expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `player.Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session = session_of(w, player);
    enqueue_send(w, session, msg);
}

/// `player.Session.Network.EnqueueSend(new GameEventWeenieError(player.Session, error))`.
fn send_weenie_error_event(w: &mut World, player: ObjectGuid, error: WeenieError) {
    let session = session_of(w, player);
    let msg = game_event_weenie_error(session_data(w, session), error);
    enqueue_send(w, session, msg);
}

/// `Enum.HasFlag`.
fn has_damage_flag(value: DamageType, flag: DamageType) -> bool {
    (value.0 & flag.0) == flag.0
}

fn has_transfer_flag(value: TransferFlags, flag: TransferFlags) -> bool {
    (value.0 & flag.0) == flag.0
}

/// `(float)uint`.
fn f32_of(v: u32) -> f32 {
    v.cs_cast()
}

/// `Math.Round(double)` then `(uint)`: .NET 10 saturates.
fn round_to_u32(v: f64) -> u32 {
    math::round(v).cs_cast()
}

/// `Math.Round(double)` then `(int)`.
fn round_to_i32(v: f64) -> i32 {
    math::round(v).cs_cast()
}

/// Component-wise `Vector3 * Vector3`.
fn mul_v(a: Vector3, b: Vector3) -> Vector3 {
    Vector3::new(a.x * b.x, a.y * b.y, a.z * b.z)
}

const UNIT_Z: Vector3 = Vector3::new(0.0, 0.0, 1.0);

/// `float.ToRadians()` (ACE.Server's physics extension): `(float)(Math.PI / 180.0f * angle)`.
fn to_radians(angle: f32) -> f32 {
    f32_of_f64(std::f64::consts::PI / f64::from(180.0f32) * f64::from(angle))
}

/// `(float)double`.
fn f32_of_f64(v: f64) -> f32 {
    v.cs_cast()
}

// ---------------------------------------------------------------------------------- casting

/// Instantly casts a spell for a WorldObject (ie. spell traps). `item_caster`, `weapon`,
/// `is_weapon_spell`, `from_proc` and `try_resist` default to null / null / false / false / true.
// ACE: WorldObject.TryCastSpell
#[allow(clippy::too_many_arguments)]
pub fn try_cast_spell(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    is_weapon_spell: bool,
    from_proc: bool,
    try_resist: bool,
) {
    // TODO: look into further normalizing this / caster / weapon

    // verify spell exists in database
    if spell.spell.is_none() {
        if let Some(target_player) = as_player(w, live(w, target)) {
            let msg = game_message_system_chat(
                &format!("{} spell not implemented, yet!", spell.name()),
                ChatMessageType::System,
            );
            send(w, target_player, msg);
        }

        return;
    }

    if spell.is_fellowship_spell() {
        let Some(target_player) = as_player(w, live(w, target)) else {
            return;
        };
        let Some(fellows) = player_fellowship_get_fellowship_members(w, target_player) else {
            return;
        };

        for fellow in fellows {
            try_cast_spell_inner(
                w,
                this,
                spell,
                Some(fellow),
                item_caster,
                weapon,
                is_weapon_spell,
                from_proc,
                try_resist,
            );
        }
    } else {
        try_cast_spell_inner(
            w,
            this,
            spell,
            target,
            item_caster,
            weapon,
            is_weapon_spell,
            from_proc,
            try_resist,
        );
    }
}

// ACE: WorldObject.TryCastSpell_Inner
#[allow(clippy::too_many_arguments)]
pub fn try_cast_spell_inner(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    is_weapon_spell: bool,
    from_proc: bool,
    try_resist: bool,
) {
    // verify before resist, still consumes source item
    if spell.meta_spell_type() == SpellType::Dispel
        && !verify_dispel_pk_status(w, item_caster, target)
    {
        return;
    }

    // perform resistance check, if applicable
    if try_resist && try_resist_spell(w, this, target, spell, item_caster, false) {
        return;
    }

    // if not resisted, cast spell
    handle_cast_spell(
        w,
        this,
        spell,
        target,
        item_caster,
        weapon,
        is_weapon_spell,
        from_proc,
        false,
    );
}

/// Instantly casts a spell for a WorldObject, with optional redirects for item enchantments.
// ACE: WorldObject.TryCastSpell_WithRedirects
#[allow(clippy::too_many_arguments)]
pub fn try_cast_spell_with_redirects(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    is_weapon_spell: bool,
    from_proc: bool,
    try_resist: bool,
) -> bool {
    if let Some(creature_target) = as_creature(w, live(w, target)) {
        let targets = get_non_component_target_types(w, spell, creature_target);

        if let Some(targets) = targets {
            for &item_target in &targets {
                try_cast_spell(
                    w,
                    this,
                    spell,
                    Some(item_target),
                    item_caster,
                    weapon,
                    is_weapon_spell,
                    from_proc,
                    try_resist,
                );
            }

            return !targets.is_empty();
        }
    }

    try_cast_spell(
        w,
        this,
        spell,
        target,
        item_caster,
        weapon,
        is_weapon_spell,
        from_proc,
        try_resist,
    );

    true
}

/// Determines whether a spell will be resisted, based upon the caster's magic skill vs target's
/// magic defense skill. Returns `(resisted, resistChance)`.
// ACE: WorldObject.MagicDefenseCheck
#[must_use]
pub fn magic_defense_check(
    caster_magic_skill: u32,
    target_magic_defense_skill: u32,
) -> (bool, f32) {
    // uses regular 0.03 factor, and not magic casting 0.07 factor
    let chance = crate::world_objects::skill_check::get_skill_chance(
        caster_magic_skill.cs_cast(),
        target_magic_defense_skill.cs_cast(),
        crate::world_objects::skill_check::DEFAULT_FACTOR,
    );
    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    let resist_chance: f32 = (f64::from(1.0f32) - chance).cs_cast();

    (chance <= rng, resist_chance)
}

/// If this spell has a chance to be resisted, rolls for a chance. Returns TRUE if spell is
/// resistable and was resisted for this attempt. `item_caster` and `projectile_hit` default to
/// null / false.
// ACE: WorldObject.TryResistSpell
pub fn try_resist_spell(
    w: &mut World,
    this: ObjectGuid,
    target: Option<ObjectGuid>,
    spell: &Spell,
    item_caster: Option<ObjectGuid>,
    projectile_hit: bool,
) -> bool {
    // fix hermetic void?
    if !spell.is_resistable() && spell.category() != SpellCategory::ManaConversionModLowering
        || spell.is_self_targeted()
    {
        //if (!spell.IsResistable || spell.IsSelfTargeted)
        return false;
    }

    if spell.meta_spell_type() == SpellType::Dispel
        && spell.align() == DispelType::Negative
        && !property_manager::get_bool(w, "allow_negative_dispel_resist", false, true).item
    {
        return false;
    }

    if spell.num_projectiles() > 0 && !projectile_hit {
        return false;
    }

    let item_caster = live(w, item_caster);
    if let Some(item_caster) = item_caster {
        if cloak_is_cloak(w, item_caster) {
            return false;
        }
    }

    let mut magic_skill: u32 = 0;

    let caster = item_caster.unwrap_or(this);

    let caster_creature = as_creature(w, Some(caster));

    if let Some(caster_creature) = caster_creature {
        // Retrieve caster's skill level in the Magic School
        magic_skill =
            creature_get_creature_skill_school_current(w, caster_creature, spell.school());
    } else if let Some(item_spellcraft) = obj(w, caster).and_then(WorldObject::item_spellcraft) {
        // Retrieve casting item's spellcraft
        magic_skill = item_spellcraft.cast_unsigned();
    } else if let Some(wielder) = as_creature(w, live(w, obj(w, caster).and_then(|c| c.wielder))) {
        // Receive wielder's skill level in the Magic School?
        magic_skill = creature_get_creature_skill_school_current(w, wielder, spell.school());
    }

    //Console.WriteLine($"Magic skill: {magicSkill}");

    // only creatures can resist spells?
    let Some(target_creature) = as_creature(w, live(w, target)) else {
        return false;
    };

    // Retrieve target's Magic Defense Skill
    let difficulty = creature_get_effective_magic_defense(w, target_creature);

    //Console.WriteLine($"{target.Name}.ResistSpell({Name}, {spell.Name}): magicSkill: {magicSkill}, difficulty: {difficulty}");
    let (mut resisted, resist_chance) = magic_defense_check(magic_skill, difficulty);

    let player = as_player(w, Some(this));
    let target_player = as_player(w, Some(target_creature));

    // Any invincible creature resists, not only a player.
    if obj(w, target_creature).is_some_and(WorldObject::invincible) {
        resisted = true;
    }

    if let Some(target_player) = target_player {
        if obj(w, target_player).is_some_and(WorldObject::under_lifestone_protection) {
            player_handle_lifestone_protection(w, target_player);
            resisted = true;
        }
    }

    if caster == target_creature {
        resisted = false;
    }

    if resisted {
        if let Some(player) = player {
            let text = format!("{} resists your spell", name_of(w, target_creature));
            player_send_chat_message(
                w,
                player,
                Some(target_creature),
                &text,
                ChatMessageType::Magic,
            );

            send(
                w,
                player,
                game_message_sound(player, Sound::ResistSpell, 1.0),
            );
        }

        if let Some(target_player) = target_player {
            let text = format!("You resist the spell cast by {}", name_of(w, this));
            player_send_chat_message(w, target_player, Some(this), &text, ChatMessageType::Magic);

            send(
                w,
                target_player,
                game_message_sound(target_player, Sound::ResistSpell, 1.0),
            );

            if let Some(caster_creature) = caster_creature {
                player_set_current_attacker(w, target_player, caster_creature);
            }

            proficiency_on_success_use(w, target_player, Skill::MagicDefense, magic_skill);
        }

        if is_creature(w, this) {
            emote_manager_on_resist_spell(w, target_creature, this);
        }
    }

    if let Some(player) = player {
        if creature_debug_damage(w, player).has_flag(DebugDamageType::Attacker) {
            show_resist_info(
                w,
                player,
                this,
                target_creature,
                spell,
                magic_skill,
                difficulty,
                resist_chance,
                resisted,
            );
        }
    }
    if creature_debug_damage(w, target_creature).has_flag(DebugDamageType::Defender) {
        show_resist_info(
            w,
            target_creature,
            this,
            target_creature,
            spell,
            magic_skill,
            difficulty,
            resist_chance,
            resisted,
        );
    }

    resisted
}

/// `Creature.DebugDamageType` (declared in `Creature_Combat.cs`), a `[Flags]` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DebugDamageType(pub i32);

#[allow(non_upper_case_globals)]
impl DebugDamageType {
    pub const None: Self = Self(0x0);
    pub const Attacker: Self = Self(0x1);
    pub const Defender: Self = Self(0x2);
    pub const All: Self = Self(0x3);

    /// `Enum.HasFlag`.
    #[must_use]
    pub fn has_flag(self, flag: Self) -> bool {
        (self.0 & flag.0) == flag.0
    }
}

/// Sends (or buffers) the resist diagnostics to the observer's debug target.
// ACE: WorldObject.ShowResistInfo
#[allow(clippy::too_many_arguments)]
pub fn show_resist_info(
    w: &mut World,
    observed: ObjectGuid,
    attacker: ObjectGuid,
    defender: ObjectGuid,
    spell: &Spell,
    attack_skill: u32,
    defense_skill: u32,
    resist_chance: f32,
    resisted: bool,
) {
    let target_info = creature_debug_damage_target(w, observed)
        .and_then(|g| player_manager::get_online_player(w, g));

    let Some(target_info) = target_info else {
        creature_set_debug_damage(w, observed, DebugDamageType::None);
        return;
    };

    // initial info / resist chance
    let mut info = format!("Attacker: {} ({})\n", name_of(w, attacker), attacker);
    info += &format!("Defender: {} ({})\n", name_of(w, defender), defender);

    info += "CombatType: Magic\n";

    info += &format!("Spell: {} ({})\n", spell.name(), spell.id());

    info += &format!("EffectiveAttackSkill: {attack_skill}\n");
    info += &format!("EffectiveDefenseSkill: {defense_skill}\n");

    info += &format!("ResistChance: {}\n", to_string(resist_chance));

    info += &format!("Resisted: {}", if resisted { "True" } else { "False" });

    if resisted || spell.num_projectiles() == 0 {
        send(
            w,
            target_info,
            game_message_system_chat(&info, ChatMessageType::Broadcast),
        );
    } else {
        player_set_debug_damage_buffer(w, target_info, format!("{info}\n"));
    }
}

/// Creates a spell based on MetaSpellType. `item_caster`, `weapon`, `is_weapon_spell`,
/// `from_proc` and `equip` default to null / null / false / false / false.
// ACE: WorldObject.HandleCastSpell
#[allow(clippy::too_many_arguments)]
pub fn handle_cast_spell(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    is_weapon_spell: bool,
    from_proc: bool,
    equip: bool,
) -> bool {
    let target = live(w, target);
    let item_caster = live(w, item_caster);

    let mut target_creature = if !spell.is_self_targeted() || spell.is_fellowship_spell() {
        as_creature(w, target)
    } else {
        as_creature(w, Some(this))
    };

    if obj(w, this).is_some_and(|o| o.is_gem() || o.is_food() || o.is_hook()) {
        target_creature = as_creature(w, target);
    }

    if spell.school() == MagicSchool::LifeMagic || spell.meta_spell_type() == SpellType::Dispel {
        // NonComponentTargetType should be 0 for untargeted spells.
        // Return if the spell type is targeted with no target defined or the target is already dead.
        let alive = target_creature.is_some_and(|t| creature_is_alive(w, t));
        if !alive
            && spell.non_component_target_type() != ItemType::None
            && spell.dispel_school() != MagicSchool::ItemEnchantment
        {
            return false;
        }
    }

    match spell.meta_spell_type() {
        SpellType::Enchantment | SpellType::FellowEnchantment => {
            let enchant_target = target_creature
                .or(target)
                .expect("ACE: CreateEnchantment target is null (NullReferenceException)");

            // TODO: replace with some kind of 'rootOwner unless equip' concept?
            if let Some(item_caster) = item_caster
                .filter(|&ic| equip || obj(w, ic).is_some_and(|o| o.is_gem() || o.is_food()))
            {
                create_enchantment(
                    w,
                    this,
                    enchant_target,
                    item_caster,
                    Some(item_caster),
                    spell,
                    equip,
                    false,
                    false,
                );
            } else {
                create_enchantment(
                    w,
                    this,
                    enchant_target,
                    this,
                    weapon,
                    spell,
                    equip,
                    false,
                    is_weapon_spell,
                );
            }
        }

        SpellType::Boost | SpellType::FellowBoost => {
            handle_cast_spell_boost(w, this, spell, target_creature);
        }

        SpellType::Transfer => {
            handle_cast_spell_transfer(w, this, spell, target_creature);
        }

        SpellType::Projectile | SpellType::LifeProjectile | SpellType::EnchantmentProjectile => {
            handle_cast_spell_projectile(
                w,
                this,
                spell,
                target_creature,
                item_caster,
                weapon,
                is_weapon_spell,
                from_proc,
            );
        }

        SpellType::PortalLink => {
            handle_cast_spell_portal_link(w, this, spell, target);
        }

        SpellType::PortalRecall => {
            handle_cast_spell_portal_recall(w, this, spell, target_creature);
        }

        SpellType::PortalSummon => {
            handle_cast_spell_portal_summon(w, this, spell, target_creature, item_caster);
        }

        SpellType::PortalSending => {
            handle_cast_spell_portal_sending(w, this, spell, target_creature, item_caster);
        }

        SpellType::FellowPortalSending => {
            handle_cast_spell_fellow_portal_sending(w, this, spell, target_creature, item_caster);
        }

        SpellType::Dispel | SpellType::FellowDispel => {
            let dispel_target = target_creature
                .or(target)
                .expect("ACE: HandleCastSpell_Dispel target is null (NullReferenceException)");
            handle_cast_spell_dispel(w, this, spell, dispel_target);
        }

        _ => {
            if is_player(w, this) {
                send(
                    w,
                    this,
                    game_message_system_chat("Spell not implemented, yet!", ChatMessageType::Magic),
                );
            }

            return false;
        }
    }

    // play spell effects
    do_spell_effects(w, this, spell, this, target, false);

    true
}

/// Plays the caster/target effects for a spell. `projectile_hit` defaults to false.
// ACE: WorldObject.DoSpellEffects
pub fn do_spell_effects(
    w: &mut World,
    _this: ObjectGuid,
    spell: &Spell,
    caster: ObjectGuid,
    target: Option<ObjectGuid>,
    projectile_hit: bool,
) {
    if spell.caster_effect().0 != 0 && (!spell.is_projectile() || !projectile_hit) {
        let msg = game_message_script(caster, spell.caster_effect(), spell.formula_ref().scale());
        world_object_enqueue_broadcast(w, caster, msg);
    }

    let target = live(w, target);
    if spell.target_effect().0 != 0 && (!spell.is_projectile() || projectile_hit) {
        if let Some(target) = target {
            let target_broadcaster =
                live(w, obj(w, target).and_then(|t| t.wielder)).unwrap_or(target);

            let msg =
                game_message_script(target, spell.target_effect(), spell.formula_ref().scale());
            world_object_enqueue_broadcast(w, target_broadcaster, msg);
        }
    }
}

/// Handles casting SpellType.Enchantment / FellowEnchantment spells; this is also called if
/// SpellType.EnchantmentProjectile successfully hits. `equip`, `from_proc` and `is_weapon_spell`
/// default to false.
// ACE: WorldObject.CreateEnchantment
#[allow(clippy::too_many_arguments)]
pub fn create_enchantment(
    w: &mut World,
    this: ObjectGuid,
    target: ObjectGuid,
    caster: ObjectGuid,
    weapon: Option<ObjectGuid>,
    spell: &Spell,
    equip: bool,
    from_proc: bool,
    is_weapon_spell: bool,
) {
    // weird itemCaster -> caster collapsing going on here -- fixme

    let player = as_player(w, Some(this));

    let mut caster = caster;
    let mut aetheria_proc = false;
    let mut cloak_proc = false;

    // technically unsafe, should be using fromProc
    let caster_proc_spell = obj(w, caster).and_then(WorldObject::proc_spell);
    if caster_proc_spell == Some(spell.id()) {
        let caster_is_aetheria = obj(w, caster)
            .is_some_and(|c| c.is_gem() && aetheria::is_aetheria(c.biota.weenie_class_id));
        if caster_is_aetheria {
            caster = this;
            aetheria_proc = true;
        } else if cloak_is_cloak(w, caster) {
            caster = this;
            cloak_proc = true;
        }
    } else if from_proc {
        // fromProc is assumed to be cloakProc currently
        // todo: change fromProc from bool to WorldObject
        // do we need separate concepts for itemCaster and fromProc objects?
        caster = this;
        cloak_proc = true;
    }

    // create enchantment
    let add_result: AddEnchantmentResult = emc::add(
        w,
        target,
        spell,
        Some(caster),
        weapon,
        equip,
        is_weapon_spell,
    );

    // build message
    let spell_name = |s: &Option<Spell>| {
        s.as_ref()
            .expect("ACE: AddEnchantmentResult spell is null (NullReferenceException)")
            .name()
            .to_owned()
    };
    let suffix = match add_result.stack_type {
        StackType::Surpass => format!(", surpassing {}", spell_name(&add_result.surpass_spell)),
        StackType::Refresh => format!(", refreshing {}", spell_name(&add_result.refresh_spell)),
        StackType::Surpassed => {
            format!(
                ", but it is surpassed by {}",
                spell_name(&add_result.surpassed_spell)
            )
        }
        _ => String::new(),
    };

    if aetheria_proc {
        let text = format!(
            "Aetheria surges on {} with the power of {}!",
            name_of(w, target),
            spell.name()
        );
        let message = game_message_system_chat(&text, ChatMessageType::Spellcasting);

        world_object_enqueue_broadcast_range(
            w,
            this,
            message,
            LOCAL_BROADCAST_RANGE,
            Some(ChatMessageType::Spellcasting),
        );
    } else if let Some(player) = player.filter(|_| !cloak_proc) {
        // TODO: replace with some kind of 'rootOwner unless equip' concept?
        // for item casters where the message should be 'You cast', we still need pass the caster as item
        // down this far, to prevent using player's AugmentationIncreasedSpellDuration
        let caster_check =
            caster == this || obj(w, caster).is_some_and(|c| c.is_gem() || c.is_food());

        if caster_check || target == this || caster != target {
            let caster_name = if caster_check {
                "You".to_owned()
            } else {
                name_of(w, caster)
            };
            let mut target_name = name_of(w, target);
            if target == this {
                target_name = if caster_check {
                    "yourself".to_owned()
                } else {
                    "you".to_owned()
                };
            }

            let text = format!(
                "{caster_name} cast {} on {target_name}{suffix}",
                spell.name()
            );
            player_send_chat_message(w, player, Some(player), &text, ChatMessageType::Magic);
        }
    }

    let mut player_target = as_player(w, Some(target));

    if let Some(player_target) = player_target {
        let entry = add_result
            .enchantment
            .clone()
            .expect("ACE: AddEnchantmentResult.Enchantment is null (NullReferenceException)");
        let enchantment = em::new_enchantment(w, player_target, &entry);
        let session = session_of(w, player_target);
        let msg = game_event_magic_update_enchantment(session_data(w, session), &enchantment);
        enqueue_send(w, session, msg);

        player_handle_spell_hooks(w, player_target, spell);

        if !spell.is_beneficial() && is_creature(w, this) {
            player_set_current_attacker(w, player_target, this);
        }
    }

    if player_target.is_none() {
        if let Some(wielder) = as_player(w, live(w, obj(w, target).and_then(|t| t.wielder))) {
            player_target = Some(wielder);
        }
    }

    if let Some(player_target) = player_target.filter(|&p| p != this && !cloak_proc) {
        let target_name = if target == player_target {
            "you".to_owned()
        } else {
            format!("your {}", name_of(w, target))
        };

        let text = format!(
            "{} cast {} on {target_name}{suffix}",
            name_of(w, caster),
            spell.name()
        );
        player_send_chat_message(w, player_target, Some(this), &text, ChatMessageType::Magic);
    }
}

/// Handles casting SpellType.Boost / FellowBoost spells, typically for Life Magic, ie. Heal, Harm.
///
/// # Panics
/// With no target creature (ACE: `NullReferenceException` on `targetCreature.GetResistanceMod`).
// ACE: WorldObject.HandleCastSpell_Boost
#[allow(unused_assignments)] // ACE's dead stores to `boost`, kept in its order
pub fn handle_cast_spell_boost(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target_creature: Option<ObjectGuid>,
) {
    let player = as_player(w, Some(this));
    let creature = as_creature(w, Some(this));

    // prevent double deaths from indirect casts
    // caster is already checked in player/monster, and re-checking caster here would break death emotes such as bunny smite
    if let Some(t) = target_creature {
        if creature_is_dead(w, t) {
            return;
        }
    }

    // handle negatives?
    let min_boost_value = spell.boost().min(spell.max_boost());
    let max_boost_value = spell.boost().max(spell.max_boost());

    let resistance_type = if min_boost_value > 0 {
        get_boost_resistance_type(spell.vital_damage_type())
    } else {
        get_drain_resistance_type(spell.vital_damage_type())
    };

    let mut try_boost = ThreadSafeRandom::next(min_boost_value, max_boost_value);
    let target_creature =
        target_creature.expect("ACE: targetCreature is null (NullReferenceException)");
    try_boost = round_to_i32(
        f64::from(try_boost) * creature_get_resistance_mod(w, target_creature, resistance_type),
    );

    let mut boost = try_boost;

    // handle cloak damage proc for harm other
    let equipped_cloak = creature_equipped_cloak(w, target_creature);

    if target_creature != this && spell.vital_damage_type() == DamageType::Health && try_boost < 0 {
        let percent = f32_of(try_boost.wrapping_neg().cast_unsigned())
            / f32_of(vital_max_value(
                w,
                target_creature,
                PropertyAttribute2nd::MaxHealth,
            ));

        if let Some(cloak) = equipped_cloak {
            if cloak_has_damage_proc(w, cloak) && cloak_roll_proc(w, cloak, percent) {
                let reduced = -cloak_get_reduced_amount(w, this, try_boost.wrapping_neg());

                cloak_show_message(
                    w,
                    target_creature,
                    this,
                    try_boost.wrapping_neg(),
                    reduced.wrapping_neg(),
                );

                try_boost = reduced;
                boost = reduced;
            }
        }
    }

    let src_vital;

    match spell.vital_damage_type() {
        DamageType::Mana => {
            boost = creature_update_vital_delta(
                w,
                target_creature,
                PropertyAttribute2nd::MaxMana,
                try_boost,
            );
            src_vital = "mana";
        }
        DamageType::Stamina => {
            boost = creature_update_vital_delta(
                w,
                target_creature,
                PropertyAttribute2nd::MaxStamina,
                try_boost,
            );
            src_vital = "stamina";
        }
        _ => {
            // Health
            boost = creature_update_vital_delta(
                w,
                target_creature,
                PropertyAttribute2nd::MaxHealth,
                try_boost,
            );
            src_vital = "health";

            if boost >= 0 {
                damage_history_on_heal(w, target_creature, boost.cast_unsigned());
            } else {
                damage_history_add(
                    w,
                    target_creature,
                    this,
                    DamageType::Health,
                    boost.wrapping_neg().cast_unsigned(),
                );
            }

            //if (targetPlayer != null && targetPlayer.Fellowship != null)
            //targetPlayer.Fellowship.OnVitalUpdate(targetPlayer);
        }
    }

    if let Some(player) = player {
        let caster_message = if player != target_creature {
            if spell.is_beneficial() {
                format!(
                    "With {} you restore {boost} points of {src_vital} to {}.",
                    spell.name(),
                    name_of(w, target_creature)
                )
            } else {
                format!(
                    "With {} you drain {} points of {src_vital} from {}.",
                    spell.name(),
                    boost.abs(),
                    name_of(w, target_creature)
                )
            }
        } else {
            let verb = if spell.is_beneficial() {
                "restore"
            } else {
                "drain"
            };

            format!(
                "You cast {} and {verb} {} points of your {src_vital}.",
                spell.name(),
                boost.abs()
            )
        };

        player_send_chat_message(
            w,
            player,
            Some(player),
            &caster_message,
            ChatMessageType::Magic,
        );
    }

    if let Some(target_player) = as_player(w, Some(target_creature)).filter(|&t| player != Some(t))
    {
        let target_message = if spell.is_beneficial() {
            format!(
                "{} casts {} and restores {boost} points of your {src_vital}.",
                name_of(w, this),
                spell.name()
            )
        } else {
            let m = format!(
                "{} casts {} and drains {} points of your {src_vital}.",
                name_of(w, this),
                spell.name(),
                boost.abs()
            );

            if let Some(creature) = creature {
                player_set_current_attacker(w, target_player, creature);
            }
            m
        };

        player_send_chat_message(
            w,
            target_player,
            player,
            &target_message,
            ChatMessageType::Magic,
        );
    }

    if target_creature != this
        && creature_is_alive(w, target_creature)
        && spell.vital_damage_type() == DamageType::Health
        && boost < 0
    {
        // handle cloak spell proc
        if let Some(cloak) = equipped_cloak.filter(|&c| cloak_has_proc_spell(w, c)) {
            let pct = f32_of(boost.wrapping_neg().cast_unsigned())
                / f32_of(vital_max_value(
                    w,
                    target_creature,
                    PropertyAttribute2nd::MaxHealth,
                ));

            // ensure message is sent after enchantment.Message
            let mut action_chain = ActionChain::new();
            action_chain.add_delay_for_one_tick(w);
            action_chain.add_action(Actor::Object(this), move |w| {
                cloak_try_proc_spell(w, target_creature, this, cloak, pct)
            });
            action_chain.enqueue_chain(w);
        }

        // ensure emote process occurs after damage msg
        let mut emote_chain = ActionChain::new();
        emote_chain.add_delay_for_one_tick(w);
        emote_chain.add_action(Actor::Object(target_creature), move |w| {
            emote_manager_on_damage(w, target_creature, creature)
        });
        //if (critical)
        //    emoteChain.AddAction(target, () => target.EmoteManager.OnReceiveCritical(creature));
        emote_chain.enqueue_chain(w);
    }

    handle_boost_transfer_death(w, this, creature, Some(target_creature));
}

/// Returns the boost resistance for a damage type.
// ACE: WorldObject.GetBoostResistanceType
#[must_use]
pub fn get_boost_resistance_type(damage_type: DamageType) -> ResistanceType {
    match damage_type {
        DamageType::Health => ResistanceType::HealthBoost,
        DamageType::Stamina => ResistanceType::StaminaBoost,
        DamageType::Mana => ResistanceType::ManaBoost,
        _ => ResistanceType::Undef,
    }
}

/// Returns the drain resistance for a damage type.
// ACE: WorldObject.GetDrainResistanceType
#[must_use]
pub fn get_drain_resistance_type(damage_type: DamageType) -> ResistanceType {
    match damage_type {
        DamageType::Health => ResistanceType::HealthDrain,
        DamageType::Stamina => ResistanceType::StaminaDrain,
        DamageType::Mana => ResistanceType::ManaDrain,
        _ => ResistanceType::Undef,
    }
}

/// Returns the boost resistance type for a vital (the `PropertyAttribute2nd` overload).
// ACE: WorldObject.GetBoostResistanceType
#[must_use]
pub fn get_boost_resistance_type_vital(vital: PropertyAttribute2nd) -> ResistanceType {
    match vital {
        PropertyAttribute2nd::Health => ResistanceType::HealthBoost,
        PropertyAttribute2nd::Stamina => ResistanceType::StaminaBoost,
        PropertyAttribute2nd::Mana => ResistanceType::ManaBoost,
        _ => ResistanceType::Undef,
    }
}

/// Returns the drain resistance type for a vital (the `PropertyAttribute2nd` overload).
// ACE: WorldObject.GetDrainResistanceType
#[must_use]
pub fn get_drain_resistance_type_vital(vital: PropertyAttribute2nd) -> ResistanceType {
    match vital {
        PropertyAttribute2nd::Health => ResistanceType::HealthDrain,
        PropertyAttribute2nd::Stamina => ResistanceType::StaminaDrain,
        PropertyAttribute2nd::Mana => ResistanceType::ManaDrain,
        _ => ResistanceType::Undef,
    }
}

/// Checks for death from a boost / transfer spell.
// ACE: WorldObject.HandleBoostTransferDeath
pub fn handle_boost_transfer_death(
    w: &mut World,
    _this: ObjectGuid,
    caster: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
) {
    if let Some(caster) = caster.filter(|&c| creature_is_dead(w, c)) {
        let last_damager = damage_history_last_damager(w, caster);
        crate::dispatch::on_death::on_death(w, caster, last_damager, DamageType::Health, false);
        creature_die(w, caster);
    }

    if let Some(target) = target.filter(|&t| creature_is_dead(w, t) && Some(t) != caster) {
        let last_damager = damage_history_last_damager(w, target);
        crate::dispatch::on_death::on_death(w, target, last_damager, DamageType::Health, false);
        creature_die(w, target);
    }
}

/// `GetCreatureVital(vital)`: the max-vital key of a current-vital id (`Health` is `MaxHealth`).
///
/// # Panics
/// For any other id: ACE's `GetCreatureVital` answers null and the caller's `.Current` throws.
fn creature_vital_key(vital: PropertyAttribute2nd) -> PropertyAttribute2nd {
    match vital {
        PropertyAttribute2nd::Health => PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::Stamina => PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::Mana => PropertyAttribute2nd::MaxMana,
        _ => panic!("ACE: Creature.GetCreatureVital({vital:?}) is null (NullReferenceException)"),
    }
}

/// Handles casting SpellType.Transfer spells, usually for Life Magic, ie. Stamina to Mana, Drain.
///
/// # Panics
/// When the source or destination is null (a non-creature caster, or no target creature), as ACE
/// throws `NullReferenceException`.
// ACE: WorldObject.HandleCastSpell_Transfer
#[allow(clippy::too_many_lines)]
pub fn handle_cast_spell_transfer(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target_creature: Option<ObjectGuid>,
) {
    let player = as_player(w, Some(this));
    let creature = as_creature(w, Some(this));

    let target_player = as_player(w, target_creature);

    // prevent double deaths from indirect casts
    // caster is already checked in player/monster, and re-checking caster here would break death emotes such as bunny smite
    if let Some(t) = target_creature {
        if creature_is_dead(w, t) {
            return;
        }
    }

    // source and destination can be the same creature, or different creatures
    let caster = as_creature(w, Some(this));
    let transfer_source = if has_transfer_flag(spell.transfer_flags(), TransferFlags::CasterSource)
    {
        caster
    } else {
        target_creature
    };
    let destination = if has_transfer_flag(spell.transfer_flags(), TransferFlags::CasterDestination)
    {
        caster
    } else {
        target_creature
    };

    let transfer_source =
        transfer_source.expect("ACE: transferSource is null (NullReferenceException)");
    let destination = destination.expect("ACE: destination is null (NullReferenceException)");

    // Drain Resistances - allows one to partially resist drain health/stamina/mana and harm attacks (not including other life transfer spells).
    let is_drain = has_transfer_flag(
        spell.transfer_flags(),
        TransferFlags(TransferFlags::TargetSource.0 | TransferFlags::CasterDestination.0),
    );
    let drain_mod: f32 = if is_drain {
        creature_get_resistance_mod(
            w,
            transfer_source,
            get_drain_resistance_type_vital(spell.source()),
        )
        .cs_cast()
    } else {
        1.0
    };

    let source_current = vital_current(w, transfer_source, creature_vital_key(spell.source()));
    let mut src_vital_change = round_to_u32(f64::from(
        f32_of(source_current) * spell.proportion() * drain_mod,
    ));

    // TransferCap caps both srcVitalChange and destVitalChange
    // https://asheron.fandom.com/wiki/Announcements_-_2003/01_-_The_Slumbering_Giant#Letter_to_the_Players

    if spell.transfer_cap() != 0 && i64::from(src_vital_change) > i64::from(spell.transfer_cap()) {
        src_vital_change = spell.transfer_cap().cast_unsigned();
    }

    // should healing resistances be applied here?
    let boost_mod: f32 = if is_drain {
        creature_get_resistance_mod(
            w,
            destination,
            get_boost_resistance_type_vital(spell.destination()),
        )
        .cs_cast()
    } else {
        1.0
    };

    let mut dest_vital_change = round_to_u32(f64::from(
        f32_of(src_vital_change) * (1.0 - spell.loss_percent()) * boost_mod,
    ));

    // scale srcVitalChange to destVitalChange?
    let missing_dest = vital_missing(w, destination, creature_vital_key(spell.destination()));

    let mut max_dest_vital_change = missing_dest;
    if spell.transfer_cap() != 0
        && i64::from(max_dest_vital_change) > i64::from(spell.transfer_cap())
    {
        max_dest_vital_change = spell.transfer_cap().cast_unsigned();
    }

    if dest_vital_change > max_dest_vital_change {
        let scalar = f32_of(max_dest_vital_change) / f32_of(dest_vital_change);

        src_vital_change = round_to_u32(f64::from(f32_of(src_vital_change) * scalar));
        dest_vital_change = max_dest_vital_change;
    }

    // handle cloak damage procs for drain health other
    let equipped_cloak = target_creature.and_then(|t| creature_equipped_cloak(w, t));

    if is_drain && spell.source() == PropertyAttribute2nd::Health {
        let tc = target_creature.expect("ACE: targetCreature is null (NullReferenceException)");
        let percent = f32_of(src_vital_change)
            / f32_of(vital_max_value(w, tc, PropertyAttribute2nd::MaxHealth));

        if let Some(cloak) = equipped_cloak {
            if cloak_has_damage_proc(w, cloak) && cloak_roll_proc(w, cloak, percent) {
                let reduced = cloak_get_reduced_amount_uint(w, this, src_vital_change);

                cloak_show_message_uint(w, tc, this, src_vital_change, reduced);

                src_vital_change = reduced;
                dest_vital_change = round_to_u32(f64::from(
                    f32_of(src_vital_change) * (1.0 - spell.loss_percent()) * boost_mod,
                ));
            }
        }
    }

    // Apply the change in vitals to the source
    let src_vital = match spell.source() {
        PropertyAttribute2nd::Mana => {
            let d = creature_update_vital_delta(
                w,
                transfer_source,
                PropertyAttribute2nd::MaxMana,
                (src_vital_change.cast_signed()).wrapping_neg(),
            );
            src_vital_change = d.wrapping_neg().cast_unsigned();
            "mana"
        }
        PropertyAttribute2nd::Stamina => {
            let d = creature_update_vital_delta(
                w,
                transfer_source,
                PropertyAttribute2nd::MaxStamina,
                (src_vital_change.cast_signed()).wrapping_neg(),
            );
            src_vital_change = d.wrapping_neg().cast_unsigned();
            "stamina"
        }
        _ => {
            // Health
            let d = creature_update_vital_delta(
                w,
                transfer_source,
                PropertyAttribute2nd::MaxHealth,
                (src_vital_change.cast_signed()).wrapping_neg(),
            );
            src_vital_change = d.wrapping_neg().cast_unsigned();

            damage_history_add(
                w,
                transfer_source,
                this,
                DamageType::Health,
                src_vital_change,
            );

            //var sourcePlayer = source as Player;
            //if (sourcePlayer != null && sourcePlayer.Fellowship != null)
            //sourcePlayer.Fellowship.OnVitalUpdate(sourcePlayer);
            "health"
        }
    };

    // Apply the scaled change in vitals to the caster
    let dest_vital = match spell.destination() {
        PropertyAttribute2nd::Mana => {
            dest_vital_change = creature_update_vital_delta(
                w,
                destination,
                PropertyAttribute2nd::MaxMana,
                dest_vital_change.cast_signed(),
            )
            .cast_unsigned();
            "mana"
        }
        PropertyAttribute2nd::Stamina => {
            dest_vital_change = creature_update_vital_delta(
                w,
                destination,
                PropertyAttribute2nd::MaxStamina,
                dest_vital_change.cast_signed(),
            )
            .cast_unsigned();
            "stamina"
        }
        _ => {
            // Health
            dest_vital_change = creature_update_vital_delta(
                w,
                destination,
                PropertyAttribute2nd::MaxHealth,
                dest_vital_change.cast_signed(),
            )
            .cast_unsigned();

            damage_history_on_heal(w, destination, dest_vital_change);

            //var destPlayer = destination as Player;
            //if (destPlayer != null && destPlayer.Fellowship != null)
            //destPlayer.Fellowship.OnVitalUpdate(destPlayer);
            "health"
        }
    };

    // You gain 52 points of health due to casting Drain Health Other I on Olthoi Warrior
    // You lose 22 points of mana due to casting Incantation of Infuse Mana Other on High-Voltage VI
    // You lose 12 points of mana due to Zofrit Zefir casting Drain Mana Other II on you

    // You cast Stamina to Mana Self I on yourself and lose 50 points of stamina and also gain 45 points of mana
    // You cast Stamina to Health Self VI on yourself and fail to affect your  stamina and also gain 1 point of health

    // unverified:
    // You gain X points of vital due to caster casting spell on you
    // You lose X points of vital due to caster casting spell on you

    let player_source = as_player(w, Some(transfer_source));
    let player_destination = as_player(w, Some(destination));

    let mut source_msg: Option<String> = None;
    let mut target_msg: Option<String> = None;

    // `targetCreature.Name` / `caster.Name`: ACE dereferences both; a null one throws.
    let target_name = |w: &World| {
        name_of(
            w,
            target_creature.expect("ACE: targetCreature is null (NullReferenceException)"),
        )
    };
    let caster_name = |w: &World| {
        name_of(
            w,
            caster.expect("ACE: caster is null (NullReferenceException)"),
        )
    };

    if player_source.is_some() && player_destination.is_some() && transfer_source == destination {
        source_msg = Some(format!(
            "You cast {} on yourself and lose {src_vital_change} points of {src_vital} and also gain {dest_vital_change} points of {dest_vital}",
            spell.name()
        ));
    } else {
        if let Some(player_source) = player_source {
            if transfer_source == this {
                source_msg = Some(format!(
                    "You lose {src_vital_change} points of {src_vital} due to casting {} on {}",
                    spell.name(),
                    target_name(w)
                ));
            } else {
                target_msg = Some(format!(
                    "You lose {src_vital_change} points of {src_vital} due to {} casting {} on you",
                    caster_name(w),
                    spell.name()
                ));
            }

            if is_creature(w, destination) {
                player_set_current_attacker(w, player_source, destination);
            }
        }

        if player_destination.is_some() {
            if destination == this {
                source_msg = Some(format!(
                    "You gain {dest_vital_change} points of {dest_vital} due to casting {} on {}",
                    spell.name(),
                    target_name(w)
                ));
            } else {
                target_msg = Some(format!(
                    "You gain {dest_vital_change} points of {dest_vital} due to {} casting {} on you",
                    caster_name(w),
                    spell.name()
                ));
            }
        }
    }

    if let (Some(player), Some(source_msg)) = (player, source_msg) {
        player_send_chat_message(w, player, Some(player), &source_msg, ChatMessageType::Magic);
    }

    if let (Some(target_player), Some(target_msg)) = (target_player, target_msg) {
        player_send_chat_message(
            w,
            target_player,
            caster,
            &target_msg,
            ChatMessageType::Magic,
        );
    }

    if is_drain && spell.source() == PropertyAttribute2nd::Health {
        let tc = target_creature.expect("ACE: targetCreature is null (NullReferenceException)");
        if creature_is_alive(w, tc) {
            // handle cloak spell proc
            if let Some(cloak) = equipped_cloak.filter(|&c| cloak_has_proc_spell(w, c)) {
                let pct = f32_of(src_vital_change)
                    / f32_of(vital_max_value(w, tc, PropertyAttribute2nd::MaxHealth));

                // ensure message is sent after enchantment.Message
                let mut action_chain = ActionChain::new();
                action_chain.add_delay_for_one_tick(w);
                action_chain.add_action(Actor::Object(this), move |w| {
                    cloak_try_proc_spell(w, tc, this, cloak, pct)
                });
                action_chain.enqueue_chain(w);
            }

            // ensure emote process occurs after damage msg
            let mut emote_chain = ActionChain::new();
            emote_chain.add_delay_for_one_tick(w);
            emote_chain.add_action(Actor::Object(tc), move |w| {
                emote_manager_on_damage(w, tc, creature)
            });
            //if (critical)
            //    emoteChain.AddAction(targetCreature, () => targetCreature.EmoteManager.OnReceiveCritical(creature));
            emote_chain.enqueue_chain(w);
        }
    }

    handle_boost_transfer_death(w, this, creature, target_creature);
}

/// Handles casting SpellType.Projectile / LifeProjectile / EnchantmentProjectile spells.
///
/// # Panics
/// A Life Magic projectile from a non-creature caster (ACE: `NullReferenceException`).
// ACE: WorldObject.HandleCastSpell_Projectile
#[allow(clippy::too_many_arguments)]
pub fn handle_cast_spell_projectile(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    _item_caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    is_weapon_spell: bool,
    from_proc: bool,
) {
    let mut damage: u32 = 0;
    let caster = as_creature(w, Some(this));
    let mut damage_type = DamageType::Undef;

    if spell.school() == MagicSchool::LifeMagic {
        let c = || caster.expect("ACE: caster is null (NullReferenceException)");
        let drain = |w: &mut World, key: PropertyAttribute2nd| -> u32 {
            let current = vital_current(w, c(), key);
            let try_damage = round_to_i32(f64::from(f32_of(current) * spell.drain_percentage()));
            creature_update_vital_delta(w, c(), key, try_damage.wrapping_neg())
                .wrapping_neg()
                .cast_unsigned()
        };

        if has_damage_flag(spell.damage_type(), DamageType::Mana) {
            damage = drain(w, PropertyAttribute2nd::MaxMana);
            damage_type = DamageType::Mana;
        } else if has_damage_flag(spell.damage_type(), DamageType::Stamina) {
            damage = drain(w, PropertyAttribute2nd::MaxStamina);
            damage_type = DamageType::Stamina;
        } else if has_damage_flag(spell.damage_type(), DamageType::Health) {
            damage = drain(w, PropertyAttribute2nd::MaxHealth);
            damage_history_add(w, c(), this, DamageType::Health, damage);
            damage_type = DamageType::Health;

            //if (player != null && player.Fellowship != null)
            //player.Fellowship.OnVitalUpdate(player);
        } else if spell.damage_type() != DamageType::Undef {
            // Handle rare case where some of these "Life Magic" spells do physical damage e.g. Hunter's Lash 2970 and Thorn Valley 6159
            damage_type = spell.damage_type();
        } else {
            log::warn!(
                "Unknown DamageType ({}) for LifeProjectile {} - {}",
                spell.damage_type().to_dotnet_string(),
                spell.name(),
                spell.id()
            );
            return;
        }
    }

    create_spell_projectiles(
        w,
        this,
        spell,
        target,
        weapon,
        is_weapon_spell,
        from_proc,
        damage,
    );

    if spell.school() == MagicSchool::LifeMagic {
        let c = caster.expect("ACE: caster is null (NullReferenceException)");
        if vital_current(w, c, PropertyAttribute2nd::MaxHealth) == 0 {
            // should this be possible?
            // `var lastDamager = caster != null ? new DamageHistoryInfo(caster) : null;`
            let last_damager = Some(DamageHistoryInfo::new(w, c, 0.0));

            crate::dispatch::on_death::on_death(w, c, last_damager, damage_type, false);
            creature_die(w, c);
        }
    }
}

/// Handles casting SpellType.PortalLink spells.
// ACE: WorldObject.HandleCastSpell_PortalLink
pub fn handle_cast_spell_portal_link(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
) {
    let Some(player) = as_player(w, Some(this)) else {
        return;
    };

    if player_is_olthoi_player(w, player) {
        send_weenie_error_event(w, player, WeenieError::OlthoiCanOnlyRecallToLifestone);
        return;
    }

    let target_of = |w: &World| {
        target
            .filter(|&t| w.objects.get(t).is_some())
            .expect("ACE: target is null (NullReferenceException)")
    };

    match SpellId(spell.id()) {
        SpellId::LifestoneTie1 => {
            // Lifestone Tie
            let t = target_of(w);
            if obj(w, t).is_some_and(|o| o.biota.weenie_type == WeenieType::LifeStone) {
                player_send_chat_message(
                    w,
                    player,
                    Some(this),
                    "You have successfully linked with the life stone.",
                    ChatMessageType::Magic,
                );
                let location = obj(w, t).and_then(WorldObject::location);
                if let Some(p) = w.objects.get_mut(player) {
                    p.set_linked_lifestone(location);
                }
            } else {
                player_send_chat_message(
                    w,
                    player,
                    Some(this),
                    "You cannot link that.",
                    ChatMessageType::Magic,
                );
            }
        }

        SpellId::PortalTie1 | SpellId::PortalTie2 => {
            // Primary Portal Tie / Secondary Portal Tie
            let t = target_of(w);
            if obj(w, t).is_none_or(|o| o.biota.weenie_type != WeenieType::Portal) {
                player_send_chat_message(
                    w,
                    player,
                    Some(this),
                    "You cannot link that.",
                    ChatMessageType::Magic,
                );
                return;
            }

            let (original_portal, wcid) = obj(w, t)
                .map(|o| (o.original_portal(), o.biota.weenie_class_id))
                .unwrap_or_default();

            let summoned = original_portal.is_some();

            let target_did = if summoned {
                original_portal
            } else {
                Some(wcid)
            };

            let tie_portal = get_portal(
                w,
                target_did.expect("ACE: targetDID has no value (InvalidOperationException)"),
            );

            let Some(tie_portal) = tie_portal else {
                send_weenie_error_event(w, player, WeenieError::YouCannotLinkToThatPortal);
                return;
            };

            let (result, tie_portal) = portal_check_use_requirements(w, tie_portal, player);

            if !result.success {
                if let Some(message) = result.message {
                    send(w, player, message);
                }
            }

            if portal_no_tie(&tie_portal) || !result.success {
                send_weenie_error_event(w, player, WeenieError::YouCannotLinkToThatPortal);
                return;
            }

            let is_primary = spell.id() == SpellId::PortalTie1.0;

            if let Some(p) = w.objects.get_mut(player) {
                if is_primary {
                    p.set_linked_portal_one_did(target_did);
                    p.set_property(PropertyBool::LinkedPortalOneSummon, summoned);
                } else {
                    p.set_linked_portal_two_did(target_did);
                    p.set_property(PropertyBool::LinkedPortalTwoSummon, summoned);
                }
            }

            player_send_chat_message(
                w,
                player,
                Some(this),
                "You have successfully linked with the portal.",
                ChatMessageType::Magic,
            );
        }

        _ => {}
    }
}

/// Returns a Portal object for a WCID: a new, detached object that never enters the world (ACE
/// builds it with `new ObjectGuid(wcid)` and discards it).
// ACE: WorldObject.GetPortal
#[must_use]
pub fn get_portal(w: &World, wcid: u32) -> Option<WorldObject> {
    let weenie = w.content.get_cached_weenie(wcid);

    let wo = crate::world_objects::world_object::CtorEnv::with_world(w, |env| {
        crate::factories::world_object_factory::create_world_object(
            env,
            weenie,
            ObjectGuid::new(wcid),
        )
    });
    wo.filter(WorldObject::is_portal)
}

/// Handles casting SpellType.PortalRecall spells.
///
/// # Panics
/// The recall spells dereference the target player; with none, ACE throws
/// `NullReferenceException`.
// ACE: WorldObject.HandleCastSpell_PortalRecall
#[allow(clippy::too_many_lines)]
pub fn handle_cast_spell_portal_recall(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target_creature: Option<ObjectGuid>,
) {
    let player = as_player(w, Some(this));

    if let Some(player) = player {
        if player_is_olthoi_player(w, player) {
            send_weenie_error_event(w, player, WeenieError::OlthoiCanOnlyRecallToLifestone);
            return;
        }
    }

    let _creature = as_creature(w, Some(this));

    let target_player = as_player(w, target_creature);
    let tp = || target_player.expect("ACE: targetPlayer is null (NullReferenceException)");

    if let Some(player) = player {
        if player_pk_timer_active(w, player) {
            send_weenie_error_event(w, player, WeenieError::YouHaveBeenInPKBattleTooRecently);
            return;
        }
    }

    let mut recall = PositionType::Undef;
    let mut recall_did: Option<u32> = None;

    // verify pre-requirements for recalls

    match SpellId(spell.id()) {
        SpellId::PortalRecall => {
            // portal recall
            let last_portal_did = obj(w, tp()).and_then(WorldObject::last_portal_did);
            if last_portal_did.is_none() {
                // You must link to a portal to recall it!
                send_weenie_error_event(w, tp(), WeenieError::YouMustLinkToPortalToRecall);
            } else {
                recall = PositionType::LastPortal;
                recall_did = last_portal_did;
            }
        }

        SpellId::LifestoneRecall1 => {
            // lifestone recall
            if obj(w, tp())
                .and_then(|o| o.get_position(PositionType::LinkedLifestone))
                .is_none()
            {
                // You must link to a lifestone to recall it!
                send_weenie_error_event(w, tp(), WeenieError::YouMustLinkToLifestoneToRecall);
            } else {
                recall = PositionType::LinkedLifestone;
            }
        }

        #[allow(clippy::if_same_then_else)] // ACE's two branches
        SpellId::LifestoneSending1 => {
            if player
                .and_then(|p| obj(w, p))
                .and_then(|o| o.get_position(PositionType::Sanctuary))
                .is_some()
            {
                recall = PositionType::Sanctuary;
            } else if target_player
                .and_then(|p| obj(w, p))
                .and_then(|o| o.get_position(PositionType::Sanctuary))
                .is_some()
            {
                recall = PositionType::Sanctuary;
            }
        }

        SpellId::PortalTieRecall1 => {
            // primary portal tie recall
            let did = obj(w, tp()).and_then(WorldObject::linked_portal_one_did);
            if did.is_none() {
                // You must link to a portal to recall it!
                send_weenie_error_event(w, tp(), WeenieError::YouMustLinkToPortalToRecall);
            } else {
                recall = PositionType::LinkedPortalOne;
                recall_did = did;
            }
        }

        SpellId::PortalTieRecall2 => {
            // secondary portal tie recall
            let did = obj(w, tp()).and_then(WorldObject::linked_portal_two_did);
            if did.is_none() {
                // You must link to a portal to recall it!
                send_weenie_error_event(w, tp(), WeenieError::YouMustLinkToPortalToRecall);
            } else {
                recall = PositionType::LinkedPortalTwo;
                recall_did = did;
            }
        }

        _ => {}
    }

    if recall != PositionType::Undef {
        let target_player = tp();
        if let Some(recall_did) = recall_did {
            // portal recall
            let portal = get_portal(w, recall_did);
            let Some(portal) = portal.filter(|p| !portal_no_recall(p)) else {
                // You cannot recall that portal!
                let player = player.expect("ACE: player is null (NullReferenceException)");
                send_weenie_error_event(w, player, WeenieError::YouCannotRecallPortal);
                return;
            };

            let (result, portal) = portal_check_use_requirements(w, portal, target_player);
            if !result.success {
                if let Some(message) = result.message {
                    send(w, target_player, message);
                }

                return;
            }

            let destination = portal.destination();

            let mut portal_recall = ActionChain::new();
            portal_recall.add_action(Actor::Object(target_player), move |w| {
                player_do_pre_teleport_hide(w, target_player)
            });
            portal_recall.add_delay_seconds(w, f64::from(2.0f32)); // 2 second delay
            portal_recall.add_action(Actor::Object(target_player), move |w| {
                let mut teleport_dest = Position::from_position(
                    &destination.expect("ACE: Portal.Destination is null (NullReferenceException)"),
                );
                crate::world_objects::world_object::adjust_dungeon(w, &mut teleport_dest);

                player_teleport(w, target_player, teleport_dest);
            });
            portal_recall.enqueue_chain(w);
        } else {
            // lifestone recall
            let mut lifestone_recall = ActionChain::new();
            lifestone_recall.add_action(Actor::Object(target_player), move |w| {
                player_do_pre_teleport_hide(w, target_player)
            });
            lifestone_recall.add_delay_seconds(w, f64::from(2.0f32)); // 2 second delay
            lifestone_recall.add_action(Actor::Object(target_player), move |w| {
                player_tele_to_position(w, target_player, recall)
            });
            lifestone_recall.enqueue_chain(w);
        }
    }
}

/// Handles casting SpellType.PortalSummon spells.
// ACE: WorldObject.HandleCastSpell_PortalSummon
pub fn handle_cast_spell_portal_summon(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target_creature: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
) {
    let player = as_player(w, Some(this));

    if let Some(player) = player {
        if player_is_olthoi_player(w, player) {
            send_weenie_error_event(w, player, WeenieError::OlthoiCanOnlyRecallToLifestone);
            return;
        }

        if player_pk_timer_active(w, player) {
            send_weenie_error_event(w, player, WeenieError::YouHaveBeenInPKBattleTooRecently);
            return;
        }
    }

    let source = player
        .or(item_caster)
        .expect("ACE: source is null (NullReferenceException)");

    // spell.link = 1 = LinkedPortalOneDID
    // spell.link = 2 = LinkedPortalTwoDID

    let (portal_id, link_summoned) = {
        let s = obj(w, source).expect("ACE: source is null (NullReferenceException)");
        if spell.link() <= 1 {
            (
                s.linked_portal_one_did().unwrap_or(0),
                s.get_property(PropertyBool::LinkedPortalOneSummon)
                    .unwrap_or(false),
            )
        } else {
            (
                s.linked_portal_two_did().unwrap_or(0),
                s.get_property(PropertyBool::LinkedPortalTwoSummon)
                    .unwrap_or(false),
            )
        }
    };

    let mut summon_loc: Option<Position> = None;

    if let Some(player) = player {
        if portal_id == 0 {
            // You must link to a portal to summon it!
            send_weenie_error_event(w, player, WeenieError::YouMustLinkToPortalToSummonIt);
            return;
        }

        let summon_portal = get_portal(w, portal_id);
        let gateway_ties_summonable =
            property_manager::get_bool(w, "gateway_ties_summonable", false, true).item;
        let Some(summon_portal) = summon_portal
            .filter(|p| !(portal_no_summon(p) || (link_summoned && !gateway_ties_summonable)))
        else {
            // You cannot summon that portal!
            send_weenie_error_event(w, player, WeenieError::YouCannotSummonPortal);
            return;
        };

        let (result, _summon_portal) = portal_check_use_requirements(w, summon_portal, player);
        if !result.success {
            if let Some(message) = result.message {
                send(w, player, message);
            }

            return;
        }

        let location = obj(w, player)
            .and_then(WorldObject::location)
            .expect("ACE: Player.Location is null (NullReferenceException)");
        summon_loc = Some(location.in_front_of(f64::from(3.0f32), false));
    } else if let Some(item_caster) = live(w, item_caster) {
        if obj(w, item_caster)
            .and_then(WorldObject::portal_summon_loc)
            .is_some()
        {
            // ACE-BUG: this reads `this.PortalSummonLoc`, not `itemCaster.PortalSummonLoc` it just
            // checked; for an item caster other than `this` the gateway lands at `this`'s summon
            // location, or `new Position(null)` throws when `this` has none.
            let own = obj(w, this)
                .and_then(WorldObject::portal_summon_loc)
                .expect("ACE: new Position(null) (NullReferenceException)");
            summon_loc = Some(Position::from_position(&own));
        } else if let Some(location) = obj(w, item_caster).and_then(WorldObject::location) {
            summon_loc = Some(location.in_front_of(f64::from(3.0f32), false));
        } else if let Some(location) = target_creature
            .and_then(|t| obj(w, t))
            .and_then(WorldObject::location)
        {
            summon_loc = Some(location.in_front_of(f64::from(3.0f32), false));
        }
    }

    if let Some(loc) = summon_loc.as_mut() {
        let cell = position_extensions::get_cell(w, loc);
        loc.set_landblock_id(LandblockId::new(cell));
    }

    let success = summon_portal(w, portal_id, summon_loc.as_ref(), spell.portal_lifetime());

    if !success {
        if let Some(player) = player {
            send_weenie_error_event(w, player, WeenieError::YouFailToSummonPortal);
        }
    }
}

/// Spawns a portal for SpellType.PortalSummon spells.
// ACE: WorldObject.SummonPortal
pub fn summon_portal(
    w: &mut World,
    portal_id: u32,
    location: Option<&Position>,
    portal_lifetime: f64,
) -> bool {
    let portal = get_portal(w, portal_id);

    let (Some(portal), Some(location)) = (portal, location) else {
        return false;
    };

    let Some(gateway) = world_object_factory_create_new_world_object_by_name(w, "portalgateway")
    else {
        return false;
    };

    let Some(mut g) = w.objects.get_mut(gateway).map(std::mem::take) else {
        return false;
    };
    g.set_location(Some(Position::from_position(location)));
    g.set_original_portal(Some(portal_id));
    if let Some(slot) = w.objects.get_mut(gateway) {
        *slot = g;
    }

    let portal_destination = portal
        .destination()
        .expect("ACE: Portal.Destination is null (NullReferenceException)");
    portal_update_portal_destination(w, gateway, Position::from_position(&portal_destination));

    if let Some(g) = w.objects.get_mut(gateway) {
        g.set_time_to_rot(Some(portal_lifetime));

        g.set_min_level(portal.min_level());
        g.set_max_level(portal.max_level());
        g.set_portal_restrictions(portal.portal_restrictions());
        g.set_account_requirements_portal(portal.account_requirements_portal());
        g.set_advocate_quest_portal(portal.advocate_quest_portal());

        g.set_quest(portal.quest());
        g.set_quest_restriction(portal.quest_restriction());

        g.biota
            .properties_emote
            .clone_from(&portal.biota.properties_emote);

        // all gateways are marked NoSummon but by default ruleset, the OriginalPortal is the one that is checked against
        let restrictions = g.portal_restrictions();
        g.set_portal_restrictions(PortalBitmask(restrictions.0 | PortalBitmask::NoSummon.0));
    }

    crate::dispatch::enter_world::enter_world(w, gateway);

    true
}

/// Handles casting SpellType.PortalSending spells.
// ACE: WorldObject.HandleCastSpell_PortalSending
pub fn handle_cast_spell_portal_sending(
    w: &mut World,
    _this: ObjectGuid,
    spell: &Spell,
    target_creature: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
) {
    if let Some(target_player) = as_player(w, target_creature) {
        if player_pk_timer_active(w, target_player) {
            send_weenie_error_event(
                w,
                target_player,
                WeenieError::YouHaveBeenInPKBattleTooRecently,
            );
            return;
        }

        let spell = spell.clone();
        let mut portal_sending_chain = ActionChain::new();
        //portalSendingChain.AddDelaySeconds(2.0f);  // 2 second delay
        portal_sending_chain.add_action(Actor::Object(target_player), move |w| {
            player_do_pre_teleport_hide(w, target_player)
        });
        portal_sending_chain.add_action(Actor::Object(target_player), move |w| {
            let mut teleport_dest = Position::from_position(&spell.position());
            crate::world_objects::world_object::adjust_dungeon(w, &mut teleport_dest);

            player_teleport(w, target_player, teleport_dest);

            player_send_teleported_via_magic_message(w, target_player, item_caster, &spell);
        });
        portal_sending_chain.enqueue_chain(w);
    } else if let Some(target_creature) = target_creature {
        // monsters can cast some portal spells on themselves too, possibly?
        // under certain circumstances, such as ensuring the destination is the same landblock
        let mut teleport_dest = Position::from_position(&spell.position());
        crate::world_objects::world_object::adjust_dungeon(w, &mut teleport_dest);

        creature_fake_teleport(w, target_creature, teleport_dest);
    }
}

/// Handles casting SpellType.FellowPortalSending spells.
///
/// # Panics
/// For a non-creature caster (ACE: `NullReferenceException` on `creature.GetDistance`).
// ACE: WorldObject.HandleCastSpell_FellowPortalSending
pub fn handle_cast_spell_fellow_portal_sending(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target_creature: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
) -> bool {
    let creature = as_creature(w, Some(this));

    let target_player = as_player(w, target_creature);

    let Some(target_player) = target_player else {
        return false;
    };
    if player_fellowship_get_fellowship_members(w, target_player).is_none() {
        return false;
    }

    if player_pk_timer_active(w, target_player) {
        send_weenie_error_event(
            w,
            target_player,
            WeenieError::YouHaveBeenInPKBattleTooRecently,
        );
        return false;
    }

    let creature = creature.expect("ACE: creature is null (NullReferenceException)");
    let distance_to_target = world_object_get_distance(w, creature, target_player);
    let (init_level, ranks) =
        creature_get_creature_skill_school_init_and_ranks(w, creature, spell.school());
    let magic_skill = init_level.wrapping_add(u32::from(ranks)); // Range uses the initial skill plus trained ranks.

    let mut max_range = spell.base_range_constant() + f32_of(magic_skill) * spell.base_range_mod();
    #[allow(clippy::float_cmp)]
    if max_range == 0.0 {
        max_range = f32::INFINITY;
    }

    if distance_to_target > max_range {
        return false;
    }

    let spell = spell.clone();
    let mut portal_sending_chain = ActionChain::new();
    portal_sending_chain.add_action(Actor::Object(target_player), move |w| {
        player_do_pre_teleport_hide(w, target_player)
    });
    portal_sending_chain.add_action(Actor::Object(target_player), move |w| {
        let mut teleport_dest = Position::from_position(&spell.position());
        crate::world_objects::world_object::adjust_dungeon(w, &mut teleport_dest);

        player_teleport(w, target_player, teleport_dest);

        player_send_teleported_via_magic_message(w, target_player, item_caster, &spell);
    });
    portal_sending_chain.enqueue_chain(w);

    true
}

/// Handles casting SpellType.Dispel / FellowDispel spells.
// ACE: WorldObject.HandleCastSpell_Dispel
pub fn handle_cast_spell_dispel(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: ObjectGuid,
) {
    let player = as_player(w, Some(this));
    let creature = as_creature(w, Some(this));

    let remove_spells = em::select_dispel(w, target, spell);

    // dispel on server and client
    let entries: Vec<_> = remove_spells
        .iter()
        .map(|s| s.enchantment.clone())
        .collect();
    emc::dispel_list(w, target, Some(&entries));

    let spell_list = build_spell_list(&remove_spells);
    let suffix = if remove_spells.is_empty() {
        ", but the dispel fails.".to_owned()
    } else {
        format!(" and dispel: {spell_list}.")
    };

    if let Some(player) = player {
        let caster_msg = if player == target {
            format!("You cast {} on yourself{suffix}", spell.name())
        } else {
            format!(
                "You cast {} on {}{suffix}",
                spell.name(),
                name_of(w, target)
            )
        };

        player_send_chat_message(w, player, Some(player), &caster_msg, ChatMessageType::Magic);
    }

    if let Some(target_player) = as_player(w, Some(target)).filter(|&t| player != Some(t)) {
        let target_msg = format!(
            "{} casts {} on you{}",
            name_of(w, this),
            spell.name(),
            suffix.replace("and dispel", "and dispels")
        );

        player_send_chat_message(
            w,
            target_player,
            Some(this),
            &target_msg,
            ChatMessageType::Magic,
        );

        // all dispels appear to be listed as non-beneficial, even the ones that only dispel negative spells
        // we filter here to positive or all
        if let Some(creature) = creature {
            if spell.align() != DispelType::Negative {
                player_set_current_attacker(w, target_player, creature);
            }
        }
    }
}

/// The PK timer rules for dispels (see the 2004/04 "A New Threat" announcement): a caster or a
/// target in a PK action within the last 20 seconds cannot dispel or be dispelled by a gem or
/// potion.
///
/// # Panics
/// With no target (ACE: `NullReferenceException`).
// ACE: WorldObject.VerifyDispelPKStatus
pub fn verify_dispel_pk_status(
    w: &mut World,
    caster: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
) -> bool {
    // https://asheron.fandom.com/wiki/Announcements_-_2004/04_-_A_New_Threat
    // https://asheron.fandom.com/wiki/Dispel_Spells

    // Dispel spells and potions have been revised. All dispels are also now tied to the PK/L timer.

    // If you have been in a PK/L action within the last 20 seconds, you will not be able to:

    // - Use a dispel gem.
    // - Use a dispel potion.
    // - Use the Awakener or Attenuated Awakener on someone else.
    // - Cast any dispel spell on yourself.
    // - Cast any dispel spell on someone else.

    let caster = live(w, caster);
    let caster_player = as_player(w, caster);

    if let Some(caster_player) = caster_player {
        if player_pk_timer_active(w, caster_player) {
            player_send_weenie_error(
                w,
                caster_player,
                WeenieError::YouHaveBeenInPKBattleTooRecently,
            );
            return false;
        }
    }

    let target = live(w, target).expect("ACE: target is null (NullReferenceException)");
    let wielder_or_target = live(w, obj(w, target).and_then(|t| t.wielder)).unwrap_or(target);
    if let Some(target_player) = as_player(w, Some(wielder_or_target)) {
        if player_pk_timer_active(w, target_player)
            && caster
                .and_then(|c| obj(w, c))
                .is_some_and(|c| c.is_gem() || c.is_food())
        {
            /* casterPlayer != null || */
            if let Some(caster_player) = caster_player {
                let text = format!(
                    "{} has been involved in a player killer battle too recently to do that!",
                    name_of(w, target_player)
                );
                send(
                    w,
                    caster_player,
                    game_message_system_chat(&text, ChatMessageType::Magic),
                );
            } else {
                player_send_weenie_error(
                    w,
                    target_player,
                    WeenieError::YouHaveBeenInPKBattleTooRecently,
                );
            }

            return false;
        }
    }

    true
}

/// Returns a string with the spell list format as: Spell Name 1, Spell Name 2, and Spell Name 3.
// ACE: WorldObject.BuildSpellList
#[must_use]
pub fn build_spell_list(spells: &[SpellEnchantment]) -> String {
    let mut sb = String::new();
    for (i, spell) in spells.iter().enumerate() {
        if i > 0 {
            sb.push_str(", ");
            if i == spells.len() - 1 {
                sb.push_str("and ");
            }
        }

        sb.push_str(spell.spell.name());
    }
    sb
}

// ------------------------------------------------------------------------------- projectiles

/// Creates and launches the projectiles for a spell. `is_weapon_spell`, `from_proc` and
/// `life_projectile_damage` default to false / false / 0.
// ACE: WorldObject.CreateSpellProjectiles
#[allow(clippy::too_many_arguments)]
pub fn create_spell_projectiles(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    is_weapon_spell: bool,
    from_proc: bool,
    life_projectile_damage: u32,
) -> Vec<ObjectGuid> {
    if spell.num_projectiles() == 0 {
        log::error!(
            "{} ({}).CreateSpellProjectiles({} - {}) - spell.NumProjectiles == 0",
            name_of(w, this),
            this,
            spell.id(),
            spell.name()
        );
        return Vec::new();
    }

    let spell_type = spell_projectile_get_projectile_spell_type(w, spell.id());

    // Not ACE's (a fix, V325): a target that left the world before release (its
    // log-off finished during the cast) is no target, and the spell flies straight ahead; ACE aimed
    // at the destroyed body's frame in landblock 0.
    let target = in_world(w, target);

    let origins = calculate_projectile_origins(w, this, spell, spell_type, target);

    let velocity = calculate_projectile_velocity(w, this, spell, target, spell_type, origins[0]);

    launch_spell_projectiles(
        w,
        this,
        spell,
        target,
        spell_type,
        weapon,
        is_weapon_spell,
        from_proc,
        &origins,
        velocity,
        life_projectile_damage,
    )
}

// ACE: WorldObject.ProjHeight
pub const PROJ_HEIGHT: f32 = 2.0 / 3.0;

/// The offset of the spawn point from the caster for the first projectile, in the caster's
/// local space.
// ACE: WorldObject.CalculatePreOffset
#[must_use]
pub fn calculate_pre_offset(
    w: &World,
    this: ObjectGuid,
    spell: &Spell,
    spell_type: ProjectileSpellType,
    target: Option<ObjectGuid>,
) -> Vector3 {
    let start_factor = if spell_type == ProjectileSpellType::Arc {
        1.0
    } else {
        PROJ_HEIGHT
    };

    let pre_offset = Vector3::new(0.0, 0.0, world_object_height(w, this) * start_factor);

    let Some(target) = live(w, target) else {
        return pre_offset;
    };

    let mut start_pos = physics_obj_position(w, this);
    start_pos.frame.origin.z += world_object_height(w, this) * start_factor;

    let end_factor = if spell_type == ProjectileSpellType::Arc {
        PROJ_HEIGHT_ARC
    } else {
        PROJ_HEIGHT
    };

    let mut end_pos = physics_obj_position(w, target);
    end_pos.frame.origin.z += world_object_height(w, target) * end_factor;

    let glob_offset = physics_get_offset(&start_pos, &end_pos);

    // align in x
    let rotate = Quaternion::create_from_axis_angle(
        UNIT_Z,
        f32_of_f64(empyrean_common::math::atan2(
            f64::from(glob_offset.x),
            f64::from(glob_offset.y),
        )),
    );

    let offset = Vector3::transform(glob_offset, rotate);

    let local_dir = Vector3::normalize(offset);

    let radsum = physics_obj_get_physics_radius(w, this) + get_projectile_radius(w, this, spell);

    let default_spawn_pos = Vector3::UNIT_Y * radsum;

    let spawn_pos = local_dir * radsum;

    let spawn_offset = spawn_pos - default_spawn_pos;

    pre_offset + spawn_offset
}

/// Returns a list of positions to spawn projectiles for a spell, in local space relative to the
/// caster.
// ACE: WorldObject.CalculateProjectileOrigins
#[must_use]
pub fn calculate_projectile_origins(
    w: &World,
    this: ObjectGuid,
    spell: &Spell,
    spell_type: ProjectileSpellType,
    target: Option<ObjectGuid>,
) -> Vec<Vector3> {
    let mut origins = Vec::new();

    let radius = get_projectile_radius(w, this, spell);
    //Console.WriteLine($"Radius: {radius}");

    let v_radius = Vector3::new(1.0, 1.0, 1.0) * radius;

    let mut base_offset = spell.create_offset();

    let mut radsum = physics_obj_get_physics_radius(w, this) * 2.0 + radius * 2.0;

    let height_offset = calculate_pre_offset(w, this, spell, spell_type, target);

    if let Some(target) = live(w, target) {
        let cyl_dist = world_object_get_cylinder_distance(w, this, target);
        //Console.WriteLine($"CylDist: {cylDist}");
        if cyl_dist < 0.6 {
            radsum = physics_obj_get_physics_radius(w, this) + radius;
        }
    }

    #[allow(clippy::float_cmp)]
    if spell.spread_angle() == 360.0 {
        radsum *= 0.6;
    }

    base_offset.y += radsum;

    base_offset = base_offset + height_offset;

    let angle_per_step = get_spread_angle_per_step(spell);

    // TODO: normalize data
    let db = spell.db();
    #[allow(clippy::cast_precision_loss)]
    let dims = Vector3::new(
        db.dims_origin_x.unwrap_or(spell.num_projectiles() as f32),
        db.dims_origin_y.unwrap_or(1.0),
        db.dims_origin_z.unwrap_or(1.0),
    );

    let num_projectiles = spell.num_projectiles();
    let spread_zero = spell.spread_angle() == 0.0;
    let mut i: i32 = 0;
    let mut z = 0i32;
    while (z as f32) < dims.z {
        let mut y = 0i32;
        while (y as f32) < dims.y {
            let odd_row = {
                let min: i32 =
                    math::min_f32(dims.x, num_projectiles.wrapping_sub(i) as f32).cs_cast();
                min % 2 == 1
            };

            let mut x = 0i32;
            while (x as f32) < dims.x {
                if i >= num_projectiles {
                    break;
                }

                let mut cur_offset = base_offset;

                if spell.peturbation() != Vector3::ZERO {
                    let rng = Vector3::new(
                        f32_of_f64(ThreadSafeRandom::next_float(-1.0, 1.0)),
                        f32_of_f64(ThreadSafeRandom::next_float(-1.0, 1.0)),
                        f32_of_f64(ThreadSafeRandom::next_float(-1.0, 1.0)),
                    );

                    cur_offset =
                        cur_offset + mul_v(mul_v(rng, spell.peturbation()), spell.padding());
                }

                if !odd_row && spread_zero {
                    cur_offset.x += spell.padding().x * 0.5 + radius;
                }

                let x_factor = if spread_zero {
                    if odd_row {
                        (x as f32 * 0.5).ceil()
                    } else {
                        (x as f32 * 0.5).floor()
                    }
                } else {
                    0.0
                };

                let mut origin = cur_offset
                    + mul_v(
                        v_radius * 2.0 + spell.padding(),
                        Vector3::new(x_factor, y as f32, z as f32),
                    );

                if spread_zero {
                    if x % 2 == i32::from(odd_row) {
                        origin.x *= -1.0;
                    }
                } else {
                    // get the rotation matrix to apply to x
                    let mut num_steps = (x + 1) / 2;
                    if x % 2 == 0 {
                        num_steps *= -1;
                    }

                    //Console.WriteLine($"NumSteps: {numSteps}");

                    let cur_angle = angle_per_step * num_steps as f32;
                    let rads = to_radians(cur_angle);

                    let rot = Quaternion::create_from_axis_angle(UNIT_Z, rads);
                    origin = Vector3::transform(origin, rot);
                }

                origins.push(origin);
                i += 1;
                x += 1;
            }

            if i >= num_projectiles {
                break;
            }
            y += 1;
        }

        if i >= num_projectiles {
            break;
        }
        z += 1;
    }

    /*foreach (var origin in origins)
    Console.WriteLine(origin);*/

    origins
}

/// Returns the angle in degrees between projectiles for spells with SpreadAngle.
// ACE: WorldObject.GetSpreadAnglePerStep
#[must_use]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)]
pub fn get_spread_angle_per_step(spell: &Spell) -> f32 {
    if spell.spread_angle() == 0.0 || spell.num_projectiles() == 1 {
        return 0.0;
    }

    let mut num_projectiles = spell.num_projectiles();

    if num_projectiles % 2 == 1 {
        num_projectiles -= 1;
    }

    spell.spread_angle() / num_projectiles as f32
}

/// `Quaternion.CreateFromAxisAngle(Vector3.UnitZ, (float)Math.PI)`.
// ACE: WorldObject.OneEighty
#[must_use]
pub fn one_eighty() -> Quaternion {
    Quaternion::create_from_axis_angle(UNIT_Z, f32_of_f64(std::f64::consts::PI))
}

// ACE: WorldObject.ProjHeightArc
pub const PROJ_HEIGHT_ARC: f32 = 5.0 / 6.0;

/// Calculates the spell projectile velocity in global space.
// ACE: WorldObject.CalculateProjectileVelocity
pub fn calculate_projectile_velocity(
    w: &World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    spell_type: ProjectileSpellType,
    origin: Vector3,
) -> Vector3 {
    let caster_loc = ace_position(&physics_obj_position(w, this));

    let speed = get_projectile_speed(w, this, spell, None);

    // Not ACE's (a fix, V325): a target (or a monster's attack target) that
    // left the world counts as no target, and the spell launches along the forward vector.
    let mut target = in_world(w, target);
    if target.is_none() && is_creature(w, this) && !is_player(w, this) {
        target = in_world(w, creature_attack_target(w, this));
    }

    let Some(target) = target else {
        // launch along forward vector
        return Vector3::transform(Vector3::UNIT_Y, caster_loc.rotation()) * speed;
    };

    let target_loc = ace_position(&physics_obj_position(w, target));

    let strike_spell = spell_type == ProjectileSpellType::Strike;

    let cross_landblock = !strike_spell && caster_loc.landblock() != target_loc.landblock();

    let q_dir = physics_get_offset(
        &physics_obj_position(w, this),
        &physics_obj_position(w, target),
    );
    let rotate = Quaternion::create_from_axis_angle(
        UNIT_Z,
        f32_of_f64(empyrean_common::math::atan2(
            f64::from(-q_dir.x),
            f64::from(q_dir.y),
        )),
    );

    let mut start_pos = if strike_spell {
        target_loc.pos()
    } else if cross_landblock {
        position_extensions::to_global(&caster_loc, false)
    } else {
        caster_loc.pos()
    };
    start_pos = start_pos
        + Vector3::transform(
            origin,
            if strike_spell {
                rotate * one_eighty()
            } else {
                rotate
            },
        );

    let mut end_pos = if cross_landblock {
        position_extensions::to_global(&target_loc, false)
    } else {
        target_loc.pos()
    };

    end_pos.z += world_object_height(w, target)
        * if spell_type == ProjectileSpellType::Arc {
            PROJ_HEIGHT_ARC
        } else {
            PROJ_HEIGHT
        };

    let dir = Vector3::normalize(end_pos - start_pos);

    let target_velocity = if spell.is_tracking() {
        physics_obj_cached_velocity(w, target)
    } else {
        Vector3::ZERO
    };

    let use_gravity = spell_type == ProjectileSpellType::Arc;

    let mut velocity;

    if use_gravity || target_velocity != Vector3::ZERO {
        let gravity = if use_gravity {
            trajectory::GRAVITY
        } else {
            0.0
        };
        let alt_solver = property_manager::get_bool(w, "trajectory_alt_solver", false, true).item;

        velocity = if alt_solver {
            trajectory2::calculate_trajectory(
                start_pos,
                end_pos,
                target_velocity,
                speed,
                use_gravity,
            )
        } else {
            trajectory::solve_ballistic_arc_lateral_moving(
                start_pos,
                speed,
                end_pos,
                target_velocity,
                gravity,
            )
            .fire_velocity
        };

        if velocity == Vector3::ZERO && use_gravity && target_velocity != Vector3::ZERO {
            // intractable?
            // try to solve w/ zero velocity
            velocity = if alt_solver {
                trajectory2::calculate_trajectory(
                    start_pos,
                    end_pos,
                    Vector3::ZERO,
                    speed,
                    use_gravity,
                )
            } else {
                trajectory::solve_ballistic_arc_lateral_moving(
                    start_pos,
                    speed,
                    end_pos,
                    Vector3::ZERO,
                    gravity,
                )
                .fire_velocity
            };
        }
        if velocity != Vector3::ZERO {
            return velocity;
        }
    }

    dir * speed
}

/// Creates the `SpellProjectile`s at `origins` and puts them in the world.
// ACE: WorldObject.LaunchSpellProjectiles
#[allow(clippy::too_many_arguments)]
pub fn launch_spell_projectiles(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    spell_type: ProjectileSpellType,
    weapon: Option<ObjectGuid>,
    is_weapon_spell: bool,
    from_proc: bool,
    origins: &[Vector3],
    velocity: Vector3,
    life_projectile_damage: u32,
) -> Vec<ObjectGuid> {
    let target = live(w, target);
    let use_gravity = spell_type == ProjectileSpellType::Arc;

    let strike_spell = target.is_some() && spell_type == ProjectileSpellType::Strike;

    let mut spell_projectiles = Vec::new();

    let caster_loc = ace_position(&physics_obj_position(w, this));
    let target_loc = target.map(|t| ace_position(&physics_obj_position(w, t)));

    for (i, &origin) in origins.iter().enumerate() {
        let sp = world_object_factory_create_new_world_object(w, spell.wcid())
            .filter(|&g| obj(w, g).is_some_and(WorldObject::is_spell_projectile));

        let Some(sp) = sp else {
            log::error!(
                "{} ({}).LaunchSpellProjectiles({} - {}) - failed to create spell projectile from wcid {}",
                name_of(w, this),
                this,
                spell.id(),
                spell.name(),
                spell.wcid()
            );
            break;
        };

        spell_projectile_setup(w, sp, spell, spell_type);

        let mut rotate = caster_loc.rotation();
        if let Some(target) = target {
            let q_dir = physics_get_offset(
                &physics_obj_position(w, this),
                &physics_obj_position(w, target),
            );
            rotate = Quaternion::create_from_axis_angle(
                UNIT_Z,
                f32_of_f64(empyrean_common::math::atan2(
                    f64::from(-q_dir.x),
                    f64::from(q_dir.y),
                )),
            );
        }

        let mut location = if strike_spell {
            Position::from_position(target_loc.as_ref().expect("the strike target's location"))
        } else {
            Position::from_position(&caster_loc)
        };
        location.set_pos(
            location.pos()
                + Vector3::transform(
                    origin,
                    if strike_spell {
                        rotate * one_eighty()
                    } else {
                        rotate
                    },
                ),
        );
        if let Some(o) = w.objects.get_mut(sp) {
            o.set_location(Some(location));
        }

        let mut sp_velocity = velocity;

        if spell.spread_angle() > 0.0 {
            let n = Vector3::normalize(origin);
            let angle = empyrean_common::math::atan2(f64::from(-n.x), f64::from(n.y));
            let q = Quaternion::create_from_axis_angle(UNIT_Z, f32_of_f64(angle));
            sp_velocity = Vector3::transform(velocity, q);
        }
        physics_obj_set_velocity(w, sp, sp_velocity);

        // set orientation
        let dir = Vector3::normalize(sp_velocity);
        physics_obj_set_vector_heading(w, sp, dir);

        if let Some(o) = w.objects.get_mut(sp) {
            let links = o.projectile.get_or_insert_with(Default::default);
            links.source = Some(this);

            // side projectiles always untargeted?
            if i == 0 {
                links.target = target;
            }

            links.launcher = weapon;
        }
        spell_projectile_set_fields(w, sp, from_proc, is_weapon_spell, life_projectile_damage);

        let sp_target = if i == 0 { target } else { None };
        spell_projectile_set_projectile_physics_state(w, sp, sp_target, use_gravity);

        if !crate::managers::landblock_manager::add_object(w, sp, false) {
            crate::world_objects::world_object::destroy(w, sp, true, false);
            continue;
        }

        if spell_projectile_world_entry_collision(w, sp) {
            continue;
        }

        let intensity = spell_projectile_get_projectile_script_intensity(w, sp, spell_type);
        world_object_enqueue_broadcast(
            w,
            sp,
            game_message_script(sp, empyrean_entity::enums::PlayScript::Launch, intensity),
        );

        if !crate::world_objects::world_object::is_projectile_visible(w, this, sp) {
            crate::dispatch::on_collide_environment::on_collide_environment(w, sp);
            continue;
        }

        spell_projectiles.push(sp);
    }

    spell_projectiles
}

thread_local! {
    /// ACE's static `ProjectileRadiusCache` (per world thread; see the module doc).
    // ACE: WorldObject.ProjectileRadiusCache
    static PROJECTILE_RADIUS_CACHE: RefCell<HashMap<u32, f32>> = RefCell::new(HashMap::new());

    /// ACE's static `ProjectileSpeedCache`: a temporary structure; `GetSpellProjectileSpeed()` can
    /// easily be moved to `SpellProjectile.CalculateSpeed()`, however the current calling pattern
    /// for Rings and Walls needs some work still.
    // ACE: WorldObject.ProjectileSpeedCache
    static PROJECTILE_SPEED_CACHE: RefCell<HashMap<u32, f32>> = RefCell::new(HashMap::new());
}

// ACE: WorldObject.ClearSpellCache
pub fn clear_spell_cache() {
    PROJECTILE_RADIUS_CACHE.with(|c| c.borrow_mut().clear());
    PROJECTILE_SPEED_CACHE.with(|c| c.borrow_mut().clear());
}

/// The radius of the spell's projectile weenie: its setup's first sphere times its default scale.
// ACE: WorldObject.GetProjectileRadius
#[must_use]
pub fn get_projectile_radius(w: &World, this: ObjectGuid, spell: &Spell) -> f32 {
    let projectile_wcid = spell.weenie_class_id();

    if let Some(radius) =
        PROJECTILE_RADIUS_CACHE.with(|c| c.borrow().get(&projectile_wcid).copied())
    {
        return radius;
    }

    let weenie = w.content.get_cached_weenie(projectile_wcid);

    let Some(weenie) = weenie else {
        log::error!(
            "{} ({}).GetSetupRadius({} - {}): couldn't find weenie {projectile_wcid}",
            name_of(w, this),
            this,
            spell.id(),
            spell.name()
        );
        return 0.0;
    };

    let Some(setup_id) = weenie
        .properties_did
        .as_ref()
        .and_then(|d| d.get(&PropertyDataId::Setup).copied())
    else {
        log::error!(
            "{} ({}).GetSetupRadius({} - {}): couldn't find setup ID for {} - {}",
            name_of(w, this),
            this,
            spell.id(),
            spell.name(),
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
        .expect("ACE: Setup.Spheres[0] (ArgumentOutOfRangeException)");
    let result = f32_of_f64(f64::from(sphere_radius) * scale);

    PROJECTILE_RADIUS_CACHE.with(|c| {
        c.borrow_mut().entry(projectile_wcid).or_insert(result);
    });

    result
}

/// Gets the speed of a projectile based on the distance to the target (`distance` defaults to
/// null: the base speed).
// ACE: WorldObject.GetProjectileSpeed
#[must_use]
pub fn get_projectile_speed(
    w: &World,
    this: ObjectGuid,
    spell: &Spell,
    distance: Option<f32>,
) -> f32 {
    let projectile_wcid = spell.weenie_class_id();

    let cached = PROJECTILE_SPEED_CACHE.with(|c| c.borrow().get(&projectile_wcid).copied());
    let base_speed = if let Some(base_speed) = cached {
        base_speed
    } else {
        let weenie = w.content.get_cached_weenie(projectile_wcid);

        let Some(weenie) = weenie else {
            log::error!(
                "{} ({}).GetSpellProjectileSpeed({} - {}, {distance:?}): couldn't find weenie {projectile_wcid}",
                name_of(w, this),
                this,
                spell.id(),
                spell.name()
            );
            return 0.0;
        };

        let Some(max_velocity) = weenie
            .properties_float
            .as_ref()
            .and_then(|f| f.get(&PropertyFloat::MaximumVelocity).copied())
        else {
            log::error!(
                "{} ({}).GetSpellProjectileSpeed({} - {}, {distance:?}): couldn't find MaxVelocity for {} - {}",
                name_of(w, this),
                this,
                spell.id(),
                spell.name(),
                weenie.weenie_class_id,
                weenie.class_name.as_deref().unwrap_or_default()
            );
            return 0.0;
        };

        let base_speed = f32_of_f64(max_velocity);

        PROJECTILE_SPEED_CACHE.with(|c| {
            c.borrow_mut().entry(projectile_wcid).or_insert(base_speed);
        });
        base_speed
    };

    // TODO:
    // Speed seems to increase when target is moving away from the caster and decrease when
    // the target is moving toward the caster. This still needs more research.
    let Some(distance) = distance else {
        return base_speed;
    };

    let d = f64::from(distance);
    let speed = f32_of_f64(
        f64::from(base_speed * 0.999_836_3) - f64::from(base_speed * 0.62034) / d
            + f64::from(base_speed * 0.44868) / empyrean_common::math::pow(d, 2.0)
            - f64::from(base_speed * 0.25256) / empyrean_common::math::pow(d, 3.0),
    );

    speed.clamp(1.0, 50.0)
}

// ---------------------------------------------------------------------------- item spells

/// Returns the epic cantrips from this item's spellbook (spell id => probability).
// ACE: WorldObject.EpicCantrips
#[must_use]
pub fn epic_cantrips(w: &World, this: ObjectGuid) -> Vec<(i32, f32)> {
    biota_get_matching_spells(w, this, "EpicCantrips")
}

/// Returns the legendary cantrips from this item's spellbook (spell id => probability).
// ACE: WorldObject.LegendaryCantrips
#[must_use]
pub fn legendary_cantrips(w: &World, this: ObjectGuid) -> Vec<(i32, f32)> {
    biota_get_matching_spells(w, this, "LegendaryCantrips")
}

/// The highest spell level in this object's spellbook (cached in `_maxSpellLevel`).
// ACE: WorldObject.GetMaxSpellLevel
pub fn get_max_spell_level(w: &mut World, this: ObjectGuid) -> i32 {
    let cached = obj(w, this).and_then(|o| o.wo.world_object_magic.max_spell_level);
    if let Some(v) = cached {
        return v;
    }

    let spell_ids: Vec<i32> = obj(w, this)
        .and_then(|o| o.biota.properties_spell_book.as_ref())
        .map(|b| b.keys().copied().collect())
        .unwrap_or_default();
    let max = if spell_ids.is_empty() {
        0
    } else {
        spell_ids
            .iter()
            .map(|&i| spell_level_cache_get_spell_level(w, i))
            .max()
            .unwrap_or(0)
    };
    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object_magic.max_spell_level = Some(max);
    }
    max
}

/// Calculates the StatModVal x buffs to enter into the enchantment registry. `spell` is a spell
/// with a DotDuration.
// ACE: WorldObject.CalculateDotEnchantment_StatModValue
#[must_use]
pub fn calculate_dot_enchantment_stat_mod_value(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    stat_mod_val: f32,
) -> f32 {
    // here are all the dots with current content:

    // - 3 void dots (2 projectiles, 1 direct enchantment)
    // - surge of affliction (target loses health over time)
    // - surge of regeneration (caster gains health over time)
    // - dirty fighting bleed

    #[allow(clippy::float_cmp)]
    if spell.dot_duration() == 0.0 {
        return stat_mod_val;
    }

    let mut enchantment_stat_mod_val = stat_mod_val;

    let creature_target = as_creature(w, live(w, target));

    if spell.category() == SpellCategory::AetheriaProcHealthOverTimeRaising {
        // no healing boost rating modifier found in retail pcaps on apply,
        // could there have been one on tick?
        //if (creatureTarget != null)
        //enchantment_statModVal *= creatureTarget.GetHealingRatingMod();

        return enchantment_stat_mod_val;
    }

    if spell.category() == SpellCategory::AetheriaProcDamageOverTimeRaising {
        // no mods found in retail pcaps
        return enchantment_stat_mod_val;
    }

    let player = as_player(w, Some(this));
    let creature_source = as_creature(w, Some(this));

    let mut damage_rating_mod = 1.0f32;

    if let Some(creature_source) = creature_source {
        // damage rating mod
        let mut damage_rating = creature_get_damage_rating(w, creature_source);

        if let Some(player) = player {
            // TODO: merge this with damage rating
            let equipped_weapon = creature_get_equipped_weapon(w, player)
                .or_else(|| creature_get_equipped_wand(w, player));
            if player_get_heritage_bonus(w, player, equipped_weapon) {
                damage_rating += 5;
            }

            if target.is_some_and(|t| is_player(w, t)) {
                damage_rating += player_get_pk_damage_rating(w, player);
            }
        }
        damage_rating_mod =
            crate::world_objects::creature_rating::get_positive_rating_mod(damage_rating);
    }

    if spell.category() == SpellCategory::DFBleedDamage {
        // retail pcaps have modifiers in the range of 1.1x - 1.7x
        return enchantment_stat_mod_val * damage_rating_mod;
    }

    if spell.category() != SpellCategory::NetherDamageOverTimeRaising
        && spell.category() != SpellCategory::NetherDamageOverTimeRaising2
        && spell.category() != SpellCategory::NetherDamageOverTimeRaising3
    {
        log::error!(
            "{}.CalculateDamageOverTimeBase({} - {}, {}) - unknown dot spell category {}",
            name_of(w, this),
            spell.id(),
            spell.name(),
            target.map(|t| name_of(w, t)).unwrap_or_default(),
            spell.category().to_dotnet_string()
        );
        return enchantment_stat_mod_val;
    }

    // factors:
    // - damage rating
    // - heritage bonus (universal masteries at end of retail, TODO: merge this with damage rating)
    // - caster damage type bonus (pvm, half for pvp)
    // - skill in magic school vs. spell difficulty (for projectiles)

    // thanks to Xenocide for figuring this part out!

    let mut elemental_damage_mod = 1.0f32;
    let mut skill_mod = 1.0f32;

    if let Some(creature_source) = creature_source {
        // elemental damage mod
        elemental_damage_mod = world_object_get_caster_elemental_damage_modifier(
            w,
            weapon,
            creature_source,
            creature_target,
            spell.damage_type(),
        );

        // skillMod only applied to projectiles -- no destructive curse
        if let Some(player) = player.filter(|_| spell.num_projectiles() > 0) {
            // from SpellProjectile, slightly modified
            // convert this to common function
            let magic_skill = creature_get_creature_skill_school_current(w, player, spell.school());

            if magic_skill > spell.power() {
                let percentage_bonus = f32_of(magic_skill - spell.power()) / 1000.0;

                skill_mod = 1.0 + percentage_bonus;
            }
        }
    }
    enchantment_stat_mod_val *= skill_mod * elemental_damage_mod * damage_rating_mod;

    enchantment_stat_mod_val
}

/// Item spells cast on a target, redirected to its equipped items where ACE does (impen / bane /
/// brittlemail / lure; blood loather, spirit loather, lure blade, turn blade, leaden weapon,
/// hermetic void). `item_caster` defaults to null.
// ACE: WorldObject.TryCastItemEnchantment_WithRedirects
#[allow(clippy::too_many_lines)]
pub fn try_cast_item_enchantment_with_redirects(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
    item_caster: Option<ObjectGuid>,
) {
    let caster = live(w, item_caster).unwrap_or(this);

    let creature = as_creature(w, Some(this));
    let player = as_player(w, Some(this));

    let target = live(w, target);
    let target_creature = as_creature(w, target);
    let target_player = as_player(w, target);

    // if negative item spell, can be resisted by the wielder
    if spell.is_harmful() {
        let mut target_resist = target_creature;

        if target_resist.is_none() {
            if let Some(wielder_id) = target
                .and_then(|t| obj(w, t))
                .and_then(WorldObject::wielder_id)
            {
                // `CurrentLandblock?.GetObject(target.WielderId.Value) as Creature`
                let on_landblock = obj(w, this).and_then(|o| o.current_landblock).is_some();
                target_resist = if on_landblock {
                    as_creature(w, live(w, Some(ObjectGuid::new(wielder_id))))
                } else {
                    None
                };
            }
        }

        // skip TryResistSpell() for non-player casters, they already performed it previously
        if player.is_some() {
            if let Some(target_resist) = target_resist {
                if try_resist_spell(w, this, Some(target_resist), spell, Some(caster), false) {
                    return;
                }
            }
        }
        // should this be set if the spell is invalid / 'fails to affect' below?
        if let (Some(creature), Some(player_target_resist)) =
            (creature, as_player(w, target_resist))
        {
            player_set_current_attacker(w, player_target_resist, creature);
        }
    }

    let fails_to_affect = |w: &mut World, target_creature: ObjectGuid| {
        // 'fails to affect'?
        if let Some(player) = player {
            let text = format!(
                "You fail to affect {} with {}",
                name_of(w, target_creature),
                spell.name()
            );
            send(
                w,
                player,
                game_message_system_chat(&text, ChatMessageType::Magic),
            );
        }

        if let Some(target_player) = target_player {
            if !squelch_manager_squelches_contains(
                w,
                target_player,
                Some(this),
                ChatMessageType::Magic,
            ) {
                let text = format!(
                    "{} fails to affect you with {}",
                    name_of(w, this),
                    spell.name()
                );
                send(
                    w,
                    target_player,
                    game_message_system_chat(&text, ChatMessageType::Magic),
                );
            }
        }
    };

    if spell.is_impen_bane_type() {
        // impen / bane / brittlemail / lure

        // a lot of these will already be filtered out by IsInvalidTarget()
        match target_creature {
            None => {
                // targeting an individual item / wo
                handle_cast_spell(w, this, spell, target, None, None, false, false, false);
            }
            Some(target_creature) => {
                // targeting a creature
                if target_player == Some(this) {
                    // targeting self
                    if let Some(creature) = creature {
                        let items: Vec<ObjectGuid> = creature_equipped_objects(w, creature)
                            .into_iter()
                            .filter(|&i| {
                                obj(w, i).is_some_and(|o| {
                                    (o.biota.weenie_type == WeenieType::Clothing || o.is_shield())
                                        && is_enchantable(o)
                                })
                            })
                            .collect();

                        for &item in &items {
                            handle_cast_spell(
                                w,
                                this,
                                spell,
                                Some(item),
                                None,
                                None,
                                false,
                                false,
                                false,
                            );
                        }

                        if !items.is_empty() {
                            do_spell_effects(w, this, spell, this, Some(creature), false);
                        }
                    }
                } else {
                    // targeting another player or monster
                    let item = creature_equipped_objects(w, target_creature)
                        .into_iter()
                        .find(|&i| obj(w, i).is_some_and(|o| o.is_shield() && is_enchantable(o)));

                    if let Some(item) = item {
                        handle_cast_spell(
                            w,
                            this,
                            spell,
                            Some(item),
                            None,
                            None,
                            false,
                            false,
                            false,
                        );
                    } else {
                        fails_to_affect(w, target_creature);
                    }
                }
            }
        }
    } else if spell.is_other_negative_redirectable() || spell.is_item_redirectable_type() {
        // blood loather, spirit loather, lure blade, turn blade, leaden weapon, hermetic void
        match target_creature {
            None => {
                // targeting an individual item / wo
                handle_cast_spell(w, this, spell, target, None, None, false, false, false);
            }
            Some(target_creature) => {
                // targeting a creature, try to redirect to primary weapon
                let weapon = match spell.non_component_target_type() {
                    ItemType::Weapon => creature_get_equipped_weapon(w, target_creature),
                    ItemType::Caster => creature_get_equipped_wand(w, target_creature),
                    ItemType::WeaponOrCaster => creature_get_equipped_weapon(w, target_creature)
                        .or_else(|| creature_get_equipped_wand(w, target_creature)),
                    ItemType::MeleeWeapon => creature_get_equipped_melee_weapon(w, target_creature),
                    ItemType::MissileWeapon => {
                        creature_get_equipped_missile_weapon(w, target_creature)
                    }
                    _ => None,
                };

                if let Some(weapon) = weapon.filter(|&wpn| obj(w, wpn).is_some_and(is_enchantable))
                {
                    handle_cast_spell(
                        w,
                        this,
                        spell,
                        Some(weapon),
                        None,
                        None,
                        false,
                        false,
                        false,
                    );
                } else {
                    fails_to_affect(w, target_creature);
                }
            }
        }
    } else {
        // all other item spells, cast directly on target
        handle_cast_spell(w, this, spell, target, None, None, false, false, false);
    }
}

// ACE: WorldObject.OnSpellsActivated
pub fn on_spells_activated(w: &mut World, this: ObjectGuid) {
    if let Some(o) = w.objects.get_mut(this) {
        o.set_is_affecting(true);
        o.wo.world_object_magic.item_mana_rate_accumulator = 0.0;
        o.wo.world_object_magic.item_mana_depletion_message = false;
    }
}

// ACE: WorldObject.OnSpellsDeactivated
pub fn on_spells_deactivated(w: &mut World, this: ObjectGuid) {
    if let Some(o) = w.objects.get_mut(this) {
        o.set_is_affecting(false);
    }
}

// ACE: WorldObject.defaultIgnoreSomeMagicProjectileDamage
const DEFAULT_IGNORE_SOME_MAGIC_PROJECTILE_DAMAGE: f64 = 0.25;

/// `AbsorbMagicDamage`, or 0.25 for an item imbued with IgnoreSomeMagicProjectileDamage.
// ACE: WorldObject.GetAbsorbMagicDamage
#[must_use]
pub fn get_absorb_magic_damage(o: &WorldObject) -> Option<f64> {
    let mut absorb_magic_damage = o.absorb_magic_damage();

    let imbued = o.imbued_effect();
    let flag = ImbuedEffectType::IgnoreSomeMagicProjectileDamage;
    if absorb_magic_damage.is_none() && (imbued.0 & flag.0) == flag.0 {
        absorb_magic_damage = Some(DEFAULT_IGNORE_SOME_MAGIC_PROJECTILE_DAMAGE);
    }

    absorb_magic_damage
}

/// For spells with NonComponentTargetType, returns the list of equipped items matching the target
/// type (`None` for ACE's `null`: the spell is not redirected).
// ACE: WorldObject.GetNonComponentTargetTypes
#[must_use]
pub fn get_non_component_target_types(
    w: &World,
    spell: &Spell,
    target: ObjectGuid,
) -> Option<Vec<ObjectGuid>> {
    let nctt = spell.non_component_target_type();
    match nctt {
        ItemType::Vestements      // impen / bane
        | ItemType::Weapon        // blood drinker
        | ItemType::LockableMagicTarget // strengthen lock
        | ItemType::Caster        // hermetic void
        | ItemType::WeaponOrCaster // lure blade, defender cantrip, hermetic link cantrip, mukkir sense
        | ItemType::Item => {
            // essence lull
            Some(
                creature_equipped_objects(w, target)
                    .into_iter()
                    .filter(|&i| {
                        obj(w, i).is_some_and(|o| {
                            (o.item_type().0 & nctt.0) != 0
                                && (o.valid_locations().unwrap_or(EquipMask(0)).0 & EquipMask::Selectable.0) != 0
                                && is_enchantable(o)
                        })
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Not ACE: shims and pointers to members of other ACE classes, named after ACE's members. Each
// calls `not_ported!` with that name and answers the value noted, unless it is marked an exact
// copy of a small ACE body.
// ---------------------------------------------------------------------------------------------

/// `WorldObject.IsEnchantable` (`WorldObject_Weapon.cs`): `(ResistMagic ?? 0) < 9999`. Exact.
fn is_enchantable(o: &WorldObject) -> bool {
    o.resist_magic().unwrap_or(0) < 9999
}

/// `Cloak.IsCloak(wo)` (`Entity/Cloak.cs`).
fn cloak_is_cloak(w: &World, wo: ObjectGuid) -> bool {
    obj(w, wo).is_some_and(crate::entity::cloak::is_cloak)
}

/// `Portal.NoRecall` (`Portal_Properties.cs`): `(PortalRestrictions & NoRecall) != 0`. Exact.
fn portal_no_recall(p: &WorldObject) -> bool {
    p.no_recall()
}

/// `Portal.NoTie => NoRecall`. Exact.
fn portal_no_tie(p: &WorldObject) -> bool {
    p.no_tie()
}

/// `Portal.NoSummon`: `(PortalRestrictions & NoSummon) != 0`. Exact.
fn portal_no_summon(p: &WorldObject) -> bool {
    p.no_summon()
}

/// The biota's skill record: `(InitLevel, Ranks)` (`CreatureSkill.InitLevel` / `.Ranks`, which
/// read `PropertiesSkill.InitLevel` / `.LevelFromPP`); zeroes when the biota has no such skill.
fn biota_skill(o: &WorldObject, skill: Skill) -> (u32, u16) {
    o.biota
        .get_skill(skill)
        .map_or((0, 0), |s| (s.init_level, s.level_from_pp))
}

/// `GetCreatureSkill(skill).Current`: 4.0's `CreatureSkill.Current` on the creature in the world
/// (`GetCreatureSkill` adds a missing skill as Untrained, as ACE's default `add: true` does).
#[must_use]
pub fn creature_get_creature_skill_current(w: &mut World, this: ObjectGuid, skill: Skill) -> u32 {
    let creature_skill = w
        .objects
        .get_mut(this)
        .expect("ACE: Creature is null (NullReferenceException)")
        .get_creature_skill(skill, true)
        .expect("GetCreatureSkill(skill, add: true) always answers");
    creature_skill.current(w, this)
}

/// The school's skill: `GetCreatureSkill(MagicSchool)` answers null for any other school.
fn magic_school_skill(school: MagicSchool) -> Skill {
    match school {
        MagicSchool::CreatureEnchantment => Skill::CreatureEnchantment,
        MagicSchool::ItemEnchantment => Skill::ItemEnchantment,
        MagicSchool::LifeMagic => Skill::LifeMagic,
        MagicSchool::VoidMagic => Skill::VoidMagic,
        MagicSchool::WarMagic => Skill::WarMagic,
        _ => panic!("ACE: Creature.GetCreatureSkill({school:?}) is null (NullReferenceException)"),
    }
}

/// `GetCreatureSkill(MagicSchool).Current`.
fn creature_get_creature_skill_school_current(
    w: &mut World,
    this: ObjectGuid,
    school: MagicSchool,
) -> u32 {
    creature_get_creature_skill_current(w, this, magic_school_skill(school))
}

/// `GetCreatureSkill(MagicSchool)`'s `InitLevel` and `Ranks` (exact: both read the biota record).
fn creature_get_creature_skill_school_init_and_ranks(
    w: &World,
    this: ObjectGuid,
    school: MagicSchool,
) -> (u32, u16) {
    let o = obj(w, this).expect("ACE: Creature is null (NullReferenceException)");
    biota_skill(o, magic_school_skill(school))
}

/// `Creature.GetEffectiveMagicDefense()` (`Creature_Magic.cs`).
fn creature_get_effective_magic_defense(w: &mut World, this: ObjectGuid) -> u32 {
    crate::world_objects::creature_magic::get_effective_magic_defense(w, this)
}

/// `Vitals[vital]` (4.0's live `CreatureVital`); `vital` is the max-vital key (`MaxHealth`, ...).
fn creature_vital(w: &World, this: ObjectGuid, vital: PropertyAttribute2nd) -> CreatureVital {
    let o = obj(w, this).expect("ACE: Creature is null (NullReferenceException)");
    *o.vitals().get(&vital).unwrap_or_else(|| {
        panic!(
            "KeyNotFoundException: Creature.Vitals[{}]",
            vital.to_dotnet_string()
        )
    })
}

/// `Vitals[vital].Current` (4.0's `CreatureVital.Current`).
#[must_use]
pub fn vital_current(w: &World, this: ObjectGuid, vital: PropertyAttribute2nd) -> u32 {
    let o = obj(w, this).expect("ACE: Creature is null (NullReferenceException)");
    creature_vital(w, this, vital).current(o)
}

/// `Vitals[vital].MaxValue` (4.0's `CreatureVital.MaxValue`, through the creature's caching
/// `EnchantmentManager`).
#[must_use]
pub fn vital_max_value(w: &mut World, this: ObjectGuid, vital: PropertyAttribute2nd) -> u32 {
    let v = creature_vital(w, this, vital);
    v.max_value(&mut StatCtx::in_world(w, this))
}

/// `Vitals[vital].Missing` (4.0's `CreatureVital.Missing`).
fn vital_missing(w: &mut World, this: ObjectGuid, vital: PropertyAttribute2nd) -> u32 {
    let v = creature_vital(w, this, vital);
    v.missing(&mut StatCtx::in_world(w, this))
}

/// `Creature.UpdateVitalDelta(vital, delta)` (4.0's port; the virtual `UpdateVital`, so a player
/// also gets its vital update message).
pub fn creature_update_vital_delta(
    w: &mut World,
    this: ObjectGuid,
    vital: PropertyAttribute2nd,
    delta: i32,
) -> i32 {
    let v = creature_vital(w, this, vital);
    crate::world_objects::creature_vitals::update_vital_delta(w, this, v, delta)
}

/// `Creature.IsAlive => Health.Current > 0`. Exact.
fn creature_is_alive(w: &World, this: ObjectGuid) -> bool {
    vital_current(w, this, PropertyAttribute2nd::MaxHealth) > 0
}

/// `Creature.IsDead => Health.Current <= 0` (`Monster_Combat.cs`). Exact.
fn creature_is_dead(w: &World, this: ObjectGuid) -> bool {
    vital_current(w, this, PropertyAttribute2nd::MaxHealth) == 0
}

/// `Creature.GetResistanceMod(ResistanceType, null, null, 1.0f)` (`Creature_Properties.cs`).
fn creature_get_resistance_mod(w: &mut World, this: ObjectGuid, resistance: ResistanceType) -> f64 {
    crate::world_objects::creature_properties::get_resistance_mod(
        w, this, resistance, None, None, 1.0,
    )
}

/// `Creature.EquippedCloak` (`Creature_Combat.cs`).
fn creature_equipped_cloak(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::creature_combat::equipped_cloak(w, this)
}

/// `Creature.EquippedObjects.Values` (4.5a).
fn creature_equipped_objects(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    crate::world_objects::creature_equipment::equipped_objects_values(w, this)
}

/// `Creature.GetEquippedWeapon()` (4.5a; `forceMainHand` defaults to false).
fn creature_get_equipped_weapon(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::creature_equipment::get_equipped_weapon(w, this, false)
}

/// `Creature.GetEquippedWand()` (4.5a).
fn creature_get_equipped_wand(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::creature_equipment::get_equipped_wand(w, this)
}

/// `Creature.GetEquippedMeleeWeapon()` (4.5a; `forceMainHand` defaults to false).
fn creature_get_equipped_melee_weapon(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::creature_equipment::get_equipped_melee_weapon(w, this, false)
}

/// `Creature.GetEquippedMissileWeapon()` (4.5a).
fn creature_get_equipped_missile_weapon(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::creature_equipment::get_equipped_missile_weapon(w, this)
}

/// `Cloak.HasDamageProc(cloak)`.
fn cloak_has_damage_proc(w: &World, cloak: ObjectGuid) -> bool {
    crate::entity::cloak::has_damage_proc(w.objects.get(cloak))
}

/// `Cloak.RollProc(cloak, percent)`.
fn cloak_roll_proc(w: &mut World, cloak: ObjectGuid, percent: f32) -> bool {
    crate::entity::cloak::roll_proc(w, cloak, percent)
}

/// `Cloak.GetReducedAmount(source, int damage)`.
fn cloak_get_reduced_amount(w: &World, source: ObjectGuid, damage: i32) -> i32 {
    crate::entity::cloak::get_reduced_amount_int(w, Some(source), damage)
}

/// `Cloak.GetReducedAmount(source, uint damage)`.
fn cloak_get_reduced_amount_uint(w: &World, source: ObjectGuid, damage: u32) -> u32 {
    crate::entity::cloak::get_reduced_amount_uint(w, Some(source), damage)
}

/// `Cloak.ShowMessage(defender, attacker, int damage, int reduced)`.
fn cloak_show_message(
    w: &mut World,
    defender: ObjectGuid,
    attacker: ObjectGuid,
    damage: i32,
    reduced: i32,
) {
    crate::entity::cloak::show_message(w, defender, Some(attacker), damage, reduced);
}

/// `Cloak.ShowMessage(defender, attacker, uint damage, uint reduced)`: C# picks the `float`
/// overload for `uint` arguments.
#[allow(clippy::cast_precision_loss)]
fn cloak_show_message_uint(
    w: &mut World,
    defender: ObjectGuid,
    attacker: ObjectGuid,
    damage: u32,
    reduced: u32,
) {
    crate::entity::cloak::show_message_float(
        w,
        defender,
        Some(attacker),
        damage as f32,
        reduced as f32,
    );
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

/// `creature.DamageHistory.OnHeal(amount)` (4.9).
fn damage_history_on_heal(w: &mut World, this: ObjectGuid, amount: u32) {
    crate::entity::damage_history::on_heal(w, this, amount);
}

/// `creature.DamageHistory.Add(attacker, damageType, amount)` (4.9).
fn damage_history_add(
    w: &mut World,
    this: ObjectGuid,
    attacker: ObjectGuid,
    damage_type: DamageType,
    amount: u32,
) {
    crate::entity::damage_history::add(w, this, attacker, damage_type, amount);
}

/// `creature.DamageHistory.LastDamager`.
fn damage_history_last_damager(w: &World, this: ObjectGuid) -> Option<DamageHistoryInfo> {
    crate::entity::damage_history::of(w, this).last_damager()
}

/// `creature.Die()`: `Die(DamageHistory.LastDamager, DamageHistory.TopDamager)`, the virtual.
fn creature_die(w: &mut World, this: ObjectGuid) {
    crate::world_objects::creature_death::die(w, this);
}

/// `targetCreature.EmoteManager.OnResistSpell(caster)`.
fn emote_manager_on_resist_spell(w: &mut World, this: ObjectGuid, caster: ObjectGuid) {
    crate::world_objects::managers::emote_manager::on_resist_spell(w, this, Some(caster));
}

/// `targetCreature.EmoteManager.OnDamage(attacker)`.
fn emote_manager_on_damage(w: &mut World, this: ObjectGuid, attacker: Option<ObjectGuid>) {
    crate::world_objects::managers::emote_manager::on_damage(w, this, attacker);
}

/// `Proficiency.OnSuccessUse(player, player.GetCreatureSkill(skill), difficulty)`.
fn proficiency_on_success_use(w: &mut World, player: ObjectGuid, skill: Skill, difficulty: u32) {
    let skill = crate::entity::proficiency::get_creature_skill(w, player, skill);
    crate::entity::proficiency::on_success_use(w, player, skill, difficulty);
}

/// `player.SendChatMessage(source, msg, msgType)` (`Player_Magic.cs`): unless the player squelches
/// the source, `Session.Network.EnqueueSend(new GameMessageSystemChat(msg, msgType))`. Exact,
/// with the squelch pointer below.
fn player_send_chat_message(
    w: &mut World,
    player: ObjectGuid,
    source: Option<ObjectGuid>,
    msg: &str,
    msg_type: ChatMessageType,
) {
    if !squelch_manager_squelches_contains(w, player, source, msg_type) {
        send(w, player, game_message_system_chat(msg, msg_type));
    }
}

/// `player.SquelchManager.Squelches.Contains(source, msgType)`.
fn squelch_manager_squelches_contains(
    w: &World,
    player: ObjectGuid,
    source: Option<ObjectGuid>,
    msg_type: ChatMessageType,
) -> bool {
    crate::world_objects::managers::squelch_manager::squelches_contains(w, player, source, msg_type)
}

/// `player.SendWeenieError(error)` (`Player_Networking.cs`). Exact.
fn player_send_weenie_error(w: &mut World, player: ObjectGuid, error: WeenieError) {
    send_weenie_error_event(w, player, error);
}

/// `player.SetCurrentAttacker(creature)` (`Player_Combat.cs`).
fn player_set_current_attacker(w: &mut World, player: ObjectGuid, attacker: ObjectGuid) {
    crate::world_objects::player_combat::set_current_attacker(w, player, attacker);
}

/// `player.HandleSpellHooks(spell)` (`Player_Spells.cs`).
fn player_handle_spell_hooks(w: &mut World, player: ObjectGuid, spell: &Spell) {
    crate::world_objects::player_spells::handle_spell_hooks(w, player, spell);
}

/// `player.PKTimerActive` (`Player_Combat.cs`).
fn player_pk_timer_active(w: &World, player: ObjectGuid) -> bool {
    crate::world_objects::player_combat::pk_timer_active(w, player)
}

/// `player.IsOlthoiPlayer` (`Player_Properties.cs`).
fn player_is_olthoi_player(w: &World, player: ObjectGuid) -> bool {
    crate::entity::damage_history_info::player_is_olthoi_player(w, player)
}

/// `player.HandleLifestoneProtection()` (`Player_Death.cs`).
fn player_handle_lifestone_protection(w: &mut World, player: ObjectGuid) {
    crate::world_objects::player_death::handle_lifestone_protection(w, player);
}

/// `player.Fellowship?.GetFellowshipMembers().Values` (null without a fellowship).
fn player_fellowship_get_fellowship_members(
    w: &mut World,
    player: ObjectGuid,
) -> Option<Vec<ObjectGuid>> {
    let fellowship = crate::world_objects::player_fellowship::fellowship(w, player)?;
    Some(
        crate::entity::fellowship::get_fellowship_members(w, &fellowship)
            .values()
            .copied()
            .collect(),
    )
}

/// `player.HasFoci(school)` (`Player_Spells.cs`).
#[must_use]
pub fn player_has_foci(w: &World, player: ObjectGuid, school: MagicSchool) -> bool {
    crate::world_objects::player_spells::has_foci(w, player, school)
}

/// `player.DoPreTeleportHide()` (`Player_Location.cs`).
fn player_do_pre_teleport_hide(w: &mut World, player: ObjectGuid) {
    crate::world_objects::player_location::do_pre_teleport_hide(w, player);
}

/// `player.TeleToPosition(positionType)` (`Player_Location.cs`; the result is discarded here).
fn player_tele_to_position(w: &mut World, player: ObjectGuid, position_type: PositionType) {
    let _ = crate::world_objects::player_location::tele_to_position(w, player, position_type);
}

/// `player.Teleport(position)` (`Player_Location.cs`; `fromPortal = false`).
fn player_teleport(w: &mut World, player: ObjectGuid, position: Position) {
    crate::world_objects::player_location::teleport(w, player, &position, false);
}

/// `player.SendTeleportedViaMagicMessage(itemCaster, spell)` (`Player_Location.cs`).
fn player_send_teleported_via_magic_message(
    w: &mut World,
    player: ObjectGuid,
    item_caster: Option<ObjectGuid>,
    spell: &Spell,
) {
    crate::world_objects::player_location::send_teleported_via_magic_message(
        w,
        player,
        item_caster,
        spell,
    );
}

/// `creature.FakeTeleport(position)` (`Creature_Navigation.cs`).
fn creature_fake_teleport(w: &mut World, creature: ObjectGuid, position: Position) {
    crate::world_objects::creature_navigation::fake_teleport(w, creature, &position);
}

/// `creature.DebugDamage` (a `Creature_Combat.cs` field).
fn creature_debug_damage(w: &World, creature: ObjectGuid) -> DebugDamageType {
    let o = obj(w, creature).expect("ACE: creature is null (NullReferenceException)");
    crate::world_objects::creature_combat::fields(o).debug_damage
}

/// `creature.DebugDamage = value`.
fn creature_set_debug_damage(w: &mut World, creature: ObjectGuid, value: DebugDamageType) {
    let o = w
        .objects
        .get_mut(creature)
        .expect("ACE: creature is null (NullReferenceException)");
    crate::world_objects::creature_combat::fields_mut(o).debug_damage = value;
}

/// `creature.DebugDamageTarget` (a `Creature_Combat.cs` `ObjectGuid` field): its full
/// guid (0 when never set).
fn creature_debug_damage_target(w: &World, creature: ObjectGuid) -> Option<u32> {
    let o = obj(w, creature).expect("ACE: creature is null (NullReferenceException)");
    Some(
        crate::world_objects::creature_combat::fields(o)
            .debug_damage_target
            .full(),
    )
}

/// `player.DebugDamageBuffer = value` (a `Player_Combat.cs` field).
fn player_set_debug_damage_buffer(w: &mut World, player: ObjectGuid, value: String) {
    crate::world_objects::player_magic::fields_mut(w, player).debug_damage_buffer = Some(value);
}

/// `creature.AttackTarget` (`Monster_Combat.cs`).
fn creature_attack_target(w: &World, creature: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::monster_combat::attack_target(w, creature)
}

/// `creature.GetDamageRating()` (`Creature_Rating.cs`, 4.8a's port; fills the enchantment
/// rating cache as ACE's `EnchantmentManagerWithCaching` does).
fn creature_get_damage_rating(w: &mut World, creature: ObjectGuid) -> i32 {
    crate::world_objects::creature_rating::get_damage_rating(w, creature)
}

/// `player.GetHeritageBonus(weapon)` (virtual; `Player_Skills.cs` for a player). A null weapon is
/// `ObjectGuid::INVALID`, which no live object has: the override answers false for it, as ACE's
/// `weapon == null` check does.
fn player_get_heritage_bonus(w: &World, player: ObjectGuid, weapon: Option<ObjectGuid>) -> bool {
    crate::dispatch::get_heritage_bonus::get_heritage_bonus(
        w,
        player,
        weapon.unwrap_or(ObjectGuid::INVALID),
    )
}

/// `player.GetPKDamageRating()` (`Creature_Rating.cs`).
fn player_get_pk_damage_rating(w: &mut World, player: ObjectGuid) -> i32 {
    crate::world_objects::creature_rating::get_pk_damage_rating(w, player)
}

/// `WorldObject.GetCasterElementalDamageModifier(weapon, wielder, target, damageType)`
/// (`WorldObject_Weapon.cs`, 4.8a's port).
fn world_object_get_caster_elemental_damage_modifier(
    w: &mut World,
    weapon: Option<ObjectGuid>,
    wielder: ObjectGuid,
    target: Option<ObjectGuid>,
    damage_type: DamageType,
) -> f32 {
    crate::world_objects::world_object_weapon::get_caster_elemental_damage_modifier(
        w,
        weapon,
        Some(wielder),
        target,
        damage_type,
    )
}

/// `SpellLevelCache.GetSpellLevel(spellId)` (4.10).
fn spell_level_cache_get_spell_level(w: &World, spell_id: i32) -> i32 {
    crate::factories::entity::spell_level_cache::get_spell_level(w, spell_id)
}

/// `Biota.GetMatchingSpells(LootTables.<table>, BiotaDatabaseLock)`, in spell-book order.
fn biota_get_matching_spells(w: &World, this: ObjectGuid, table: &str) -> Vec<(i32, f32)> {
    let sets = &*crate::factories::loot_tables::CANTRIP_SETS;
    let set = match table {
        "EpicCantrips" => &sets.epic_cantrips,
        "LegendaryCantrips" => &sets.legendary_cantrips,
        _ => unreachable!("LootTables has no {table} set"),
    };
    w.objects
        .get(this)
        .expect("ACE: this is null")
        .biota
        .get_matching_spells(set)
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect()
}

/// `creature.GetDistance(target)` (`Creature_Navigation.cs`).
fn world_object_get_distance(w: &World, this: ObjectGuid, wo: ObjectGuid) -> f32 {
    crate::world_objects::creature_navigation::get_distance(w, this, wo)
}

/// `WorldObject.GetCylinderDistance(wo)` (`WorldObject_Use.cs`).
fn world_object_get_cylinder_distance(w: &World, this: ObjectGuid, wo: ObjectGuid) -> f32 {
    crate::world_objects::world_object_use::get_cylinder_distance(w, this, wo)
}

/// The result of `CheckUseRequirements`: `ActivationResult.Success` and `.Message`.
struct ActivationResultView {
    success: bool,
    message: Option<GameMessage>,
}

/// `portal.CheckUseRequirements(player)` on the detached portal `GetPortal` built (`Portal.cs`,
/// virtual). The portal is lent to `World.objects` for the call (the port's members address
/// objects by guid) and handed back.
// DIVERGE: ACE calls the method on an object outside the world; here the detached portal (guid = its wcid) joins World.objects for the call only, so anything it queues against itself (a portal quest's OnQuest emote chain) finds it gone afterwards.
fn portal_check_use_requirements(
    w: &mut World,
    portal: WorldObject,
    player: ObjectGuid,
) -> (ActivationResultView, WorldObject) {
    let guid = portal.guid;
    if let Err(dup) = w.objects.insert(portal) {
        panic!("GetPortal: a live object already has guid {:?}", dup.guid);
    }
    let result = crate::dispatch::check_use_requirements::check_use_requirements(w, guid, player);
    let portal = *w.objects.remove(guid).expect("the portal lent above");
    (
        ActivationResultView {
            success: result.success,
            message: result.message,
        },
        portal,
    )
}

/// `gateway.UpdatePortalDestination(destination)` (`Portal.cs`).
fn portal_update_portal_destination(w: &mut World, gateway: ObjectGuid, destination: Position) {
    let o = w
        .objects
        .get_mut(gateway)
        .expect("ACE: gateway is null (NullReferenceException)");
    crate::world_objects::portal::update_portal_destination(o, Some(destination));
}

/// `WorldObjectFactory.CreateNewWorldObject(weenieClassName)`, put in `World.objects`.
fn world_object_factory_create_new_world_object_by_name(
    w: &mut World,
    weenie_class_name: &str,
) -> Option<ObjectGuid> {
    crate::factories::world_object_factory::create_new_world_object_by_name_in_world(
        w,
        weenie_class_name,
    )
}

/// `WorldObjectFactory.CreateNewWorldObject(weenieClassId)`: the cached weenie, a new dynamic
/// guid, the constructor (guid recycled when it builds nothing); the object joins `World.objects`.
fn world_object_factory_create_new_world_object(
    w: &mut World,
    weenie_class_id: u32,
) -> Option<ObjectGuid> {
    let weenie = w.content.get_cached_weenie(weenie_class_id)?;
    let guid = crate::managers::guid_manager::new_dynamic_guid(w);
    let wo = crate::world_objects::world_object::CtorEnv::with_world(w, |env| {
        crate::factories::world_object_factory::create_world_object(env, Some(weenie), guid)
    });
    let Some(wo) = wo else {
        crate::managers::guid_manager::recycle_dynamic_guid(w, guid);
        return None;
    };
    let guid = wo.guid;
    w.objects.insert(wo).ok()?;
    crate::world_objects::creature::post_insert(w, guid);
    Some(guid)
}

/// `SpellProjectile.GetProjectileSpellType(spellID)` (the leaf class): Undef.
fn spell_projectile_get_projectile_spell_type(w: &World, spell_id: u32) -> ProjectileSpellType {
    crate::world_objects::spell_projectile::get_projectile_spell_type(w, spell_id)
}

/// `sp.Setup(spell, spellType)`.
fn spell_projectile_setup(
    w: &mut World,
    sp: ObjectGuid,
    spell: &Spell,
    spell_type: ProjectileSpellType,
) {
    crate::world_objects::spell_projectile::setup(w, sp, spell, spell_type);
}

/// `sp.FromProc = fromProc; sp.IsWeaponSpell = isWeaponSpell; sp.SpawnPos = new
/// Position(sp.Location); sp.LifeProjectileDamage = lifeProjectileDamage` (SpellProjectile's
/// fields).
fn spell_projectile_set_fields(
    w: &mut World,
    sp: ObjectGuid,
    from_proc: bool,
    is_weapon_spell: bool,
    life_projectile_damage: u32,
) {
    let spawn_pos = obj(w, sp)
        .and_then(WorldObject::location)
        .map(|l| Position::from_position(&l));
    let f = crate::world_objects::spell_projectile::fields_mut(w, sp);
    f.from_proc = from_proc;
    f.is_weapon_spell = is_weapon_spell;
    f.spawn_pos = spawn_pos;
    f.life_projectile_damage = life_projectile_damage;
}

/// `sp.SetProjectilePhysicsState(target, useGravity)`.
fn spell_projectile_set_projectile_physics_state(
    w: &mut World,
    sp: ObjectGuid,
    target: Option<ObjectGuid>,
    use_gravity: bool,
) {
    crate::world_objects::spell_projectile::set_projectile_physics_state(
        w,
        sp,
        target,
        use_gravity,
    );
}

/// `sp.WorldEntryCollision`: false.
fn spell_projectile_world_entry_collision(w: &World, sp: ObjectGuid) -> bool {
    crate::world_objects::spell_projectile::fields(w, sp).world_entry_collision
}

/// `sp.GetProjectileScriptIntensity(spellType)`: 1.0.
fn spell_projectile_get_projectile_script_intensity(
    w: &World,
    sp: ObjectGuid,
    spell_type: ProjectileSpellType,
) -> f32 {
    crate::world_objects::spell_projectile::get_projectile_script_intensity(w, sp, spell_type)
}

/// `wo.EnqueueBroadcast(msg)` (`WorldObject_Networking.cs`; `sendSelf = true`).
fn world_object_enqueue_broadcast(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    crate::world_objects::world_object_networking::enqueue_broadcast(w, this, true, &[msg]);
}

/// `wo.EnqueueBroadcast(msg, range, squelchType)` (`WorldObject_Networking.cs`).
fn world_object_enqueue_broadcast_range(
    w: &mut World,
    this: ObjectGuid,
    msg: GameMessage,
    range: f32,
    squelch_type: Option<ChatMessageType>,
) {
    crate::world_objects::world_object_networking::enqueue_broadcast_range(
        w,
        this,
        &msg,
        range,
        squelch_type,
    );
}

/// `WorldObject.Height` (`WorldObject_Properties.cs`): `PhysicsObj?.GetHeight() ?? 0`.
/// A body the landblock destroyed still answers in ACE (`destroyed_physics_obj`).
fn world_object_height(w: &World, this: ObjectGuid) -> f32 {
    match obj(w, this)
        .and_then(|o| o.phys)
        .and_then(|h| w.physics.get(h))
    {
        Some(b) => b.height(),
        None => destroyed_physics_obj(w, this).map_or(0.0, |d| d.height),
    }
}

/// The destroyed body ACE's reference still holds after the landblock removed the object.
fn destroyed_physics_obj(
    w: &World,
    this: ObjectGuid,
) -> Option<crate::world_objects::world_object::DestroyedPhysicsObj> {
    obj(w, this).and_then(|o| o.wo.world_object.destroyed_physics_obj)
}

/// `PhysicsObj.GetPhysicsRadius()`.
fn physics_obj_get_physics_radius(w: &World, this: ObjectGuid) -> f32 {
    let h = obj(w, this)
        .and_then(|o| o.phys)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    crate::physics::phys_ext::get_physics_radius(w, h)
}

/// `PhysicsObj.Position`; a body the landblock destroyed answers with its cell-0 position.
fn physics_obj_position(w: &World, this: ObjectGuid) -> PPosition {
    match obj(w, this).and_then(|o| o.phys) {
        Some(h) => crate::physics::phys_ext::position(w, h)
            .expect("ACE: PhysicsObj is null (NullReferenceException)"),
        None => {
            destroyed_physics_obj(w, this)
                .expect("ACE: PhysicsObj is null (NullReferenceException)")
                .position
        }
    }
}

/// `PhysicsObj.CachedVelocity`; a body the landblock destroyed answers with its last one.
fn physics_obj_cached_velocity(w: &World, this: ObjectGuid) -> Vector3 {
    let v = match obj(w, this).and_then(|o| o.phys) {
        Some(h) => {
            w.physics
                .get(h)
                .expect("ACE: PhysicsObj is null (NullReferenceException)")
                .cached_velocity
        }
        None => {
            destroyed_physics_obj(w, this)
                .expect("ACE: PhysicsObj is null (NullReferenceException)")
                .cached_velocity
        }
    };
    Vector3::new(v.x, v.y, v.z)
}

/// `sp.PhysicsObj.Velocity = v`: the new projectile's body, when it has one yet.
fn physics_obj_set_velocity(w: &mut World, this: ObjectGuid, v: Vector3) {
    // `sp.Setup` made the body just before
    let h = obj(w, this)
        .and_then(|o| o.phys)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    crate::physics::phys_ext::set_velocity_field(w, h, v);
}

/// `sp.PhysicsObj.Position.Frame.set_vector_heading(dir); sp.Location.Rotation =
/// sp.PhysicsObj.Position.Frame.Orientation`.
fn physics_obj_set_vector_heading(w: &mut World, this: ObjectGuid, dir: Vector3) {
    let h = obj(w, this)
        .and_then(|o| o.phys)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    let rotation = w.physics.get_mut(h).map(|b| {
        dereth_physics::math::set_vector_heading(
            &mut b.position.frame,
            dereth_primitives::Vec3::new(dir.x, dir.y, dir.z),
        );
        b.position.frame.rotation
    });
    if let (Some(r), Some(o)) = (rotation, w.objects.get_mut(this)) {
        if let Some(mut loc) = o.location() {
            loc.set_rotation(Quaternion::new(r.x, r.y, r.z, r.w));
            o.set_location(Some(loc));
        }
    }
}

/// `Physics.Common.Position.GetOffset(pos)`: the shared crate's offset.
fn physics_get_offset(from: &PPosition, to: &PPosition) -> Vector3 {
    let v = dereth_physics::math::get_offset(from, to);
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
