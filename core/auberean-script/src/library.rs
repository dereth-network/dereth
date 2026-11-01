//! The compiled script library as the machine keeps it.
//!
//! The library record is read into its sections by `auberean-assets`; this module keeps what running
//! code needs: the code words, the string literals, the packages with their functions, and the
//! tables a call is dispatched through. The debugging sections (function names, frames) are kept
//! too when the library carries them, for messages and for checking script calls against their
//! declared parameters.

use std::collections::HashMap;

use auberean_assets::script::{Frame, ScriptLibrary, Section};

use crate::value::FunctionId;

/// The script library's record id.
pub const RECORD: u32 = 0x5600_0005;

/// The base of the saved-state id range: a package's default instance is this plus its id.
pub const STATE_BASE: u32 = 0x7000_0000;

/// Package flag of a native package (its functions have no code).
pub const NATIVE_PACKAGE: u32 = 0x4;

/// Function flag of an event handler the other side may call.
pub const EVENT_HANDLER: u32 = 0x20;

/// The code offset that stands for "no code".
pub const NO_CODE: u32 = 0xFFFF_FFFF;

/// Why a library could not be taken in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError(pub String);

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LoadError {}

/// One function of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub id: FunctionId,
    /// The byte offset of its code, `None` for a native.
    pub offset: Option<u32>,
    pub size: u32,
    pub flags: u32,
}

/// One package: a class of script objects, or a native package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub base_name: String,
    pub flags: u32,
    /// Instance size in slots.
    pub size: u32,
    pub checksum: u32,
    pub id: u32,
    /// The export index of the base package.
    pub parent: Option<usize>,
    pub functions: Vec<Function>,
}

impl Package {
    /// Whether this is a native package.
    #[must_use]
    pub fn is_native(&self) -> bool {
        self.flags & NATIVE_PACKAGE != 0
    }
}

/// The element kinds of the container natives, from their names: `A` a 32-bit value, `L` a
/// 64-bit value, `R` a reference, `N` a reference that carries a cached hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Elem {
    Word,
    Long,
    Ref,
    HashedRef,
}

impl Elem {
    fn from_letter(c: u8) -> Option<Self> {
        Some(match c {
            b'A' => Self::Word,
            b'L' => Self::Long,
            b'R' => Self::Ref,
            b'N' => Self::HashedRef,
            _ => return None,
        })
    }
}

/// A container native, recognized from its package's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Container {
    /// `<K><V>Hash`: one value per key, in buckets.
    Hash(Elem, Elem),
    /// `<K><V>MultiHash`: several values per key.
    MultiHash(Elem, Elem),
    /// `<K><V>MultiMap`.
    MultiMap(Elem, Elem),
    /// `<T>HashSet`.
    HashSet(Elem),
    /// `<T>List`.
    List(Elem),
    /// `<T>Array`.
    Array(Elem),
}

/// What a container package's name says, and whether it is the container or its iterator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContainerName {
    pub container: Container,
    pub iterator: bool,
}

/// Read a container package's name: the element letters, then the family, then `Iter` for its
/// iterator. `None` for any other name.
#[must_use]
pub fn container_name(name: &str) -> Option<ContainerName> {
    let (base, iterator) = match name.strip_suffix("Iter") {
        Some(b) => (b, true),
        None => (name, false),
    };
    let families: [(&str, usize); 6] = [
        ("MultiHash", 2),
        ("MultiMap", 2),
        ("HashSet", 1),
        ("Hash", 2),
        ("List", 1),
        ("Array", 1),
    ];
    for (family, letters) in families {
        let Some(kinds) = base.strip_suffix(family) else {
            continue;
        };
        let k = kinds.as_bytes();
        // One set is named with two letters; its items are the first letter's kind.
        let set_of_two = family == "HashSet" && k.len() == 2;
        if k.len() != letters && !set_of_two {
            return None;
        }
        let e: Vec<Elem> = k
            .iter()
            .map(|c| Elem::from_letter(*c))
            .collect::<Option<_>>()?;
        let container = match family {
            "MultiHash" => Container::MultiHash(e[0], e[1]),
            "MultiMap" => Container::MultiMap(e[0], e[1]),
            "HashSet" => Container::HashSet(e[0]),
            "Hash" => Container::Hash(e[0], e[1]),
            "List" => Container::List(e[0]),
            _ => Container::Array(e[0]),
        };
        return Some(ContainerName {
            container,
            iterator,
        });
    }
    None
}

