//! The checks a legitimate character passes: its numbers are ones play could have produced.
//!
//! They are ACE's own invariants, the arithmetic of its `verify-*` console commands, for one
//! character and with nothing skipped:
//! - the experience spent on attributes, vitals, skills and purchased augmentations is the
//!   experience earned less what is unspent, and the earned experience is the level's; the
//!   experience held beyond that is creation's free ranks (526 for each skill it trained). Where
//!   the character's creation is known (a build's), that must be exactly its free ranks; where it
//!   is not (a character as stored), a whole number of them, as `verify-xp` takes it;
//! - the credits are the heritage's and the levels' (and the quests', when they were done), the
//!   spent ones are what the trained and specialized skills cost the heritage, at most 70 of them
//!   on specializations, and the rest are unspent;
//! - every rank is the rank its experience buys, within the curve's cap; a skill starts at 10
//!   when specialized and 0 otherwise, and an untrained skill has no experience;
//! - the innate attributes are a legal creation, each 10 to 100 and together the heritage's
//!   points, before the 5 each innate augmentation adds;
//! - every skill is one the files have, none is specialized that only an augmentation can but by
//!   its augmentation (which costs only its trained credits), and the character holds its
//!   heritage's free augmentation;
//! - each augmentation is held within the server's limits, its own and its family's, and an aura
//!   only from the level luminance can be earned, no more than Nalicana and one Seer sell.

use std::collections::BTreeMap;

use crate::augmentations::{table, Family};
use crate::budget::{self, Budget};
use crate::{Sac, Tables, AUGMENTATION_ONLY_COST, SPECIALIZED_CREDIT_LIMIT};

/// A skill as a character holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetSkill {
    pub id: u32,
    pub sac: Sac,
    /// Its starting level.
    pub start: u32,
    pub ranks: u32,
    pub xp: u32,
}

/// A character's numbers, as they are stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub level: u32,
    pub heritage: u32,
    pub total_xp: i64,
    pub available_xp: i64,
    pub total_credits: i32,
    pub available_credits: i32,
    /// The credits its finished quests granted.
    pub quest_credits: i32,
    /// `(id, innate, ranks, xp)` of each attribute.
    pub attributes: Vec<(u32, u32, u32, u32)>,
    /// `(id, ranks, xp)` of each vital.
    pub vitals: Vec<(u32, u32, u32)>,
    pub skills: Vec<SheetSkill>,
    /// What its purchased augmentations cost (the heritage's free one aside).
    pub augmentation_xp: i64,
    /// How many of its heritage's own augmentation it holds; `None` for a heritage that has none.
    pub heritage_augmentation: Option<i32>,
    /// The experience creation's free ranks are, when its creation is known; `None` for a
    /// character as stored, whose creation is not.
    pub creation_xp: Option<i64>,
    /// Every augmentation and aura it holds, by its int property, and how many.
    pub augmentations: BTreeMap<u32, i32>,
}

/// What a finding is about, so a view can take the builder to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Level,
    Experience,
    Credits,
    Innate,
    Attribute(u32),
    Vital(u32),
    Skill(u32),
    Augmentation,
    /// What it wears and wields.
    Gear,
    /// What it carries in its packs.
    Items,
    Titles,
    Quests,
}

/// One thing wrong with a character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub field: Field,
    pub text: String,
    /// Possible but unusual: it is shown, and does not refuse the character.
    pub warning: bool,
}

