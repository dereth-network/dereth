//! What a player gained beyond the build itself: augmentations bought with experience, luminance
//! and the auras bought with it, titles, and the quests it has solved, some of which grant skill
//! credits.
//!
//! **Augmentations come out of the budget.** Each one bought with experience costs what its gem
//! costs in the world ([`Costs`]), and that experience is not there to raise anything else. The
//! heritage's free augmentation is always held and costs nothing. Each is held within the
//! server's own limits, and its effects are part of the build: an innate augmentation raises its
//! attribute's innate value by 5, and a skill augmentation specializes a skill the build trains.
//! How many a character holds against what end-of-retail characters of its level held is the
//! band: past it is unusual, and warned of, not refused.
//!
//! **Luminance** can be earned from level 200, and only then can a character hold auras. A
//! character that can earn it holds the most it can hold, and has spent what it earned.
//!
//! **Quests** are held as they are named, solved the times given, long enough ago that none of
//! their timers runs. The quests that grant skill credits add a credit for each solve, from the
//! level a character can do them.
//!
//! **A summoning mastery** decides which summoning gems a character can use. A player chooses one
//! at a statue in Arwic from level 50; one chosen earlier is unusual, and warned of.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::augmentations::{table, Costs, Family};
use crate::check::{Field, Finding};
use crate::{heritage_augmentation, Build, Sac, Tables, ATTRIBUTES};

/// What a player gained beyond the build.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extras {
    /// The augmentations bought with experience, by id, and how many of each; the heritage's free
    /// one is held besides.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub augmentations: BTreeMap<String, u32>,
    /// Whether it can earn luminance, and what it bought with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luminance: Option<Luminous>,
    /// The titles it holds, by id, the one shown first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub titles: Vec<u32>,
    /// The quests it has solved, by name, and how many times.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub quests: BTreeMap<String, u32>,
    /// The quests that grant skill credits, solved as far as its level allows.
    #[serde(default, skip_serializing_if = "is_false")]
    pub quest_credits: bool,
    /// Its summoning mastery; none when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summoning_mastery: Option<Mastery>,
}

/// A summoning mastery: which summoning gems a character can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mastery {
    Primalist,
    Necromancer,
    Naturalist,
}

impl Mastery {
    /// Every mastery, in the order of their numbers.
    pub const ALL: [Self; 3] = [Self::Primalist, Self::Necromancer, Self::Naturalist];

    /// Its number, as a character's summoning mastery property holds it.
    #[must_use]
    pub const fn number(self) -> i32 {
        match self {
            Self::Primalist => 1,
            Self::Necromancer => 2,
            Self::Naturalist => 3,
        }
    }

    /// Its name, as a player reads it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Primalist => "Primalist",
            Self::Necromancer => "Necromancer",
            Self::Naturalist => "Naturalist",
        }
    }
}

/// The level from which a player can choose a summoning mastery.
pub const MASTERY_FROM_LEVEL: u32 = 50;

/// What a character that can earn luminance bought with it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Luminous {
    /// The auras, by id, and how many ranks of each.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub auras: BTreeMap<String, u32>,
    /// The Seer it follows, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seer: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's signature
const fn is_false(b: &bool) -> bool {
    !*b
}

/// How the extras change a build: what is set aside of the experience, the credits the quests
/// grant, the innate values raised, the skills specialized, and the augmentations and auras held.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Adjust {
    /// The experience the augmentations cost.
    pub reserve: u64,
    /// The skill credits its quests granted.
    pub credits: i32,
    /// What each attribute's innate value is raised by, in [`ATTRIBUTES`] order.
    pub innate: [u32; 6],
    /// The skills an augmentation specializes, which the build must train.
    pub specialize: Vec<u32>,
    /// Every augmentation and aura held, by its int property, and how many.
    pub held: BTreeMap<u32, i32>,
}

impl Extras {
    /// Whether it adds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether it can earn luminance, as it may at `level`.
    #[must_use]
    pub fn luminous(&self, level: u32) -> bool {
        self.luminance.is_some() && level >= table().luminance.from_level
    }

