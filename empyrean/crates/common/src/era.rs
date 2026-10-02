// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/PlayerFactory.cs, Source/ACE.Entity/Enum/Skill.cs, Source/ACE.Server/Network/Handlers/CharacterHandler.cs
//! Not ACE: the era a world plays, and the rules that differ by era.
//!
//! A world names its era in `empyrean.toml` (`[era] profile`), and its `world.pack` records the
//! era it was built for; the server refuses to start when the two differ. Every era speaks the
//! same end-of-retail client protocol: an era changes data and rules, never the wire.
//!
//! [`EraId::Eor`] (end of retail) is ACE's behaviour, unchanged, and the default. Each other era
//! is one static [`EraRules`] table. Code asks the table for a rule (a start position, the level
//! cap, the Throne of Destiny flag); it never tests the era's name.
//!
//! [`EraId::Infiltration`] is the February 2005 world (ACE-World-16PY): its start positions are the
//! six outdoor starter areas of that era's character-generation table, and its characters start
//! with recalls enabled. The rule that picks among them (by town, one of the town's two areas at
//! random) is ClassicACE's.

use serde::{Deserialize, Serialize};

pub use dereth_primitives::era::{EraFeatureOverrides, EraFeatures, EraId, VitaeRecovery};

/// The server's side of an era: its rules, and the code `world.pack` stores for it.
pub trait EraExt: Sized {
    /// The code `world.pack` stores for the era. End of retail is 0, so a pack written before
    /// packs recorded an era reads as end of retail, which is what every such pack was.
    fn pack_code(self) -> u32;
    /// The era a `world.pack` code names; `None` for a code this build does not know.
    fn from_pack_code(code: u32) -> Option<Self>;
    /// The era's rules.
    fn rules(self) -> &'static EraRules;
}

impl EraExt for EraId {
    fn pack_code(self) -> u32 {
        match self {
            Self::Eor => 0,
            Self::Infiltration => 1,
        }
    }

    fn from_pack_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.pack_code() == code)
    }

    fn rules(self) -> &'static EraRules {
        match self {
            Self::Eor => &EOR,
            Self::Infiltration => &INFILTRATION,
        }
    }
}

/// `serde` for an [`EraId`] by its name (`"eor"`, `"infiltration"`).
pub mod era_name {
    use super::EraId;
    use serde::{Deserialize, Deserializer, Serializer};

    /// # Errors
    /// The serializer's.
    pub fn serialize<S: Serializer>(era: &EraId, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(era.name())
    }

    /// # Errors
    /// A name that is no era's.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<EraId, D::Error> {
        let name = String::deserialize(d)?;
        EraId::parse(&name).ok_or_else(|| {
            let names: Vec<&str> = EraId::ALL.iter().map(|e| e.name()).collect();
            serde::de::Error::custom(format!(
                "unknown variant `{name}`, expected one of {}",
                names.join(", ")
            ))
        })
    }
}

/// One start position: a cell and a frame (origin, then rotation as x, y, z, w).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StartPosition {
    /// The starter area's name, for logs.
    pub area: &'static str,
    pub cell: u32,
    pub origin: [f32; 3],
    /// The rotation quaternion's x, y, z and w.
    pub rotation: [f32; 4],
}

/// The start positions of one town a new character can choose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TownStart {
    /// The town, as the character-generation table names its starter area.
    pub town: &'static str,
    /// The town's starter areas; a new character starts in one of them, chosen evenly.
    pub areas: [StartPosition; 2],
}

/// Where a new character starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StartPositions {
    /// ACE's rule: the character-generation table's starter area (the Training Academy), with
    /// recalls disabled until the character leaves it, and the town's "Free Ride" spell as the
    /// instantiation point.
    FromCharGen,
    /// A listed outdoor position per town. The starter area the client chose is matched by name;
    /// any other area starts in the first town. Recalls are enabled, and the instantiation point
    /// is the start position.
    Towns(&'static [TownStart]),
}

