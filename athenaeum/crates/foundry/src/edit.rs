//! A world held open for editing: its rows placed, moved, turned, changed, removed and put back,
//! every change a step of text undone and redone, and saved only where the text changed.
//!
//! **What may change.** Edits go into one of the world's packs, its *target* (by default the last
//! pack it loads). A row of the target is changed where it is written. A row of the base database
//! is never written: changing one writes a row of the same key into the target, which replaces it
//! (the editor shows it as authored, and what it was); removing one writes a `[[remove]]` of its
//! item into the target; putting one back takes both away again. A row of another of the world's
//! packs is that pack's, and is left alone.
//!
//! **Where rows go.** People, objects and camps stand in a landblock and are written in its file,
//! `<target>/server/landblocks/XXYY.toml`; places in `<target>/server/places.toml`. A row moved
//! into another landblock goes to that landblock's file (its key kept where it is free there). A
//! new row's key is the kind's word and a number (`person-1`, `static-2`, `camp-1`, `place-3`),
//! the first that names nothing in the world or its base. Every row the session writes says it
//! was authored.
//!
//! **Steps.** Each change is one step of the history: the text of every file it touched, before
//! and after (`dereth-pack`'s session). Undo puts the text before back and redo the text after;
//! after each, the world is read, checked and merged over its base again, and its rows and
//! problems are the session's view of it.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use auberean_pack::Item as ItemKey;
use dereth_pack::session::{History, Sources, Step, Texts};
use dereth_pack::store::{DiskStore, SourceStore};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};
use yalain_world_db::compile::outdoor_cell;
use yalain_world_db::layer::{self, Base};
use yalain_world_db::merge::item_key;
use yalain_world_db::rows::{At, Camp, Clearing, Located, Object, ObjectKind, Place, Prov, Row};
use yalain_world_db::{Files, Problem};

use crate::toml as rowtoml;
use crate::world::Opened;

/// Where a row lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// The base database: never written; changed by a row of the same key in the target.
    Base,
    /// The pack the session's edits go into.
    Target,
    /// Another of the world's packs: left alone.
    Other,
}

/// A row of the world as it stands: placed people and objects, camps and places.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Its item, as the build's report names it (`object person-1@77B8`).
    pub item: String,
    pub row: Located,
    pub layer: Layer,
    /// The base's row this one replaces, when it replaces one.
    pub was: Option<Located>,
}

impl Entry {
    /// The landblock it stands in (0 for a place, which names its cell).
    #[must_use]
    pub fn landblock(&self) -> u16 {
        match &self.row.row {
            Row::Place(p) => p.cell.map_or(0, |c| (c >> 16) as u16),
            _ => self.row.landblock.unwrap_or(0),
        }
    }

    /// Where it stands in its landblock.
    #[must_use]
    pub fn at(&self) -> At {
        match &self.row.row {
            Row::Object(o) => o.at,
            Row::Camp(c) => c.at,
            Row::Place(p) => p.at,
            Row::Clearing(c) => c.at,
            _ => At {
                x: 0.0,
                y: 0.0,
                z: None,
            },
        }
    }

    /// Its provenance.
    #[must_use]
    pub fn prov(&self) -> Option<&Prov> {
        self.row.row.prov()
    }
}

/// The kinds of row a session places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewKind {
    /// A person the server numbers (`[[object]] kind = "npc"`).
    Person,
    /// An object of the world (`[[object]] kind = "static"`).
    Static,
    /// A camp that keeps creatures of one template alive.
    Camp,
    /// A destination.
    Place,
    /// Ground kept free of what the land grows.
    Clearing,
}

impl NewKind {
    /// The word a new row's key starts with.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Static => "static",
            Self::Camp => "camp",
            Self::Place => "place",
            Self::Clearing => "clearing",
        }
    }

    /// Whether it is made of a template.
    #[must_use]
    pub const fn has_template(self) -> bool {
        !matches!(self, Self::Place | Self::Clearing)
    }
}

/// A clearing's shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClearingShape {
    /// A circle, meters.
    Round { radius: f32 },
    /// A rectangle along its own axes, meters, turned clockwise from north, degrees.
    Rectangle { size: [f32; 2], heading: f32 },
}

/// A row to place: what (a template; unused for a place), where and facing which way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewRow {
    pub kind: NewKind,
    pub template: u32,
    pub landblock: u16,
    pub at: [f32; 3],
    /// Degrees clockwise from north.
    pub heading: f32,
}

/// A clearing's radius when one is placed with no shape drawn, meters.
pub const NEW_CLEARING: f32 = 15.0;

/// A camp's numbers when one is placed: a radius, how many it keeps alive, and how long a
/// replacement takes (the median respawn the recordings measured).
pub const NEW_CAMP: (f32, u32, f64) = (5.0, 2, 192.0);

/// A world held open for editing.
pub struct Session {
    dir: PathBuf,
    store: Box<dyn SourceStore>,
    sources: Sources,
    history: History,
    /// The step changes are gathered into, between [`Session::begin_step`] and [`Session::end_step`].
    gathering: Option<Step>,
    files: Files,
    base: Arc<Base>,
    /// The base's rows by item, as indexes into its merged rows.
    base_items: HashMap<String, usize>,
    /// The target pack's folder and name.
    target: String,
    target_name: String,
    world_packs: BTreeSet<String>,
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
    /// Every item a row of the world or its base names, removed or not.
    taken: BTreeSet<String>,
    removed: Vec<(String, Located)>,
    problems: Vec<Problem>,
    warnings: Vec<Problem>,
    revision: u64,
    /// The world's key, and which entries are its clearings.
    key: String,
    clearings: Vec<usize>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("dir", &self.dir)
            .field("target", &self.target)
            .field("entries", &self.entries.len())
            .field("problems", &self.problems.len())
            .finish_non_exhaustive()
    }
}

