//! Not ACE (V574): a world's own rules, from its world repository's `[[rules]]` row, carried in
//! its `world.pack`.
//!
//! The rules a world plays are built in three steps: its era's table ([`crate::era::EraRules`],
//! the `[era] profile`), then the world's rules over it ([`WorldRules::apply`]), then the
//! operator's `[era]` system keys, which win ([`crate::era::EraConfiguration::rules_over`]). A
//! field the world leaves out is the era's.
//!
//! The pack holds the rules as numbers (a skill's, a heritage's); the world repository's row names
//! them, and `empyrean-world-db` turns one into the other.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::era::{
    with_features, EraFeatureOverrides, EraFeatures, EraFormulas, EraRules, LootRules, LootTables,
    MonsterCorpses, StartPositions, StarterGearSet, VitaeRecovery,
};

/// What a world's rules say, each field the era's when left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldRules {
    /// The highest level a character can reach.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_level: Option<u32>,
    /// Where new characters start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<Start>,
    /// Whether accounts own Throne of Destiny, as the character list tells the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_has_tod: Option<bool>,
    /// How random loot treats a weenie the loot tables name and the world lacks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loot: Option<Loot>,
    /// Which loot tables and mutation scripts random loot is made with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loot_rules: Option<LootSet>,
    /// The items and spells a new character is given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starter_gear: Option<GearSet>,
    /// The world is made for a land that places its own things.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placements_on_own_land: Option<bool>,
    /// The letter a first login opens (a weenie class), in place of the welcome popup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_letter: Option<u32>,
    /// The skills a new character may train or specialize.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creation_skills: Option<Allowed>,
    /// The skills a character may train after creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_skills: Option<Allowed>,
    /// The heritages a new character may be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heritages: Option<Allowed>,
    /// The world's systems, by name ([`EraFeatures::NAMES`]).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub features: BTreeMap<String, bool>,
    /// The era's formula settings, by name ([`FORMULAS`]).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub formulas: BTreeMap<String, Setting>,
    /// Defaults of the server's properties; a value the operator has set still wins.
    #[serde(skip_serializing_if = "Properties::is_empty")]
    pub properties: Properties,
}

/// Where new characters start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Start {
    /// At the character-creation table's starter area the player chose.
    #[serde(rename = "chargen")]
    CharGen,
    /// At the world's points of interest named `Start: <area>`.
    #[serde(rename = "world")]
    World,
}

/// How random loot treats a weenie the world lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Loot {
    /// As the tables say.
    #[serde(rename = "tables")]
    Tables,
    /// Only what the world holds.
    #[serde(rename = "pack-only")]
    PackOnly,
}

/// The loot tables random loot is made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LootSet {
    #[serde(rename = "eor")]
    EndOfRetail,
    #[serde(rename = "infiltration")]
    Infiltration,
    #[serde(rename = "ac1999")]
    Ac1999,
}

/// The starter gear new characters are given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GearSet {
    #[serde(rename = "eor")]
    EndOfRetail,
    #[serde(rename = "infiltration")]
    Infiltration,
    #[serde(rename = "launch")]
    Launch,
}

/// Which of a kind a world allows: all the data files have, or only these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Allowed {
    /// Every one the data files have.
    #[serde(rename = "all")]
    All,
    /// Only these, by number.
    #[serde(rename = "only")]
    Only(Vec<u32>),
}

/// A formula setting's value: a switch, a number, a name (of a choice), or none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Setting {
    Switch(bool),
    Number(i64),
    Name(String),
}

/// The server properties' defaults a world sets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Properties {
    pub bool: BTreeMap<String, bool>,
    pub long: BTreeMap<String, i64>,
    pub string: BTreeMap<String, String>,
}

impl Properties {
    /// Whether no property is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bool.is_empty() && self.long.is_empty() && self.string.is_empty()
    }
}

/// The formula settings a world can set, by name.
pub const FORMULAS: &[&str] = &[
    "vitae",
    "multiplicative_weapon_speed",
    "older_melee_damage",
    "daggers_use_coordination",
    "creature_projectiles_halved",
    "physical_hits_skip_innate_resistance",
    "kill_xp_from_level",
    "shields_without_skill",
    "death_items_level_divisor",
    "death_loss_exemptions",
    "player_corpse_limit",
    "monster_corpses",
    "empty_corpses_rot_early",
    "unassigned_xp_cap",
    "total_xp_cap",
    "long_range_missile_penalty",
    "thrown_reach_by_strength",
    "missile_flight_by_distance",
];

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

