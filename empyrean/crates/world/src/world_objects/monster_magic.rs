// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Magic.cs
//! Port of `Source/ACE.Server/WorldObjects/Monster_Magic.cs`: monster casting (the spell roll,
//! its range, the casting motions and the cast through the spell engine).

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    ChatMessageType, ItemType, MagicSchool, MotionCommand, SpellFlags, SpellType,
};
use empyrean_entity::ObjectGuid;

use crate::dispatch;
use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::probability;
use crate::entity::spell::Spell;
use crate::entity::timers;
use crate::network::game_messages::messages::game_message_hear_speech::game_message_hear_speech;
use crate::network::motion::movement_data::Motion;
use crate::physics::motion_table;
use crate::world_objects::world_object::LOCAL_BROADCAST_RANGE;
use crate::world_objects::world_object_magic;
use crate::world_objects::world_object_networking::{
    enqueue_broadcast_motion, enqueue_broadcast_range,
};
use crate::world_objects::{creature_combat, monster_combat};
use crate::world_objects::{
    creature_equipment, monster_awareness, monster_melee, monster_missile, monster_navigation,
    monster_tick,
};
use crate::World;

/// `Player.MaxRadarRange_Outdoors` (`Player.cs`).
const MAX_RADAR_RANGE_OUTDOORS: f32 = 75.0;

/// Non-property fields declared in `Monster_Magic.cs`.
#[derive(Debug, Default)]
pub struct MonsterMagicFields {
    /// The next spell the monster will attempt to cast.
    // ACE: Creature.CurrentSpell
    pub current_spell: Option<Spell>,
}

/// `Creature`'s `Monster_Magic.cs` fields.
///
/// # Panics
/// When `this` is gone or not a creature.
#[must_use]
pub fn fields(w: &World, this: ObjectGuid) -> &MonsterMagicFields {
    &w.objects
        .get(this)
        .and_then(|o| o.creature.as_ref())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_magic
}

/// Mutable [`fields`].
///
/// # Panics
/// As [`fields`].
pub fn fields_mut(w: &mut World, this: ObjectGuid) -> &mut MonsterMagicFields {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.creature.as_mut())
        .expect("ACE: Creature is null (NullReferenceException)")
        .monster_magic
}

/// `CurrentSpell` (ACE dereferences it without a check).
///
/// # Panics
/// With no current spell (ACE: `NullReferenceException`).
#[must_use]
pub fn current_spell(w: &World, this: ObjectGuid) -> &Spell {
    fields(w, this)
        .current_spell
        .as_ref()
        .expect("ACE: CurrentSpell is null (NullReferenceException)")
}

/// Returns TRUE if monster has known spells.
// ACE: Creature.HasKnownSpells
#[must_use]
pub fn has_known_spells(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .expect("ACE: this")
        .biota
        .has_known_spell()
}

/// Rolls each spell of the spellbook in order against its probability (a value above 2.0 is
/// `value - 2.0`, else a percentage); the first hit becomes `CurrentSpell`.
// ACE: Creature.TryRollSpell
pub fn try_roll_spell(w: &mut World, this: ObjectGuid) -> bool {
    fields_mut(w, this).current_spell = None;

    //Console.WriteLine($"{Name}.TryRollSpell(), probability={GetProbabilityAny()}");

    // monster spellbooks have probabilities with base 2.0
    // ie. a 5% chance would be 2.05 instead of 0.05

    // much less common, some monsters will have spells with just base 2.0 probability
    // there were probably other criteria used to select these spells (emote responses, monster ai responses)
    // for now, 2.0 base just becomes a 2% chance

    let Some(spell_book) = w
        .objects
        .get(this)
        .expect("ACE: this")
        .biota
        .properties_spell_book
        .clone()
    else {
        return false;
    };

    // We don't use thread safety here. Monster spell books aren't mutated cross-threads.
    // This reduces memory consumption by not cloning the spell book every single TryRollSpell()
    //foreach (var spell in Biota.CloneSpells(BiotaDatabaseLock)) // Thread-safe
    for (&key, &value) in spell_book.iter() {
        let probability = if value > 2.0f32 {
            value - 2.0f32
        } else {
            value / 100.0f32
        };

        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        if rng < f64::from(probability) {
            let spell = Spell::from_int(w, key, true);
            fields_mut(w, this).current_spell = Some(spell);
            return true;
        }
    }
    false
}

