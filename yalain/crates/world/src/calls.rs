//! Reading the values of the calls a client makes on the server, without the script library: plain
//! values, and object graphs whose entries are script objects (their slots and tags, as packed) or
//! natives of the kinds the server reads (strings, string references, places, vectors, examination
//! requests and the other records whose extent the packed form gives).
//!
//! The values come in packing order: the called function's last parameter first.

use auberean_script::args::pack_tag;
use auberean_script::graph::field;
use auberean_script::native_object::{native_type, step_over, TextInfo};
use auberean_script::wire::{CodecError, In};

/// One entry of an object graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A script object: its package and its slots with their tags.
    Script { package: u16, slots: Vec<(u8, u32)> },
    /// A string.
    Text(String),
    /// A string reference.
    TextInfo(TextInfo),
    /// A native object of another kind, as its packed bytes.
    Native { ty: u16, bytes: Vec<u8> },
    /// A singleton: the id of its saved state.
    Singleton(u32),
    /// Something the graph does not carry (an object packed elsewhere, a reference into a slot).
    Other,
}

/// One value of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(u32),
    Long(u64),
    /// A float, as its bits.
    Float(u32),
    /// A long float, as its bits.
    LongFloat(u64),
    /// An object graph: its entries by id, the root first.
    Graph(Vec<(u32, Node)>),
}

impl Value {
    /// The root of an object value.
    #[must_use]
    pub fn root(&self) -> Option<&Node> {
        match self {
            Self::Graph(g) => g.first().map(|(_, n)| n),
            _ => None,
        }
    }

    /// The entry of an object value with this id.
    #[must_use]
    pub fn entry(&self, id: u32) -> Option<&Node> {
        match self {
            Self::Graph(g) => g.iter().find(|(i, _)| *i == id).map(|(_, n)| n),
            _ => None,
        }
    }
}

/// Native types of the lists a client's calls carry: of 64-bit values, of references.
pub const LONG_LIST: u32 = 0x3B;
pub const REF_LIST: u32 = 0x65;

/// Native types of the tables and lists of words calls carry, and the bytes of each entry: words
/// to words (one key to one value, or several), words to 64-bit values, words to references,
/// 64-bit values to references, 64-bit values to words; a list of words; a set of 64-bit values;
/// names (a reference, its hash and whether it is used) to references.
pub const TABLES: [(u32, usize); 9] = [
    (0x1, 8),
    (0x3, 8),
    (0x8, 12),
    (0x11, 8),
    (0x40, 12),
    (0x32, 12),
    (0xA, 4),
    (0x1AC8, 8),
    (0x4D, 16),
];

fn e(e: CodecError) -> String {
    e.to_string()
}