/// Everything wrong with `s`. Empty for a character a player could have.
#[must_use]
pub fn check(t: &Tables<'_>, s: &Sheet) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut refuse = |field: Field, text: String| {
        out.push(Finding {
            field,
            text,
            warning: false,
        });
    };
    let budget = match Budget::of(t, s.level, s.heritage) {
        Ok(b) => b,
        Err(e) => {
            refuse(Field::Level, e);
            return out;
        }
    };

    // The experience.
    let level_xp = i64::try_from(budget.experience).unwrap_or(i64::MAX);
    let next =
        t.xp.level_xp
            .get(usize::try_from(s.level).unwrap_or(usize::MAX) + 1)
            .map(|x| i64::try_from(*x).unwrap_or(i64::MAX));
    if s.total_xp < level_xp || next.is_some_and(|n| s.total_xp >= n) {
        refuse(
            Field::Experience,
            format!("Level {} is not the level of its experience.", s.level),
        );
    }
    if s.available_xp < 0 || s.available_xp > s.total_xp {
        refuse(
            Field::Experience,
            "The unspent experience is more than was earned.".into(),
        );
    }
    let spent: i64 = s
        .attributes
        .iter()
        .map(|a| i64::from(a.3))
        .chain(s.vitals.iter().map(|v| i64::from(v.2)))
        .chain(s.skills.iter().map(|k| i64::from(k.xp)))
        .sum::<i64>()
        + s.augmentation_xp;
    let earned = s.total_xp - s.available_xp;
    let free_ranks_xp =
        t.xp.trained_xp
            .get(usize::try_from(crate::CREATION_RANKS).unwrap_or(usize::MAX))
            .map_or(0, |x| i64::from(*x));
    let reconciles = match s.creation_xp {
        Some(created) => spent == earned + created,
        // As `verify-xp` takes a character: what it holds beyond what it earned is a whole
        // number of creation's free ranks.
        None => free_ranks_xp > 0 && (spent - earned) % free_ranks_xp == 0,
    };
    if !reconciles {
        refuse(
            Field::Experience,
            format!(
                "The experience spent ({spent}) is not the experience earned less the unspent ({earned}), with creation's free ranks."
            ),
        );
    }

    // The ranks.
    for &(id, _, ranks, xp) in &s.attributes {
        rank_check(
            &mut refuse,
            Field::Attribute(id),
            &t.xp.attribute_xp,
            ranks,
            xp,
        );
    }
    for &(id, ranks, xp) in &s.vitals {
        rank_check(&mut refuse, Field::Vital(id), &t.xp.vital_xp, ranks, xp);
    }

    // The innate attributes, as created: less the 5 each innate augmentation adds.
    let held = |property: u32| {
        s.augmentations
            .get(&property)
            .and_then(|n| u32::try_from(*n).ok())
            .unwrap_or(0)
    };
    let innate: Vec<u32> = s
        .attributes
        .iter()
        .map(|a| {
            let raised: u32 = table()
                .augmentation
                .iter()
                .filter(|x| x.attribute == Some(a.0))
                .filter_map(|x| x.property)
                .map(|p| 5 * held(p))
                .sum();
            a.1.saturating_sub(raised)
        })
        .collect();
    if innate.len() != 6
        || innate.iter().any(|v| !(10..=100).contains(v))
        || s.attributes.iter().any(|a| a.1 > 100)
        || innate.iter().sum::<u32>() != budget.attribute_points
    {
        refuse(
            Field::Innate,
            format!(
                "The innate attributes must each be from 10 to 100 and come to {}.",
                budget.attribute_points
            ),
        );
    }

    // The skills and the credits. A skill its augmentation specializes costs its trained credits
    // alone, and none of the specializations' 70.
    let by_augmentation: Vec<u32> = table()
        .augmentation
        .iter()
        .filter(|a| a.property.is_some_and(|p| held(p) > 0))
        .filter_map(|a| t.skill_named(a.skill.as_deref()?))
        .collect();
    let mut used = 0;
    let mut specialized = 0;
    for k in &s.skills {
        let field = Field::Skill(k.id);
        let name = t.skill_name(k.id);
        if k.sac < Sac::Trained {
            if k.xp > 0 || k.ranks > 0 {
                refuse(field, format!("{name} is untrained but has experience."));
            }
            continue;
        }
        if !t.has_skill(k.id) {
            refuse(field, format!("The data files have no skill {}.", k.id));
            continue;
        }
        let (trained, specialized_cost) = budget::costs(t, s.heritage, k.id);
        if trained < 0 || specialized_cost < 0 {
            refuse(field, format!("{name} cannot be trained."));
            continue;
        }
        if k.sac == Sac::Specialized && by_augmentation.contains(&k.id) {
            used += budget::spent_on(t, s.heritage, k.id, Sac::Trained);
        } else {
            used += budget::spent_on(t, s.heritage, k.id, k.sac);
        }
        if k.sac == Sac::Specialized && !by_augmentation.contains(&k.id) {
            if specialized_cost >= AUGMENTATION_ONLY_COST {
                refuse(
                    field,
                    format!("{name} is specialized only by an augmentation."),
                );
            }
            specialized += specialized_cost;
        }
        let want_start = if k.sac == Sac::Specialized { 10 } else { 0 };
        if k.start != want_start {
            refuse(field, format!("{name} starts at {}.", k.start));
        }
        if let Some(curve) = budget::skill_curve(t, k.sac) {
            rank_check(&mut refuse, field, curve, k.ranks, k.xp);
        }
    }
    let credits = budget.credits() + s.quest_credits;
    if s.total_credits != credits {
        refuse(
            Field::Credits,
            format!(
                "A character of level {} has {credits} skill credits, not {}.",
                s.level, s.total_credits
            ),
        );
    }
    if used > credits {
        refuse(
            Field::Credits,
            format!("The skills cost {used} credits, more than the {credits} it has."),
        );
    } else if s.available_credits != credits - used {
        refuse(
            Field::Credits,
            format!(
                "It should have {} credits unspent, not {}.",
                credits - used,
                s.available_credits
            ),
        );
    }
    if specialized > SPECIALIZED_CREDIT_LIMIT {
        refuse(
            Field::Credits,
            format!(
                "Its specializations cost {specialized} credits, more than {SPECIALIZED_CREDIT_LIMIT}."
            ),
        );
    }

    // The heritage's free augmentation.
    if let Some(n) = s.heritage_augmentation {
        if n < 1 {
            refuse(
                Field::Augmentation,
                "It lacks its heritage's free augmentation.".into(),
            );
        }
    }
    held_check(&mut refuse, s);
    out
}

