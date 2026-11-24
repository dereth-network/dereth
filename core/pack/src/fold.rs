//! A world's layers folded into the world at any of them: rows as keyed records of fields, each
//! field with the provenance of the row that set it.
//!
//! **Within a layer**, its packs merge field by field ([`merge_layer`]): a row sets only the fields
//! it names (and unsets those its `remove_fields` names), and two packs that set one field
//! differently conflict unless the layer's `[[override]]` says which wins ([`crate::conflict`]),
//! the item named as `<item> <field>` in the report. One pack setting an item twice is the pack's
//! to put right. A layer's `[[remove]]` rows take items away last, so an item a layer both sets
//! and removes is gone from it.
//!
//! **Across layers**, a later layer wins by order: its fields replace those below, the rest come
//! from below. An item keeps its place where it was first set; a new one comes after; one removed
//! and set again by a later layer comes after too.
//!
//! **A checkpoint** ([`Kind::Checkpoint`]) is a whole known world, its snapshot, supplied by the
//! game's builder. What it changes is computed, field by field: what its snapshot changed since
//! the previous checkpoint's (nothing before the first: all of it). So where the two snapshots
//! agree, the layers below stand, and where its source changed a field, the snapshot's value
//! takes it. Then the folded world and the snapshot are compared, and every field (or item) where
//! they disagree is **drift**: acknowledged in the checkpoint's `drift.toml` to keep the folded
//! world's or take the snapshot's ([`crate::lineage::Ack`]), or an error. Last, the checkpoint's
//! own packs apply, its corrections, as a delta's do.
//!
//! **What a fold holds.** While nothing has changed the world since the last checkpoint, the
//! world is that checkpoint's snapshot, value for value, and stands in for it: the next
//! checkpoint's fields are moved into the world rather than copied, and no snapshot is kept. A
//! copy of the world is taken, as the previous snapshot, only when a later layer changes it and a
//! later checkpoint may need it ([`Input::last_checkpoint`]). Folded so, the items a checkpoint
//! takes away are reported in the world's order rather than the previous snapshot's: the same
//! findings, always in the same order for the same layers.
//!
//! What a fold finds, layer by layer ([`Finding`]): conflicts, an item set twice in one pack, a
//! removal of an item not there (a stale tombstone) and unacknowledged drift, which are errors; an
//! acknowledgement of no drift and a change to the value already below, warnings; and an edit a
//! later layer sets again, so it never reaches the last layer, reported.
//!
//! The fold knows no game: an item is its name as reports and overrides say it
//! ([`crate::conflict::Item`]), a field a name, and a value whatever the game's builder holds
//! ([`Value`]).
//!
//! A field's provenance is shared ([`Arc`]): the fields of a row, and the rows of a pack or a
//! snapshot that say the same, hold one copy of it between them.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;

use crate::conflict::{settle, Claim, Conflict, Item, Overridden, Override, Settled};
use crate::error::Loc;
use crate::lineage::{Ack, Take};
use crate::provenance::Provenance;

/// A field's value, as a game's builder holds it.
pub trait Value: Clone + PartialEq + fmt::Debug {
    /// The value as a report says it.
    fn text(&self) -> String {
        format!("{self:?}")
    }
}

impl Value for toml::Value {
    fn text(&self) -> String {
        self.to_string()
    }
}

impl Value for String {}

/// Where a field's value was set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A row of one of a layer's packs (boxed: most fields of a big world come from a snapshot,
    /// and each field carries its origin).
    Pack(Box<PackRow>),
    /// A checkpoint's snapshot: what it changed since the one before, or a drift given to it.
    Snapshot,
}

/// A row of one of a layer's packs: the pack, and where the row is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackRow {
    pub pack: String,
    pub at: Loc,
}

impl Origin {
    /// A row of `pack` at `at`.
    #[must_use]
    pub fn pack(pack: &str, at: &Loc) -> Self {
        Self::Pack(Box::new(PackRow {
            pack: pack.to_owned(),
            at: at.clone(),
        }))
    }

    /// Where the row is written, for a pack's row.
    #[must_use]
    pub fn at(&self) -> Option<&Loc> {
        match self {
            Self::Pack(r) => Some(&r.at),
            Self::Snapshot => None,
        }
    }
}

/// A value a layer sets, with where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting<V> {
    pub value: V,
    pub prov: Arc<Provenance>,
    pub origin: Origin,
}

/// A field of a folded world: its value, where it came from, and the layer that set it.
#[derive(Clone, Debug, PartialEq)]
pub struct Field<V> {
    pub value: V,
    pub prov: Arc<Provenance>,
    pub origin: Origin,
    /// The layer's place in the lineage: 0 for the base.
    pub layer: usize,
}

/// A field's name, shared by every item that has the field.
pub type Name = Arc<str>;

/// One item of a folded world, and its fields by name.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry<V> {
    pub item: Item,
    pub fields: Fields<V>,
}

/// An item's fields by name, in the order of their names: a map as small as its fields, for a
/// world holds millions of them.
#[derive(Clone, Debug, PartialEq)]
pub struct Fields<V>(Vec<(Name, Field<V>)>);