fn not_found(item: &str) -> String {
    format!("no row {item} in the world")
}

/// Which cell a copy of a row stands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyCell {
    /// The land cell under it, or the room the row was in.
    Follows,
    /// The land cell under it.
    Land,
    /// This room.
    Room(u32),
}

impl Session {
    /// The world `opened`, its edits going into the pack `target` (its folder or its name; by
    /// default the last pack the world loads), read and written on disk.
    ///
    /// # Errors
    /// The world names no such pack, or names none.
    pub fn open(opened: Opened, target: Option<&str>) -> Result<Self, String> {
        Self::new(
            opened.dir,
            Box::new(DiskStore),
            opened.files,
            opened.base,
            target,
        )
    }

    /// A world whose files are `files` (by path in `dir`), kept in `store`, over `base`.
    ///
    /// # Errors
    /// As [`Session::open`].
    pub fn new(
        dir: PathBuf,
        store: Box<dyn SourceStore>,
        files: Files,
        base: Base,
        target: Option<&str>,
    ) -> Result<Self, String> {
        let (db, _) = layer::read(&base, &files);
        let packs: Vec<(String, String)> = db
            .map(|d| {
                d.packs
                    .iter()
                    .map(|p| (p.folder.clone(), p.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let chosen = match target {
            Some(t) => packs.iter().find(|(f, n)| f == t || n == t).cloned(),
            None => packs.last().cloned(),
        };
        let Some((target, target_name)) = chosen else {
            return Err(match target {
                Some(t) => format!("the world loads no pack {t}"),
                None => "the world loads no pack to edit: name one in world.toml's packs".into(),
            });
        };
        let base_items = base
            .merged
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, l)| item_key(l).map(|k| (k, i)))
            .collect();
        let mut s = Self {
            dir,
            store,
            sources: Sources::default(),
            history: History::default(),
            gathering: None,
            files,
            base: Arc::new(base),
            base_items,
            target,
            target_name,
            world_packs: BTreeSet::new(),
            entries: Vec::new(),
            index: HashMap::new(),
            taken: BTreeSet::new(),
            removed: Vec::new(),
            problems: Vec::new(),
            warnings: Vec::new(),
            revision: 0,
            key: String::new(),
            clearings: Vec::new(),
        };
        s.refresh();
        Ok(s)
    }

    /// The world's folder.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The target pack's folder.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// The base the world lies over.
    #[must_use]
    pub fn base(&self) -> &Base {
        &self.base
    }

    /// The world's files as the session holds them, by path in its folder.
    #[must_use]
    pub fn files(&self) -> &Files {
        &self.files
    }

    /// The world's rows as they stand, base and packs merged.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The row of `item`.
    #[must_use]
    pub fn entry(&self, item: &str) -> Option<&Entry> {
        self.index.get(item).map(|i| &self.entries[*i])
    }

    /// The base's rows the world takes away, by item.
    #[must_use]
    pub fn removed(&self) -> &[(String, Located)] {
        &self.removed
    }

    /// What is wrong with the world as it stands, by file and line.
    #[must_use]
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// What a build would say but not stop for.
    #[must_use]
    pub fn warnings(&self) -> &[Problem] {
        &self.warnings
    }

    /// A number that changes with every change to the world's text.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// How many steps undo and redo can take.
    #[must_use]
    pub fn steps(&self) -> (usize, usize) {
        self.history.counts()
    }

    /// Whether a file's text differs from what is saved.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        self.sources.unsaved()
    }

    /// Write every changed file. Answers the files written or removed.
    ///
    /// # Errors
    /// A file that will not write, and why.
    pub fn save(&mut self) -> Result<Vec<PathBuf>, String> {
        self.sources
            .save(&*self.store)
            .map_err(|(p, e)| format!("{}: {e}", p.display()))
    }

    /// Take the last change back. Answers whether there was one.
    pub fn undo(&mut self) -> bool {
        let Some(step) = self.history.take_undo() else {
            return false;
        };
        self.put_texts(&step.before);
        self.history.undone(step);
        self.refresh();
        true
    }

    /// Make the last change taken back again. Answers whether there was one.
    pub fn redo(&mut self) -> bool {
        let Some(step) = self.history.take_redo() else {
            return false;
        };
        self.put_texts(&step.after);
        self.history.push(step);
        self.refresh();
        true
    }

    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn put_texts(&mut self, texts: &Texts) {
        for (path, text) in texts {
            self.sources.put(&*self.store, path, text.clone());
            let rel = self.rel(path);
            match text {
                Some(t) => {
                    self.files.insert(rel, t.clone());
                }
                None => {
                    self.files.remove(&rel);
                }
            }
        }
    }

    /// Make one change: new text (or no file) for each file named, as one step.
    fn commit(&mut self, changes: Vec<(String, Option<String>)>) {
        let mut step = Step::default();
        for (rel, text) in changes {
            let before = self.files.get(&rel).cloned();
            if before == text {
                continue;
            }
            let path = self.dir.join(&rel);
            let _ = self.sources.text(&*self.store, &path);
            step.before.insert(path.clone(), before);
            step.after.insert(path, text);
        }
        if step.after.is_empty() {
            return;
        }
        self.put_texts(&step.after);
        match &mut self.gathering {
            Some(g) => {
                for (path, text) in step.before {
                    g.before.entry(path).or_insert(text);
                }
                g.after.extend(step.after);
            }
            None => self.history.record(step),
        }
        self.refresh();
    }

    /// Changes from here to [`Session::end_step`] undo as one step.
    pub fn begin_step(&mut self) {
        self.gathering.get_or_insert_with(Step::default);
    }

    /// The changes since [`Session::begin_step`] become one step.
    pub fn end_step(&mut self) {
        if let Some(g) = self.gathering.take() {
            if !g.after.is_empty() {
                self.history.record(g);
            }
        }
    }

    /// Read, check and merge the world again.
    fn refresh(&mut self) {
        self.revision += 1;
        let (db, mut problems) = layer::read(&self.base, &self.files);
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        let mut taken: BTreeSet<String> = self.base_items.keys().cloned().collect();
        let mut removed = Vec::new();
        let mut keyed = None;
        if let Some(db) = db {
            keyed = Some(db.world.key.clone());
            self.world_packs = db.packs.iter().map(|p| p.name.clone()).collect();
            taken.extend(db.rows().filter_map(item_key));
            if problems.is_empty() {
                problems.extend(layer::check_world(&self.base, &db));
            }
            let merged = match layer::merge_over(&self.base, &db) {
                Ok(m) => Some(m),
                Err(p) => {
                    problems.extend(p);
                    None
                }
            };
            let rows: Vec<Located> = match &merged {
                Some(m) => {
                    warnings.clone_from(&m.warnings);
                    m.rows.clone()
                }
                // Unmerged, the base's rows and every pack's, so the world can still be seen.
                None => self
                    .base
                    .merged
                    .rows
                    .iter()
                    .chain(db.rows())
                    .cloned()
                    .collect(),
            };
            let standing: BTreeSet<String> = rows.iter().filter_map(item_key).collect();
            for (item, i) in &self.base_items {
                if !standing.contains(item) {
                    removed.push((item.clone(), self.base.merged.rows[*i].clone()));
                }
            }
            for l in rows {
                if !matches!(
                    l.row,
                    Row::Object(_) | Row::Camp(_) | Row::Place(_) | Row::Clearing(_)
                ) {
                    continue;
                }
                let Some(item) = item_key(&l) else { continue };
                let layer = if l.pack == self.target_name {
                    Layer::Target
                } else if self.world_packs.contains(&l.pack) {
                    Layer::Other
                } else {
                    Layer::Base
                };
                let was = (layer != Layer::Base)
                    .then(|| self.base_items.get(&item))
                    .flatten()
                    .map(|i| self.base.merged.rows[*i].clone());
                entries.push(Entry {
                    item,
                    row: l,
                    layer,
                    was,
                });
            }
        }
        removed.sort_by(|a, b| a.0.cmp(&b.0));
        problems.sort();
        problems.dedup();
        self.index = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.item.clone(), i))
            .collect();
        self.clearings = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.row.row, Row::Clearing(_)))
            .map(|(i, _)| i)
            .collect();
        if let Some(key) = keyed {
            self.key = key;
        }
        self.entries = entries;
        self.taken = taken;
        self.removed = removed;
        self.problems = problems;
        self.warnings = warnings;
    }

    /// The world's key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The world's clearings as they stand, each with the landblock it is written in.
    #[must_use]
    pub fn clearings(&self) -> Vec<(&Clearing, u16)> {
        self.clearings
            .iter()
            .filter_map(|i| {
                let e = self.entries.get(*i)?;
                match &e.row.row {
                    Row::Clearing(c) => Some((c, e.row.landblock?)),
                    _ => None,
                }
            })
            .collect()
    }

    /// The file a landblock's rows are written in, in the target.
    fn landblock_file(&self, landblock: u16) -> String {
        format!("{}/server/landblocks/{landblock:04X}.toml", self.target)
    }

    /// The file places are written in, in the target.
    fn places_file(&self) -> String {
        format!("{}/server/places.toml", self.target)
    }

    fn doc(&self, rel: &str) -> Result<DocumentMut, String> {
        match self.files.get(rel) {
            Some(t) => t
                .parse::<DocumentMut>()
                .map_err(|e| format!("{rel}: not TOML: {e}")),
            None => Ok(DocumentMut::new()),
        }
    }

    /// The first key `word-N` naming no item of the world or its base.
    fn free_key(&self, word: &str, item: impl Fn(&str) -> ItemKey) -> String {
        (1..)
            .map(|n| format!("{word}-{n}"))
            .find(|k| !self.taken.contains(&item(k).to_string()))
            .unwrap_or_else(|| word.to_string())
    }

    /// Place a new row. Answers its item.
    ///
    /// # Errors
    /// The target's file for it is not TOML.
    pub fn place(&mut self, new: &NewRow) -> Result<String, String> {
        self.place_in(new, None)
    }

    /// Place a new row in a cell of its landblock named in full (an interior cell, say; a row with
    /// none stands in the land cell under it). Answers its item.
    ///
    /// # Errors
    /// The target's file for it is not TOML.
    pub fn place_in(&mut self, new: &NewRow, cell: Option<u32>) -> Result<String, String> {
        let lb = new.landblock;
        let at = At {
            x: new.at[0],
            y: new.at[1],
            z: Some(new.at[2]),
        };
        let p = Prov::AUTHORED;
        if new.kind == NewKind::Clearing {
            let round = ClearingShape::Round {
                radius: NEW_CLEARING,
            };
            return self.place_clearing(lb, [new.at[0], new.at[1]], round);
        }
        let (kind, file, table, item) = match new.kind {
            NewKind::Person | NewKind::Static => {
                let key = self.free_key(new.kind.word(), |k| ItemKey::Object {
                    key: k.into(),
                    landblock: lb,
                });
                let o = Object {
                    key: key.clone(),
                    kind: if new.kind == NewKind::Person {
                        ObjectKind::Npc
                    } else {
                        ObjectKind::Static
                    },
                    id: None,
                    template: new.template,
                    cell,
                    at,
                    rotation: None,
                    heading: Some(new.heading.rem_euclid(360.0)),
                    scale: None,
                    name: None,
                    name_text: None,
                    to: None,
                    landings: Vec::new(),
                    scene: None,
                    locked: false,
                    lock: None,
                    route: None,
                    route_start: 0,
                    route_backward: false,
                    requires: None,
                    recall: false,
                    summon: false,
                    prov: p.clone(),
                };
                let item = ItemKey::Object {
                    key: key.clone(),
                    landblock: lb,
                };
                (
                    "object",
                    self.landblock_file(lb),
                    rowtoml::object(&o, None, &p),
                    item,
                )
            }
            NewKind::Camp => {
                let key = self.free_key("camp", |k| ItemKey::Camp {
                    key: k.into(),
                    landblock: lb,
                });
                let (radius, population, regen) = NEW_CAMP;
                let c = Camp {
                    key: key.clone(),
                    cell,
                    at,
                    radius,
                    spots: Vec::new(),
                    population,
                    regen,
                    spawns: vec![(new.template, 1)],
                    profile: None,
                    profile_name: None,
                    parent: None,
                    moves: yalain_content::MovementOverrides::default(),
                    route: None,
                    route_start: 0,
                    route_backward: false,
                    prov: p.clone(),
                };
                let item = ItemKey::Camp {
                    key: key.clone(),
                    landblock: lb,
                };
                (
                    "camp",
                    self.landblock_file(lb),
                    rowtoml::camp(&c, None, &p),
                    item,
                )
            }
            NewKind::Place => {
                let key = self.free_key("place", |k| ItemKey::Place { key: k.into() });
                let pl = Place {
                    key: key.clone(),
                    cell: cell.or_else(|| Some(outdoor_cell(lb, at.x, at.y))),
                    at,
                    rotation: None,
                    heading: Some(new.heading.rem_euclid(360.0)),
                    named_by: Vec::new(),
                    destination: None,
                    row: None,
                    holds: None,
                    prov: p.clone(),
                };
                let item = ItemKey::Place { key: key.clone() };
                ("place", self.places_file(), rowtoml::place(&pl, &p), item)
            }
            NewKind::Clearing => unreachable!("placed above"),
        };
        let mut doc = self.doc(&file)?;
        push_row(&mut doc, kind, table);
        self.commit(vec![(file, Some(doc.to_string()))]);
        Ok(item.to_string())
    }

    /// Keep a spot of a landblock free of what the land grows. Answers the new row's item.
    ///
    /// # Errors
    /// The target's file for it is not TOML.
    pub fn place_clearing(
        &mut self,
        landblock: u16,
        at: [f32; 2],
        shape: ClearingShape,
    ) -> Result<String, String> {
        let named = |k: &str| ItemKey::Clearing {
            key: k.into(),
            landblock,
        };
        let key = self.free_key("clearing", named);
        let item = named(&key);
        let (radius, size, heading) = shape_fields(shape);
        let table = rowtoml::clearing(
            &Clearing {
                key,
                at: At {
                    x: at[0],
                    y: at[1],
                    z: None,
                },
                heading,
                radius,
                size,
                prov: Prov::AUTHORED,
            },
            &Prov::AUTHORED,
        );
        let file = self.landblock_file(landblock);
        let mut doc = self.doc(&file)?;
        push_row(&mut doc, "clearing", table);
        self.commit(vec![(file, Some(doc.to_string()))]);
        Ok(item.to_string())
    }

    /// Give a clearing another shape, as one step.
    ///
    /// # Errors
    /// No such row, a row of another pack, or a file that is not TOML.
    pub fn shape_clearing(&mut self, item: &str, shape: ClearingShape) -> Result<String, String> {
        let (radius, size, heading) = shape_fields(shape);
        self.change(item, |t| {
            for field in ["radius", "size", "heading"] {
                t.remove(field);
            }
            if let Some(r) = radius {
                t.insert("radius", Item::Value(rowtoml::number(r)));
            }
            if let Some([x, y]) = size {
                let v = format!("[{}, {}]", rowtoml::number_text(x), rowtoml::number_text(y));
                if let Ok(v) = v.parse::<Value>() {
                    t.insert("size", Item::Value(v));
                }
            }
            if let Some(h) = heading {
                t.insert("heading", Item::Value(rowtoml::number(h)));
            }
        })
    }

    /// The row of `item` as a table to write in the target, with what to change in which files
    /// to put it there: for a row of the target, its own table where it stands; for a row of the
    /// base, an authored copy (its references qualified with the base's pack) in the target.
    fn editable(&self, item: &str) -> Result<(Entry, Table), String> {
        let e = self.entry(item).ok_or_else(|| not_found(item))?.clone();
        let table = match e.layer {
            Layer::Other => {
                return Err(format!(
                    "{item} is pack {}'s; this session edits pack {}",
                    e.row.pack, self.target_name
                ))
            }
            Layer::Target => {
                let doc = self.doc(&e.row.file)?;
                find_row(&doc, e.row.row.kind(), &key_of(&e.row.row))
                    .cloned()
                    .ok_or_else(|| format!("{item}: not found in {}", e.row.file))?
            }
            Layer::Base => {
                let p = Prov::AUTHORED;
                let pack = &e.row.pack;
                match &e.row.row {
                    Row::Object(o) => rowtoml::object(
                        &yalain_world_db::rows::Object {
                            route: o.route.as_deref().map(|r| rowtoml::qualified(r, pack)),
                            ..o.clone()
                        },
                        o.to.as_deref()
                            .map(|t| rowtoml::qualified(t, pack))
                            .as_deref(),
                        &p,
                    ),
                    Row::Camp(c) => rowtoml::camp(
                        &yalain_world_db::rows::Camp {
                            route: c.route.as_deref().map(|r| rowtoml::qualified(r, pack)),
                            ..c.clone()
                        },
                        c.parent
                            .as_deref()
                            .map(|t| rowtoml::qualified(t, pack))
                            .as_deref(),
                        &p,
                    ),
                    Row::Place(pl) => rowtoml::place(pl, &p),
                    Row::Clearing(c) => rowtoml::clearing(c, &p),
                    _ => return Err(format!("{item}: not a placed row")),
                }
            }
        };
        Ok((e, table))
    }

    /// Write `table` (the row of entry `e`, changed) where it belongs in the target: over its
    /// own table when it is the target's, else as a new table of its landblock's file (or the
    /// places' file).
    fn write_back(&mut self, e: &Entry, table: Table) -> Result<(), String> {
        let kind = e.row.row.kind();
        let file = match e.layer {
            Layer::Target => e.row.file.clone(),
            _ => match &e.row.row {
                Row::Place(_) => self.places_file(),
                _ => self.landblock_file(e.row.landblock.unwrap_or(0)),
            },
        };
        let mut doc = self.doc(&file)?;
        let key = key_of(&e.row.row);
        if !replace_row(&mut doc, kind, &key, table.clone()) {
            push_row(&mut doc, kind, table);
        }
        self.commit(vec![(file, Some(doc.to_string()))]);
        Ok(())
    }

    /// Change a row's fields with `change`. Answers its item.
    fn change(&mut self, item: &str, change: impl FnOnce(&mut Table)) -> Result<String, String> {
        let (e, mut table) = self.editable(item)?;
        change(&mut table);
        self.write_back(&e, table)?;
        Ok(item.to_string())
    }

    /// Turn a row to face `heading` degrees clockwise from north.
    ///
    /// # Errors
    /// No such row, a row of another pack, or a file that is not TOML.
    pub fn turn(&mut self, item: &str, heading: f32) -> Result<String, String> {
        self.change(item, |t| {
            t.remove("rotation");
            t.insert(
                "heading",
                Item::Value(rowtoml::number(heading.rem_euclid(360.0))),
            );
        })
    }

    /// Give a place this world added another key, and every row of this world that leads to it
    /// the new one, as one step. Answers the place's new item.
    ///
    /// # Errors
    /// No such place, a place of another pack, a key that is not one or is taken.
    pub fn rename_place(&mut self, item: &str, new: &str) -> Result<String, String> {
        let e = self.entry(item).ok_or_else(|| not_found(item))?.clone();
        let Row::Place(p) = &e.row.row else {
            return Err(format!("{item} is not a place"));
        };
        if e.layer != Layer::Target {
            return Err(format!(
                "{item} is not this world's own: only its own can be renamed"
            ));
        }
        if !yalain_world_db::rows::is_symbol(new) {
            return Err(format!("{new}: a key is letters, digits, - and _"));
        }
        let renamed = ItemKey::Place { key: new.into() }.to_string();
        if self.entry(&renamed).is_some() {
            return Err(format!("{new}: there is a place of that key"));
        }
        let old = p.key.clone();
        let leading: Vec<String> = self
            .entries()
            .iter()
            .filter(|l| l.layer == Layer::Target)
            .filter(|l| match &l.row.row {
                Row::Object(o) => o.to.as_deref() == Some(&old),
                Row::Step(s) => s.to.as_deref() == Some(&old),
                _ => false,
            })
            .map(|l| l.item.clone())
            .collect();
        self.begin_step();
        let done = self
            .change(item, |t| {
                t.insert("key", Item::Value(Value::from(new)));
            })
            .and_then(|_| {
                leading.iter().try_for_each(|l| {
                    self.change(l, |t| {
                        t.insert("to", Item::Value(Value::from(new)));
                    })
                    .map(|_| ())
                })
            });
        self.end_step();
        done.map(|()| renamed)
    }

    /// Set one field of a row to `value`, written as TOML (`0x47002224`, `5.0`, `"inn"`), or take
    /// it away with an empty `value`. A row's key names it and is not changed here. A change that
    /// leaves the row unread (a value of the wrong type, a needed field taken away) is refused,
    /// with what is wrong, and nothing changes.
    ///
    /// # Errors
    /// No such row, a row of another pack, the key, a field the row's kind lacks, a value that is
    /// not TOML, a change the row would not read after, or a file that is not TOML.
    pub fn set(&mut self, item: &str, field: &str, value: &str) -> Result<String, String> {
        if field == "key" {
            return Err("a row's key names it: place a new row to have another".into());
        }
        let kind = self
            .entry(item)
            .ok_or_else(|| not_found(item))?
            .row
            .row
            .kind();
        let fields = yalain_world_db::rows::FIELDS
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(&[][..], |(_, f)| *f);
        if !fields.contains(&field) {
            return Err(format!(
                "{field} is not a field of [[{kind}]]: one of {}",
                fields.join(", ")
            ));
        }
        let steps = self.history.len();
        let result = self.set_unchecked(item, field, value)?;
        if self.entry(&result).is_none() && self.history.len() > steps {
            let file = self
                .problems
                .iter()
                .filter(|p| p.message.starts_with(&format!("{field}:")))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n");
            self.undo();
            self.history.clear_redo();
            return Err(if file.is_empty() {
                format!("{field} = {value} leaves {item} unread")
            } else {
                file
            });
        }
        Ok(result)
    }

    fn set_unchecked(&mut self, item: &str, field: &str, value: &str) -> Result<String, String> {
        let v = if value.trim().is_empty() {
            None
        } else {
            Some(
                value
                    .trim()
                    .parse::<Value>()
                    .map_err(|e| format!("{field}: {value:?} is not a TOML value: {e}"))?,
            )
        };
        self.change(item, |t| match v {
            Some(v) => {
                t.insert(field, Item::Value(v));
            }
            None => {
                t.remove(field);
            }
        })
    }

    /// Move a row to `at` in `landblock`. Answers its item, which names the new landblock when it
    /// moved into another.
    ///
    /// # Errors
    /// No such row, a row of another pack, or a file that is not TOML.
    pub fn move_to(&mut self, item: &str, landblock: u16, at: [f32; 3]) -> Result<String, String> {
        self.move_into(item, landblock, at, None)
    }

    /// Move a row to `at` in `landblock`, in a cell named in full: indoors and out, or from room to
    /// room. With none, a land cell follows the point and an interior cell is kept. Answers its
    /// item, which names the new landblock when it moved into another.
    ///
    /// # Errors
    /// No such row, a row of another pack, or a file that is not TOML.
    pub fn move_into(
        &mut self,
        item: &str,
        landblock: u16,
        at: [f32; 3],
        cell: Option<u32>,
    ) -> Result<String, String> {
        let (e, mut table) = self.editable(item)?;
        let point = placed_point(&e.row.row, at);
        table.insert("at", Item::Value(rowtoml::point(point)));
        // A land cell follows the point; an interior cell is kept.
        let outdoor = |t: &Table| {
            t.get("cell")
                .and_then(Item::as_integer)
                .is_none_or(|c| c & 0xFFFF < 0x100)
        };
        let is_place = matches!(e.row.row, Row::Place(_));
        if let Some(cell) = cell {
            table.insert("cell", Item::Value(rowtoml::hex(cell)));
        } else if outdoor(&table) && (is_place || table.contains_key("cell")) {
            table.insert(
                "cell",
                Item::Value(rowtoml::hex(outdoor_cell(landblock, at[0], at[1]))),
            );
        }
        let from = e.row.landblock.unwrap_or(0);
        if is_place || landblock == from {
            self.write_back(&e, table)?;
            return Ok(item.to_string());
        }
        // Into another landblock: out of the old one's file, into the new one's.
        let kind = e.row.row.kind();
        let mut key = key_of(&e.row.row);
        let named = |k: &str| match kind {
            "camp" => ItemKey::Camp {
                key: k.into(),
                landblock,
            },
            "clearing" => ItemKey::Clearing {
                key: k.into(),
                landblock,
            },
            _ => ItemKey::Object {
                key: k.into(),
                landblock,
            },
        };
        if self.taken.contains(&named(&key).to_string()) {
            key = self.free_key(word_of(&e.row.row), named);
            table.insert("key", Item::Value(Value::from(key.as_str())));
        }
        let old = self.landblock_file(from);
        let new = self.landblock_file(landblock);
        let mut old_doc = self.doc(&old)?;
        if e.layer == Layer::Target {
            remove_row(&mut old_doc, kind, &key_of(&e.row.row));
        }
        // What the base has there stays away.
        if e.layer == Layer::Base || e.was.is_some() {
            push_remove(&mut old_doc, item);
        }
        let mut new_doc = self.doc(&new)?;
        push_row(&mut new_doc, kind, table);
        self.commit(vec![(old, text_of(&old_doc)), (new, text_of(&new_doc))]);
        Ok(named(&key).to_string())
    }

    /// Copy a row to `at` in `landblock`, with a key of its own and all else as it is. Answers the
    /// copy's item. The copy is always a new row of the target, whichever pack the row is from; an
    /// object's world-wide id stays with the row it was given to.
    ///
    /// # Errors
    /// No such row, or a file that is not TOML.
    pub fn duplicate(
        &mut self,
        item: &str,
        landblock: u16,
        at: [f32; 3],
    ) -> Result<String, String> {
        self.copy_row(item, landblock, at, CopyCell::Follows)
    }

    /// Copy a row to `at` in `landblock`, among the rooms in `room`'s, or out of any room when
    /// there is none. Otherwise as [`Self::duplicate`].
    ///
    /// # Errors
    /// No such row, or a file that is not TOML.
    pub fn copy_to(
        &mut self,
        item: &str,
        landblock: u16,
        at: [f32; 3],
        room: Option<u32>,
    ) -> Result<String, String> {
        let cell = room.map_or(CopyCell::Land, CopyCell::Room);
        self.copy_row(item, landblock, at, cell)
    }

    fn copy_row(
        &mut self,
        item: &str,
        landblock: u16,
        at: [f32; 3],
        cell: CopyCell,
    ) -> Result<String, String> {
        let e = self.entry(item).ok_or_else(|| not_found(item))?.clone();
        // Another pack's row is read, and only copied.
        let mut table = if e.layer == Layer::Other {
            let doc = self.doc(&e.row.file)?;
            find_row(&doc, e.row.row.kind(), &key_of(&e.row.row))
                .cloned()
                .ok_or_else(|| format!("{item}: not found in {}", e.row.file))?
        } else {
            self.editable(item)?.1
        };
        let word = word_of(&e.row.row);
        let (key, new) = match e.row.row {
            Row::Camp(_) => {
                let named = |k: &str| ItemKey::Camp {
                    key: k.into(),
                    landblock,
                };
                let key = self.free_key(word, named);
                let i = named(&key);
                (key, i)
            }
            Row::Place(_) => {
                let named = |k: &str| ItemKey::Place { key: k.into() };
                let key = self.free_key(word, named);
                let i = named(&key);
                (key, i)
            }
            Row::Clearing(_) => {
                let named = |k: &str| ItemKey::Clearing {
                    key: k.into(),
                    landblock,
                };
                let key = self.free_key(word, named);
                let i = named(&key);
                (key, i)
            }
            _ => {
                let named = |k: &str| ItemKey::Object {
                    key: k.into(),
                    landblock,
                };
                let key = self.free_key(word, named);
                let i = named(&key);
                (key, i)
            }
        };
        table.insert("key", Item::Value(Value::from(key.as_str())));
        table.remove("id");
        table.insert(
            "at",
            Item::Value(rowtoml::point(placed_point(&e.row.row, at))),
        );
        // A land cell follows the point; an interior cell is kept unless the copy goes elsewhere.
        let interior = table
            .get("cell")
            .and_then(Item::as_integer)
            .is_some_and(|c| c & 0xFFFF >= 0x100);
        let is_place = matches!(e.row.row, Row::Place(_));
        if let CopyCell::Room(room) = cell {
            table.insert("cell", Item::Value(rowtoml::hex(room)));
        } else if (cell == CopyCell::Land || !interior) && (is_place || table.contains_key("cell"))
        {
            table.insert(
                "cell",
                Item::Value(rowtoml::hex(outdoor_cell(landblock, at[0], at[1]))),
            );
        }
        let file = if is_place {
            self.places_file()
        } else {
            self.landblock_file(landblock)
        };
        let mut doc = self.doc(&file)?;
        push_row(&mut doc, e.row.row.kind(), table);
        self.commit(vec![(file, Some(doc.to_string()))]);
        Ok(new.to_string())
    }

    /// Take a row away: the target's own is deleted, the base's is removed by item.
    ///
    /// # Errors
    /// No such row, a row of another pack, or a file that is not TOML.
    pub fn delete(&mut self, item: &str) -> Result<(), String> {
        let e = self.entry(item).ok_or_else(|| not_found(item))?.clone();
        if e.layer == Layer::Other {
            return Err(format!(
                "{item} is pack {}'s; this session edits pack {}",
                e.row.pack, self.target_name
            ));
        }
        let file = match (&e.layer, &e.row.row) {
            (Layer::Target, _) => e.row.file.clone(),
            (_, Row::Place(_)) => self.places_file(),
            _ => self.landblock_file(e.row.landblock.unwrap_or(0)),
        };
        let mut doc = self.doc(&file)?;
        if e.layer == Layer::Target {
            remove_row(&mut doc, e.row.row.kind(), &key_of(&e.row.row));
        }
        if e.layer == Layer::Base || e.was.is_some() {
            push_remove(&mut doc, item);
        }
        self.commit(vec![(file, text_of(&doc))]);
        Ok(())
    }

    /// Put a base row back as the base has it: the target's row of its item and its removals
    /// go.
    ///
    /// # Errors
    /// The base has no such row, or a file that is not TOML.
    pub fn put_back(&mut self, item: &str) -> Result<(), String> {
        if !self.base_items.contains_key(item) {
            return Err(format!("the base has no row {item} to put back"));
        }
        let prefix = format!("{}/", self.target);
        let parsed = ItemKey::parse(&item.split_whitespace().collect::<Vec<_>>());
        let (kind, key) = match &parsed {
            Some(ItemKey::Object { key, .. }) => ("object", key.clone()),
            Some(ItemKey::Camp { key, .. }) => ("camp", key.clone()),
            Some(ItemKey::Place { key }) => ("place", key.clone()),
            Some(ItemKey::Clearing { key, .. }) => ("clearing", key.clone()),
            _ => return Err(format!("{item}: not a placed row")),
        };
        let own = self
            .entry(item)
            .filter(|e| e.layer == Layer::Target)
            .map(|e| e.row.file.clone());
        let mut changes = Vec::new();
        let files: Vec<String> = self
            .files
            .keys()
            .filter(|f| f.starts_with(&prefix) && f.ends_with(".toml") && !f.ends_with("pack.toml"))
            .cloned()
            .collect();
        for f in files {
            let mut doc = self.doc(&f)?;
            let mut changed = take_remove(&mut doc, item);
            if own.as_deref() == Some(f.as_str()) {
                changed |= remove_row(&mut doc, kind, &key);
            }
            if changed {
                changes.push((f, text_of(&doc)));
            }
        }
        self.commit(changes);
        Ok(())
    }
}

