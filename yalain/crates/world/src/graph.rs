//! Building the packed object graphs script-call arguments carry: script objects with their
//! slots, and native objects (lists, hash tables, records kept as bytes).
//!
//! A graph is written as the client packs one: the root first, then what it reaches, every entry
//! loaded and scannable. Reference ids are the graph's own, from 1. A script object's slot count
//! must be its package's size in the client's library, or the client drops the call; the layouts
//! the server builds are in [`crate::session`], held to the library by the corpus tier.

use auberean_script::graph::{field, Body, Entry, Graph, PackedObject};
use auberean_script::library::Elem;
use auberean_script::native_object::{empty_buckets, Hash, HashSet, Item, NativeObject};

/// One slot's value, as the script machine tags it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Slot {
    /// A 32-bit integer or enumeration.
    Int(u32),
    /// A 64-bit integer: two slots.
    Long(u64),
    /// A 64-bit float: two slots.
    Double(f64),
    /// A time: a 64-bit float in two slots.
    Time(f64),
    /// A reference to another entry of the graph, or none.
    Ref(Option<u32>),
    /// A 32-bit value with the tag it is to carry as it is (the script machine tags a value by
    /// how it was last set: a whole number, a truth value, an enumeration).
    Tagged(u8, u32),
}

/// Slot tags.
pub mod tag {
    pub use auberean_script::value::tag::{
        DOUBLE, FLOAT, FRESH as HIGH, INT, LONG_INT as LONG, REFERENCE as REF, TIME,
    };
    /// The low slot of a 64-bit value a native wrote (the script does not say what it holds).
    pub const MIXED_LONG: u8 =
        auberean_script::value::tag::LONG | auberean_script::value::tag::MIXED;
}

pub use auberean_script::value::NULL;

/// The flags every entry of a graph the server packs carries: an object, loaded, scannable.
pub const ENTRY: u32 = field::LOADED | field::SCAN | field::OBJECT;

/// A graph under construction.
#[derive(Debug, Default)]
pub struct Builder {
    ids: Vec<u32>,
    entries: Vec<Entry>,
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The id the next entry will get.
    #[must_use]
    pub fn next_id(&self) -> u32 {
        u32::try_from(self.ids.len()).unwrap_or(u32::MAX - 1) + 1
    }

    fn push(&mut self, o: PackedObject) -> u32 {
        let id = self.next_id();
        self.ids.push(id);
        self.entries.push(Entry {
            field: ENTRY,
            body: Body::Object(o),
        });
        id
    }

    /// Reserve an entry for a script object whose slots are filled later (when it refers to
    /// entries added after it).
    pub fn reserve(&mut self, package: u16) -> u32 {
        self.push(PackedObject {
            state: 0,
            package,
            native_type: 0,
            native: None,
            slots: Vec::new(),
            tags: Vec::new(),
        })
    }

    /// Fill a reserved script object's slots.
    pub fn fill(&mut self, id: u32, slots: &[Slot]) {
        let Some(i) = self.ids.iter().position(|x| *x == id) else {
            return;
        };
        if let Body::Object(o) = &mut self.entries[i].body {
            let (s, t) = encode(slots);
            o.slots = s;
            o.tags = t;
        }
    }

    /// A script object of a package with these slots.
    pub fn object(&mut self, package: u16, slots: &[Slot]) -> u32 {
        let id = self.reserve(package);
        self.fill(id, slots);
        id
    }

    /// Put another native object in the place of one added before (a list whose items are added
    /// after it).
    pub fn replace_native(&mut self, id: u32, n: NativeObject) {
        let Some(i) = self.ids.iter().position(|x| *x == id) else {
            return;
        };
        if let Body::Object(o) = &mut self.entries[i].body {
            o.native = Some(n);
        }
    }

    /// A singleton: the object filled from a saved state on first use.
    pub fn singleton(&mut self, state: u32) -> u32 {
        let id = self.next_id();
        self.ids.push(id);
        self.entries.push(Entry {
            field: ENTRY | field::SINGLETON,
            body: Body::Singleton(state),
        });
        id
    }

    /// A native object of a type.
    pub fn native(&mut self, native_type: u16, n: NativeObject) -> u32 {
        self.push(PackedObject {
            state: 0,
            package: 0xFFFF,
            native_type,
            native: Some(n),
            slots: Vec::new(),
            tags: Vec::new(),
        })
    }

    /// The finished graph: its root is the first entry added.
    #[must_use]
    pub fn graph(self) -> Graph {
        Graph {
            ids: self.ids,
            entries: self.entries,
        }
    }
}

fn encode(slots: &[Slot]) -> (Vec<u32>, Vec<u8>) {
    let (mut s, mut t) = (Vec::new(), Vec::new());
    let two = |v: u64, tg: u8, s: &mut Vec<u32>, t: &mut Vec<u8>| {
        #[allow(clippy::cast_possible_truncation)]
        s.extend([v as u32, (v >> 32) as u32]);
        t.extend([tg, tag::HIGH]);
    };
    for slot in slots {
        match *slot {
            Slot::Int(v) => {
                s.push(v);
                t.push(tag::INT);
            }
            Slot::Long(v) => two(v, tag::LONG, &mut s, &mut t),
            Slot::Double(v) => two(v.to_bits(), tag::DOUBLE, &mut s, &mut t),
            Slot::Time(v) => two(v.to_bits(), tag::TIME, &mut s, &mut t),
            Slot::Ref(r) => {
                s.push(r.unwrap_or(NULL));
                t.push(tag::REF);
            }
            Slot::Tagged(tg, v) => {
                s.push(v);
                t.push(tg);
            }
        }
    }
    (s, t)
}

/// A hash table with its entries inserted in this order: each goes to the head of its bucket's
/// chain, as the client's tables insert.
#[must_use]
pub fn hash(key: Elem, value: Elem, size_index: u8, inserted: &[(Item, Item)]) -> NativeObject {
    let mut buckets = empty_buckets(size_index);
    let n = buckets.len();
    for (k, v) in inserted {
        buckets[k.bucket(n)].insert(0, (*k, *v));
    }
    NativeObject::Hash(Hash {
        key,
        value,
        size_index,
        buckets,
    })
}

/// The bucket count at a place in the list of bucket counts.
#[must_use]
pub fn bucket_count(size_index: u8) -> u32 {
    auberean_script::native_object::BUCKET_SIZES[usize::from(size_index)]
}

/// A set with its items inserted in this order: each goes to the head of its bucket's chain, as
/// the client's sets insert.
#[must_use]
pub fn set(item: Elem, size_index: u8, inserted: &[Item]) -> NativeObject {
    let mut buckets = empty_buckets(size_index);
    let n = buckets.len();
    for i in inserted {
        buckets[i.bucket(n)].insert(0, *i);
    }
    NativeObject::HashSet(HashSet {
        item,
        size_index,
        buckets,
    })
}

/// A list of items.
#[must_use]
pub fn list(item: Elem, items: Vec<Item>) -> NativeObject {
    NativeObject::Sequence { item, items }
}