impl<V> Default for Fields<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<V> Fields<V> {
    /// No fields.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn find<Q: Ord + ?Sized>(&self, name: &Q) -> Result<usize, usize>
    where
        Name: std::borrow::Borrow<Q>,
    {
        self.0
            .binary_search_by(|(n, _)| std::borrow::Borrow::<Q>::borrow(n).cmp(name))
    }

    /// The field `name`.
    #[must_use]
    pub fn get<Q: Ord + ?Sized>(&self, name: &Q) -> Option<&Field<V>>
    where
        Name: std::borrow::Borrow<Q>,
    {
        self.find(name).ok().map(|i| &self.0[i].1)
    }

    /// The field `name`, to change.
    pub fn get_mut<Q: Ord + ?Sized>(&mut self, name: &Q) -> Option<&mut Field<V>>
    where
        Name: std::borrow::Borrow<Q>,
    {
        self.find(name).ok().map(|i| &mut self.0[i].1)
    }

    /// Whether there is a field `name`.
    #[must_use]
    pub fn contains_key<Q: Ord + ?Sized>(&self, name: &Q) -> bool
    where
        Name: std::borrow::Borrow<Q>,
    {
        self.find(name).is_ok()
    }

    /// The field `name` set to `field`; what it was.
    pub fn insert(&mut self, name: Name, field: Field<V>) -> Option<Field<V>> {
        match self.find(&*name) {
            Ok(i) => Some(std::mem::replace(&mut self.0[i].1, field)),
            Err(i) => {
                self.0.insert(i, (name, field));
                None
            }
        }
    }

    /// The field `name` taken away.
    pub fn remove<Q: Ord + ?Sized>(&mut self, name: &Q) -> Option<Field<V>>
    where
        Name: std::borrow::Borrow<Q>,
    {
        self.find(name).ok().map(|i| self.0.remove(i).1)
    }

    /// Every field, in the order of their names.
    pub fn iter(&self) -> impl Iterator<Item = (&Name, &Field<V>)> + '_ {
        self.0.iter().map(|(n, f)| (n, f))
    }

    /// Every field's name, in order.
    pub fn keys(&self) -> impl Iterator<Item = &Name> + '_ {
        self.0.iter().map(|(n, _)| n)
    }

    /// Every field, in the order of their names.
    pub fn values(&self) -> impl Iterator<Item = &Field<V>> + '_ {
        self.0.iter().map(|(_, f)| f)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<V> FromIterator<(Name, Field<V>)> for Fields<V> {
    /// The fields, by name; of two with one name, the later.
    fn from_iter<I: IntoIterator<Item = (Name, Field<V>)>>(iter: I) -> Self {
        let mut v: Vec<(Name, Field<V>)> = iter.into_iter().collect();
        v.reverse();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v.dedup_by(|a, b| a.0 == b.0);
        v.shrink_to_fit();
        Self(v)
    }
}

impl<V> IntoIterator for Fields<V> {
    type Item = (Name, Field<V>);
    type IntoIter = std::vec::IntoIter<(Name, Field<V>)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a Fields<V> {
    type Item = (&'a Name, &'a Field<V>);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (Name, Field<V>)>,
        fn(&'a (Name, Field<V>)) -> (&'a Name, &'a Field<V>),
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(n, f)| (n, f))
    }
}

impl<V, Q: Ord + ?Sized> std::ops::Index<&Q> for Fields<V>
where
    Name: std::borrow::Borrow<Q>,
{
    type Output = Field<V>;
    fn index(&self, name: &Q) -> &Field<V> {
        self.get(name).expect("no such field")
    }
}

/// Field names shared: one copy of each, however many items have the field.
#[derive(Clone, Debug, Default)]
struct Names(std::collections::HashSet<Name>);

impl PartialEq for Names {
    /// What is shared says nothing of what holds it.
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Names {
    /// `name`, shared.
    fn of(&mut self, name: &str) -> Name {
        if let Some(n) = self.0.get(name) {
            return Arc::clone(n);
        }
        let n: Name = Arc::from(name);
        self.0.insert(Arc::clone(&n));
        n
    }
}

/// The provenance of a row's field `name`: the row's own `prov.fields` entry for it, else the row's
/// (without its fields).
#[must_use]
pub fn field_prov(prov: &Provenance, name: &str) -> Provenance {
    prov.fields
        .get(name)
        .cloned()
        .unwrap_or_else(|| Provenance {
            fields: BTreeMap::new(),
            ..prov.clone()
        })
}

/// Provenances shared: one copy of each that differs, however many fields say it.
#[derive(Clone, Debug, Default)]
struct Shared(HashMap<Provenance, Arc<Provenance>>);

impl PartialEq for Shared {
    /// What is shared says nothing of what holds it.
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Shared {
    /// `p`, shared.
    fn of(&mut self, p: Provenance) -> Arc<Provenance> {
        if let Some(a) = self.0.get(&p) {
            return Arc::clone(a);
        }
        let a = Arc::new(p.clone());
        self.0.insert(p, Arc::clone(&a));
        a
    }

    /// The provenance of a row's field `name` ([`field_prov`]), shared; `row` is the row's own,
    /// shared already.
    fn field(&mut self, prov: &Provenance, row: &Arc<Provenance>, name: &str) -> Arc<Provenance> {
        match prov.fields.get(name) {
            Some(own) => self.of(own.clone()),
            None => Arc::clone(row),
        }
    }

    /// A row's own provenance, without its fields', shared.
    fn row(&mut self, prov: &Provenance) -> Arc<Provenance> {
        if prov.fields.is_empty() {
            if let Some(a) = self.0.get(prov) {
                return Arc::clone(a);
            }
        }
        self.of(Provenance {
            fields: BTreeMap::new(),
            ..prov.clone()
        })
    }
}

/// A row of a layer's pack: the fields it sets of one item, and those it unsets.
#[derive(Clone, Debug, PartialEq)]
pub struct Row<V> {
    pub item: Item,
    /// The pack's name.
    pub pack: String,
    pub at: Loc,
    pub prov: Provenance,
    pub set: BTreeMap<String, V>,
    /// The fields it unsets (`remove_fields`).
    pub unset: BTreeSet<String>,
}

impl<V> Row<V> {
    /// A row of `pack` at `at` setting nothing of `item` yet.
    #[must_use]
    pub fn new(item: Item, pack: &str, at: &Loc, prov: Provenance) -> Self {
        Self {
            item,
            pack: pack.to_owned(),
            at: at.clone(),
            prov,
            set: BTreeMap::new(),
            unset: BTreeSet::new(),
        }
    }

