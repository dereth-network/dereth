//! What a world changes, counted from its sources: the landblocks whose terrain it edits, the
//! statics, buildings, dungeons and rooms it adds or changes, the spawns and weenies its server
//! content holds, and the pictures and models it brings. Reading needs no data files: only the
//! world file and its packs.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use crate::error::Error;
use crate::model_import::Record;
use crate::source::{LandblockEdits, NewRecord, Pack, RecordRef, Target};
use crate::world::World;

/// A world's changes, counted over all its packs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// The world's key.
    pub key: String,
    /// The era whose files it is built on, as configuration files spell it.
    pub era: &'static str,
    /// The packs it lists.
    pub packs: usize,
    /// Landblocks whose terrain any pack edits.
    pub terrain: usize,
    /// Statics placed, moved or removed, outdoors and in rooms (a new room's own among them).
    pub statics: usize,
    /// Buildings added (each of a set) and buildings moved.
    pub buildings: usize,
    /// Landblocks whose dungeon a pack adds rooms to, reshapes, copies or names.
    pub dungeons: usize,
    /// Rooms added or reshaped.
    pub rooms: usize,
    /// Instances the packs' server content places.
    pub spawns: usize,
    /// Weenie classes the packs' server content defines.
    pub weenies: usize,
    /// Pictures added or replaced.
    pub textures: usize,
    /// Models added or replaced.
    pub models: usize,
}

impl Facts {
    /// One line of JSON, with the fields in the order they are declared.
    #[must_use]
    pub fn json(&self) -> String {
        format!(
            "{{\"key\":{},\"era\":{},\"packs\":{},\"terrain\":{},\"statics\":{},\"buildings\":{},\"dungeons\":{},\"rooms\":{},\"spawns\":{},\"weenies\":{},\"textures\":{},\"models\":{}}}",
            json_string(&self.key),
            json_string(self.era),
            self.packs,
            self.terrain,
            self.statics,
            self.buildings,
            self.dungeons,
            self.rooms,
            self.spawns,
            self.weenies,
            self.textures,
            self.models,
        )
    }
}

/// `text` as a JSON string.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The changes of the world whose file is `world`, counted from its sources.
///
/// # Errors
/// A world file or a pack that does not read.
pub fn facts(world: &Path) -> Result<Facts, Error> {
    let world = World::read(world)?;
    let packs = world
        .packs
        .iter()
        .map(|dir| crate::source::read_pack(dir))
        .collect::<Result<Vec<_>, _>>()?;
    let mut facts = Facts {
        key: world.key.clone(),
        era: world.era.name(),
        packs: packs.len(),
        ..Facts::default()
    };
    let edits: Vec<&LandblockEdits> = packs.iter().flat_map(|p| &p.edits).collect();
    count_edits(&edits, &mut facts);
    count_server(&packs, &mut facts);
    count_records(&packs, &mut facts);
    Ok(facts)
}

/// The landblock operations' share: terrain, statics, buildings, dungeons and rooms.
fn count_edits(edits: &[&LandblockEdits], facts: &mut Facts) {
    let mut terrain = BTreeSet::new();
    let mut dungeons = BTreeSet::new();
    for e in edits {
        if !e.vertices.is_empty() {
            terrain.insert(e.landblock);
        }
        if !e.cells.is_empty()
            || !e.replaces.is_empty()
            || !e.taken.is_empty()
            || !e.dungeon_copies.is_empty()
            || e.dungeon.is_some()
        {
            dungeons.insert(e.landblock);
        }
        facts.statics += e.statics.len()
            + e.moves.len()
            + e.removes.len()
            + e.cells.iter().map(|c| c.item.statics.len()).sum::<usize>()
            + e.cell_adds.len()
            + e.cell_moves.len()
            + e.cell_removes.len();
        facts.buildings +=
            e.drops.iter().map(|d| d.item.models.len()).sum::<usize>() + e.buildings.len();
        facts.rooms += e.cells.len() + e.replaces.len();
    }
    facts.terrain = terrain.len();
    facts.dungeons = dungeons.len();
}

/// The server content's share: the instances placed and the weenie classes defined.
fn count_server(packs: &[Pack], facts: &mut Facts) {
    let mut classes = BTreeSet::new();
    for file in packs.iter().flat_map(|p| &p.sql) {
        facts.spawns += crate::sql_ids::instances(&file.text).len();
        crate::sql_ids::rows(&file.text, |row| {
            if row.table == "weenie" {
                if let Some(class) = row.get("class_Id") {
                    classes.insert(class.trim().to_owned());
                }
            }
        });
    }
    facts.weenies = classes.len();
}

