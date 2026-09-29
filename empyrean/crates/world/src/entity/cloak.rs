// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Cloak.cs
//! Port of `Source/ACE.Server/Entity/Cloak.cs`: the cloak procs (a spell, or a damage reduction
//! with `CloakWeaveProc` 2).

use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{ChatMessageType, EquipMask, ItemType, SpellFlags};
use empyrean_entity::ObjectGuid;

use crate::entity::spell::Spell;
use crate::managers::property_manager;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::world_object::{WorldObject, LOCAL_BROADCAST_RANGE};
use crate::World;

// ACE: Cloak.UseCustomMath
/// Opt out of the standarad ACE formula and proc at a custom rate. This setting will affect
/// cloaks with any proc, including -200.
fn use_custom_math(w: &World) -> bool {
    property_manager::get_bool(w, "use_cloak_proc_custom_scale", false, true).item
}

// ACE: Cloak.MinDelay
/// The maximum frequency of cloak procs, in seconds.
fn min_delay(w: &World) -> f64 {
    if !use_custom_math(w) {
        return 5.0;
    }
    property_manager::get_double(w, "cloak_cooldown_seconds", 0.0, true).item
}

// ACE: Cloak.MaxProcBase
/// The base maximum percentage at which cloaks will proc.
fn max_proc_base(w: &World) -> f32 {
    if !use_custom_math(w) {
        return 0.25;
    }
    property_manager::get_double(w, "cloak_max_proc_base", 0.0, true)
        .item
        .cs_cast()
}

// ACE: Cloak.MinProc
/// The minimum proc chance of a cloak.
fn min_proc(w: &World) -> f32 {
    if !use_custom_math(w) {
        return 0.0;
    }
    // Not ACE's (a fix, V345): reads `cloak_min_proc`, the name the registered
    // default (0) has, so an operator's setting takes effect. ACE read `cloak_min_proc_base`,
    // which no default names, so the setting changed nothing.
    property_manager::get_double(w, "cloak_min_proc", 0.0, true)
        .item
        .cs_cast()
}

// ACE: Cloak.MaxProcBase200
/// The base maxiumum percentage at which cloaks with -200 will proc.
fn max_proc_base_200(w: &World) -> f32 {
    if !use_custom_math(w) {
        return 0.15;
    }
    property_manager::get_double(w, "cloak_max_proc_base", 0.0, true)
        .item
        .cs_cast()
}

// ACE: Cloak.TwoThirds
const TWO_THIRDS: f32 = 2.0 / 3.0;

// ACE: Cloak.TryProcSpell
/// Rolls for a chance at procing a cloak spell; if successful, casts the spell.
pub fn try_proc_spell(
    w: &mut World,
    defender: ObjectGuid,
    attacker: Option<ObjectGuid>,
    cloak: Option<ObjectGuid>,
    damage_percent: f32,
) -> bool {
    let Some(cloak) = cloak else { return false };

    if !roll_proc(w, cloak, damage_percent) {
        return false;
    }

    handle_proc_spell(w, defender, attacker, cloak)
}

// ACE: Cloak.RollProc
/// Rolls for a chance at procing a cloak spell. `damage_percent` is the percent of MaxHealth
/// inflicted by an enemy's hit. A proc stamps the cloak's `UseTimestamp`.
pub fn roll_proc(w: &mut World, cloak: ObjectGuid, damage_percent: f32) -> bool {
    // TODO: find retail formula

    let mut damage_percent = damage_percent;

    let current_time = w.now.unix_time;

    let o = w
        .objects
        .get(cloak)
        .expect("ACE: cloak is null (NullReferenceException)");

    // `currentTime - cloak.UseTimestamp < MinDelay`: a null timestamp lifts to false
    if o.use_timestamp()
        .is_some_and(|t| current_time - t < min_delay(w))
    {
        return false;
    }

    let item_level = o.item_level().unwrap_or(0);

    if item_level < 1 {
        return false;
    }

    let mut max_proc_base = max_proc_base(w);

    if has_damage_proc(Some(o)) {
        max_proc_base = max_proc_base_200(w);
        damage_percent *= TWO_THIRDS;
    }

    #[allow(clippy::cast_precision_loss)]
    let max_proc_rate = max_proc_base + (item_level - 1) as f32 * 0.0125;

    if use_custom_math(w) {
        // The proc chance should only plateau for damage above a certain configured percentage
        let max_proc_at_damage_percent: f32 =
            property_manager::get_double(w, "cloak_max_proc_damage_percentage", 30.0, true)
                .item
                .cs_cast();
        // Reduce the damage percent for the calculation if necessary to a fraction of the percentage based on the configuration
        damage_percent = max_proc_rate * (damage_percent / max_proc_at_damage_percent);
    }

    // take the lowest of the chance between damage and proc rate, then override
    // with min proc if necessary
    let chance = math::max_f32(math::min_f32(damage_percent, max_proc_rate), min_proc(w));

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    if rng < f64::from(chance) {
        w.objects
            .get_mut(cloak)
            .expect("ACE: cloak")
            .set_use_timestamp(Some(current_time));
        true
    } else {
        false
    }
}