/// The augmentations and auras `s` holds are within the server's limits, each its own and its
/// family's; and an aura is held only from the level luminance can be earned, no more ranks of
/// each than Nalicana sells and, of the two a Seer sells, its five more.
fn held_check(refuse: &mut impl FnMut(Field, String), s: &Sheet) {
    let table = table();
    let mut families: BTreeMap<&str, i32> = BTreeMap::new();
    for (property, n) in &s.augmentations {
        if let Some(a) = table.by_property(*property) {
            if *n < 0 || *n > i32::try_from(a.max).unwrap_or(i32::MAX) {
                refuse(
                    Field::Augmentation,
                    format!("It holds {} {n} times; at most {}.", a.name, a.max),
                );
            }
            match a.family {
                Some(Family::Innate) => *families.entry("innate").or_default() += n,
                Some(Family::Resistance) => *families.entry("resistance").or_default() += n,
                None => {}
            }
        }
    }
    for (family, most) in [
        ("innate", table.family.innate),
        ("resistance", table.family.resistance),
    ] {
        if families.get(family).copied().unwrap_or(0) > i32::try_from(most).unwrap_or(i32::MAX) {
            refuse(
                Field::Augmentation,
                format!("It holds more than {most} {family} augmentations."),
            );
        }
    }
    // The auras: over Nalicana's ranks only where one Seer sells them.
    let mut beyond: Vec<u32> = Vec::new();
    let mut any = false;
    for a in &table.aura {
        let n = s.augmentations.get(&a.property).copied().unwrap_or(0);
        if n == 0 {
            continue;
        }
        any = true;
        if n < 0 || u32::try_from(n).unwrap_or(0) > a.ranks + table.seers.ranks {
            refuse(
                Field::Augmentation,
                format!("It holds {} {n} times.", a.name),
            );
        } else if u32::try_from(n).unwrap_or(0) > a.ranks {
            beyond.push(a.property);
        }
    }
    if any && s.level < table.luminance.from_level {
        refuse(
            Field::Augmentation,
            format!(
                "Auras can be bought from level {}.",
                table.luminance.from_level
            ),
        );
    }
    if !beyond.is_empty()
        && !table
            .seer
            .iter()
            .any(|seer| beyond.iter().all(|p| seer.auras.contains(p)))
    {
        refuse(
            Field::Augmentation,
            "No one Seer sells all the auras it holds past Nalicana's.".into(),
        );
    }
}

/// A rank and its experience agree: the rank is what the experience buys along `curve`, and the
/// experience is within the curve.
fn rank_check(
    refuse: &mut impl FnMut(Field, String),
    field: Field,
    curve: &[u32],
    ranks: u32,
    xp: u32,
) {
    let cap = curve.last().copied().unwrap_or(0);
    if xp > cap {
        refuse(
            field,
            format!("Its experience ({xp}) is past the cap ({cap})."),
        );
    } else if budget::rank_of(curve, xp) != ranks {
        refuse(
            field,
            format!(
                "Its rank is {ranks}, but its experience buys {}.",
                budget::rank_of(curve, xp)
            ),
        );
    }
}
