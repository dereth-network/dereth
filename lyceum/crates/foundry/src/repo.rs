//! A world repository, read: a world of ordered layers (a base and dated patches, as Empyrean's
//! world repositories hold them), its rows folded at any layer, each field with the layer that set
//! it and where it came from.
//!
//! The rows are the server's, and Lyceum knows no game's words for them: the vocabulary comes from
//! the server's tools at run time, as `empyrean-import schema`'s `rows` member ([`RowSchema`]):
//! each kind of row, the item it sets (`spawn {guid:#010X}`), and which of its tables fold key by
//! key. The fold itself is the shared pack format's (`dereth_pack::fold`).
//!
//! A repository's `world.toml` is read as its lineage ([`dereth_pack::lineage`]): `packs` (the
//! base's, in order) and `lineage` (the patches, oldest first). A pack holds its rows at its root
//! (`weenie/00509.toml`, `landblock/A9B4.toml`, ...); its header (`pack.toml`), the rows' meta file
//! (`world-db.toml`) and the world builder's own folders (`records/`, `edits/`, `art/`) are not
//! rows. A checkpoint layer is a whole world pinned to a release of another project, read from that
//! release's dump, which the world builder doesn't read: the world is shown up to the layer before
//! the first checkpoint.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use dereth_pack::conflict::Item;
use dereth_pack::fold::{self, Input, Snapshot};
use dereth_pack::lineage::Lineage;
use dereth_pack::provenance::Provenance;
use dereth_pack::records::{self, Kinds, RowKinds};
use dereth_pack::store::DiskStore;
use dereth_pack::symbol::{Numbering, SymbolKind};
use dereth_pack::{Dialect, Loc};
use dereth_primitives::EraId;

/// A pack's folders the world builder keeps for itself, which hold no rows.
pub const BUILDER_FOLDERS: [&str; 3] = ["records", "edits", "art"];

/// The files of a pack that hold no rows: its header and the rows' meta file.
const NOT_ROWS: [&str; 2] = ["pack.toml", "world-db.toml"];

/// One kind of row, as the schema describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct RowKind {
    /// The item a row sets, as `dereth_pack::records::item_from` makes it from the row.
    pub item: String,
    /// The file a row lives in (`weenie/{class:05}.toml`).
    pub file: String,
    /// The tables that fold key by key: its maps and its tables.
    pub folds: BTreeSet<String>,
    /// The kind as the schema describes it: its fields, maps, tables and lists, with their types.
    pub described: serde_json::Value,
}

/// The rows' vocabulary: `empyrean-import schema`'s `rows` member.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowSchema {
    pub kinds: BTreeMap<String, RowKind>,
}

impl RowSchema {
    /// The vocabulary in the schema document `text`, as `empyrean-import schema` writes it; `None`
    /// when it has no `rows` member.
    ///
    /// # Errors
    /// Text that isn't a schema document.
    pub fn read(text: &str) -> Result<Option<Self>, String> {
        let doc: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Self::of_schema(&doc)
    }

    /// The vocabulary in a schema document (`empyrean-import schema`'s JSON); `None` when it has
    /// no `rows` member (a server too old to say).
    ///
    /// # Errors
    /// A `rows` member that doesn't read.
    pub fn of_schema(schema: &serde_json::Value) -> Result<Option<Self>, String> {
        let Some(rows) = schema.get("rows") else {
            return Ok(None);
        };
        let rows = rows
            .as_object()
            .ok_or("the schema's rows are not a table")?;
        let mut kinds = BTreeMap::new();
        for (name, k) in rows {
            let text = |f: &str| {
                k.get(f)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("row kind {name}: no {f}"))
            };
            let mut folds = BTreeSet::new();
            for part in ["maps", "tables"] {
                if let Some(m) = k.get(part).and_then(serde_json::Value::as_object) {
                    folds.extend(m.keys().cloned());
                }
            }
            kinds.insert(
                name.clone(),
                RowKind {
                    item: text("item")?,
                    file: text("file")?,
                    folds,
                    described: k.clone(),
                },
            );
        }
        Ok(Some(Self { kinds }))
    }
}

