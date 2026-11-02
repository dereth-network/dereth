//! The interpreter: a handler per instruction, found by the instruction word's low byte.
//!
//! Integer arithmetic is two's complement and wraps; division truncates toward zero and a zero
//! divisor stops the run (the client faults); shift counts are masked to five bits (the 64-bit
//! shifts take the count's low byte, and a count of 64 or more shifts everything out).
//! Comparisons push an integer 0 or 1. Float comparisons follow IEEE: every ordered comparison is
//! false when either side is NaN, inequality is true, and the float "not", "or" and "and" test
//! against zero (NaN is not zero). Float to integer truncates toward zero; out of range it gives
//! the lowest 64-bit integer, of which the 32-bit forms keep the low word.
//!
//! The handlers never check tags; the results carry the tags the client gives them.

use crate::context::Context;
use crate::link::Program;
use crate::native::{Binding, Call, Host, Status, Stop};
use crate::opcode::{op, signed, tag_of, unsigned, LONG};
use crate::value::{float_to_int, float_to_long, join, split, tag, FunctionId};
use crate::vm::Vm;

/// What a handler works on.
pub(crate) struct Exec<'a> {
    pub(crate) vm: &'a mut Vm,
    pub cx: &'a mut Context,
    pub host: &'a mut dyn Host,
    pub prog: &'a Program,
}

