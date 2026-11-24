//! The rolls of a fight: whether an attack lands, how well, and for how much.
//!
//! **Whether it lands** goes by its two ratings, its user's accuracy against its target's evasion
//! ([`crate::ratings`]): the chance is `1 / (1 + e^-(k × (accuracy - evasion)))`, kept within
//! [`LANDING`]'s least and most (2017: the chance to hit the server showed players on 56,166
//! examined creatures was the roll, 49,173 of a player's basic attacks landing at the chance shown;
//! `k` 0.0429, 0.0423 and 0.0437 per 10 points of a template's rating within a character for magic,
//! melee and missile, and 0.0434, 0.0437 and 0.0421 for what effects add to the attack attribute;
//! none at equal ratings at level 1; the shown chance never past 95% nor under 5%). An attack that
//! does not land is evaded, or for a ranged attack missed in the measured share; nothing is blocked
//! (2017's results are hits, criticals, misses and evades). A player's landed attack is critical in
//! the measured share of landed attacks, a basic attack with what its weapon adds, a special by
//! that alone; a creature's at the chance its skill sets, with what its weapon adds.
//!
//! **How much** a creature's hit does: its natural damage times a uniform roll across its natural
//! variance, times what the player's armor lets through (the measured share for the band of the
//! player's level: all of it at levels 1 and 2, before players wore armor); a critical is its
//! skill's size more.
//! A player's hit: its skill's damage (authored until the skills' attack hooks are applied: a base
//! by level with the same kind of roll, less a point for each of the creature's armor levels, as
//! 2017's hits lost about one); a critical doubles it.

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

/// An attack's two ratings: its user's accuracy and its target's evasion ([`crate::ratings`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ratings {
    pub accuracy: f64,
    pub evasion: f64,
}

/// The rule from an attack's ratings to its chance to land: the slope of the chance's logit a
/// point of accuracy over evasion, and the least and most the chance is kept within.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    pub slope: f64,
    pub least: f64,
    pub most: f64,
}

/// The measured rule (2017; see the module's notes): 0.0043 a point, kept within 5% and 95% (5,295
/// creatures shown at 95%, one above; basic attacks shown at 95% landing 0.951, 0.939 and 0.943 by
/// kind).
pub const LANDING: Landing = Landing {
    slope: 0.0043,
    least: 0.05,
    most: 0.95,
};

impl Landing {
    /// The chance an attack at these ratings lands.
    #[must_use]
    pub fn chance(self, r: Ratings) -> f64 {
        let p = 1.0 / (1.0 + math::exp(-self.slope * (r.accuracy - r.evasion)));
        p.clamp(self.least, self.most)
    }
}

/// The measured shares for one direction and kind: the critical share of landed attacks, the miss
/// share of attacks that did not land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub critical: f64,
    pub miss: f64,
}

const fn two(critical: f64, miss: f64) -> Fit {
    Fit { critical, miss }
}

/// The measured shares, by direction and kind (attacks: a player's melee 26,005, missile 26,742
/// and magic 20,891; a creature's melee 20,342, missile 16,029, magic 9,893). A player's critical
/// is its basic attacks' with a weapon that adds nothing (melee 1,389 hits, missile 5,665, magic
/// 2,080); a creature's, of all its skills at once, stands for a skill whose definition the server
/// lacks ([`skill_critical`] otherwise).
#[must_use]
pub const fn fit(dir: Direction, kind: Kind) -> Fit {
    match (dir, kind) {
        (Direction::PlayerOnCreature, Kind::Melee) => two(0.051, 0.0),
        (Direction::PlayerOnCreature, Kind::Missile) => two(0.055, 0.248),
        (Direction::PlayerOnCreature, Kind::Magic) => two(0.043, 0.028),
        (Direction::CreatureOnPlayer, Kind::Melee) => two(0.023, 0.0),
        (Direction::CreatureOnPlayer, Kind::Missile) => two(0.029, 0.346),
        (Direction::CreatureOnPlayer, Kind::Magic) => two(0.012, 0.208),
    }
}

/// The chance an attack at these ratings lands, by the measured rule ([`LANDING`]).
#[must_use]
pub fn landing_chance(r: Ratings) -> f64 {
    LANDING.chance(r)
}

/// Roll an attack's outcome.
pub fn roll_outcome(rng: &mut Rng, dir: Direction, kind: Kind, ratings: Ratings) -> Outcome {
    let critical = fit(dir, kind).critical;
    roll_with_critical(rng, dir, kind, ratings, critical)
}

