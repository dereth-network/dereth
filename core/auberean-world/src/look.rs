//! How an object looks: its visual description put together with every description it extends,
//! and what its appearance keys do to each part group.
//!
//! A visual description extends a parent description, which may extend another. The client puts
//! the chain together into one **total** description, the root ancestor first: later descriptions
//! override the motion, behavior and mode tables they set, multiply the scales, add part groups
//! the total lacks (each hung from its parent group's connection point) and add their appearance
//! values to the groups the total has.
//!
//! An appearance value names an appearance table (`.apr`), one of its keys and a value: skin
//! shade, hair, a garment worn, its dye. The table holds, per key, a list of change sets each with
//! an intensity; the value picks the last set whose intensity is not above it. A table's base
//! tables apply first. A set's changes are atoms: material modifiers (by mesh kind and material
//! kind, with an optional replacement material), opacities, mesh replacements (by mesh kind), part
//! groups swapped in, effects and icon layers. Atoms of one kind replace each other by priority.
//!
//! For each part group the client gathers the atoms of the whole object's appearance values, then
//! of the group's own, and applies them to the group's parts: meshes first, then opacity, then the
//! materials. Part-group atoms of the whole object's values change the total's groups before any
//! part is built.

use std::sync::Arc;

mod edit;
pub use edit::ObjectLook;

use auberean_assets::fx::FxData;
use auberean_assets::visual::{Appearance, AppearanceChanges, IconLayer, PartGroup, VisualDesc};
use auberean_primitives::{DataId, Vec3};

/// The records a look reads: visual descriptions and appearance tables.
pub trait LookRecords {
    fn visual(&self, id: DataId) -> Option<Arc<VisualDesc>>;
    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>>;
}

/// The key of the changes that apply to every part group of an object.
pub const GLOBAL_KEY: u32 = 0x4000_0002;
/// A mesh or material kind that matches every kind.
pub const ANY_KIND: u32 = 1;
/// The value that marks an appearance key as taken off: it applies nothing.
pub const REMOVED: f32 = -1.0;
/// An opacity atom's value that stands for the appearance value it was applied at.
const OPACITY_OF_VALUE: f32 = -999.0;
/// The bucket counts of the client's keyed tables: a description's part groups, and a group's
/// appearance tables and their keys.
pub const GROUP_BUCKETS: u32 = 23;
pub const TABLE_BUCKETS: u32 = 47;

/// A keyed table in the order the client's hash tables walk it: bucket by bucket (the key modulo
/// the bucket count), each bucket from its newest entry to its oldest. A table read from a record
/// or a message keeps its stored order; an entry added later goes first in its bucket.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyed<V> {
    pub buckets: u32,
    pub entries: Vec<(u32, V)>,
}

impl<V> Default for Keyed<V> {
    fn default() -> Self {
        Self {
            buckets: TABLE_BUCKETS,
            entries: Vec::new(),
        }
    }
}

impl<V> Keyed<V> {
    /// A table read in this order.
    #[must_use]
    pub fn stored(buckets: u32, entries: Vec<(u32, V)>) -> Self {
        Self { buckets, entries }
    }

    #[must_use]
    pub fn get(&self, key: u32) -> Option<&V> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: u32) -> Option<&mut V> {
        self.entries
            .iter_mut()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn bucket(&self, key: u32) -> u32 {
        key % self.buckets.max(1)
    }

    /// Add an entry the table lacks, first in its bucket; `false` when the key is there already.
    pub fn add(&mut self, key: u32, value: V) -> bool {
        if self.get(key).is_some() {
            return false;
        }
        let b = self.bucket(key);
        let at = self
            .entries
            .iter()
            .position(|(k, _)| self.bucket(*k) >= b)
            .unwrap_or(self.entries.len());
        self.entries.insert(at, (key, value));
        true
    }

    /// Set a key's value: in place when the table has it, else added first in its bucket.
    pub fn set(&mut self, key: u32, value: V) {
        if let Some(v) = self.get_mut(key) {
            *v = value;
        } else {
            self.add(key, value);
        }
    }

    /// Take a key out, the others keeping their order.
    pub fn remove(&mut self, key: u32) -> Option<V> {
        let at = self.entries.iter().position(|(k, _)| *k == key)?;
        Some(self.entries.remove(at).1)
    }
}

impl<V: Clone> Keyed<V> {
    /// A copy as the client makes one: a table of [`TABLE_BUCKETS`] buckets, each entry put first
    /// in its bucket in this table's order, so each bucket's entries come out reversed.
    #[must_use]
    pub fn copied(&self) -> Self {
        let mut out = Self {
            buckets: TABLE_BUCKETS,
            entries: Vec::with_capacity(self.entries.len()),
        };
        for (k, v) in &self.entries {
            out.add(*k, v.clone());
        }
        out
    }
}

/// A group's appearance tables copied as the client copies them: the tables as [`Keyed::copied`]
/// copies a table, each table's values copied the same way.
#[must_use]
pub fn copied_tables(tables: &Keyed<Keyed<f32>>) -> Keyed<Keyed<f32>> {
    let mut out = Keyed {
        buckets: TABLE_BUCKETS,
        entries: Vec::with_capacity(tables.entries.len()),
    };
    for (apr, values) in &tables.entries {
        out.add(*apr, values.copied());
    }
    out
}

/// One appearance table's values: each key's value, in the client's table order.
// D2-13926, D2-13927, D2-13921, D2-13925, D2-13920, D2-13915, D2-13916
pub type Values = Keyed<f32>;

