//! A checkpoint: a known world pinned to the release it comes from, converted to rows when a fold
//! needs it, never stored.
//!
//! ```toml
//! # patches/2005-02-infiltration/checkpoint.toml
//! world = "16py"                 # the world line its release is of, as `empyrean-import fetch --world` names it
//! source = "ACE-World-16PY"      # the project, as its rows' provenance names it
//! revision = "v0.8.8"            # the release
//! url = "https://github.com/ACEmulator/ACE-World-16PY/releases/download/v0.8.8/ACE-World-16PY-db-v0.8.8.sql.zip"
//! sha256 = "..."                 # the release's zip
//! dataset_id = "..."             # the dump's: what the dump inside it must hash to
//! ```
//!
//! The dump is read from a folder of dumps the caller names (`--dumps`, else the [`DUMPS_VAR`]
//! variable), else from the per-user cache `empyrean-import fetch --world <world>` fills; it must be
//! the one the pin names ([`Pin::dataset_id`]). A pin never names a folder: it is committed, and
//! where a machine keeps its dumps is the machine's. Its world becomes rows as a base's are, each
//! said to come from the pinned source.
//!
//! **Keys.** A placement's guid is its source's, and two sources mean different things by one guid.
//! So a checkpoint's placements are given the lineage's keys ([`match_keys`]):
//!
//! 1. a placement of the world before it with the same guid, class and cell, within 0.5 m, is the
//!    same placement;
//! 2. else the nearest one of the same class in the same cell, within 2 m, not taken already;
//! 3. else it is new: it keeps its guid when the lineage has never used it, and otherwise takes a
//!    free guid in its landblock.
//!
//! The keys that differ from the source's guids are kept beside the pin, in `keys.toml`, so the
//! same pin always makes the same rows; a guid it doesn't name keeps itself.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use empyrean_common::era::EraId;
use empyrean_common::world_release::WorldLine;
use empyrean_content::models::world::LandblockInstance;
use empyrean_content::pack::Pack;
use empyrean_content::WorldContent;

use crate::canon::{from_pack, number};
use crate::value::R;

/// What a checkpoint pins, read from its `checkpoint.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// The world line its release is of.
    pub world: &'static WorldLine,
    /// The project, as its rows' provenance names it.
    pub source: String,
    /// The release.
    pub tag: String,
    /// The dump's dataset id, lowercase hex: the first 16 bytes of its BLAKE3.
    pub dataset_id: String,
}

/// The checkpoint a layer's pin names.
///
/// # Errors
/// A pin of no world line this build knows, or whose address or digest is not that release's.
pub fn pin_of(p: &dereth_pack::lineage::Pin) -> R<Pin> {
    let word = |k: &str| p.rest.get(k).and_then(toml::Value::as_str);
    let id = word("world").ok_or("checkpoint.toml: world, the world line the release is of")?;
    let world = WorldLine::by_id(id)
        .ok_or_else(|| format!("checkpoint.toml: no world line is called {id:?}"))?;
    let tag = p.revision.clone();
    let url = world.download_url(&tag);
    if p.url.as_deref() != Some(url.as_str()) {
        return Err(format!("checkpoint.toml: url is {url} for {id} {tag}"));
    }
    if world.pinned_tag == tag && p.sha256.as_deref() != Some(world.pinned_zip_sha256) {
        return Err(format!(
            "checkpoint.toml: sha256 is {} for {id} {tag}",
            world.pinned_zip_sha256
        ));
    }
    let dataset_id = word("dataset_id")
        .ok_or("checkpoint.toml: dataset_id, the dump's")?
        .to_owned();
    Ok(Pin {
        world,
        source: p.source.clone(),
        tag,
        dataset_id,
    })
}

/// The variable naming a folder of dumps, when a command is given none.
pub const DUMPS_VAR: &str = "EMPYREAN_WORLD_DUMPS";