/// A player's basic attack: its outcome, its chance of a critical the measured one with what its
/// weapon adds ([`crate::combat::critical_bonus`]).
pub fn roll_basic(rng: &mut Rng, kind: Kind, ratings: Ratings, bonus: f64) -> Outcome {
    let dir = Direction::PlayerOnCreature;
    let critical = fit(dir, kind).critical + bonus;
    roll_with_critical(rng, dir, kind, ratings, critical)
}

/// A player's special: its outcome, its chance of a critical what its skill's class sets
/// ([`skill_critical`]) with what its weapon adds (2017, players' landed specials counted once: at
/// 2, 434 of 92,997, all by weapons; at 1.8, 469 of 2,258, 0.208).
pub fn roll_special(rng: &mut Rng, kind: Kind, ratings: Ratings, critical: f64) -> Outcome {
    let dir = Direction::PlayerOnCreature;
    roll_with_critical(rng, dir, kind, ratings, critical)
}

/// The chance a landed attack is critical by the threshold its skill's class sets: what the
/// threshold falls short of 2, none at 2 or more (2017, creatures' landed attacks with a weapon that
/// adds nothing: at 2, 0 of 162,680; at 1.975, 0.0225 of 17,021; at 1.95, 0.0503 of 12,114).
#[must_use]
pub fn skill_critical(threshold: f32) -> f64 {
    (2.0 - f64::from(threshold)).clamp(0.0, 1.0)
}

/// An attack's outcome at a chance of a critical among those that land.
pub fn roll_with_critical(
    rng: &mut Rng,
    dir: Direction,
    kind: Kind,
    ratings: Ratings,
    critical: f64,
) -> Outcome {
    let f = fit(dir, kind);
    let p = landing_chance(ratings);
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
/// it was loosed (2017: 794 of 10,349 at players), and one whose target has moved more
/// than a meter since (2,157 of 8,358).
pub const MISSILE_MISSES_STILL: f64 = 794.0 / 10_349.0;
pub const MISSILE_MISSES_MOVED: f64 = 2_157.0 / 8_358.0;

/// How a player's thrown attack misses: `still`, the share of those that do not land on a target
/// that stood that miss rather than being evaded; `moved`, the share of those that would land that
/// miss instead, the target having moved more than a meter since the throw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrownMisses {
    pub still: f64,
    pub moved: f64,
}

/// An arrow, a missile thrown alone (2017, players' throws at creatures: on a creature that stood,
/// 8,208 of 10,001 landed and 151 of the 1,793 others missed; on one that moved, 656 of 2,332
/// missed). A moved target's misses over a still one's come from those that would have landed.
pub const ARROW_MISSES: ThrownMisses = ThrownMisses {
    still: 151.0 / 1_793.0,
    moved: (656.0 / 2_332.0 - 151.0 / 10_001.0) / (8_208.0 / 10_001.0),
};

/// A bolt, a missile thrown after a hook that only plays (2017: on a creature that stood, 2,302 of
/// 3,051 landed and 44 of the 749 others missed; on one that moved, 56 of 817 missed).
pub const BOLT_MISSES: ThrownMisses = ThrownMisses {
    still: 44.0 / 749.0,
    moved: (56.0 / 817.0 - 44.0 / 3_051.0) / (2_302.0 / 3_051.0),
};

/// How a player's arrow or bolt misses.
#[must_use]
pub const fn thrown_misses(bolt: bool) -> ThrownMisses {
    if bolt {
        BOLT_MISSES
    } else {
        ARROW_MISSES
    }
}

/// Roll a player's thrown attack as it is loosed: landed by its ratings (critical at the chance
/// given), else missed by the share for a target that stands, else evaded.
pub fn roll_thrown(rng: &mut Rng, ratings: Ratings, critical: f64, bolt: bool) -> Outcome {
    if f64::from(rng.unit()) < landing_chance(ratings) {
        if f64::from(rng.unit()) < critical {
            Outcome::Critical
        } else {
            Outcome::Hit
        }
    } else if f64::from(rng.unit()) < thrown_misses(bolt).still {
        Outcome::Miss
    } else {
        Outcome::Evade
    }
}

/// Does a player's thrown attack that would land miss instead, its target having moved since?
pub fn misses_the_moved(rng: &mut Rng, bolt: bool) -> bool {
    f64::from(rng.unit()) < thrown_misses(bolt).moved
}

