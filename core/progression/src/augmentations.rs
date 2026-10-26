//! The augmentations a character can hold, as data: those bought with experience (each one's
//! property, its gem and its limit), the luminance auras and the Seers, the warning band of what
//! end-of-retail characters held by level, and the quests that grant skill credits.
//!
//! An augmentation bought with experience costs what its gem says; the gems are the world's, so
//! the costs come in from outside (see [`Costs`]).

use std::collections::BTreeMap;

use serde::Deserialize;

/// What an augmentation does to a build besides its own property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    /// One more side pack.
    Pack,
    /// A fifth more burden carried at full speed, each.
    Burden,
}

/// A shared limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Innate,
    Resistance,
}

/// One augmentation bought with experience.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Augmentation {
    /// What a recipe names it by.
    pub id: String,
    pub name: String,
    /// Its int property; `None` for one held as a quest stamp.
    #[serde(default)]
    pub property: Option<u32>,
    /// The quest whose stamp holds it, for one that is no property.
    #[serde(default)]
    pub quest: Option<String>,
    /// Its gem weenie's class name and class, whose cost it is.
    #[serde(default)]
    pub gem: Option<String>,
    #[serde(default)]
    pub class: Option<u32>,
    /// A fixed cost, for one bought without a gem.
    #[serde(default)]
    pub cost: Option<u64>,
    /// How many one character holds at most.
    pub max: u32,
    #[serde(default)]
    pub family: Option<Family>,
    /// The attribute (by property id) an innate augmentation raises.
    #[serde(default)]
    pub attribute: Option<u32>,
    /// The skill (by the data files' name) it specializes.
    #[serde(default)]
    pub skill: Option<String>,
    #[serde(default)]
    pub effect: Option<Effect>,
}

/// An aura bought with luminance.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aura {
    pub id: String,
    pub name: String,
    pub property: u32,
    /// The ranks Nalicana sells.
    pub ranks: u32,
}

/// A Seer: who sells more ranks of two auras, to a character who follows them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seer {
    pub id: String,
    pub name: String,
    /// The auras it sells, by property.
    pub auras: Vec<u32>,
}

/// What end-of-retail characters of levels up to `to` held.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Band {
    pub to: u32,
    /// How many augmentations bought with experience they typically held, as a reader reads it.
    pub typical: String,
    /// Past this many it is unusual.
    pub warn_past: u32,
    /// Past this many auras it is unusual; `None` where no aura can be held.
    #[serde(default)]
    pub luminance_warn_past: Option<u32>,
}

/// A quest whose solves grant skill credits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestCredit {
    pub quest: String,
    /// What a builder reads.
    pub name: String,
    pub from_level: u32,
    /// The most solves that count, a credit each.
    pub solves: u32,
    /// Bought with luminance.
    #[serde(default)]
    pub luminance: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Families {
    pub innate: u32,
    pub resistance: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Luminance {
    /// The level a character can first earn luminance at.
    pub from_level: u32,
    /// The most luminance a character who can earn it holds.
    pub maximum: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Seers {
    /// The ranks a Seer sells of each of its auras.
    pub ranks: u32,
}

/// The whole table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub family: Families,
    pub augmentation: Vec<Augmentation>,
    pub luminance: Luminance,
    pub aura: Vec<Aura>,
    pub seer: Vec<Seer>,
    pub seers: Seers,
    pub band: Vec<Band>,
    pub quest_credit: Vec<QuestCredit>,
}

/// The table, as the crate holds it.
///
/// # Panics
/// Never: the table is part of the crate, and a test reads it.
#[must_use]
pub fn table() -> &'static Table {
    static TABLE: std::sync::OnceLock<Table> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        toml::from_str(include_str!("augmentations.toml")).expect("the augmentations table")
    })
}

impl Table {
    /// The augmentation bought with experience named `id`.
    #[must_use]
    pub fn augmentation(&self, id: &str) -> Option<&Augmentation> {
        self.augmentation.iter().find(|a| a.id == id)
    }

    /// The augmentation held as int property `property`.
    #[must_use]
    pub fn by_property(&self, property: u32) -> Option<&Augmentation> {
        self.augmentation
            .iter()
            .find(|a| a.property == Some(property))
    }

    /// The aura named `id`.
    #[must_use]
    pub fn aura(&self, id: &str) -> Option<&Aura> {
        self.aura.iter().find(|a| a.id == id)
    }

    /// The Seer named `id`.
    #[must_use]
    pub fn seer(&self, id: &str) -> Option<&Seer> {
        self.seer.iter().find(|s| s.id == id)
    }

    /// The most ranks of `aura` a character following `seer` can hold.
    #[must_use]
    pub fn aura_ranks(&self, aura: &Aura, seer: Option<&Seer>) -> u32 {
        let extra = seer
            .filter(|s| s.auras.contains(&aura.property))
            .map_or(0, |_| self.seers.ranks);
        aura.ranks + extra
    }