/// One part group as the total description holds it.
// D2-14057, D2-14054, D2-14044, D2-14052, D2-14053
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Group {
    pub key: u32,
    pub parent_key: u32,
    pub connection: u32,
    pub setup: DataId,
    pub anim_map: DataId,
    /// Per appearance table, its keys and their values.
    pub appearance: Keyed<Keyed<f32>>,
    pub fx_table: DataId,
    pub startup_fx: Keyed<f32>,
    pub fx_overrides: Vec<(u32, Vec<FxData>)>,
    /// The keys of the groups hung from this one, in the order they were hung.
    pub children: Vec<u32>,
    /// The key of the group this one hangs from, once linked.
    pub parent: Option<u32>,
}

impl Group {
    /// A stored part group, its tables in their stored order.
    #[must_use]
    pub fn from_stored(g: &PartGroup) -> Self {
        let appearance = g
            .appearance_keys
            .iter()
            .flatten()
            .map(|(apr, keys)| (*apr, Keyed::stored(TABLE_BUCKETS, keys.clone())))
            .collect();
        Self {
            key: g.key.unwrap_or(0),
            parent_key: g.parent_key.unwrap_or(0),
            connection: g.connection_point.unwrap_or(0),
            setup: g.setup.unwrap_or(DataId::NONE),
            anim_map: g.anim_map.unwrap_or(DataId::NONE),
            appearance: Keyed::stored(TABLE_BUCKETS, appearance),
            fx_table: g.fx_table.unwrap_or(DataId::NONE),
            startup_fx: Keyed::stored(TABLE_BUCKETS, g.startup_fx.clone().unwrap_or_default()),
            fx_overrides: g.fx_overrides.clone().unwrap_or_default(),
            children: Vec::new(),
            parent: None,
        }
    }

    /// A copy as the client makes one: its own fields without its place in a tree, its tables
    /// copied as [`Keyed::copied`] copies them.
    // D2-14062, D2-14060
    #[must_use]
    pub fn copied(&self) -> Self {
        Self {
            appearance: copied_tables(&self.appearance),
            startup_fx: self.startup_fx.copied(),
            children: Vec::new(),
            parent: None,
            ..self.clone()
        }
    }

    /// The group as a description takes it in when it adds one: copied, and the copy copied into
    /// the description's table.
    fn detached(&self) -> Self {
        self.copied().copied()
    }

    /// Does it carry effects?
    // D2-14039
    #[must_use]
    pub fn has_fx(&self) -> bool {
        !self.fx_table.is_none() || !self.fx_overrides.is_empty()
    }

    /// Add an appearance table's values: a table the group lacks is added whole, else each value
    /// is set over the group's own.
    // D2-14051, D2-13924
    pub fn augment_appearance(&mut self, apr: u32, keys: &Keyed<f32>) {
        match self.appearance.get_mut(apr) {
            Some(own) => {
                for (k, v) in &keys.entries {
                    own.set(*k, *v);
                }
            }
            None => {
                self.appearance.add(apr, keys.copied());
            }
        }
    }

    /// The value the group gives key `key` of appearance table `apr`, when it gives one.
    // D2-14045, D2-13919
    #[must_use]
    pub fn applied_value(&self, apr: u32, key: u32) -> Option<f32> {
        self.appearance.get(apr)?.get(key).copied()
    }

    /// Take key `key` of appearance table `apr` off the group; a table left with no keys goes.
    /// `false` when the group has no such table.
    // D2-14047, D2-13918
    pub fn remove_appearance_key(&mut self, apr: u32, key: u32) -> bool {
        let Some(keys) = self.appearance.get_mut(apr) else {
            return false;
        };
        keys.remove(key);
        if keys.is_empty() {
            self.appearance.remove(apr);
        }
        true
    }

    /// Take off every key another group's appearance tables give; a table left with no keys goes.
    // D2-14046, D2-13923
    pub fn remove_appearance_keys(&mut self, other: &Group) {
        for (apr, keys) in &other.appearance.entries {
            let Some(own) = self.appearance.get_mut(*apr) else {
                continue;
            };
            for (k, _) in &keys.entries {
                own.remove(*k);
            }
            if own.is_empty() {
                self.appearance.remove(*apr);
            }
        }
    }

    /// Does the group hold nothing: no setup, animation map, appearance, effect table, startup
    /// effects or effect overrides?
    // D2-14037
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.setup.is_none()
            && self.anim_map.is_none()
            && self.appearance.is_empty()
            && self.fx_table.is_none()
            && self.startup_fx.is_empty()
            && self.fx_overrides.is_empty()
    }

    /// Do two groups stand in the same place with the same tables: the same animation map,
    /// connection point, effect table, key, parent key and the group they hang from? (Their setups
    /// and appearances are not compared.)
    // D2-14029
    #[must_use]
    pub fn same_place(&self, other: &Group) -> bool {
        self.anim_map == other.anim_map
            && self.connection == other.connection
            && self.fx_table == other.fx_table
            && self.key == other.key
            && self.parent_key == other.parent_key
            && self.parent == other.parent
    }

    /// Add another description's group of the same key: its appearance values, its effect table
    /// when it names one, its startup effects the group lacks, and its effect overrides. The setup
    /// and animation map stay the group's own. `false` when the keys differ.
    // D2-14061
    pub fn apply_additive(&mut self, other: &Group) -> bool {
        if self.key != other.key {
            return false;
        }
        for (apr, keys) in &other.appearance.entries {
            self.augment_appearance(*apr, keys);
        }
        if !other.fx_table.is_none() {
            self.fx_table = other.fx_table;
        }
        for (k, v) in &other.startup_fx.entries {
            self.startup_fx.add(*k, *v);
        }
        for (k, fx) in &other.fx_overrides {
            if !self.fx_overrides.iter().any(|(own, _)| own == k) {
                self.fx_overrides.push((*k, fx.clone()));
            }
        }
        true
    }

    /// Gather the atoms of every appearance value the group carries into `atoms`, table by table
    /// in the group's order; `groups_only` gathers only part-group atoms, else every other kind.
    /// `false` when a table it names does not exist.
    // D2-14042
    pub fn augment_atoms(
        &self,
        records: &dyn LookRecords,
        atoms: &mut Atoms,
        groups_only: bool,
    ) -> bool {
        for (apr, keys) in &self.appearance.entries {
            if !augment_from_table(records, *apr, keys, atoms, groups_only) {
                return false;
            }
        }
        true
    }
}

