//! The component model's canonical ABI for the schema's types: how a host operation's parameters
//! arrive as core values and guest memory, and how its `result<T, string>` is written back.
//!
//! Only the schema's types occur: `bool`, `s64`, `f64`, `string` (and `value`, which is a string),
//! `option<T>` of those, and `result<T, string>` for every answer.

use dereth_plugin_api::schema::{Op, Ty};
use dereth_plugin_api::Value;

/// One core WebAssembly value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Flat {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

/// A core value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatTy {
    I32,
    I64,
    F32,
    F64,
}

impl Flat {
    fn i32(self) -> Result<i32, String> {
        match self {
            Self::I32(v) => Ok(v),
            other => Err(format!("expected an i32, got {other:?}")),
        }
    }

    fn u32(self) -> Result<u32, String> {
        self.i32().map(i32::cast_unsigned)
    }
}

/// The guest as an operation sees it while it runs: its memory, and its allocator.
pub trait Guest {
    /// `len` bytes of the guest's memory at `at`.
    ///
    /// # Errors
    /// Out of bounds.
    fn read(&mut self, at: u32, len: u32) -> Result<Vec<u8>, String>;

    /// Write `bytes` into the guest's memory at `at`.
    ///
    /// # Errors
    /// Out of bounds.
    fn write(&mut self, at: u32, bytes: &[u8]) -> Result<(), String>;

    /// `size` bytes allocated by the guest (its `cabi_realloc`), aligned to `align`.
    ///
    /// # Errors
    /// The guest's allocator trapped or failed.
    fn alloc(&mut self, align: u32, size: u32) -> Result<u32, String>;

    /// Take `fuel` from the module's fuel for work the host did on its behalf. A runtime that
    /// cannot reach the fuel from an import takes none; the host's time is still bounded by the
    /// entry's budget.
    ///
    /// # Errors
    /// The fuel ran out (the module traps as over its budget).
    fn spend(&mut self, fuel: i64) -> Result<(), String> {
        let _ = fuel;
        Ok(())
    }
}

/// The core values a type flattens to.
#[must_use]
pub fn flat(ty: Ty) -> Vec<FlatTy> {
    match ty {
        Ty::Bool => vec![FlatTy::I32],
        Ty::Int => vec![FlatTy::I64],
        Ty::Num => vec![FlatTy::F64],
        Ty::Str | Ty::Value => vec![FlatTy::I32, FlatTy::I32],
        Ty::Opt(inner) => {
            let mut v = vec![FlatTy::I32];
            v.extend(flat(*inner));
            v
        }
    }
}

/// The core signature of an operation's import: its parameters flattened, then the pointer the
/// answer is written to; no results.
#[must_use]
pub fn import_signature(op: &Op) -> (Vec<FlatTy>, Vec<FlatTy>) {
    let mut params: Vec<FlatTy> = op.params.iter().flat_map(|p| flat(p.ty)).collect();
    params.push(FlatTy::I32);
    (params, Vec::new())
}

fn align(ty: Ty) -> u32 {
    match ty {
        Ty::Bool => 1,
        Ty::Int | Ty::Num => 8,
        Ty::Str | Ty::Value => 4,
        Ty::Opt(inner) => align(*inner),
    }
}

fn align_to(n: u32, a: u32) -> u32 {
    n.div_ceil(a) * a
}

/// Where a `result<T, string>`'s payload starts: after its one-byte case, at the larger of the
/// two payloads' alignments.
fn result_payload_offset(ok: Option<Ty>) -> u32 {
    let a = ok.map_or(1, align).max(4);
    align_to(1, a)
}

fn opt_payload_offset(inner: Ty) -> u32 {
    align_to(1, align(inner))
}

fn string_at(guest: &mut dyn Guest, ptr: u32, len: u32) -> Result<String, String> {
    let bytes = guest.read(ptr, len)?;
    String::from_utf8(bytes).map_err(|_| "a string that is not UTF-8".to_owned())
}

/// Lift one parameter from `args` (consumed from the front).
fn lift(
    ty: Ty,
    args: &mut std::slice::Iter<'_, Flat>,
    guest: &mut dyn Guest,
) -> Result<Value, String> {
    let mut next = || {
        args.next()
            .copied()
            .ok_or_else(|| "too few arguments".to_owned())
    };
    Ok(match ty {
        Ty::Bool => Value::Bool(next()?.i32()? != 0),
        Ty::Int => match next()? {
            Flat::I64(v) => Value::Int(v),
            other => return Err(format!("expected an i64, got {other:?}")),
        },
        Ty::Num => match next()? {
            Flat::F64(v) => Value::Num(v),
            other => return Err(format!("expected an f64, got {other:?}")),
        },
        Ty::Str => {
            let (ptr, len) = (next()?.u32()?, next()?.u32()?);
            within_copy(len)?;
            Value::Str(string_at(guest, ptr, len)?)
        }
        Ty::Value => {
            let (ptr, len) = (next()?.u32()?, next()?.u32()?);
            within_copy(len)?;
            let text = string_at(guest, ptr, len)?;
            Value::from_json_within(&text, dereth_plugin_api::engine::COPY_NODES)
                .map_err(|e| format!("a value that is not JSON the host takes: {e}"))?
        }
        Ty::Opt(inner) => {
            let case = next()?.i32()?;
            // The payload's core values are there whichever the case.
            let v = lift_or_skip(*inner, args, guest, case != 0)?;
            if case == 0 {
                Value::Nil
            } else {
                v
            }
        }
    })
}

