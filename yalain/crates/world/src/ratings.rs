//! Accuracy and evasion: the two ratings that decide whether an attack lands
//! ([`crate::rules::landing_chance`]).
//!
//! An attack's skill names what its user's accuracy and its target's evasion are made of
//! ([`crate::skills::rating_parts`]): each part a weight times a rating in a skill (an attack or
//! defense attribute, a mastery, a class's adept skill), or times the level part. So a point of an
//! attribute is a point of rating, and whatever changes a skill's rating (an effect, jewelry, a
//! debuff) changes the rating with it.
//!
//! - **A player's** rating in a skill is its rating with its effects
//!   ([`crate::skill_use::effective_rating`]; a skill it does not hold rates nothing). Its level
//!   part is [`LEVEL_POINTS`] a level up to [`LEVEL_POINTS_UP_TO`], plus its Heroic Mastery
//!   attribute.
//! - **A creature's** rating in a skill is its template's level in it, with its effects; its level
//!   part is nothing.

use yalain_net::ConnId;
use yalain_store::Store;

use crate::rules::Ratings;
use crate::skills::{Rated, RatingPart};
use crate::Server;

/// The points of the level part a player's level gives, a level, and the level they stop at.
pub const LEVEL_POINTS: f64 = 8.0;
pub const LEVEL_POINTS_UP_TO: i32 = 50;
/// The attribute category whose value adds to a player's level part (Heroic Mastery).
pub const HEROIC_MASTERY_CATEGORY: u32 = 6;

/// Who an attack's rating is of: a player, by its world connection, or a thing in the world (a
/// creature, a pet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rater {
    Player(ConnId),
    Thing(u64),
}

/// The ratings of an attack with a skill: its user's accuracy against its target's evasion, each
/// by the parts the skill names. A skill the server does not know rates both sides at nothing.
#[must_use]
pub fn of<S: Store>(s: &Server<S>, skill: u32, attacker: Rater, defender: Rater) -> Ratings {
    let Some(d) = s.skill_table.skills.get(&skill) else {
        return Ratings::default();
    };
    Ratings {
        accuracy: sum(s, attacker, &d.accuracy),
        evasion: sum(s, defender, &d.evasion),
    }
}

/// A side's rating by a list of parts.
#[must_use]
pub fn sum<S: Store>(s: &Server<S>, who: Rater, parts: &[RatingPart]) -> f64 {
    if parts.is_empty() {
        return 0.0;
    }
    match who {
        Rater::Player(world) => {
            let Some(p) = s.players.get(&world) else {
                return 0.0;
            };
            let held = crate::skill_use::rated_skills(s, p);
            let level = crate::examine::level_of(&p.qualities);
            parts
                .iter()
                .map(|part| {
                    let v = match part.of {
                        Rated::Level => {
                            LEVEL_POINTS * f64::from(level.clamp(0, LEVEL_POINTS_UP_TO))
                                + f64::from(crate::effects::attribute(
                                    s,
                                    p,
                                    &held,
                                    HEROIC_MASTERY_CATEGORY,
                                ))
                        }
                        Rated::Skill(k) => {
                            f64::from(crate::skill_use::effective_rating(s, p, &held, k))
                        }
                    };
                    f64::from(part.weight) * v
                })
                .sum()
        }
        Rater::Thing(id) => {
            let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
                return 0.0;
            };
            let records = crate::effects::records_of(s, crate::effects::On::Creature(id));
            parts
                .iter()
                .map(|part| match part.of {
                    Rated::Level => 0.0,
                    Rated::Skill(k) => {
                        let own = c
                            .stats
                            .skills
                            .iter()
                            .find(|(sk, _)| *sk == k)
                            .map_or(0, |(_, r)| u32::try_from(*r).unwrap_or(0));
                        let rating = crate::effects::skill_rating(&s.effect_table, records, k, own);
                        f64::from(part.weight) * f64::from(rating)
                    }
                })
                .sum()
        }
    }
}
