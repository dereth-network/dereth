//! The packed object graph: a root reference and everything it reaches, in one buffer.
//!
//! Saved script states hold one, a script-call argument of object type is one, and copying an
//! object goes through one. The layout:
//!
//! ```text
//! u32 count, u32 ids[count]            the graph's reference ids; ids[0] is the root
//! count x entry, in the same order:
//!   u32 field                          the reference's flags (the collection epoch cleared):
//!                                      bits 8-9 its kind (1 an object, 2 a reference into a
//!                                      slot), 26 safe to scan, 27 owned by a process, 28 the root
//!                                      of the graph it was packed with, 29 a singleton, 30 loaded
//!   an object, singleton:              u32 the id of its saved state
//!   an object owned by a process and
//!   not the root:                      nothing (it is not packed with the graph)
//!   any other object:                  the object (below)
//!   a reference into a slot:           u32 the referenced id
//! object:
//!   u32 saved-state id                 the state it came from
//!   u32 package << 16 | native type    package 0xFFFF = none; native type 0 = a script object
//!   native object                      its own packed form, when the native type is not 0
//!   u32 n                              slots
//!   u32 data[n], u8 tags[n], pad to 4
//! ```
//!
//! A script object's slot count must equal its package's size in the library, or the graph is
//! refused: saved states and arguments are tied to the library they were packed against.
//! Reference ids inside slots (tagged as references) and inside native containers are the
//! graph's own ids; reading a graph into the reference table gives every entry a new id and
//! rewrites them.

use crate::library::Library;
use crate::native_object::NativeObject;
use crate::wire::{In, Out, Result};

/// Reference flag bits.
pub mod field {
    /// The kind of reference: bits 8-9.
    pub const KIND: u32 = 0x300;
    /// An object.
    pub const OBJECT: u32 = 0x100;
    /// A reference into a slot of an object or of a stack.
    pub const RELATIVE: u32 = 0x200;
    /// The slot offset a reference into a slot adds: bits 10-25.
    pub const OFFSET_SHIFT: u32 = 10;
    /// The collector may scan this object's contents.
    pub const SCAN: u32 = 0x0400_0000;
    /// Owned by a process: packed only as the root.
    pub const PROCESS: u32 = 0x0800_0000;
    /// The root of the graph it was packed with.
    pub const ROOT: u32 = 0x1000_0000;
    /// A singleton, filled from its saved state on first use.
    pub const SINGLETON: u32 = 0x2000_0000;
    /// The object's slots are present.
    pub const LOADED: u32 = 0x4000_0000;
    /// The collection epoch, which packing clears.
    pub const EPOCH: u32 = 0xFF;

    /// The slot offset of a reference into a slot.
    #[must_use]
    pub const fn offset(f: u32) -> u32 {
        (f >> OFFSET_SHIFT) & 0xFFFF
    }
}

/// One packed object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedObject {
    /// The saved state it came from.
    pub state: u32,
    /// The package, 16 bits as stored (`0xFFFF` for none).
    pub package: u16,
    /// The native type, 0 for a script object.
    pub native_type: u16,
    pub native: Option<NativeObject>,
    pub slots: Vec<u32>,
    pub tags: Vec<u8>,
}

impl PackedObject {
    /// The package id, with 16 bits of ones read as none.
    #[must_use]
    pub fn package_id(&self) -> u32 {
        if self.package == 0xFFFF {
            crate::value::NULL
        } else {
            u32::from(self.package)
        }
    }

    // D2-16266
    fn read(r: &mut In<'_>, lib: &Library) -> Result<Self> {
        let state = r.u32()?;
        let w = r.u32()?;
        #[allow(clippy::cast_possible_truncation)]
        let (package, native_type) = ((w >> 16) as u16, w as u16);
        let native = if native_type != 0 {
            Some(NativeObject::read(r, lib, u32::from(native_type))?)
        } else {
            None
        };
        let n = r.u32()? as usize;
        if native_type == 0 {
            let want = lib.package_size(u32::from(package));
            if want.map(|w| w as usize) != Some(n) {
                return r.fail(format!(
                    "an object of package {package:#x} with {n} slots; the library says {want:?}"
                ));
            }
        }
        let n = r.count(n, 5)?;
        let data = r.bytes(n * 4)?;
        let slots = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let tags = r.bytes(n)?.to_vec();
        r.align()?;
        Ok(Self {
            state,
            package,
            native_type,
            native,
            slots,
            tags,
        })
    }