    /// Every quest it has solved, and how many times: those named, those that grant skill credits
    /// when it does them (as far as `level` allows), and those its augmentations are held as.
    #[must_use]
    pub fn solved(&self, level: u32) -> BTreeMap<String, u32> {
        let mut out = self.quests.clone();
        if self.quest_credits {
            for q in &table().quest_credit {
                if level >= q.from_level && (!q.luminance || self.luminous(level)) {
                    out.insert(q.quest.clone(), q.solves);
                }
            }
        }
        for (id, n) in &self.augmentations {
            if let Some(quest) = table().augmentation(id).and_then(|a| a.quest.clone()) {
                if *n > 0 {
                    out.insert(quest, 1);
                }
            }
        }
        out
    }

    /// The skill credits its solved quests grant, as the server's skill credit check counts
    /// them: a credit for each solve of a quest that grants one.
    #[must_use]
    pub fn quest_credits(&self, level: u32) -> i32 {
        credits_of(&self.solved(level))
    }

    /// How it changes the build of a character of `level` and `heritage`, in a world whose gems
    /// cost `costs`.
    ///
    /// # Errors
    /// An augmentation or aura it names that there is none of, or one the world has no gem for.
    pub fn adjust(
        &self,
        t: &Tables<'_>,
        level: u32,
        heritage: u32,
        costs: &Costs,
    ) -> Result<Adjust, String> {
        let table = table();
        let mut out = Adjust {
            credits: self.quest_credits(level),
            ..Adjust::default()
        };
        if let Some(free) = heritage_augmentation(heritage) {
            out.held.insert(free, 1);
        }
        for (id, n) in self.augmentations.iter().filter(|(_, n)| **n > 0) {
            let a = table
                .augmentation(id)
                .ok_or_else(|| format!("There is no augmentation {id:?}."))?;
            let cost = costs
                .get(id)
                .ok_or_else(|| format!("This world has no gem for {}.", a.name))?;
            out.reserve = out
                .reserve
                .saturating_add(cost.saturating_mul(u64::from(*n)));
            if let Some(p) = a.property {
                *out.held.entry(p).or_default() += i32::try_from(*n).unwrap_or(i32::MAX);
            }
            if let Some(i) = a
                .attribute
                .and_then(|attr| ATTRIBUTES.iter().position(|x| *x == attr))
            {
                out.innate[i] += 5 * n;
            }
            if let Some(skill) = a.skill.as_deref().and_then(|s| t.skill_named(s)) {
                out.specialize.push(skill);
            }
        }
        if let Some(l) = self.luminance.as_ref().filter(|_| self.luminous(level)) {
            for (id, n) in l.auras.iter().filter(|(_, n)| **n > 0) {
                let a = table
                    .aura(id)
                    .ok_or_else(|| format!("There is no aura {id:?}."))?;
                out.held
                    .insert(a.property, i32::try_from(*n).unwrap_or(i32::MAX));
            }
        }
        Ok(out)
    }

