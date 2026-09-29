// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Magic.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Magic.cs`.
//!
//! `GetManaCost` (with its ACE vectors), `GetEffectiveMagicDefense`,
//! `CalculateManaUsage` and `CreateItemSpell`/`RemoveItemSpell`.

use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    ImbuedEffectType, MagicSchool, Skill, SkillAdvancementClass, SpellCategory, SpellFlags,
    SpellType,
};
use empyrean_entity::ObjectGuid;

use crate::world_objects::{creature_equipment, world_object_networking};
use crate::World;

/// Non-property fields declared in `Creature_Magic.cs`.
#[derive(Debug, Default)]
pub struct CreatureMagicFields {}

/// `Math.Round(double)` then `(uint)`.
fn round_to_u32(v: f64) -> u32 {
    math::round(v).cs_cast()
}

/// The mana cost after the caster's Mana Conversion skill checks (thanks to GDLE for this
/// function!). Draws two or three `ThreadSafeRandom.Next(0.0f, 1.0f)` values when `mana_conv`
/// is non-zero.
// ACE: Creature.GetManaCost
#[must_use]
pub fn get_mana_cost(difficulty: u32, mana_cost: u32, mana_conv: u32) -> u32 {
    use crate::world_objects::skill_check::{get_skill_chance_uint, DEFAULT_FACTOR};

    // thanks to GDLE for this function!
    if mana_conv == 0 {
        return mana_cost;
    }

    let mut mana_cost = mana_cost;

    // Dropping diff by half as Specced ManaC is only 48 with starter Aug so 50 at level 1 means no bonus
    //   easiest change without having to create two different formulas to try to emulate retail
    let mut success_chance = get_skill_chance_uint(mana_conv, difficulty / 2, DEFAULT_FACTOR);
    let mut roll = ThreadSafeRandom::next_float(0.0, 1.0);

    // Luck lowers the roll value to give better outcome
    // e.g. successChance = 0.83 & roll = 0.71 would still provide some savings.
    //   but a luck roll of 0.19 will lower that 0.71 to 0.13 so the caster would
    //   receive a 60% reduction in mana cost.  without the luck roll, 12%
    //   so players will always have a level of "luck" in manacost if they make skill checks
    let luck = ThreadSafeRandom::next_float(0.0, 1.0);

    if roll < success_chance {
        mana_cost = round_to_u32(
            f64::from(mana_cost) * (f64::from(1.0f32) - (success_chance - (roll * luck))),
        );
    }

    // above seems to give a good middle of the range
    // seen in pcaps for mana usage for low level chars
    // bug still need a way to give a better reduction for the "lucky"

    // save some calc time if already at 1 mana cost
    if mana_cost > 1 {
        success_chance = get_skill_chance_uint(mana_conv, difficulty, DEFAULT_FACTOR);
        roll = ThreadSafeRandom::next_float(0.0, 1.0);

        if roll < success_chance {
            mana_cost = round_to_u32(
                f64::from(mana_cost) * (f64::from(1.0f32) - (success_chance - (roll * luck))),
            );
        }
    }

    mana_cost.max(1)
}