/// A string or value of `len` bytes is within what one copy into the host may hold.
fn within_copy(len: u32) -> Result<(), String> {
    if usize::try_from(len).map_or(true, |l| l > dereth_plugin_api::engine::COPY_BYTES) {
        return Err(format!(
            "a value too large to hand to the host (more than {} MiB)",
            dereth_plugin_api::engine::COPY_BYTES >> 20
        ));
    }
    Ok(())
}

fn lift_or_skip(
    ty: Ty,
    args: &mut std::slice::Iter<'_, Flat>,
    guest: &mut dyn Guest,
    read: bool,
) -> Result<Value, String> {
    if read {
        return lift(ty, args, guest);
    }
    for _ in flat(ty) {
        args.next();
    }
    Ok(Value::Nil)
}

/// An operation's parameters, lifted from the core values of its import call, and the pointer its
/// answer goes to.
///
/// # Errors
/// The core values do not match the operation, or the memory they name does not read.
pub fn lift_params(
    op: &Op,
    args: &[Flat],
    guest: &mut dyn Guest,
) -> Result<(Vec<Value>, u32), String> {
    let mut it = args.iter();
    let mut values = Vec::with_capacity(op.params.len());
    for p in op.params {
        values.push(lift(p.ty, &mut it, guest)?);
    }
    let retptr = it
        .next()
        .ok_or_else(|| "no answer pointer".to_owned())?
        .u32()?;
    Ok((values, retptr))
}

/// Allocate `text` in the guest and answer its pointer and length.
///
/// # Errors
/// The guest's allocator failed.
pub fn put_string(guest: &mut dyn Guest, text: &str) -> Result<(u32, u32), String> {
    let len =
        u32::try_from(text.len()).map_err(|_| "a string too long for the guest".to_owned())?;
    if len == 0 {
        return Ok((1, 0));
    }
    let ptr = guest.alloc(1, len)?;
    guest.write(ptr, text.as_bytes())?;
    Ok((ptr, len))
}

fn store_string(guest: &mut dyn Guest, at: u32, text: &str) -> Result<(), String> {
    let (ptr, len) = put_string(guest, text)?;
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&ptr.to_le_bytes());
    b[4..].copy_from_slice(&len.to_le_bytes());
    guest.write(at, &b)
}

/// Store `v` as a `ty` at `at`. A value of the wrong kind is an error the plugin sees.
fn store(guest: &mut dyn Guest, at: u32, ty: Ty, v: &Value) -> Result<(), String> {
    match ty {
        Ty::Bool => {
            let b = v.as_bool().ok_or("the host answered no boolean")?;
            guest.write(at, &[u8::from(b)])
        }
        Ty::Int => {
            let n = v.as_i64().ok_or("the host answered no integer")?;
            guest.write(at, &n.to_le_bytes())
        }
        Ty::Num => {
            let n = v.as_f64().ok_or("the host answered no number")?;
            guest.write(at, &n.to_le_bytes())
        }
        Ty::Str => {
            let s = v.as_str().ok_or("the host answered no string")?;
            store_string(guest, at, s)
        }
        Ty::Value => store_string(guest, at, &v.to_json()),
        Ty::Opt(inner) => {
            if matches!(v, Value::Nil) {
                guest.write(at, &[0])
            } else {
                guest.write(at, &[1])?;
                store(guest, at + opt_payload_offset(*inner), *inner, v)
            }
        }
    }
}

/// Write an operation's answer, `ok` or the error's text, as its `result<T, string>` at `retptr`.
///
/// # Errors
/// The guest's memory or allocator failed.
pub fn store_result(
    guest: &mut dyn Guest,
    op: &Op,
    retptr: u32,
    answer: Result<Value, String>,
) -> Result<(), String> {
    let payload = retptr + result_payload_offset(op.result);
    let answer = match (answer, op.result) {
        (Ok(v), Some(ty)) => {
            // An answer of the wrong kind is the plugin's error, not a trap.
            match check(ty, &v) {
                Ok(()) => Ok((ty, v)),
                Err(e) => Err(format!("{}: {e}", op.name)),
            }
        }
        (Ok(_), None) => {
            return guest.write(retptr, &[0]);
        }
        (Err(e), _) => Err(e),
    };
    match answer {
        Ok((ty, v)) => {
            guest.write(retptr, &[0])?;
            store(guest, payload, ty, &v)
        }
        Err(e) => {
            guest.write(retptr, &[1])?;
            store_string(guest, retptr + result_payload_offset(op.result), &e)
        }
    }
}

