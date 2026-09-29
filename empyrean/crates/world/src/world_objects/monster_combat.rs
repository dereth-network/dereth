// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Combat.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Combat.cs`: the attack target, the choice of
//! the next attack, the attack timing, and the monster's `TakeDamage`.
//!
//! The `Creature_Combat.cs` members the AI calls (`SetCombatMode`, the splatter, `CombatTable`,
//! ...) are ported in `creature_combat.rs`; [`shim`] keeps a member of a file still
//! unported (`Strings.GetAttackVerb`).

use dereth_assets::CombatManeuverTable;
use empyrean_common::dotnet::cast::CsCast;
use empyrean_common::dotnet::math;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    AttackHeight, ChatMessageType, CombatMode, DamageType, PlayScript, Sound,
};
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::body_part_table::BodyPartTable;
use crate::entity::damage_history;
use crate::entity::timers;
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::physics::phys_ext;
use crate::world_objects::creature_combat::{self, CombatType};
use crate::world_objects::creature_navigation::update_position_sync_location;
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::enqueue_broadcast;
use crate::world_objects::{
    creature_death, creature_equipment, monster_awareness, monster_magic, monster_melee,
    monster_missile, monster_navigation, player,
};
use crate::World;

/// Non-property fields declared in `Monster_Combat.cs`.
#[derive(Debug, Default)]
pub struct MonsterCombatFields {
    /// The current attack target for the monster (a guid: gone from the store reads as null).
    // ACE: Creature.AttackTarget
    pub attack_target: Option<ObjectGuid>,
    /// A monster chooses 1 attack height.
    // ACE: Creature.AttackHeight
    pub attack_height: Option<AttackHeight>,
    /// The next type of attack (melee/range/magic).
    // ACE: Creature.CurrentAttack
    pub current_attack: Option<CombatType>,
    /// The maximum distance for the next attack.
    // ACE: Creature.MaxRange
    pub max_range: f32,
    /// The time when monster started its last attack.
    // ACE: Creature.PrevAttackTime
    pub prev_attack_time: f64,
    /// The time when monster can perform its next attack.
    // ACE: Creature.NextAttackTime
    pub next_attack_time: f64,
    // ACE: Creature._attackHeights
    pub attack_heights: Option<Vec<AttackHeight>>,
}

/// `Creature`'s `Monster_Combat.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterCombatFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_combat
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterCombatFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_combat
}

/// `AttackTarget`: a target gone from the store reads as null.
#[must_use]
pub fn attack_target(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    fields(w, this)
        .attack_target
        .filter(|&g| w.objects.contains(g))
}

/// `AttackTarget as Creature`.
#[must_use]
pub fn attack_target_creature(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    attack_target(w, this).filter(|&g| monster_awareness::is_creature(w, g))
}

/// `AttackTarget = value`.
pub fn set_attack_target(w: &mut World, this: ObjectGuid, value: Option<ObjectGuid>) {
    fields_mut(w, this).attack_target = value;
}

/// `CombatTable` (`Creature_Combat.cs`).
fn combat_table(w: &World, this: ObjectGuid) -> Option<std::sync::Arc<CombatManeuverTable>> {
    creature_combat::fields(w.objects.get(this).expect("ACE: this"))
        .combat_table
        .clone()
}

/// The time when monster can perform its next magic attack: the previous attack's start plus
/// `AiUseMagicDelay` (3 s by default, the most common value in the py16 db).
// ACE: Creature.NextMagicAttackTime
#[must_use]
pub fn next_magic_attack_time(w: &World, this: ObjectGuid) -> f64 {
    // defaults to most common value found in py16 db
    let magic_delay = w
        .objects
        .get(this)
        .expect("ACE: this")
        .ai_use_magic_delay()
        .unwrap_or(f64::from(3.0f32));

    fields(w, this).prev_attack_time + magic_delay
}

/// Returns true if monster is dead.
// ACE: Creature.IsDead
#[must_use]
pub fn is_dead(o: &WorldObject) -> bool {
    o.health().current(o) == 0
}

