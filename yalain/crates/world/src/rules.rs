//! The rolls of a fight: whether an attack lands, how well, and for how much.
//!
//! **Whether it lands** follows logistic fits to the 119,902 attacks by or on the capturing
//! players in January 2017, by direction and kind (melee, missile, magic): the chance is
//! `1 / (1 + e^-(a + b × d / 10 + c × (L - 1) / 10))`, `d` the attacker's level minus the
//! defender's and `L` the player's own level. A player's attacks land more often as it rises
//! (about half of them at level 1, four in five at level 40, at equal levels), which the level
//! difference alone does not say; a creature's land on a player of any level alike (`c` is 0).
//! Below the lowest level 2017 shows for a kind, the player's term holds at that level. An attack
//! that does not land is evaded, or for a ranged attack missed in the measured share; nothing is
//! blocked (2017's results are hits, criticals, misses and evades). A landed attack is critical in
//! the measured share of landed attacks, a basic attack with what its weapon adds, a special by that
//! alone.
//!
//! **How much** a creature's hit does: its natural damage times a uniform roll across its natural
//! variance, times what the player's armor lets through (the measured share for the band of the
//! player's level: all of it at levels 1 and 2, before players wore armor); a critical adds half.
//! A player's hit: its skill's damage (authored until the skills' attack hooks are applied: a base
//! by level with the same kind of roll, less a point for each of the creature's armor levels, as
//! 2017's hits lost about one); a critical doubles it. The ratings, armor and buffs 2017 used are
//! not in the captures; these are the measured outcomes standing in for them.

use crate::rng::Rng;
use dereth_primitives::num::math;

/// The kind of an attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Melee,
    Missile,
    Magic,
}

/// The kind of an attack skill by its name's family (`Hum_Me_AutoAttack1` melee, `Mn_Mi_Strong`
/// missile, `Tum_Ma_Harm` magic): a bow's strikes up close are missile attacks, a stave's magic.
#[must_use]
pub fn kind_of_name(name: &str) -> Option<Kind> {
    let mut parts = name.split('_').skip(1);
    match parts.next() {
        Some("Me") => Some(Kind::Melee),
        Some("Mi") => Some(Kind::Missile),
        Some("Ma") => Some(Kind::Magic),
        _ => None,
    }
}

/// Is a skill one a creature uses on itself or its friends, by its name (`Mn_Ma_Buff_Heal`,
/// `Mn_Ma_Buff_ArmorInc`: the family's buffs and heals)?
#[must_use]
pub fn helps_by_name(name: &str) -> bool {
    name.split('_').any(|w| w == "Buff" || w == "Heal")
}

/// Who attacks whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    PlayerOnCreature,
    CreatureOnPlayer,
}

/// How an attack came out (the story's result values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Hit,
    Critical,
    Miss,
    Evade,
}

impl Outcome {
    /// The result value a story hook carries.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Hit => 0x4000_0008,
            Self::Critical => 0x4000_0009,
            Self::Miss => 0x4000_0007,
            Self::Evade => 0x4000_000B,
        }
    }

    #[must_use]
    pub const fn landed(self) -> bool {
        matches!(self, Self::Hit | Self::Critical)
    }
}

/// The fit for one direction and kind: `a`, `b`, `c`, the lowest player level its `c` term is
/// measured from, the critical share of landed attacks, the miss share of attacks that did not
/// land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub floor: i32,
    pub critical: f64,
    pub miss: f64,
}

const fn two(a: f64, b: f64, critical: f64, miss: f64) -> Fit {
    Fit {
        a,
        b,
        c: 0.0,
        floor: 1,
        critical,
        miss,
    }
}