/// Returns the probability of this monster casting a spell for an attack.
// ACE: Creature.GetProbabilityAny
#[must_use]
pub fn get_probability_any(w: &World, this: ObjectGuid) -> f32 {
    let mut probabilities = Vec::new();

    for spell in w
        .objects
        .get(this)
        .expect("ACE: this")
        .biota
        .get_known_spells_probabilities()
    {
        let probability = if spell > 2.0f32 {
            spell - 2.0f32
        } else {
            spell / 100.0f32
        };

        probabilities.push(probability);
    }

    probability::get_probability_any(&probabilities)
}

/// Returns the maximum range for the current spell (infinite for a zero range: a self spell).
// ACE: Creature.GetSpellMaxRange
pub fn get_spell_max_range(w: &mut World, this: ObjectGuid) -> f32 {
    let skill = get_magic_skill_for_range_check(w, this);

    let spell = current_spell(w, this);
    #[allow(clippy::cast_precision_loss)] // `uint * float` in C#
    let mut max_range = spell.base_range_constant() + skill as f32 * spell.base_range_mod();

    // Not ACE's (retail, V336): with the `monster_ranged_closing` option on,
    // a monster's spell range is not capped; retail's casters closed to ranges of 95 to 125 m.
    if !monster_navigation::ranged_closing_enabled(w) {
        max_range = max_range.min(MAX_RADAR_RANGE_OUTDOORS);
    }

    if max_range == 0.0 {
        max_range = f32::INFINITY;
    }

    max_range
}

/// TRUE when the current attack is magic with a self spell.
// ACE: Creature.IsSelfCast
pub fn is_self_cast(w: &mut World, this: ObjectGuid) -> bool {
    if monster_combat::fields(w, this).current_attack != Some(creature_combat::CombatType::Magic) {
        return false;
    }

    get_spell_max_range(w, this) == f32::INFINITY
}

/// Performs the monster windup spell animation, casts the spell, and returns to attack stance.
// ACE: Creature.MagicAttack
pub fn magic_attack(w: &mut World, this: ObjectGuid) {
    let target = monster_combat::attack_target_creature(w, this);

    let Some(target) =
        target.filter(|&t| !monster_combat::is_dead(w.objects.get(t).expect("resolved")))
    else {
        dispatch::find_next_target::find_next_target(w, this);
        return;
    };

    let spell = current_spell(w, this).clone();

    // turn to?
    if w.objects.get(this).expect("ACE: this").ai_uses_mana() && !use_mana(w, this) {
        return;
    }

    // spell words
    if w.objects
        .get(this)
        .expect("ACE: this")
        .ai_use_human_magic_animations()
    {
        let spell_words = spell_words(w, &spell);
        if !spell_words.trim().is_empty() {
            let name = monster_awareness::name(w, this);
            let msg = game_message_hear_speech(
                &spell_words,
                &name,
                this.full(),
                ChatMessageType::Spellcasting,
            );
            enqueue_broadcast_range(w, this, &msg, LOCAL_BROADCAST_RANGE, None);
        }
    }

    let attack_target = monster_combat::attack_target(w, this).expect("the creature target");
    let pre_cast_time = pre_cast_motion(w, this, attack_target, false);

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(pre_cast_time));
    let cast = spell.clone();
    action_chain.add_action(Actor::Object(this), move |w| {
        let me_dead = w.objects.get(this).is_none_or(monster_combat::is_dead);
        let target_dead = w.objects.get(target).is_none_or(monster_combat::is_dead);
        if me_dead || monster_combat::attack_target(w, this).is_none() || target_dead {
            return;
        }

        cast_spell(w, this, &cast);

        post_cast_motion(w, this);
    });
    action_chain.enqueue_chain(w);

    let post_cast_time = get_post_cast_time(w, this, &spell, false);
    let _anim_time = pre_cast_time + post_cast_time;

    //Console.WriteLine($"{Name}.MagicAttack(): preCastTime({preCastTime}), postCastTime({postCastTime})");

    // slight variation here
    let prev_attack_time = timers::running_time(w) + f64::from(pre_cast_time);
    monster_combat::fields_mut(w, this).prev_attack_time = prev_attack_time;
    #[allow(clippy::cast_possible_truncation)] // ACE's `(float)` cast
    let powerup_time = w
        .objects
        .get(this)
        .expect("ACE: this")
        .powerup_time()
        .unwrap_or(f64::from(1.0f32)) as f32;

    let post_delay = ThreadSafeRandom::next_float(0.0, powerup_time);

    let next = prev_attack_time + f64::from(post_cast_time) + post_delay;
    monster_combat::fields_mut(w, this).next_attack_time = next;
    monster_navigation::fields_mut(w, this).next_move_time = next;
}

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

