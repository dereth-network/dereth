//! The reference table: every object a script can name, by reference id.
//!
//! An entry is either an **object** (a script object, slot data with a tag per slot, or a native
//! object) or a **reference into a slot** (of another entry's object, or of a context's stack: the
//! way out parameters work). Its flags (see [`crate::graph::field`]) say which, and whether the
//! object is a singleton still to be filled from its saved state, owned by a process, loaded, and
//! safe for the collector to scan.
//!
//! Ids are indexes. A freed id is reused: a new entry takes the first free id at or after the
//! last one handed out, so a run hands out the same ids every time.
//!
//! **Collection** is mark and sweep: the caller names the roots (processes, contexts, singletons,
//! literals, references the host holds); marking follows script slots tagged as references and
//! the references native objects hold, but never into an object still being built (one added
//! while a graph is being read or an object initialized is held as reserved until the build ends,
//! and only then becomes scannable); every entry left unmarked is freed.

use std::collections::{BTreeMap, HashMap};

use crate::graph::{field, Body, Entry as PackedEntry, Graph, PackedObject};
use crate::native_object::NativeObject;
use crate::value::{tag, NULL};

/// An object in the table.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// The package, [`NULL`] for none.
    pub package: u32,
    /// The native type, 0 for a script object.
    pub native_type: u32,
    /// The saved state it came from (0 for none).
    pub state: u32,
    pub slots: Vec<u32>,
    pub tags: Vec<u8>,
    pub native: Option<NativeObject>,
    /// The engine object that owns it, as the host numbers them (0 for none).
    pub owner: u32,
}

impl Object {
    /// A script object of `size` slots, every slot and tag filled with the fresh-slot byte, as a
    /// new object starts before its initializer runs.
    // D2-00875, D2-00956
    #[must_use]
    pub fn script(package: u32, size: u32) -> Self {
        let n = size as usize;
        Self {
            package,
            native_type: 0,
            state: 0,
            slots: vec![0xCDCD_CDCD; n],
            tags: vec![tag::FRESH; n],
            native: None,
            owner: 0,
        }
    }

    /// A native object.
    #[must_use]
    pub fn native(native_type: u32, value: NativeObject) -> Self {
        Self {
            package: NULL,
            native_type,
            state: 0,
            slots: Vec::new(),
            tags: Vec::new(),
            native: Some(value),
            owner: 0,
        }
    }

    /// An object from its packed form, as the engine reads one (a multi-hash is built entry by
    /// entry: see [`crate::multi_hash`]).
    #[must_use]
    pub fn from_packed(p: &PackedObject) -> Self {
        Self {
            package: p.package_id(),
            native_type: u32::from(p.native_type),
            state: p.state,
            slots: p.slots.clone(),
            tags: p.tags.clone(),
            native: p.native.as_ref().map(NativeObject::unpacked),
            owner: 0,
        }
    }

    /// This object's packed form (a multi-hash's entries in its iteration order).
    #[must_use]
    pub fn to_packed(&self) -> PackedObject {
        #[allow(clippy::cast_possible_truncation)]
        PackedObject {
            state: self.state,
            package: if self.package == NULL {
                0xFFFF
            } else {
                self.package as u16
            },
            native_type: self.native_type as u16,
            native: self.native.as_ref().map(NativeObject::packed),
            slots: self.slots.clone(),
            tags: self.tags.clone(),
        }
    }

    /// The references this object holds, in visiting order: a native's own, or every slot tagged
    /// as a reference.
    #[must_use]
    pub fn references(&self) -> Vec<u32> {
        if let Some(n) = &self.native {
            return n.references();
        }
        self.slots
            .iter()
            .zip(&self.tags)
            .filter(|(_, t)| tag::is_reference(**t))
            .map(|(s, _)| *s)
            .filter(|s| *s != NULL)
            .collect()
    }
}

/// What an entry holds.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Target {
    /// A free id.
    #[default]
    Free,
    Object(Box<Object>),
    /// A reference into a slot: the referenced id, [`NULL`] for a context's stack.
    Relative(u32),
}

/// One entry: its flags and what it holds.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Entry {
    pub field: u32,
    pub target: Target,
}

/// Where a reference with an offset finally points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// A slot of a context's stack.
    Stack(usize),
    /// A slot of an object.
    Object { id: u32, slot: usize },
}

/// Why the table refused an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeapError {
    /// An id that names nothing.
    BadReference(u32),
    /// An object needed but a reference into a slot found, or the reverse.
    WrongKind(u32),
    /// A slot past an object's end.
    SlotOutOfRange { id: u32, slot: i64 },
    /// A chain of references into slots that loops.
    Loop(u32),
}