/// One appearance table's values into the atoms: each value not taken off, in the table's order.
// D2-13917
fn augment_from_table(
    records: &dyn LookRecords,
    apr: u32,
    keys: &Keyed<f32>,
    atoms: &mut Atoms,
    groups_only: bool,
) -> bool {
    if apr == 0 {
        return false;
    }
    let Some(table) = records.appearance(DataId(apr)) else {
        return false;
    };
    for (key, value) in &keys.entries {
        #[allow(clippy::float_cmp)]
        if *value != REMOVED {
            table_atoms(records, &table, *key, *value, atoms, groups_only, 0);
        }
    }
    true
}

/// Does appearance table `table` (or one of its base tables, searched first) have key `key`?
/// `false` when the table is not in the data.
// D2-13936, D2-13912
#[must_use]
pub fn appearance_has_key(records: &dyn LookRecords, table: DataId, key: u32) -> bool {
    fn has(records: &dyn LookRecords, t: &Appearance, key: u32, depth: usize) -> bool {
        if depth < 16 {
            for base in &t.base {
                if records
                    .appearance(DataId(*base))
                    .is_some_and(|b| has(records, &b, key, depth + 1))
                {
                    return true;
                }
            }
        }
        t.keys.iter().any(|(k, _)| *k == key)
    }
    records
        .appearance(table)
        .is_some_and(|t| has(records, &t, key, 0))
}

/// The strength steps of key `key` in appearance table `table`: the intensities of its change
/// sets, its base tables' first, each put in order after the equal ones. `None` when the table is
/// not in the data or lists the key with no change sets.
// D2-13937, D2-13914
#[must_use]
pub fn appearance_key_steps(
    records: &dyn LookRecords,
    table: DataId,
    key: u32,
) -> Option<Vec<f32>> {
    fn steps(
        records: &dyn LookRecords,
        t: &Appearance,
        key: u32,
        out: &mut Vec<f32>,
        depth: usize,
    ) -> bool {
        if depth < 16 {
            for base in &t.base {
                if let Some(b) = records.appearance(DataId(*base)) {
                    steps(records, &b, key, out, depth + 1);
                }
            }
        }
        let Some((_, sets)) = t.keys.iter().find(|(k, _)| *k == key) else {
            return true;
        };
        if sets.is_empty() {
            return false;
        }
        for s in sets {
            let at = out
                .iter()
                .position(|v| s.intensity < *v)
                .unwrap_or(out.len());
            out.insert(at, s.intensity);
        }
        true
    }
    let t = records.appearance(table)?;
    let mut out = Vec::new();
    steps(records, &t, key, &mut out, 0).then_some(out)
}

/// How many strength steps appearance table `table` itself gives key `key`.
// D2-13938, D2-13913
#[must_use]
pub fn appearance_step_count(records: &dyn LookRecords, table: DataId, key: u32) -> Option<i32> {
    records
        .appearance(table)?
        .step_counts
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, n)| *n)
}

/// The atoms appearance table `table` gives key `key` at `value` (its base tables' first, then
/// its own change set for the value) gathered into `atoms`; `groups_only` gathers only part-group
/// atoms, else every other kind. `false` when the table does not exist.
pub fn appearance_atoms(
    records: &dyn LookRecords,
    table: DataId,
    key: u32,
    value: f32,
    atoms: &mut Atoms,
    groups_only: bool,
) -> bool {
    let Some(t) = records.appearance(table) else {
        return false;
    };
    table_atoms(records, &t, key, value, atoms, groups_only, 0)
}

/// A table's atoms for one key at one value: its base tables' first, then its own change set
/// for the value.
// D2-13935
fn table_atoms(
    records: &dyn LookRecords,
    table: &Appearance,
    key: u32,
    value: f32,
    atoms: &mut Atoms,
    groups_only: bool,
    depth: usize,
) -> bool {
    if depth < 16 {
        for base in &table.base {
            if let Some(b) = records.appearance(DataId(*base)) {
                table_atoms(records, &b, key, value, atoms, groups_only, depth + 1);
            }
        }
    }
    match table.keys.iter().find(|(k, _)| *k == key) {
        Some((_, sets)) => change_set_atoms(sets, value, atoms, groups_only),
        None => true,
    }
}

/// The change set a value picks: the last before the first whose intensity is above the value.
// D2-14067
fn change_set_atoms(
    sets: &[AppearanceChanges],
    value: f32,
    atoms: &mut Atoms,
    groups_only: bool,
) -> bool {
    let mut picked = None;
    for s in sets {
        if value < s.intensity {
            break;
        }
        picked = Some(s);
    }
    match picked {
        Some(s) => atoms.update(s, value, groups_only),
        None => true,
    }
}