impl RowKinds for RowSchema {
    fn item(&self, kind: &str, row: &toml::Table) -> Result<Item, String> {
        let k = self
            .kinds
            .get(kind)
            .ok_or_else(|| format!("no kind of row is called {kind}"))?;
        records::item_from(&k.item, row).map(Item::new)
    }

    fn is_map(&self, kind: &str, field: &str) -> bool {
        self.kinds
            .get(kind)
            .is_some_and(|k| k.folds.contains(field))
    }

    fn removal(&self, text: &str) -> Result<String, String> {
        self.kinds
            .values()
            .find_map(|k| records::item_in_form(&k.item, text))
            .ok_or_else(|| format!("{text:?} names no item"))
    }
}

/// The item `text` names, in its one form: as a kind of `schema` reads it.
///
/// # Errors
/// Text that names no item.
pub fn item_name(schema: &RowSchema, text: &str) -> Result<String, String> {
    schema.removal(text)
}

/// A repository's symbols: its own weenie classes, which its `[ids]` places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RepoKind {
    Class,
}

impl fmt::Display for RepoKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_name())
    }
}

impl SymbolKind for RepoKind {
    const KINDS: &'static [Self] = &[Self::Class];

    fn kind_name(self) -> &'static str {
        "class"
    }

    fn numbering(self) -> Numbering {
        Numbering::Counted {
            start: 1_000_000,
            world_sets: true,
        }
    }

    fn written_in_hex(self) -> bool {
        false
    }
}

/// A world repository's words for the shared pack format, as the world builder reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Repository;

impl Dialect for Repository {
    const GAME: &'static str = dereth_pack::DEFAULT_GAME;
    const PRODUCT: &'static str = "Lyceum";
    const BUILD_COMMAND: &'static str = "empyrean-import world build";
    const ITEM_FORMS: &'static str = "an item as the world's rows name it";
    const NAMES_LINEAGE: bool = true;
    type Kind = RepoKind;
    type Era = EraId;

    fn era(name: Option<&str>) -> Result<EraId, String> {
        name.map_or(Ok(EraId::Eor), |n| {
            EraId::parse(n).ok_or_else(|| format!("{n:?} is not an era"))
        })
    }

    /// The vocabulary is the server's, known only at run time: an override names an item as the
    /// rows do, and is kept as it is written.
    fn item(text: &str, _words: &[&str]) -> Option<String> {
        let t = text.trim();
        (!t.is_empty()).then(|| t.to_owned())
    }
}

/// A pack of rows: its name, the provenance its header gives its rows, and its row files by path.
#[derive(Clone, Debug, PartialEq)]
pub struct RowPack {
    pub name: String,
    pub dir: PathBuf,
    pub prov: Provenance,
    pub files: BTreeMap<String, String>,
}

/// A layer of a repository: its id (`base`, or a patch's `<yyyy-mm>-<name>`), whether it is a
/// checkpoint, and its packs.
#[derive(Clone, Debug, PartialEq)]
pub struct RepoLayer {
    pub id: String,
    pub checkpoint: bool,
    pub packs: Vec<RowPack>,
    pub overrides: Vec<dereth_pack::conflict::Override>,
}

/// A world repository, read.
#[derive(Clone, Debug, PartialEq)]
pub struct Repo {
    /// Its `world.toml`.
    pub path: PathBuf,
    /// Its title, when its world file gives one.
    pub title: Option<String>,
    /// The era whose rules it plays (`era`), which may be one of the server's own (`newdereth`)
    /// while its data files are the end of retail's.
    pub era: EraId,
    pub layers: Vec<RepoLayer>,
}

/// One field of an item of a folded world: its value, the layer that set it (its id), where, and
/// where the value came from.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownField {
    pub name: String,
    pub value: toml::Value,
    pub layer: String,
    pub at: Option<Loc>,
    pub prov: Provenance,
}

