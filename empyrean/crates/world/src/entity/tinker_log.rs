// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/TinkerLog.cs
//! Port of `Source/ACE.Server/Entity/TinkerLog.cs`.

use empyrean_entity::enums::MaterialType;

use crate::entity::mutations::mutation_cache::enum_try_parse_by;

// ACE: TinkerLog
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TinkerLog {
    // ACE: TinkerLog.Tinkers
    pub tinkers: Vec<MaterialType>,
}

impl TinkerLog {
    // ACE: TinkerLog.TinkerLog
    /// Parses a `TinkerLog` csv: each value a `MaterialType` (`Enum.TryParse(val, true, ...)`: a
    /// number, or member names in any case); a value that does not parse is skipped.
    #[must_use]
    pub fn new(csv: Option<&str>) -> Self {
        let mut tinkers = Vec::new();

        let Some(csv) = csv else {
            return Self { tinkers };
        };

        let vals = csv.split(',');

        for val in vals {
            let Some(material_type) = material_type_try_parse_ignore_case(val) else {
                // (ACE writes this with Console.WriteLine)
                log::info!("Couldn't parse {val}");
                continue;
            };
            tinkers.push(material_type);
        }

        Self { tinkers }
    }

    // ACE: TinkerLog.NumTinkers
    #[must_use]
    pub fn num_tinkers(&self, r#type: MaterialType) -> i32 {
        i32::try_from(self.tinkers.iter().filter(|&&i| i == r#type).count()).unwrap_or(i32::MAX)
    }
}

/// `Enum.TryParse(val, ignoreCase: true, out MaterialType)`: a `uint` number, or comma-separated
/// member names compared case-insensitively.
fn material_type_try_parse_ignore_case(val: &str) -> Option<MaterialType> {
    let v = enum_try_parse_by(val, |name| {
        MaterialType::NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|i| i64::from(MaterialType::ALL[i].0))
    })?;
    u32::try_from(v).ok().map(MaterialType)
}
