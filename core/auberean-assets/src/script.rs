//! The game's scripts and what they keep: the script library (`.wlib`), saved script states
//! (`.wc`), qualities (`0x80000000`-`0x81FFFFFF`), quality filters (`.wqf`), the word filter and
//! conversations.
//!
//! The script library is read here as a container: its sections, each to its declared size, and
//! inside each the tables a loader keeps (packages, functions, literals, links, dispatch tables,
//! debugging names). What the bytecode does is the script machine's business. A saved script
//! state is a package's packed object graph, kept as bytes until the script machine unpacks it.

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use auberean_primitives::DataId;

use crate::common::Position;
use crate::property::StringInfo;
use crate::{Context, Decode};

// ---------------------------------------------------------------------------------------------
// The script library
// ---------------------------------------------------------------------------------------------

/// The four bytes that open a script library's stream, `BS\x01\x1A`.
pub const LIBRARY_MAGIC: u32 = 0x1A01_5342;

/// The stream version this client reads. A stream of another version still loads; the client
/// only logs the mismatch.
pub const LIBRARY_VERSION: u32 = 15;

/// The compiled script library: a magic word, a version, and typed sections.
#[derive(Clone, Debug, PartialEq, Default)]
// D2-16785, D2-05074, D2-16790, D2-16786, D2-16791, D2-16792, D2-16794, D2-16606, D2-16576, D2-16524
// D2-16537, D2-16556, D2-16602, D2-16604, D2-16544
pub struct ScriptLibrary {
    pub version: u32,
    pub sections: Vec<Section>,
}

/// One section of the script library, read by its type.
#[derive(Clone, Debug, PartialEq)]
pub enum Section {
    /// Type 5: the version entries, each a pair of narrow strings the loader checks in turn
    /// (none in the retail library).
    // D2-16543
    Versions(Vec<(String, String)>),
    /// Type 1: the bytecode every code offset points into.
    Code(Vec<u8>),
    /// Type 2: the string literals, and for each the code offsets that use it.
    Literals(Vec<Literal>),
    /// Type 3: the link table: every package the code refers to and where.
    Imports(Vec<Import>),
    /// Type 4: every package the library defines, with its functions.
    Exports(Vec<Export>),
    /// Type 6: the dispatch tables.
    Dispatch(Box<DispatchTables>),
    /// Type 7: the functions a server may call, by function id.
    ServerCallable(Vec<(u32, u32)>),
    /// Type 256: function names and their code offsets.
    // D2-16678, D2-16680, D2-04291
    FunctionNames(Vec<(String, u32)>),
    /// Type 257: source files with their text (empty in the retail library).
    // D2-16560, D2-00657, D2-00679
    SourceFiles(Vec<(String, String)>),
    /// Type 258: per source file, code offsets and the line each starts.
    Lines(Vec<SourceLines>),
    /// Type 259: the frames of every package and function: names and types of their slots.
    Frames(Vec<Frame>),
}

impl Section {
    /// The section's stored type number.
    #[must_use]
    pub fn type_number(&self) -> u32 {
        match self {
            Self::Code(_) => 1,
            Self::Literals(_) => 2,
            Self::Imports(_) => 3,
            Self::Exports(_) => 4,
            Self::Versions(_) => 5,
            Self::Dispatch(_) => 6,
            Self::ServerCallable(_) => 7,
            Self::FunctionNames(_) => 256,
            Self::SourceFiles(_) => 257,
            Self::Lines(_) => 258,
            Self::Frames(_) => 259,
        }
    }
}

/// A string literal and the code offsets of the operands that name it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Literal {
    pub text: String,
    pub uses: Vec<u32>,
}

/// One package the code refers to: its name, the symbols of it the code uses and where, and where
/// the package's own id appears.
#[derive(Clone, Debug, PartialEq, Default)]
// D2-16582, D2-00660, D2-00680
pub struct Import {
    pub package: String,
    pub symbols: Vec<ImportSymbol>,
    pub uses: Vec<u32>,
}

/// One symbol of an imported package: its name, the code offsets that use it, and a word that is
/// 0 in every retail entry.
#[derive(Clone, Debug, PartialEq, Default)]
// D2-16585, D2-00678
pub struct ImportSymbol {
    pub name: String,
    pub uses: Vec<u32>,
    pub from: i32,
}

