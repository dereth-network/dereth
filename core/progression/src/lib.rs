//! A believable character of a chosen level: the level's budget from the data files, the
//! archetypes, how the experience and credits are spent, and the checks a legitimate character
//! passes.
//!
//! A builder describes a player (a level, an archetype) and this crate makes the character such a
//! player could have: the level's experience and skill credits (the budget), the skills an
//! archetype trains and specializes within those credits, and the experience spent on its
//! attributes, vitals and skills in steps until nothing more fits. The same recipe and tables
//! always give the same build. The checks then say whether a character's numbers are ones play
//! could have produced: the experience spent adds up to the experience earned, the credits to
//! the heritage's and the level's, every rank matches the experience behind it, and the innate
//! attributes are a legal creation. Beyond the build, a player may have bought augmentations and
//! luminance auras, held titles and solved quests (`extras`): what they cost comes out of the same
//! budget, and the checks hold them to the server's limits.
//!
//! **Depends on** `dereth-primitives`, the decoded tables (`dereth-assets`) and the creation
//! arithmetic of `dereth-rules`. **Used by** Lyceum, to show a build before it is made, and the
//! server, which makes it and checks it before it writes it.
//!
//! **Must never** read a file or reach the wire: tables in, a build and findings out. Each era
//! gets its own numbers from its own tables (the level cap, the credits per level, the skills it
//! has).

#![doc(html_no_source)]

pub mod allocate;
pub mod archetypes;
pub mod augmentations;
pub mod budget;
pub mod check;
pub mod extras;
pub mod gear;
pub mod load;
pub mod order;

pub use allocate::{build, build_with, Build, Raised, SkillBuild};
pub use archetypes::{Archetype, Plan};
pub use budget::Budget;
pub use check::{check, Field, Finding, Sheet, SheetSkill};
pub use extras::Extras;

use dereth_assets::tables::{Attribute2ndTable, CharGen, SkillTable, XpTable};
pub use dereth_rules::skills::Sac;

/// The four tables a build is made from, as the data files hold them.
#[derive(Debug, Clone, Copy)]
pub struct Tables<'a> {
    /// The experience curves, the level curve and the credits each level grants.
    pub xp: &'a XpTable,
    /// Each skill's training costs and its formula.
    pub skills: &'a SkillTable,
    /// The heritages: their attribute points, skill credits and skill cost overrides.
    pub chargen: &'a CharGen,
    /// The three vitals' formulas.
    pub vitals: &'a Attribute2ndTable,
}

/// The six attributes, by their property ids, in the order a character sheet lists them:
/// Strength, Endurance, Coordination, Quickness, Focus, Self.
pub const ATTRIBUTES: [u32; 6] = [1, 2, 4, 3, 5, 6];

/// The attributes' names, in [`ATTRIBUTES`] order.
pub const ATTRIBUTE_NAMES: [&str; 6] = [
    "Strength",
    "Endurance",
    "Coordination",
    "Quickness",
    "Focus",
    "Self",
];

/// The three vitals' maximums, by their property ids: Health, Stamina, Mana.
pub const VITALS: [u32; 3] = [1, 3, 5];

/// The vitals' names, in [`VITALS`] order.
pub const VITAL_NAMES: [&str; 3] = ["Health", "Stamina", "Mana"];

/// The heritages' ids.
pub mod heritage {
    pub const ALUVIAN: u32 = 1;
    pub const GHARUNDIM: u32 = 2;
    pub const SHO: u32 = 3;
    pub const VIAMONTIAN: u32 = 4;
    pub const SHADOWBOUND: u32 = 5;
    pub const GEARKNIGHT: u32 = 6;
    pub const TUMEROK: u32 = 7;
    pub const LUGIAN: u32 = 8;
    pub const EMPYREAN: u32 = 9;
    pub const PENUMBRAEN: u32 = 10;
    pub const UNDEAD: u32 = 11;
    pub const OLTHOI: u32 = 12;
    pub const OLTHOI_ACID: u32 = 13;
}

/// The genders, by the creation table's ids.
pub mod gender {
    pub const MALE: u32 = 1;
    pub const FEMALE: u32 = 2;
}

/// The augmentation every character of heritage `of` holds once, free, by its int property id:
/// Jack of All Trades for the four human heritages, each other heritage its own; none for a
/// heritage that has none (the Olthoi).
#[must_use]
pub const fn heritage_augmentation(of: u32) -> Option<u32> {
    use self::heritage as h;
    match of {
        h::ALUVIAN | h::GHARUNDIM | h::SHO | h::VIAMONTIAN => Some(326),
        h::SHADOWBOUND | h::PENUMBRAEN => Some(298),
        h::GEARKNIGHT => Some(310),
        h::UNDEAD => Some(233),
        h::EMPYREAN => Some(296),
        h::TUMEROK => Some(299),
        h::LUGIAN => Some(230),
        _ => None,
    }
}

/// The ranks character creation gives each skill it trains, for nothing: the trained curve's
/// cost of them (526) is held as the skill's experience without being earned. A skill it
/// specializes gets none.
pub const CREATION_RANKS: u32 = 5;