/// Pays the current spell's mana cost from the monster's mana; false when it has too little.
// ACE: Creature.UseMana
pub fn use_mana(w: &mut World, this: ObjectGuid) -> bool {
    // do any monsters have mana conversion?
    let target = if get_spell_max_range(w, this) < f32::INFINITY {
        monster_combat::attack_target(w, this)
    } else {
        Some(this)
    };

    let spell = current_spell(w, this).clone();
    let mana_used = calculate_mana_usage(w, this, &spell, target);

    let o = w.objects.get(this).expect("ACE: this");
    let mana = o.mana();
    let current = mana.current(o);
    if mana_used > current {
        return false;
    }

    let o = w.objects.get_mut(this).expect("ACE: this");
    mana.set_current(o, current - mana_used);
    true
}

// ACE: Creature.PreCastSpeed
const PRE_CAST_SPEED: f32 = 2.0;
// ACE: Creature.PostCastSpeed
const POST_CAST_SPEED: f32 = 1.0;
// ACE: Creature.PostCastSpeed_Ranged
const POST_CAST_SPEED_RANGED: f32 = 1.66; // ??

/// Perform the first part of monster spell casting animation - spreading arms out; answers its
/// length (`PreCastMotion(WorldObject target, bool fallback = false)`).
// ACE: Creature.PreCastMotion
pub fn pre_cast_motion(w: &mut World, this: ObjectGuid, target: ObjectGuid, fallback: bool) -> f32 {
    if w.objects
        .get(this)
        .expect("ACE: this")
        .ai_use_human_magic_animations()
        && !fallback
    {
        return pre_cast_motion_human(w, this, target);
    }

    let mut motion = Motion::from_world_object(w, this, MotionCommand::CastSpell, PRE_CAST_SPEED);
    motion.motion_state.turn_speed = 2.25;
    //motion.HasTarget = true;
    //motion.TargetGuid = target.Guid;
    set_current_motion_state(w, this, motion.clone());

    enqueue_broadcast_motion(w, this, &motion, None, None);

    let motion_table_id = w.objects.get(this).expect("ACE: this").motion_table_id();
    let stance = monster_tick::current_stance(w, this);
    motion_table::get_animation_length(
        w,
        motion_table_id,
        stance,
        MotionCommand::CastSpell,
        PRE_CAST_SPEED,
    )
}

/// For monsters with AiUseHumanMagicAnimations = true, performs the windup gestures from the
/// spell scarabs; answers the time for the windup gestures to complete.
// ACE: Creature.PreCastMotion_Human
pub fn pre_cast_motion_human(w: &mut World, this: ObjectGuid, target: ObjectGuid) -> f32 {
    // todo: play each motion at the proper time,
    // ensuring the monster is still alive at each step
    let mut spell = current_spell(w, this).clone();
    spell
        .formula
        .as_mut()
        .expect("ACE: Spell.Formula is null (NullReferenceException)")
        .get_monster_formula(w);
    fields_mut(w, this).current_spell = Some(spell.clone());
    let formula = spell.formula.as_ref().expect("set above");

    // FIXME: data
    let motion_table_id = w.objects.get(this).expect("ACE: this").motion_table_id();
    let cast_gesture = formula.cast_gesture(w);
    let stance = monster_tick::current_stance(w, this);
    let cast_anim_time = motion_table::get_animation_length(
        w,
        motion_table_id,
        stance,
        cast_gesture,
        PRE_CAST_SPEED,
    );

    if cast_anim_time == 0.0 {
        return pre_cast_motion(w, this, target, true);
    }

    let mut anim_time = 0.0f32;

    for windup_gesture in formula.windup_gestures(w) {
        let mut motion = Motion::from_world_object(w, this, windup_gesture, PRE_CAST_SPEED);
        motion.motion_state.turn_speed = 2.25;
        set_current_motion_state(w, this, motion.clone());

        enqueue_broadcast_motion(w, this, &motion, None, None);

        let stance = monster_tick::current_stance(w, this);
        anim_time += motion_table::get_animation_length(
            w,
            motion_table_id,
            stance,
            windup_gesture,
            PRE_CAST_SPEED,
        );
    }

    let mut cast_motion = Motion::from_world_object(w, this, cast_gesture, PRE_CAST_SPEED);
    cast_motion.motion_state.turn_speed = 2.25;
    set_current_motion_state(w, this, cast_motion.clone());

    enqueue_broadcast_motion(w, this, &cast_motion, None, None);

    anim_time += cast_anim_time;

    anim_time
}