/// One package the library defines.
#[derive(Clone, Debug, PartialEq, Default)]
// D2-16609, D2-00732, D2-00736, D2-16615, D2-00731, D2-16621
pub struct Export {
    pub name: String,
    pub base_name: String,
    pub flags: u32,
    /// The instance size in 4-byte slots.
    pub size: u32,
    pub checksum: u32,
    /// The package id the code names it by.
    pub package_id: u32,
    /// The export index of the base package, -1 for none.
    pub parent: i32,
    /// Fields named for saving: name, slot and tag.
    pub checkpoints: Vec<(String, u32, u8)>,
    pub functions: Vec<ExportFunction>,
}

/// One function of a package.
#[derive(Clone, Debug, PartialEq, Default)]
// D2-16612, D2-16613, D2-00673
pub struct ExportFunction {
    pub name: String,
    pub id: u32,
    /// The code offset, `0xFFFFFFFF` for a function the client implements natively.
    pub offset: u32,
    /// Bytes of code.
    pub size: u32,
    pub flags: u32,
    pub dependencies: Vec<u32>,
}

/// The tables a call is dispatched through.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DispatchTables {
    /// Per package id, the slot of each of its function numbers.
    pub function_slots: Vec<Vec<u32>>,
    /// Per export index, the code offset of each slot.
    pub slot_tables: Vec<Vec<u32>>,
    /// Per package id, its instance size and checksum.
    pub package_sizes: Vec<(u32, u32)>,
    /// Per package id, its export index (`0xFFFFFFFF` where none).
    pub export_index: Vec<u32>,
    /// Package names to package ids.
    pub names: Vec<(String, u32)>,
}

/// One source file's line table.
#[derive(Clone, Debug, PartialEq, Default)]
// D2-16595, D2-00656, D2-00681
pub struct SourceLines {
    pub path: String,
    /// `(code offset, line)` pairs.
    pub lines: Vec<(u32, u32)>,
}

/// The slots of one package (its fields) or function (its parameters and locals).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Frame {
    pub name: String,
    /// 1 a function, 2 a package.
    pub kind: u32,
    /// Slots in all.
    pub size: u32,
    pub members: Vec<FrameMember>,
}

/// One slot of a frame.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FrameMember {
    /// The slot: fields from 0; a function's locals from 0, the receiver at -1, the parameters
    /// below it.
    pub slot: i32,
    /// 2 integer, 3 float, 5 time, 6 type name, 7 string, 8 enumeration, 9 package.
    pub kind: u32,
    /// Bit `0x2`: a 64-bit value in two slots.
    pub flags: u32,
    pub name: String,
    pub type_name: String,
}

/// A package's fields named for saving: a 32-bit count, then each name, its slot, and its tag in
/// one byte brought to a multiple of four.
// D2-00714, D2-00716
fn checkpoints(r: &mut Reader<'_>) -> Result<Vec<(String, u32, u8)>> {
    r.list(12, |r| {
        let name = r.narrow_string()?;
        let slot = r.u32()?;
        let tag = r.u8()?;
        r.align()?;
        Ok((name, slot, tag))
    })
}

/// A function name of the debugging table: plain bytes up to a zero byte.
fn zero_terminated(r: &mut Reader<'_>) -> Result<String> {
    let rest = r.rest();
    let Some(n) = rest.iter().position(|b| *b == 0) else {
        return r.fail("a name without its terminating zero");
    };
    let s: String = rest[..n].iter().map(|b| char::from(*b)).collect();
    r.skip(n + 1)?;
    Ok(s)
}