impl Pin {
    /// The pinned dump: in `dumps` when given, else in the folder [`DUMPS_VAR`] names, else in the
    /// per-user cache.
    ///
    /// # Errors
    /// No such dump where it is looked for.
    pub fn dump(&self, dumps: Option<&Path>) -> R<PathBuf> {
        let name = self.world.sql_name(&self.tag);
        let folder = dumps
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os(DUMPS_VAR).map(PathBuf::from));
        if let Some(dir) = folder {
            let path = dir.join(&name);
            return if path.is_file() {
                Ok(path)
            } else {
                Err(format!("{name} is not in {}", dir.display()))
            };
        }
        self.world.cached_sql(&self.tag).ok_or_else(|| {
            format!(
                "{name} is not in the world-database cache: run `empyrean-import fetch --world {} --version {}`, or name a folder of dumps (--dumps, or {DUMPS_VAR})",
                self.world.id,
                self.tag
            )
        })
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The world of the dump at `path`, numbered, checked to be the one `pin` names.
///
/// # Errors
/// A dump that doesn't read or import, or another one than the pin's.
pub fn ingest(pin: &Pin, path: &Path, era: EraId) -> R<WorldContent> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (bytes, _) = empyrean_content::import::import_for(file, era)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let pack = Pack::from_bytes(bytes).map_err(|e| e.to_string())?;
    let id = hex(&pack.header().dataset_id);
    if id != pin.dataset_id {
        return Err(format!(
            "{}: its dataset id is {id}, and the pin names {}",
            path.display(),
            pin.dataset_id
        ));
    }
    let mut c = from_pack(&pack).map_err(|e| e.to_string())?;
    number(&mut c);
    Ok(c)
}

/// A placement of the world before a checkpoint: its guid in its own source, its key in the
/// lineage, and what it is where.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Known {
    pub source: u32,
    pub key: u32,
    pub class: u32,
    pub cell: u32,
    pub at: [f32; 3],
}

impl Known {
    /// The placements of `c`, each its own key: a world whose guids are the lineage's.
    #[must_use]
    pub fn of(c: &WorldContent) -> Vec<Self> {
        c.landblock_instances
            .iter()
            .map(|i| Self::keyed(i, i.guid))
            .collect()
    }

    /// `i`, its guid in its source, keyed `key` in the lineage.
    #[must_use]
    pub fn keyed(i: &LandblockInstance, key: u32) -> Self {
        Self {
            source: i.guid,
            key,
            class: i.weenie_class_id,
            cell: i.obj_cell_id,
            at: [i.origin_x, i.origin_y, i.origin_z],
        }
    }
}

/// How each of a checkpoint's placements was keyed, counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Matched {
    /// The same guid, class, cell and spot.
    pub same: usize,
    /// The same class near the same spot.
    pub near: usize,
    /// New, its own guid kept.
    pub new_kept: usize,
    /// New, given a free guid of its landblock.
    pub new_renumbered: usize,
}

/// A checkpoint's placements keyed: each key that differs from its source's guid, by that guid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    pub map: BTreeMap<u32, u32>,
    pub matched: Matched,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d: Vec<f32> = (0..3).map(|i| a[i] - b[i]).collect();
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The landblock a placement's guid is numbered in (`0x7LLLLnnn`).
const fn landblock_of(guid: u32) -> u32 {
    (guid >> 12) & 0xFFFF
}