/// Returns the creature's effective magic defense skill with `item.WeaponMagicDefense` and imbues
/// factored in.
// ACE: Creature.GetEffectiveMagicDefense
pub fn get_effective_magic_defense(w: &mut World, this: ObjectGuid) -> u32 {
    let current = crate::world_objects::world_object_magic::creature_get_creature_skill_current(
        w,
        this,
        Skill::MagicDefense,
    );
    let weapon_defense_mod = world_object_get_weapon_magic_defense_modifier(w, this);
    let defense_imbues: u32 =
        creature_get_defense_imbues(w, this, ImbuedEffectType::MagicDefense).cs_cast();

    // `(current * weaponDefenseMod) + defenseImbues`: uint * float and float + uint are float sums.
    #[allow(clippy::cast_precision_loss)]
    let effective_magic_defense = round_to_u32(f64::from(
        current as f32 * weapon_defense_mod + defense_imbues as f32,
    ));

    //Console.WriteLine($"EffectiveMagicDefense: {effectiveMagicDefense}");

    effective_magic_defense
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members of other ACE files, named after ACE's members.
// ---------------------------------------------------------------------------------------------

/// `WorldObject.GetWeaponMagicDefenseModifier(this)` (`WorldObject_Weapon.cs`).
fn world_object_get_weapon_magic_defense_modifier(w: &mut World, wielder: ObjectGuid) -> f32 {
    crate::world_objects::world_object_weapon::get_weapon_magic_defense_modifier(w, wielder)
}

/// `Creature.GetDefenseImbues(imbuedEffectType)` (`Creature_Combat.cs`):
/// `EquippedObjects.Values.Count(i => i.ImbuedEffect.HasFlag(imbuedEffectType))` over the
/// equipped objects. Exact.
fn creature_get_defense_imbues(
    w: &World,
    this: ObjectGuid,
    imbued_effect_type: ImbuedEffectType,
) -> i32 {
    let count = creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .filter(|i| {
            w.objects.get(*i).is_some_and(|o| {
                (o.imbued_effect().0 & imbued_effect_type.0) == imbued_effect_type.0
            })
        })
        .count();
    i32::try_from(count).expect("a count fits an int")
}

// ------------------------------------------------------------------ CalculateManaUsage

fn obj(w: &World, g: ObjectGuid) -> &crate::world_objects::world_object::WorldObject {
    w.objects
        .get(g)
        .unwrap_or_else(|| panic!("ACE: {g:?} is null (NullReferenceException)"))
}

/// A guid that still resolves (ACE: a non-null reference).
fn live(w: &World, g: Option<ObjectGuid>) -> Option<ObjectGuid> {
    g.filter(|&g| w.objects.get(g).is_some())
}

fn is_player(w: &World, g: ObjectGuid) -> bool {
    w.objects
        .get(g)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
}

/// `GetCreatureSkill(skill)` (added if absent).
fn get_creature_skill(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
) -> crate::world_objects::entity::creature_skill::CreatureSkill {
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .get_creature_skill(skill, true)
        .expect("the skill was added")
}

/// `(this as Player).GetFellowshipTargets()`'s source: `Fellowship.GetFellowshipMembers()`, when
/// the player has a fellowship.
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

/// The mana a cast costs `caster`: the spell's base mana (a built-in spell's `ItemManaCost`),
/// plus the per-item or per-fellow mana for item-armor and fellowship spells, reduced by Mana
/// Conversion (`GetManaCost`: two or three draws) unless it is untrained or the spell ignores it.
/// `this` is the creature whose fellowship counts (ACE's instance; the players' own calls pass
/// themselves as `caster`).
// ACE: Creature.CalculateManaUsage
pub fn calculate_mana_usage(
    w: &mut World,
    this: ObjectGuid,
    caster: ObjectGuid,
    spell: &crate::entity::spell::Spell,
    target: Option<ObjectGuid>,
) -> u32 {
    let mut base_cost = spell.base_mana();

    // for casting spells built into a casting implement, use the ItemManaCost
    if let Some(cast_item) = creature_equipment::get_equipped_wand(w, caster) {
        let o = obj(w, cast_item);
        if o.spell_did().unwrap_or(0) == spell.id() {
            base_cost = o.item_mana_cost().unwrap_or(0).cast_unsigned();
        }
    }

    let target = live(w, target);
    let target_player = target.filter(|&t| is_player(w, t));
    if spell.school() == MagicSchool::ItemEnchantment
        && spell.meta_spell_type() == SpellType::Enchantment
        && spell.category() >= SpellCategory::ArmorValueRaising
        && spell.category() <= SpellCategory::AcidicResistanceLowering
        && target_player.is_some()
    {
        let mut num_target_items: u32 = 1;
        if let Some(target_player) = target_player {
            let count = creature_equipment::equipped_objects_values(w, target_player)
                .into_iter()
                .filter(|&i| {
                    let o = obj(w, i);
                    (o.is_clothing() || o.is_shield())
                        && world_object_networking::shims::is_enchantable(w, i)
                })
                .count();
            num_target_items = u32::try_from(count).unwrap_or(u32::MAX);
        }

        base_cost = base_cost.wrapping_add(spell.mana_mod().wrapping_mul(num_target_items));
    } else if spell.is_fellowship_spell() {
        let mut num_fellows: u32 = 1;
        if is_player(w, this) {
            if let Some(members) = player_fellowship_get_fellowship_members(w, this) {
                num_fellows = u32::try_from(members.len()).unwrap_or(u32::MAX);
            }
        }

        base_cost = base_cost.wrapping_add(spell.mana_mod().wrapping_mul(num_fellows));
    }

    let mana_conversion = get_creature_skill(w, caster, Skill::ManaConversion);

    if mana_conversion.advancement_class(obj(w, caster)) < SkillAdvancementClass::Trained
        || (spell.flags() & SpellFlags::IgnoresManaConversion) == SpellFlags::IgnoresManaConversion
    {
        return base_cost;
    }

    let difficulty = spell.power_mod(); // modified power difficulty

    let current: f32 = mana_conversion.current(w, caster).cs_cast();
    let weapon_mod =
        crate::world_objects::world_object_weapon::get_weapon_mana_conversion_modifier(w, caster);
    let mana_conversion_skill: u32 = math::round(f64::from(current * weapon_mod)).cs_cast();

    crate::world_objects::creature_magic::get_mana_cost(
        difficulty,
        base_cost,
        mana_conversion_skill,
    )
}

/// Handles equipping an item casting a spell on player or creature. False when the spell is not
/// found (a player is told why).
// ACE: Creature.CreateItemSpell
pub fn create_item_spell(w: &mut World, this: ObjectGuid, item: ObjectGuid, spell_id: u32) -> bool {
    use crate::entity::spell::Spell;
    use crate::world_objects::world_object_magic::handle_cast_spell;
    use empyrean_entity::enums::{ChatMessageType, MagicSchool};

    let spell = Spell::new(w, spell_id, true);

    if spell.not_found() {
        if w.objects
            .get(this)
            .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
        {
            let player = this;
            if spell.spell_base.is_none() {
                crate::world_objects::player_networking::send_transient_error(
                    w,
                    player,
                    &format!("SpellID {spell_id} Invalid."),
                );
            } else {
                let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
                    &format!("{} spell not implemented, yet!", spell.name()),
                    ChatMessageType::System,
                );
                let session = crate::managers::player_manager::player_session(w, player)
                    .expect("ACE: player.Session is null (NullReferenceException)");
                crate::network::game_messages::game_message::enqueue_send(w, session, msg);
            }
        }
        return false;
    }

    // TODO: look into condensing this
    let school = spell.school();
    if school == MagicSchool::CreatureEnchantment || school == MagicSchool::LifeMagic {
        handle_cast_spell(
            w,
            this,
            &spell,
            Some(this),
            Some(item),
            None,
            false,
            false,
            true,
        );
    } else if school == MagicSchool::ItemEnchantment {
        if spell.has_item_category() || spell.is_portal_spell() {
            handle_cast_spell(
                w,
                this,
                &spell,
                Some(this),
                Some(item),
                Some(item),
                false,
                false,
                true,
            );
        } else {
            handle_cast_spell(
                w,
                this,
                &spell,
                Some(item),
                Some(item),
                Some(item),
                false,
                false,
                true,
            );
        }
    }

    true
}