impl Section {
    /// Read one section's contents, all of `bytes`, by its type number.
    // D2-16533, D2-16546, D2-16564, D2-16688, D2-00699, D2-00733, D2-16515, D2-16516, D2-16517, D2-16607, D2-16610, D2-16580, D2-16583, D2-16535, D2-16597, D2-16590, D2-16558, D2-16548, D2-16550, D2-16557
    // D2-16518, D2-16519, D2-16520, D2-16526
    // D2-16568, D2-16614, D2-16617, D2-16589, D2-16596, D2-16684, D2-16685, D2-16538, D2-16603, D2-16522
    pub fn read(kind: u32, bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let r = &mut reader;
        let section = match kind {
            1 => {
                let n = r.count(1)?;
                Self::Code(r.bytes(n)?.to_vec())
            }
            2 => {
                let texts = r.list(4, Reader::wide_string)?;
                let uses = r.list(4, Reader::u32_list)?;
                r.ensure(uses.len() == texts.len(), || {
                    format!("{} literals, {} use lists", texts.len(), uses.len())
                })?;
                Self::Literals(
                    texts
                        .into_iter()
                        .zip(uses)
                        .map(|(text, uses)| Literal { text, uses })
                        .collect(),
                )
            }
            3 => Self::Imports(r.list(12, |r| {
                let package = r.narrow_string()?;
                let symbols = r.list(12, |r| {
                    Ok(ImportSymbol {
                        name: r.narrow_string()?,
                        uses: r.u32_list()?,
                        from: r.i32()?,
                    })
                })?;
                Ok(Import {
                    package,
                    symbols,
                    uses: r.u32_list()?,
                })
            })?),
            4 => Self::Exports(r.list(36, |r| {
                Ok(Export {
                    name: r.narrow_string()?,
                    base_name: r.narrow_string()?,
                    flags: r.u32()?,
                    size: r.u32()?,
                    checksum: r.u32()?,
                    package_id: r.u32()?,
                    parent: r.i32()?,
                    checkpoints: checkpoints(r)?,
                    functions: r.list(24, |r| {
                        Ok(ExportFunction {
                            name: r.narrow_string()?,
                            id: r.u32()?,
                            offset: r.u32()?,
                            size: r.u32()?,
                            flags: r.u32()?,
                            dependencies: r.u32_list()?,
                        })
                    })?,
                })
            })?),
            // D2-16541, D2-16531, D2-16575
            5 => Self::Versions(r.list(8, |r| Ok((r.narrow_string()?, r.narrow_string()?)))?),
            6 => Self::Dispatch(Box::new(DispatchTables {
                function_slots: r.list(4, Reader::u32_list)?,
                slot_tables: r.list(4, Reader::u32_list)?,
                package_sizes: r.list(8, |r| Ok((r.u32()?, r.u32()?)))?,
                export_index: r.u32_list()?,
                // D2-16599
                names: r.hash_table(8, |r| Ok((r.narrow_string()?, r.u32()?)))?,
            })),
            // D2-16577
            7 => Self::ServerCallable(r.hash_table(8, |r| Ok((r.u32()?, r.u32()?)))?),
            256 => {
                let mut v = Vec::new();
                while !r.at_end() {
                    let name = zero_terminated(r)?;
                    v.push((name, r.u32()?));
                }
                Self::FunctionNames(v)
            }
            257 => Self::SourceFiles(r.list(8, |r| Ok((r.narrow_string()?, r.narrow_string()?)))?),
            258 => Self::Lines(r.list(8, |r| {
                Ok(SourceLines {
                    path: r.narrow_string()?,
                    lines: r.list(8, |r| Ok((r.u32()?, r.u32()?)))?,
                })
            })?),
            // D2-16571, D2-08017
            259 => Self::Frames(r.list(16, |r| {
                Ok(Frame {
                    name: r.narrow_string()?,
                    kind: r.u32()?,
                    size: r.u32()?,
                    members: r.list(20, |r| {
                        Ok(FrameMember {
                            slot: r.i32()?,
                            kind: r.u32()?,
                            flags: r.u32()?,
                            name: r.narrow_string()?,
                            type_name: r.narrow_string()?,
                        })
                    })?,
                })
            })?),
            other => return r.fail(format!("section type {other}")),
        };
        r.finish()?;
        Ok(section)
    }
}

impl Decode for ScriptLibrary {
    /// The id, the magic word, the version, then sections to the end of the record: a type word, a
    /// size word, that many bytes, alignment. A section of a type the client does not know fails
    /// the load.
    // D2-16793, D2-16561, D2-16562
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let magic = r.u32()?;
        r.ensure(magic == LIBRARY_MAGIC, || format!("magic {magic:#x}"))?;
        let version = r.u32()?;
        let mut sections = Vec::new();
        while !r.at_end() {
            let kind = r.u32()?;
            let size = r.u32()? as usize;
            let at = r.pos();
            let bytes = r.bytes(size)?;
            let section = Section::read(kind, bytes).map_err(|mut e| {
                e.what = format!("section {kind}: {}", e.what);
                e.at += at;
                e
            })?;
            sections.push(section);
            r.align()?;
        }
        Ok(Self { version, sections })
    }
}

