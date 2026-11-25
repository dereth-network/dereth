//! The layer of a world repository being edited, as SQL files of the pack being edited: the
//! mirror. Each item the layer sets, of a kind the server's schema writes as SQL (a weenie, a
//! quest, a death or wielded treasure), is a file in the pack's `server/sql/.repository/`, the
//! item as the world at the layer holds it, viewed as SQL ([`crate::row_sql`]); so is each
//! placement, viewed as the spawn the spawn tool writes ([`crate::placement_sql`]). The world below
//! the layer stands in for the server's database ([`Mirror::below_sql`]), its placements the
//! server's own spawns.
//!
//! So every edit the editor makes of the pack's SQL works on the repository: a change of a mirror
//! file is read back, item by item, and written as the layer's rows of it in the same step
//! ([`Mirror::rows_for`]): the fields that differ from the world below, a removed field unset, and
//! for an item the files no longer hold, the layer's row of it taken away. A server's item changed
//! "in the pack" is the layer's change of it, put back "as the server has it" is the layer's row
//! taken away, and a pack's own item is one the layer makes.
//!
//! The mirror lives in the session only. It is never saved, and nothing that leaves the machine
//! (an export, a build) reads it: an editor world's export never carries a repository's items.
//!
//! It can't win against the rows: before an item is written, every file of the repository its
//! fields came from, and its layer's row file, must be on the disk as they were read (or as the
//! session last saved them). A file changed outside the editor since (another editor, a pull)
//! refuses the write, until the repository is opened again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::content::{Schema, SqlDoc};
use crate::layer_rows;
use crate::repo::{Repo, RowSchema};
use crate::row_sql::RowSql;

/// The folder of the pack's SQL the mirror's files are in.
pub const MIRROR: &str = ".repository";

/// The file of a patch's pack that takes items of the world below away (`[[remove]]`).
pub const REMOVALS: &str = "remove.toml";

/// The kind of row that places an instance in the world, viewed as a spawn.
pub const PLACEMENT: &str = "placement";

/// The first instance index of a landblock a placement the editor makes takes: the lower ones are
/// the end of retail's.
const FIRST_OWN_INDEX: u16 = 0x800;

/// An item of the world below the layer: its kind, its row, and the files its fields came from.
#[derive(Clone, Debug, PartialEq)]
struct Below {
    kind: String,
    row: toml::Table,
    from: BTreeSet<PathBuf>,
}

/// The layer of a repository being edited, as the pack's SQL.
#[derive(Clone, Debug, PartialEq)]
pub struct Mirror {
    names: Schema,
    vocabulary: RowSchema,
    stamp: Option<String>,
    /// The kinds written as SQL.
    kinds: Vec<String>,
    /// The repository's folder.
    pub repo: PathBuf,
    /// The layer: its id, and whether it is a patch.
    pub layer: String,
    pub patch: bool,
    /// The layer's pack the rows are written in: its name and folder.
    pub pack_name: String,
    pub pack: PathBuf,
    /// The folder of the mirror's files.
    pub dir: PathBuf,
    below: BTreeMap<String, Below>,
    /// The item each mirror file holds now, by file.
    held: BTreeMap<PathBuf, BTreeSet<String>>,
    /// The file of the layer's pack that writes each item.
    files: BTreeMap<String, PathBuf>,
    /// Every file of the repository's layers as read.
    read: BTreeMap<PathBuf, String>,
    /// The classes the repository numbers its own weenies from (`[ids] classes`).
    pub classes: Option<(u32, u32)>,
}

/// What a session tells the mirror of a file: its text now, and what the disk had when the
/// session last read or saved it (`None`: a file the session doesn't hold).
pub trait Files {
    fn now(&mut self, path: &Path) -> Option<String>;
    fn saved(&mut self, path: &Path) -> Option<Option<String>>;
}