/// Removes an item's spell from the appropriate enchantment registry (either the wielder, or the
/// item). With `silent`, the spell is dispelled without a message to the target player.
// ACE: Creature.RemoveItemSpell
pub fn remove_item_spell(
    w: &mut World,
    this: ObjectGuid,
    item: Option<ObjectGuid>,
    spell_id: u32,
    silent: bool,
) {
    use crate::entity::spell::Spell;
    use crate::world_objects::managers::{
        enchantment_manager as em, enchantment_manager_with_caching as emc,
    };
    use empyrean_entity::enums::MagicSchool;

    let Some(item) = item else { return };

    let spell = Spell::new(w, spell_id, true);

    if spell.spell_base.is_none() {
        if w.objects
            .get(this)
            .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
        {
            crate::world_objects::player_networking::send_transient_error(
                w,
                this,
                &format!("SpellId {spell_id} Invalid."),
            );
        }

        return;
    }

    let target = if spell.school() == MagicSchool::ItemEnchantment && !spell.has_item_category() {
        item
    } else {
        this
    };

    // Retrieve enchantment on target and remove it, if present
    let properties_enchantment_registry =
        em::get_enchantment(w, target, spell_id, Some(item.full())).cloned();

    if let Some(entry) = properties_enchantment_registry {
        if !silent {
            emc::remove(w, target, Some(&entry), true);
        } else {
            emc::dispel(w, target, Some(&entry));
        }
    }
}