// ---------------------------------------------------------------------------------------------
// Saved script states
// ---------------------------------------------------------------------------------------------

/// A package's saved state: its packed object graph, and the package it belongs to. The record
/// is made, emptied and let go of with the value.
// D2-16772, D2-05080, D2-16779, D2-16773, D2-16767, D2-16780, D2-16396, D2-16398, D2-16399
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScriptState {
    /// The packed object graph, unpacked by the script machine when an object of the package is
    /// made.
    pub graph: Vec<u8>,
    pub package: u32,
}

impl crate::Encode for ScriptState {
    /// As [`ScriptState`] decodes: the id, the graph's byte count and bytes, the package id.
    // D2-16770, D2-16776, D2-16769, D2-16775, D2-16397, D2-16736
    fn encode(&self, id: u32, w: &mut auberean_dat::Writer) {
        w.u32(id);
        w.count(self.graph.len());
        w.bytes(&self.graph);
        w.u32(self.package);
    }
}

impl Decode for ScriptState {
    /// The id, a byte count, the graph's bytes, the package id.
    // D2-16771, D2-16400, D2-16777
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let n = r.count(1)?;
        let graph = r.bytes(n)?.to_vec();
        let package = r.u32()?;
        Ok(Self { graph, package })
    }
}

// ---------------------------------------------------------------------------------------------
// Qualities
// ---------------------------------------------------------------------------------------------

/// What an object is to the game: its package and entity, names, icon, the objects it belongs
/// to, counts, value and the flags the physics reads. Every field is optional; a mask word says
/// which are stored.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ObjectDescription {
    pub mask: u32,
    pub package: Option<u32>,
    pub entity: Option<DataId>,
    pub flags: Option<u32>,
    pub name: Option<StringInfo>,
    pub plural_name: Option<StringInfo>,
    pub icon: Option<DataId>,
    pub container: Option<u64>,
    pub wielder: Option<u64>,
    pub monarch: Option<u64>,
    pub originator: Option<u64>,
    pub claimant: Option<u64>,
    pub killer: Option<u64>,
    pub pet_summoner: Option<u64>,
    pub scale: Option<f32>,
    pub quantity: Option<i32>,
    pub value: Option<i32>,
    pub faction: Option<i32>,
    pub pk_always_true: Option<i32>,
    pub pk_always_false: Option<i32>,
    pub physics_type_low: Option<i32>,
    pub physics_type_high: Option<i32>,
    pub movement_ethereal_low: Option<i32>,
    pub movement_ethereal_high: Option<i32>,
    pub placement_ethereal_low: Option<i32>,
    pub placement_ethereal_high: Option<i32>,
    pub durability: Option<i32>,
    pub durability_max: Option<i32>,
}

impl ObjectDescription {
    /// The records the description names: its icon, then its name's and plural name's tables
    /// (and the tables of the references inside them).
    // D2-12067
    pub fn sub_ids(&self, out: &mut auberean_dat::QualifiedIds) {
        out.add(self.icon.map_or(0, |i| i.0));
        for s in [&self.name, &self.plural_name].into_iter().flatten() {
            s.sub_ids(out);
        }
    }

