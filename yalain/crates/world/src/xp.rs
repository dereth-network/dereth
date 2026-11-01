//! Experience: what a kill is worth to its killer.
//!
//! **A solo kill** is worth the creature template's death experience, in full from ten levels
//! below the killer to nine above it; further below, 15% less for each level more (never under
//! 1%); further above, 20% less for each level more (never under 1%); rounded down. The 2017
//! servers paid exactly this for 1,242 of 1,667 solo kills by the capturing players (70 more paid
//! twice it, on the two days of a double-experience event); kills under vitae were not reduced.
//!
//! **Paid in parts**: a kill's experience is paid in parts, one for each damager, each the
//! experience times the health that damager took over all the health its damagers took, rounded
//! to the nearest (2017: of 772 kills by a player and its pets alone, 701 paid a part for each
//! damager; of 667 parts where the nearest and the rounded-down amounts differ, 255 paid the
//! nearest, 20 the rounded-down).

/// The share of a creature's death experience a killer gets, in hundredths, by the creature's
/// level minus the killer's.
#[must_use]
pub fn kill_share(level_difference: i32) -> i64 {
    let m = if level_difference < -10 {
        100 - 15 * i64::from(-level_difference - 10)
    } else if level_difference > 9 {
        100 - 20 * i64::from(level_difference - 9)
    } else {
        100
    };
    m.max(1)
}

/// The experience a solo kill pays: the death experience times the share, rounded down.
#[must_use]
pub fn kill_award(death_xp: i64, creature_level: i32, killer_level: i32) -> i64 {
    (death_xp.max(0) * kill_share(creature_level - killer_level)) / 100
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