/// The key a placed row is written with.
fn key_of(row: &Row) -> String {
    match row {
        Row::Object(o) => o.key.clone(),
        Row::Camp(c) => c.key.clone(),
        Row::Place(p) => p.key.clone(),
        Row::Clearing(c) => c.key.clone(),
        _ => String::new(),
    }
}

/// A clearing shape's fields: its radius, its size and its heading.
fn shape_fields(shape: ClearingShape) -> (Option<f32>, Option<[f32; 2]>, Option<f32>) {
    match shape {
        ClearingShape::Round { radius } => (Some(radius), None, None),
        ClearingShape::Rectangle { size, heading } => {
            (None, Some(size), Some(heading.rem_euclid(360.0)))
        }
    }
}

/// A point a row is put at: a clearing lies on the ground, whatever the height.
const fn placed_point(row: &Row, at: [f32; 3]) -> At {
    At {
        x: at[0],
        y: at[1],
        z: match row {
            Row::Clearing(_) => None,
            _ => Some(at[2]),
        },
    }
}

/// The word new keys of a row's kind start with.
fn word_of(row: &Row) -> &'static str {
    match row {
        Row::Object(o) if o.kind == ObjectKind::Npc => "person",
        Row::Object(_) => "static",
        Row::Camp(_) => "camp",
        Row::Clearing(_) => "clearing",
        _ => "place",
    }
}