    /// The mask word, then each masked field in this order (mask bit in parentheses): package
    /// (`0x1`), entity (`0x2000000`), flags (`0x2`), name (`0x4`), plural name (`0x8`), icon
    /// (`0x10`), container (`0x20`), wielder (`0x40`), monarch (`0x100`), originator (`0x200`),
    /// claimant (`0x20000000`), killer (`0x40000000`), pet summoner (`0x4000000`), scale (`0x80`),
    /// quantity (`0x800`), value (`0x1000`), faction (`0x2000`), the two player-killer words
    /// (`0x200000`, `0x400000`), the physics type's halves (`0x8000`, `0x10000`), the movement
    /// and placement ethereal words' halves (`0x20000`, `0x40000`, `0x80000`, `0x100000`), and
    /// durability and its maximum (`0x8000000`, `0x10000000`). Other mask bits store nothing.
    // D2-12071
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mask = r.u32()?;
        let has = |bit: u32| mask & bit != 0;
        let mut d = Self {
            mask,
            ..Self::default()
        };
        if has(0x1) {
            d.package = Some(r.u32()?);
        }
        if has(0x200_0000) {
            d.entity = Some(r.data_id()?);
        }
        if has(0x2) {
            d.flags = Some(r.u32()?);
        }
        if has(0x4) {
            d.name = Some(StringInfo::read(r)?);
        }
        if has(0x8) {
            d.plural_name = Some(StringInfo::read(r)?);
        }
        if has(0x10) {
            d.icon = Some(r.data_id()?);
        }
        for (bit, slot) in [
            (0x20, &mut d.container),
            (0x40, &mut d.wielder),
            (0x100, &mut d.monarch),
            (0x200, &mut d.originator),
            (0x2000_0000, &mut d.claimant),
            (0x4000_0000, &mut d.killer),
            (0x400_0000, &mut d.pet_summoner),
        ] {
            if has(bit) {
                *slot = Some(r.u64()?);
            }
        }
        if has(0x80) {
            d.scale = Some(r.f32()?);
        }
        for (bit, slot) in [
            (0x800, &mut d.quantity),
            (0x1000, &mut d.value),
            (0x2000, &mut d.faction),
            (0x20_0000, &mut d.pk_always_true),
            (0x40_0000, &mut d.pk_always_false),
            (0x8000, &mut d.physics_type_low),
            (0x1_0000, &mut d.physics_type_high),
            (0x2_0000, &mut d.movement_ethereal_low),
            (0x4_0000, &mut d.movement_ethereal_high),
            (0x8_0000, &mut d.placement_ethereal_low),
            (0x10_0000, &mut d.placement_ethereal_high),
            (0x800_0000, &mut d.durability),
            (0x1000_0000, &mut d.durability_max),
        ] {
            if has(bit) {
                *slot = Some(r.i32()?);
            }
        }
        Ok(d)
    }
}

/// An object's default qualities: its description and tables of typed values, each keyed by a
/// quality id. A flags word says which are stored.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Qualities {
    pub flags: u32,
    pub description: Option<ObjectDescription>,
    pub ints: Vec<(u32, i32)>,
    pub bools: Vec<(u32, i32)>,
    pub floats: Vec<(u32, f32)>,
    pub times: Vec<(u32, f64)>,
    pub strings: Vec<(u32, String)>,
    pub data_ids: Vec<(u32, DataId)>,
    pub instance_ids: Vec<(u32, u64)>,
    pub positions: Vec<(u32, Position)>,
    pub string_infos: Vec<(u32, StringInfo)>,
    pub packages: Vec<(u32, u32)>,
    pub long_ints: Vec<(u32, i64)>,
}

impl Qualities {
    /// The records a qualities record names: each data id it holds (not 0), the tables of its
    /// string references (and of the references inside them), then what its description names.
    // D2-07289
    pub fn sub_ids(&self, out: &mut auberean_dat::QualifiedIds) {
        for (_, id) in &self.data_ids {
            out.add(id.0);
        }
        for (_, s) in &self.string_infos {
            s.sub_ids(out);
        }
        if let Some(d) = &self.description {
            d.sub_ids(out);
        }
    }
}

/// The flags a qualities record may carry: one per table.
const QUALITY_FLAGS: u32 = 0xFFF;