    /// What is wrong with it for a character of `level` and `heritage`, or unusual: an
    /// augmentation the world has no gem for or past its limit, more experience than the level
    /// has, an innate value past 100, auras before luminance can be earned or past what Nalicana
    /// and its Seer sell, a quest solved more often than it can be; and past the band, or a
    /// summoning mastery before its level, warned of.
    /// With the build `b`, a skill an augmentation specializes that the build does not train.
    #[must_use]
    pub fn findings(
        &self,
        t: &Tables<'_>,
        level: u32,
        heritage: u32,
        innate: &[u32; 6],
        costs: &Costs,
        b: Option<&Build>,
    ) -> Vec<Finding> {
        let table = table();
        let mut out = Vec::new();
        let mut say = |field: Field, text: String, warning: bool| {
            out.push(Finding {
                field,
                text,
                warning,
            });
        };
        let free = heritage_augmentation(heritage);
        let mut bought = 0;
        let mut cost = 0_u64;
        let mut families: BTreeMap<&str, u32> = BTreeMap::new();
        for (id, n) in self.augmentations.iter().filter(|(_, n)| **n > 0) {
            let Some(a) = table.augmentation(id) else {
                say(
                    Field::Augmentation,
                    format!("There is no augmentation {id:?}."),
                    false,
                );
                continue;
            };
            bought += n;
            let held = n + u32::from(a.property.is_some() && a.property == free);
            if held > a.max {
                say(
                    Field::Augmentation,
                    format!("{} can be held at most {} times.", a.name, a.max),
                    false,
                );
            }
            match costs.get(id) {
                Some(c) => cost = cost.saturating_add(c.saturating_mul(u64::from(*n))),
                None => say(
                    Field::Augmentation,
                    format!("This world has no gem for {}.", a.name),
                    false,
                ),
            }
            match a.family {
                Some(Family::Innate) => *families.entry("innate").or_default() += n,
                Some(Family::Resistance) => *families.entry("resistance").or_default() += n,
                None => {}
            }
            if let Some(i) = a
                .attribute
                .and_then(|attr| ATTRIBUTES.iter().position(|x| *x == attr))
            {
                if innate[i] + 5 * n > 100 {
                    say(
                        Field::Augmentation,
                        format!(
                            "{} would raise {} past an innate 100.",
                            a.name,
                            crate::ATTRIBUTE_NAMES[i]
                        ),
                        false,
                    );
                }
            }
            if let (Some(b), Some(skill)) = (b, a.skill.as_deref()) {
                let trained = t
                    .skill_named(skill)
                    .and_then(|id| b.skills.iter().find(|s| s.id == id))
                    .is_some_and(|s| s.sac >= Sac::Trained);
                if !trained {
                    say(
                        Field::Augmentation,
                        format!(
                            "{} needs {skill} trained, and its credits do not cover it.",
                            a.name
                        ),
                        false,
                    );
                }
            }
        }
        for (family, n) in &families {
            let most = if *family == "innate" {
                table.family.innate
            } else {
                table.family.resistance
            };
            if *n > most {
                say(
                    Field::Augmentation,
                    format!("At most {most} {family} augmentations can be held together."),
                    false,
                );
            }
        }
        let experience =
            t.xp.level_xp
                .get(usize::try_from(level).unwrap_or(usize::MAX))
                .copied()
                .unwrap_or(0);
        if cost > experience {
            say(
                Field::Augmentation,
                format!("The augmentations cost more experience than level {level} has."),
                false,
            );
        }
        let band = table.band(level);
        if let Some(band) = band.filter(|b| bought > b.warn_past) {
            say(
                Field::Augmentation,
                format!(
                    "{bought} augmentations is more than characters of level {level} had: most had {}.",
                    band.typical
                ),
                true,
            );
        }
        if let Some(l) = &self.luminance {
            let auras: u32 = l.auras.values().sum();
            if level < table.luminance.from_level {
                say(
                    Field::Augmentation,
                    format!(
                        "Luminance can be earned from level {}.",
                        table.luminance.from_level
                    ),
                    false,
                );
            } else {
                let seer = match l.seer.as_deref() {
                    Some(id) => {
                        let s = table.seer(id);
                        if s.is_none() {
                            say(
                                Field::Augmentation,
                                format!("There is no Seer {id:?}."),
                                false,
                            );
                        }
                        s
                    }
                    None => None,
                };
                for (id, n) in l.auras.iter().filter(|(_, n)| **n > 0) {
                    let Some(a) = table.aura(id) else {
                        say(
                            Field::Augmentation,
                            format!("There is no aura {id:?}."),
                            false,
                        );
                        continue;
                    };
                    let most = table.aura_ranks(a, seer);
                    if *n > most {
                        say(
                            Field::Augmentation,
                            if most == 0 {
                                format!("Only a Seer sells {}.", a.name)
                            } else {
                                format!("{} can be raised at most {most} times.", a.name)
                            },
                            false,
                        );
                    }
                }
                if let Some(past) = band
                    .and_then(|b| b.luminance_warn_past)
                    .filter(|p| auras > *p)
                {
                    say(
                        Field::Augmentation,
                        format!(
                            "{auras} ranks of aura is more than characters of level {level} had: most had at most {past}."
                        ),
                        true,
                    );
                }
            }
        }
        if let Some(m) = self
            .summoning_mastery
            .filter(|_| level < MASTERY_FROM_LEVEL)
        {
            say(
                Field::Augmentation,
                format!(
                    "A summoning mastery is chosen from level {MASTERY_FROM_LEVEL}: {} at {level} is unusual.",
                    m.name()
                ),
                true,
            );
        }
        for (quest, n) in &self.quests {
            if *n == 0 {
                say(
                    Field::Quests,
                    format!("Quest {quest} is solved no times."),
                    false,
                );
            }
            if let Some(q) = table.quest_credit(quest) {
                if level < q.from_level {
                    say(
                        Field::Quests,
                        format!("{} can be had from level {}.", q.name, q.from_level),
                        false,
                    );
                } else if *n > q.solves {
                    say(
                        Field::Quests,
                        format!("{} can be had at most {} times.", q.name, q.solves),
                        false,
                    );
                }
                if q.luminance && !self.luminous(level) {
                    say(
                        Field::Quests,
                        format!("{} is bought with luminance.", q.name),
                        false,
                    );
                }
            }
        }
        out
    }
}

