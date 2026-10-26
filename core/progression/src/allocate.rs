//! How a plan becomes a build: the credits spent down the plan's skill lists, then the
//! experience spent in steps.
//!
//! **Experience is spent a raise at a time, never by maxing one thing first.** Each step buys the
//! raise that keeps every target's ranks closest to its share: the target whose ranks, with the
//! raise, are the fewest for its weight, among the targets still under their cap whose raise the
//! remaining experience covers. So a target of weight 70 gains 70 ranks for every 100 a target of
//! weight 100 gains, as players' ranks rose together. It stops when no raise fits; what is left
//! once the dearest raises no longer fit goes on the cheaper ones.
//!
//! **Creation's free ranks.** Character creation gives each skill it trains (not one it
//! specializes) [`CREATION_RANKS`] ranks for nothing: the trained curve's cost of them is held as
//! the skill's experience without being earned. A build trains at creation what a player would:
//! the skills that cost nothing, then its choices in its plan's order while the heritage's
//! creation credits pay for them; a skill chosen later, with a level's credits, starts at none.

use std::collections::BTreeMap;

use crate::archetypes::{Plan, Target};
use crate::budget::{self, Budget};
use crate::check::{Sheet, SheetSkill};
use crate::extras::Adjust;
use crate::CREATION_RANKS;
use crate::{heritage_augmentation, Sac, Tables, ATTRIBUTES, AUGMENTATION_ONLY_COST};
use crate::{SPECIALIZED_CREDIT_LIMIT, VITALS};

/// An attribute or vital, raised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Raised {
    /// Its property id.
    pub id: u32,
    /// The innate value an attribute was created with; 0 for a vital.
    pub innate: u32,
    pub ranks: u32,
    /// The experience its ranks cost, in all.
    pub xp: u32,
}

/// A skill of the build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillBuild {
    pub id: u32,
    pub name: String,
    pub sac: Sac,
    /// The level it starts at: 10 when specialized, else 0.
    pub start: u32,
    pub ranks: u32,
    /// The experience its ranks cost, in all, creation's free ranks among them.
    pub xp: u32,
    /// Trained at creation, with creation's free ranks.
    pub creation: bool,
}

/// A character of a level, as a player with that plan would have it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    pub budget: Budget,
    pub heritage: u32,
    /// The experience left unspent: what no raise fitted.
    pub unspent_xp: u64,
    /// The credits left unspent.
    pub unspent_credits: i32,
    /// In [`ATTRIBUTES`] order.
    pub attributes: [Raised; 6],
    /// In [`VITALS`] order.
    pub vitals: [Raised; 3],
    /// Every skill the files have, by id.
    pub skills: Vec<SkillBuild>,
    /// The heritage's free augmentation, held once.
    pub heritage_augmentation: Option<u32>,
    /// What creation's free ranks are in experience: held by its skills, never earned.
    pub creation_xp: u64,
    /// The experience its augmentations cost.
    pub augmentation_xp: u64,
    /// The skill credits its quests granted, among its credits.
    pub quest_credits: i32,
    /// Every augmentation and aura it holds, by its int property, and how many.
    pub held: BTreeMap<u32, i32>,
}

/// One thing experience is spent on, during the allocation.
struct Spend<'a> {
    what: Target,
    curve: &'a [u32],
    weight: u32,
    rank: u32,
}

impl Spend<'_> {
    fn spent(&self) -> u32 {
        self.curve
            .get(usize::try_from(self.rank).unwrap_or(usize::MAX))
            .copied()
            .unwrap_or(0)
    }

    /// The cost of the next raise, when there is one.
    fn step(&self) -> Option<u32> {
        let next = self.curve.get(
            usize::try_from(self.rank)
                .unwrap_or(usize::MAX)
                .checked_add(1)?,
        )?;
        next.checked_sub(self.spent())
    }
}

/// The build of `plan` at `level` for `heritage`.
///
/// # Errors
/// A level the tables do not reach, a heritage they lack, or an innate split that is not a legal
/// creation.
pub fn build(t: &Tables<'_>, level: u32, heritage: u32, plan: &Plan) -> Result<Build, String> {
    build_with(t, level, heritage, plan, &Adjust::default())
}