/// The formula setting `name` as `f` has it, in the form a world's rules write it; `None` for a
/// name that is no setting's.
#[must_use]
pub fn formula(f: &EraFormulas, name: &str) -> Option<Setting> {
    let limit = |n: Option<i64>| n.map_or(Setting::Switch(false), Setting::Number);
    Some(match name {
        "vitae" => Setting::Name(
            match f.vitae {
                VitaeRecovery::EndOfRetail => "eor",
                VitaeRecovery::BeforeThroneOfDestiny => "before-tod",
            }
            .into(),
        ),
        "monster_corpses" => Setting::Name(
            match f.monster_corpses {
                MonsterCorpses::LootDecaysWithCorpse => "eor",
                MonsterCorpses::NinetySecondsLootLeftBehind => "ninety-seconds",
            }
            .into(),
        ),
        "multiplicative_weapon_speed" => Setting::Switch(f.multiplicative_weapon_speed),
        "older_melee_damage" => Setting::Switch(f.older_melee_damage),
        "daggers_use_coordination" => Setting::Switch(f.daggers_use_coordination),
        "creature_projectiles_halved" => Setting::Switch(f.creature_projectiles_halved),
        "physical_hits_skip_innate_resistance" => {
            Setting::Switch(f.physical_hits_skip_innate_resistance)
        }
        "kill_xp_from_level" => Setting::Switch(f.kill_xp_from_level),
        "shields_without_skill" => Setting::Switch(f.shields_without_skill),
        "death_items_level_divisor" => Setting::Number(i64::from(f.death_items_level_divisor)),
        "death_loss_exemptions" => Setting::Switch(f.death_loss_exemptions),
        "player_corpse_limit" => limit(f.player_corpse_limit.map(i64::from)),
        "empty_corpses_rot_early" => Setting::Switch(f.empty_corpses_rot_early),
        "unassigned_xp_cap" => limit(f.unassigned_xp_cap),
        "total_xp_cap" => limit(f.total_xp_cap),
        "long_range_missile_penalty" => Setting::Switch(f.long_range_missile_penalty),
        "thrown_reach_by_strength" => Setting::Switch(f.thrown_reach_by_strength),
        "missile_flight_by_distance" => Setting::Switch(f.missile_flight_by_distance),
        _ => return None,
    })
}

/// `formulas` with the setting `name` set to `v`.
fn set_formula(f: &mut EraFormulas, name: &str, v: &Setting) -> Result<(), String> {
    let bad = || format!("formula {name}: {v:?} is not a value it takes");
    let switch = |v: &Setting| match v {
        Setting::Switch(b) => Ok(*b),
        _ => Err(bad()),
    };
    let number = |v: &Setting| match v {
        Setting::Number(n) => Ok(*n),
        _ => Err(bad()),
    };
    // A limit: a number, or `false` for none.
    let limit = |v: &Setting| match v {
        Setting::Number(n) => Ok(Some(*n)),
        Setting::Switch(false) => Ok(None),
        _ => Err(bad()),
    };
    match name {
        "vitae" => {
            f.vitae = match v {
                Setting::Name(n) if n == "eor" => VitaeRecovery::EndOfRetail,
                Setting::Name(n) if n == "before-tod" => VitaeRecovery::BeforeThroneOfDestiny,
                _ => {
                    return Err(format!(
                        "formula vitae is \"eor\" or \"before-tod\", not {v:?}"
                    ))
                }
            }
        }
        "monster_corpses" => {
            f.monster_corpses = match v {
                Setting::Name(n) if n == "eor" => MonsterCorpses::LootDecaysWithCorpse,
                Setting::Name(n) if n == "ninety-seconds" => {
                    MonsterCorpses::NinetySecondsLootLeftBehind
                }
                _ => {
                    return Err(format!(
                        "formula monster_corpses is \"eor\" or \"ninety-seconds\", not {v:?}"
                    ))
                }
            }
        }
        "multiplicative_weapon_speed" => f.multiplicative_weapon_speed = switch(v)?,
        "older_melee_damage" => f.older_melee_damage = switch(v)?,
        "daggers_use_coordination" => f.daggers_use_coordination = switch(v)?,
        "creature_projectiles_halved" => f.creature_projectiles_halved = switch(v)?,
        "physical_hits_skip_innate_resistance" => {
            f.physical_hits_skip_innate_resistance = switch(v)?;
        }
        "kill_xp_from_level" => f.kill_xp_from_level = switch(v)?,
        "shields_without_skill" => f.shields_without_skill = switch(v)?,
        "death_items_level_divisor" => {
            f.death_items_level_divisor = i32::try_from(number(v)?).map_err(|_| bad())?;
        }
        "death_loss_exemptions" => f.death_loss_exemptions = switch(v)?,
        "player_corpse_limit" => {
            f.player_corpse_limit = limit(v)?
                .map(|n| u32::try_from(n).map_err(|_| bad()))
                .transpose()?;
        }
        "empty_corpses_rot_early" => f.empty_corpses_rot_early = switch(v)?,
        "unassigned_xp_cap" => f.unassigned_xp_cap = limit(v)?,
        "total_xp_cap" => f.total_xp_cap = limit(v)?,
        "long_range_missile_penalty" => f.long_range_missile_penalty = switch(v)?,
        "thrown_reach_by_strength" => f.thrown_reach_by_strength = switch(v)?,
        "missile_flight_by_distance" => f.missile_flight_by_distance = switch(v)?,
        _ => {
            return Err(format!(
                "{name:?} is not a formula setting; they are {FORMULAS:?}"
            ))
        }
    }
    Ok(())
}