/// The rules one era changes. Each field is a rule with its end-of-retail value in [`EOR`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EraRules {
    pub id: EraId,
    /// Where new characters start.
    pub start_positions: StartPositions,
    /// Whether accounts own Throne of Destiny, as the character list tells the client. Without it
    /// the client caps the level it shows at 126 and offers only the original heritages.
    pub account_has_tod: bool,
    /// The highest level a character can reach. `None`: the experience table's last level.
    pub max_level: Option<u32>,
    /// How random loot treats a weenie the loot tables name and the world database lacks.
    pub loot: LootTables,
    /// The skills a new character may train or specialise. `None`: ACE's rule (any skill the dat
    /// skill table has, the retired weapon skills added to it).
    pub creation_skills: Option<&'static [u32]>,
    /// The heritages a new character may be. `None`: any the dat character-generation table has.
    pub heritages: Option<&'static [u32]>,
    /// The systems the world has, each a gate at ACE's sites: the era's table
    /// ([`EraId::features`]) with the world's `[era]` settings over it ([`with_features`]).
    pub features: EraFeatures,
    /// Server properties whose default the era changes (ClassicACE's per-ruleset defaults); a
    /// value the operator has set still wins.
    pub property_defaults: &'static [(&'static str, bool)],
    /// The combat, death and experience formulas of the era.
    pub formulas: EraFormulas,
    /// The items and spells a new character is given for its trained skills.
    pub starter_gear: StarterGearSet,
    /// A first login opens this letter (a weenie class id the starter gear gives) rather than
    /// showing the training halls' welcome popup; `None`: the popup.
    pub welcome_letter: Option<u32>,
    /// Which loot tables and mutation scripts random loot is made with.
    pub loot_rules: LootRules,
}

/// The loot tables and mutation scripts random loot is made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootRules {
    /// ACE's.
    EndOfRetail,
    /// ClassicACE's for its Infiltration ruleset: weapons from the pre-2013 weapon tables by skill,
    /// heritage and tier (no two-handed weapons) with that era's damage, speed and defense
    /// scripts; that era's pyreal amounts and item values; no aetheria or coalesced mana.
    Infiltration,
}

/// Which starter-gear table new characters are given from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarterGearSet {
    /// ACE's `starterGear.json`.
    EndOfRetail,
    /// ClassicACE's `starterGear.infiltration.json`: the old weapon skills' starter weapons (by
    /// heritage), and a Welcome Letter, a Calling Stone and food for everyone.
    Infiltration,
}

/// Formulas that changed during retail's life. [`EOR`]'s are ACE's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct EraFormulas {
    /// The experience it takes to win back one point of vitae.
    pub vitae: VitaeRecovery,
    /// Weapon-speed enchantments that multiply (Rockslide and its kin) scale a weapon's speed.
    /// Without it only the additive ones (Swift Killer, Leaden Weapon) count.
    pub multiplicative_weapon_speed: bool,
    /// Melee damage before the weapon-skill consolidation: daggers (rather than Finesse Weapons)
    /// take Coordination; an unarmed humanoid scales with Strength at 0.004 per point rather than
    /// 0.011 and adds a twentieth of its Unarmed Combat skill to its maximum damage; a bare
    /// punch or kick does 1 (plus the hand or foot armour's own damage) rather than 2.
    pub older_melee_damage: bool,
    /// A creature's war-magic projectiles do half their damage.
    pub creature_projectiles_halved: bool,
    /// Shields before the Shield skill: a shield's armour level counts in full whatever the
    /// skill, in any stance, and absorbs magic in melee stance as a missile launcher does.
    pub shields_without_skill: bool,
    /// From level 21 a player dropping items at death drops `level / this` plus 0 to 2.
    pub death_items_level_divisor: i32,
    /// The most unassigned experience a character can hold; `None`: no cap.
    pub unassigned_xp_cap: Option<i64>,
}

impl EraFormulas {
    /// ACE's.
    pub const END_OF_RETAIL: Self = Self {
        vitae: EraId::Eor.vitae_recovery(),
        multiplicative_weapon_speed: false,
        older_melee_damage: false,
        creature_projectiles_halved: false,
        shields_without_skill: false,
        death_items_level_divisor: 20,
        unassigned_xp_cap: None,
    };

    /// February 2005, as ClassicACE plays it.
    pub const INFILTRATION: Self = Self {
        vitae: EraId::Infiltration.vitae_recovery(),
        multiplicative_weapon_speed: true,
        older_melee_damage: true,
        creature_projectiles_halved: true,
        shields_without_skill: true,
        death_items_level_divisor: 10,
        // Unassigned experience was a 32-bit count.
        unassigned_xp_cap: Some(u32::MAX as i64),
    };
}

/// ClassicACE's Infiltration property defaults, without `max_level` (the era's cap) and the data
/// file warning (the era's dats cannot be asked for over the end-of-retail wire).
pub static INFILTRATION_PROPERTY_DEFAULTS: [(&str, bool); 4] = [
    ("corpse_destroy_pyreals", false),
    ("item_dispel", true),
    ("vendor_shop_uses_generator", true),
    ("allow_fast_chug", false),
];