/// Whether `v` stores as a `ty`.
fn check(ty: Ty, v: &Value) -> Result<(), String> {
    let ok = match ty {
        Ty::Bool => v.as_bool().is_some(),
        Ty::Int => v.as_i64().is_some(),
        Ty::Num => v.as_f64().is_some(),
        Ty::Str => v.as_str().is_some(),
        Ty::Value => true,
        Ty::Opt(inner) => {
            return if matches!(v, Value::Nil) {
                Ok(())
            } else {
                check(*inner, v)
            }
        }
    };
    if ok {
        Ok(())
    } else {
        Err(format!("the host's answer {} is not a {ty:?}", v.to_json()))
    }
}

/// Read a `result<string, string>` (or, with `ok_string` false, a `result<_, string>`) the guest
/// answered at `at`: `Ok` with the string (empty for none), or `Err` with the error's text.
///
/// # Errors
/// The memory does not read, or the case is neither.
pub fn read_result(
    guest: &mut dyn Guest,
    at: u32,
    ok_string: bool,
) -> Result<Result<String, String>, String> {
    let case = guest.read(at, 1)?[0];
    let payload = at + 4;
    let string = |guest: &mut dyn Guest| -> Result<String, String> {
        let b = guest.read(payload, 8)?;
        let ptr = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        let len = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        string_at(guest, ptr, len)
    };
    match case {
        0 if ok_string => Ok(Ok(string(guest)?)),
        0 => Ok(Ok(String::new())),
        1 => Ok(Err(string(guest)?)),
        other => Err(format!("a result with case {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{lift_params, store_result, Flat, Guest};
    use dereth_plugin_api::schema::OPS;
    use dereth_plugin_api::Value;

    /// A flat memory with a bump allocator.
    struct Mem(Vec<u8>, u32);

    impl Guest for Mem {
        fn read(&mut self, at: u32, len: u32) -> Result<Vec<u8>, String> {
            self.0
                .get(at as usize..(at + len) as usize)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| "out of bounds".to_owned())
        }
        fn write(&mut self, at: u32, bytes: &[u8]) -> Result<(), String> {
            let end = at as usize + bytes.len();
            self.0
                .get_mut(at as usize..end)
                .ok_or_else(|| "out of bounds".to_owned())?
                .copy_from_slice(bytes);
            Ok(())
        }
        fn alloc(&mut self, align: u32, size: u32) -> Result<u32, String> {
            let at = self.1.div_ceil(align) * align;
            self.1 = at + size;
            Ok(at)
        }
    }

    fn op(name: &str) -> &'static dereth_plugin_api::schema::Op {
        OPS.iter().find(|o| o.name == name).unwrap()
    }

    #[test]
    fn parameters_lift_from_core_values_and_memory() {
        let mut m = Mem(vec![0; 1024], 512);
        m.write(16, b"hello").unwrap();
        m.write(32, b"system").unwrap();
        let args = [
            Flat::I32(16),
            Flat::I32(5),
            Flat::I32(1),
            Flat::I32(32),
            Flat::I32(6),
            Flat::I32(64),
        ];
        let (v, retptr) = lift_params(op("chat.print"), &args, &mut m).unwrap();
        assert_eq!(
            v,
            vec![Value::Str("hello".into()), Value::Str("system".into())]
        );
        assert_eq!(retptr, 64);
        // An absent option still carries its payload's core values.
        let args = [
            Flat::I32(16),
            Flat::I32(5),
            Flat::I32(0),
            Flat::I32(0),
            Flat::I32(0),
            Flat::I32(64),
        ];
        let (v, _) = lift_params(op("chat.print"), &args, &mut m).unwrap();
        assert_eq!(v, vec![Value::Str("hello".into()), Value::Nil]);
    }

    #[test]
    fn answers_are_written_as_their_result_and_a_wrong_kind_is_an_error() {
        let mut m = Mem(vec![0; 1024], 512);
        store_result(&mut m, op("task.timer"), 64, Ok(Value::Int(7))).unwrap();
        assert_eq!(m.read(64, 1).unwrap(), [0]);
        assert_eq!(m.read(72, 8).unwrap(), 7i64.to_le_bytes());
        store_result(
            &mut m,
            op("files.package"),
            128,
            Ok(Value::Str("abc".into())),
        )
        .unwrap();
        // result<option<string>, string>: case, then at 4 the option's case, then at 8 the string.
        assert_eq!(m.read(128, 1).unwrap(), [0]);
        assert_eq!(m.read(132, 1).unwrap(), [1]);
        let b = m.read(136, 8).unwrap();
        let ptr = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        assert_eq!(m.read(ptr, 3).unwrap(), b"abc");
        store_result(&mut m, op("task.timer"), 200, Ok(Value::Str("x".into()))).unwrap();
        assert_eq!(m.read(200, 1).unwrap(), [1]);
        store_result(&mut m, op("chat.print"), 300, Err("denied".into())).unwrap();
        let r = super::read_result(&mut m, 300, false).unwrap();
        assert_eq!(r, Err("denied".to_owned()));
    }
}
