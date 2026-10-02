// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/PlayerFactory.cs
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

/// An era, in the order of retail history, so "at or before" compares as the ordinal does.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum EraId {
    /// February 2005: the Infiltration patch, before Throne of Destiny.
    #[serde(rename = "infiltration")]
    Infiltration,
    /// The end of retail (ACE's world, and ACE's rules).
    #[default]
    #[serde(rename = "eor")]
    Eor,
}

impl EraId {
    /// Every era, in order.
    pub const ALL: [Self; 2] = [Self::Infiltration, Self::Eor];

    /// The name `empyrean.toml`, `empyrean-import --era` and the status document use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Infiltration => "infiltration",
            Self::Eor => "eor",
        }
    }

    /// The era named `name` (as [`Self::name`] spells it, any case).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|e| e.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The code `world.pack` stores for the era. End of retail is 0, so a pack written before
    /// packs recorded an era reads as end of retail, which is what every such pack was.
    #[must_use]
    pub const fn pack_code(self) -> u32 {
        match self {
            Self::Eor => 0,
            Self::Infiltration => 1,
        }
    }

    /// The era a `world.pack` code names; `None` for a code this build does not know.
    #[must_use]
    pub fn from_pack_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.pack_code() == code)
    }

    /// The era's rules.
    #[must_use]
    pub const fn rules(self) -> &'static EraRules {
        match self {
            Self::Eor => &EOR,
            Self::Infiltration => &INFILTRATION,
        }
    }
}

impl std::fmt::Display for EraId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
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
}

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
}

/// `empyrean.toml` → `[era]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EraConfiguration {
    /// The era this world plays; its `world.pack` must have been built for the same era.
    #[serde(rename = "Profile")]
    pub profile: EraId,
}

/// The configured era's rules: `[era] profile`, or end of retail before a configuration is loaded.
#[must_use]
pub fn current() -> &'static EraRules {
    crate::config_manager::ConfigManager::try_config()
        .map_or(EraId::Eor, |c| c.era.profile)
        .rules()
}

#[cfg(test)]
mod tests {
    //! The era table's rules and names.
    //! Divergence: V386 (start positions), V387 (loot), V389 (the Throne of Destiny flag), V390 (the
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
}