/// The February 2005 skills: exactly the 36 of that era's skill table, which ClassicACE's
/// Infiltration skill list also is: the weapon skills Axe (1) to Unarmed Combat (13) without Sling
/// (8), and none of the 2010-2013 skills (Two Handed Combat, Void Magic, Heavy, Light, Finesse and
/// Missile Weapons, Shield, Dual Wield, Recklessness, Sneak Attack, Dirty Fighting, Summoning).
pub static INFILTRATION_SKILLS: [u32; 36] = [
    1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22, 23, 24, 27, 28, 29, 30,
    31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
];

/// The February 2005 heritages: Aluvian, Gharu'ndim and Sho.
pub static INFILTRATION_HERITAGES: [u32; 3] = [1, 2, 3];

/// How random loot treats the loot tables' weenies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootTables {
    /// ACE's rule: a rolled weenie the world database lacks creates nothing (and is logged as an
    /// error). The end-of-retail world has every weenie the tables name.
    AsTables,
    /// The tables as far as the world database has them: a rolled weenie it lacks is rolled
    /// again, and a mundane add-on it lacks is not dropped. For a world older than the tables.
    PackOnly,
}

/// End of retail: ACE's rules.
pub static EOR: EraRules = EraRules {
    id: EraId::Eor,
    start_positions: StartPositions::FromCharGen,
    account_has_tod: true,
    max_level: None,
    loot: LootTables::AsTables,
    creation_skills: None,
    heritages: None,
    features: EraFeatures::END_OF_RETAIL,
    property_defaults: &[],
    formulas: EraFormulas::END_OF_RETAIL,
    starter_gear: StarterGearSet::EndOfRetail,
    welcome_letter: None,
    loot_rules: LootRules::EndOfRetail,
};

const fn at(area: &'static str, cell: u32, origin: [f32; 3], rotation: [f32; 4]) -> StartPosition {
    StartPosition {
        area,
        cell,
        origin,
        rotation,
    }
}

/// The February 2005 character-generation table's starter areas, the first position of each.
pub static INFILTRATION_TOWNS: [TownStart; 3] = [
    TownStart {
        town: "Holtburg",
        areas: [
            at(
                "Holtburg South",
                0xA9B0_0014,
                [56.065, 94.537, 64.929],
                [0.0, 0.0, -0.170_630_81, -0.985_335_05],
            ),
            at(
                "Holtburg West",
                0xA5B4_002A,
                [125.1, 30.476, 53.467],
                [0.0, 0.0, -0.183_041_99, 0.983_105_1],
            ),
        ],
    },
    TownStart {
        town: "Shoushi",
        areas: [
            at(
                "Shoushi Southeast",
                0xDE51_001D,
                [76.396, 103.326, 16.005],
                [0.0, 0.0, -0.138_058_23, -0.990_424_1],
            ),
            at(
                "Shoushi West",
                0xD655_0023,
                [97.4, 63.8, 52.005],
                [0.0, 0.0, -0.477_158_8, 0.878_817_1],
            ),
        ],
    },
    TownStart {
        town: "Yaraq",
        areas: [
            at(
                "Yaraq North",
                0x7D68_0012,
                [52.204, 37.99, 16.343],
                [0.0, 0.0, -0.942_092_5, 0.335_353_05],
            ),
            at(
                "Yaraq East",
                0x8164_000D,
                [32.599, 103.123, 30.734],
                [0.0, 0.0, -0.382_118_9, -0.924_113_1],
            ),
        ],
    },
];

/// February 2005 (Infiltration).
pub static INFILTRATION: EraRules = EraRules {
    id: EraId::Infiltration,
    start_positions: StartPositions::Towns(&INFILTRATION_TOWNS),
    account_has_tod: false,
    max_level: Some(126),
    loot: LootTables::PackOnly,
    creation_skills: Some(&INFILTRATION_SKILLS),
    heritages: Some(&INFILTRATION_HERITAGES),
    features: EraFeatures::INFILTRATION,
    property_defaults: &INFILTRATION_PROPERTY_DEFAULTS,
    formulas: EraFormulas::INFILTRATION,
    starter_gear: StarterGearSet::Infiltration,
    // The Welcome Letter.
    welcome_letter: Some(1077),
    loot_rules: LootRules::Infiltration,
};