/// Key `source`'s placements against `known`, the world before the checkpoint, `used` every key
/// the lineage has given ([`Keys`]'s rules, in the module's words).
#[must_use]
pub fn match_keys(known: &[Known], used: &BTreeSet<u32>, source: &[LandblockInstance]) -> Keys {
    let mut by_guid: BTreeMap<u32, usize> = BTreeMap::new();
    let mut by_cell: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, k) in known.iter().enumerate() {
        by_guid.insert(k.source, i);
        by_cell.entry((k.class, k.cell)).or_default().push(i);
    }
    let mut taken = vec![false; known.len()];
    let mut key: BTreeMap<u32, u32> = BTreeMap::new();
    let mut matched = Matched::default();
    // The same placement first, so a nearer stranger never takes its key.
    for p in source {
        if let Some(&i) = by_guid.get(&p.guid) {
            let k = &known[i];
            if !taken[i]
                && k.class == p.weenie_class_id
                && k.cell == p.obj_cell_id
                && distance(k.at, [p.origin_x, p.origin_y, p.origin_z]) <= 0.5
            {
                taken[i] = true;
                key.insert(p.guid, k.key);
                matched.same += 1;
            }
        }
    }
    // Then the nearest pairs of the same class in the same cell, nearest first.
    let mut pairs: Vec<(f32, u32, usize)> = Vec::new();
    for p in source.iter().filter(|p| !key.contains_key(&p.guid)) {
        let at = [p.origin_x, p.origin_y, p.origin_z];
        for &i in by_cell
            .get(&(p.weenie_class_id, p.obj_cell_id))
            .into_iter()
            .flatten()
        {
            let d = distance(known[i].at, at);
            if !taken[i] && d <= 2.0 {
                pairs.push((d, p.guid, i));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, guid, i) in pairs {
        if !taken[i] && !key.contains_key(&guid) {
            taken[i] = true;
            key.insert(guid, known[i].key);
            matched.near += 1;
        }
    }
    let mut given: BTreeSet<u32> = used.iter().copied().chain(key.values().copied()).collect();
    // A source guid the lineage has never used keeps itself; the rest take free guids.
    let strangers: Vec<&LandblockInstance> = source
        .iter()
        .filter(|p| !key.contains_key(&p.guid))
        .collect();
    let own: BTreeSet<u32> = source.iter().map(|p| p.guid).collect();
    for p in &strangers {
        if !given.contains(&p.guid) {
            given.insert(p.guid);
            key.insert(p.guid, p.guid);
            matched.new_kept += 1;
        }
    }
    for p in strangers {
        if key.contains_key(&p.guid) {
            continue;
        }
        let block = 0x7000_0000 | (landblock_of(p.guid) << 12);
        let free = (0..=0xFFFu32)
            .rev()
            .map(|n| block | n)
            .find(|g| !given.contains(g) && !own.contains(g));
        let fresh = free.unwrap_or(p.guid);
        given.insert(fresh);
        key.insert(p.guid, fresh);
        matched.new_renumbered += 1;
    }
    Keys {
        map: key.into_iter().filter(|(s, k)| s != k).collect(),
        matched,
    }
}

/// `c`'s placements and their links given the keys `map` names; a guid it doesn't name keeps itself.
pub fn apply_keys(c: &mut WorldContent, map: &BTreeMap<u32, u32>) {
    let key = |g: u32| map.get(&g).copied().unwrap_or(g);
    for i in &mut c.landblock_instances {
        i.guid = key(i.guid);
        for l in &mut i.landblock_instance_link {
            l.parent_guid = i.guid;
            l.child_guid = key(l.child_guid);
        }
    }
    number(c);
}

/// `keys` as `keys.toml`: each key that differs from its source's guid, one line each, in order.
#[must_use]
pub fn keys_text(keys: &Keys, pin: &Pin) -> String {
    let m = keys.matched;
    let mut out = format!(
        "# The lineage's keys for {} {}'s placements, where they differ from its guids; generated by\n\
         # `empyrean-world-db checkpoints --keys`. A guid not listed keeps itself.\n\
         format = 1\n\
         # How its placements were matched: the same guid, class and spot; the same class near the same\n\
         # spot; new, keeping their guid; new, renumbered.\n\
         matched = {{ same = {}, near = {}, new_kept = {}, new_renumbered = {} }}\n\
         keys = [\n",
        pin.source, pin.tag, m.same, m.near, m.new_kept, m.new_renumbered
    );
    for (s, k) in &keys.map {
        out.push_str(&format!("  [{s:#010X}, {k:#010X}],\n"));
    }
    out.push_str("]\n");
    out
}

/// The keys a `keys.toml` names, and how it says they were matched.
///
/// # Errors
/// Text that isn't such a file, or a guid given two keys or two guids one.
pub fn keys_of(text: &str) -> R<Keys> {
    let t: toml::Table = toml::from_str(text).map_err(|e| format!("keys.toml: {e}"))?;
    let count = |k: &str| {
        t.get("matched")
            .and_then(|m| m.get(k))
            .and_then(toml::Value::as_integer)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0)
    };
    let matched = Matched {
        same: count("same"),
        near: count("near"),
        new_kept: count("new_kept"),
        new_renumbered: count("new_renumbered"),
    };
    let rows = t
        .get("keys")
        .and_then(toml::Value::as_array)
        .ok_or("keys.toml: keys, a list of [guid, key]")?;
    let mut map = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for r in rows {
        let pair = r
            .as_array()
            .filter(|p| p.len() == 2)
            .and_then(|p| Some((p[0].as_integer()?, p[1].as_integer()?)))
            .and_then(|(a, b)| Some((u32::try_from(a).ok()?, u32::try_from(b).ok()?)))
            .ok_or("keys.toml: each key is [guid, key]")?;
        if map.insert(pair.0, pair.1).is_some() || !keys.insert(pair.1) {
            return Err(format!(
                "keys.toml: {:#010X} or {:#010X} is listed twice",
                pair.0, pair.1
            ));
        }
    }
    Ok(Keys { map, matched })
}

