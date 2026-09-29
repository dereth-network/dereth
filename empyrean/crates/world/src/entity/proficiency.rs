// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Proficiency.cs
//! Port of `Source/ACE.Server/Entity/Proficiency.cs`.
//!
//! Skill experience earned by use. A successful use against a harder target than the
//! last one, or 15 minutes after the last, grants experience scaled by the difficulty and the time
//! since, and spends it on the skill.

use empyrean_common::dotnet::math;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{ShareType, Skill, SkillAdvancementClass, XpType};
use empyrean_entity::ObjectGuid;

use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{player_skills, player_xp};
use crate::World;

// ACE: Proficiency.FullTime
/// `TimeSpan.FromMinutes(15)`, in seconds (`FullTime.TotalSeconds`).
pub const FULL_TIME_TOTAL_SECONDS: f64 = 15.0 * 60.0;

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!("System.NullReferenceException: object {g:?} is not in World.objects")
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!("System.NullReferenceException: object {g:?} is not in World.objects")
    })
}

/// `player.GetCreatureSkill(skill)` (adding an untrained record the biota lacks), the argument
/// the callers build.
///
/// # Panics
/// When the player is not in the store (ACE: `NullReferenceException`).
pub fn get_creature_skill(w: &mut World, player: ObjectGuid, skill: Skill) -> CreatureSkill {
    obj_mut(w, player)
        .get_creature_skill(skill, true)
        .expect("GetCreatureSkill(add: true) always answers")
}

/// `player.IsOlthoiPlayer` (`Player_Properties.cs`, stored by `SetEphemeralValues`).
fn is_olthoi_player(w: &World, player: ObjectGuid) -> bool {
    obj(w, player)
        .player
        .as_ref()
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

fn player_name(w: &World, player: ObjectGuid) -> String {
    crate::dispatch::name::name(w, player).unwrap_or_default()
}

// ACE: Proficiency.OnSuccessUse
/// A successful use of `skill` against `difficulty`: when the difficulty beats the last one
/// recorded, or `FullTime` has passed since the last use, records both and grants proficiency
/// experience (the difficulty, scaled by the time since the last use when under `FullTime`): 110%
/// of it as unassigned experience, then the rest raises the skill.
pub fn on_success_use(w: &mut World, player: ObjectGuid, skill: CreatureSkill, difficulty: u32) {
    //Console.WriteLine($"Proficiency.OnSuccessUse({player.Name}, {skill.Skill}, targetDiff: {difficulty})");

    // TODO: this formula still probably needs some work to match up with retail truly...

    // possible todo: does this only apply to players?
    // ie., can monsters still level up from skill usage, or killing players?
    // it was possible on release, but i think they might have removed that feature?

    if is_olthoi_player(w, player) {
        return;
    }

    // ensure skill is at least trained
    if skill.advancement_class(obj(w, player)) < SkillAdvancementClass::Trained {
        return;
    }

    let (last_difficulty, last_used_time) = {
        let r = skill.properties_skill(obj(w, player));
        (r.resistance_at_last_check, r.last_used_time)
    };

    let current_time = w.now.unix_time;

    let time_diff = current_time - last_used_time;

    if time_diff < 0.0 {
        // can happen if server clock is rewound back in time
        log::warn!(
            "Proficiency.OnSuccessUse({}, {}, {difficulty}) - timeDiff: {time_diff}",
            player_name(w, player),
            skill.skill.to_dotnet_string()
        );
        skill
            .properties_skill_mut(obj_mut(w, player))
            .last_used_time = current_time; // update to prevent log spam
        return;
    }

    let difficulty_check = difficulty > last_difficulty;
    let time_check = time_diff >= FULL_TIME_TOTAL_SECONDS;

    if difficulty_check || time_check {
        // todo: not independent variables?
        // always scale if timeDiff < FullTime?
        let mut time_scale = 1.0f32;
        if !time_check {
            // 10 mins elapsed from 15 min FullTime:
            // 0.66f timeScale
            #[allow(clippy::cast_possible_truncation)] // (float)(double)
            {
                time_scale = (time_diff / FULL_TIME_TOTAL_SECONDS) as f32;
            }

            // any rng involved?
        }

        {
            let o = obj_mut(w, player);
            let r = skill.properties_skill_mut(o);
            r.resistance_at_last_check = difficulty;
            r.last_used_time = current_time;

            o.wo.world_object_database.changes_detected = true;
        }

        if player_xp::is_max_level(w, player) {
            return;
        }

        #[allow(clippy::cast_precision_loss)] // uint * float is a float multiply in C#
        let mut pp: u32 = math::round(f64::from(difficulty as f32 * time_scale)).cs_cast();
        #[allow(clippy::cast_precision_loss)]
        let mut total_xp_granted: i64 = math::round(f64::from(pp as f32 * 1.1f32)).cs_cast(); // give additional 10% of proficiency XP to unassigned XP

        if total_xp_granted > 10000 {
            log::warn!(
                "Proficiency.OnSuccessUse({}, {}, {difficulty}) - totalXPGranted: {}",
                player_name(w, player),
                skill.skill.to_dotnet_string(),
                empyrean_common::dotnet::format(total_xp_granted, "N0")
            );
        }

        let max_level = player_xp::get_max_level(w);
        let remaining_xp = player_xp::get_remaining_xp_to_level(w, player, max_level)
            .expect("InvalidOperationException: Nullable object must have a value.");

        if total_xp_granted > remaining_xp {
            // checks and balances:
            // total xp = pp * 1.1
            // pp = total xp / 1.1

            total_xp_granted = remaining_xp;
            #[allow(clippy::cast_precision_loss)] // long / float is a float divide in C#
            {
                pp = math::round(f64::from(total_xp_granted as f32 / 1.1f32)).cs_cast();
            }
        }

        // if skill is maxed out, but player is below MaxLevel,
        // not sure if retail granted 0%, 10%, or 110% of the pp to TotalExperience here
        // since pp is such a miniscule system at the higher levels,
        // going to just naturally add it to TotalXP for now..

        pp = pp.min(skill.experience_left(w, obj(w, player)));

        //Console.WriteLine($"Earned {pp} PP ({skill.Skill})");

        // send CP to player as unassigned XP
        player_xp::grant_xp(
            w,
            player,
            total_xp_granted,
            XpType::Proficiency,
            ShareType::None,
        );

        // send PP to player as skill XP, which gets spent from the CP sent
        if pp > 0 {
            player_skills::handle_action_raise_skill(w, player, skill.skill, pp);
        }
    }
}

// ACE: Proficiency.OnSuccessUse
/// The `int` overload: a negative difficulty is logged and ignored.
pub fn on_success_use_int(
    w: &mut World,
    player: ObjectGuid,
    skill: CreatureSkill,
    difficulty: i32,
) {
    if difficulty < 0 {
        log::error!(
            "Proficiency.OnSuccessUse({}, {}, {difficulty}) - difficulty cannot be negative",
            player_name(w, player),
            skill.skill.to_dotnet_string()
        );
        return;
    }
    on_success_use(w, player, skill, difficulty.cast_unsigned());
}