impl StartPositions {
    /// The town a starter area named `name` stands for: the town of that name, else the first.
    #[must_use]
    pub fn town(towns: &'static [TownStart], name: &str) -> &'static TownStart {
        towns
            .iter()
            .find(|t| t.town.eq_ignore_ascii_case(name))
            .unwrap_or(&towns[0])
    }

    /// The listed starter area named `name` exactly, when there is one: on the era's own dats the
    /// character-generation table offers the areas themselves ("Holtburg South", ...) rather than
    /// the towns the later table names.
    #[must_use]
    pub fn area(towns: &'static [TownStart], name: &str) -> Option<&'static StartPosition> {
        towns
            .iter()
            .flat_map(|t| t.areas.iter())
            .find(|a| a.area.eq_ignore_ascii_case(name))
    }
}

/// `empyrean.toml` → `[era]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EraConfiguration {
    /// The era this world plays; its `world.pack` must have been built for the same era.
    #[serde(rename = "Profile", with = "era_name")]
    pub profile: EraId,
    /// The systems the world turns on or off over its era's table: one key per system, named as
    /// [`EraFeatures::NAMES`] spells it (`Aetheria`, `SpellResearch`, ... in the serde form).
    #[serde(flatten, with = "feature_settings")]
    pub features: EraFeatureOverrides,
}

impl EraConfiguration {
    /// The world's rules: the profile's, with the configured systems over its table.
    #[must_use]
    pub fn rules(&self) -> &'static EraRules {
        let rules = self.profile.rules();
        with_features(rules, self.features.apply(rules.features))
    }
}

/// `serde` for the `[era]` system keys: each a bool under its name in `PascalCase`, `null` (or left
/// out) for the profile's value.
pub mod feature_settings {
    use super::{EraFeatureOverrides, EraFeatures};
    use serde::de::{MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserializer, Serializer};

    /// A system's name in the serde form: `spell_research` is `SpellResearch`.
    #[must_use]
    pub fn key(name: &str) -> String {
        name.split('_')
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The system a serde key names (any case, underscores ignored).
    fn system(key: &str) -> Option<&'static str> {
        let plain = |s: &str| s.replace('_', "").to_ascii_lowercase();
        let k = plain(key);
        EraFeatures::NAMES.iter().copied().find(|n| plain(n) == k)
    }

    /// # Errors
    /// The serializer's.
    pub fn serialize<S: Serializer>(o: &EraFeatureOverrides, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(EraFeatures::COUNT))?;
        for name in EraFeatures::NAMES {
            map.serialize_entry(&key(name), &o.get(name))?;
        }
        map.end()
    }

    /// # Errors
    /// A system key whose value is not a bool. Keys that are no system's are left alone.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<EraFeatureOverrides, D::Error> {
        struct Keys;
        impl<'de> Visitor<'de> for Keys {
            type Value = EraFeatureOverrides;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("the era's system settings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut o = EraFeatureOverrides::default();
                while let Some(k) = map.next_key::<String>()? {
                    if let Some(name) = system(&k) {
                        if let Some(on) = map.next_value::<Option<bool>>()? {
                            o.set(name, on);
                        }
                    } else {
                        map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                Ok(o)
            }
        }
        d.deserialize_map(Keys)
    }
}

/// `rules` with `features` in place of its own. The same arguments answer the same table, made
/// once for the life of the process.
#[must_use]
pub fn with_features(rules: &'static EraRules, features: EraFeatures) -> &'static EraRules {
    if rules.features == features {
        return rules;
    }
    static MADE: std::sync::Mutex<Vec<&'static EraRules>> = std::sync::Mutex::new(Vec::new());
    let mut made = MADE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let want = EraRules { features, ..*rules };
    if let Some(r) = made.iter().find(|r| ***r == want) {
        return r;
    }
    let r: &'static EraRules = Box::leak(Box::new(want));
    made.push(r);
    r
}

/// The configured world's rules: `[era]`, or end of retail before a configuration is loaded.
#[must_use]
pub fn current() -> &'static EraRules {
    crate::config_manager::ConfigManager::try_config().map_or(EraId::Eor.rules(), |c| c.era.rules())
}

#[cfg(test)]
mod tests {
    //! The era table's rules and names.
    //! Divergence: V386 (start positions), V387 (loot), V389 (the Throne of Destiny flag), V392
    //! (creation skills and heritages), V390 (the
    //! level cap)
    use super::*;

    #[test]
    fn every_era_round_trips_its_name_and_pack_code() {
        for e in EraId::ALL {
            assert_eq!(EraId::parse(e.name()), Some(e));
            assert_eq!(EraId::from_pack_code(e.pack_code()), Some(e));
            assert_eq!(e.rules().id, e);
        }
        assert_eq!(EraId::parse("INFILTRATION"), Some(EraId::Infiltration));
        assert_eq!(EraId::parse("tod"), None);
        assert_eq!(EraId::from_pack_code(7), None);
        assert_eq!(EraId::default(), EraId::Eor);
        assert_eq!(EraId::Eor.pack_code(), 0);
    }