    /// The row, setting `field` to `value` too.
    #[must_use]
    pub fn set(mut self, field: &str, value: V) -> Self {
        self.set.insert(field.to_owned(), value);
        self
    }

    /// The row, unsetting `field` too.
    #[must_use]
    pub fn unset(mut self, field: &str) -> Self {
        self.unset.insert(field.to_owned());
        self
    }
}

/// A `[[remove]]` of one item, by the name reports give it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removal {
    pub item: String,
    pub pack: String,
    pub at: Loc,
}

/// What one layer changes of one item: the fields it sets and those it unsets.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit<V> {
    pub item: Item,
    pub set: BTreeMap<String, Setting<V>>,
    pub unset: BTreeMap<String, Origin>,
}

impl<V> Edit<V> {
    /// An edit of `item` changing nothing yet: as a change's only edit of a new item, it is there
    /// with no fields.
    #[must_use]
    pub fn new(item: Item) -> Self {
        Self {
            item,
            set: BTreeMap::new(),
            unset: BTreeMap::new(),
        }
    }
}

/// What one layer's packs change, merged: each item's edit in the order first set, then its
/// removals.
#[derive(Clone, Debug, PartialEq)]
pub struct Change<V> {
    pub edits: Vec<Edit<V>>,
    pub removals: Vec<Removal>,
}

impl<V> Default for Change<V> {
    fn default() -> Self {
        Self {
            edits: Vec::new(),
            removals: Vec::new(),
        }
    }
}

/// A layer's packs merged: what they change, and what the merge found.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge<V> {
    pub change: Change<V>,
    /// Conflicts and items set twice in one pack.
    pub findings: Vec<Finding>,
    /// The fields packs set differently that the layer's overrides settled.
    pub overridden: Vec<Overridden>,
}

/// What a claim on a field compares by: the value, or its removal.
fn claimed<V: Value>(value: Option<&V>) -> String {
    value.map_or_else(|| "\u{0}unset".to_owned(), |v| format!("{v:?}"))
}

/// Merge a layer's `rows` (in load order) and `removals` field by field, two packs that set one
/// field differently settled by `overrides`. A conflicted field takes its last row's value, so
/// the world can still be shown; the conflict is a finding.
#[must_use]
pub fn merge_layer<V: Value>(
    rows: &[Row<V>],
    removals: &[Removal],
    overrides: &[Override],
) -> Merge<V> {
    let mut order: Vec<&str> = Vec::new();
    let mut by: BTreeMap<&str, Vec<&Row<V>>> = BTreeMap::new();
    let mut findings = Vec::new();
    for r in rows {
        let name = r.item.name.as_str();
        let list = by.entry(name).or_insert_with(|| {
            order.push(name);
            Vec::new()
        });
        if let Some(first) = list.iter().find(|p| p.pack == r.pack) {
            findings.push(Finding::SetTwice {
                item: name.to_owned(),
                pack: r.pack.clone(),
                at: r.at.clone(),
                first: first.at.clone(),
            });
            continue;
        }
        list.push(r);
    }
    let mut settled = Settled::default();
    let mut edits = Vec::new();
    let mut shared = Shared::default();
    for name in order {
        let rows = &by[name];
        let mut item = rows[0].item.clone();
        for r in &rows[1..] {
            item.landblocks.extend(r.item.landblocks.iter().copied());
        }
        let fields: BTreeSet<&String> = rows
            .iter()
            .flat_map(|r| r.set.keys().chain(&r.unset))
            .collect();
        let mut edit = Edit::new(item.clone());
        for field in fields {
            let setters: Vec<&Row<V>> = rows
                .iter()
                .copied()
                .filter(|r| r.set.contains_key(field) || r.unset.contains(field))
                .collect();
            let claims: Vec<Claim> = setters
                .iter()
                .map(|r| {
                    let v = r.set.get(field);
                    let sets = v.map_or_else(
                        || format!("removes its {field}"),
                        |v| format!("sets its {field} to {}", v.text()),
                    );
                    Claim::new(&r.pack, &r.at, sets, claimed(v))
                })
                .collect();
            let (conflicts, overridden) = (settled.conflicts.len(), settled.overridden.len());
            let w = settle(overrides, &item, &claims, &mut settled);
            for c in &mut settled.conflicts[conflicts..] {
                c.what = format!("{} {field}", c.what);
            }
            for o in &mut settled.overridden[overridden..] {
                o.what = format!("{} {field}", o.what);
            }
            let r = setters[w];
            let origin = Origin::pack(&r.pack, &r.at);
            match r.set.get(field) {
                Some(v) => {
                    edit.set.insert(
                        field.clone(),
                        Setting {
                            value: v.clone(),
                            prov: {
                                let row = shared.row(&r.prov);
                                shared.field(&r.prov, &row, field)
                            },
                            origin,
                        },
                    );
                }
                None => {
                    edit.unset.insert(field.clone(), origin);
                }
            }
        }
        edits.push(edit);
    }
    findings.extend(settled.conflicts.into_iter().map(Finding::Conflict));
    Merge {
        change: Change {
            edits,
            removals: removals.to_vec(),
        },
        findings,
        overridden: settled.overridden,
    }
}