/// A list of possible attack heights for this monster, as determined by the combat maneuvers
/// table (its distinct heights in file order), cached.
// ACE: Creature.AttackHeights
pub fn attack_heights(w: &mut World, this: ObjectGuid) -> Option<Vec<AttackHeight>> {
    let table = combat_table(w, this)?;

    if fields(w, this).attack_heights.is_none() {
        let mut heights: Vec<AttackHeight> = Vec::new();
        for m in &table.maneuvers {
            let height = AttackHeight(m.attack_height.cast_signed());
            if !heights.contains(&height) {
                heights.push(height);
            }
        }
        fields_mut(w, this).attack_heights = Some(heights);
    }

    fields(w, this).attack_heights.clone()
}

/// Selects a random attack height for the next attack.
///
/// # Panics
/// With no combat table (ACE: `NullReferenceException`).
// ACE: Creature.ChooseAttackHeight
pub fn choose_attack_height(w: &mut World, this: ObjectGuid) -> AttackHeight {
    let heights =
        attack_heights(w, this).expect("ACE: AttackHeights is null (NullReferenceException)");
    let count = i32::try_from(heights.len()).expect("a few heights");
    let rng = ThreadSafeRandom::next(0, count - 1);
    heights[usize::try_from(rng).expect("ACE: ArgumentOutOfRangeException")]
}

/// Magic when the monster knows spells and rolls one, else missile with a missile weapon, else
/// melee.
// ACE: Creature.GetNextAttackType
pub fn get_next_attack_type(w: &mut World, this: ObjectGuid) -> CombatType {
    if combat_table(w, this).is_none() {
        get_combat_table(w, this);
    }

    // if caster, roll for spellcasting chance
    if monster_magic::has_known_spells(w, this) && monster_magic::try_roll_spell(w, this) {
        return CombatType::Magic;
    }

    if monster_missile::is_ranged(w, this) {
        CombatType::Missile
    } else {
        CombatType::Melee
    }
}

/// Reads the combat maneuvers table from the DAT file.
// ACE: Creature.GetCombatTable
pub fn get_combat_table(w: &mut World, this: ObjectGuid) {
    if let Some(did) = w.objects.get(this).expect("ACE: this").combat_table_did() {
        let table = w
            .dats
            .portal_dat()
            .read_from_dat::<CombatManeuverTable>(did);
        creature_combat::fields_mut(w.objects.get_mut(this).expect("ACE: this")).combat_table =
            table;
    }
}

/// Switch to attack stance.
// ACE: Creature.DoAttackStance
pub fn do_attack_stance(w: &mut World, this: ObjectGuid) {
    let is_ranged = monster_missile::is_ranged(w, this);
    let combat_mode = if is_ranged {
        CombatMode::Missile
    } else {
        CombatMode::Melee
    };

    let stance_time = creature_combat::set_combat_mode(w, this, combat_mode);

    let now = timers::running_time(w);
    let next_time = now + f64::from(stance_time);

    let nav = monster_navigation::fields_mut(w, this);
    if nav.next_move_time > now {
        nav.next_move_time += f64::from(stance_time);
    } else {
        nav.next_move_time = next_time;
    }

    let f = fields_mut(w, this);
    if f.next_attack_time > now {
        f.next_attack_time += f64::from(stance_time);
    } else {
        f.next_attack_time = next_time;
    }

    let ai_use_magic_delay = w
        .objects
        .get(this)
        .expect("ACE: this")
        .ai_use_magic_delay()
        .unwrap_or(f64::from(3.0f32));
    if is_ranged {
        let f = fields_mut(w, this);
        f.prev_attack_time =
            f.next_attack_time + f64::from(monster_missile::MISSILE_DELAY) - ai_use_magic_delay;

        f.next_attack_time += f64::from(monster_missile::MISSILE_DELAY);
    }

    if w.objects.get(this).expect("ACE: this").never_attack() {
        let f = fields_mut(w, this);
        f.next_attack_time = f64::MAX - ai_use_magic_delay;
        f.prev_attack_time = f.next_attack_time;
    }

    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    let h = monster_navigation::physics_obj(w, this);
    monster_navigation::physics_obj_start_timer(w, h);
}