// ACE: Cloak.HandleProcSpell
/// Casts the cloak proc spell.
pub fn handle_proc_spell(
    w: &mut World,
    defender: ObjectGuid,
    attacker: Option<ObjectGuid>,
    cloak: ObjectGuid,
) -> bool {
    let Some(proc_spell) = w.objects.get(cloak).and_then(WorldObject::proc_spell) else {
        return false;
    };

    let spell = Spell::new(w, proc_spell, true);

    if spell.not_found() {
        if w.objects.get(defender).is_some_and(WorldObject::is_player) {
            let text = if spell.spell_base.is_none() {
                format!("SpellId {proc_spell} Invalid.")
            } else {
                format!("{} spell not implemented, yet!", spell.name())
            };
            let session =
                crate::world_objects::world_object_networking::shims::player_session(w, defender)
                    .expect("ACE: Player.Session is null (NullReferenceException)");
            enqueue_send(
                w,
                session,
                game_message_system_chat(&text, ChatMessageType::System),
            );
        }
        return false;
    }

    let target_self = (spell.flags() & SpellFlags::SelfTargeted) == SpellFlags::SelfTargeted;
    let untargeted = spell.non_component_target_type() == ItemType::None;

    let target = if untargeted {
        None
    } else if target_self {
        Some(defender)
    } else {
        attacker
    };

    // cloak range?

    let defender_name = crate::dispatch::name::name(w, defender).unwrap_or_default();
    let msg = game_message_system_chat(
        &format!(
            "The cloak of {defender_name} weaves the magic of {}!",
            spell.name()
        ),
        ChatMessageType::Spellcasting,
    );

    crate::world_objects::world_object_networking::enqueue_broadcast_range(
        w,
        defender,
        &msg,
        LOCAL_BROADCAST_RANGE,
        Some(ChatMessageType::Spellcasting),
    );

    crate::world_objects::world_object_magic::try_cast_spell(
        w,
        defender,
        &spell,
        target,
        Some(cloak),
        Some(cloak),
        true,
        true,
        false,
    );

    true
}

// ACE: Cloak.IsCloak
/// Returns TRUE if object is cloak.
#[must_use]
pub fn is_cloak(wo: &WorldObject) -> bool {
    wo.valid_locations() == Some(EquipMask::Cloak)
}

// ACE: Cloak.DamageReductionAmount
/// The amount of damage reduced by a cloak proced with `PropertyInt.CloakWeaveProc` = 2.
pub const DAMAGE_REDUCTION_AMOUNT: i32 = 200;

// ACE: Cloak.GetDamageReductionAmount
/// 200, halved against a player source: "Cloaks with the chance to reduce incoming damage by 200
/// have been reduced to 100 for PvP circumstances" (asheron.fandom.com/wiki/Master_of_Arms).
#[must_use]
pub fn get_damage_reduction_amount(source_is_player: bool) -> i32 {
    let mut damage_reduction_amount = DAMAGE_REDUCTION_AMOUNT;

    if source_is_player {
        damage_reduction_amount /= 2;
    }

    damage_reduction_amount
}

fn source_is_player(w: &World, source: Option<ObjectGuid>) -> bool {
    source
        .and_then(|s| w.objects.get(s))
        .is_some_and(WorldObject::is_player)
}