/// The measured fits, by direction and kind (attacks: a player's melee 26,005 from level 11 up,
/// missile 26,742 and magic 20,891 from level 1; a creature's melee 20,342, missile 16,029, magic
/// 9,893). A player's critical is its basic attacks' with a weapon that adds nothing (melee 1,389
/// hits, missile 5,665, magic 2,080).
#[must_use]
pub const fn fit(dir: Direction, kind: Kind) -> Fit {
    match (dir, kind) {
        (Direction::PlayerOnCreature, Kind::Melee) => Fit {
            a: -0.396,
            b: 0.739,
            c: 0.504,
            floor: 11,
            critical: 0.051,
            miss: 0.0,
        },
        (Direction::PlayerOnCreature, Kind::Missile) => Fit {
            a: -0.107,
            b: 0.291,
            c: 0.298,
            floor: 1,
            critical: 0.055,
            miss: 0.248,
        },
        (Direction::PlayerOnCreature, Kind::Magic) => Fit {
            a: 0.131,
            b: 0.628,
            c: 0.230,
            floor: 1,
            critical: 0.043,
            miss: 0.028,
        },
        (Direction::CreatureOnPlayer, Kind::Melee) => two(0.180, 0.274, 0.023, 0.0),
        (Direction::CreatureOnPlayer, Kind::Missile) => two(0.052, 0.441, 0.029, 0.346),
        (Direction::CreatureOnPlayer, Kind::Magic) => two(0.191, 0.796, 0.012, 0.208),
    }
}

/// The chance an attack lands.
#[must_use]
pub fn landing_chance(dir: Direction, kind: Kind, attacker_level: i32, defender_level: i32) -> f64 {
    let f = fit(dir, kind);
    let d = f64::from(attacker_level - defender_level) / 10.0;
    let player = match dir {
        Direction::PlayerOnCreature => attacker_level,
        Direction::CreatureOnPlayer => defender_level,
    };
    let l = f64::from(player.max(f.floor) - 1) / 10.0;
    1.0 / (1.0 + math::exp(-(f.a + f.b * d + f.c * l)))
}

/// Roll an attack's outcome.
pub fn roll_outcome(
    rng: &mut Rng,
    dir: Direction,
    kind: Kind,
    attacker_level: i32,
    defender_level: i32,
) -> Outcome {
    let critical = fit(dir, kind).critical;
    roll_with_critical(rng, dir, kind, (attacker_level, defender_level), critical)
}

/// A player's basic attack: its outcome, its chance of a critical the measured one with what its
/// weapon adds ([`crate::combat::critical_bonus`]).
pub fn roll_basic(
    rng: &mut Rng,
    kind: Kind,
    attacker_level: i32,
    defender_level: i32,
    bonus: f64,
) -> Outcome {
    let dir = Direction::PlayerOnCreature;
    let critical = fit(dir, kind).critical + bonus;
    roll_with_critical(rng, dir, kind, (attacker_level, defender_level), critical)
}

/// A player's special: its outcome, critical only by what its weapon adds (2017: 6 of the 5,397
/// specials that landed with a weapon that adds nothing, against basic attacks' 0.043-0.055).
pub fn roll_special(
    rng: &mut Rng,
    kind: Kind,
    attacker_level: i32,
    defender_level: i32,
    bonus: f64,
) -> Outcome {
    let dir = Direction::PlayerOnCreature;
    roll_with_critical(rng, dir, kind, (attacker_level, defender_level), bonus)
}

/// An attack's outcome at a chance of a critical among those that land.
pub fn roll_with_critical(
    rng: &mut Rng,
    dir: Direction,
    kind: Kind,
    (attacker_level, defender_level): (i32, i32),
    critical: f64,
) -> Outcome {
    let f = fit(dir, kind);
    let p = landing_chance(dir, kind, attacker_level, defender_level);
    if f64::from(rng.unit()) < p {
        if f64::from(rng.unit()) < critical {
            Outcome::Critical
        } else {
            Outcome::Hit
        }
    } else if f64::from(rng.unit()) < f.miss {
        Outcome::Miss
    } else {
        Outcome::Evade
    }
}