/// A checkpoint's snapshot: a whole known world, as the game's builder converted it from the
/// source the checkpoint pins.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot<V> {
    entries: Vec<Option<Entry<V>>>,
    index: BTreeMap<String, usize>,
    shared: Shared,
    names: Names,
}

impl<V> Default for Snapshot<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: BTreeMap::new(),
            shared: Shared::default(),
            names: Names::default(),
        }
    }
}

impl<V: Value> Snapshot<V> {
    /// An empty snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The snapshot with `item` too, its `fields` from where `prov` says (a field its own
    /// `prov.fields` names, from there).
    ///
    /// # Errors
    /// An item the snapshot already has, as a sentence: a whole known world holds each item once,
    /// and the snapshot is left as it was.
    pub fn push(
        &mut self,
        item: Item,
        prov: &Provenance,
        fields: impl IntoIterator<Item = (String, V)>,
    ) -> Result<(), String> {
        if self.index.contains_key(&item.name) {
            return Err(format!("{} is given twice", item.name));
        }
        let row = self.shared.row(prov);
        let shared = &mut self.shared;
        let names = &mut self.names;
        let fields = fields
            .into_iter()
            .map(|(name, value)| {
                let prov = shared.field(prov, &row, &name);
                (
                    names.of(&name),
                    Field {
                        value,
                        prov,
                        origin: Origin::Snapshot,
                        layer: 0,
                    },
                )
            })
            .collect();
        let entry = Entry { item, fields };
        self.index
            .insert(entry.item.name.clone(), self.entries.len());
        self.entries.push(Some(entry));
        Ok(())
    }

    /// The item named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Entry<V>> {
        self.index.get(name).and_then(|&i| self.entries[i].as_ref())
    }

    /// The item named `name`, taken out.
    fn take(&mut self, name: &str) -> Option<Entry<V>> {
        self.index.remove(name).and_then(|i| self.entries[i].take())
    }

    /// Every item, in the order given.
    pub fn iter(&self) -> impl Iterator<Item = &Entry<V>> + '_ {
        self.entries.iter().flatten()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// A world folded up to a layer: its items in order, each with its fields.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved<V> {
    slots: Vec<Option<Entry<V>>>,
    index: BTreeMap<String, usize>,
    names: Names,
}

impl<V> Default for Resolved<V> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            index: BTreeMap::new(),
            names: Names::default(),
        }
    }
}

impl<V> Resolved<V> {
    /// The item named `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Entry<V>> {
        self.index.get(name).and_then(|&i| self.slots[i].as_ref())
    }

    /// Whether the world has the item named `name`.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Every item, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Entry<V>> + '_ {
        self.slots.iter().flatten()
    }

    /// Every item, in the order of their names.
    pub fn iter_by_name(&self) -> impl Iterator<Item = &Entry<V>> + '_ {
        self.index.values().filter_map(|&i| self.slots[i].as_ref())
    }

    /// Every item, in order, taken.
    pub fn into_entries(self) -> impl Iterator<Item = Entry<V>> {
        self.slots.into_iter().flatten()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Entry<V>> {
        self.index.get(name).and_then(|&i| self.slots[i].as_mut())
    }

    /// The entry of `item`, a new one after every other when the world has none.
    fn entry(&mut self, item: &Item) -> &mut Entry<V> {
        let i = match self.index.get(&item.name) {
            Some(&i) => i,
            None => {
                self.index.insert(item.name.clone(), self.slots.len());
                self.slots.push(Some(Entry {
                    item: item.clone(),
                    fields: Fields::new(),
                }));
                self.slots.len() - 1
            }
        };
        self.slots[i]
            .as_mut()
            .expect("an indexed slot holds its entry")
    }

    fn remove(&mut self, name: &str) -> Option<Entry<V>> {
        self.index.remove(name).and_then(|i| self.slots[i].take())
    }
}