impl Decode for Qualities {
    /// A flags word **before** the id, the id, then in this order the description (`0x1`) and
    /// hash tables of integers (`0x2`), truth values (`0x4`), floats (`0x8`), times (`0x10`, 64-bit
    /// floats), strings (`0x80`), data ids (`0x20`), instance ids (`0x40`), positions (`0x100`),
    /// text references (`0x200`), packages (`0x400`) and 64-bit integers (`0x800`).
    // D2-07350, D2-07314, D2-01473, D2-07329, D2-07333, D2-00124, D2-02931, D2-07342, D2-07347, D2-07320
    // D2-05081, D2-07255, D2-07251
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let flags = r.u32()?;
        r.ensure(flags & !QUALITY_FLAGS == 0, || {
            format!("qualities flags {flags:#x}")
        })?;
        r.expect_id(id)?;
        let has = |bit: u32| flags & bit != 0;
        let mut q = Self {
            flags,
            ..Self::default()
        };
        if has(0x1) {
            q.description = Some(ObjectDescription::read(r)?);
        }
        if has(0x2) {
            q.ints = r.hash_table(8, |r| Ok((r.u32()?, r.i32()?)))?;
        }
        if has(0x4) {
            q.bools = r.hash_table(8, |r| Ok((r.u32()?, r.i32()?)))?;
        }
        if has(0x8) {
            q.floats = r.hash_table(8, |r| Ok((r.u32()?, r.f32()?)))?;
        }
        if has(0x10) {
            q.times = r.hash_table(12, |r| Ok((r.u32()?, r.f64()?)))?;
        }
        if has(0x80) {
            q.strings = r.hash_table(8, |r| Ok((r.u32()?, r.narrow_string()?)))?;
        }
        if has(0x20) {
            q.data_ids = r.hash_table(8, |r| Ok((r.u32()?, r.data_id()?)))?;
        }
        if has(0x40) {
            q.instance_ids = r.hash_table(12, |r| Ok((r.u32()?, r.u64()?)))?;
        }
        if has(0x100) {
            q.positions = r.hash_table(36, |r| Ok((r.u32()?, Position::read(r)?)))?;
        }
        if has(0x200) {
            q.string_infos = r.hash_table(16, |r| Ok((r.u32()?, StringInfo::read(r)?)))?;
        }
        if has(0x400) {
            q.packages = r.hash_table(8, |r| Ok((r.u32()?, r.u32()?)))?;
        }
        if has(0x800) {
            q.long_ints = r.hash_table(12, |r| Ok((r.u32()?, r.i64()?)))?;
        }
        Ok(q)
    }
}

/// Which qualities a view of an object passes on: a table of quality ids to a mode word.
// D2-13118, D2-13115
#[derive(Clone, Debug, PartialEq, Default)]
pub struct QualityFilter {
    /// The size place the table was stored with.
    pub buckets: u8,
    pub qualities: Vec<(u32, i32)>,
}

impl Decode for QualityFilter {
    /// The id, then a hash table of quality id to a 32-bit value.
    // D2-13114, D2-05069, D2-13119, D2-13116
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let (buckets, qualities) = r.hash_table_with_buckets(8, |r| Ok((r.u32()?, r.i32()?)))?;
        Ok(Self { buckets, qualities })
    }
}

impl crate::Encode for QualityFilter {
    /// The id, then the table as it was read.
    // D2-13112, D2-13113
    fn encode(&self, id: u32, w: &mut auberean_dat::Writer) {
        w.u32(id);
        w.hash_table(self.buckets, &self.qualities, |w, (k, v)| {
            w.u32(*k);
            w.i32(*v);
        });
    }
}

// ---------------------------------------------------------------------------------------------
// The word filter and conversations
// ---------------------------------------------------------------------------------------------

/// The word filter (`0x0EBADA55`): per audience, the patterns names and chat may not contain.
// D2-14296, D2-14287
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WordFilter {
    /// The size place the audience table was stored with.
    pub buckets: u8,
    pub audiences: Vec<(u32, Vec<String>)>,
}

impl WordFilter {
    /// An audience's patterns, in stored order.
    #[must_use]
    pub fn patterns(&self, audience: u32) -> Option<&[String]> {
        self.audiences
            .iter()
            .find(|(a, _)| *a == audience)
            .map(|(_, p)| p.as_slice())
    }
}

// D2-05027, D2-14295, D2-14297
impl Decode for WordFilter {
    /// The id, then a hash table of audience to a counted list of wide-string patterns.
    // D2-14282, D2-00379, D2-14292
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let (buckets, audiences) =
            r.hash_table_with_buckets(8, |r| Ok((r.u32()?, r.list(4, Reader::wide_string)?)))?;
        Ok(Self { buckets, audiences })
    }
}

impl crate::Encode for WordFilter {
    // D2-14281, D2-00372, D2-00382
    fn encode(&self, id: u32, w: &mut auberean_dat::Writer) {
        w.u32(id);
        w.hash_table(self.buckets, &self.audiences, |w, (a, p)| {
            w.u32(*a);
            w.list(p, |w, s| w.wide_string(s));
        });
    }
}

/// A conversation tree (`.conversation`, `0x60000000`+): named nodes, each with the actions it
/// takes and the decisions that lead on. No install holds one. Copied whole into another.
// D2-16841, D2-16843, D2-16840, D2-16854
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Conversation {
    pub nodes: Vec<(String, DialogueNode)>,
}