impl WorldRules {
    /// The era's own rules written as a world's: every field a world's rules can say, as `era`
    /// has it. A start a world can't say (a list of towns) is left out.
    #[must_use]
    pub fn of_era(era: &EraRules) -> Self {
        let allowed =
            |a: Option<&[u32]>| Some(a.map_or(Allowed::All, |v| Allowed::Only(v.to_vec())));
        Self {
            max_level: era.max_level,
            start: match era.start_positions {
                StartPositions::FromCharGen => Some(Start::CharGen),
                StartPositions::FromWorld => Some(Start::World),
                _ => None,
            },
            account_has_tod: Some(era.account_has_tod),
            loot: Some(match era.loot {
                LootTables::AsTables => Loot::Tables,
                LootTables::PackOnly => Loot::PackOnly,
            }),
            loot_rules: Some(match era.loot_rules {
                LootRules::EndOfRetail => LootSet::EndOfRetail,
                LootRules::Infiltration => LootSet::Infiltration,
                LootRules::Ac1999 => LootSet::Ac1999,
            }),
            starter_gear: Some(match era.starter_gear {
                StarterGearSet::EndOfRetail => GearSet::EndOfRetail,
                StarterGearSet::Infiltration => GearSet::Infiltration,
                StarterGearSet::Launch => GearSet::Launch,
            }),
            placements_on_own_land: Some(era.placements_on_own_land),
            welcome_letter: era.welcome_letter,
            creation_skills: allowed(era.creation_skills),
            training_skills: allowed(era.training_skills),
            heritages: allowed(era.heritages),
            features: era
                .features
                .iter()
                .map(|(n, on)| (n.to_owned(), on))
                .collect(),
            formulas: FORMULAS
                .iter()
                .filter_map(|n| formula(&era.formulas, n).map(|v| ((*n).to_owned(), v)))
                .collect(),
            properties: Properties {
                bool: era
                    .property_defaults
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), *v))
                    .collect(),
                long: era
                    .long_property_defaults
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), *v))
                    .collect(),
                string: era
                    .string_property_defaults
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect(),
            },
        }
    }

    /// Whether the rules say nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Every name in the rules is one the server knows: each system and formula setting.
    ///
    /// # Errors
    /// The first name that is not, or a formula value its setting doesn't take.
    pub fn check(&self) -> Result<(), String> {
        for name in self.features.keys() {
            if !EraFeatures::NAMES.contains(&name.as_str()) {
                return Err(format!("{name:?} is not one of the world's systems"));
            }
        }
        let mut f = crate::era::EOR.formulas;
        for (name, v) in &self.formulas {
            set_formula(&mut f, name, v)?;
        }
        Ok(())
    }

    /// `era` with these rules over it. The same rules over the same era answer the same table,
    /// made once for the life of the process.
    ///
    /// # Errors
    /// A name the server doesn't know ([`Self::check`]).
    pub fn apply(&self, era: &'static EraRules) -> Result<&'static EraRules, String> {
        self.check()?;
        if self.is_empty() {
            return Ok(era);
        }
        let mut r = *era;
        if let Some(v) = self.max_level {
            r.max_level = Some(v);
        }
        if let Some(v) = self.start {
            r.start_positions = match v {
                Start::CharGen => StartPositions::FromCharGen,
                Start::World => StartPositions::FromWorld,
            };
        }
        if let Some(v) = self.account_has_tod {
            r.account_has_tod = v;
        }
        if let Some(v) = self.loot {
            r.loot = match v {
                Loot::Tables => LootTables::AsTables,
                Loot::PackOnly => LootTables::PackOnly,
            };
        }
        if let Some(v) = self.loot_rules {
            r.loot_rules = match v {
                LootSet::EndOfRetail => LootRules::EndOfRetail,
                LootSet::Infiltration => LootRules::Infiltration,
                LootSet::Ac1999 => LootRules::Ac1999,
            };
        }
        if let Some(v) = self.starter_gear {
            r.starter_gear = match v {
                GearSet::EndOfRetail => StarterGearSet::EndOfRetail,
                GearSet::Infiltration => StarterGearSet::Infiltration,
                GearSet::Launch => StarterGearSet::Launch,
            };
        }
        if let Some(v) = self.placements_on_own_land {
            r.placements_on_own_land = v;
        }
        if let Some(v) = self.welcome_letter {
            r.welcome_letter = Some(v);
        }
        let allowed = |a: &Allowed| match a {
            Allowed::All => None,
            Allowed::Only(v) => Some(leak(v.clone())),
        };
        if let Some(a) = &self.creation_skills {
            r.creation_skills = allowed(a);
        }
        if let Some(a) = &self.training_skills {
            r.training_skills = allowed(a);
        }
        if let Some(a) = &self.heritages {
            r.heritages = allowed(a);
        }
        let mut features = EraFeatureOverrides::default();
        for (name, on) in &self.features {
            features.set(name, *on);
        }
        let before = r.features;
        r.features = features.apply(r.features);
        if self.heritages.is_none() {
            r.heritages = crate::era::heritages_following(r.heritages, before, r.features);
        }
        for (name, v) in &self.formulas {
            set_formula(&mut r.formulas, name, v)?;
        }
        // A property the world sets replaces the era's default of it; the era's others stay.
        fn over<T: Clone>(
            era: &'static [(&'static str, T)],
            world: &BTreeMap<String, T>,
        ) -> &'static [(&'static str, T)] {
            if world.is_empty() {
                return era;
            }
            let mut v: Vec<(&'static str, T)> = era
                .iter()
                .filter(|(k, _)| !world.contains_key(*k))
                .cloned()
                .collect();
            v.extend(world.iter().map(|(k, x)| (leak_str(k), x.clone())));
            leak(v)
        }
        r.property_defaults = over(era.property_defaults, &self.properties.bool);
        r.long_property_defaults = over(era.long_property_defaults, &self.properties.long);
        let strings: BTreeMap<String, &'static str> = self
            .properties
            .string
            .iter()
            .map(|(k, v)| (k.clone(), leak_str(v)))
            .collect();
        r.string_property_defaults = over(era.string_property_defaults, &strings);
        Ok(made(r))
    }
}