/// The range of the current attack: the spell's range (a projectile spell needs direct line of
/// sight, else the attack type is re-rolled, up to 10 times before falling back to melee), the
/// missile range, or melee range.
// ACE: Creature.GetMaxRange
pub fn get_max_range(w: &mut World, this: ObjectGuid) -> f32 {
    // FIXME
    let mut it = 0;
    let mut is_visible: Option<bool> = None;

    while fields(w, this).current_attack == Some(CombatType::Magic) {
        // select a magic spell
        //CurrentSpell = GetRandomSpell();
        if monster_magic::current_spell(w, this).is_projectile() {
            if is_visible.is_none() {
                let target = attack_target(w, this)
                    .expect("ACE: AttackTarget is null (NullReferenceException)");
                is_visible = Some(crate::world_objects::world_object::is_direct_visible(
                    w, this, target,
                ));
            }

            // ensure direct los
            if is_visible == Some(false) {
                // reroll attack type
                let next = get_next_attack_type(w, this);
                fields_mut(w, this).current_attack = Some(next);
                it += 1;

                // max iterations to melee?
                if it >= 10 {
                    //log.Warn($"{Name} ({Guid}) reached max iterations");
                    fields_mut(w, this).current_attack = Some(CombatType::Melee);

                    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
                    let powerup_time = w
                        .objects
                        .get(this)
                        .expect("ACE: this")
                        .powerup_time()
                        .unwrap_or(f64::from(1.0f32)) as f32;
                    let fail_delay = ThreadSafeRandom::next_float(0.0, powerup_time);

                    monster_navigation::fields_mut(w, this).next_move_time =
                        timers::running_time(w) + fail_delay;
                }
                continue;
            }
        }
        return monster_magic::get_spell_max_range(w, this);
    }

    if fields(w, this).current_attack == Some(CombatType::Missile) {
        /*var weapon = GetEquippedWeapon();
        if (weapon == null) return MaxMissileRange;

        var maxRange = weapon.GetProperty(PropertyInt.WeaponRange) ?? MaxMissileRange;
        return Math.Min(maxRange, MaxMissileRange);     // in-game cap @ 80 yds.*/
        crate::world_objects::creature_missile::get_max_missile_range(w, this)
    } else {
        monster_navigation::MAX_MELEE_RANGE // distance_to_target?
    }
}

/// TRUE once the next move time has passed and the body (stepped and synced first) is not
/// animating.
// ACE: Creature.MoveReady
pub fn move_ready(w: &mut World, this: ObjectGuid) -> bool {
    if timers::running_time(w) < monster_navigation::fields(w, this).next_move_time {
        return false;
    }

    let h = monster_navigation::physics_obj(w, this);
    phys_ext::update_object(w, h);
    update_position_sync_location(w, this);

    !phys_ext::is_animating(w, h)
}

/// Returns TRUE if creature can perform its next attack.
// ACE: Creature.AttackReady
pub fn attack_ready(w: &mut World, this: ObjectGuid) -> bool {
    let next_attack_time = if fields(w, this).current_attack == Some(CombatType::Magic) {
        next_magic_attack_time(w, this)
    } else {
        fields(w, this).next_attack_time
    };

    if timers::running_time(w) < next_attack_time || !monster_navigation::is_attack_range(w, this) {
        return false;
    }

    let h = monster_navigation::physics_obj(w, this);
    phys_ext::update_object(w, h);
    update_position_sync_location(w, this);

    !phys_ext::is_animating(w, h)
}

/// Performs the current attack on the target.
// ACE: Creature.Attack
pub fn attack(w: &mut World, this: ObjectGuid) {
    // `if (DebugMove) Console.WriteLine(...)`: the debug flag is never set

    match fields(w, this).current_attack {
        Some(CombatType::Melee) => {
            monster_melee::melee_attack(w, this);
        }
        Some(CombatType::Missile) => monster_missile::range_attack(w, this),
        Some(CombatType::Magic) => monster_magic::magic_attack(w, this),
        None => {}
    }

    let target = attack_target(w, this);
    monster_awareness::emote_manager_on_attack(w, this, target);

    reset_attack(w, this);
}

/// Called after attack has completed.
// ACE: Creature.ResetAttack
pub fn reset_attack(w: &mut World, this: ObjectGuid) {
    // wait for missile to strike
    //if (CurrentAttack == CombatType.Missile)
    //return;

    let nav = monster_navigation::fields_mut(w, this);
    nav.is_turning = false;
    nav.is_moving = false;

    let f = fields_mut(w, this);
    f.current_attack = None;
    f.max_range = 0.0;
}