/// A material modifier applied to the parts of a mesh kind, to the materials of a material kind,
/// with the material it replaces the template with (none keeps it).
// D2-14000, D2-03203
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MaterialAtom {
    pub priority: f32,
    pub template: DataId,
    pub modifier: DataId,
    pub mesh_kind: u32,
    pub material_kind: u32,
}

impl MaterialAtom {
    /// Does this atom replace `other`: a higher priority on the same material kind and the same
    /// mesh kind (or every mesh kind)?
    // D2-14002
    #[must_use]
    pub fn replaces(&self, other: &Self) -> bool {
        other.priority < self.priority
            && (self.mesh_kind == ANY_KIND || self.mesh_kind == other.mesh_kind)
            && self.material_kind == other.material_kind
    }
}

/// A mesh swapped in for the parts of a mesh kind.
// D2-14007, D2-03205
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MeshAtom {
    pub priority: f32,
    pub mesh_kind: u32,
    pub mesh: DataId,
}

impl MeshAtom {
    /// Does this atom replace `other`: the same mesh kind at no lower priority?
    // D2-14008
    #[must_use]
    pub fn replaces(&self, other: &Self) -> bool {
        self.mesh_kind == other.mesh_kind && other.priority <= self.priority
    }
}

/// An opacity for the parts.
// D2-14004, D2-03204
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct OpacityAtom {
    pub priority: f32,
    pub opacity: f32,
}

impl OpacityAtom {
    /// Does this atom replace `other`: a priority at least as high? (The client's test is "higher
    /// differs from equal", which is "at least as high" for numbers.)
    // D2-14005
    #[must_use]
    pub fn replaces(&self, other: &Self) -> bool {
        (other.priority < self.priority) != (other.priority == self.priority)
    }
}

/// A part group swapped in for the group of its key.
// D2-14011, D2-03206
#[derive(Clone, Debug, PartialEq, Default)]
pub struct GroupAtom {
    pub priority: f32,
    pub group: Group,
}

impl GroupAtom {
    /// Does this atom replace `other`: the same group key at a priority at least as high?
    // D2-14012
    #[must_use]
    pub fn replaces(&self, other: &Self) -> bool {
        self.group.key == other.group.key
            && ((other.priority < self.priority) != (other.priority == self.priority))
    }

    /// The same priority, and a group in the same place with the same tables.
    // D2-14013
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.priority == other.priority && self.group.same_place(&other.group)
    }
}

/// Effects an appearance lists: the effect ids it lists them for, and the effects.
// D2-14027, D2-13970, D2-13988
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FxAtom {
    pub ids: Vec<u32>,
    pub effects: Vec<FxData>,
}

impl FxAtom {
    /// Whether it replaces another: both list the same first id.
    // D2-14022
    #[must_use]
    pub fn replaces(&self, other: &Self) -> bool {
        self.ids.first() == other.ids.first()
    }

    /// Whether it goes before another in a collection: by their first ids, one listing none
    /// going before any.
    // D2-14023
    #[must_use]
    pub fn before(&self, other: &Self) -> bool {
        match (self.ids.first(), other.ids.first()) {
            (Some(a), Some(b)) => a < b,
            _ => true,
        }
    }
}

/// The atoms gathered for one part group.
// D2-13997, D2-03256, D2-03257, D2-13994, D2-13987, D2-13982, D2-13983, D2-13984, D2-13985
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Atoms {
    pub materials: Vec<MaterialAtom>,
    pub opacities: Vec<OpacityAtom>,
    pub meshes: Vec<MeshAtom>,
    pub groups: Vec<GroupAtom>,
    /// Effect atoms, in order of their first ids.
    pub effects: Vec<FxAtom>,
    /// Icon layers, in the order gathered.
    pub icons: Vec<IconLayer>,
}

impl Atoms {
    /// Add a material atom: the atoms it replaces go; it is not added when one of them replaces
    /// it. `false` when an equal atom is there already.
    // D2-13986, D2-14001
    pub fn add_material(&mut self, a: MaterialAtom) -> bool {
        let mut gone = Vec::new();
        for (i, e) in self.materials.iter().enumerate() {
            if a.replaces(e) {
                gone.push(i);
            } else if e.replaces(&a) {
                return true;
            }
        }
        for i in gone.into_iter().rev() {
            self.materials.remove(i);
        }
        if self.materials.contains(&a) {
            return false;
        }
        self.materials.push(a);
        true
    }

    /// Add an opacity atom: it takes the place of the first it replaces; it is not added when one
    /// replaces it. `false` when an equal atom is there already.
    // D2-13979, D2-14006
    pub fn add_opacity(&mut self, a: OpacityAtom) -> bool {
        for e in &mut self.opacities {
            if a.replaces(e) {
                *e = a;
                return true;
            }
            if e.replaces(&a) {
                return true;
            }
        }
        #[allow(clippy::float_cmp)]
        if self
            .opacities
            .iter()
            .any(|e| e.priority == a.priority && e.opacity == a.opacity)
        {
            return false;
        }
        self.opacities.push(a);
        true
    }

    /// Add a mesh atom: it takes the place of the first it replaces; it is not added when one
    /// replaces it. `false` when an equal atom is there already.
    // D2-13980, D2-14009
    pub fn add_mesh(&mut self, a: MeshAtom) -> bool {
        for e in &mut self.meshes {
            if a.replaces(e) {
                *e = a;
                return true;
            }
            if e.replaces(&a) {
                return true;
            }
        }
        if self.meshes.contains(&a) {
            return false;
        }
        self.meshes.push(a);
        true
    }