/// Casts the current monster spell on target.
// ACE: Creature.CastSpell
pub fn cast_spell(w: &mut World, this: ObjectGuid, spell: &Spell) {
    let Some(attack_target) = monster_combat::attack_target(w, this) else {
        return;
    };

    let target_self = spell.flags().contains(SpellFlags::SelfTargeted);
    let untargeted = spell.non_component_target_type() == ItemType::None;

    let target = if untargeted {
        None
    } else if target_self {
        Some(this)
    } else {
        Some(attack_target)
    };

    let caster = creature_equipment::get_equipped_wand(w, this);

    // handle self procs
    if spell.is_harmful() && target != Some(this) {
        monster_melee::try_proc_equipped_items(w, this, this, true, caster);
    }

    // If the target is too far away, don't cast. This checks to see of this monster and the target are on separate landblock groups, and potentially separate threads.
    // This also fixes cross-threading issues
    if let Some(target) = target {
        let group = |g: ObjectGuid| {
            let lb = w.objects.get(g).and_then(|o| o.current_landblock)?;
            Some(
                w.landblock_manager
                    .landblocks
                    .expect(lb)
                    .current_landblock_group,
            )
        };
        let (mine, theirs) = (group(this), group(target));
        if mine.is_none() || theirs.is_none() || mine != theirs {
            return;
        }
    }

    // try to resist spell, if applicable
    if world_object_magic::try_resist_spell(w, this, target, spell, None, false) {
        try_handle_faction_mob(w, this, target);
        return;
    }

    let target_creature = target.filter(|&t| monster_awareness::is_creature(w, t));

    // TODO: see if this can be coalesced
    match spell.school() {
        MagicSchool::CreatureEnchantment => {
            world_object_magic::handle_cast_spell(
                w, this, spell, target, None, None, false, false, false,
            );

            if spell.is_harmful() {
                // handle target procs
                if let Some(tc) = target_creature.filter(|&t| t != this) {
                    monster_melee::try_proc_equipped_items(w, this, tc, false, caster);
                }
            }
        }

        MagicSchool::ItemEnchantment => {
            world_object_magic::try_cast_item_enchantment_with_redirects(
                w, this, spell, target, None,
            );
        }

        MagicSchool::LifeMagic => {
            world_object_magic::handle_cast_spell(
                w, this, spell, target, None, caster, false, false, false,
            );

            if spell.meta_spell_type() != SpellType::LifeProjectile {
                try_handle_faction_mob(w, this, target);

                if spell.is_harmful() {
                    // handle target procs
                    if let Some(tc) = target_creature.filter(|&t| t != this) {
                        monster_melee::try_proc_equipped_items(w, this, tc, false, caster);
                    }
                }
            }
        }

        MagicSchool::WarMagic | MagicSchool::VoidMagic => {
            world_object_magic::handle_cast_spell(
                w, this, spell, target, caster, None, false, false, false,
            );
        }

        _ => {}
    }
}

/// Perform the animations after casting a spell, ie. moving arms back in, returning to previous
/// stance.
// ACE: Creature.PostCastMotion
pub fn post_cast_motion(w: &mut World, this: ObjectGuid) {
    let anim_speed = if monster_missile::is_ranged(w, this) {
        POST_CAST_SPEED_RANGED
    } else {
        POST_CAST_SPEED
    };

    let mut motion = Motion::from_world_object(w, this, MotionCommand::Ready, anim_speed);
    motion.motion_state.turn_speed = 2.25;
    //motion.HasTarget = true;
    //motion.TargetGuid = target.Guid;
    set_current_motion_state(w, this, motion.clone());

    enqueue_broadcast_motion(w, this, &motion, None, None);
}

