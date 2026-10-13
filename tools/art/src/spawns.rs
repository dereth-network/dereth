//! The server's spawn table: which setups the objects a server spawns are drawn with, how many it
//! spawns and where.
//!
//! The table is written by the server's world-database importer (`empyrean-import spawns`); this
//! tool only reads it, so the survey depends on no server code. It is tab-separated, with a
//! header line and one row per setup:
//!
//! | column | what |
//! |---|---|
//! | `setup` | the setup (`0x02…`) |
//! | `type` | what most of its spawns are: `creature`, `npc`, `door`, `portal`, `lifestone`, `chest`, `hook`, `generator` or `item` |
//! | `spawns` | how many are present when every generator is full |
//! | `landblocks` | how many landblocks they may be in |
//! | `weenies` | `;`-separated `class:name:type:count`, the weenie classes drawn with it |
//! | `where` | space-separated `XXYY:count`, the landblocks, most first |

use crate::tally::{keep_examples, Example, ModelId, Tally, Use};
use dereth_primitives::DataId;

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnRow {
    pub setup: DataId,
    pub kind: String,
    pub spawns: u64,
    pub landblocks: u64,
    /// `(class, name, type, count)`.
    pub weenies: Vec<(u32, String, String, u64)>,
    /// `(landblock, count)`, most first.
    pub places: Vec<(u16, u64)>,
}

/// Reads the table. A malformed row is an error naming its line.
///
/// # Errors
/// The first row that will not read.
pub fn parse(text: &str) -> Result<Vec<SpawnRow>, String> {
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("setup\t") {
            continue;
        }
        let bad = |what: &str| format!("line {}: {what}: {line:?}", n + 1);
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 6 {
            return Err(bad("six tab-separated columns wanted"));
        }
        let setup = match ModelId::parse(cols[0]) {
            Some(ModelId::Object(id)) => id,
            _ => return Err(bad("the setup is not an id")),
        };
        let number = |s: &str| s.trim().parse::<u64>().map_err(|_| bad("not a count"));
        let mut weenies = Vec::new();
        for w in cols[4].split(';').filter(|w| !w.trim().is_empty()) {
            let parts: Vec<&str> = w.trim().split(':').collect();
            let [class, name, kind, count] = parts[..] else {
                return Err(bad("a weenie is class:name:type:count"));
            };
            weenies.push((
                class
                    .parse()
                    .map_err(|_| bad("a weenie class is a number"))?,
                name.to_owned(),
                kind.to_owned(),
                number(count)?,
            ));
        }
        let mut places = Vec::new();
        for p in cols[5].split_whitespace() {
            let (lb, count) = p
                .split_once(':')
                .ok_or_else(|| bad("a place is XXYY:count"))?;
            let lb = u16::from_str_radix(lb, 16).map_err(|_| bad("a landblock is XXYY"))?;
            places.push((lb, number(count)?));
        }
        rows.push(SpawnRow {
            setup,
            kind: cols[1].trim().to_owned(),
            spawns: number(cols[2])?,
            landblocks: number(cols[3])?,
            weenies,
            places,
        });
    }
    Ok(rows)
}

/// Adds the rows to `tally` as the spawn use of each setup.
pub fn fold(tally: &mut Tally, rows: &[SpawnRow]) {
    for r in rows {
        let s = tally.stats_mut(ModelId::Object(r.setup), Use::Spawn);
        s.instances += r.spawns;
        s.landblocks += r.landblocks;
        *s.notes.entry(format!("type {}", r.kind)).or_default() += r.spawns;
        for (class, name, kind, count) in &r.weenies {
            *s.notes
                .entry(format!("{class} {name} ({kind})"))
                .or_default() += count;
        }
        let mut examples: Vec<Example> = r
            .places
            .iter()
            .map(|&(landblock, count)| Example {
                landblock,
                count,
                position: None,
                cell: None,
            })
            .collect();
        examples.extend(s.examples.iter().copied());
        keep_examples(&mut examples);
        s.examples = examples;
    }
}

/// How many landblocks the rows place anything in.
#[must_use]
pub fn landblocks(rows: &[SpawnRow]) -> u64 {
    let all: std::collections::BTreeSet<u16> = rows
        .iter()
        .flat_map(|r| r.places.iter().map(|p| p.0))
        .collect();
    all.len() as u64
}