/// The damage type of an attack: the equipped weapon's, else the attacking body part's (one of
/// its types at random when it has several).
// ACE: Creature.GetDamageType
pub fn get_damage_type_for_part(
    w: &World,
    this: ObjectGuid,
    attack_part: &PropertiesBodyPart,
    combat_type: Option<CombatType>,
) -> DamageType {
    let weapon = creature_equipment::get_equipped_weapon(w, this, false);

    if weapon.is_some() {
        dispatch::get_damage_type::get_damage_type(w, this, false, combat_type)
    } else {
        let mut damage_type = attack_part.d_type;

        if damage_type.is_multi_damage() {
            damage_type = empyrean_entity::enums::DamageType::select_damage_type(damage_type, None);
        }

        damage_type
    }
}

/// Simplified monster take damage over time function, only called for DoTs currently.
// ACE: Creature.TakeDamageOverTime
pub fn creature_take_damage_over_time(
    w: &mut World,
    this: ObjectGuid,
    amount: f32,
    damage_type: DamageType,
) {
    if is_dead(w.objects.get(this).expect("ACE: this")) {
        return;
    }

    dispatch::take_damage::take_damage(w, this, ObjectGuid::INVALID, damage_type, amount, false);

    // splatter effects
    let hit_sound = game_message_sound(this, Sound::HitFlesh1, 0.5);
    //var splatter = (PlayScript)Enum.Parse(typeof(PlayScript), "Splatter" + playerSource.GetSplatterHeight() + playerSource.GetSplatterDir(this));
    let splatter = game_message_script(
        this,
        if damage_type == DamageType::Nether {
            PlayScript::HealthDownVoid
        } else {
            PlayScript::DirtyFightingDamageOverTime
        },
        1.0,
    );
    enqueue_broadcast(w, this, true, &[hit_sound, splatter]);

    let o = w.objects.get(this).expect("ACE: this");
    if o.health().current(o) == 0 {
        return;
    }

    let health = o.health();
    let max = health.max_value(&mut StatCtx::in_world(w, this));
    #[allow(clippy::cast_precision_loss)] // `uint * float` in C#
    if amount >= max as f32 * 0.25f32 {
        let pain_sound = wound_sound();
        enqueue_broadcast(w, this, true, &[game_message_sound(this, pain_sound, 1.0)]);
    }
}

/// `(Sound)Enum.Parse(typeof(Sound), "Wound" + ThreadSafeRandom.Next(1, 3), true)`.
fn wound_sound() -> Sound {
    let n = ThreadSafeRandom::next(1, 3);
    Sound::from_name(&format!("Wound{n}")).expect("ACE: Sound.Wound1..3")
}

