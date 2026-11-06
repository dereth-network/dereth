//! Named asset compatibility adaptations, applied only to exact effective source rows.

use std::collections::BTreeMap;

use empyrean_content::error::ImportError;
use empyrean_content::import::mysqldump::{Row, Value};
use serde_json::json;

use super::convert::{fail, number};
use super::source::hash;

pub const ADAPTER: &str = "yanshi-cells-v1";

/// One retail data file a target names: its capture date, header iteration and SHA-256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatIdentity {
    pub capture: &'static str,
    pub iteration: u32,
    pub sha256: &'static str,
}

/// The October 31, 1999 `portal.dat`: the retail CD's with that month's records added.
pub const PORTAL_1999_10_31: DatIdentity = DatIdentity {
    capture: "1999-10-31",
    iteration: 105,
    sha256: "778f8d927d4fb82fc69bf03e06576a04fff3529d610a08debc0b863abddf80ff",
};
/// The retail CD's `portal.dat`, October 9, 1999.
pub const PORTAL_1999_10_09: DatIdentity = DatIdentity {
    capture: "1999-10-09",
    iteration: 102,
    sha256: "195e6fbb6d1cb9c849b2f4732c29509b5f20f67038a1d0763041387287c1975a",
};
/// The retail CD's `cell.dat`, October 9, 1999, which both 1999 targets use unchanged.
pub const CELL_1999_10_09: DatIdentity = DatIdentity {
    capture: "1999-10-09",
    iteration: 67,
    sha256: "111855655c7b1daec563a96476e29cf10f17a8252a50f6102f1245146e0870b8",
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// Every row as the source wrote it, for the later dats it was placed against.
    Source,
    /// The world as it launched: the October 31, 1999 portal and the retail CD's cells.
    #[default]
    Launch,
    /// The retail CD's own pair, October 9, 1999.
    ClassicOctober1999,
}

impl Target {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "source" => Ok(Self::Source),
            "launch" => Ok(Self::Launch),
            "classic-1999-10-09" => Ok(Self::ClassicOctober1999),
            _ => Err(format!(
                "unsupported asset target {value}; expected launch, classic-1999-10-09 or source"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Launch => "launch",
            Self::ClassicOctober1999 => "classic-1999-10-09",
        }
    }

    /// The `portal.dat` and `cell.dat` the target is played with, when it names them.
    pub fn dats(self) -> Option<(DatIdentity, DatIdentity)> {
        match self {
            Self::Source => None,
            Self::Launch => Some((PORTAL_1999_10_31, CELL_1999_10_09)),
            Self::ClassicOctober1999 => Some((PORTAL_1999_10_09, CELL_1999_10_09)),
        }
    }

    /// Whether the target is a 1999 pair, whose world drops what those files cannot show.
    pub fn is_1999(self) -> bool {
        self.dats().is_some()
    }

    pub fn manifest(self) -> serde_json::Value {
        let identity =
            |d: DatIdentity| json!({"capture":d.capture,"iteration":d.iteration,"sha256":d.sha256});
        json!({"name":self.name(),
            "adapter":self.is_1999().then_some(ADAPTER),
            "world":self.is_1999().then_some(super::world::POLICY),
            "dats":self.dats().map(|(portal, cell)| json!({
                "portal.dat":identity(portal),"cell.dat":identity(cell)}))})
    }
}

#[derive(Debug)]
struct Rule {
    guid: i128,
    class: i128,
    before: u32,
    after: u32,
    fingerprint: String,
    seen: bool,
}

#[derive(Debug, Default)]
pub struct Adaptation {
    target: Target,
    rules: Vec<Rule>,
}

impl Adaptation {
    pub fn new(target: Target) -> Self {
        if !target.is_1999() {
            return Self {
                target,
                rules: Vec::new(),
            };
        }
        // ac1999's extended data for landblock 0xB973 at 5fe910e7a3e549d955117920a1bd30b4ee646938,
        // lines 4, 11 and 15.
        // The hashes cover every effective SQL field, including coordinates and timestamps.
        let rows = [
            (
                0x7B973000,
                1154,
                0xB9730100,
                0xB9730107,
                "c6b5008134416c4b895043e5313878e1d65081cc90e3783290196a3ecb74f263",
            ),
            (
                0x7B973001,
                5064,
                0xB9730100,
                0xB9730107,
                "fdd7a34b7480936c3f413817d90f7ab45763ed2278e95404a1af830762cfe684",
            ),
            (
                0x7B973002,
                4565,
                0xB9730106,
                0xB973010D,
                "f5a2a5c67117378300c639e60af2c080814323408590bbc30efb3aff3c318855",
            ),
        ];
        Self {
            target,
            rules: rows
                .into_iter()
                .map(|(guid, class, before, after, fingerprint)| Rule {
                    guid,
                    class,
                    before,
                    after,
                    fingerprint: fingerprint.into(),
                    seen: false,
                })
                .collect(),
        }
    }

    pub fn target(&self) -> Target {
        self.target
    }

    pub fn cell(&mut self, row: &Row<'_>) -> Result<Option<u32>, ImportError> {
        if self.rules.is_empty() || row.table != "landblock_instance" {
            return Ok(None);
        }
        let guid = number(row, "guid")?;
        let Some(rule) = self.rules.iter_mut().find(|r| r.guid == guid) else {
            return Ok(None);
        };
        if rule.seen
            || number(row, "weenie_Class_Id")? != rule.class
            || number(row, "obj_Cell_Id")? != i128::from(rule.before)
            || fingerprint(row)? != rule.fingerprint
        {
            return Err(fail(format!("{ADAPTER}: source row {guid:#X} does not match the reviewed row; refusing target adaptation")));
        }
        rule.seen = true;
        Ok(Some(rule.after))
    }

    pub fn complete(&self) -> Result<(), ImportError> {
        if let Some(rule) = self.rules.iter().find(|r| !r.seen) {
            return Err(fail(format!(
                "{ADAPTER}: missing required source row {:#X}; refusing target adaptation",
                rule.guid
            )));
        }
        Ok(())
    }
}

/// Stable JSON of lowercase column names to (quoted, nullable UTF-8 value), ordered by name.
fn fingerprint(row: &Row<'_>) -> Result<String, ImportError> {
    let mut fields = BTreeMap::new();
    for (column, value) in row.columns().iter().zip(row.values()) {
        let text = value
            .bytes()
            .map(std::str::from_utf8)
            .transpose()
            .map_err(|e| fail(e.to_string()))?;
        if fields
            .insert(
                column.to_ascii_lowercase(),
                (matches!(value, Value::Quoted(_)), text),
            )
            .is_some()
        {
            return Err(fail("duplicate source column in cell adaptation"));
        }
    }
    Ok(hash(
        &serde_json::to_vec(&fields).map_err(|e| fail(e.to_string()))?,
    ))
}

#[cfg(test)]
mod tests;