/// [`build`], changed as the player's extras change it (`adjust`): the experience its
/// augmentations cost set aside, the credits its quests granted spent with the rest, its innate
/// values raised, and the skills an augmentation specializes trained, then specialized.
///
/// # Errors
/// As [`build`], or augmentations that cost more experience than the level has.
pub fn build_with(
    t: &Tables<'_>,
    level: u32,
    heritage: u32,
    plan: &Plan,
    adjust: &Adjust,
) -> Result<Build, String> {
    let budget = Budget::of(t, level, heritage)?;
    let total: u32 = plan.innate.iter().sum();
    if plan.innate.iter().any(|v| !(10..=100).contains(v)) || total != budget.attribute_points {
        return Err(format!(
            "The innate attributes must each be from 10 to 100 and come to {}.",
            budget.attribute_points
        ));
    }

    // Skills: what a new character of the heritage has, then the plan's choices, in order.
    let mut skills: Vec<SkillBuild> = t
        .skills
        .skills
        .keys()
        .filter_map(|id| {
            budget::starting_class(t, heritage, *id).map(|sac| SkillBuild {
                id: *id,
                name: t.skill_name(*id),
                sac,
                start: 0,
                ranks: 0,
                xp: 0,
                creation: false,
            })
        })
        .collect();
    let used = |skills: &[SkillBuild]| -> i32 {
        skills
            .iter()
            .map(|s| budget::spent_on(t, heritage, s.id, s.sac))
            .sum()
    };
    let specialized = |skills: &[SkillBuild]| -> i32 {
        skills
            .iter()
            .filter(|s| s.sac == Sac::Specialized)
            .map(|s| budget::spent_on(t, heritage, s.id, s.sac))
            .sum()
    };
    let credits = budget.credits() + adjust.credits;
    // The skills an augmentation specializes are trained after the plan's own.
    let specialize: Vec<(u32, Sac)> = adjust
        .specialize
        .iter()
        .map(|id| (*id, Sac::Trained))
        .collect();
    for (id, want) in plan
        .specialized
        .iter()
        .map(|s| (s, Sac::Specialized))
        .chain(plan.trained.iter().map(|s| (s, Sac::Trained)))
        .filter_map(|(token, want)| Some((t.resolve_skill(token)?, want)))
        .chain(specialize)
    {
        let Some(at) = skills.iter().position(|s| s.id == id) else {
            continue;
        };
        let (_, specialized_cost) = budget::costs(t, heritage, id);
        let mut tries = Vec::new();
        if want == Sac::Specialized && specialized_cost < AUGMENTATION_ONLY_COST {
            tries.push(Sac::Specialized);
        }
        tries.push(Sac::Trained);
        for sac in tries {
            if skills[at].sac >= sac {
                break;
            }
            let was = skills[at].sac;
            skills[at].sac = sac;
            if used(&skills) <= credits && specialized(&skills) <= SPECIALIZED_CREDIT_LIMIT {
                break;
            }
            skills[at].sac = was;
        }
    }
    let unspent_credits = credits - used(&skills);
    // An augmentation specializes a skill trained, at no more credits.
    for s in &mut skills {
        if s.sac == Sac::Trained && adjust.specialize.contains(&s.id) {
            s.sac = Sac::Specialized;
        }
        if s.sac == Sac::Specialized {
            s.start = 10;
        }
    }

    // What creation trained: the free skills, then the plan's choices in its order while the
    // creation credits pay for them, each at the class it ends at.
    let mut at_creation = 0;
    let mut specialized_at_creation = 0;
    let free = |id: u32| budget::starting_class(t, heritage, id).is_some_and(|c| c >= Sac::Trained);
    let chosen = plan
        .specialized
        .iter()
        .chain(&plan.trained)
        .filter_map(|token| t.resolve_skill(token));
    let mut created: Vec<u32> = Vec::new();
    for id in skills
        .iter()
        .map(|s| s.id)
        .filter(|id| free(*id))
        .chain(chosen)
    {
        let Some(s) = skills.iter().find(|s| s.id == id) else {
            continue;
        };
        if created.contains(&id) || s.sac < Sac::Trained {
            continue;
        }
        let cost = budget::spent_on(t, heritage, id, s.sac);
        let spec = if s.sac == Sac::Specialized { cost } else { 0 };
        if at_creation + cost > budget.creation_credits
            || specialized_at_creation + spec > SPECIALIZED_CREDIT_LIMIT
        {
            continue;
        }
        at_creation += cost;
        specialized_at_creation += spec;
        created.push(id);
    }
    let trained_curve = &t.xp.trained_xp;
    let free_xp = trained_curve
        .get(usize::try_from(CREATION_RANKS).unwrap_or(usize::MAX))
        .copied()
        .unwrap_or(0);
    let mut creation_xp = 0_u64;
    for s in &mut skills {
        if s.sac == Sac::Trained && created.contains(&s.id) {
            s.creation = true;
            s.ranks = CREATION_RANKS;
            s.xp = free_xp;
            creation_xp += u64::from(free_xp);
        }
    }

    // Experience, in steps.
    let weight_of = |what: Target| plan.weight_of(t, what);
    let mut spends: Vec<Spend<'_>> = Vec::new();
    for i in 0..ATTRIBUTES.len() {
        spends.push(Spend {
            what: Target::Attribute(i),
            curve: &t.xp.attribute_xp,
            weight: weight_of(Target::Attribute(i)),
            rank: 0,
        });
    }
    for i in 0..VITALS.len() {
        spends.push(Spend {
            what: Target::Vital(i),
            curve: &t.xp.vital_xp,
            weight: weight_of(Target::Vital(i)),
            rank: 0,
        });
    }
    for s in &skills {
        if let Some(curve) = budget::skill_curve(t, s.sac) {
            spends.push(Spend {
                what: Target::Skill(s.id),
                curve,
                weight: weight_of(Target::Skill(s.id)),
                rank: s.ranks,
            });
        }
    }
    spends.retain(|s| s.weight > 0);
    let mut left = budget
        .experience
        .checked_sub(adjust.reserve)
        .ok_or_else(|| format!("The augmentations cost more experience than level {level} has."))?;
    loop {
        let mut best: Option<(usize, u64)> = None;
        for (i, s) in spends.iter().enumerate() {
            let Some(step) = s.step() else { continue };
            if u64::from(step) > left {
                continue;
            }
            let after = u64::from(s.rank) + 1;
            // The fewest ranks for its weight: after / weight, compared without dividing.
            let better = best.is_none_or(|(b, b_after)| {
                u128::from(after) * u128::from(spends[b].weight)
                    < u128::from(b_after) * u128::from(s.weight)
            });
            if better {
                best = Some((i, after));
            }
        }
        let Some((i, _)) = best else { break };
        let step = spends[i].step().unwrap_or(0);
        spends[i].rank += 1;
        left -= u64::from(step);
    }

    let mut attributes = [Raised {
        id: 0,
        innate: 0,
        ranks: 0,
        xp: 0,
    }; 6];
    for (i, a) in attributes.iter_mut().enumerate() {
        *a = Raised {
            id: ATTRIBUTES[i],
            innate: plan.innate[i] + adjust.innate[i],
            ranks: 0,
            xp: 0,
        };
    }
    let mut vitals = [Raised {
        id: 0,
        innate: 0,
        ranks: 0,
        xp: 0,
    }; 3];
    for (i, v) in vitals.iter_mut().enumerate() {
        v.id = VITALS[i];
    }
    for s in &spends {
        match s.what {
            Target::Attribute(i) => {
                attributes[i].ranks = s.rank;
                attributes[i].xp = s.spent();
            }
            Target::Vital(i) => {
                vitals[i].ranks = s.rank;
                vitals[i].xp = s.spent();
            }
            Target::Skill(id) => {
                if let Some(k) = skills.iter_mut().find(|k| k.id == id) {
                    k.ranks = s.rank;
                    k.xp = s.spent();
                }
            }
        }
    }
    Ok(Build {
        budget,
        heritage,
        unspent_xp: left,
        unspent_credits,
        attributes,
        vitals,
        skills,
        heritage_augmentation: heritage_augmentation(heritage),
        creation_xp,
        augmentation_xp: adjust.reserve,
        quest_credits: adjust.credits,
        held: {
            let mut held = adjust.held.clone();
            if let Some(free) = heritage_augmentation(heritage) {
                held.entry(free).or_insert(1);
            }
            held
        },
    })
}

