//! People's names, drawn from a world's name tables: one table a heritage, each a run of slots,
//! each slot a set of parts with weights. A name is one part drawn from each slot in order, by
//! weight, as the table writes it; a slot marked `word` starts a new word. The draw is seeded, so the same seed always
//! gives the same name from the same table.
//!
//! The tables are rows of the world's packs, `name_table/<heritage>.toml`:
//!
//! ```toml
//! [[name_table]]
//! heritage = "Aluvian"
//! [[name_table.slot]]
//! parts = { Bre = 3, Cal = 1 }
//! [[name_table.slot]]
//! parts = { wen = 2, "" = 1 }
//! [[name_table.slot]]
//! word = true
//! parts = { Ash = 1, Marsh = 1 }
//! ```
//!
//! A later pack's table of a heritage takes the place of an earlier one's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The folder of a pack its name tables are in.
pub const FOLDER: &str = "name_table";

/// AC1's heritages by number, as a weenie's `HeritageGroup` names them.
pub const HERITAGES: [(i64, &str); 13] = [
    (1, "Aluvian"),
    (2, "Gharundim"),
    (3, "Sho"),
    (4, "Viamontian"),
    (5, "Shadowbound"),
    (6, "Gearknight"),
    (7, "Tumerok"),
    (8, "Lugian"),
    (9, "Empyrean"),
    (10, "Penumbraen"),
    (11, "Undead"),
    (12, "Olthoi"),
    (13, "OlthoiAcid"),
];

/// The heritage numbered `n`.
#[must_use]
pub fn heritage_named(n: i64) -> Option<&'static str> {
    HERITAGES.iter().find(|(i, _)| *i == n).map(|(_, h)| *h)
}