/// The most credits a character may spend on specialized skills, all its specializations' full
/// costs together.
pub const SPECIALIZED_CREDIT_LIMIT: i32 = 70;

/// A specialized cost at or above this cannot be bought with credits: the skill is specialized
/// only by an augmentation.
pub const AUGMENTATION_ONLY_COST: i32 = 999;

/// A skill's name made comparable: letters and digits only, lower case (`Heavy Weapons` and
/// `HeavyWeapons` are the same skill).
#[must_use]
pub fn normalized(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl Tables<'_> {
    /// The highest level the experience table reaches.
    #[must_use]
    pub fn max_level(&self) -> u32 {
        u32::try_from(self.xp.level_xp.len().saturating_sub(1)).unwrap_or(0)
    }

    /// Whether the data files have skill `id`: a named row of the skill table. Rows without a
    /// name stand for skills the files no longer carry.
    #[must_use]
    pub fn has_skill(&self, id: u32) -> bool {
        self.skills
            .skills
            .get(&id)
            .is_some_and(|s| !s.name.trim().is_empty())
    }

    /// The skill named `name` (spaces and case aside), when the data files have it.
    #[must_use]
    pub fn skill_named(&self, name: &str) -> Option<u32> {
        let want = normalized(name);
        self.skills
            .skills
            .iter()
            .find(|(id, s)| self.has_skill(**id) && normalized(&s.name) == want)
            .map(|(id, _)| *id)
    }

    /// The first of `token`'s alternatives (`HeavyWeapons|Sword`) the data files have.
    #[must_use]
    pub fn resolve_skill(&self, token: &str) -> Option<u32> {
        token.split('|').find_map(|n| self.skill_named(n))
    }

    /// The heritages a playtest character can have, by id and name: every one the files have but
    /// the Olthoi's, whose players are not made by character creation.
    #[must_use]
    pub fn heritages(&self) -> Vec<(u32, String)> {
        self.chargen
            .heritage_groups
            .iter()
            .filter(|(id, _)| !matches!(**id, heritage::OLTHOI | heritage::OLTHOI_ACID))
            .map(|(id, h)| (*id, h.name.clone()))
            .collect()
    }

    /// The heritage named `name` (as [`normalized`] compares names), when a playtest character
    /// can have it.
    #[must_use]
    pub fn heritage_named(&self, name: &str) -> Option<u32> {
        let want = normalized(name);
        self.heritages()
            .into_iter()
            .find(|(_, n)| normalized(n) == want)
            .map(|(id, _)| id)
    }

    /// Skill `id`'s name as the data files write it.
    #[must_use]
    pub fn skill_name(&self, id: u32) -> String {
        self.skills
            .skills
            .get(&id)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }
}

/// ACE's attribute formula: 0 when the formula is off (`x` 0); else the first attribute plus the
/// second (when it names one), divided by `z` and rounded half away from zero unless `z` is 1.
#[must_use]
pub fn formula(f: &dereth_assets::tables::SkillFormula, attribute: impl Fn(u32) -> u32) -> u32 {
    if f.x == 0 || f.z == 0 {
        return 0;
    }
    let mut total = attribute(f.attr1);
    if f.attr2 != 0 {
        total = total.saturating_add(attribute(f.attr2));
    }
    if f.z == 1 {
        return total;
    }
    #[allow(clippy::cast_precision_loss)] // ACE divides in single precision
    let v = (total as f32 / f.z as f32).round();
    u32::try_from(dereth_primitives::num::to_i64_f64(f64::from(v))).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character builder's arithmetic)
    use super::*;
    use dereth_assets::tables::SkillFormula;

    #[test]
    fn skill_names_compare_without_spaces_or_case() {
        assert_eq!(normalized("Heavy Weapons"), normalized("heavyweapons"));
        assert_ne!(normalized("Heavy Weapons"), normalized("Light Weapons"));
    }

    #[test]
    fn a_formula_divides_the_two_attributes_and_rounds_half_away_from_zero() {
        let f = |attr2, z| SkillFormula {
            w: 0,
            x: 1,
            y: 1,
            z,
            attr1: 1,
            attr2,
        };
        let attrs = |id| if id == 1 { 101 } else { 100 };
        assert_eq!(formula(&f(0, 2), attrs), 51, "101 / 2 rounds up");
        assert_eq!(formula(&f(2, 3), attrs), 67, "201 / 3");
        assert_eq!(formula(&f(2, 1), attrs), 201);
        assert_eq!(formula(&SkillFormula { x: 0, ..f(2, 3) }, attrs), 0);
    }

    #[test]
    fn every_heritage_but_the_olthoi_holds_a_free_augmentation_and_the_four_humans_share_one() {
        for h in [
            heritage::ALUVIAN,
            heritage::GHARUNDIM,
            heritage::SHO,
            heritage::VIAMONTIAN,
        ] {
            assert_eq!(heritage_augmentation(h), Some(326));
        }
        for h in 5..=11 {
            assert!(heritage_augmentation(h).is_some_and(|a| a != 326), "{h}");
        }
        assert_eq!(heritage_augmentation(12), None);
    }
}