/// What kind of layer one is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind<V> {
    /// Its packs' rows on the layer before it.
    Delta,
    /// A whole known world, its change computed from the previous checkpoint's, its drift
    /// acknowledged; then its packs' rows on top.
    Checkpoint {
        snapshot: Snapshot<V>,
        drift: Vec<Ack>,
    },
}

/// One layer to fold: its id, its kind, and its packs merged.
#[derive(Clone, Debug, PartialEq)]
pub struct Input<V> {
    pub id: String,
    pub kind: Kind<V>,
    pub change: Change<V>,
    /// What merging its packs found, reported as the layer's.
    pub findings: Vec<Finding>,
    pub overridden: Vec<Overridden>,
    /// For a checkpoint: whether its snapshot is kept, for a later checkpoint to compute its change
    /// from. The last checkpoint of a fold needs not keep it ([`Input::last_checkpoint`]).
    pub keep_snapshot: bool,
}

impl<V> Input<V> {
    /// A delta layer `id`, its packs merged.
    #[must_use]
    pub fn delta(id: &str, merge: Merge<V>) -> Self {
        Self {
            id: id.to_owned(),
            kind: Kind::Delta,
            change: merge.change,
            findings: merge.findings,
            overridden: merge.overridden,
            keep_snapshot: true,
        }
    }

    /// A delta layer `id` of an already merged `change`.
    #[must_use]
    pub fn of_change(id: &str, change: Change<V>) -> Self {
        Self {
            id: id.to_owned(),
            kind: Kind::Delta,
            change,
            findings: Vec::new(),
            overridden: Vec::new(),
            keep_snapshot: true,
        }
    }

    /// A checkpoint layer `id` of `snapshot`, its acknowledged `drift`, and its packs merged.
    #[must_use]
    pub fn checkpoint(id: &str, snapshot: Snapshot<V>, drift: Vec<Ack>, merge: Merge<V>) -> Self {
        Self {
            kind: Kind::Checkpoint { snapshot, drift },
            ..Self::delta(id, merge)
        }
    }

    /// The layer, said to be the fold's last checkpoint: its snapshot is let go once its change
    /// and drift are folded in, as no later checkpoint computes its change from it.
    #[must_use]
    pub fn last_checkpoint(mut self) -> Self {
        self.keep_snapshot = false;
        self
    }
}

/// How much a finding matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The world does not check.
    Error,
    /// Probably a mistake, which the world still checks with.
    Warning,
    /// Worth knowing.
    Report,
}

/// A field (or a whole item) where the world folded up to a checkpoint and its snapshot disagree,
/// each side's value as a report says it (`None`: that side has none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drift {
    pub item: String,
    /// `None`: the item itself, which one side has and the other not.
    pub field: Option<String>,
    pub folded: Option<String>,
    pub snapshot: Option<String>,
}

/// Something a fold found in a layer.
#[derive(Clone, Debug, PartialEq)]
pub enum Finding {
    /// Two packs of the layer set a field differently, with no override saying which wins.
    Conflict(Conflict),
    /// One pack sets an item twice.
    SetTwice {
        item: String,
        pack: String,
        at: Loc,
        first: Loc,
    },
    /// A removal of an item the layer neither has from below nor sets.
    StaleRemoval { item: String, at: Loc },
    /// Drift at a checkpoint that its `drift.toml` does not acknowledge.
    Drift(Drift),
    /// An acknowledgement of drift there is not.
    StaleAck {
        item: String,
        field: Option<String>,
        at: Loc,
    },
    /// A change to the value already below: a field set to it, or unset where it is not set.
    NoOp {
        item: String,
        field: String,
        at: Loc,
    },
    /// An edit of this layer that a later one changes again or removes, so it never reaches the
    /// last layer folded: one field, or (`None`) every edit of a removed item.
    Shadowed {
        item: String,
        field: Option<String>,
        at: Loc,
        /// The later layer, and where it says so (`None`: its snapshot).
        by: String,
        by_at: Option<Loc>,
    },
}

impl Finding {
    #[must_use]
    pub fn severity(&self) -> Severity {
        match self {
            Self::Conflict(_)
            | Self::SetTwice { .. }
            | Self::StaleRemoval { .. }
            | Self::Drift(_) => Severity::Error,
            Self::StaleAck { .. } | Self::NoOp { .. } => Severity::Warning,
            Self::Shadowed { .. } => Severity::Report,
        }
    }

    /// Where the finding is written, when it is in a file.
    #[must_use]
    pub fn at(&self) -> Option<&Loc> {
        match self {
            Self::Conflict(c) => c.claims.last().map(|c| &c.at),
            Self::SetTwice { at, .. }
            | Self::StaleRemoval { at, .. }
            | Self::StaleAck { at, .. }
            | Self::NoOp { at, .. }
            | Self::Shadowed { at, .. } => Some(at),
            Self::Drift(_) => None,
        }
    }
}