/// `r`, kept for the life of the process: the same table is made once.
fn made(r: EraRules) -> &'static EraRules {
    static MADE: std::sync::Mutex<Vec<&'static EraRules>> = std::sync::Mutex::new(Vec::new());
    let mut made = MADE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(m) = made.iter().find(|m| ***m == r) {
        return m;
    }
    let m: &'static EraRules = Box::leak(Box::new(r));
    made.push(m);
    // The systems are the table's own; `with_features` answers the same table for them.
    with_features(m, m.features)
}

#[cfg(test)]
mod tests {
    //! Divergence: V574
    use super::*;
    use crate::era::{EraConfiguration, EraExt, EraId, NEW_DERETH};

    fn rules() -> WorldRules {
        WorldRules {
            max_level: Some(50),
            start: Some(Start::CharGen),
            loot: Some(Loot::Tables),
            heritages: Some(Allowed::Only(vec![1, 2])),
            creation_skills: Some(Allowed::All),
            features: BTreeMap::from([
                ("housing".to_owned(), false),
                ("aetheria".to_owned(), false),
            ]),
            formulas: BTreeMap::from([
                ("older_melee_damage".to_owned(), Setting::Switch(true)),
                ("total_xp_cap".to_owned(), Setting::Number(1_000_000)),
                ("vitae".to_owned(), Setting::Name("before-tod".into())),
            ]),
            properties: Properties {
                string: BTreeMap::from([("popup_welcome".to_owned(), "Welcome!".to_owned())]),
                bool: BTreeMap::from([("pk_server".to_owned(), true)]),
                ..Properties::default()
            },
            ..WorldRules::default()
        }
    }

