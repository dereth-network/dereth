//! Experience: what a kill is worth to its killer.
//!
//! **A solo kill** is worth the creature template's death experience, in full from ten levels
//! below the killer to nine above it; further below, 15% less for each level more (never under
//! 1%); further above, 20% less for each level more (never under 1%); rounded down.
//!
//! **From level 50 the fall comes sooner and faster:** in full only to four levels below the
//! killer, then 20% less for each level more (never under 1%). A level 49 killer took creatures 8
//! levels below it in full (3 of 3); from level 50 they paid a fifth. Of the 71 solo kills 5 to 10
//! levels below a killer of level 50 or more, 57 paid exactly this and none paid in full. Level
//! 54 shows the same shares in a fellowship, and level 85 twice them on the last day recorded. No
//! killer above 57 was recorded alone. What is above such a killer falls a level sooner too: in full
//! to eight above it, then 20% less for each level more (at level 50 and up, ten above paid 60%, 3
//! of 3; at levels 46 to 49, heroes among them, 80%, 12 of 12).
//!
//! The 2017 servers paid exactly this for 1,323 of 1,667 solo kills by the capturing players (70
//! more paid twice it, on the two days of a double-experience event); kills under vitae were not
//! reduced.
//!
//! **Paid in parts**: a kill's experience is paid in parts, one for each damager, each the
//! experience times the health that damager took over all the health its damagers took, rounded
//! to the nearest (2017: of 772 kills by a player and its pets alone, 701 paid a part for each
//! damager; of 667 parts where the nearest and the rounded-down amounts differ, 255 paid the
//! nearest, 20 the rounded-down).

/// The level from which the experience of what is below a killer falls sooner and faster, and of
/// what is above it a level sooner.
const STEEP_FROM_LEVEL: i32 = 50;

/// The share of a creature's death experience a killer gets, in hundredths, by the creature's
/// level and the killer's.
#[must_use]
pub fn kill_share(creature_level: i32, killer_level: i32) -> i64 {
    let difference = creature_level - killer_level;
    let m = if killer_level >= STEEP_FROM_LEVEL && difference < -4 {
        100 - 20 * i64::from(-difference - 4)
    } else if difference < -10 {
        100 - 15 * i64::from(-difference - 10)
    } else if killer_level >= STEEP_FROM_LEVEL && difference > 8 {
        100 - 20 * i64::from(difference - 8)
    } else if difference > 9 {
        100 - 20 * i64::from(difference - 9)
    } else {
        100
    };
    m.max(1)
}

/// The experience a solo kill pays: the death experience times the share, rounded down.
#[must_use]
pub fn kill_award(death_xp: i64, creature_level: i32, killer_level: i32) -> i64 {
    (death_xp.max(0) * kill_share(creature_level, killer_level)) / 100
}

/// A damager's part of an award: the award times the health it took over all the health taken,
/// rounded to the nearest (a half up).
#[must_use]
pub fn part(award: i64, taken: i64, all: i64) -> i64 {
    if all <= 0 {
        return 0;
    }
    let (award, taken, all) = (
        i128::from(award.max(0)),
        i128::from(taken.max(0)),
        i128::from(all),
    );
    i64::try_from((2 * award * taken + all) / (2 * all)).unwrap_or(i64::MAX)
}
