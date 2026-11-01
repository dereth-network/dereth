//! Reading the values of the client's calls the server keeps (its options), without the script
//! library: plain values, and object values that are one native object of a known kind, kept as
//! its packed bytes.

use auberean_script::args::pack_tag;
use auberean_script::graph::field;
use auberean_script::wire::In;

/// Native kinds a kept call carries.
pub mod kind {
    /// A hash table of words to words.
    pub const WORD_WORD_HASH: u16 = 1;
    /// The interface's window places.
    pub const WINDOW_PLACES: u16 = 111;
    /// One shortcut.
    pub const SHORTCUT: u16 = 172;
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
    /// An object that is one native of a known kind: its kind and its packed bytes.
    Native(u16, Vec<u8>),
}

fn native_body(r: &mut In<'_>, ty: u16) -> Result<Vec<u8>, String> {
    let e = |e: auberean_script::wire::CodecError| e.to_string();
    let mut from = r.clone();
    let start = r.pos();
    match ty {
        kind::WORD_WORD_HASH => {
            let n = (r.u32().map_err(e)? & 0x00FF_FFFF) as usize;
            let n = r.count(n, 8).map_err(e)?;
            r.bytes(n * 8).map_err(e)?;
        }
        kind::WINDOW_PLACES => {
            let n = (r.u32().map_err(e)? & 0x00FF_FFFF) as usize;
            let n = r.count(n, 8).map_err(e)?;
            for _ in 0..n {
                r.u32().map_err(e)?;
                let m = (r.u32().map_err(e)? & 0x00FF_FFFF) as usize;
                let m = r.count(m, 24).map_err(e)?;
                r.bytes(m * 24).map_err(e)?;
            }
        }
        kind::SHORTCUT => match r.u32().map_err(e)? {
            0 => {}
            0x4000_0001 | 0x4000_0004 | 0x4000_0006 => {
                r.u32().map_err(e)?;
            }
            0x4000_0002 => {
                r.bytes(8).map_err(e)?;
            }
            0x4000_0003 => {
                r.wide().map_err(e)?;
            }
            k => return Err(format!("a shortcut of kind {k:#x}")),
        },
        other => return Err(format!("a native of kind {other}")),
    }
    let n = r.pos() - start;
    Ok(from.bytes(n).map_err(e)?.to_vec())
}

/// A call's values, in packing order (the called function's last parameter first).
///
/// # Errors
/// A tag the buffer may not hold, an object that is not one known native, or bytes left over.
pub fn values(bytes: &[u8]) -> Result<Vec<Value>, String> {
    let e = |e: auberean_script::wire::CodecError| e.to_string();
    let mut r = In::new(bytes);
    let mut out = Vec::new();
    while !r.at_end() {
        out.push(match r.u32().map_err(e)? {
            pack_tag::INT => Value::Int(r.u32().map_err(e)?),
            pack_tag::LONG_INT => Value::Long(r.u64().map_err(e)?),
            pack_tag::FLOAT => Value::Float(r.u32().map_err(e)?),
            pack_tag::LONG_FLOAT => Value::LongFloat(r.u64().map_err(e)?),
            pack_tag::OBJECT => {
                if r.u32().map_err(e)? != 1 {
                    return Err("an object of more than one entry".into());
                }
                r.u32().map_err(e)?;
                let f = r.u32().map_err(e)?;
                if f & field::KIND != field::OBJECT || f & field::SINGLETON != 0 {
                    return Err(format!("an entry of flags {f:#x}"));
                }
                r.u32().map_err(e)?;
                let w = r.u32().map_err(e)?;
                #[allow(clippy::cast_possible_truncation)]
                let ty = w as u16;
                if w >> 16 != 0xFFFF || ty == 0 {
                    return Err(format!("a script object of package {:#x}", w >> 16));
                }
                let body = native_body(&mut r, ty)?;
                if r.u32().map_err(e)? != 0 {
                    return Err("a native with slots".into());
                }
                Value::Native(ty, body)
            }
            t => return Err(format!("a value of tag {t}")),
        });
    }
    Ok(out)
}