/// One item of a folded world, its fields in the order of their names.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownItem {
    pub kind: String,
    pub fields: Vec<ShownField>,
}

/// A world folded at a layer: each item by its name, and what the fold found, by layer.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownWorld {
    /// The layer it is folded at.
    pub layer: String,
    pub items: BTreeMap<String, ShownItem>,
    /// What each layer's fold found, as lines (`<layer>: <finding>`).
    pub findings: Vec<String>,
}

/// A pack folder's rows: every `.toml` under it but its header, its meta file and those in the
/// world builder's folders at its root.
fn rows_of(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let read = std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        for e in read {
            let p = e.map_err(|e| e.to_string())?.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if p.is_dir() {
                if !(d == dir && BUILDER_FOLDERS.contains(&name)) {
                    todo.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "toml") {
                let rel: Vec<String> = p
                    .strip_prefix(dir)
                    .map_err(|e| e.to_string())?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                let rel = rel.join("/");
                if d == dir && NOT_ROWS.contains(&rel.as_str()) {
                    continue;
                }
                let text =
                    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                out.insert(rel, text);
            }
        }
    }
    Ok(out)
}

/// A pack's provenance for its rows: its header's `prov`, or a person's.
fn pack_prov(dir: &Path) -> Result<Provenance, String> {
    #[derive(serde::Deserialize)]
    struct Header {
        #[serde(default)]
        prov: Option<Provenance>,
    }
    let path = dir.join("pack.toml");
    if !path.is_file() {
        return Ok(Provenance::AUTHORED);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let h: toml::Table = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let prov = h
        .get("prov")
        .map(|p| {
            toml::Value::try_into::<Header>(toml::Value::Table(toml::Table::from_iter([(
                "prov".to_owned(),
                p.clone(),
            )])))
            .map_err(|e| format!("{}: prov: {e}", path.display()))
        })
        .transpose()?
        .and_then(|h| h.prov);
    Ok(prov.unwrap_or(Provenance::AUTHORED))
}

impl Repo {
    /// The repository whose world file is `world_toml`, its layers' packs read.
    ///
    /// # Errors
    /// A world file or pack that doesn't read.
    pub fn open(world_toml: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(world_toml)
            .map_err(|e| format!("{}: {e}", world_toml.display()))?;
        let top: toml::Table =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", world_toml.display()))?;
        let era = Repository::era(top.get("era").and_then(toml::Value::as_str))?;
        let title = top
            .get("title")
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
        let lineage = Lineage::read_world_file::<Repository>(&DiskStore, world_toml)
            .map_err(|e| e.to_string())?;
        let mut layers = Vec::new();
        for l in &lineage.layers {
            let mut packs = Vec::new();
            for dir in &l.packs {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .ok_or_else(|| format!("{}: not a pack folder", dir.display()))?;
                packs.push(RowPack {
                    name,
                    dir: dir.clone(),
                    prov: pack_prov(dir)?,
                    files: rows_of(dir)?,
                });
            }
            layers.push(RepoLayer {
                id: l.id.clone(),
                checkpoint: l.is_checkpoint(),
                packs,
                overrides: l.overrides.clone(),
            });
        }
        if layers.first().is_none_or(|b| b.packs.is_empty()) {
            return Err(format!(
                "{}: the base has no pack of rows",
                world_toml.display()
            ));
        }
        Ok(Self {
            path: world_toml.to_path_buf(),
            title,
            era,
            layers,
        })
    }

    /// The layers' ids, base first.
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.layers.iter().map(|l| l.id.as_str()).collect()
    }

    /// How many layers can be shown: those before the first checkpoint after the base.
    #[must_use]
    pub fn shown_layers(&self) -> usize {
        1 + self
            .layers
            .iter()
            .skip(1)
            .take_while(|l| !l.checkpoint)
            .count()
    }

    /// The world folded at layer `at` (0, the base), with the vocabulary `schema` gives.
    ///
    /// # Errors
    /// A layer past those that can be shown ([`Self::shown_layers`]), or a row that doesn't read:
    /// one of no kind the schema knows, or that names no item.
    pub fn fold(&self, schema: &RowSchema, at: usize) -> Result<ShownWorld, String> {
        if at >= self.shown_layers() {
            let id = self.layers.get(at).map_or("?", |l| l.id.as_str());
            return Err(format!(
                "{id}: the world after a checkpoint comes from its pinned release, which Lyceum doesn't read"
            ));
        }
        let mut kinds = Kinds::new();
        let mut inputs = Vec::new();
        for (n, layer) in self.layers.iter().take(at + 1).enumerate() {
            let (snapshot, rest) = if n == 0 {
                let first = &layer.packs[0];
                (
                    Some(snapshot(schema, first, &mut kinds)?),
                    &layer.packs[1..],
                )
            } else {
                (None, &layer.packs[..])
            };
            let merge = merge_of(schema, rest, &layer.overrides, &mut kinds)
                .map_err(|e| format!("{}: {e}", layer.id))?;
            inputs.push(match snapshot {
                Some(s) => Input::checkpoint(&layer.id, s, Vec::new(), merge),
                None => Input::delta(&layer.id, merge),
            });
        }
        let folded = fold::fold(inputs);
        let ids = self.ids();
        let mut items = BTreeMap::new();
        for e in folded.world.iter_by_name() {
            let Some(kind) = kinds.get(&e.item.name) else {
                continue;
            };
            let fields = e
                .fields
                .iter()
                .map(|(name, f)| ShownField {
                    name: name.to_string(),
                    value: f.value.clone(),
                    layer: ids
                        .get(f.layer)
                        .map_or_else(String::new, |s| (*s).to_owned()),
                    at: f.origin.at().cloned(),
                    prov: (*f.prov).clone(),
                })
                .collect();
            items.insert(
                e.item.name.clone(),
                ShownItem {
                    kind: kind.clone(),
                    fields,
                },
            );
        }
        let findings = folded
            .layers
            .iter()
            .flat_map(|l| l.findings.iter().map(move |f| format!("{}: {f}", l.id)))
            .collect();
        Ok(ShownWorld {
            layer: ids[at].to_owned(),
            items,
            findings,
        })
    }
}