/// The length of the post-cast motion (`GetPostCastTime(Spell spell, bool fallback = false)`).
// ACE: Creature.GetPostCastTime
pub fn get_post_cast_time(w: &World, this: ObjectGuid, spell: &Spell, fallback: bool) -> f32 {
    if w.objects
        .get(this)
        .expect("ACE: this")
        .ai_use_human_magic_animations()
        && !fallback
    {
        return get_post_cast_time_human(w, this, spell);
    }

    let anim_speed = if monster_missile::is_ranged(w, this) {
        POST_CAST_SPEED_RANGED
    } else {
        POST_CAST_SPEED
    };

    let motion_table_id = w.objects.get(this).expect("ACE: this").motion_table_id();
    let stance = monster_tick::current_stance(w, this);
    motion_table::get_animation_length_between(
        w,
        motion_table_id,
        stance,
        MotionCommand::CastSpell,
        MotionCommand::Ready,
        anim_speed,
    )
}

// ACE: Creature.GetPostCastTime_Human
fn get_post_cast_time_human(w: &World, this: ObjectGuid, spell: &Spell) -> f32 {
    let anim_speed = if monster_missile::is_ranged(w, this) {
        POST_CAST_SPEED_RANGED
    } else {
        POST_CAST_SPEED
    };

    let motion_table_id = w.objects.get(this).expect("ACE: this").motion_table_id();
    let stance = monster_tick::current_stance(w, this);
    let cast_gesture = spell
        .formula
        .as_ref()
        .expect("ACE: Spell.Formula is null (NullReferenceException)")
        .cast_gesture(w);
    let anim_time = motion_table::get_animation_length_between(
        w,
        motion_table_id,
        stance,
        cast_gesture,
        MotionCommand::Ready,
        anim_speed,
    );

    // FIXME: data
    if anim_time == 0.0 {
        return get_post_cast_time(w, this, spell, true);
    }

    anim_time
}

/// Returns the magic skill level used for spell range checks (initial points + points due to
/// directly raising the skill).
// ACE: Creature.GetMagicSkillForRangeCheck
pub fn get_magic_skill_for_range_check(w: &mut World, this: ObjectGuid) -> u32 {
    let school = current_spell(w, this).school();
    let o = w.objects.get_mut(this).expect("ACE: this");
    let skill = o
        .get_creature_skill_school(school)
        .expect("ACE: GetCreatureSkill(MagicSchool) is null (NullReferenceException)");

    // verify this - should it be using base?
    // seems like it could be off, player formula uses current + cap?

    skill.init_level(o) + u32::from(skill.ranks(o))
}

/// A faction mob's (or a foe's) spell on another creature makes the target retaliate.
// ACE: Creature.TryHandleFactionMob
pub fn try_handle_faction_mob(w: &mut World, this: ObjectGuid, target: Option<ObjectGuid>) {
    if target == Some(this)
        || target.is_some_and(|t| {
            w.objects
                .get(t)
                .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
        })
    {
        return;
    }

    let Some(creature_target) = target.filter(|&t| monster_awareness::is_creature(w, t)) else {
        return;
    };
    if !creature_combat::allow_faction_combat(w, this, creature_target)
        && !creature_combat::potential_foe(w, this, creature_target)
    {
        return;
    }

    creature_combat::monster_on_attack_monster(w, this, creature_target);
}

/// Checks for AiUseHumanMagicAnimations and if true, sets CurrentSpell and sets combat mode to
/// Magic.
// ACE: Creature.CheckForHumanPreCast
pub fn check_for_human_pre_cast(w: &mut World, this: ObjectGuid, spell: &Spell) {
    if w.objects
        .get(this)
        .expect("ACE: this")
        .ai_use_human_magic_animations()
    {
        let s = Spell::new(w, spell.id(), true);
        fields_mut(w, this).current_spell = Some(s);
        creature_combat::set_combat_mode(w, this, empyrean_entity::enums::CombatMode::Magic);
    }
}

/// `CurrentMotionState = motion`.
fn set_current_motion_state(w: &mut World, this: ObjectGuid, motion: Motion) {
    w.objects
        .get_mut(this)
        .expect("ACE: this")
        .wo
        .world_object_properties
        .current_motion_state = Some(motion);
}

/// `CalculateManaUsage(this, CurrentSpell, target)` (`Creature_Magic.cs`):
/// the monster is both the instance and the caster.
fn calculate_mana_usage(
    w: &mut World,
    caster: ObjectGuid,
    spell: &Spell,
    target: Option<ObjectGuid>,
) -> u32 {
    crate::world_objects::creature_magic::calculate_mana_usage(w, caster, caster, spell, target)
}