/// The new records' share: pictures, and models (a setup, or a mesh no setup of the world names
/// as one of its parts).
fn count_records(packs: &[Pack], facts: &mut Facts) {
    let records: Vec<&NewRecord> = packs
        .iter()
        .flat_map(|p| &p.records)
        .map(|r| &r.item)
        .collect();
    let parts: Vec<&RecordRef> = records
        .iter()
        .filter_map(|r| match r {
            NewRecord::Geometry { record, .. } => match record.as_ref() {
                Record::Setup { parts, .. } => Some(parts),
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .collect();
    let named = |target: &Target| {
        parts.iter().any(|p| match (p, target) {
            (RecordRef::Symbol(s), Target::New(t)) => s == t,
            (RecordRef::Id(id), Target::Replace(t)) => id == t,
            _ => false,
        })
    };
    for r in records {
        match r {
            NewRecord::RenderSurface { .. } => facts.textures += 1,
            NewRecord::Geometry { target, record } => match record.as_ref() {
                Record::Image { .. } => facts.textures += 1,
                Record::Setup { .. } => facts.models += 1,
                Record::Mesh { .. } if !named(target) => facts.models += 1,
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a world's changes, counted for the launcher's list of worlds)
    use super::*;
    use dereth_dat::testing::ScratchDir;

    #[test]
    fn the_sample_world_counts_its_hill_stones_house_guide_and_stripes() {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample/world.toml");
        let f = facts(&sample).unwrap();
        assert_eq!(
            f,
            Facts {
                key: "sample-world".into(),
                era: "eor",
                packs: 1,
                terrain: 1,
                statics: 6,
                buildings: 0,
                dungeons: 0,
                rooms: 0,
                spawns: 1,
                weenies: 1,
                textures: 1,
                models: 0,
            }
        );
        assert_eq!(
            f.json(),
            "{\"key\":\"sample-world\",\"era\":\"eor\",\"packs\":1,\"terrain\":1,\"statics\":6,\"buildings\":0,\"dungeons\":0,\"rooms\":0,\"spawns\":1,\"weenies\":1,\"textures\":1,\"models\":0}"
        );
    }

    /// A world of two packs: buildings dropped and moved, a dungeon built, reshaped, copied and
    /// named, statics in rooms, and terrain on one landblock edited by both packs.
    #[test]
    fn buildings_dungeons_and_rooms_are_counted_across_packs() {
        let dir = ScratchDir::new("lyceum-facts").unwrap();
        let root = dir.path();
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "world.toml",
            "key = \"made-up\"\nera = \"infiltration\"\npacks = [\"one\", \"two\"]\n",
        );
        write("one/pack.toml", "name = \"one\"\nversion = \"0.1.0\"\n");
        write(
            "one/edits/landblocks/A9B4.toml",
            r#"
[[vertex]]
at = [1, 1]
height = 20

[[add-building]]
name = "mansion"
model = [0x01001111, 0x01001112, 0x01001113, 0x01001114]
template = ["3B9C/1", "3B9C/2", "3B9C/3", "3B9C/4"]
at = [100.0, 60.0, "ground"]

[[move-building]]
index = 0
at = [90.0, 50.0, 80.0]

[[remove-static]]
index = 2
"#,
        );
        write("two/pack.toml", "name = \"two\"\nversion = \"0.1.0\"\n");
        write(
            "two/edits/landblocks/A9B4.toml",
            r#"
[[vertex]]
at = [2, 2]
height = 30
"#,
        );
        write(
            "two/edits/landblocks/0101.toml",
            r#"
[dungeon]
name = "Made-up Warren"

[[cell]]
name = "hall"
environment = 0x0D000105
surfaces = [0x080000A1]
at = [12.0, -40.0, 0.0]
portals = ["", "room:0"]

[[cell]]
name = "room"
environment = 0x0D000105
surfaces = [0x080000A1]
at = [22.0, -40.0, 0.0]
portals = ["hall:1"]
static = [{ model = 0x0200040B, at = [22.0, -40.0, 0.0] }]

[[replace-cell]]
cell = 0x0105
environment = 0x0D000105
surfaces = [0x080000A1]
at = [0.0, 0.0, 0.0]
portals = ["keep"]

[[add-cell-static]]
cell = 0x0106
model = 0x0200040B
at = [1.0, 2.0, 3.0]
"#,
        );
        write(
            "two/edits/landblocks/0202.toml",
            r#"
[[add-dungeon]]
name = "copy"
template = 0x0398
"#,
        );
        let f = facts(&root.join("world.toml")).unwrap();
        assert_eq!(f.key, "made-up");
        assert_eq!(f.era, "infiltration");
        assert_eq!(f.packs, 2);
        assert_eq!(f.terrain, 1, "one landblock, edited by both packs");
        assert_eq!(f.buildings, 5, "a set of four dropped, and one moved");
        assert_eq!(f.dungeons, 2, "one built and named, one copied");
        assert_eq!(f.rooms, 3, "two added, one reshaped");
        assert_eq!(
            f.statics, 3,
            "one removed outdoors, one added to a world's room, one in a new room"
        );
        assert_eq!((f.spawns, f.weenies, f.textures, f.models), (0, 0, 0, 0));
    }

    #[test]
    fn a_world_that_does_not_read_is_an_error() {
        let dir = ScratchDir::new("lyceum-facts-missing").unwrap();
        assert!(facts(&dir.path().join("world.toml")).is_err());
        std::fs::write(
            dir.path().join("world.toml"),
            "key = \"lost\"\npacks = [\"gone\"]\n",
        )
        .unwrap();
        assert!(facts(&dir.path().join("world.toml")).is_err());
    }

    #[test]
    fn a_key_is_written_as_a_json_string() {
        assert_eq!(json_string("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
    }
}