// ACE: Cloak.GetReducedAmount
/// `GetReducedAmount(WorldObject source, uint damage)`: the reduced damage amount when a cloak
/// procs with `CloakWeaveProc` 2.
#[must_use]
pub fn get_reduced_amount_uint(w: &World, source: Option<ObjectGuid>, damage: u32) -> u32 {
    let damage_reduction_amount = get_damage_reduction_amount(source_is_player(w, source));
    reduced_amount_uint(damage_reduction_amount, damage)
}

/// The arithmetic of the `uint` overload: `damage > amount` compares as `long`.
#[must_use]
pub fn reduced_amount_uint(damage_reduction_amount: i32, damage: u32) -> u32 {
    if i64::from(damage) > i64::from(damage_reduction_amount) {
        (i64::from(damage) - i64::from(damage_reduction_amount)).cs_cast()
    } else {
        0
    }
}

// ACE: Cloak.GetReducedAmount
/// `GetReducedAmount(WorldObject source, int damage)`.
#[must_use]
pub fn get_reduced_amount_int(w: &World, source: Option<ObjectGuid>, damage: i32) -> i32 {
    let damage_reduction_amount = get_damage_reduction_amount(source_is_player(w, source));

    0.max(damage.wrapping_sub(damage_reduction_amount))
}

// ACE: Cloak.GetReducedAmount
/// `GetReducedAmount(WorldObject source, float damage)`.
#[must_use]
pub fn get_reduced_amount_float(w: &World, source: Option<ObjectGuid>, damage: f32) -> f32 {
    let damage_reduction_amount = get_damage_reduction_amount(source_is_player(w, source));

    #[allow(clippy::cast_precision_loss)]
    let reduced = damage - damage_reduction_amount as f32;
    math::max_f32(0.0, reduced)
}

// ACE: Cloak.ShowMessage
/// Sends the message to attacker and defender when cloak is proced with `CloakWeaveProc` 2.
pub fn show_message(
    w: &mut World,
    defender: ObjectGuid,
    attacker: Option<ObjectGuid>,
    orig_damage: i32,
    reduced_damage: i32,
) {
    let suffix = format!("reduced the damage from {orig_damage} down to {reduced_damage}!");

    if w.objects.get(defender).is_some_and(WorldObject::is_player) {
        let session =
            crate::world_objects::world_object_networking::shims::player_session(w, defender)
                .expect("ACE: Player.Session is null (NullReferenceException)");
        enqueue_send(
            w,
            session,
            game_message_system_chat(&format!("Your cloak {suffix}"), ChatMessageType::Magic),
        );
    }

    // send message to attacker?
    if let Some(attacker) =
        attacker.filter(|&a| w.objects.get(a).is_some_and(WorldObject::is_player))
    {
        let defender_name = crate::dispatch::name::name(w, defender).unwrap_or_default();
        let session =
            crate::world_objects::world_object_networking::shims::player_session(w, attacker)
                .expect("ACE: Player.Session is null (NullReferenceException)");
        enqueue_send(
            w,
            session,
            game_message_system_chat(
                &format!("The cloak of {defender_name} {suffix}"),
                ChatMessageType::Magic,
            ),
        );
    }
}

// ACE: Cloak.ShowMessage
/// `ShowMessage(Creature, WorldObject, float, float)`: both amounts `(int)Math.Round(..)`. C#
/// picks this overload for `uint` amounts too (`uint` converts to `float`, not to `int`).
pub fn show_message_float(
    w: &mut World,
    defender: ObjectGuid,
    attacker: Option<ObjectGuid>,
    orig_damage: f32,
    reduced_damage: f32,
) {
    show_message(
        w,
        defender,
        attacker,
        math::round(f64::from(orig_damage)).cs_cast(),
        math::round(f64::from(reduced_damage)).cs_cast(),
    );
}

// ACE: Cloak.HasDamageProc
/// Returns TRUE If cloak has a damage reduction proc. Matches client logic.
#[must_use]
pub fn has_damage_proc(cloak: Option<&WorldObject>) -> bool {
    cloak.and_then(WorldObject::cloak_weave_proc) == Some(2)
}

// ACE: Cloak.HasProcSpell
/// Returns TRUE if cloak has a spell proc.
#[must_use]
pub fn has_proc_spell(cloak: Option<&WorldObject>) -> bool {
    cloak.and_then(WorldObject::proc_spell).is_some()
}