/// `item`, or `item field`.
fn naming(item: &str, field: Option<&str>) -> String {
    field.map_or_else(|| item.to_owned(), |f| format!("{item} {f}"))
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict(c) => write!(f, "{}", c.line()),
            Self::SetTwice {
                item,
                pack,
                at,
                first,
            } => {
                write!(f, "{at}: {item} is set twice in pack {pack} (also {first})")
            }
            Self::StaleRemoval { item, at } => {
                write!(
                    f,
                    "{at}: remove: nothing below or in this layer sets {item}"
                )
            }
            Self::Drift(d) => {
                let side = |v: &Option<String>| v.clone().unwrap_or_else(|| "nothing".to_owned());
                write!(
                    f,
                    "drift: {}: the folded world has {}, the snapshot {}; acknowledge it in drift.toml (keep or snapshot)",
                    naming(&d.item, d.field.as_deref()),
                    side(&d.folded),
                    side(&d.snapshot)
                )
            }
            Self::StaleAck { item, field, at } => write!(
                f,
                "{at}: drift: {} does not drift here",
                naming(item, field.as_deref())
            ),
            Self::NoOp { item, field, at } => {
                write!(f, "{at}: {item} {field}: changes nothing below")
            }
            Self::Shadowed {
                item,
                field,
                at,
                by,
                by_at,
            } => {
                write!(
                    f,
                    "{at}: {} is changed again by {by}",
                    naming(item, field.as_deref())
                )?;
                match by_at {
                    Some(l) => write!(f, " ({l})"),
                    None => write!(f, "'s snapshot"),
                }
            }
        }
    }
}

/// One layer's findings, and the fields its overrides settled.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LayerReport {
    pub id: String,
    pub findings: Vec<Finding>,
    pub overridden: Vec<Overridden>,
}

impl LayerReport {
    /// Its errors.
    pub fn errors(&self) -> impl Iterator<Item = &Finding> + '_ {
        self.findings
            .iter()
            .filter(|f| f.severity() == Severity::Error)
    }
}

/// Layers folded: the world at the last, and what each layer's fold found.
#[derive(Clone, Debug, PartialEq)]
pub struct Folded<V> {
    pub world: Resolved<V>,
    pub layers: Vec<LayerReport>,
}

impl<V> Folded<V> {
    /// Every error of every layer, with its layer's id.
    pub fn errors(&self) -> impl Iterator<Item = (&str, &Finding)> + '_ {
        self.layers
            .iter()
            .flat_map(|l| l.errors().map(move |f| (l.id.as_str(), f)))
    }

    /// Whether no layer has an error.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.errors().next().is_none()
    }
}

/// The fold's state between layers.
struct Folding<V> {
    world: Resolved<V>,
    ids: Vec<String>,
    reports: Vec<LayerReport>,
}

impl<V: Value> Folding<V> {
    /// `field` of the entry `item` is changed again at layer `n`: an edit below it is shadowed.
    fn shadow(&mut self, item: &str, field: Option<&str>, old: &Field<V>, n: usize, by: &Origin) {
        if old.layer >= n {
            return;
        }
        let Some(at) = old.origin.at() else {
            return;
        };
        let finding = Finding::Shadowed {
            item: item.to_owned(),
            field: field.map(str::to_owned),
            at: at.clone(),
            by: self.ids[n].clone(),
            by_at: by.at().cloned(),
        };
        self.reports[old.layer].findings.push(finding);
    }

    /// Set `field` of the item `item` at layer `n`, the entry made when the world has none.
    fn set(&mut self, item: &Item, field: &str, s: Setting<V>, n: usize) {
        let old = self.world.entry(item).fields.remove(field);
        if let Some(old) = &old {
            if old.value == s.value {
                if let Some(at) = s.origin.at() {
                    self.reports[n].findings.push(Finding::NoOp {
                        item: item.name.clone(),
                        field: field.to_owned(),
                        at: at.clone(),
                    });
                }
            } else {
                self.shadow(&item.name, Some(field), old, n, &s.origin);
            }
        }
        let name = self.world.names.of(field);
        self.world.entry(item).fields.insert(
            name,
            Field {
                value: s.value,
                prov: s.prov,
                origin: s.origin,
                layer: n,
            },
        );
    }

    /// Unset `field` of the item `name` at layer `n`, if the world has the item.
    fn unset(&mut self, name: &str, field: &str, origin: &Origin, n: usize) {
        let Some(entry) = self.world.get_mut(name) else {
            return;
        };
        match entry.fields.remove(field) {
            Some(old) => self.shadow(name, Some(field), &old, n, origin),
            None => {
                if let Some(at) = origin.at() {
                    self.reports[n].findings.push(Finding::NoOp {
                        item: name.to_owned(),
                        field: field.to_owned(),
                        at: at.clone(),
                    });
                }
            }
        }
    }

    /// Remove the item `name` at layer `n`, every edit below it shadowed; whether it was there.
    fn remove(&mut self, name: &str, n: usize, by: &Origin) -> bool {
        let Some(old) = self.world.remove(name) else {
            return false;
        };
        // One report for each layer below whose edits the removal takes away.
        let mut seen = BTreeSet::new();
        for f in old.fields.values() {
            if f.layer < n && matches!(f.origin, Origin::Pack(_)) && seen.insert(f.layer) {
                self.shadow(name, None, f, n, by);
            }
        }
        true
    }