fn node(r: &mut In<'_>) -> Result<Node, String> {
    r.u32().map_err(e)?;
    let w = r.u32().map_err(e)?;
    #[allow(clippy::cast_possible_truncation)]
    let (package, ty) = ((w >> 16) as u16, w as u16);
    let native = match u32::from(ty) {
        0 => None,
        native_type::WP_STRING | native_type::CASE_INSENSITIVE_WP_STRING => {
            Some(Node::Text(String::from_utf16_lossy(&r.wide().map_err(e)?)))
        }
        native_type::STRING_INFO => Some(Node::TextInfo(TextInfo::unpack(r).map_err(e)?)),
        // A place: its cell and its frame (seven words).
        native_type::POSITION => Some(Node::Native {
            ty,
            bytes: r.bytes(32).map_err(e)?.to_vec(),
        }),
        // Three floats.
        native_type::VECTOR => Some(Node::Native {
            ty,
            bytes: r.bytes(12).map_err(e)?.to_vec(),
        }),
        // Lists of 64-bit values and of references: a count, then the items.
        t @ (LONG_LIST | REF_LIST) => {
            let n = r.u32().map_err(e)? as usize;
            let size = if t == LONG_LIST { 8 } else { 4 };
            let n = r.count(n, size).map_err(e)?;
            let mut bytes = u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes().to_vec();
            bytes.extend_from_slice(r.bytes(n * size).map_err(e)?);
            Some(Node::Native { ty, bytes })
        }
        // A table or a list of words: its count (with the table's size place on top), then its
        // entries.
        t if TABLES.iter().any(|(x, _)| *x == t) => {
            let each = TABLES.iter().find(|(x, _)| *x == t).map_or(8, |(_, e)| *e);
            let header = r.u32().map_err(e)?;
            let n = r.count((header & 0x00FF_FFFF) as usize, each).map_err(e)?;
            let mut bytes = header.to_le_bytes().to_vec();
            bytes.extend_from_slice(r.bytes(n * each).map_err(e)?);
            Some(Node::Native { ty, bytes })
        }
        t => {
            let start = r.pos();
            let mut probe = r.clone();
            step_over(&mut probe, t).map_err(e)?;
            let n = probe.pos() - start;
            Some(Node::Native {
                ty,
                bytes: r.bytes(n).map_err(e)?.to_vec(),
            })
        }
    };
    let n = r.u32().map_err(e)? as usize;
    let n = r.count(n, 5).map_err(e)?;
    let mut slots = Vec::with_capacity(n);
    for _ in 0..n {
        slots.push(r.u32().map_err(e)?);
    }
    let tags = r.bytes(n).map_err(e)?.to_vec();
    r.align().map_err(e)?;
    Ok(native.unwrap_or_else(|| Node::Script {
        package,
        slots: tags.into_iter().zip(slots).collect(),
    }))
}

fn graph(r: &mut In<'_>) -> Result<Vec<(u32, Node)>, String> {
    let n = r.u32().map_err(e)? as usize;
    let n = r.count(n, 8).map_err(e)?;
    let mut ids = Vec::with_capacity(n);
    for _ in 0..n {
        ids.push(r.u32().map_err(e)?);
    }
    let mut out = Vec::with_capacity(n);
    for id in ids {
        let f = r.u32().map_err(e)?;
        let n = match f & field::KIND {
            field::OBJECT if f & field::SINGLETON != 0 => Node::Singleton(r.u32().map_err(e)?),
            field::OBJECT if f & field::PROCESS == 0 || f & field::ROOT != 0 => node(r)?,
            field::OBJECT => Node::Other,
            field::RELATIVE => {
                r.u32().map_err(e)?;
                Node::Other
            }
            k => return Err(format!("a reference of kind {}", k >> 8)),
        };
        out.push((id, n));
    }
    Ok(out)
}

/// A call's values, in packing order.
///
/// # Errors
/// A tag the buffer may not hold, a native of a kind this reader does not know, or bytes left
/// over.
pub fn values(bytes: &[u8]) -> Result<Vec<Value>, String> {
    let mut r = In::new(bytes);
    let mut out = Vec::new();
    while !r.at_end() {
        out.push(match r.u32().map_err(e)? {
            pack_tag::INT => Value::Int(r.u32().map_err(e)?),
            pack_tag::LONG_INT => Value::Long(r.u64().map_err(e)?),
            pack_tag::FLOAT => Value::Float(r.u32().map_err(e)?),
            pack_tag::LONG_FLOAT => Value::LongFloat(r.u64().map_err(e)?),
            pack_tag::OBJECT => Value::Graph(graph(&mut r)?),
            t => return Err(format!("a value of tag {t}")),
        });
    }
    Ok(out)
}

/// A script object's slot values, its 64-bit slots (a long or a double over two slots) joined:
/// each as its tag and value.
#[must_use]
pub fn joined(slots: &[(u8, u32)]) -> Vec<(u8, u64)> {
    let mut out = Vec::with_capacity(slots.len());
    let mut it = slots.iter().peekable();
    while let Some(&(tag, lo)) = it.next() {
        if it
            .peek()
            .is_some_and(|(t, _)| *t == crate::graph::tag::HIGH)
        {
            let (_, hi) = it.next().copied().unwrap_or_default();
            out.push((tag, u64::from(hi) << 32 | u64::from(lo)));
        } else {
            out.push((tag, u64::from(lo)));
        }
    }
    out
}