type Handler = fn(u32, &mut Exec<'_>) -> Result<Status, Stop>;

/// The table, by low byte.
// D2-16374
static HANDLERS: [Handler; 256] = build();

const fn build() -> [Handler; 256] {
    let mut t: [Handler; 256] = [no_handler; 256];
    let mut i = 0;
    while i < 256 {
        #[allow(clippy::cast_possible_truncation)]
        let b = i as u8;
        let long = b & 0x80 != 0;
        let o = b & 0x7F;
        let h: Option<Handler> = match o {
            op::NOTHING => Some(nothing),
            op::PUSH => Some(if long { push_long } else { push }),
            op::PUSH_SMALL => Some(if long { push_small_long } else { push_small }),
            op::POP => Some(pop),
            op::POP_MANY => Some(pop_many),
            op::NEW => Some(new),
            op::NEW_NATIVE => Some(new_native),
            op::NEW_STRING => Some(new_string),
            op::ENTER => Some(enter),
            op::LEAVE => Some(leave),
            op::CALL => Some(call),
            op::CALL_NATIVE => Some(call_native),
            op::ADD
            | op::SUB
            | op::MUL
            | op::DIV
            | op::REM
            | op::BIT_OR
            | op::BIT_XOR
            | op::BIT_AND
            | op::SHL
            | op::SHR
            | op::USHR => Some(if long { int_op_long } else { int_op }),
            op::OR
            | op::AND
            | op::EQ
            | op::NE
            | op::LT
            | op::GT
            | op::LE
            | op::GE
            | op::ULT
            | op::UGT
            | op::ULE
            | op::UGE => Some(if long { compare_long } else { compare }),
            op::REF_EQ | op::REF_NE => {
                if long {
                    None
                } else {
                    Some(compare)
                }
            }
            op::NEG
            | op::INC
            | op::DEC
            | op::NOT
            | op::BIT_NOT
            | op::INT_TO_FLOAT
            | op::WIDEN_INT
            | op::WIDEN_UINT
            | op::UINT_TO_FLOAT => Some(if long { int_unary_long } else { int_unary }),
            op::FADD | op::FSUB | op::FMUL | op::FDIV => {
                Some(if long { float_op_long } else { float_op })
            }
            op::FOR | op::FAND | op::FEQ | op::FNE | op::FLT | op::FGT | op::FLE | op::FGE => {
                Some(if long {
                    float_compare_long
                } else {
                    float_compare
                })
            }
            op::FNEG
            | op::FINC
            | op::FDEC
            | op::FLOAT_TO_INT
            | op::WIDEN_FLOAT
            | op::FLOAT_TO_TIME
            | op::FNOT => Some(if long { float_unary_long } else { float_unary }),
            op::LOAD => Some(if long { load_long } else { load }),
            op::LOAD_THIS => Some(load_this),
            op::STORE => Some(if long { store_long } else { store }),
            op::FIELD_LOAD => Some(if long { field_load_long } else { field_load }),
            op::FIELD_STORE => Some(if long { field_store_long } else { field_store }),
            op::SLOT_REF => Some(slot_ref),
            op::FIELD_REF => Some(field_ref),
            op::IF_ZERO => Some(if_zero),
            op::JUMP => Some(jump),
            op::SWITCH => Some(switch),
            op::END => Some(end),
            op::OVERFLOW | op::DEAD => Some(crash),
            op::STORE_POP_MANY => {
                if long {
                    None
                } else {
                    Some(store_pop_many)
                }
            }
            op::THIS_CALL => {
                if long {
                    None
                } else {
                    Some(this_call)
                }
            }
            op::LOAD_CALL => {
                if long {
                    None
                } else {
                    Some(load_call)
                }
            }
            op::FIELD_LOAD_CALL => {
                if long {
                    None
                } else {
                    Some(field_load_call)
                }
            }
            op::THIS_FIELD_LOAD => {
                if long {
                    None
                } else {
                    Some(this_field_load)
                }
            }
            op::THIS_CALL_NATIVE | op::LOAD_CALL_NATIVE | op::FIELD_LOAD_CALL_NATIVE => {
                if long {
                    None
                } else {
                    Some(unlinked_native_call)
                }
            }
            op::CALL_AT => Some(call_at),
            _ => None,
        };
        if let Some(h) = h {
            t[i] = h;
        }
        i += 1;
    }
    t
}

/// Run a context until a handler returns anything but [`Status::Success`]. When the context is
/// back at its base frame the next instruction is the end, whatever the code says.
// D2-16373, D2-16372
pub fn run(
    vm: &mut Vm,
    cx: &mut Context,
    host: &mut dyn Host,
    prog: &Program,
) -> Result<Status, Stop> {
    let mut ex = Exec { vm, cx, host, prog };
    loop {
        let word = if ex.cx.at_base() {
            u32::from(op::END)
        } else {
            let at = ex.cx.ip;
            let w = *prog.code.get(at).ok_or(Stop::BadCode { at, word: 0 })?;
            ex.cx.ip += 1;
            w
        };
        if ex.vm.budget == 0 {
            return Err(Stop::Budget);
        }
        ex.vm.budget -= 1;
        ex.vm.steps += 1;
        let status = match HANDLERS[(word & 0xFF) as usize](word, &mut ex) {
            Ok(s) => s,
            Err(e) => {
                if ex.vm.stopped_at.is_none() {
                    let at = ex.cx.ip.saturating_sub(1);
                    ex.vm.stopped_at = Some(at);
                    *ex.vm.stops.entry((e.stopped(), at)).or_default() += 1;
                }
                return Err(e);
            }
        };
        if status != Status::Success {
            return Ok(status);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

impl Exec<'_> {
    #[inline]
    fn operand(&mut self) -> Result<u32, Stop> {
        let at = self.cx.ip;
        let w = *self
            .prog
            .code
            .get(at)
            .ok_or(Stop::BadCode { at, word: 0 })?;
        self.cx.ip += 1;
        Ok(w)
    }

    /// Move the instruction pointer by a byte displacement.
    #[inline]
    fn branch(&mut self, bytes: i64) -> Result<(), Stop> {
        let target = i64::try_from(self.cx.ip).map_err(|_| Stop::BadCode {
            at: self.cx.ip,
            word: 0,
        })? * 4
            + bytes;
        if target < 0 || target % 4 != 0 {
            return Err(Stop::BadCode {
                at: self.cx.ip,
                word: 0,
            });
        }
        self.cx.ip = usize::try_from(target / 4).map_err(|_| Stop::BadCode {
            at: self.cx.ip,
            word: 0,
        })?;
        Ok(())
    }

    #[inline]
    fn pop2(&mut self) -> Result<(u32, u32), Stop> {
        let b = self.cx.pop()?;
        let a = self.cx.pop()?;
        Ok((a, b))
    }

    #[inline]
    fn pop2_long(&mut self) -> Result<(u64, u64), Stop> {
        let b = self.cx.pop_long()?;
        let a = self.cx.pop_long()?;
        Ok((a, b))
    }
}

fn no_handler(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    Err(Stop::BadCode {
        at: ex.cx.ip.saturating_sub(1),
        word,
    })
}

fn unlinked_native_call(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    // The load pass leaves these operands as native ids, which the client would then call as
    // code addresses; none occurs in the library.
    Err(Stop::BadCode {
        at: ex.cx.ip.saturating_sub(1),
        word,
    })
}

// ---------------------------------------------------------------------------------------------
// The stack
// ---------------------------------------------------------------------------------------------

// D2-16340
fn nothing(_: u32, _: &mut Exec<'_>) -> Result<Status, Stop> {
    Ok(Status::Success)
}

// D2-16348
fn push(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let v = ex.operand()?;
    ex.cx.push(v, tag_of(word))?;
    Ok(Status::Success)
}

/// The long push: two operand words, the high word first.
// D2-16349
fn push_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let hi = ex.operand()?;
    let lo = ex.operand()?;
    #[allow(clippy::cast_possible_truncation)]
    let t = tag_of(word) | LONG as u8;
    ex.cx.push(lo, t)?;
    ex.cx.push(hi, t)?;
    Ok(Status::Success)
}

// D2-16350
fn push_small(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_sign_loss)]
    ex.cx.push(signed(word) as u32, tag_of(word))?;
    Ok(Status::Success)
}