    /// Apply a layer's merged change at layer `n`: its edits, then its removals.
    fn apply(&mut self, change: Change<V>, n: usize) {
        let mut set_here: BTreeSet<String> = BTreeSet::new();
        for edit in change.edits {
            set_here.insert(edit.item.name.clone());
            self.world.entry(&edit.item);
            for (field, s) in edit.set {
                self.set(&edit.item, &field, s, n);
            }
            for (field, origin) in edit.unset {
                self.unset(&edit.item.name, &field, &origin, n);
            }
        }
        let mut removed: BTreeSet<String> = BTreeSet::new();
        for r in change.removals {
            let by = Origin::pack(&r.pack, &r.at);
            if self.remove(&r.item, n, &by)
                || removed.contains(&r.item)
                || set_here.contains(&r.item)
            {
                removed.insert(r.item);
            } else {
                self.reports[n].findings.push(Finding::StaleRemoval {
                    item: r.item,
                    at: r.at,
                });
            }
        }
    }

    /// What `snapshot` changed since `prev` (an empty world before the first), applied at layer
    /// `n`. An item the world has lost keeps lost unless the snapshot makes it new.
    ///
    /// `prev` is spent as it is read, each of its items let go once compared, so the world before
    /// and both snapshots are never all held whole.
    fn apply_snapshot(&mut self, prev: Option<Snapshot<V>>, snapshot: &Snapshot<V>, n: usize) {
        let mut prev = prev.unwrap_or_default();
        for e in snapshot.iter() {
            let name = &e.item.name;
            match prev.take(name) {
                None => {
                    self.world.entry(&e.item);
                    for (field, f) in &e.fields {
                        self.set(&e.item, field, snapshot_setting(f), n);
                    }
                }
                Some(p) => {
                    if !self.world.contains(name) {
                        continue;
                    }
                    for (field, f) in &e.fields {
                        if p.fields.get(field).is_none_or(|pf| pf.value != f.value) {
                            self.set(&e.item, field, snapshot_setting(f), n);
                        }
                    }
                    for field in p.fields.keys().filter(|k| !e.fields.contains_key(*k)) {
                        self.unset(name, field, &Origin::Snapshot, n);
                    }
                }
            }
        }
        // What is left of `prev` is what the snapshot no longer has.
        for p in prev.iter() {
            self.remove(&p.item.name, n, &Origin::Snapshot);
        }
    }

    /// The drift between the world and `snapshot` at layer `n`, settled by `acks`: each
    /// acknowledged drift kept or given to the snapshot, the rest findings, and each
    /// acknowledgement of no drift a finding.
    /// Whether there was any drift, acknowledged or not.
    fn settle_drift(&mut self, snapshot: &Snapshot<V>, acks: &[Ack], n: usize) -> bool {
        let mut drifts: Vec<Drift> = Vec::new();
        for e in self.world.iter() {
            let name = &e.item.name;
            let Some(s) = snapshot.get(name) else {
                drifts.push(Drift {
                    item: name.clone(),
                    field: None,
                    folded: Some("it".to_owned()),
                    snapshot: None,
                });
                continue;
            };
            let fields: BTreeSet<&Name> = e.fields.keys().chain(s.fields.keys()).collect();
            for field in fields {
                let (a, b) = (e.fields.get(field), s.fields.get(field));
                if a.map(|f| &f.value) != b.map(|f| &f.value) {
                    drifts.push(Drift {
                        item: name.clone(),
                        field: Some(field.to_string()),
                        folded: a.map(|f| f.value.text()),
                        snapshot: b.map(|f| f.value.text()),
                    });
                }
            }
        }
        for s in snapshot
            .iter()
            .filter(|s| !self.world.contains(&s.item.name))
        {
            drifts.push(Drift {
                item: s.item.name.clone(),
                field: None,
                folded: None,
                snapshot: Some("it".to_owned()),
            });
        }
        let drifted = !drifts.is_empty();
        let mut used = vec![false; acks.len()];
        for d in drifts {
            let ack = acks
                .iter()
                .position(|a| a.item == d.item && a.field.is_some() && a.field == d.field)
                .or_else(|| {
                    acks.iter()
                        .position(|a| a.item == d.item && a.field.is_none())
                });
            let Some(i) = ack else {
                self.reports[n].findings.push(Finding::Drift(d));
                continue;
            };
            used[i] = true;
            if acks[i].take == Take::Keep {
                continue;
            }
            match (&d.field, snapshot.get(&d.item)) {
                (None, None) => {
                    self.remove(&d.item, n, &Origin::Snapshot);
                }
                (None, Some(s)) => {
                    self.world.entry(&s.item);
                    for (field, f) in &s.fields {
                        self.set(&s.item, field, snapshot_setting(f), n);
                    }
                }
                (Some(field), Some(s)) => match s.fields.get(field.as_str()) {
                    Some(f) => self.set(&s.item, field, snapshot_setting(f), n),
                    None => self.unset(&d.item, field, &Origin::Snapshot, n),
                },
                (Some(_), None) => {}
            }
        }
        for (a, used) in acks.iter().zip(used) {
            if !used {
                self.reports[n].findings.push(Finding::StaleAck {
                    item: a.item.clone(),
                    field: a.field.clone(),
                    at: a.at.clone(),
                });
            }
        }
        drifted
    }