/// The build of `plan` at `level` for `heritage` with `extras`, in a world whose gems cost `costs`,
/// and what the extras' own checks find of it.
///
/// # Errors
/// A build that cannot be made: an augmentation or aura there is none of, one the world has no gem
/// for, or what [`crate::build_with`] refuses.
pub fn build(
    t: &Tables<'_>,
    level: u32,
    heritage: u32,
    plan: &crate::Plan,
    extras: &Extras,
    costs: &Costs,
) -> Result<(Build, Vec<Finding>), String> {
    let adjust = extras.adjust(t, level, heritage, costs)?;
    let b = crate::build_with(t, level, heritage, plan, &adjust)?;
    let found = extras.findings(t, level, heritage, &plan.innate, costs, Some(&b));
    Ok((b, found))
}

/// The skill credits quests solved as `solved` holds them grant: a credit for each solve of a
/// quest that grants one, up to its most.
#[must_use]
pub fn credits_of(solved: &BTreeMap<String, u32>) -> i32 {
    solved
        .iter()
        .filter_map(|(name, n)| {
            let q = table().quest_credit(name)?;
            i32::try_from((*n).min(q.solves)).ok()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character builder's augmentations and quests)
    use super::*;

    #[test]
    fn quest_credits_count_each_solve_from_the_level_a_character_can_do_them() {
        let e = Extras {
            quest_credits: true,
            ..Extras::default()
        };
        assert_eq!(e.quest_credits(50), 0);
        assert_eq!(e.quest_credits(85), 1, "Ralirea's figurine from 80");
        assert_eq!(e.quest_credits(150), 2, "and Oswald's manual from 90");
        assert_eq!(
            e.quest_credits(250),
            2,
            "Nalicana's are bought with luminance"
        );
        let luminous = Extras {
            luminance: Some(Luminous::default()),
            ..e.clone()
        };
        assert_eq!(luminous.quest_credits(250), 4);
        assert_eq!(luminous.quest_credits(150), 2, "no luminance before 200");
        let named = Extras {
            quests: BTreeMap::from([("oswaldmanualcompleted".to_owned(), 1)]),
            ..Extras::default()
        };
        assert_eq!(named.quest_credits(100), 1, "named by hand, case aside");
    }

    #[test]
    fn a_summoning_mastery_is_written_by_name_and_left_out_when_there_is_none() {
        let e = Extras {
            summoning_mastery: Some(Mastery::Necromancer),
            ..Extras::default()
        };
        let text = serde_json::to_string(&e).unwrap();
        assert_eq!(text, r#"{"summoning_mastery":"necromancer"}"#);
        assert_eq!(serde_json::from_str::<Extras>(&text).unwrap(), e);
        assert_eq!(serde_json::to_string(&Extras::default()).unwrap(), "{}");
        assert!(!e.is_empty());
        assert_eq!(
            Mastery::ALL.map(Mastery::number),
            [1, 2, 3],
            "Primalist, Necromancer, Naturalist"
        );
    }

    #[test]
    fn a_bought_augmentation_held_as_a_quest_is_solved_with_the_rest() {
        let e = Extras {
            augmentations: BTreeMap::from([("asherons-lesser-benediction".to_owned(), 1)]),
            ..Extras::default()
        };
        assert_eq!(
            e.solved(200),
            BTreeMap::from([("LesserBenedictionAug".to_owned(), 1)])
        );
    }

    #[test]
    fn nothing_added_writes_nothing() {
        assert!(Extras::default().is_empty());
        let text = serde_json::to_string(&Extras::default()).unwrap();
        assert_eq!(text, "{}");
    }
}