/// A file's text, or no file when nothing is left in it.
fn text_of(doc: &DocumentMut) -> Option<String> {
    if doc.as_table().is_empty() {
        None
    } else {
        Some(doc.to_string())
    }
}

/// The `[[kind]]` table whose key is `key`.
fn find_row<'d>(doc: &'d DocumentMut, kind: &str, key: &str) -> Option<&'d Table> {
    doc.get(kind)?
        .as_array_of_tables()?
        .iter()
        .find(|t| t.get("key").and_then(Item::as_str) == Some(key))
}

/// Put `table` in place of the `[[kind]]` table whose key is `key`, keeping its place and the
/// lines before it. Answers whether there was one.
fn replace_row(doc: &mut DocumentMut, kind: &str, key: &str, mut table: Table) -> bool {
    let Some(aot) = doc.get_mut(kind).and_then(Item::as_array_of_tables_mut) else {
        return false;
    };
    for t in aot.iter_mut() {
        if t.get("key").and_then(Item::as_str) == Some(key) {
            table.decor_mut().clone_from(t.decor());
            if let Some(p) = t.position() {
                table.set_position(Some(p));
            }
            *t = table;
            return true;
        }
    }
    false
}

/// Take the `[[kind]]` table whose key is `key` out. Answers whether there was one.
fn remove_row(doc: &mut DocumentMut, kind: &str, key: &str) -> bool {
    let Some(aot) = doc.get_mut(kind).and_then(Item::as_array_of_tables_mut) else {
        return false;
    };
    let Some(i) = aot
        .iter()
        .position(|t| t.get("key").and_then(Item::as_str) == Some(key))
    else {
        return false;
    };
    aot.remove(i);
    if aot.is_empty() {
        doc.remove(kind);
    }
    true
}