/// How often a creature's missile misses outright: one that finds its target where it stood when
/// it was loosed (6 of 420 in 2017), and one whose target has moved away since (52 of 108).
pub const MISSILE_MISSES_STILL: f64 = 6.0 / 420.0;
pub const MISSILE_MISSES_MOVED: f64 = 52.0 / 108.0;

/// Roll a creature's missile (or bolt of magic) as it arrives: missed outright by the share for a
/// target that stood or moved, else landed by the measured chance for its kind, else evaded.
pub fn roll_missile(
    rng: &mut Rng,
    kind: Kind,
    attacker_level: i32,
    defender_level: i32,
    target_moved: bool,
) -> Outcome {
    let miss = if target_moved {
        MISSILE_MISSES_MOVED
    } else {
        MISSILE_MISSES_STILL
    };
    if f64::from(rng.unit()) < miss {
        return Outcome::Miss;
    }
    let kind = if kind == Kind::Melee {
        Kind::Missile
    } else {
        kind
    };
    let f = fit(Direction::CreatureOnPlayer, kind);
    let p = landing_chance(
        Direction::CreatureOnPlayer,
        kind,
        attacker_level,
        defender_level,
    );
    if f64::from(rng.unit()) < p {
        if f64::from(rng.unit()) < f.critical {
            Outcome::Critical
        } else {
            Outcome::Hit
        }
    } else {
        Outcome::Evade
    }
}

/// The critical multipliers: a player's critical doubles its damage, a creature's adds half (the
/// median critical over the median ordinary hit of the same skill on the same target, 1,678 and 248
/// criticals).
pub const PLAYER_CRITICAL: f32 = 2.0;
/// A player's special's critical over its plain hit (2017: 173 criticals of specials, the median
/// 1.01 of this and 0.63 of doubling; 128 of them against the plain hits of the same skill, armor and
/// session, 1.23 of those).
pub const SPECIAL_CRITICAL: f32 = 1.25;
pub const CREATURE_CRITICAL: f32 = 1.5;

/// What a player's armor lets through of a creature's natural damage, by the player's level: the
/// measured median of damage over natural damage for melee (levels 1-2 all of it, 39 hits; 3-5
/// 0.42, 79; 6-19 0.26, as 6-10's 0.25 over 229 and 10-19's 0.26; 20-29 0.45, 30-39 0.32, 40 and up
/// 0.11).
#[must_use]
pub fn armor_lets_through(player_level: i32) -> f32 {
    match player_level {
        ..=2 => 1.0,
        3..=5 => 0.42,
        6..=19 => 0.26,
        20..=29 => 0.45,
        30..=39 => 0.32,
        _ => 0.11,
    }
}

/// What a player's armor lets through of a creature's missiles, by the player's level: the
/// measured medians for missiles (3-19 0.29-0.31, 20-29 0.28, 30-39 0.46, 40 and up 0.21; none
/// measured below level 3).
#[must_use]
pub fn armor_lets_through_missiles(player_level: i32) -> f32 {
    match player_level {
        ..=19 => 0.31,
        20..=29 => 0.28,
        30..=39 => 0.46,
        _ => 0.21,
    }
}

/// What a creature's hit is made of: its natural damage and variance, what its skill's hook adds at
/// its rating, and the factor of a skill known without its definition (1 otherwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Natural {
    pub damage: f32,
    pub variance: f32,
    pub added: f32,
    pub factor: f32,
}

/// A creature's hit on a player: its natural damage times a uniform roll across its variance,
/// with what its skill adds, times what the player's armor lets through of an attack of its kind
/// (and the factor); a critical adds half. At least 1.
pub fn creature_hit(
    rng: &mut Rng,
    kind: Kind,
    n: Natural,
    player_level: i32,
    critical: bool,
) -> i32 {
    let v = n.variance.clamp(0.0, 1.0);
    let roll = rng.between(1.0 - v, 1.0 + v);
    let through = match kind {
        Kind::Missile => armor_lets_through_missiles(player_level),
        Kind::Melee | Kind::Magic => armor_lets_through(player_level),
    };
    let mut d = (n.damage * roll + n.added) * through * n.factor;
    if critical {
        d *= CREATURE_CRITICAL;
    }
    let d = dereth_primitives::num::to_i32(d.round());
    d.max(1)
}