    // D2-16265
    fn write(&self, w: &mut Out) -> core::result::Result<(), String> {
        w.u32(self.state);
        w.u32(u32::from(self.package) << 16 | u32::from(self.native_type));
        if let Some(n) = &self.native {
            n.write(w)?;
        }
        w.u32(u32::try_from(self.slots.len()).map_err(|e| e.to_string())?);
        for s in &self.slots {
            w.u32(*s);
        }
        w.bytes(&self.tags);
        w.align();
        Ok(())
    }
}

/// What an entry of a graph holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// A singleton: the id of its saved state.
    Singleton(u32),
    /// An object packed in full.
    Object(PackedObject),
    /// An object owned by a process and not the root: nothing is packed.
    Elsewhere,
    /// A reference into a slot: the referenced id.
    Relative(u32),
}

/// One entry: its flags and what it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub field: u32,
    pub body: Body,
}

/// A packed object graph.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Graph {
    /// The graph's ids; the first is the root.
    pub ids: Vec<u32>,
    pub entries: Vec<Entry>,
}

impl Graph {
    /// The root's id in the graph, if the graph has any entry.
    #[must_use]
    pub fn root(&self) -> Option<u32> {
        self.ids.first().copied()
    }

    /// Read one graph. The reader's alignment must count from a four-byte boundary.
    // D2-00976, D2-00975, D2-16432
    pub fn read(r: &mut In<'_>, lib: &Library) -> Result<Self> {
        let n = r.u32()? as usize;
        let n = r.count(n, 8)?;
        let mut ids = Vec::with_capacity(n);
        for _ in 0..n {
            ids.push(r.u32()?);
        }
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let at = r.pos();
            let f = r.u32()?;
            let body = match f & field::KIND {
                field::OBJECT if f & field::SINGLETON != 0 => Body::Singleton(r.u32()?),
                field::OBJECT if f & field::PROCESS == 0 || f & field::ROOT != 0 => {
                    Body::Object(PackedObject::read(r, lib)?)
                }
                field::OBJECT => Body::Elsewhere,
                field::RELATIVE => Body::Relative(r.u32()?),
                k => {
                    return Err(crate::wire::CodecError {
                        at,
                        what: format!("a reference of kind {}", k >> 8),
                    });
                }
            };
            entries.push(Entry { field: f, body });
        }
        Ok(Self { ids, entries })
    }

    /// Read a graph that must fill `bytes` exactly.
    pub fn decode(bytes: &[u8], lib: &Library) -> Result<Self> {
        let mut r = In::new(bytes);
        let g = Self::read(&mut r, lib)?;
        r.finish()?;
        Ok(g)
    }

    /// Write the graph. The writer must stand at a four-byte boundary.
    // D2-16492, D2-16403, D2-16431
    pub fn write(&self, w: &mut Out) -> core::result::Result<(), String> {
        if self.ids.len() != self.entries.len() {
            return Err(format!(
                "{} ids for {} entries",
                self.ids.len(),
                self.entries.len()
            ));
        }
        w.u32(u32::try_from(self.ids.len()).map_err(|e| e.to_string())?);
        for id in &self.ids {
            w.u32(*id);
        }
        for e in &self.entries {
            w.u32(e.field & !field::EPOCH);
            match &e.body {
                Body::Singleton(did) => w.u32(*did),
                Body::Object(o) => o.write(w)?,
                Body::Elsewhere => {}
                Body::Relative(id) => w.u32(*id),
            }
        }
        Ok(())
    }

    /// The graph's packed bytes.
    pub fn encode(&self) -> core::result::Result<Vec<u8>, String> {
        let mut w = Out::new();
        self.write(&mut w)?;
        Ok(w.buf)
    }
}

/// A saved script state: its packed graph and the package of its root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedState {
    pub graph: Graph,
    pub package: u32,
}

impl SavedState {
    /// Read a saved state from its decoded record: the graph's bytes and the package id.
    // D2-16400
    pub fn decode(graph: &[u8], package: u32, lib: &Library) -> Result<Self> {
        Ok(Self {
            graph: Graph::decode(graph, lib)?,
            package,
        })
    }
}