/// The compiled library.
#[derive(Clone, Debug, Default)]
pub struct Library {
    /// The code, as words. Byte offsets elsewhere are four times a word index.
    pub code: Vec<u32>,
    /// The string literals, as UTF-16 units.
    pub literals: Vec<Vec<u16>>,
    /// Every package, by export index.
    pub packages: Vec<Package>,
    /// Per package id, its export index.
    export_index: Vec<u32>,
    /// Per package id, the slot of each function number.
    function_slots: Vec<Vec<u32>>,
    /// Per export index, the code offset of each slot.
    slot_code: Vec<Vec<u32>>,
    /// Per package id, its instance size and checksum.
    package_info: Vec<(u32, u32)>,
    /// Package names to ids.
    by_name: HashMap<String, u32>,
    /// The functions the other side may call.
    pub server_callable: Vec<u32>,
    /// Function names by code offset (empty when the library carries none).
    pub function_names: Vec<(String, u32)>,
    /// Frames of packages and functions (empty when the library carries none).
    pub frames: Vec<Frame>,
    frame_index: HashMap<String, usize>,
    /// Native packages recognized as containers, by package id.
    containers: HashMap<u32, ContainerName>,
}

impl Library {
    /// Read a library record (`.wlib`) and take it in.
    pub fn from_record(bytes: &[u8]) -> Result<Self, LoadError> {
        let lib: ScriptLibrary =
            auberean_assets::decode(RECORD, bytes, &auberean_assets::Context::empty())
                .map_err(|e| LoadError(format!("the library record: {e}")))?;
        Self::from_sections(lib)
    }

