// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_OlthoiPlay.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_OlthoiPlay.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::float_extensions;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::ObjectGuid;
use empyrean_tables::entity::ChanceTable;
use empyrean_tables::enums::WeenieClassName;

use crate::factories::loot_generation_factory::{
    set_stack_size, world_object_factory_create_new_world_object,
};
use crate::world_objects::world_object::WorldObject;
use crate::World;

const SLAG_WCID: WeenieClassName = WeenieClassName::coinolthoi;

// https://asheron.fandom.com/wiki/Pitted_Slag

static NUM_SLAG_CHANCE: ChanceTable<i32> = ChanceTable::new(&[(1, 0.65), (2, 0.25), (3, 0.10)]);

static RARE_SLAG_CHANCE: ChanceTable<bool> = ChanceTable::new(&[(false, 0.95), (true, 0.05)]);

/// Rolls to generate slag for a monster killed by an OlthoiPlayer (tier 5+).
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the slag weenie is missing and a stack of more
/// than one was rolled.
// ACE: LootGenerationFactory.RollSlag
pub fn roll_slag(w: &mut World, profile: &TreasureDeath) -> Option<WorldObject> {
    let tier = profile.tier;

    // https://asheron.fandom.com/wiki/Pitted_Slag
    // If killed by an Olthoi, a small amount may be dropped by creatures level 100 or greater.

    // interpreting this to mean t5+

    if tier < 5 {
        return None;
    }

    // The chance of the killed creature having slag on their corpse is based exclusively on level
    // (ie. a level 220 creature will drop slag far more often than a level 100)

    // T5 = 1 in 5 chance
    // T6 = 1 in 4 chance
    // T7 = 1 in 3 chance
    // T8 = 1 in 2 chance
    let slag_chance = match tier {
        5 => 5,
        6 => 4,
        7 => 3,
        _ => 2,
    };

    let rng = ThreadSafeRandom::next(1, slag_chance);

    if rng < slag_chance {
        return None;
    }

    let mut slag = world_object_factory_create_new_world_object(w, SLAG_WCID.0.cast_unsigned());

    // roll for # of slag

    // It is commonly 1-2 slag, rarely 3-6.

    // It is possible that high level boss monsters such as the Tower Guardian
    // can yield a very high amount of slag compared to other creatures.

    // taking LootQualityMod into account for this part

    let mut num_slag = NUM_SLAG_CHANCE.roll(profile.loot_quality_mod);

    // 1 in 20 chance of dropping a rare amount
    let rare_chance = RARE_SLAG_CHANCE.roll(profile.loot_quality_mod);

    if rare_chance {
        num_slag += 3;
    }

    if num_slag > 1 {
        set_stack_size(
            slag.as_mut()
                .expect("NullReferenceException: no slag weenie"),
            Some(num_slag),
        );
    }

    slag
}

/// The amount of time that must elapse since player was last killed by an OlthoiPlayer
/// before they are dropping full amounts of slag again (one hour, in seconds).
const PVP_SLAG_TIMER_TOTAL_SECONDS: f64 = 3600.0;

