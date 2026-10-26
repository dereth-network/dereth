//! A level's budget: the experience it takes to reach it and the skill credits a character of it
//! has, both from the data files, and what training a skill costs a heritage.

use crate::Tables;
use dereth_rules::chargen::{default_skill_class, skill_costs, SkillAdvancementClass};
use dereth_rules::skills::Sac;

/// What a character of one level and heritage has to spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub level: u32,
    /// The experience the level takes, all of it earned.
    pub experience: u64,
    /// The heritage's credits at creation.
    pub creation_credits: i32,
    /// The credits the levels up to this one granted.
    pub level_credits: i32,
    /// The innate attribute points the heritage splits at creation.
    pub attribute_points: u32,
}

impl Budget {
    /// The budget of `level` for `heritage`.
    ///
    /// # Errors
    /// A level the experience table does not reach, or a heritage the creation table lacks.
    pub fn of(t: &Tables<'_>, level: u32, heritage: u32) -> Result<Self, String> {
        let max = t.max_level();
        if !(1..=max).contains(&level) {
            return Err(format!("The level must be from 1 to {max}."));
        }
        let hg = t
            .chargen
            .heritage_groups
            .get(&heritage)
            .ok_or_else(|| format!("The data files have no heritage {heritage}."))?;
        let index = usize::try_from(level).unwrap_or(usize::MAX);
        let level_credits =
            t.xp.level_credits
                .iter()
                .take(index.saturating_add(1))
                .map(|c| i32::try_from(*c).unwrap_or(0))
                .sum();
        Ok(Self {
            level,
            experience: t.xp.level_xp.get(index).copied().unwrap_or(0),
            creation_credits: i32::try_from(hg.skill_credits).unwrap_or(0),
            level_credits,
            attribute_points: hg.attribute_credits,
        })
    }

    /// Every credit the character has: its creation's and its levels'.
    #[must_use]
    pub const fn credits(&self) -> i32 {
        self.creation_credits + self.level_credits
    }
}

/// What training and specializing skill `id` costs `heritage`: the heritage's own costs where it
/// has them, else the skill table's. The specialized cost is the whole cost, not a step from
/// trained.
#[must_use]
pub fn costs(t: &Tables<'_>, heritage: u32, id: u32) -> (i32, i32) {
    skill_costs(t.chargen, t.skills, heritage, id)
}

/// The class a new character of `heritage` has skill `id` at: trained (or specialized) when it
/// costs nothing, else untrained; `None` for a skill the data files lack or cannot train.
#[must_use]
pub fn starting_class(t: &Tables<'_>, heritage: u32, id: u32) -> Option<Sac> {
    if !t.has_skill(id) {
        return None;
    }
    match default_skill_class(t.chargen, t.skills, heritage, id) {
        SkillAdvancementClass::Inactive => None,
        SkillAdvancementClass::Untrained => Some(Sac::Untrained),
        SkillAdvancementClass::Trained => Some(Sac::Trained),
        SkillAdvancementClass::Specialized => Some(Sac::Specialized),
    }
}

/// The credits a skill of class `sac` has cost: its trained cost when trained, its whole
/// specialized cost when specialized, nothing otherwise.
#[must_use]
pub fn spent_on(t: &Tables<'_>, heritage: u32, id: u32, sac: Sac) -> i32 {
    let (trained, specialized) = costs(t, heritage, id);
    match sac {
        Sac::Trained => trained.max(0),
        Sac::Specialized => specialized.max(0),
        _ => 0,
    }
}

/// The experience table a skill of class `sac` raises along; `None` when it cannot be raised.
#[must_use]
pub fn skill_curve<'a>(t: &Tables<'a>, sac: Sac) -> Option<&'a [u32]> {
    match sac {
        Sac::Trained => Some(&t.xp.trained_xp),
        Sac::Specialized => Some(&t.xp.specialized_xp),
        _ => None,
    }
}

/// The highest rank whose whole cost along `curve` is at or below `xp`.
#[must_use]
pub fn rank_of(curve: &[u32], xp: u32) -> u32 {
    let n = curve.iter().take_while(|c| **c <= xp).count();
    u32::try_from(n.saturating_sub(1)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character builder's arithmetic)
    use super::*;

    #[test]
    fn a_rank_is_the_highest_whose_whole_cost_the_experience_covers() {
        let curve = [0, 10, 30, 60];
        assert_eq!(rank_of(&curve, 0), 0);
        assert_eq!(rank_of(&curve, 29), 1);
        assert_eq!(rank_of(&curve, 30), 2);
        assert_eq!(rank_of(&curve, 1000), 3);
    }
}