    /// Take in a library already read into its sections.
    // D2-08000, D2-16688, D2-16527
    pub fn from_sections(lib: ScriptLibrary) -> Result<Self, LoadError> {
        let mut out = Self::default();
        let mut have_code = false;
        let mut have_dispatch = false;
        for section in lib.sections {
            match section {
                Section::Code(bytes) => {
                    if bytes.len() % 4 != 0 {
                        return Err(LoadError(format!(
                            "{} bytes of code, not whole words",
                            bytes.len()
                        )));
                    }
                    out.code = bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|w| u32::from_le_bytes(*w))
                        .collect();
                    have_code = true;
                }
                Section::Literals(lits) => {
                    out.literals = lits
                        .into_iter()
                        .map(|l| l.text.encode_utf16().collect())
                        .collect();
                }
                Section::Exports(exports) => {
                    out.packages = exports
                        .into_iter()
                        .map(|e| Package {
                            name: e.name,
                            base_name: e.base_name,
                            flags: e.flags,
                            size: e.size,
                            checksum: e.checksum,
                            id: e.package_id,
                            parent: usize::try_from(e.parent).ok(),
                            functions: e
                                .functions
                                .into_iter()
                                .map(|f| Function {
                                    name: f.name,
                                    id: FunctionId(f.id),
                                    offset: (f.offset != NO_CODE).then_some(f.offset),
                                    size: f.size,
                                    flags: f.flags,
                                })
                                .collect(),
                        })
                        .collect();
                }
                Section::Dispatch(d) => {
                    out.function_slots = d.function_slots;
                    out.slot_code = d.slot_tables;
                    out.package_info = d.package_sizes;
                    out.export_index = d.export_index;
                    out.by_name = d.names.into_iter().collect();
                    have_dispatch = true;
                }
                Section::ServerCallable(v) => {
                    out.server_callable = v.into_iter().map(|(f, _)| f).collect()
                }
                Section::FunctionNames(v) => out.function_names = v,
                Section::Frames(v) => out.frames = v,
                Section::Versions(_)
                | Section::Imports(_)
                | Section::SourceFiles(_)
                | Section::Lines(_) => {}
            }
        }
        if !have_code || !have_dispatch {
            return Err(LoadError(
                "a library without its code or its dispatch tables".into(),
            ));
        }
        out.frame_index = out
            .frames
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        for p in &out.packages {
            if p.is_native() {
                if let Some(c) = container_name(&p.name) {
                    out.containers.insert(p.id, c);
                }
            }
        }
        Ok(out)
    }

    /// The export index of a package id.
    #[must_use]
    pub fn export_of(&self, package: u32) -> Option<usize> {
        let i = *self.export_index.get(package as usize)?;
        (i != NO_CODE).then_some(i as usize)
    }

    /// A package by id.
    #[must_use]
    pub fn package(&self, package: u32) -> Option<&Package> {
        self.packages.get(self.export_of(package)?)
    }

    /// A package's name, or its id in hexadecimal when it has none.
    #[must_use]
    pub fn package_name(&self, package: u32) -> String {
        self.package(package)
            .map_or_else(|| format!("{package:#x}"), |p| p.name.clone())
    }

    /// A package id by name.
    // D2-16479
    #[must_use]
    pub fn package_id(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    /// A package's instance size in slots, as the dispatch tables give it.
    #[must_use]
    pub fn package_size(&self, package: u32) -> Option<u32> {
        self.package_info.get(package as usize).map(|p| p.0)
    }

    /// How many package ids the dispatch tables cover.
    #[must_use]
    pub fn package_count(&self) -> usize {
        self.package_info.len()
    }

    /// The code offset a call of `function` on an object of package `receiver` runs: the
    /// function's slot from the declaring package's table, then that slot of the receiver's
    /// package. `Ok(None)` when the receiver's package leaves the slot empty; `Err` when a table
    /// has no such entry.
    // D2-00963
    pub fn dispatch(&self, receiver: u32, function: FunctionId) -> Result<Option<u32>, String> {
        let slots = self
            .function_slots
            .get(function.package() as usize)
            .ok_or_else(|| format!("no function table for package {:#x}", function.package()))?;
        let slot = slots
            .get(function.number() as usize)
            .ok_or_else(|| format!("function {:#x} past its package's table", function.0))?
            & 0xFFFF;
        let export = self
            .export_of(receiver)
            .ok_or_else(|| format!("package {receiver:#x} is not exported"))?;
        let code = self
            .slot_code
            .get(export)
            .and_then(|t| t.get(slot as usize))
            .ok_or_else(|| format!("slot {slot} past package {receiver:#x}'s table"))?;
        Ok((*code != NO_CODE).then_some(*code))
    }

    /// The code offset a call from outside runs: a function named outright resolves in its own
    /// package, any other through `receiver`.
    // D2-00967
    pub fn function_offset(
        &self,
        receiver: u32,
        function: FunctionId,
    ) -> Result<Option<u32>, String> {
        if function.is_direct() {
            self.dispatch(
                function.package(),
                FunctionId(function.0 & !FunctionId::DIRECT),
            )
        } else {
            self.dispatch(receiver, function)
        }
    }

    /// Whether package `package` is `base` or derives from it.
    // D2-16528, D2-16534
    #[must_use]
    pub fn derives_from(&self, package: u32, base: u32) -> bool {
        let mut at = self.export_of(package);
        let mut guard = 0;
        while let Some(i) = at {
            let Some(p) = self.packages.get(i) else {
                return false;
            };
            if p.id == base {
                return true;
            }
            at = p.parent;
            guard += 1;
            if guard > self.packages.len() {
                return false;
            }
        }
        false
    }

    /// The container a native package is, recognized from its name.
    #[must_use]
    pub fn container(&self, native_type: u32) -> Option<ContainerName> {
        self.containers.get(&native_type).copied()
    }

    /// A native's name, `Package::function`, for messages.
    #[must_use]
    pub fn native_name(&self, id: u32) -> String {
        let pkg = id >> 16;
        if let Some(p) = self.package(pkg) {
            if let Some(f) = p.functions.iter().find(|f| f.id.0 == id) {
                return format!("{}::{}", p.name, f.name);
            }
        }
        format!("{pkg:#x}/{}", id & 0xFFFF)
    }

    /// A function's name by id, `Package::function`, when the library names it.
    #[must_use]
    pub fn function_name(&self, id: FunctionId) -> Option<String> {
        let id = FunctionId(id.0 & !FunctionId::DIRECT);
        let p = self.package(id.package())?;
        let f = p.functions.iter().find(|f| f.id == id)?;
        Some(format!("{}::{}", p.name, f.name))
    }

    /// The function a code offset falls in, by the debugging names (empty string when none).
    #[must_use]
    pub fn function_at(&self, offset: u32) -> &str {
        let i = self.function_names.partition_point(|(_, o)| *o <= offset);
        if i == 0 {
            return "";
        }
        &self.function_names[i - 1].0
    }

    /// A frame by its name (`Package` or `Package::function`), when the library carries frames.
    #[must_use]
    pub fn frame(&self, name: &str) -> Option<&Frame> {
        self.frame_index.get(name).map(|i| &self.frames[*i])
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (name grammar on hand-written names)
    use super::*;

    #[test]
    fn container_names_read_their_element_letters_and_family() {
        let n = container_name("ARHash").unwrap();
        assert_eq!(
            n,
            ContainerName {
                container: Container::Hash(Elem::Word, Elem::Ref),
                iterator: false
            }
        );
        let n = container_name("LAHashSetIter").unwrap();
        assert!(n.iterator);
        assert_eq!(n.container, Container::HashSet(Elem::Long));
        assert_eq!(
            container_name("AHashSet").unwrap().container,
            Container::HashSet(Elem::Word)
        );
        assert_eq!(
            container_name("RList").unwrap().container,
            Container::List(Elem::Ref)
        );
        assert_eq!(
            container_name("NAMultiHash").unwrap().container,
            Container::MultiHash(Elem::HashedRef, Elem::Word)
        );
        assert!(container_name("StringInfo").is_none());
        assert!(container_name("AAAHash").is_none());
        assert!(container_name("Hash").is_none());
    }
}