    /// A world's rules lie over its era's: each field they set is theirs, each they leave out the
    /// era's; a property they set replaces the era's default of it and keeps the era's others.
    #[test]
    fn a_worlds_rules_lie_over_its_eras_and_leave_the_rest_as_the_era_has_it() {
        let era = &NEW_DERETH;
        let r = rules().apply(era).expect("applies");
        assert_eq!(r.max_level, Some(50));
        assert_eq!(r.start_positions, StartPositions::FromCharGen);
        assert_eq!(r.loot, LootTables::AsTables);
        assert_eq!(r.heritages, Some(&[1, 2][..]));
        assert_eq!(r.creation_skills, None);
        assert!(!r.features.housing && !r.features.aetheria);
        assert_eq!(
            r.features.consolidated_weapon_skills,
            era.features.consolidated_weapon_skills
        );
        assert!(r.formulas.older_melee_damage);
        assert_eq!(r.formulas.total_xp_cap, Some(1_000_000));
        assert_eq!(r.formulas.vitae, VitaeRecovery::BeforeThroneOfDestiny);
        assert_eq!(
            r.formulas.kill_xp_from_level,
            era.formulas.kill_xp_from_level
        );
        assert_eq!(r.string_property_defaults, &[("popup_welcome", "Welcome!")]);
        assert_eq!(r.property_defaults, &[("pk_server", true)]);
        assert_eq!(
            (r.id, r.calendar, r.welcome_letter),
            (era.id, era.calendar, era.welcome_letter)
        );
        assert!(std::ptr::eq(r, rules().apply(era).unwrap()), "made once");
        assert!(std::ptr::eq(WorldRules::default().apply(era).unwrap(), era));
    }

    /// The operator's `[era]` keys win over the world's rules; without the world's, the rules are
    /// the era's with the keys over them, as before.
    #[test]
    fn the_operators_era_keys_win_over_the_worlds_rules() {
        let mut config = EraConfiguration {
            profile: EraId::NewDereth,
            ..EraConfiguration::default()
        };
        config.features.set("housing", true);
        let r = config.rules_over(Some(&rules())).expect("applies");
        assert!(r.features.housing, "the operator's key");
        assert!(!r.features.aetheria, "the world's");
        assert_eq!(r.max_level, Some(50));
        assert!(std::ptr::eq(
            config.rules_over(None).unwrap(),
            config.rules()
        ));
        assert_eq!(config.rules().max_level, EraId::NewDereth.rules().max_level);
    }

    /// A system or formula setting the server doesn't know, or a value its setting doesn't take,
    /// is refused, by name.
    #[test]
    fn a_name_the_server_doesnt_know_is_refused() {
        let mut r = WorldRules::default();
        r.features.insert("teleporting_cows".into(), true);
        assert!(r
            .apply(&NEW_DERETH)
            .unwrap_err()
            .contains("teleporting_cows"));
        let mut r = WorldRules::default();
        r.formulas
            .insert("older_melee_damage".into(), Setting::Number(3));
        assert!(r.check().unwrap_err().contains("older_melee_damage"));
        let mut r = WorldRules::default();
        r.formulas
            .insert("vitae".into(), Setting::Name("gentle".into()));
        assert!(r.check().unwrap_err().contains("vitae"));
    }

    /// An era's rules written as a world's say every field the era has, each as it has it, so laid
    /// over any other era they make that era again, but for what a world's rules can't say: no
    /// level cap, a start of towns and no welcome letter (each keeps the era's below), and an era's
    /// property default taken away (the era's others stay). Every formula setting reads back as one the server takes.
    #[test]
    fn an_eras_rules_written_as_a_worlds_make_the_era_again() {
        for era in EraId::ALL {
            let rules = era.rules();
            let written = WorldRules::of_era(rules);
            written.check().unwrap();
            assert_eq!(written.formulas.len(), FORMULAS.len());
            assert_eq!(written.features.len(), EraFeatures::COUNT);
            for over in EraId::ALL {
                let mut made = WorldRules::of_era(written.apply(over.rules()).unwrap());
                if written.max_level.is_none() {
                    made.max_level = None;
                }
                if written.start.is_none() {
                    made.start = None;
                }
                if written.welcome_letter.is_none() {
                    made.welcome_letter = None;
                }
                let p = &written.properties;
                made.properties.bool.retain(|k, _| p.bool.contains_key(k));
                made.properties.long.retain(|k, _| p.long.contains_key(k));
                made.properties
                    .string
                    .retain(|k, _| p.string.contains_key(k));
                assert_eq!(made, written, "{era:?} over {over:?}");
            }
        }
        let eor = WorldRules::of_era(&crate::era::EOR);
        assert!(eor.features["consolidated_weapon_skills"]);
        assert_eq!(eor.formulas["vitae"], Setting::Name("eor".into()));
        assert_eq!(formula(&crate::era::EOR.formulas, "teleporting_cows"), None);
    }
}