    /// Divergence: V392
    #[test]
    fn infiltration_creation_rules_are_the_february_2005_tables() {
        let skills = INFILTRATION.creation_skills.expect("a skill set");
        assert_eq!(skills.len(), 36);
        assert!(skills.windows(2).all(|w| w[0] < w[1]));
        for old in [1, 2, 3, 4, 5, 9, 10, 11, 12, 13, 40] {
            assert!(skills.contains(&old), "{old}");
        }
        for later in [
            8, 17, 25, 26, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54,
        ] {
            assert!(!skills.contains(&later), "{later}");
        }
        assert_eq!(INFILTRATION.heritages, Some(&[1, 2, 3][..]));
        assert_eq!((EOR.creation_skills, EOR.heritages), (None, None));
    }

    #[test]
    fn eras_order_as_retail_history() {
        assert!(EraId::Infiltration < EraId::Eor);
    }

    #[test]
    fn end_of_retail_is_aces_rules() {
        assert_eq!(EOR.start_positions, StartPositions::FromCharGen);
        assert!(EOR.account_has_tod);
        assert_eq!(EOR.max_level, None);
        assert_eq!(EOR.loot, LootTables::AsTables);
    }

    #[test]
    fn infiltration_starts_outdoors_by_town_and_caps_at_126() {
        let StartPositions::Towns(towns) = INFILTRATION.start_positions else {
            panic!("listed towns");
        };
        let landblocks = |t: &TownStart| t.areas.map(|a| a.cell >> 16);
        assert_eq!(
            landblocks(StartPositions::town(towns, "Holtburg")),
            [0xA9B0, 0xA5B4]
        );
        assert_eq!(
            landblocks(StartPositions::town(towns, "shoushi")),
            [0xDE51, 0xD655]
        );
        assert_eq!(
            landblocks(StartPositions::town(towns, "Yaraq")),
            [0x7D68, 0x8164]
        );
        // Sanamar, the Olthoi lair and anything else start in Holtburg.
        assert_eq!(StartPositions::town(towns, "Sanamar").town, "Holtburg");
        assert_eq!(StartPositions::town(towns, "OlthoiLair").town, "Holtburg");
        for t in towns {
            for a in t.areas {
                let low = a.cell & 0xFFFF;
                assert!((1..=64).contains(&low), "{} is an outdoor cell", a.area);
                let [x, y, z, w] = a.rotation;
                let norm = (x * x + y * y + z * z + w * w).sqrt();
                assert!(
                    (norm - 1.0).abs() < 1e-4,
                    "{} rotation is a unit quaternion",
                    a.area
                );
            }
        }
        assert!(!INFILTRATION.account_has_tod);
        assert_eq!(INFILTRATION.max_level, Some(126));
        assert_eq!(INFILTRATION.loot, LootTables::PackOnly);
    }

    #[test]
    fn the_era_configuration_reads_the_era_names() {
        let c: EraConfiguration = serde_json::from_str(r#"{"Profile":"infiltration"}"#).unwrap();
        assert_eq!(c.profile, EraId::Infiltration);
        let c: EraConfiguration = serde_json::from_str("{}").unwrap();
        assert_eq!(c.profile, EraId::Eor);
        assert!(serde_json::from_str::<EraConfiguration>(r#"{"Profile":"tod"}"#).is_err());
    }

    /// Divergence: V418
    /// A world's `[era]` system keys replace its profile's values, and the rules it plays are the
    /// profile's with them.
    #[test]
    fn the_era_configuration_turns_systems_on_and_off_over_the_profile() {
        let c: EraConfiguration = serde_json::from_str(
            r#"{"Profile":"infiltration","Aetheria":true,"Trade":false,"SpellResearch":false}"#,
        )
        .unwrap();
        assert_eq!(c.features.get("aetheria"), Some(true));
        assert_eq!(c.features.get("spell_research"), Some(false));
        let r = c.rules();
        assert_eq!(r.id, EraId::Infiltration);
        assert!(r.features.aetheria && !r.features.trade && r.features.housing);
        assert_eq!(r.max_level, Some(126), "the rest is the profile's");
        assert!(std::ptr::eq(r, c.rules()), "made once");
        let back: EraConfiguration =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
        // No settings: the profile's own table.
        let c: EraConfiguration = serde_json::from_str(r#"{"Profile":"infiltration"}"#).unwrap();
        assert!(std::ptr::eq(c.rules(), &INFILTRATION));
        assert!(serde_json::from_str::<EraConfiguration>(r#"{"Chess":"no"}"#).is_err());
        assert_eq!(
            feature_settings::key("pre_order_items_and_rares"),
            "PreOrderItemsAndRares"
        );
    }
}