impl Build {
    /// The build a character's stored numbers `s` are: what its base values are worked out from,
    /// as they are for a build this crate made. An attribute or vital the sheet lacks has none.
    ///
    /// # Errors
    /// A level the experience table does not reach, or a heritage the creation table lacks.
    pub fn of_sheet(t: &Tables<'_>, s: &Sheet) -> Result<Self, String> {
        let budget = Budget::of(t, s.level, s.heritage)?;
        let none = Raised {
            id: 0,
            innate: 0,
            ranks: 0,
            xp: 0,
        };
        let mut attributes = [none; 6];
        for (a, id) in attributes.iter_mut().zip(ATTRIBUTES) {
            let (innate, ranks, xp) = s
                .attributes
                .iter()
                .find(|x| x.0 == id)
                .map_or((0, 0, 0), |x| (x.1, x.2, x.3));
            *a = Raised {
                id,
                innate,
                ranks,
                xp,
            };
        }
        let mut vitals = [none; 3];
        for (v, id) in vitals.iter_mut().zip(VITALS) {
            let (ranks, xp) = s
                .vitals
                .iter()
                .find(|x| x.0 == id)
                .map_or((0, 0), |x| (x.1, x.2));
            *v = Raised {
                id,
                innate: 0,
                ranks,
                xp,
            };
        }
        let skills = s
            .skills
            .iter()
            .map(|k| SkillBuild {
                id: k.id,
                name: t.skill_name(k.id),
                sac: k.sac,
                start: k.start,
                ranks: k.ranks,
                xp: k.xp,
                creation: false,
            })
            .collect();
        Ok(Self {
            budget,
            heritage: s.heritage,
            unspent_xp: u64::try_from(s.available_xp).unwrap_or(0),
            unspent_credits: s.available_credits,
            attributes,
            vitals,
            skills,
            heritage_augmentation: heritage_augmentation(s.heritage),
            creation_xp: s
                .creation_xp
                .and_then(|x| u64::try_from(x).ok())
                .unwrap_or(0),
            augmentation_xp: u64::try_from(s.augmentation_xp).unwrap_or(0),
            quest_credits: s.quest_credits,
            held: s.augmentations.clone(),
        })
    }