impl Mirror {
    /// The mirror of `repo`'s layer `layer` (its last pack), the pack's SQL in `sql_dir`, with the
    /// server's `names` (its `rows` among them).
    ///
    /// # Errors
    /// A layer with no pack, a schema with no rows, or a world that doesn't fold.
    pub fn new(repo: &Repo, names: &Schema, layer: usize, sql_dir: &Path) -> Result<Self, String> {
        let rows = names
            .rows
            .as_ref()
            .ok_or("the server's schema has no rows")?;
        let vocabulary = RowSchema::of_schema(&serde_json::json!({ "rows": rows }))?
            .ok_or("the server's schema has no rows")?;
        let mut kinds: Vec<String> = rows
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, k)| k.get("sql").is_some())
            .map(|(n, _)| n.clone())
            .collect();
        if vocabulary.kinds.contains_key(PLACEMENT) {
            kinds.push(PLACEMENT.to_owned());
        }
        let l = repo
            .layers
            .get(layer)
            .ok_or_else(|| format!("no layer {layer}"))?;
        let at = l.packs.len().checked_sub(1).ok_or_else(|| {
            format!(
                "{}: the layer has no pack to write in; New Patch makes a patch with one",
                l.id
            )
        })?;
        let pack = &l.packs[at];
        let below_world = repo.fold_below(&vocabulary, layer, at)?;
        // Every file of the layers up to this one as read, and the files below the pack that
        // write each item.
        let mut read = BTreeMap::new();
        let mut from: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
        for (n, l) in repo.layers.iter().take(layer + 1).enumerate() {
            for (i, p) in l.packs.iter().enumerate() {
                for (rel, text) in &p.files {
                    let path = p.dir.join(rel);
                    read.insert(path.clone(), text.clone());
                    if n == layer && i == at {
                        continue;
                    }
                    let Ok(t) = toml::from_str::<toml::Table>(text) else {
                        continue;
                    };
                    for (kind, rows) in t.iter().filter(|(k, _)| kinds.contains(k)) {
                        for r in rows.as_array().into_iter().flatten() {
                            if let Some(item) = r
                                .as_table()
                                .and_then(|r| crate::repo::item_of(&vocabulary, kind, r))
                            {
                                from.entry(item).or_default().insert(path.clone());
                            }
                        }
                    }
                }
            }
        }
        let mut below = BTreeMap::new();
        for (name, item) in &below_world.items {
            if !kinds.contains(&item.kind) {
                continue;
            }
            below.insert(
                name.clone(),
                Below {
                    kind: item.kind.clone(),
                    row: item.row(&vocabulary),
                    from: from.remove(name).unwrap_or_default(),
                },
            );
        }
        let mut files = BTreeMap::new();
        for (rel, text) in &pack.files {
            let Ok(t) = toml::from_str::<toml::Table>(text) else {
                continue;
            };
            for (kind, rows) in &t {
                for r in rows.as_array().into_iter().flatten() {
                    if let Some(item) = r
                        .as_table()
                        .and_then(|r| crate::repo::item_of(&vocabulary, kind, r))
                    {
                        files.insert(item, pack.dir.join(rel));
                    }
                }
            }
        }
        let classes = std::fs::read_to_string(&repo.path)
            .ok()
            .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
            .and_then(|t| {
                let c = t.get("ids")?.get("classes")?.as_array()?;
                let n = |i: usize| c.get(i)?.as_integer().and_then(|n| u32::try_from(n).ok());
                Some((n(0)?, n(1)?))
            });
        Ok(Self {
            names: names.clone(),
            vocabulary,
            stamp: repo.stamp.clone(),
            kinds,
            repo: repo
                .path
                .parent()
                .map_or_else(PathBuf::new, Path::to_path_buf),
            layer: l.id.clone(),
            patch: layer > 0,
            pack_name: pack.name.clone(),
            pack: pack.dir.clone(),
            dir: sql_dir.join(MIRROR),
            below,
            held: BTreeMap::new(),
            files,
            read,
            classes,
        })
    }

    fn view(&self, kind: &str) -> Result<RowSql<'_>, String> {
        RowSql::new(&self.names, kind, self.stamp.as_deref())
    }

    /// Row `row` of `kind` viewed as SQL.
    fn render(&self, kind: &str, row: &toml::Table) -> Result<String, String> {
        if kind == PLACEMENT {
            return crate::placement_sql::to_sql(
                row,
                self.stamp.as_deref().unwrap_or("2000-01-01 00:00:00"),
            );
        }
        Ok(self.view(kind)?.to_sql(row)?.render())
    }

    /// The rows of `kind` SQL `text` holds, as its view reads them back.
    fn read_rows(&self, kind: &str, text: &str) -> Result<Vec<toml::Table>, String> {
        if kind == PLACEMENT {
            return crate::placement_sql::rows(text);
        }
        self.view(kind)?.rows(&SqlDoc::parse(text))
    }

    /// The vocabulary of rows the mirror reads.
    #[must_use]
    pub fn vocabulary(&self) -> &RowSchema {
        &self.vocabulary
    }

    /// The mirror file of item `item` of `kind`.
    #[must_use]
    pub fn file_of(&self, kind: &str, item: &str) -> PathBuf {
        self.dir
            .join(kind)
            .join(format!("{}.sql", dereth_pack::records::file_safe(item)))
    }

    /// Whether `path` is one of the mirror's files.
    #[must_use]
    pub fn holds(&self, path: &Path) -> bool {
        path.starts_with(&self.dir)
    }

    /// The items of the kinds written as SQL that SQL `text` holds: each one's name, kind and row.
    fn items(&self, text: Option<&str>) -> Result<Vec<(String, String, toml::Table)>, String> {
        let Some(text) = text else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for kind in &self.kinds {
            for row in self.read_rows(kind, text)? {
                let item = crate::repo::item_of(&self.vocabulary, kind, &row)
                    .ok_or_else(|| format!("a {kind} its rows don't name"))?;
                out.push((item, kind.clone(), row));
            }
        }
        Ok(out)
    }

    /// The mirror's files, as the world at the layer holds each item the layer sets, read from
    /// `repo`; they are the files the mirror holds from now.
    ///
    /// # Errors
    /// A world that doesn't fold, or an item that doesn't view.
    pub fn files(&mut self, repo: &Repo) -> Result<Vec<(PathBuf, String)>, String> {
        let at = repo
            .layers
            .iter()
            .position(|l| l.id == self.layer)
            .ok_or("the layer is gone")?;
        let world = repo.fold(&self.vocabulary, at)?;
        let mut out = Vec::new();
        self.held.clear();
        for item in self.files.keys() {
            let Some(shown) = world.items.get(item) else {
                continue;
            };
            if !self.kinds.contains(&shown.kind) {
                continue;
            }
            let row = shown.row(&self.vocabulary);
            let text = self.render(&shown.kind, &row)?;
            let path = self.file_of(&shown.kind, item);
            self.held
                .entry(path.clone())
                .or_default()
                .insert(item.clone());
            out.push((path, text));
        }
        Ok(out)
    }

    /// The world below the layer as the server's SQL: every item of the kinds written as SQL.
    ///
    /// # Errors
    /// An item that doesn't view.
    pub fn below_sql(&self) -> Result<String, String> {
        let mut out = String::new();
        for b in self.below.values() {
            out.push_str(&self.render(&b.kind, &b.row)?);
            out.push('\n');
        }
        Ok(out)
    }

    /// Item `item` as the world below the layer holds it, as SQL; `None` for one it lacks.
    #[must_use]
    pub fn below_text(&self, item: &str) -> Option<String> {
        let b = self.below.get(item)?;
        self.render(&b.kind, &b.row).ok()
    }

    /// The file of the layer's pack that writes item `item`'s row, when one does.
    #[must_use]
    pub fn row_file(&self, item: &str) -> Option<&Path> {
        self.files.get(item).map(PathBuf::as_path)
    }

    /// The weenie classes the mirror's files `files` (each one's path and text) hold: the
    /// layer's own, as the session has them.
    #[must_use]
    pub fn classes_in<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a Path, &'a str)>,
    ) -> BTreeSet<u32> {
        files
            .into_iter()
            .filter(|(p, _)| p.starts_with(self.dir.join("weenie")))
            .filter_map(|(_, t)| self.items(Some(t)).ok())
            .flatten()
            .filter(|(_, kind, _)| kind == "weenie")
            .filter_map(|(_, _, r)| r.get("class")?.as_integer())
            .filter_map(|c| u32::try_from(c).ok())
            .collect()
    }

    /// Whether the world below the layer has item `item`.
    #[must_use]
    pub fn below_has(&self, item: &str) -> bool {
        self.below.contains_key(item)
    }

    /// Item `item` as the world below holds it, read back as its view reads it (so a default a
    /// row writes out reads as left out): what the layer's row is the change of.
    fn below_read(&self, item: &str) -> Result<Option<toml::Table>, String> {
        let Some(b) = self.below.get(item) else {
            return Ok(None);
        };
        if b.kind == PLACEMENT {
            return Ok(self
                .read_rows(PLACEMENT, &self.render(PLACEMENT, &b.row)?)?
                .pop());
        }
        let v = self.view(&b.kind)?;
        v.to_row(&v.to_sql(&b.row)?).map(Some)
    }

    /// The first class of the repository's own numbers (`[ids] classes`) that no item of the world
    /// below, of the layer, or of `taken`, has.
    #[must_use]
    pub fn free_class(&self, taken: &BTreeSet<u32>) -> Option<u32> {
        let (lo, hi) = self.classes?;
        let used: BTreeSet<u32> = self
            .below
            .keys()
            .chain(self.files.keys())
            .filter_map(|i| i.strip_prefix("weenie ")?.parse().ok())
            .collect();
        (lo..=hi).find(|c| !used.contains(c) && !taken.contains(c))
    }

    /// The item of the placement whose guid is `guid`, as the rows name it.
    #[must_use]
    pub fn spawn_item(&self, guid: u32) -> Option<String> {
        let mut row = toml::Table::new();
        row.insert("guid".into(), toml::Value::Integer(guid.into()));
        crate::repo::item_of(&self.vocabulary, PLACEMENT, &row)
    }

    /// Whether the mirror views placements: the server's rows have them.
    #[must_use]
    pub fn places(&self) -> bool {
        self.kinds.iter().any(|k| k == PLACEMENT)
    }

    /// The guids of the placements of the world below the layer and of the layer.
    fn placed_guids(&self) -> BTreeSet<u32> {
        let mut out: BTreeSet<u32> = self
            .below
            .values()
            .filter(|b| b.kind == PLACEMENT)
            .filter_map(|b| b.row.get("guid")?.as_integer())
            .filter_map(|g| u32::try_from(g).ok())
            .collect();
        out.extend(self.files.keys().filter_map(|item| {
            let g = item.strip_prefix("spawn 0x")?;
            u32::from_str_radix(g, 16).ok()
        }));
        out
    }

    /// The guid a new placement in landblock `lb` takes: the first of the landblock's own
    /// instances, from index 0x800, that no placement of the world below, of the layer or of
    /// `taken` has.
    #[must_use]
    pub fn free_guid(&self, lb: u16, taken: &BTreeSet<u32>) -> Option<u32> {
        let used = self.placed_guids();
        (FIRST_OWN_INDEX..=0xFFF)
            .map(|i| crate::placement_sql::guid_of(lb, i))
            .find(|g| !used.contains(g) && !taken.contains(g))
    }

    /// A step's changed files, `changed`, with the layer's row files its mirror files' changes
    /// make: each item a changed mirror file holds written as the layer's row of what differs from
    /// the world below, and each item they no longer hold taken out of the layer. `files` tells
    /// each file's text now and as last saved.
    ///
    /// # Errors
    /// A file of the repository changed on the disk since it was read (open the repository again),
    /// or an item that doesn't read back.
    pub fn rows_for(
        &mut self,
        changed: &BTreeMap<PathBuf, Option<String>>,
        files: &mut dyn Files,
    ) -> Result<BTreeMap<PathBuf, Option<String>>, String> {
        let mut before: BTreeMap<String, String> = BTreeMap::new();
        let mut after: BTreeMap<String, (String, toml::Table)> = BTreeMap::new();
        let mut held = Vec::new();
        for (path, text) in changed.iter().filter(|(p, _)| self.holds(p)) {
            for (item, kind, _) in self.items(files.now(path).as_deref())? {
                before.insert(item, kind);
            }
            let now = self.items(text.as_deref())?;
            held.push((
                path.clone(),
                now.iter().map(|(i, _, _)| i.clone()).collect(),
            ));
            for (item, kind, row) in now {
                after.insert(item, (kind, row));
            }
        }
        let gone: Vec<(String, String)> = before
            .into_iter()
            .filter(|(i, _)| !after.contains_key(i))
            .collect();
        for item in after.keys().chain(gone.iter().map(|(i, _)| i)) {
            self.fresh(item, files)?;
        }
        let mut out: BTreeMap<PathBuf, Option<String>> = BTreeMap::new();
        let mut text_of = |out: &BTreeMap<PathBuf, Option<String>>, f: &Path| match out.get(f) {
            Some(t) => t.clone(),
            None => files.now(f),
        };
        for (item, (kind, row)) in &after {
            let below = self.below_read(item)?;
            let mut row = self.times_kept(kind, below.as_ref(), row);
            if kind == PLACEMENT {
                row = crate::placement_sql::links_as_below(&row, below.as_ref());
            }
            let delta = layer_rows::delta(&self.vocabulary, kind, below.as_ref(), &row);
            let file = match self.files.get(item) {
                Some(f) => f.clone(),
                None => self
                    .pack
                    .join(layer_rows::file_of(&self.vocabulary, kind, &row)?),
            };
            let text = text_of(&out, &file);
            let new = layer_rows::write_row(
                &self.vocabulary,
                kind,
                text.as_deref(),
                item,
                delta.as_ref(),
            )?;
            out.insert(file.clone(), new);
            self.files.insert(item.clone(), file);
        }
        for (item, kind) in &gone {
            if let Some(file) = self.files.get(item).cloned() {
                let text = text_of(&out, &file);
                let new =
                    layer_rows::write_row(&self.vocabulary, kind, text.as_deref(), item, None)?;
                out.insert(file, new);
            }
        }
        for (path, items) in held {
            self.held.insert(path, items);
        }
        // A file left as it was is no change.
        out.retain(|f, t| files.now(f) != *t);
        Ok(out)
    }

    /// Row `row` of `kind` with its times (a `time` field, and a list entry's) as the world below
    /// has them, at the same place in a list, or left out: the time the editor stamps on what it
    /// writes is no change of the item.
    fn times_kept(
        &self,
        kind: &str,
        below: Option<&toml::Table>,
        row: &toml::Table,
    ) -> toml::Table {
        let Some(d) = self.vocabulary.kinds.get(kind).map(|k| &k.described) else {
            return row.clone();
        };
        let times = |fields: &serde_json::Value| -> Vec<String> {
            fields
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(_, t)| t.as_str() == Some("time"))
                .map(|(f, _)| f.clone())
                .collect()
        };
        let keep = |out: &mut toml::Table, was: Option<&toml::Table>, fields: &[String]| {
            for f in fields {
                match was.and_then(|w| w.get(f)) {
                    Some(v) => {
                        out.insert(f.clone(), v.clone());
                    }
                    None => {
                        out.remove(f);
                    }
                }
            }
        };
        let mut out = row.clone();
        keep(&mut out, below, &times(&d["fields"]));
        for (list, ld) in d["lists"].as_object().into_iter().flatten() {
            let fields = times(&ld["fields"]);
            if fields.is_empty() {
                continue;
            }
            let was = below
                .and_then(|b| b.get(list))
                .and_then(toml::Value::as_array);
            if let Some(toml::Value::Array(entries)) = out.get_mut(list) {
                for (i, e) in entries.iter_mut().enumerate() {
                    if let toml::Value::Table(t) = e {
                        let w = was.and_then(|w| w.get(i)).and_then(toml::Value::as_table);
                        keep(t, w, &fields);
                    }
                }
            }
        }
        out
    }

    /// Refused when a file item `item` was read from (the world below's, or its layer's row file)
    /// is no longer on the disk as read or last saved.
    fn fresh(&self, item: &str, files: &mut dyn Files) -> Result<(), String> {
        let mut paths: Vec<&Path> = self
            .below
            .get(item)
            .map(|b| b.from.iter().map(PathBuf::as_path).collect())
            .unwrap_or_default();
        if let Some(f) = self.files.get(item) {
            paths.push(f);
        }
        for p in paths {
            let disk = std::fs::read_to_string(p).ok();
            let expected = match files.saved(p) {
                Some(saved) => saved,
                None => self.read.get(p).cloned(),
            };
            if disk != expected {
                return Err(format!(
                    "{}: changed on the disk since the repository was opened; open it again",
                    p.display()
                ));
            }
        }
        Ok(())
    }
}