/// A pack's rows as a snapshot: a whole known world, each item once.
fn snapshot(
    schema: &RowSchema,
    pack: &RowPack,
    kinds: &mut Kinds,
) -> Result<Snapshot<toml::Value>, String> {
    let mut snapshot = Snapshot::new();
    for (path, text) in &pack.files {
        let t: toml::Table = toml::from_str(text).map_err(|e| format!("{path}: {e}"))?;
        let at = Loc::new(&pack.dir.join(path), 0);
        for (kind, v) in &t {
            let rows = v
                .as_array()
                .ok_or_else(|| format!("{path}: {kind} isn't a list of rows"))?;
            for row in rows {
                let row = row
                    .as_table()
                    .ok_or_else(|| format!("{path}: a {kind} isn't a table"))?;
                records::snapshot_row(schema, kind, row, &at, &pack.prov, &mut snapshot, kinds)
                    .map_err(|e| format!("{path}: {e}"))?;
            }
        }
    }
    Ok(snapshot)
}

/// Packs' rows as one layer's changes.
fn merge_of(
    schema: &RowSchema,
    packs: &[RowPack],
    overrides: &[dereth_pack::conflict::Override],
    kinds: &mut Kinds,
) -> Result<fold::Merge<toml::Value>, String> {
    let mut rows = Vec::new();
    let mut removals = Vec::new();
    let mut problems = Vec::new();
    for pack in packs {
        for (path, text) in &pack.files {
            let (file, errors) = records::read_pack_file_from(
                schema,
                &pack.name,
                &pack.dir.join(path),
                text,
                &pack.prov,
            );
            problems.extend(errors.iter().map(ToString::to_string));
            rows.extend(file.rows);
            removals.extend(file.removals);
            kinds.extend(file.kinds);
        }
    }
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    Ok(fold::merge_layer(&rows, &removals, overrides))
}