/// Notifies the damage over time (DoT) source player of the tick damage amount.
// ACE: Creature.TakeDamageOverTime_NotifySource
pub fn take_damage_over_time_notify_source(
    w: &mut World,
    this: ObjectGuid,
    source: ObjectGuid,
    damage_type: DamageType,
    amount: f32,
    aetheria: bool,
) {
    if !crate::managers::property_manager::get_bool(w, "show_dot_messages", false, true).item {
        return;
    }

    let i_amount: u32 = math::round(f64::from(amount)).cs_cast();

    let notify_type = if damage_type == DamageType::Undef {
        DamageType::Health
    } else {
        damage_type
    };

    let health = w.objects.get(this).expect("ACE: this").health();
    #[allow(clippy::cast_precision_loss)] // `float / uint` in C#
    let percent = amount / health.max_value(&mut StatCtx::in_world(w, this)) as f32;
    let (verb, _plural) = shim::strings_get_attack_verb(notify_type, percent);

    let name = monster_awareness::name(w, this);

    let (msg, r#type) = if damage_type == DamageType::Nether {
        (
            format!("You {verb} {name} for {i_amount} points of periodic nether damage!"),
            ChatMessageType::Magic,
        )
    } else if aetheria {
        (
            format!("With Surge of Affliction you {verb} {i_amount} points of health from {name}!"),
            ChatMessageType::Magic,
        )
    } else {
        /*var skill = source.GetCreatureSkill(Skill.DirtyFighting);
        var attack = skill.AdvancementClass == SkillAdvancementClass.Specialized ? "Bleeding Assault" : "Bleeding Blow";
        msg = $"With {attack} you {verb} {iAmount} points of health from {Name}!";*/

        (
            format!("You bleed {name} for {i_amount} points of periodic health damage!"),
            ChatMessageType::CombatSelf,
        )
    };
    player::send_message(w, source, &msg, r#type);
}

/// Applies some amount of damage to this monster from source (`ObjectGuid::INVALID` for none: a
/// combined DoT tick); answers the damage taken. At zero health the monster dies.
// ACE: Creature.TakeDamage
pub fn creature_take_damage(
    w: &mut World,
    this: ObjectGuid,
    source: ObjectGuid,
    damage_type: DamageType,
    amount: f32,
    crit: bool,
) -> u32 {
    let try_damage: i32 = math::round(f64::from(amount)).cs_cast();
    let health = w.objects.get(this).expect("ACE: this").health();
    let damage =
        -crate::world_objects::creature_vitals::update_vital_delta(w, this, health, -try_damage);

    // TODO: update monster stamina?

    // source should only be null for combined DoT ticks from multiple sources
    if source != ObjectGuid::INVALID {
        if damage >= 0 {
            damage_history::add(w, this, source, damage_type, damage.cast_unsigned());
        } else {
            damage_history::on_heal(w, this, (-damage).cast_unsigned());
        }
    }

    let o = w.objects.get(this).expect("ACE: this");
    if o.health().current(o) == 0 {
        let last_damager = damage_history::of(w, this).last_damager();
        dispatch::on_death::on_death(w, this, last_damager, damage_type, crit);

        creature_death::die(w, this);
    }
    0.max(damage).cast_unsigned()
}

/// Plays the hit sound, a pain sound for a hit of a quarter of the target's health or more, and
/// the blood splatter on `target`.
// ACE: Creature.EmitSplatter
pub fn emit_splatter(w: &mut World, this: ObjectGuid, target: ObjectGuid, damage: f32) {
    if is_dead(w.objects.get(target).expect("ACE: target")) {
        return;
    }

    enqueue_broadcast(
        w,
        target,
        true,
        &[game_message_sound(target, Sound::HitFlesh1, 0.5)],
    );
    let health = w.objects.get(target).expect("ACE: target").health();
    #[allow(clippy::cast_precision_loss)] // `uint * float` in C#
    if damage >= health.max_value(&mut StatCtx::in_world(w, target)) as f32 * 0.25f32 {
        let pain_sound = wound_sound();
        enqueue_broadcast(
            w,
            target,
            true,
            &[game_message_sound(target, pain_sound, 1.0)],
        );
    }
    let name = format!(
        "Splatter{}{}",
        creature_combat::get_splatter_height(w, this),
        creature_combat::get_splatter_dir(w, this, target)
    );
    let splatter = PlayScript::from_name(&name).expect("ACE: Enum.Parse(PlayScript)");
    enqueue_broadcast(
        w,
        target,
        true,
        &[game_message_script(target, splatter, 1.0)],
    );
}

/// `BodyPartTable` for a weenie, from the weenie cache.
// DIVERGE: ACE caches the table per wcid in a static `ConcurrentDictionary`; here it is built on each call from the same immutable cached weenie (the same table).
// ACE: Creature.GetBodyParts
#[must_use]
pub fn get_body_parts(w: &World, wcid: u32) -> Option<BodyPartTable> {
    let Some(weenie) = w.content.get_cached_weenie(wcid) else {
        // should never happen?
        log::error!("Monster_Combat.GetBodyParts({wcid}) - unknown wcid");
        return None;
    };

    Some(BodyPartTable::new(&weenie))
}

/// .NET `float.ToString()` for a log line.
#[must_use]
pub fn float_to_string(v: f32) -> String {
    format!("{v}")
}

/// Members of files not ported yet that the monster AI calls, by ACE's names: a complete body (no
/// anchor; an anchored port elsewhere takes precedence) or a pointer (`not_ported!`).
pub mod shim {
    use empyrean_entity::enums::DamageType;

    /// `Strings.GetAttackVerb(damageType, percent, ref verb, ref plural)` with both starting
    /// `null`: a `null` verb interpolates as the empty string.
    #[must_use]
    pub fn strings_get_attack_verb(damage_type: DamageType, percent: f32) -> (String, String) {
        let (single, plural) =
            crate::entity::strings::get_attack_verb(damage_type, percent).unwrap_or(("", ""));
        (single.to_owned(), plural.to_owned())
    }
}