/// `RollSlag(Player player, bool hadVitae)`: slag for a player killed by an OlthoiPlayer: a
/// level-scaled roll (with a 5% chance of each further roll), scaled down within an hour of the
/// player's last such death; stamps the player's `OlthoiLootTimestamp`.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the player is gone or the slag weenie is
/// missing.
pub fn roll_slag_for_player(
    w: &mut World,
    player: ObjectGuid,
    had_vitae: bool,
) -> Option<WorldObject> {
    // https://asheron.fandom.com/wiki/Pitted_Slag

    // If killed by an Olthoi, a large amount may be dropped by players.
    //
    // It is commonly 0-15, but there are reports of up to 100 from a single player. Players only drop slag under the following conditions:
    //
    // - The player must be over a certain level. The current estimate is level 100+. Players under level 180 will not yield much slag, if at all.
    // - Each time a player is killed they are less likely to drop pitted slag. This chance goes back up with time.
    // - Players don't drop slag if they have vitae.

    // can't simply use HasVitae here
    // a freshly-killed player already has vitae by this point
    //if (player.HasVitae) return null;
    if had_vitae {
        return None;
    }

    // determine preliminary scale of slag to drop based on Player level

    // divvy player levels up into "tiers"

    let (level, olthoi_loot_timestamp) = {
        let p = w
            .objects
            .get(player)
            .expect("NullReferenceException: player is null");
        (
            p.level().unwrap_or(0),
            p.olthoi_loot_timestamp().unwrap_or(0),
        )
    };

    let tier = get_tier_heuristic(level);

    if tier < 5 {
        return None; // if less than level 100, never drop any slag
    }

    let max_roll = match tier {
        5 => 5,
        6 => 10,
        _ => 15,
    };

    let mut total_slag = 0i32;
    loop {
        total_slag += ThreadSafeRandom::next(1, max_roll);
        if ThreadSafeRandom::next_float(0.0, 1.0) >= f64::from(0.05f32) {
            break; // 5% chance for another roll
        }
    }

    // scale totalSlag by last death to olthoi
    let current_time: i32 = w.now.unix_time.cs_cast();
    let time_diff = current_time.wrapping_sub(olthoi_loot_timestamp).max(0); // clamp to 0 on lower end -- avoid negatives, such as from server clock being rewound back in time
    if f64::from(time_diff) < PVP_SLAG_TIMER_TOTAL_SECONDS {
        #[allow(clippy::cast_possible_truncation)]
        let time_scale = (f64::from(time_diff) / PVP_SLAG_TIMER_TOTAL_SECONDS) as f32;
        #[allow(clippy::cast_precision_loss)]
        let scaled = total_slag as f32 * time_scale;
        total_slag = float_extensions::round(scaled, 0);
    }

    if total_slag <= 0 {
        return None;
    }

    let mut slag = world_object_factory_create_new_world_object(w, SLAG_WCID.0.cast_unsigned());

    set_stack_size(
        slag.as_mut()
            .expect("NullReferenceException: no slag weenie"),
        Some(total_slag),
    );

    if let Some(p) = w.objects.get_mut(player) {
        p.set_olthoi_loot_timestamp(Some(current_time));
    }

    slag
}

/// Returns an approximate tier for level
// ACE: LootGenerationFactory.GetTierHeuristic
fn get_tier_heuristic(level: i32) -> i32 {
    // based on http://acpedia.org/wiki/Loot

    match level {
        i32::MIN..=19 => 1, // 1-19
        20..=39 => 2,       // 20-39
        40..=59 => 3,       // 40-59
        60..=99 => 4,       // 60-99
        100..=134 => 5,     // 100-134
        135..=184 => 6,     // 135-184
        185..=274 => 7,     // 185-274
        _ => 8,             // 275
    }
}

const GLAND_WCID: WeenieClassName = WeenieClassName::olthoipvpcurrency;

/// Rolls to generate a gland for a player that killed an OlthoiPlayer: a 50% chance, none with
/// vitae.
// ACE: LootGenerationFactory.RollGland
pub fn roll_gland(w: &mut World, _player: ObjectGuid, had_vitae: bool) -> Option<WorldObject> {
    // http://acpedia.org/wiki/Mutated_Olthoi_Gland

    // - OlthoiPlayers don't drop Glands if they have vitae.

    // can't simply use HasVitae here
    // a freshly-killed player already has vitae by this point
    //if (player.HasVitae) return null;

    if had_vitae {
        return None;
    }

    let rng = ThreadSafeRandom::next(1, 100);

    if rng <= 50 {
        return None;
    }

    world_object_factory_create_new_world_object(w, GLAND_WCID.0.cast_unsigned())
}