    /// `snapshot` taken in at layer `n` over a world that is still the previous snapshot, value
    /// for value: what [`Folding::apply_snapshot`] and [`Folding::settle_drift`] do then, with the
    /// world itself standing in for the previous snapshot, and the snapshot's fields moved into
    /// it rather than copied. The world comes out the snapshot, so there is no drift, and every
    /// acknowledgement is of none.
    fn take_snapshot(&mut self, snapshot: Snapshot<V>, acks: &[Ack], n: usize) {
        let Snapshot { entries, index, .. } = snapshot;
        for e in entries.into_iter().flatten() {
            let Entry { item, fields } = e;
            if !self.world.contains(&item.name) {
                self.world.entry(&item);
                for (field, f) in fields {
                    self.set(&item, &field, moved_setting(f), n);
                }
                continue;
            }
            let have: Vec<Name> = self
                .world
                .get(&item.name)
                .map(|w| w.fields.keys().cloned().collect())
                .unwrap_or_default();
            let names: BTreeSet<Name> = fields.keys().cloned().collect();
            for (field, f) in fields {
                let same = self
                    .world
                    .get(&item.name)
                    .and_then(|w| w.fields.get(&field))
                    .is_some_and(|w| w.value == f.value);
                if !same {
                    self.set(&item, &field, moved_setting(f), n);
                }
            }
            for field in have.iter().filter(|k| !names.contains(*k)) {
                self.unset(&item.name, field, &Origin::Snapshot, n);
            }
        }
        let gone: Vec<String> = self
            .world
            .iter()
            .filter(|w| !index.contains_key(&w.item.name))
            .map(|w| w.item.name.clone())
            .collect();
        for name in gone {
            self.remove(&name, n, &Origin::Snapshot);
        }
        for a in acks {
            self.reports[n].findings.push(Finding::StaleAck {
                item: a.item.clone(),
                field: a.field.clone(),
                at: a.at.clone(),
            });
        }
    }

    /// The world as a snapshot: what the previous snapshot is, value for value, while nothing
    /// has changed the world since it was taken in.
    fn world_as_snapshot(&self) -> Snapshot<V> {
        let mut s = Snapshot::default();
        for e in self.world.iter() {
            s.index.insert(e.item.name.clone(), s.entries.len());
            s.entries.push(Some(e.clone()));
        }
        s
    }
}

/// A snapshot's field, as a setting of its checkpoint, moved.
fn moved_setting<V>(f: Field<V>) -> Setting<V> {
    Setting {
        value: f.value,
        prov: f.prov,
        origin: Origin::Snapshot,
    }
}

/// A snapshot's field, as a setting of its checkpoint.
fn snapshot_setting<V: Value>(f: &Field<V>) -> Setting<V> {
    Setting {
        value: f.value.clone(),
        prov: f.prov.clone(),
        origin: Origin::Snapshot,
    }
}

/// Fold `layers`, oldest first: the world at the last of them, and what each layer's fold found.
/// The world at an earlier layer is the fold of the layers up to it ([`resolve`]).
#[must_use]
pub fn fold<V: Value>(layers: impl IntoIterator<Item = Input<V>>) -> Folded<V> {
    fold_each(layers, |_, _| {})
}

/// [`fold`], `each` shown the world after each layer is folded in, with the layer's place (0 for
/// the base): every layer's world in one pass, without folding again up to each.
pub fn fold_each<V: Value>(
    layers: impl IntoIterator<Item = Input<V>>,
    mut each: impl FnMut(usize, &Resolved<V>),
) -> Folded<V> {
    let mut f = Folding {
        world: Resolved::default(),
        ids: Vec::new(),
        reports: Vec::new(),
    };
    // The previous checkpoint's snapshot, kept only once the world has come to differ from it;
    // while nothing has changed the world since, the world itself is that snapshot.
    let mut prev: Option<Snapshot<V>> = None;
    let mut untouched = true;
    // Whether a later checkpoint may still compute its change from the last one's snapshot.
    let mut later = true;
    for (n, layer) in layers.into_iter().enumerate() {
        f.ids.push(layer.id.clone());
        f.reports.push(LayerReport {
            id: layer.id,
            findings: layer.findings,
            overridden: layer.overridden,
        });
        if let Kind::Checkpoint { snapshot, drift } = layer.kind {
            later = layer.keep_snapshot;
            if untouched {
                f.take_snapshot(snapshot, &drift, n);
            } else {
                f.apply_snapshot(prev.take(), &snapshot, n);
                if f.settle_drift(&snapshot, &drift, n) {
                    prev = layer.keep_snapshot.then_some(snapshot);
                } else {
                    // No drift: the world is the snapshot again.
                    untouched = true;
                }
            }
        }
        let changes = !layer.change.edits.is_empty() || !layer.change.removals.is_empty();
        if changes && untouched {
            if later {
                prev = Some(f.world_as_snapshot());
            }
            untouched = false;
        }
        f.apply(layer.change, n);
        each(n, &f.world);
    }
    Folded {
        world: f.world,
        layers: f.reports,
    }
}

/// The world at layer `at` of `layers` (0 for the base): the fold of the layers up to it, what
/// each found reported as of that layer.
#[must_use]
pub fn resolve<V: Value>(layers: impl IntoIterator<Item = Input<V>>, at: usize) -> Folded<V> {
    fold(layers.into_iter().take(at + 1))
}