    /// Add a part-group atom: it takes the place of the first it replaces; it is not added when
    /// one replaces it. `false` when an equal atom is there already.
    // D2-13981
    pub fn add_group(&mut self, a: GroupAtom) -> bool {
        for e in &mut self.groups {
            if a.replaces(e) {
                *e = a;
                return true;
            }
            if e.replaces(&a) {
                return true;
            }
        }
        if self.groups.iter().any(|e| e.same_as(&a)) {
            return false;
        }
        self.groups.push(a);
        true
    }

    /// Add an effect atom: the first atom it replaces goes; it takes its place in order unless
    /// one of the same order is there.
    // D2-13995
    pub fn add_effect(&mut self, a: FxAtom) -> bool {
        if let Some(i) = self.effects.iter().position(|e| a.replaces(e)) {
            self.effects.remove(i);
        }
        let at = self.effects.iter().position(|e| !e.before(&a));
        match at {
            Some(i) if !a.before(&self.effects[i]) => {}
            Some(i) => self.effects.insert(i, a),
            None => self.effects.push(a),
        }
        true
    }

    /// Add one change set's atoms at `value`: part groups only, or everything else (materials,
    /// opacities, meshes, effects, then icon layers). An opacity stored as the value placeholder
    /// takes the value. Stops at the first atom refused as a repeat.
    // D2-13998
    pub fn update(&mut self, set: &AppearanceChanges, value: f32, groups_only: bool) -> bool {
        if groups_only {
            for g in &set.part_groups {
                let atom = GroupAtom {
                    priority: g.priority,
                    group: Group::from_stored(&g.part_group),
                };
                if !self.add_group(atom) {
                    return false;
                }
            }
            return true;
        }
        for m in &set.modifiers {
            let atom = MaterialAtom {
                priority: m.priority,
                template: m.template,
                modifier: m.modifier,
                mesh_kind: m.mesh_kind,
                material_kind: m.material_kind,
            };
            if !self.add_material(atom) {
                return false;
            }
        }
        for o in &set.opacities {
            #[allow(clippy::float_cmp)]
            let opacity = if o.opacity == OPACITY_OF_VALUE {
                value
            } else {
                o.opacity
            };
            if !self.add_opacity(OpacityAtom {
                priority: o.priority,
                opacity,
            }) {
                return false;
            }
        }
        for m in &set.meshes {
            if !self.add_mesh(MeshAtom {
                priority: m.priority,
                mesh_kind: m.mesh_kind,
                mesh: m.mesh,
            }) {
                return false;
            }
        }
        for f in &set.effects {
            self.add_effect(FxAtom {
                ids: f.ids.clone(),
                effects: f.effects.clone(),
            });
        }
        for l in &set.icon_layers {
            if !self.add_icon(*l) {
                return false;
            }
        }
        true
    }

    /// Add an icon atom: the first one it replaces goes; it is not added when an equal one is
    /// there (`false`), else it goes last.
    // D2-13977, D2-14025
    pub fn add_icon(&mut self, a: IconLayer) -> bool {
        if let Some(i) = self.icons.iter().position(|e| icon_atom_replaces(&a, e)) {
            self.icons.remove(i);
        }
        if self.icons.contains(&a) {
            return false;
        }
        self.icons.push(a);
        true
    }
}

/// Does icon atom `a` replace `b`: the same layer, and each has a picture when the other has one
/// and a color when the other has one?
// D2-14016
#[must_use]
pub fn icon_atom_replaces(a: &IconLayer, b: &IconLayer) -> bool {
    a.layer == b.layer
        && a.picture.is_none() == b.picture.is_none()
        && (a.shift_color.0 == 0) == (b.shift_color.0 == 0)
}

/// One layer added to an icon: a layer of the same number takes its picture (when it has one)
/// and its color (when it has one); else it goes in by its number, after the layers numbered no
/// higher. `false` when an equal layer is there already.
// D2-13904, D2-13909
pub fn apply_icon_layer(icon: &mut Vec<IconLayer>, layer: IconLayer) -> bool {
    if let Some(l) = icon.iter_mut().find(|l| l.layer == layer.layer) {
        if !layer.picture.is_none() {
            l.picture = layer.picture;
        }
        if layer.shift_color.0 != 0 {
            l.shift_color = layer.shift_color;
        }
        return true;
    }
    if icon.contains(&layer) {
        return false;
    }
    let at = icon
        .iter()
        .position(|l| l.layer > layer.layer)
        .unwrap_or(icon.len());
    icon.insert(at, layer);
    true
}

/// Whether an icon can be drawn: every layer has a number and a color, and one has a picture.
// D2-13887
#[must_use]
pub fn icon_is_valid(icon: &[IconLayer]) -> bool {
    let mut picture = false;
    for l in icon {
        if l.layer == 0 || l.shift_color.0 == 0 {
            return false;
        }
        picture |= !l.picture.is_none();
    }
    picture
}

/// Layers added to an icon, in order. `false` when one was there already.
// D2-13907
pub fn apply_icon(icon: &mut Vec<IconLayer>, layers: &[IconLayer]) -> bool {
    let mut all = true;
    for l in layers {
        if !apply_icon_layer(icon, *l) {
            all = false;
        }
    }
    all
}