/// A heritage's name as tables compare them: its letters and digits, lower case, so
/// "Gharu'ndim" is "Gharundim".
fn plain(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// One slot of a table: whether it starts a new word, and its parts with their weights, in order
/// of their text.
#[derive(Clone, Debug, PartialEq)]
struct Slot {
    word: bool,
    parts: Vec<(String, u64)>,
}

/// A heritage's name table.
#[derive(Clone, Debug, PartialEq)]
pub struct NameTable {
    /// The heritage, as the table writes it.
    pub heritage: String,
    slots: Vec<Slot>,
}

impl NameTable {
    /// The table a `[[name_table]]` row is.
    ///
    /// # Errors
    /// A row without a heritage, without slots, with a slot without parts, or with a weight
    /// that isn't a whole number above 0.
    pub fn of_row(row: &toml::Table) -> Result<Self, String> {
        let heritage = row
            .get("heritage")
            .and_then(toml::Value::as_str)
            .ok_or("a name table names its heritage (heritage)")?
            .to_owned();
        let slots = row
            .get("slot")
            .and_then(toml::Value::as_array)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("{heritage}'s name table has no slots"))?
            .iter()
            .enumerate()
            .map(|(n, slot)| {
                let n = n + 1;
                let parts = slot
                    .get("parts")
                    .and_then(toml::Value::as_table)
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| format!("{heritage}'s slot {n} has no parts"))?;
                let parts = parts
                    .iter()
                    .map(|(part, weight)| {
                        weight
                            .as_integer()
                            .and_then(|w| u64::try_from(w).ok())
                            .filter(|w| *w > 0)
                            .map(|w| (part.clone(), w))
                            .ok_or_else(|| {
                                format!(
                                    "{heritage}'s slot {n}: {part:?} weighs a whole number above 0"
                                )
                            })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(Slot {
                    word: slot.get("word").and_then(toml::Value::as_bool) == Some(true),
                    parts,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self { heritage, slots })
    }

    /// The name seed `seed` draws: one part from each slot, by weight, each part as the table
    /// writes it.
    #[must_use]
    pub fn name(&self, seed: &str) -> String {
        let mut draw = Draw::new(seed);
        let mut words: Vec<String> = Vec::new();
        for slot in &self.slots {
            let total: u64 = slot.parts.iter().map(|(_, w)| w).sum();
            let mut at = draw.next() % total;
            let part = slot
                .parts
                .iter()
                .find(|(_, w)| {
                    let here = at < *w;
                    at = at.saturating_sub(*w);
                    here
                })
                .map_or("", |(p, _)| p.as_str());
            match words.last_mut() {
                Some(last) if !slot.word || last.is_empty() => last.push_str(part),
                _ => words.push(part.to_owned()),
            }
        }
        words
            .iter()
            .filter(|w| !w.is_empty())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Every part of the table, slot by slot.
    #[must_use]
    pub fn parts(&self) -> Vec<Vec<&str>> {
        self.slots
            .iter()
            .map(|s| s.parts.iter().map(|(p, _)| p.as_str()).collect())
            .collect()
    }
}

/// A seeded run of numbers: the seed's FNV-1a hash, stepped by SplitMix64, the same on every
/// machine.
struct Draw(u64);

impl Draw {
    fn new(seed: &str) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in seed.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Self(h)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// A world's name tables, by heritage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NameTables(BTreeMap<String, NameTable>);

impl NameTables {
    /// The tables in `files` (each a pack file's path in its pack and its text), in load order:
    /// a later table of a heritage takes the place of an earlier one's. Files outside
    /// [`FOLDER`] are passed over.
    ///
    /// # Errors
    /// A table file that doesn't read, or a table that isn't one.
    pub fn of_files<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self, String> {
        let mut tables = BTreeMap::new();
        for (path, text) in files {
            if !path.starts_with(&format!("{FOLDER}/")) {
                continue;
            }
            let doc: toml::Table = text.parse().map_err(|e| format!("{path}: {e}"))?;
            for row in doc
                .get(FOLDER)
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
            {
                let row = row
                    .as_table()
                    .ok_or_else(|| format!("{path}: a name table is a table"))?;
                let table = NameTable::of_row(row).map_err(|e| format!("{path}: {e}"))?;
                tables.insert(plain(&table.heritage), table);
            }
        }
        Ok(Self(tables))
    }

    /// The tables of the packs in `packs` (folders, in load order).
    ///
    /// # Errors
    /// What [`Self::of_files`] refuses, and a table file that can't be read.
    pub fn of_packs(packs: &[PathBuf]) -> Result<Self, String> {
        let mut files: Vec<(String, String)> = Vec::new();
        for pack in packs {
            let dir = pack.join(FOLDER);
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut names: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .collect();
            names.sort();
            for path in names {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                let name = path.file_name().map(Path::new).unwrap_or(&path);
                files.push((format!("{FOLDER}/{}", name.display()), text));
            }
        }
        Self::of_files(files.iter().map(|(p, t)| (p.as_str(), t.as_str())))
    }

    /// The table of `heritage`, written any way ("Gharu'ndim" is "Gharundim").
    #[must_use]
    pub fn get(&self, heritage: &str) -> Option<&NameTable> {
        self.0.get(&plain(heritage))
    }

    /// The heritages there are tables of, as their tables write them.
    #[must_use]
    pub fn heritages(&self) -> Vec<&str> {
        self.0.values().map(|t| t.heritage.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world builder's name generator, drawing from a world's name tables)
    use super::*;

    const TABLE: &str = r#"
[[name_table]]
heritage = "Gharu'ndim"
[[name_table.slot]]
parts = { Ka = 3, Ze = 1, Tu = 2 }
[[name_table.slot]]
parts = { rim = 2, sha = 1, "" = 1 }
[[name_table.slot]]
word = true
parts = { "al-Wadi" = 1, "ibn Ras" = 1 }
"#;

    fn tables() -> NameTables {
        NameTables::of_files([("name_table/Gharundim.toml", TABLE)]).unwrap()
    }

    /// A seed always draws the same name, and other seeds draw others; every name is one part of
    /// each slot as the table writes it, so nothing outside the table is ever drawn.
    #[test]
    fn a_name_is_the_same_for_a_seed_and_drawn_only_from_its_table() {
        let tables = tables();
        let t = tables.get("Gharundim").expect("found by its plain name");
        assert_eq!(t.heritage, "Gharu'ndim");
        let first = t.name("2012392");
        assert_eq!(first, t.name("2012392"));
        assert_eq!(first, tables.get("gharu'ndim").unwrap().name("2012392"));
        let names: std::collections::BTreeSet<String> =
            (0..200).map(|n| t.name(&n.to_string())).collect();
        assert!(names.len() > 6, "seeds draw different names: {names:?}");
        let slots = t.parts();
        for name in &names {
            let (given, family) = name.split_once(' ').expect("two words");
            let start = slots[0]
                .iter()
                .find(|p| given.starts_with(*p))
                .unwrap_or_else(|| panic!("{name}: no first part"));
            assert!(
                slots[1].contains(&&given[start.len()..]),
                "{name}: no second part"
            );
            assert!(slots[2].contains(&family), "{name}: no third part");
        }
        // Weights count: the heaviest first part is drawn most.
        let draws: Vec<String> = (0..200).map(|n| t.name(&n.to_string())).collect();
        let ka = draws.iter().filter(|n| n.starts_with("Ka")).count();
        let ze = draws.iter().filter(|n| n.starts_with("Ze")).count();
        assert!(ka > ze, "{ka} Ka, {ze} Ze");
    }

    /// A table without slots, a slot without parts and a weight of 0 aren't tables; a later
    /// pack's table of a heritage takes the earlier one's place.
    #[test]
    fn a_table_without_parts_is_refused_and_a_later_one_takes_the_earlier_ones_place() {
        for (text, why) in [
            ("[[name_table]]\nheritage = \"Sho\"\n", "no slots"),
            (
                "[[name_table]]\nheritage = \"Sho\"\n[[name_table.slot]]\nparts = {}\n",
                "slot 1 has no parts",
            ),
            (
                "[[name_table]]\nheritage = \"Sho\"\n[[name_table.slot]]\nparts = { Ko = 0 }\n",
                "above 0",
            ),
        ] {
            let e = NameTables::of_files([("name_table/Sho.toml", text)]).unwrap_err();
            assert!(e.contains(why), "{why}: {e}");
        }
        let later =
            "[[name_table]]\nheritage = \"Gharundim\"\n[[name_table.slot]]\nparts = { Qa = 1 }\n";
        let both = NameTables::of_files([
            ("name_table/Gharundim.toml", TABLE),
            ("weenie/1.toml", "not a table at all"),
            ("name_table/Gharundim.toml", later),
        ])
        .unwrap();
        assert_eq!(both.get("Gharundim").unwrap().name("any"), "Qa");
        assert_eq!(both.heritages(), ["Gharundim"]);
        assert_eq!(heritage_named(4), Some("Viamontian"));
    }
}