// D2-16351
fn push_small_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let v = i64::from(signed(word));
    #[allow(clippy::cast_possible_truncation)]
    let t = tag_of(word) | LONG as u8;
    #[allow(clippy::cast_sign_loss)]
    ex.cx.push_long(v as u64, t)?;
    Ok(Status::Success)
}

// D2-16341
fn pop(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    ex.cx.drop_slots(1)?;
    Ok(Status::Success)
}

// D2-16342
fn pop_many(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    ex.cx.drop_slots(unsigned(word) as usize)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// Objects
// ---------------------------------------------------------------------------------------------

// D2-16371
fn new(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let package = ex.operand()?;
    let id = ex.vm.new_object(ex.host, package, ex.prog)?;
    ex.cx.push(id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-16363
fn new_native(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let ty = ex.operand()?;
    let id = ex.vm.new_native(ty, ex.prog);
    ex.cx.push(id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-16356, D2-16459
fn new_string(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let n = ex.operand()?;
    let id = ex.vm.literal(n, ex.prog)?;
    ex.cx.push(id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// Frames and calls
// ---------------------------------------------------------------------------------------------

/// A function's entry: the frame starts at the stack top; its locals are reserved and tagged
/// fresh (their data is whatever the stack held).
// D2-16352
fn enter(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let n = unsigned(word) as usize;
    let cx = &mut *ex.cx;
    cx.frames[cx.sf].fp = cx.sp;
    cx.fp = cx.sp;
    if cx.sp + n > crate::context::STACK_SLOTS {
        return Err(Stop::StackOverflow);
    }
    cx.tags[cx.sp..cx.sp + n].fill(tag::FRESH);
    cx.sp += n;
    Ok(Status::Success)
}

// D2-16343
fn leave(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    ex.cx.leave()?;
    Ok(Status::Success)
}

/// A call through the receiver on top of the stack: the function's slot in its declaring
/// package's table, then that slot of the receiver's package. An empty slot calls nothing.
// D2-16364
fn call(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let f = FunctionId(ex.operand()?);
    call_function(ex, f)
}

fn call_function(ex: &mut Exec<'_>, f: FunctionId) -> Result<Status, Stop> {
    let receiver = ex.cx.peek(0)?;
    let package = ex.vm.package_of(ex.host, receiver, ex.prog)?;
    let code = ex
        .prog
        .library
        .dispatch(package, f)
        .map_err(Stop::BadDispatch)?;
    if let Some(offset) = code {
        ex.cx.enter(ex.cx.ip, (offset / 4) as usize)?;
    }
    Ok(Status::Success)
}

/// A direct call to a code offset (no code: nothing).
// D2-16344
fn call_at(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let offset = ex.operand()?;
    if offset != crate::library::NO_CODE {
        ex.cx.enter(ex.cx.ip, (offset / 4) as usize)?;
    }
    Ok(Status::Success)
}

/// A native call: a frame whose pointer is the stack top, the native, then the frame closed
/// unless the native suspended the context.
// D2-16345
fn call_native(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let index = ex.operand()?;
    let native = *ex.prog.native(index).ok_or(Stop::BadCode {
        at: ex.cx.ip - 1,
        word: index,
    })?;
    let f = match native.binding {
        Binding::Ported(f) => f,
        Binding::NotPorted(port) => {
            return Err(Stop::NotPorted {
                native: native.id,
                port,
            })
        }
    };
    {
        let cx = &mut *ex.cx;
        cx.enter(cx.ip, 0)?;
        cx.fp = cx.sp;
        cx.frames[cx.sf].fp = cx.sp;
    }
    let status = {
        let mut call = Call {
            vm: &mut *ex.vm,
            cx: &mut *ex.cx,
            host: &mut *ex.host,
            native: native.id,
        };
        f(&mut call)?
    };
    if status == Status::Interrupted {
        ex.cx.waiting_in = native.id;
        return Ok(Status::Interrupted);
    }
    ex.cx.leave()?;
    Ok(status)
}

// D2-16381
fn this_call(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let this = ex.cx.at(-1)?;
    ex.cx.push(this, tag::REFERENCE)?;
    let f = FunctionId(ex.operand()?);
    if f.is_direct() {
        let code = ex
            .prog
            .library
            .dispatch(f.package(), FunctionId(f.0 & !FunctionId::DIRECT))
            .map_err(Stop::BadDispatch)?;
        if let Some(offset) = code {
            ex.cx.enter(ex.cx.ip, (offset / 4) as usize)?;
        }
        return Ok(Status::Success);
    }
    call_function(ex, f)
}

// D2-16377
fn load_call(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    load(word, ex)?;
    call(word, ex)
}

// D2-16379
fn field_load_call(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    field_load(word, ex)?;
    call(word, ex)
}

// D2-16382
fn this_field_load(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    load_this(word, ex)?;
    field_load(word, ex)
}

// D2-16375
fn store_pop_many(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    store(word, ex)?;
    pop_many(word, ex)
}

/// The end: the context's call is over.
// D2-00025
fn end(_: u32, _: &mut Exec<'_>) -> Result<Status, Stop> {
    Ok(Status::Terminated)
}

// D2-16347
fn crash(_: u32, _: &mut Exec<'_>) -> Result<Status, Stop> {
    Ok(Status::Crash)
}

// ---------------------------------------------------------------------------------------------
// Locals and fields
// ---------------------------------------------------------------------------------------------

// D2-16353
fn load(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let i = ex.cx.frame_index(i64::from(signed(word)))?;
    let (v, t) = (ex.cx.data[i], ex.cx.tags[i]);
    ex.cx.push(v, t)?;
    Ok(Status::Success)
}

/// Both slots take the first slot's tag with the long bit.
// D2-16354
fn load_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let i = ex.cx.frame_index(i64::from(signed(word)))?;
    let j = ex.cx.frame_index(i64::from(signed(word)) + 1)?;
    #[allow(clippy::cast_possible_truncation)]
    let t = ex.cx.tags[i] | LONG as u8;
    let (lo, hi) = (ex.cx.data[i], ex.cx.data[j]);
    ex.cx.push(lo, t)?;
    ex.cx.push(hi, t)?;
    Ok(Status::Success)
}

// D2-16355
fn load_this(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let this = ex.cx.at(-1)?;
    ex.cx.push(this, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// `value slot -> value`: the slot index comes off the stack, the value stays.
// D2-16357
fn store(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let k = ex.cx.pop()?;
    if ex.cx.sp == 0 {
        return Err(Stop::StackUnderflow);
    }
    let (v, t) = (ex.cx.data[ex.cx.sp - 1], ex.cx.tags[ex.cx.sp - 1]);
    #[allow(clippy::cast_possible_wrap)]
    ex.cx.set(i64::from(k as i32), v, t)?;
    Ok(Status::Success)
}

/// The long store writes the first slot's tag only.
// D2-16358
fn store_long(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let k = ex.cx.pop()?;
    if ex.cx.sp < 2 {
        return Err(Stop::StackUnderflow);
    }
    let lo_at = ex.cx.sp - 2;
    let (lo, hi, t) = (ex.cx.data[lo_at], ex.cx.data[lo_at + 1], ex.cx.tags[lo_at]);
    #[allow(clippy::cast_possible_wrap)]
    let k = i64::from(k as i32);
    let i = ex.cx.frame_index(k)?;
    let j = ex.cx.frame_index(k + 1)?;
    ex.cx.tags[i] = t;
    ex.cx.data[i] = lo;
    ex.cx.data[j] = hi;
    Ok(Status::Success)
}

/// Read a slot a reference reaches: `ref -> value`.
// D2-16367
fn field_load(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let r = ex.cx.pop()?;
    let (v, t) = ex
        .vm
        .read_slot(ex.host, ex.cx, r, i64::from(signed(word)), ex.prog)?;
    ex.cx.push(v, t)?;
    Ok(Status::Success)
}

// D2-16368
fn field_load_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let r = ex.cx.pop()?;
    let k = i64::from(signed(word));
    let (lo, t) = ex.vm.read_slot(ex.host, ex.cx, r, k, ex.prog)?;
    let (hi, _) = ex.vm.read_slot(ex.host, ex.cx, r, k + 1, ex.prog)?;
    #[allow(clippy::cast_possible_truncation)]
    let t = t | LONG as u8;
    ex.cx.push(lo, t)?;
    ex.cx.push(hi, t)?;
    Ok(Status::Success)
}

/// `value ref -> value`: the value goes into the slot with its tag and stays on the stack.
// D2-16369
fn field_store(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let r = ex.cx.pop()?;
    if ex.cx.sp == 0 {
        return Err(Stop::StackUnderflow);
    }
    let (v, t) = (ex.cx.data[ex.cx.sp - 1], ex.cx.tags[ex.cx.sp - 1]);
    ex.vm.write_slot(
        ex.host,
        ex.cx,
        r,
        i64::from(signed(word)),
        Some(v),
        t,
        ex.prog,
    )?;
    Ok(Status::Success)
}

/// The long field store writes the first slot's tag only.
// D2-16370
fn field_store_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let r = ex.cx.pop()?;
    if ex.cx.sp < 2 {
        return Err(Stop::StackUnderflow);
    }
    let at = ex.cx.sp - 2;
    let (lo, hi, t) = (ex.cx.data[at], ex.cx.data[at + 1], ex.cx.tags[at]);
    let k = i64::from(signed(word));
    ex.vm
        .write_slot(ex.host, ex.cx, r, k, Some(lo), t, ex.prog)?;
    ex.vm.write_data(ex.host, ex.cx, r, k + 1, hi, ex.prog)?;
    Ok(Status::Success)
}

/// A new reference to a slot of the current frame, by its place in the stack.
// D2-16361
fn slot_ref(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let i = ex.cx.frame_index(i64::from(signed(word)))?;
    #[allow(clippy::cast_possible_truncation)]
    let id = ex
        .vm
        .heap
        .add_relative(crate::value::NULL, (i & 0xFFFF) as u32);
    ex.cx.push(id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A new reference to a field of the object a reference names.
// D2-16362
fn field_ref(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let r = ex.cx.pop()?;
    let id = ex.vm.heap.add_relative(r, unsigned(word));
    ex.cx.push(id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// Control
// ---------------------------------------------------------------------------------------------

// D2-16359
fn if_zero(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    if ex.cx.pop()? == 0 {
        ex.branch(i64::from(signed(word)))?;
    }
    Ok(Status::Success)
}

// D2-16346
fn jump(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    ex.branch(i64::from(signed(word)))?;
    Ok(Status::Success)
}

/// Both the default and the cases jump from the end of the case table.
// D2-16360
fn switch(_: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let v = ex.cx.pop()?;
    let default = ex.operand()?;
    let n = ex.operand()? as usize;
    let table = ex.cx.ip;
    let end = table + 2 * n;
    if end > ex.prog.code.len() {
        return Err(Stop::BadCode { at: table, word: 0 });
    }
    let mut target = default;
    for c in 0..n {
        if ex.prog.code[table + 2 * c] == v {
            target = ex.prog.code[table + 2 * c + 1];
            break;
        }
    }
    ex.cx.ip = end;
    #[allow(clippy::cast_possible_wrap)]
    ex.branch(i64::from(target as i32))?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// Integers
// ---------------------------------------------------------------------------------------------

// D2-16270, D2-16271, D2-16272, D2-16273, D2-16274, D2-16287, D2-16288, D2-16289, D2-16290, D2-16291, D2-16292
fn int_op(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2()?;
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let (sa, sb) = (a as i32, b as i32);
    #[allow(clippy::cast_sign_loss)]
    let r = match (word & 0x7F) as u8 {
        op::ADD => a.wrapping_add(b),
        op::SUB => a.wrapping_sub(b),
        op::MUL => a.wrapping_mul(b),
        op::DIV => {
            if sb == 0 {
                return Err(Stop::DivideByZero);
            }
            sa.wrapping_div(sb) as u32
        }
        op::REM => {
            if sb == 0 {
                return Err(Stop::DivideByZero);
            }
            sa.wrapping_rem(sb) as u32
        }
        op::BIT_OR => a | b,
        op::BIT_XOR => a ^ b,
        op::BIT_AND => a & b,
        op::SHL => a << (b & 31),
        op::SHR => (sa >> (b & 31)) as u32,
        _ => a >> (b & 31),
    };
    ex.cx.push(r, tag::INT)?;
    Ok(Status::Success)
}

// D2-16293, D2-16294, D2-16295, D2-16296, D2-16297, D2-16298, D2-16299, D2-16300, D2-16301, D2-16302, D2-16303
fn int_op_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2_long()?;
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    let (sa, sb) = (a as i64, b as i64);
    // A 64-bit shift takes the count's low byte; 64 or more shifts everything out.
    let count = b & 0xFF;
    #[allow(clippy::cast_sign_loss)]
    let r = match (word & 0x7F) as u8 {
        op::ADD => a.wrapping_add(b),
        op::SUB => a.wrapping_sub(b),
        op::MUL => a.wrapping_mul(b),
        op::DIV => {
            if sb == 0 {
                return Err(Stop::DivideByZero);
            }
            sa.wrapping_div(sb) as u64
        }
        op::REM => {
            if sb == 0 {
                return Err(Stop::DivideByZero);
            }
            sa.wrapping_rem(sb) as u64
        }
        op::BIT_OR => a | b,
        op::BIT_XOR => a ^ b,
        op::BIT_AND => a & b,
        op::SHL => {
            if count >= 64 {
                0
            } else {
                a << count
            }
        }
        op::SHR => {
            if count >= 64 {
                (sa >> 63) as u64
            } else {
                (sa >> count) as u64
            }
        }
        _ => {
            if count >= 64 {
                0
            } else {
                a >> count
            }
        }
    };
    ex.cx.push_long(r, tag::LONG_INT)?;
    Ok(Status::Success)
}

// D2-16275, D2-16276, D2-16277, D2-16278, D2-16279, D2-16280, D2-16281, D2-16282, D2-16283, D2-16284, D2-16285, D2-16286
fn compare(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2()?;
    #[allow(clippy::cast_possible_wrap)]
    let (sa, sb) = (a as i32, b as i32);
    let r = match (word & 0x7F) as u8 {
        op::OR => a != 0 || b != 0,
        op::AND => a != 0 && b != 0,
        op::EQ | op::REF_EQ => a == b,
        op::NE | op::REF_NE => a != b,
        op::LT => sa < sb,
        op::GT => sa > sb,
        op::LE => sa <= sb,
        op::GE => sa >= sb,
        op::ULT => a < b,
        op::UGT => a > b,
        op::ULE => a <= b,
        _ => a >= b,
    };
    ex.cx.push(u32::from(r), tag::INT)?;
    Ok(Status::Success)
}

// D2-16304, D2-16305, D2-16306, D2-16307, D2-16308, D2-16309, D2-16310, D2-16311, D2-16312, D2-16313, D2-16314, D2-16315
fn compare_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2_long()?;
    #[allow(clippy::cast_possible_wrap)]
    let (sa, sb) = (a as i64, b as i64);
    let r = match (word & 0x7F) as u8 {
        op::OR => a != 0 || b != 0,
        op::AND => a != 0 && b != 0,
        op::EQ => a == b,
        op::NE => a != b,
        op::LT => sa < sb,
        op::GT => sa > sb,
        op::LE => sa <= sb,
        op::GE => sa >= sb,
        op::ULT => a < b,
        op::UGT => a > b,
        op::ULE => a <= b,
        _ => a >= b,
    };
    ex.cx.push(u32::from(r), tag::INT)?;
    Ok(Status::Success)
}

/// The 32-bit unary operations.
// D2-16384, D2-16385, D2-16386, D2-16387, D2-16388, D2-16389, D2-16390, D2-16391, D2-16392
fn int_unary(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let a = ex.cx.pop()?;
    #[allow(clippy::cast_possible_wrap)]
    let sa = a as i32;
    match (word & 0x7F) as u8 {
        op::NEG => ex.cx.push(a.wrapping_neg(), tag::INT)?,
        op::INC => ex.cx.push(a.wrapping_add(1), tag::INT)?,
        op::DEC => ex.cx.push(a.wrapping_sub(1), tag::INT)?,
        op::NOT => ex.cx.push(u32::from(a == 0), tag::INT)?,
        op::BIT_NOT => ex.cx.push(!a, tag::INT)?,
        #[allow(clippy::cast_precision_loss)]
        op::INT_TO_FLOAT => ex.cx.push((sa as f32).to_bits(), tag::FLOAT)?,
        #[allow(clippy::cast_sign_loss)]
        op::WIDEN_INT => ex.cx.push_long(i64::from(sa) as u64, tag::LONG_INT)?,
        op::WIDEN_UINT => ex.cx.push_long(u64::from(a), tag::LONG_INT)?,
        #[allow(clippy::cast_precision_loss)]
        _ => ex.cx.push((a as f32).to_bits(), tag::FLOAT)?,
    }
    Ok(Status::Success)
}

/// The 64-bit unary operations. The narrowings keep the low word; "not" pushes a 64-bit 0 or 1.
// D2-16393
fn int_unary_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let a = ex.cx.pop_long()?;
    #[allow(clippy::cast_possible_wrap)]
    let sa = a as i64;
    let (lo, _) = split(a);
    match (word & 0x7F) as u8 {
        op::NEG => ex.cx.push_long(a.wrapping_neg(), tag::LONG_INT)?,
        op::INC => ex.cx.push_long(a.wrapping_add(1), tag::LONG_INT)?,
        op::DEC => ex.cx.push_long(a.wrapping_sub(1), tag::LONG_INT)?,
        op::NOT => ex.cx.push_long(u64::from(a == 0), tag::LONG_INT)?,
        op::BIT_NOT => ex.cx.push_long(!a, tag::LONG_INT)?,
        #[allow(clippy::cast_precision_loss)]
        op::INT_TO_FLOAT => ex.cx.push_long((sa as f64).to_bits(), tag::DOUBLE)?,
        op::WIDEN_INT | op::WIDEN_UINT => ex.cx.push(lo, tag::INT)?,
        #[allow(clippy::cast_precision_loss)]
        _ => ex.cx.push_long((a as f64).to_bits(), tag::DOUBLE)?,
    }
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// Floats
// ---------------------------------------------------------------------------------------------

fn f(v: u32) -> f32 {
    f32::from_bits(v)
}

fn d(v: u64) -> f64 {
    f64::from_bits(v)
}

// D2-16316, D2-16317, D2-16318, D2-16319
fn float_op(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2()?;
    let r = match (word & 0x7F) as u8 {
        op::FADD => f(a) + f(b),
        op::FSUB => f(a) - f(b),
        op::FMUL => f(a) * f(b),
        _ => f(a) / f(b),
    };
    ex.cx.push(r.to_bits(), tag::FLOAT)?;
    Ok(Status::Success)
}

// D2-16328, D2-16329, D2-16330, D2-16331
fn float_op_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2_long()?;
    let r = match (word & 0x7F) as u8 {
        op::FADD => d(a) + d(b),
        op::FSUB => d(a) - d(b),
        op::FMUL => d(a) * d(b),
        _ => d(a) / d(b),
    };
    ex.cx.push_long(r.to_bits(), tag::DOUBLE)?;
    Ok(Status::Success)
}

fn float_test(o: u8, a: f64, b: f64) -> bool {
    match o {
        op::FOR => a != 0.0 || b != 0.0,
        op::FAND => a != 0.0 && b != 0.0,
        op::FEQ => a == b,
        op::FNE => a != b,
        op::FLT => a < b,
        op::FGT => a > b,
        op::FLE => a <= b,
        _ => a >= b,
    }
}

// D2-16320, D2-16321, D2-16322, D2-16323, D2-16324, D2-16325, D2-16326, D2-16327
fn float_compare(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2()?;
    let r = float_test((word & 0x7F) as u8, f64::from(f(a)), f64::from(f(b)));
    ex.cx.push(u32::from(r), tag::INT)?;
    Ok(Status::Success)
}

// D2-16332, D2-16333, D2-16334, D2-16335, D2-16336, D2-16337, D2-16338, D2-16339
fn float_compare_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let (a, b) = ex.pop2_long()?;
    let r = float_test((word & 0x7F) as u8, d(a), d(b));
    ex.cx.push(u32::from(r), tag::INT)?;
    Ok(Status::Success)
}

/// The 32-bit float unary operations: negation multiplies by -1.
// D2-16394, D2-16383
fn float_unary(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let a = f(ex.cx.pop()?);
    match (word & 0x7F) as u8 {
        // The client multiplies by -1 rather than flipping the sign bit (they differ for a NaN).
        #[allow(clippy::neg_multiply)]
        op::FNEG => ex.cx.push((a * -1.0).to_bits(), tag::FLOAT)?,
        op::FINC => ex.cx.push((a + 1.0).to_bits(), tag::FLOAT)?,
        op::FDEC => ex.cx.push((a - 1.0).to_bits(), tag::FLOAT)?,
        op::FLOAT_TO_INT => ex.cx.push(float_to_int(f64::from(a)), tag::INT)?,
        op::WIDEN_FLOAT => ex.cx.push_long(f64::from(a).to_bits(), tag::DOUBLE)?,
        op::FLOAT_TO_TIME => ex.cx.push_long(f64::from(a).to_bits(), tag::TIME)?,
        _ => ex.cx.push(u32::from(a == 0.0), tag::INT)?,
    }
    Ok(Status::Success)
}

/// The 64-bit float unary operations: to an integer is 64-bit, narrowing gives a float.
// D2-16395
fn float_unary_long(word: u32, ex: &mut Exec<'_>) -> Result<Status, Stop> {
    let a = d(ex.cx.pop_long()?);
    match (word & 0x7F) as u8 {
        #[allow(clippy::neg_multiply)]
        op::FNEG => ex.cx.push_long((a * -1.0).to_bits(), tag::DOUBLE)?,
        op::FINC => ex.cx.push_long((a + 1.0).to_bits(), tag::DOUBLE)?,
        op::FDEC => ex.cx.push_long((a - 1.0).to_bits(), tag::DOUBLE)?,
        #[allow(clippy::cast_sign_loss)]
        op::FLOAT_TO_INT => ex.cx.push_long(float_to_long(a) as u64, tag::LONG_INT)?,
        #[allow(clippy::cast_possible_truncation)]
        op::WIDEN_FLOAT => ex.cx.push((a as f32).to_bits(), tag::FLOAT)?,
        op::FLOAT_TO_TIME => ex.cx.push_long(a.to_bits(), tag::TIME)?,
        _ => ex.cx.push(u32::from(a == 0.0), tag::INT)?,
    }
    Ok(Status::Success)
}

/// A 64-bit value from two slots, for natives and tests.
#[must_use]
pub fn long_of(lo: u32, hi: u32) -> u64 {
    join(lo, hi)
}