/// An object's icon: the icons of the descriptions its own extends, the root ancestor first, then
/// its own (`own`), each laid over the one before layer by layer; then the icon atoms its own
/// appearance values (`global`, the changes to the whole object) gather. Empty when the chain does
/// not put together or a table its appearance names is missing.
#[must_use]
pub fn total_icon(
    records: &dyn LookRecords,
    parent: DataId,
    own: &[IconLayer],
    global: &Group,
) -> Vec<IconLayer> {
    let mut global = global.clone();
    global.key = GLOBAL_KEY;
    let desc = Desc {
        parent,
        scale: Vec3::new(1.0, 1.0, 1.0),
        child_scale: 1.0,
        particle_scale: 1.0,
        global,
        icon: own.to_vec(),
        ..Desc::default()
    };
    Look::total_icon_of(records, &desc)
}

/// A visual description as the client holds one it read: its groups linked into a tree.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Desc {
    pub parent: DataId,
    pub motion_desc: DataId,
    pub behavior_table: DataId,
    pub modes: DataId,
    pub scale: Vec3,
    pub child_scale: f32,
    pub particle_scale: f32,
    pub global: Group,
    /// The part groups in their stored order.
    pub groups: Vec<Group>,
    /// Its icon's layers.
    pub icon: Vec<IconLayer>,
}

impl Desc {
    /// A description as read: every group with a parent key it finds is hung from that parent,
    /// in the groups' order.
    // D2-13814, D2-14038, D2-14028
    #[must_use]
    pub fn from_stored(v: &VisualDesc) -> Self {
        let mut global = v
            .global
            .as_ref()
            .map(Group::from_stored)
            .unwrap_or_default();
        global.key = GLOBAL_KEY;
        let mut groups: Vec<Group> = v
            .part_groups
            .iter()
            .flatten()
            .map(|(k, g)| {
                let mut g = Group::from_stored(g);
                g.key = *k;
                g
            })
            .collect();
        for i in 0..groups.len() {
            let parent_key = groups[i].parent_key;
            if parent_key == 0 {
                continue;
            }
            if let Some(p) = groups.iter().position(|g| g.key == parent_key) {
                let child = groups[i].key;
                groups[p].children.push(child);
                groups[i].parent = Some(parent_key);
            }
        }
        let one = |v: Option<f32>| v.unwrap_or(1.0);
        Self {
            parent: v.parent.unwrap_or(DataId::NONE),
            motion_desc: v.motion_desc.unwrap_or(DataId::NONE),
            behavior_table: v.behavior_table.unwrap_or(DataId::NONE),
            modes: v.valid_modes.unwrap_or(DataId::NONE),
            scale: v.scale.unwrap_or(Vec3::new(1.0, 1.0, 1.0)),
            child_scale: one(v.child_scale),
            particle_scale: one(v.particle_scale),
            global,
            groups,
            icon: v.icon.clone().unwrap_or_default(),
        }
    }

    fn group(&self, key: u32) -> Option<&Group> {
        self.groups.iter().find(|g| g.key == key)
    }

    /// Does the description change nothing of a total besides its tables and scales?
    fn adds_no_groups(&self) -> bool {
        self.parent.is_none()
            && self.groups.is_empty()
            && self.global.appearance.is_empty()
            && !self.global.has_fx()
    }
}

/// Why a total description could not be put together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookError {
    /// A description of the chain is not in the data.
    Missing(DataId),
    /// A group's parent already holds another group at its connection point.
    Occupied { group: u32, connection: u32 },
    /// A group of another key was merged into a group.
    Mismatch(u32),
}

/// A visual description as the client holds one: a total (the chain put together, extending
/// nothing) or an object's own (extending `parent`), its part groups linked into a tree. (Its
/// icon is made, copied, emptied and dropped with it.)
// D2-13901, D2-13906, D2-13902, D2-13905, D2-13903, D2-13893
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// The description this one extends; none in a total.
    pub parent: DataId,
    pub motion_desc: DataId,
    pub behavior_table: DataId,
    pub modes: DataId,
    pub scale: Vec3,
    pub child_scale: f32,
    pub particle_scale: f32,
    /// The changes to every part group.
    pub global: Group,
    /// The part groups, in the client's table order.
    pub groups: Keyed<Group>,
    /// The key of the group the tree hangs from.
    pub root: Option<u32>,
    /// Its icon's layers, in order of their numbers.
    pub icon: Vec<IconLayer>,
}

impl Default for Look {
    // D2-13836, D2-13827
    fn default() -> Self {
        Self {
            parent: DataId::NONE,
            motion_desc: DataId::NONE,
            behavior_table: DataId::NONE,
            modes: DataId::NONE,
            scale: Vec3::new(1.0, 1.0, 1.0),
            child_scale: 1.0,
            particle_scale: 1.0,
            global: Group {
                key: GLOBAL_KEY,
                ..Group::default()
            },
            groups: Keyed {
                buckets: GROUP_BUCKETS,
                entries: Vec::new(),
            },
            root: None,
            icon: Vec::new(),
        }
    }
}

impl Look {
    /// The total of a stored visual description.
    // D2-06552
    pub fn of_record(records: &dyn LookRecords, id: DataId) -> Result<Self, LookError> {
        let v = records.visual(id).ok_or(LookError::Missing(id))?;
        Self::of(records, &Desc::from_stored(&v))
    }

    /// The total of a description: its ancestors' first, root first, then its own; then the
    /// part groups the whole object's appearance swaps in.
    // D2-13860, D2-06225
    pub fn of(records: &dyn LookRecords, desc: &Desc) -> Result<Self, LookError> {
        let mut look = Self::default();
        look.add_chain(records, desc, 0)?;
        look.swap_groups(records);
        Ok(look)
    }