impl core::fmt::Display for HeapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadReference(id) => write!(f, "reference {id:#x} names nothing"),
            Self::WrongKind(id) => write!(f, "reference {id:#x} is not what was needed"),
            Self::SlotOutOfRange { id, slot } => write!(f, "slot {slot} past the end of {id:#x}"),
            Self::Loop(id) => write!(f, "references into slots loop at {id:#x}"),
        }
    }
}

/// The reference table.
#[derive(Clone, Debug, Default)]
pub struct Heap {
    entries: Vec<Entry>,
    /// Where the search for a free id starts.
    next: usize,
    /// Live entries.
    live: usize,
    /// Singletons by the saved state they fill from.
    singletons: BTreeMap<u32, u32>,
    /// Entries added while something is being built.
    reserved: Vec<u32>,
    /// How many builds are under way.
    building: u32,
    /// Entries added since the last collection.
    pub added_since_collection: usize,
}

impl Heap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many entries are live.
    #[must_use]
    pub fn live(&self) -> usize {
        self.live
    }

    /// The highest id ever handed out, plus one.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.entries.len()
    }

    /// An entry by id.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&Entry> {
        self.entries
            .get(id as usize)
            .filter(|e| !matches!(e.target, Target::Free))
    }

    /// An entry's flags.
    pub fn field(&self, id: u32) -> Result<u32, HeapError> {
        self.entry(id)
            .map(|e| e.field)
            .ok_or(HeapError::BadReference(id))
    }

    /// Change an entry's flags.
    pub fn set_field(&mut self, id: u32, f: u32) -> Result<(), HeapError> {
        let e = self
            .entries
            .get_mut(id as usize)
            .ok_or(HeapError::BadReference(id))?;
        if matches!(e.target, Target::Free) {
            return Err(HeapError::BadReference(id));
        }
        e.field = f;
        Ok(())
    }

    /// The object an id names.
    // D2-16460
    pub fn object(&self, id: u32) -> Result<&Object, HeapError> {
        match &self.entry(id).ok_or(HeapError::BadReference(id))?.target {
            Target::Object(o) => Ok(o),
            _ => Err(HeapError::WrongKind(id)),
        }
    }

    /// The object an id names, to change.
    pub fn object_mut(&mut self, id: u32) -> Result<&mut Object, HeapError> {
        match self.entries.get_mut(id as usize).map(|e| &mut e.target) {
            Some(Target::Object(o)) => Ok(o),
            Some(Target::Relative(_)) => Err(HeapError::WrongKind(id)),
            _ => Err(HeapError::BadReference(id)),
        }
    }

    /// The native object an id names.
    pub fn native(&self, id: u32) -> Result<&NativeObject, HeapError> {
        self.object(id)?
            .native
            .as_ref()
            .ok_or(HeapError::WrongKind(id))
    }

    /// The native object an id names, to change.
    pub fn native_mut(&mut self, id: u32) -> Result<&mut NativeObject, HeapError> {
        self.object_mut(id)?
            .native
            .as_mut()
            .ok_or(HeapError::WrongKind(id))
    }

    /// Start building: entries added until the matching [`Heap::end_build`] are held as reserved.
    // D2-16433
    pub fn begin_build(&mut self) {
        self.building += 1;
    }

    /// End a build; when the last one ends, every reserved entry becomes scannable.
    // D2-16436
    pub fn end_build(&mut self) {
        self.building = self.building.saturating_sub(1);
        if self.building == 0 {
            for id in core::mem::take(&mut self.reserved) {
                if let Some(e) = self.entries.get_mut(id as usize) {
                    e.field |= field::SCAN;
                }
            }
        }
    }

    /// Forget every build under way (after a run was abandoned part way).
    pub fn abandon_builds(&mut self) {
        self.building = 1;
        self.end_build();
    }

    /// Add an entry and return its id.
    // D2-00899, D2-16437
    pub fn add(&mut self, f: u32, target: Target) -> u32 {
        let mut i = self.next;
        while i < self.entries.len() && !matches!(self.entries[i].target, Target::Free) {
            i += 1;
        }
        if i == self.entries.len() {
            self.entries.push(Entry::default());
        }
        let mut f = f & !field::EPOCH;
        if self.building > 0 {
            f &= !field::SCAN;
        } else {
            f |= field::SCAN;
        }
        self.entries[i] = Entry { field: f, target };
        self.next = i + 1;
        self.live += 1;
        self.added_since_collection += 1;
        #[allow(clippy::cast_possible_truncation)]
        let id = i as u32;
        if self.building > 0 {
            self.reserved.push(id);
        }
        id
    }

    /// Put a new target into an existing id (filling a singleton in place).
    pub fn replace(&mut self, id: u32, f: u32, target: Target) -> Result<(), HeapError> {
        let e = self
            .entries
            .get_mut(id as usize)
            .ok_or(HeapError::BadReference(id))?;
        if matches!(e.target, Target::Free) {
            return Err(HeapError::BadReference(id));
        }
        let mut f = f & !field::EPOCH;
        if self.building > 0 {
            f &= !field::SCAN;
            self.reserved.push(id);
        } else {
            f |= field::SCAN;
        }
        *e = Entry { field: f, target };
        Ok(())
    }

    /// A native object, registered.
    // D2-00900
    pub fn add_native(&mut self, native_type: u32, value: NativeObject) -> u32 {
        self.add(
            field::OBJECT | field::LOADED,
            Target::Object(Box::new(Object::native(native_type, value))),
        )
    }

    /// A reference into a slot: of `target`'s object, or of the running context's stack when
    /// `target` is [`NULL`], `offset` slots in.
    pub fn add_relative(&mut self, target: u32, offset: u32) -> u32 {
        let f = field::RELATIVE | ((offset & 0xFFFF) << field::OFFSET_SHIFT);
        self.add(f, Target::Relative(target))
    }

    /// The singleton filled from a saved state: its id, made (not yet loaded) on first ask.
    // D2-00972, D2-16485
    pub fn singleton(&mut self, state: u32) -> u32 {
        if let Some(id) = self.singletons.get(&state) {
            if self.entry(*id).is_some() {
                return *id;
            }
        }
        let mut o = Object::script(NULL, 0);
        o.state = state;
        let id = self.add(
            field::OBJECT | field::SINGLETON,
            Target::Object(Box::new(o)),
        );
        self.singletons.insert(state, id);
        id
    }

    /// Every singleton id.
    pub fn singleton_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.singletons.values().copied()
    }

    /// Follow references into slots from `id`, adding their offsets to `slot`, to the object or
    /// stack the chain ends in. The returned id is the object's, with no fill done: the caller
    /// fills it when it is a singleton not loaded yet.
    // D2-16366, D2-00978, D2-01549
    pub fn locate(&self, id: u32, slot: i64) -> Result<(Option<u32>, i64), HeapError> {
        let mut id = id;
        let mut slot = slot;
        for _ in 0..=self.entries.len() {
            let e = self.entry(id).ok_or(HeapError::BadReference(id))?;
            match e.target {
                Target::Object(_) => {
                    return Ok((Some(id), slot + i64::from(field::offset(e.field))))
                }
                Target::Relative(t) => {
                    slot += i64::from(field::offset(e.field));
                    if t == NULL {
                        return Ok((None, slot));
                    }
                    id = t;
                }
                Target::Free => return Err(HeapError::BadReference(id)),
            }
        }
        Err(HeapError::Loop(id))
    }

    /// Read a packed graph into the table. Every entry gets a new id, except that the first
    /// object packed in full goes into `into` when given (a singleton filled in place); singleton
    /// entries name the singleton of their saved state; an object owned by a process and not the
    /// root names nothing. References inside the new objects are rewritten to the new ids.
    /// Returns the root's new id.
    // D2-00975, D2-00976, D2-00970, D2-00969, D2-16267, D2-16268, D2-16495
    pub fn import(&mut self, g: &Graph, into: Option<u32>) -> Result<u32, HeapError> {
        let mut map: HashMap<u32, u32> = HashMap::with_capacity(g.ids.len());
        let mut fresh: Vec<u32> = Vec::with_capacity(g.ids.len());
        let mut into = into;
        self.begin_build();
        for (gid, PackedEntry { field: f, body }) in g.ids.iter().zip(&g.entries) {
            let mut f = *f;
            let target = match body {
                Body::Singleton(state) => {
                    let id = self.singleton(*state);
                    map.insert(*gid, id);
                    continue;
                }
                Body::Elsewhere => {
                    map.insert(*gid, NULL);
                    continue;
                }
                Body::Object(o) => Target::Object(Box::new(Object::from_packed(o))),
                Body::Relative(t) => Target::Relative(*t),
            };
            if f & field::PROCESS != 0 {
                f &= !field::ROOT;
            }
            let id = match into.take() {
                Some(id) => {
                    self.replace(id, f, target)?;
                    id
                }
                None => self.add(f, target),
            };
            map.insert(*gid, id);
            fresh.push(id);
        }
        self.end_build();
        let mut remap = |r: u32| map.get(&r).copied().unwrap_or(r);
        for id in fresh {
            match &mut self.entries[id as usize].target {
                Target::Relative(t) if *t != NULL => *t = remap(*t),
                Target::Object(o) => {
                    if let Some(n) = &mut o.native {
                        n.remap(&mut remap);
                    } else {
                        for (s, t) in o.slots.iter_mut().zip(&o.tags) {
                            if tag::is_reference(*t) && *s != NULL {
                                *s = remap(*s);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let root = g.ids.first().ok_or(HeapError::BadReference(NULL))?;
        Ok(map.get(root).copied().unwrap_or(NULL))
    }

    /// Pack the graph `root` reaches: every entry visited depth first from the root, an object's
    /// references in slot order (or its native's order), never into a singleton or into an object
    /// owned by a process unless it is the graph's root. A root owned by a process is packed with
    /// the root bit set, as every saved state's root is.
    // D2-16417, D2-16411, D2-16408, D2-16407, D2-16404, D2-16492, D2-16494
    pub fn export(&self, root: u32) -> Result<Graph, HeapError> {
        let mut ids = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if id == NULL || !seen.insert(id) {
                continue;
            }
            ids.push(id);
            let e = self.entry(id).ok_or(HeapError::BadReference(id))?;
            let f = if id == root && e.field & field::PROCESS != 0 {
                e.field | field::ROOT
            } else {
                e.field
            };
            if f & field::SINGLETON != 0 || (f & field::PROCESS != 0 && f & field::ROOT == 0) {
                continue;
            }
            match &e.target {
                Target::Object(o) => {
                    // Depth first in order: push the children reversed.
                    for r in o.references().into_iter().rev() {
                        if !seen.contains(&r) {
                            stack.push(r);
                        }
                    }
                }
                Target::Relative(t) => stack.push(*t),
                Target::Free => {}
            }
        }
        let mut entries = Vec::with_capacity(ids.len());
        for id in &ids {
            let e = self.entry(*id).ok_or(HeapError::BadReference(*id))?;
            let mut f = e.field & !field::EPOCH;
            if *id == root && f & field::PROCESS != 0 {
                f |= field::ROOT;
            }
            let body = match &e.target {
                Target::Object(o) if f & field::SINGLETON != 0 => Body::Singleton(o.state),
                Target::Object(_) if f & field::PROCESS != 0 && f & field::ROOT == 0 => {
                    Body::Elsewhere
                }
                Target::Object(o) => Body::Object(o.to_packed()),
                Target::Relative(t) => Body::Relative(*t),
                Target::Free => return Err(HeapError::BadReference(*id)),
            };
            entries.push(PackedEntry { field: f, body });
        }
        Ok(Graph { ids, entries })
    }

    /// Mark from `roots` and free every entry not reached. Returns how many were freed.
    // D2-00897, D2-00881, D2-00885, D2-00895, D2-00886, D2-00893, D2-00884
    pub fn collect(&mut self, roots: impl IntoIterator<Item = u32>) -> usize {
        let mut marked = vec![false; self.entries.len()];
        let mut work: Vec<u32> = roots.into_iter().collect();
        work.extend(self.singletons.values().copied());
        work.extend(self.reserved.iter().copied());
        while let Some(mut id) = work.pop() {
            // Follow a chain of references into slots, marking each.
            loop {
                if id == NULL {
                    break;
                }
                let Some(m) = marked.get_mut(id as usize) else {
                    break;
                };
                if *m {
                    break;
                }
                *m = true;
                let e = &self.entries[id as usize];
                if e.field & field::SCAN == 0 {
                    break;
                }
                match &e.target {
                    Target::Relative(t) => id = *t,
                    Target::Object(o) => {
                        work.extend(o.references());
                        break;
                    }
                    Target::Free => break,
                }
            }
        }
        let mut freed = 0;
        for (i, e) in self.entries.iter_mut().enumerate() {
            if !marked[i] && !matches!(e.target, Target::Free) {
                *e = Entry::default();
                freed += 1;
            }
        }
        self.singletons.retain(|_, id| marked[*id as usize]);
        self.live -= freed;
        self.next = 0;
        self.added_since_collection = 0;
        freed
    }
}