/// One node of a conversation: its actions are made, copied and dropped with it.
// D2-16838, D2-16855, D2-16856, D2-16858, D2-16859, D2-16860, D2-16677, D2-16837
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DialogueNode {
    pub name: String,
    /// What the node does: an action number and its arguments.
    pub actions: Vec<(u32, String)>,
    pub decisions: Vec<DialogueChoice>,
}

/// A way on from a node: a test and its comparison, the test's arguments and the node it leads to.
// D2-16863, D2-16864, D2-16865, D2-16866, D2-16867
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DialogueChoice {
    pub test: u32,
    pub comparison: u32,
    pub arguments: String,
    pub destination: String,
}

impl Decode for Conversation {
    /// No id: a counted list of nodes, each a wide-string key, then the node: its name (a wide
    /// string), a counted list of actions (a word and a wide string) and a counted list of
    /// decisions (two words and two wide strings).
    // D2-16853, D2-16852, D2-16857, D2-16862, D2-16869, D2-16823, D2-16828
    // D2-05078, D2-16845, D2-16842
    fn decode(r: &mut Reader<'_>, _id: u32, _ctx: &Context) -> Result<Self> {
        let nodes = r.list(16, |r| {
            let key = r.wide_string()?;
            let name = r.wide_string()?;
            let actions = r.list(8, |r| Ok((r.u32()?, r.wide_string()?)))?;
            let decisions = r.list(16, |r| {
                Ok(DialogueChoice {
                    test: r.u32()?,
                    comparison: r.u32()?,
                    arguments: r.wide_string()?,
                    destination: r.wide_string()?,
                })
            })?;
            Ok((
                key,
                DialogueNode {
                    name,
                    actions,
                    decisions,
                },
            ))
        })?;
        Ok(Self { nodes })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (decoders of record types no install holds, on synthetic records)
    use super::*;
    use auberean_primitives::scramble;

    fn words(ws: &[u32]) -> Vec<u8> {
        ws.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn wide(s: &str) -> Vec<u8> {
        let units: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let n = u16::try_from(s.encode_utf16().count()).unwrap();
        let mut b = n.to_le_bytes().to_vec();
        b.extend_from_slice(&scramble::conceal(&units));
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
        b
    }

    #[test]
    fn the_word_filter_reads_patterns_per_audience() {
        let mut b = words(&[0x0EBA_DA55, 0x0400_0001, 7, 2]);
        b.extend(wide("bad"));
        b.extend(wide("worse"));
        let f: WordFilter = crate::decode(0x0EBA_DA55, &b, &Context::empty()).unwrap();
        assert_eq!(
            f.audiences,
            vec![(7, vec!["bad".to_string(), "worse".to_string()])]
        );
    }

    #[test]
    fn a_conversation_reads_its_nodes_without_an_id() {
        let mut b = words(&[1]);
        b.extend(wide("start"));
        b.extend(wide("Start"));
        b.extend(words(&[1, 9]));
        b.extend(wide("greet"));
        b.extend(words(&[1, 2, 3]));
        b.extend(wide("x"));
        b.extend(wide("end"));
        let c: Conversation = crate::decode(0x6000_0001, &b, &Context::empty()).unwrap();
        let (key, node) = &c.nodes[0];
        assert_eq!((key.as_str(), node.name.as_str()), ("start", "Start"));
        assert_eq!(node.actions, vec![(9, "greet".to_string())]);
        assert_eq!(node.decisions[0].destination, "end");
    }

    #[test]
    fn a_library_section_of_an_unknown_type_fails_the_load() {
        let b = words(&[0x5600_0005, LIBRARY_MAGIC, LIBRARY_VERSION, 99, 0]);
        let e = crate::decode::<ScriptLibrary>(0x5600_0005, &b, &Context::empty()).unwrap_err();
        assert!(e.what.contains("section type 99"), "{e}");
    }

    #[test]
    fn library_sections_are_aligned_after_their_bytes() {
        let mut b = words(&[0x5600_0005, LIBRARY_MAGIC, LIBRARY_VERSION, 1, 5, 1]);
        b.extend_from_slice(&[0xAB, 0, 0, 0]);
        b.extend(words(&[5, 4, 0]));
        let lib: ScriptLibrary = crate::decode(0x5600_0005, &b, &Context::empty()).unwrap();
        assert_eq!(
            lib.sections,
            vec![Section::Code(vec![0xAB]), Section::Versions(vec![])]
        );
    }
}