    // D2-13840
    fn add_chain(
        &mut self,
        records: &dyn LookRecords,
        desc: &Desc,
        depth: usize,
    ) -> Result<(), LookError> {
        if !desc.parent.is_none() && depth < 32 {
            let parent = records
                .visual(desc.parent)
                .ok_or(LookError::Missing(desc.parent))?;
            self.add_chain(records, &Desc::from_stored(&parent), depth + 1)?;
        }
        self.apply_additive(desc)
    }

    /// The group of a key: the changes to every group for [`GLOBAL_KEY`].
    // D2-13808
    #[must_use]
    pub fn group(&self, key: u32) -> Option<&Group> {
        if key == GLOBAL_KEY {
            Some(&self.global)
        } else {
            self.groups.get(key)
        }
    }

    /// The group of a key, to change.
    pub fn group_mut(&mut self, key: u32) -> Option<&mut Group> {
        if key == GLOBAL_KEY {
            Some(&mut self.global)
        } else {
            self.groups.get_mut(key)
        }
    }

    /// The group the tree hangs from.
    #[must_use]
    pub fn root_group(&self) -> Option<&Group> {
        self.root.and_then(|k| self.group(k))
    }

    /// Add one description: its tables when set, its scales multiplied in, its icon's layers, its
    /// changes to every group, then each of its tree's top groups with the groups under them.
    // D2-13833
    pub fn apply_additive(&mut self, desc: &Desc) -> Result<(), LookError> {
        self.apply_additive_noting(desc, &mut Vec::new())
    }

    /// [`Look::apply_additive`], noting in `touched` the key of every group of this description
    /// that `desc` added to or merged into.
    // D2-13825
    pub fn apply_additive_noting(
        &mut self,
        desc: &Desc,
        touched: &mut Vec<u32>,
    ) -> Result<(), LookError> {
        if !desc.motion_desc.is_none() {
            self.motion_desc = desc.motion_desc;
        }
        if !desc.behavior_table.is_none() {
            self.behavior_table = desc.behavior_table;
        }
        if !desc.modes.is_none() {
            self.modes = desc.modes;
        }
        self.scale = Vec3::new(
            self.scale.x * desc.scale.x,
            self.scale.y * desc.scale.y,
            self.scale.z * desc.scale.z,
        );
        self.child_scale *= desc.child_scale;
        self.particle_scale *= desc.particle_scale;
        if !desc.icon.is_empty() {
            apply_icon(&mut self.icon, &desc.icon);
        }
        if desc.adds_no_groups() {
            return Ok(());
        }
        if (!desc.global.appearance.is_empty() || desc.global.has_fx())
            && !self.global.apply_additive(&desc.global)
        {
            return Err(LookError::Mismatch(desc.global.key));
        }
        for g in desc.groups.iter().filter(|g| g.parent.is_none()) {
            let root_key = self.root_group().map_or(0, |r| r.key);
            if root_key == g.key {
                let root = self.root.unwrap_or(g.key);
                let r = self.group_mut(root).ok_or(LookError::Mismatch(g.key))?;
                if !r.apply_additive(g) {
                    return Err(LookError::Mismatch(g.key));
                }
                if !touched.contains(&g.key) {
                    touched.push(g.key);
                }
                for c in &g.children {
                    self.merge_recursive(desc, *c, 0, touched)?;
                }
            } else {
                self.merge_recursive(desc, g.key, 0, touched)?;
            }
        }
        Ok(())
    }

    // D2-13823
    fn merge_recursive(
        &mut self,
        desc: &Desc,
        key: u32,
        depth: usize,
        touched: &mut Vec<u32>,
    ) -> Result<(), LookError> {
        let Some(g) = desc.group(key) else {
            return Ok(());
        };
        self.add_group_noting(g, touched)?;
        if depth < 32 {
            for c in &g.children {
                self.merge_recursive(desc, *c, depth + 1, touched)?;
            }
        }
        Ok(())
    }

    /// Add a group: merged into the total's group of its key, else copied in and hung from its
    /// parent's connection point (a group with no parent, or the first one, is the tree's root).
    // D2-13828
    pub fn add_group(&mut self, g: &Group) -> Result<(), LookError> {
        self.add_group_noting(g, &mut Vec::new())
    }

    /// [`Look::add_group`], noting the group's key in `touched`.
    // D2-13819, D2-14032
    fn add_group_noting(&mut self, g: &Group, touched: &mut Vec<u32>) -> Result<(), LookError> {
        if !touched.contains(&g.key) {
            touched.push(g.key);
        }
        if let Some(own) = self.group_mut(g.key) {
            return if own.apply_additive(g) {
                Ok(())
            } else {
                Err(LookError::Mismatch(g.key))
            };
        }
        self.groups.add(g.key, g.detached());
        if g.parent_key == 0 || self.root.is_none() {
            self.root = Some(g.key);
            return Ok(());
        }
        let Some(parent) = self.group(g.parent_key) else {
            return Ok(());
        };
        let occupied = parent.children.iter().any(|c| {
            self.group(*c)
                .is_some_and(|child| child.connection == g.connection)
        });
        if occupied {
            return Err(LookError::Occupied {
                group: g.key,
                connection: g.connection,
            });
        }
        if let Some(p) = self.group_mut(g.parent_key) {
            p.children.push(g.key);
        }
        if let Some(own) = self.group_mut(g.key) {
            own.parent = Some(g.parent_key);
        }
        Ok(())
    }