/// Roll a creature's missile (or bolt of magic) as it arrives: missed outright by the share for a
/// target that stood or moved, else landed by its ratings (critical at the chance given), else
/// evaded.
pub fn roll_missile(rng: &mut Rng, ratings: Ratings, target_moved: bool, critical: f64) -> Outcome {
    let miss = if target_moved {
        MISSILE_MISSES_MOVED
    } else {
        MISSILE_MISSES_STILL
    };
    if f64::from(rng.unit()) < miss {
        return Outcome::Miss;
    }
    if f64::from(rng.unit()) < landing_chance(ratings) {
        if f64::from(rng.unit()) < critical {
            Outcome::Critical
        } else {
            Outcome::Hit
        }
    } else {
        Outcome::Evade
    }
}

/// A player's critical doubles its damage (the median critical over the median ordinary hit of the
/// same skill on the same target, 1,678 criticals).
pub const PLAYER_CRITICAL: f32 = 2.0;
/// A player's special's critical over its plain hit where no class of its skill sets how much more
/// a critical does; a skill's class most often sets 1.2 (2017: 173 criticals of specials, the
/// median 1.01 of this and 0.63 of doubling; 128 of them against the plain hits of the same skill,
/// armor and session, 1.23 of those).
pub const SPECIAL_CRITICAL: f32 = 1.25;

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
/// its rating, the factor of a skill known without its definition (1 otherwise), the most of the
/// hit armor leaves it when armor takes nearly all of it (its template's armor threshold, else
/// [`ARMOR_FLOOR`]'s top), and how much more a critical of its skill does before armor (the
/// modifier the skill's class sets, else [`CRITICAL_BEFORE_ARMOR`]), on every target alike.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Natural {
    pub damage: f32,
    pub variance: f32,
    pub added: f32,
    pub factor: f32,
    pub threshold: f32,
    pub critical: f32,
}

/// The most of a creature's hit its armor threshold lets armor leave it: the template's own, else
/// [`ARMOR_FLOOR`]'s top (2017: hits of creatures whose templates set 0.1 came to 0.63 of a floor
/// drawn up to a quarter where armor reached 0.7 to 1.3 of the hit, 386 hits, and 0.92 of one
/// drawn up to 0.1; those setting 0.2, 0.86 and 0.92 over 2,096; at levels 35 to 44 0.86 and
/// 0.99 over 145; the rest 1.00 over 4,081).
#[must_use]
pub fn threshold_of(stats: &yalain_content::CreatureStats) -> f32 {
    stats
        .armor_threshold
        .filter(|t| *t > ARMOR_FLOOR.0)
        .unwrap_or(ARMOR_FLOOR.1)
}

/// A creature's critical on a player, before armor, where its skill's class sets no size of its own
/// (2017: 327 critical hits of skills whose classes set 1.2, damage with the armor back on over the
/// plain mean, 1.20 at every rating up to the mean hit). A class that sets its own is read: 1 does
/// no more than a plain hit, a boss's 1.25 to 1.5 more (2017: the Burun's combos, whose class sets
/// 1, struck 8 criticals of 77 to 96, a mean of 87.5, among plain hits of 70 to 101, a mean of 84,
/// on the same player, where 1.2 would have made 93 to 141; the Gigurath's Ground Tremor, 1.5, 2
/// criticals of 964 and 1,116 against plain hits of 523 and 705 on the same players, where 1.2
/// would have made about 690 and 870).
pub const CRITICAL_BEFORE_ARMOR: f32 = 1.2;
/// What a hit can come to when armor has taken more than this much of it: a share drawn from these
/// (the top a creature's armor threshold may set lower), of the hit before armor, if that's more
/// than armor left (2017: armor past natural, 1,641 hits,
/// the top twentieth at 0.23 of natural for the least variance and 0.20 for the most, a mean of 0.11;
/// armor reaching 0.8 to 0.9 of the hit keeps what it left, 1,055 hits).
pub const ARMOR_FLOOR: (f32, f32) = (0.01, 0.25);
/// Up to this level a player's hits keep the wide roll in full and read no armor (2017: 39 hits at
/// levels 1 and 2 ran from 0.8 to 1.6 of natural).
pub const LEVEL_BEFORE_ARMOR: i32 = 2;

/// A creature's hit on a player: its natural damage rolled from the share its variance leaves up to
/// all of it, with what its skill adds (and the factor), a critical its skill's size more
/// ([`Natural::critical`]), less the armor the player has, a point for a point. What armor leaves
/// is never below a draw of up to a quarter of the hit, so no rating stops one. Armor effects have
/// taken below nothing adds to the hit (2017: a group of 1,242 hits at armor 100 within 0.4 of
/// this; a point of armor a point at every level from 6 up, melee, missile and magic alike).
pub fn creature_hit_on_player(
    rng: &mut Rng,
    kind: Kind,
    n: Natural,
    player_level: i32,
    armor: f32,
    critical: bool,
) -> i32 {
    if player_level <= LEVEL_BEFORE_ARMOR {
        return creature_hit(rng, kind, n, player_level, critical);
    }
    creature_hit_on_armor(rng, n, armor, critical)
}