    /// The band of characters of `level`: the first whose levels reach it, else the last.
    #[must_use]
    pub fn band(&self, level: u32) -> Option<&Band> {
        self.band
            .iter()
            .find(|b| level <= b.to)
            .or_else(|| self.band.last())
    }

    /// The quest credit of quest `name` (as the server compares quest names, case aside).
    #[must_use]
    pub fn quest_credit(&self, name: &str) -> Option<&QuestCredit> {
        self.quest_credit
            .iter()
            .find(|q| q.quest.eq_ignore_ascii_case(name))
    }
}

/// What each augmentation bought with experience costs in a world, by its id: what its gem says,
/// or its fixed cost. One the world has no gem for is missing, and cannot be bought there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Costs(pub BTreeMap<String, u64>);

impl Costs {
    /// The costs where `gem_cost` says what each gem costs, by its class name and class.
    #[must_use]
    pub fn of(gem_cost: impl Fn(&str, u32) -> Option<u64>) -> Self {
        Self(
            table()
                .augmentation
                .iter()
                .filter_map(|a| {
                    let cost = match (&a.gem, a.class, a.cost) {
                        (_, _, Some(c)) => Some(c),
                        (Some(g), Some(c), None) => gem_cost(g, c),
                        _ => None,
                    }?;
                    Some((a.id.clone(), cost))
                })
                .collect(),
        )
    }

    /// What augmentation `id` costs; `None` where it cannot be bought.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<u64> {
        self.0.get(id).copied()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest character builder's augmentation table)
    use super::*;

    #[test]
    fn the_table_reads_with_every_property_once_and_the_limits_of_the_server() {
        let t = table();
        assert_eq!(t.augmentation.len(), 42);
        let mut seen = std::collections::BTreeSet::new();
        for a in &t.augmentation {
            assert!(a.property.is_some() != a.quest.is_some(), "{}", a.id);
            assert!(a.cost.is_some() || (a.gem.is_some() && a.class.is_some()));
            if let Some(p) = a.property {
                assert!(seen.insert(p), "{} twice", a.id);
            }
            assert!(a.max >= 1);
        }
        let max = |id: &str| t.augmentation(id).unwrap().max;
        assert_eq!(
            [
                max("ciandras-fortune"),
                max("might-of-the-seventh-mule"),
                max("innate-renewal"),
                max("archmages-endurance"),
                max("clutch-of-the-miser")
            ],
            [4, 5, 2, 5, 3]
        );
        let innate = t
            .augmentation
            .iter()
            .filter(|a| a.family == Some(Family::Innate))
            .count();
        assert_eq!((innate, t.family.innate, t.family.resistance), (6, 10, 2));
        assert_eq!(
            t.by_property(326).map(|a| a.id.as_str()),
            Some("jack-of-all-trades")
        );
    }

    #[test]
    fn nalicana_and_one_seer_together_sell_at_most_sixty_five_ranks_of_aura() {
        let t = table();
        let nalicana: u32 = t.aura.iter().map(|a| a.ranks).sum();
        assert_eq!(nalicana, 55);
        for s in &t.seer {
            let with: u32 = t.aura.iter().map(|a| t.aura_ranks(a, Some(s))).sum();
            assert_eq!(with, 65, "{}", s.id);
        }
        let valor = t.aura("aura-of-valor").unwrap();
        assert_eq!(t.aura_ranks(valor, t.seer("liam-of-gelid")), 10);
        assert_eq!(t.aura_ranks(valor, t.seer("lord-tyragar")), 5);
    }

    #[test]
    fn a_level_finds_its_band_and_luminance_has_none_below_two_hundred() {
        let t = table();
        assert_eq!(t.band(1).unwrap().warn_past, 2);
        assert_eq!(t.band(180).unwrap().luminance_warn_past, None);
        assert_eq!(t.band(200).unwrap().luminance_warn_past, None);
        assert_eq!(t.band(201).unwrap().luminance_warn_past, Some(1));
        assert_eq!(t.band(275).unwrap().warn_past, 44);
        assert_eq!(t.luminance.from_level, 200);
    }

    #[test]
    fn a_cost_is_its_gems_and_one_without_a_gem_in_the_world_cannot_be_bought() {
        let costs = Costs::of(|gem, class| {
            (gem == "gemaugmentationbonusxp" && class == 29267).then_some(4_000_000_000)
        });
        assert_eq!(costs.get("quick-learner"), Some(4_000_000_000));
        assert_eq!(costs.get("jack-of-all-trades"), None);
        assert_eq!(
            costs.get("asherons-lesser-benediction"),
            Some(2_000_000_000),
            "a fixed cost"
        );
    }
}