    /// The part groups the whole object's appearance swaps in: each replaces the setup, animation
    /// map and effects of the root's child of its key and adds its appearance values to it, or is
    /// added when the root has no such child.
    // D2-13859, D2-13858, D2-14033
    fn swap_groups(&mut self, records: &dyn LookRecords) {
        let mut atoms = Atoms::default();
        if !self.global.augment_atoms(records, &mut atoms, true) {
            return;
        }
        for a in atoms.groups {
            let child = self.root_group().and_then(|r| {
                r.children
                    .iter()
                    .copied()
                    .find(|c| self.group(*c).is_some_and(|g| g.key == a.group.key))
            });
            match child.and_then(|c| self.group_mut(c)) {
                Some(g) => {
                    g.anim_map = a.group.anim_map;
                    g.setup = a.group.setup;
                    // D2-14030
                    g.startup_fx = a.group.startup_fx.copied();
                    g.fx_overrides = a.group.fx_overrides.clone();
                    for (apr, keys) in &a.group.appearance.entries {
                        g.augment_appearance(*apr, keys);
                    }
                }
                None => {
                    let _ = self.add_group(&a.group);
                }
            }
        }
    }

    /// The atoms for the part group `key`: the whole object's appearance values first, then the
    /// group's own. `None` when a table they name is missing (the client then applies nothing).
    // D2-13812
    #[must_use]
    pub fn atoms(&self, records: &dyn LookRecords, key: u32) -> Option<Atoms> {
        if key == GLOBAL_KEY || key == 0 {
            return None;
        }
        let mut atoms = Atoms::default();
        if !self.global.augment_atoms(records, &mut atoms, false) {
            return None;
        }
        if let Some(g) = self.groups.get(key) {
            if !g.augment_atoms(records, &mut atoms, false) {
                return None;
            }
        }
        Some(atoms)
    }

    /// A description's icon: the total's (the chain's icons, the root ancestor first, then its
    /// own), with the icon atoms of the description's own changes to every group laid over it.
    /// Empty when the chain does not put together or a table its changes name is missing.
    // D2-13861
    #[must_use]
    pub fn total_icon_of(records: &dyn LookRecords, desc: &Desc) -> Vec<IconLayer> {
        let Ok(total) = Self::of(records, desc) else {
            return Vec::new();
        };
        let mut atoms = Atoms::default();
        if !desc.global.augment_atoms(records, &mut atoms, false) {
            return Vec::new();
        }
        let mut icon = total.icon;
        for l in atoms.icons {
            apply_icon_layer(&mut icon, l);
        }
        icon
    }

    /// The groups of the tree from the root, each before the groups hung from it, in the order
    /// they were hung.
    // D2-06597
    #[must_use]
    pub fn tree(&self) -> Vec<&Group> {
        let mut out = Vec::new();
        let mut stack: Vec<u32> = self.root.into_iter().collect();
        while let Some(k) = stack.pop() {
            if out.len() > 256 {
                break;
            }
            let Some(g) = self.group(k) else { continue };
            out.push(g);
            stack.extend(g.children.iter().rev());
        }
        out
    }
}

/// What the appearance does to one part group's parts.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PartLook {
    /// The meshes swapped in, in the order they apply.
    pub meshes: Vec<MeshAtom>,
    /// The material changes, in the order they apply.
    pub materials: Vec<MaterialAtom>,
    /// The parts' opacity, when the appearance sets one.
    pub opacity: Option<f32>,
}

impl PartLook {
    /// The atoms as the client applies them to a group.
    // D2-06589
    #[must_use]
    pub fn from_atoms(atoms: Atoms) -> Self {
        Self {
            meshes: atoms.meshes,
            opacity: atoms.opacities.last().map(|o| o.opacity),
            materials: atoms.materials,
        }
    }

    /// The mesh a part shows: its setup's mesh with each replacement whose kind matches the mesh
    /// it has by then (or every kind) swapped in, in order. `kind_of` gives a mesh's kind.
    // D2-06633, D2-06634, D2-05601
    pub fn mesh(
        &self,
        setup_mesh: DataId,
        kind_of: &mut dyn FnMut(DataId) -> Option<u32>,
    ) -> DataId {
        let mut mesh = setup_mesh;
        for m in &self.meshes {
            if m.mesh_kind == 0 {
                continue;
            }
            let Some(kind) = kind_of(mesh) else { continue };
            if m.mesh_kind == ANY_KIND || m.mesh_kind == kind {
                mesh = m.mesh;
            }
        }
        mesh
    }

    /// The material changes for one material of a mesh of `mesh_kind` whose instance is of
    /// `material_kind`: the replacement template (the last one), and the modifiers to add after the
    /// instance's own, in order. An atom whose modifier or template `present` does not find is
    /// passed over.
    // D2-06635, D2-05609, D2-05558, D2-10096, D2-10106
    pub fn material(
        &self,
        mesh_kind: u32,
        material_kind: u32,
        present: &mut dyn FnMut(&MaterialAtom) -> bool,
    ) -> (Option<DataId>, Vec<DataId>) {
        let mut template = None;
        let mut modifiers = Vec::new();
        for a in &self.materials {
            let mesh_ok = a.mesh_kind == ANY_KIND || a.mesh_kind == mesh_kind;
            let material_ok = a.material_kind == ANY_KIND || a.material_kind == material_kind;
            if !(mesh_ok && material_ok) || !present(a) {
                continue;
            }
            if !a.template.is_none() {
                template = Some(a.template);
            }
            if !a.modifier.is_none() {
                modifiers.push(a.modifier);
            }
        }
        (template, modifiers)
    }
}