/// What one snapshot changed of another's items of one kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KindChange {
    /// Items the later has and the earlier hasn't.
    pub added: usize,
    /// Items the earlier has and the later hasn't.
    pub removed: usize,
    /// Items both have, with some field other than `last_modified` set otherwise.
    pub changed: usize,
    /// Fields other than `last_modified` set otherwise, added or taken away, of those items.
    pub fields: usize,
    /// Items both have that differ only in `last_modified`.
    pub restamped: usize,
    /// The fields set otherwise, by name (a map's key under its table, `int.*`), and how often.
    pub by_field: BTreeMap<String, usize>,
}

/// The kind of item `name` is: the word before its key.
fn kind_word(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}

/// What `next` changed of `prev`, item by item, by the kind of item (`weenie`, `spawn`, ...).
#[must_use]
pub fn change_by_kind(
    prev: &dereth_pack::fold::Snapshot<toml::Value>,
    next: &dereth_pack::fold::Snapshot<toml::Value>,
) -> BTreeMap<String, KindChange> {
    let mut out: BTreeMap<String, KindChange> = BTreeMap::new();
    for e in next.iter() {
        let c = out.entry(kind_word(&e.item.name).to_owned()).or_default();
        let Some(p) = prev.get(&e.item.name) else {
            c.added += 1;
            continue;
        };
        let names: BTreeSet<&String> = e.fields.keys().chain(p.fields.keys()).collect();
        let differ: Vec<&&String> = names
            .iter()
            .filter(|f| e.fields.get(**f).map(|x| &x.value) != p.fields.get(**f).map(|x| &x.value))
            .collect();
        let real = differ
            .iter()
            .filter(|f| !f.ends_with("last_modified"))
            .count();
        for f in differ.iter().filter(|f| !f.ends_with("last_modified")) {
            let name = match f.split_once('.') {
                Some((table, _)) => format!("{table}.*"),
                None => (**f).clone(),
            };
            *c.by_field.entry(name).or_default() += 1;
        }
        if real > 0 {
            c.changed += 1;
            c.fields += real;
        } else if !differ.is_empty() {
            c.restamped += 1;
        }
    }
    for p in prev.iter().filter(|p| next.get(&p.item.name).is_none()) {
        out.entry(kind_word(&p.item.name).to_owned())
            .or_default()
            .removed += 1;
    }
    out
}

/// How many items of each kind a snapshot has.
#[must_use]
pub fn count_by_kind(s: &dereth_pack::fold::Snapshot<toml::Value>) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for e in s.iter() {
        *out.entry(kind_word(&e.item.name).to_owned()).or_default() += 1;
    }
    out
}
