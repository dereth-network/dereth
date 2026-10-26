//! Archetypes: the kinds of player a build can be, as data.
//!
//! An archetype is what a player chose when they built a character: the innate split at
//! creation, the skills they specialized and trained, in the order they wanted them, and how they
//! shared their experience out. It names skills, never ranks or values. A skill is named by the
//! data files' name, spaces and case aside, with alternatives for the eras (`HeavyWeapons|Sword`:
//! Heavy Weapons where the files have it, else Sword); the first the files have is taken.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Sac, Tables, ATTRIBUTE_NAMES, VITAL_NAMES};

/// What a player chose: everything a build is made from but the level and the heritage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The innate split: Strength, Endurance, Coordination, Quickness, Focus, Self.
    pub innate: [u32; 6],
    /// The skills to specialize, most wanted first.
    #[serde(default)]
    pub specialized: Vec<String>,
    /// The skills to train, most wanted first.
    #[serde(default)]
    pub trained: Vec<String>,
    /// How the experience is shared: an attribute, a vital or a skill by name, and its share.
    /// An attribute or vital not named gets none.
    #[serde(default)]
    pub weights: BTreeMap<String, u32>,
    /// The share of a trained skill the weights do not name.
    #[serde(default = "one")]
    pub other: u32,
}

const fn one() -> u32 {
    1
}

/// One built-in kind of player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Archetype {
    /// What a recipe names it by.
    pub id: String,
    /// What a builder reads.
    pub name: String,
    #[serde(flatten)]
    pub plan: Plan,
}

#[derive(Deserialize)]
struct File {
    archetype: Vec<Archetype>,
}

/// The built-in archetypes, in the order they are offered.
#[must_use]
pub fn builtin() -> Vec<Archetype> {
    all().to_vec()
}

/// The built-in archetypes, read from the table once.
///
/// # Panics
/// Never: the table is part of the crate, and a test reads it.
fn all() -> &'static [Archetype] {
    static ALL: std::sync::OnceLock<Vec<Archetype>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        toml::from_str::<File>(include_str!("archetypes.toml"))
            .expect("the built-in archetypes")
            .archetype
    })
}

impl Archetype {
    /// The built-in archetype `id`.
    #[must_use]
    pub fn named(id: &str) -> Option<Self> {
        all().iter().find(|a| a.id == id).cloned()
    }

    /// Whether a world with these tables offers it: the files have every skill it specializes.
    #[must_use]
    pub fn offered(&self, t: &Tables<'_>) -> bool {
        !self.plan.specialized.is_empty()
            && self
                .plan
                .specialized
                .iter()
                .all(|s| t.resolve_skill(s).is_some())
    }

    /// The skill it is built around, when the files have it.
    #[must_use]
    pub fn main_skill(&self, t: &Tables<'_>) -> Option<u32> {
        t.resolve_skill(self.plan.specialized.first()?)
    }
}

impl Plan {
    /// What the plan asks for skill `id`: specialized, trained, or nothing (untrained).
    #[must_use]
    pub fn wants(&self, t: &Tables<'_>, id: u32) -> Sac {
        let named = |list: &[String]| list.iter().any(|s| t.resolve_skill(s) == Some(id));
        if named(&self.specialized) {
            Sac::Specialized
        } else if named(&self.trained) {
            Sac::Trained
        } else {
            Sac::Untrained
        }
    }

    /// Ask for skill `id` as `sac`, after the skills already asked for so: specialized or
    /// trained, last in its list; untrained, in neither list.
    pub fn choose(&mut self, t: &Tables<'_>, id: u32, sac: Sac) {
        self.specialized.retain(|s| t.resolve_skill(s) != Some(id));
        self.trained.retain(|s| t.resolve_skill(s) != Some(id));
        let name: String = t.skill_name(id).split_whitespace().collect();
        match sac {
            Sac::Specialized => self.specialized.push(name),
            Sac::Trained => self.trained.push(name),
            Sac::Untrained | Sac::Undef => {}
        }
    }

    /// The share named `name` gets: its weight, else none for an attribute or vital and
    /// [`Plan::other`] for a skill.
    #[must_use]
    pub fn weight_of(&self, t: &Tables<'_>, what: Target) -> u32 {
        let named = self
            .weights
            .iter()
            .find(|(k, _)| target(t, k) == Some(what))
            .map(|(_, w)| *w);
        match what {
            Target::Skill(_) => named.unwrap_or(self.other),
            _ => named.unwrap_or(0),
        }
    }

    /// Give `what` a share of `weight`, under the name the data files give it.
    pub fn set_weight(&mut self, t: &Tables<'_>, what: Target, weight: u32) {
        self.weights.retain(|k, _| target(t, k) != Some(what));
        let name: String = match what {
            Target::Attribute(i) => ATTRIBUTE_NAMES[i].to_owned(),
            Target::Vital(i) => VITAL_NAMES[i].to_owned(),
            Target::Skill(id) => t.skill_name(id).split_whitespace().collect(),
        };
        self.weights.insert(name, weight);
    }
}

/// The built-in archetypes a world with these tables offers.
#[must_use]
pub fn offered(t: &Tables<'_>) -> Vec<Archetype> {
    all().iter().filter(|a| a.offered(t)).cloned().collect()
}

/// What a weight's name stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// An index into [`crate::ATTRIBUTES`].
    Attribute(usize),
    /// An index into [`crate::VITALS`].
    Vital(usize),
    Skill(u32),
}

/// What weight name `name` stands for in a world with these tables.
#[must_use]
pub fn target(t: &Tables<'_>, name: &str) -> Option<Target> {
    let n = crate::normalized(name);
    if let Some(i) = ATTRIBUTE_NAMES
        .iter()
        .position(|a| crate::normalized(a) == n)
    {
        return Some(Target::Attribute(i));
    }
    if let Some(i) = VITAL_NAMES.iter().position(|v| crate::normalized(v) == n) {
        return Some(Target::Vital(i));
    }
    t.resolve_skill(name).map(Target::Skill)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character builder's archetypes)
    use super::*;

    #[test]
    fn the_ten_built_in_archetypes_read_each_a_legal_innate_split_and_a_main_skill() {
        let all = builtin();
        let ids: Vec<&str> = all.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "melee-heavy",
                "melee-two-handed",
                "melee-light",
                "melee-finesse",
                "dual-wield",
                "sword-and-board",
                "archer",
                "war-mage",
                "void-mage",
                "summoner",
            ]
        );
        for a in &all {
            assert_eq!(a.plan.innate.iter().sum::<u32>(), 330, "{}", a.id);
            assert!(a.plan.innate.iter().all(|v| (10..=100).contains(v)));
            assert!(!a.plan.specialized.is_empty());
            assert!(a.plan.weights.values().any(|w| *w > 0));
        }
        assert_eq!(Archetype::named("war-mage").unwrap().name, "War Mage");
    }

    #[test]
    fn every_built_in_archetype_shares_its_ranks_by_its_main_skill_and_raises_no_salvaging() {
        for a in builtin() {
            let main = &a.plan.specialized[0];
            let top = a.plan.weights.values().max().copied();
            assert_eq!(a.plan.weights.get(main).copied(), top, "{}", a.id);
            assert_eq!(top, Some(100), "{}", a.id);
            assert!(!a.plan.weights.contains_key("Salvaging"), "{}", a.id);
            assert_eq!(a.plan.other, 0, "{}", a.id);
        }
    }
}