/// A creature's plain hit on a player (nothing added).
pub fn creature_damage(
    rng: &mut Rng,
    kind: Kind,
    natural: f32,
    variance: f32,
    player_level: i32,
    critical: bool,
) -> i32 {
    creature_hit(
        rng,
        kind,
        Natural {
            damage: natural,
            variance,
            added: 0.0,
            factor: 1.0,
        },
        player_level,
        critical,
    )
}

/// The base of a player's hit by its level (authored: a level 1 character takes a level 2 creature
/// of 46 health in about six hits, as the start area's fights took in 2017), and the share either
/// side of it a hit varies by.
#[must_use]
pub fn player_base_damage(level: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let l = level.max(1) as f32;
    6.0 + 2.0 * (l - 1.0)
}
pub const PLAYER_VARIANCE: f32 = 0.3;

/// A weapon's damage as a hit rolls it: its greatest, and the share below that a hit may fall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponDamage {
    pub most: i32,
    pub variance: f32,
}

impl WeaponDamage {
    /// Its least: the greatest less its variance, rounded down (a sword of 167 and 0.25: 125).
    #[must_use]
    pub fn least(self) -> i32 {
        #[allow(clippy::cast_precision_loss)]
        let l = (self.most as f32 * (1.0 - self.variance.clamp(0.0, 1.0))).floor();
        dereth_primitives::num::to_i32(l)
    }
}

/// Who strikes a creature, as its damage is reckoned: its level, the weapon it holds (if any has a
/// damage), and what its effects add to its hits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hitter {
    pub level: i32,
    pub weapon: Option<WeaponDamage>,
    pub added: i32,
    /// What its weapon adds to the chance of a critical.
    pub critical: f64,
}

/// A player's hit on a creature. With a weapon: a whole number drawn evenly from the weapon's
/// least to its greatest, with what the player's effects add, less the creature's armor level, at
/// least 1, doubled when critical (2017: every examined weapon's damage shown as that span; hits
/// within the span so reckoned 95-100% at levels 1-35, the median hit 0.98-1.04 of the reckoning
/// over 2,259 hits; armor taken a point a level; criticals 2.02 times the ordinary hit). Without
/// one, [`player_damage`] and the effects' add.
pub fn player_hit(rng: &mut Rng, h: Hitter, creature_armor: i32, critical: bool) -> i32 {
    let Some(w) = h.weapon else {
        return player_damage(rng, h.level, creature_armor, critical) + h.added;
    };
    let (low, high) = (w.least().min(w.most), w.most);
    let span = usize::try_from(high - low + 1).unwrap_or(1);
    let roll = low + i32::try_from(rng.below(span)).unwrap_or(0);
    let d = (roll + h.added - creature_armor).max(1);
    if critical {
        d * 2
    } else {
        d
    }
}

/// A player's hit on a creature: the base for its level times a uniform roll, less the creature's
/// armor (a point less for each of its armor levels beyond the first), doubled when critical. At
/// least 1.
pub fn player_damage(rng: &mut Rng, level: i32, creature_armor: i32, critical: bool) -> i32 {
    let roll = rng.between(1.0 - PLAYER_VARIANCE, 1.0 + PLAYER_VARIANCE);
    #[allow(clippy::cast_precision_loss)]
    let mut d = player_base_damage(level) * roll - (creature_armor - 1).max(0) as f32;
    if critical {
        d *= PLAYER_CRITICAL;
    }
    let d = dereth_primitives::num::to_i32(d.round());
    d.max(1)
}