/// A creature's hit on what has this much armor: a player past the lowest levels, or a pet of a
/// kind whose armor is known (2017: the hits on pets that armor leaves clear of the floor read
/// `natural x (1 - variance / 2) - armor` for every kind measured, the armor rising with the pet's
/// level).
pub fn creature_hit_on_armor(rng: &mut Rng, n: Natural, armor: f32, critical: bool) -> i32 {
    let v = n.variance.clamp(0.0, 1.0);
    let mut d = (n.damage * rng.between(1.0 - v, 1.0) + n.added) * n.factor;
    if critical {
        d *= n.critical;
    }
    let floor = rng.between(ARMOR_FLOOR.0, n.threshold.max(ARMOR_FLOOR.0)) * d;
    dereth_primitives::num::to_i32((d - armor).max(floor).round()).max(1)
}

/// A creature's hit on a player of the lowest levels, or on a pet whose armor is not known: its
/// natural damage times a uniform roll across its variance, with what its skill adds, times what
/// armor lets through of an attack of its kind by the level hit (and the factor); a critical its
/// skill's size more ([`Natural::critical`]), as on a player past them (2017: no creature critical
/// on a player of level 1 or 2 in 90 attacks, 39 landed; the size measured before armor elsewhere
/// is the class's, and armor here takes nothing). At least 1.
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
        d *= n.critical;
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
            threshold: ARMOR_FLOOR.1,
            critical: CRITICAL_BEFORE_ARMOR,
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
/// damage), what its effects add to its hits, and what its damage pool adds to this one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hitter {
    pub level: i32,
    pub weapon: Option<WeaponDamage>,
    pub added: i32,
    /// What its weapon adds to the chance of a critical.
    pub critical: f64,
    /// What its damage pool adds to this hit ([`DamagePool`]): to the damage before its roll.
    pub pool: i32,
}

/// A player's damage pool for one attack: the points it adds, and the chance each landed blow has
/// of taking them. An attack takes them at most once, its blows trying in turn until one does.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DamagePool {
    pub points: i32,
    pub chance: f32,
}

impl DamagePool {
    /// What this landed blow takes of the pool: all of it at the pool's chance, none once an
    /// earlier blow of the attack took it. An empty pool draws nothing.
    pub fn take(&mut self, rng: &mut Rng) -> i32 {
        if self.points <= 0 || self.chance <= 0.0 || !rng.chance(self.chance) {
            return 0;
        }
        std::mem::take(&mut self.points)
    }
}

/// A player's hit on a creature. With a weapon: a whole number drawn evenly from the weapon's
/// least to its greatest, with what the player's effects add, less the creature's armor level, at
/// least 1, doubled when critical (2017: every examined weapon's damage shown as that span; hits
/// within the span so reckoned 95-100% at levels 1-35, the median hit 0.98-1.04 of the reckoning
/// over 2,259 hits; armor taken a point a level; criticals 2.02 times the ordinary hit). Without
/// one, [`player_damage`] and the effects' add. A damage pool's points join the weapon's damage
/// (or the base) before the roll, so the roll's share applies to them too.
pub fn player_hit(rng: &mut Rng, h: Hitter, creature_armor: i32, critical: bool) -> i32 {
    let Some(w) = h.weapon else {
        return player_damage(rng, h.level, h.pool, creature_armor, critical) + h.added;
    };
    let w = WeaponDamage {
        most: w.most + h.pool,
        ..w
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

/// A player's hit on a creature: the base for its level, with what its damage pool adds, times a
/// uniform roll, less the creature's armor (a point less for each of its armor levels beyond the
/// first), doubled when critical. At least 1.
pub fn player_damage(
    rng: &mut Rng,
    level: i32,
    pool: i32,
    creature_armor: i32,
    critical: bool,
) -> i32 {
    let roll = rng.between(1.0 - PLAYER_VARIANCE, 1.0 + PLAYER_VARIANCE);
    #[allow(clippy::cast_precision_loss)]
    let mut d =
        (player_base_damage(level) + pool as f32) * roll - (creature_armor - 1).max(0) as f32;
    if critical {
        d *= PLAYER_CRITICAL;
    }
    let d = dereth_primitives::num::to_i32(d.round());
    d.max(1)
}