    /// Attribute `id`'s base value: its innate value and its ranks.
    #[must_use]
    pub fn attribute(&self, id: u32) -> u32 {
        self.attributes
            .iter()
            .find(|a| a.id == id)
            .map_or(0, |a| a.innate + a.ranks)
    }

    /// Vital `id`'s maximum before its starting value: its formula over the base attributes and
    /// its ranks.
    #[must_use]
    pub fn vital(&self, t: &Tables<'_>, id: u32) -> u32 {
        let f = match id {
            1 => &t.vitals.health,
            3 => &t.vitals.stamina,
            5 => &t.vitals.mana,
            _ => return 0,
        };
        let ranks = self
            .vitals
            .iter()
            .find(|v| v.id == id)
            .map_or(0, |v| v.ranks);
        crate::formula(f, |a| self.attribute(a)) + ranks
    }

    /// Skill `id`'s base value: its formula over the base attributes (for a skill it can use),
    /// its starting level and its ranks.
    #[must_use]
    pub fn skill(&self, t: &Tables<'_>, id: u32) -> u32 {
        let Some(s) = self.skills.iter().find(|s| s.id == id) else {
            return 0;
        };
        let usable =
            s.sac >= Sac::Trained || t.skills.skills.get(&id).is_some_and(|b| b.min_level == 1);
        let from = t
            .skills
            .skills
            .get(&id)
            .filter(|_| usable)
            .map_or(0, |b| crate::formula(&b.formula, |a| self.attribute(a)));
        from + s.start + s.ranks
    }

    /// The experience held: what the attributes, vitals and skills cost, creation's free ranks
    /// among them.
    #[must_use]
    pub fn spent_xp(&self) -> u64 {
        self.attributes
            .iter()
            .chain(&self.vitals)
            .map(|r| u64::from(r.xp))
            .chain(self.skills.iter().map(|s| u64::from(s.xp)))
            .sum()
    }

    /// The skills trained or specialized, specialized first.
    #[must_use]
    pub fn chosen_skills(&self) -> Vec<&SkillBuild> {
        let mut v: Vec<&SkillBuild> = self
            .skills
            .iter()
            .filter(|s| s.sac >= Sac::Trained)
            .collect();
        v.sort_by_key(|s| std::cmp::Reverse(s.sac));
        v
    }

    /// The build as a character sheet, as the checks read one.
    #[must_use]
    pub fn sheet(&self) -> Sheet {
        let total = i64::try_from(self.budget.experience).unwrap_or(i64::MAX);
        Sheet {
            level: self.budget.level,
            heritage: self.heritage,
            total_xp: total,
            available_xp: i64::try_from(self.unspent_xp).unwrap_or(i64::MAX),
            total_credits: self.budget.credits() + self.quest_credits,
            available_credits: self.unspent_credits,
            quest_credits: self.quest_credits,
            attributes: self
                .attributes
                .iter()
                .map(|a| (a.id, a.innate, a.ranks, a.xp))
                .collect(),
            vitals: self.vitals.iter().map(|v| (v.id, v.ranks, v.xp)).collect(),
            skills: self
                .skills
                .iter()
                .map(|s| SheetSkill {
                    id: s.id,
                    sac: s.sac,
                    start: s.start,
                    ranks: s.ranks,
                    xp: s.xp,
                })
                .collect(),
            augmentation_xp: i64::try_from(self.augmentation_xp).unwrap_or(i64::MAX),
            heritage_augmentation: self
                .heritage_augmentation
                .map(|a| self.held.get(&a).copied().unwrap_or(0)),
            creation_xp: Some(i64::try_from(self.creation_xp).unwrap_or(i64::MAX)),
            augmentations: self.held.clone(),
        }
    }
}