/// Add `table` as the last `[[kind]]` table.
fn push_row(doc: &mut DocumentMut, kind: &str, mut table: Table) {
    if !doc.as_table().is_empty() {
        table.decor_mut().set_prefix("\n");
    }
    match doc.get_mut(kind).and_then(Item::as_array_of_tables_mut) {
        Some(aot) => aot.push(table),
        None => {
            let mut aot = ArrayOfTables::new();
            aot.push(table);
            doc.insert(kind, Item::ArrayOfTables(aot));
        }
    }
}

/// Add a `[[remove]]` of `item`.
fn push_remove(doc: &mut DocumentMut, item: &str) {
    let mut t = Table::new();
    let mut items = toml_edit::Array::new();
    items.push(item);
    t.insert("items", Item::Value(Value::Array(items)));
    t.insert("prov", Item::Value(rowtoml::prov(&Prov::AUTHORED)));
    push_row(doc, "remove", t);
}

/// Take `item` out of every `[[remove]]`, dropping a removal left with nothing. Answers whether
/// any named it.
fn take_remove(doc: &mut DocumentMut, item: &str) -> bool {
    let Some(aot) = doc.get_mut("remove").and_then(Item::as_array_of_tables_mut) else {
        return false;
    };
    let mut changed = false;
    let mut empty = Vec::new();
    for (i, t) in aot.iter_mut().enumerate() {
        if t.get("item").and_then(Item::as_str) == Some(item) {
            t.remove("item");
            changed = true;
        }
        if let Some(a) = t.get_mut("items").and_then(Item::as_array_mut) {
            let before = a.len();
            a.retain(|v| v.as_str() != Some(item));
            changed |= a.len() != before;
            if a.is_empty() {
                t.remove("items");
            }
        }
        if !t.contains_key("items") && !t.contains_key("item") {
            empty.push(i);
        }
    }
    for i in empty.into_iter().rev() {
        aot.remove(i);
    }
    if aot.is_empty() {
        doc.remove("remove");
    }
    changed
}
