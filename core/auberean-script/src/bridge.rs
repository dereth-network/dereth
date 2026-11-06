//! Script calls across the connection.
//!
//! **In** (the other side calls a function here): the engine object the call is for runs it in a
//! new context of its process: the result slot (an integer 0), the arguments in buffer order, the
//! object's instance as the receiver, then the function, named outright, so it runs in its own
//! package whatever the instance is. The context is discarded when the call ends or crashes, and
//! kept when a native suspends it. A buffer that does not read refuses the call before anything
//! runs.
//!
//! **Out** (script calls a function on the other side): a sending function packs its parameters,
//! last first, into a buffer native and hands the buffer and the other side's function id to a
//! native that sends them; [`send_call`] runs a sending function as script does.

use crate::args::{self, Arg};
use crate::context::Context;
use crate::native::{Host, Status, Stop};
use crate::value::{tag, FunctionId};
use crate::vm::Vm;

/// How an incoming call went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The arguments did not read; nothing ran.
    Refused(String),
    /// The call ran and ended with this status.
    Ran(Status),
}

/// Run an incoming call on the instance of process `process`. A context the call leaves
/// suspended is kept with the process.
// D2-08038, D2-16752
pub fn receive(
    vm: &mut Vm,
    host: &mut dyn Host,
    process: usize,
    function: u32,
    args: &[u8],
) -> Result<Outcome, Stop> {
    let prog = std::sync::Arc::clone(vm.program());
    let values = match args::decode(args, &prog.library) {
        Ok(v) => v,
        Err(e) => return Ok(Outcome::Refused(e.to_string())),
    };
    let mut cx = vm.new_context(process);
    let status = run_call(vm, host, &mut cx, process, function, &values)?;
    if status == Status::Interrupted {
        vm.park(cx);
    }
    Ok(Outcome::Ran(status))
}

fn run_call(
    vm: &mut Vm,
    host: &mut dyn Host,
    cx: &mut Context,
    process: usize,
    function: u32,
    values: &[Arg],
) -> Result<Status, Stop> {
    cx.push(0, tag::INT)?;
    vm.heap.begin_build();
    let pushed = push_args(vm, cx, values);
    vm.heap.end_build();
    pushed?;
    let receiver = vm.instance_of(process);
    cx.push(receiver, tag::REFERENCE)?;
    let package = vm.package_of(host, receiver, &std::sync::Arc::clone(vm.program()))?;
    vm.call(host, cx, package, FunctionId(function))
}

/// Push arguments onto a context's stack as the receiver does: objects read into the table.
// D2-16747, D2-16748, D2-16749, D2-16750, D2-16751
pub fn push_args(vm: &mut Vm, cx: &mut Context, values: &[Arg]) -> Result<(), Stop> {
    for v in values {
        match v {
            Arg::Int(x) => cx.push(*x, tag::INT)?,
            Arg::LongInt(x) => cx.push_long(*x, tag::LONG_INT)?,
            Arg::Float(x) => cx.push(*x, tag::FLOAT)?,
            Arg::LongFloat(x) => cx.push_long(*x, tag::DOUBLE)?,
            Arg::Object(g) => {
                let id = vm.heap.import(g, None)?;
                cx.push(id, tag::REFERENCE)?;
            }
        }
    }
    Ok(())
}

/// The record type number of entities, whose saved states sit at `0x71000000` plus the low 24
/// bits of their id.
pub const ENTITY_TYPE_BASE: u32 = 0x4700_0000;

/// Make the process an engine object's scripts run in, as the engine does when it creates the
/// object: its instance is a copy of the entity's saved state when the object has an entity id
/// (`0x47xxxxxx`) and the state exists, else of its package's default state, else a fresh instance
/// of its package (its initializer run).
// D2-08028, D2-16440, D2-16441, D2-16421
pub fn object_process(
    vm: &mut Vm,
    host: &mut dyn Host,
    package: u32,
    entity: u32,
) -> Result<usize, Stop> {
    let state = if entity & 0xFF00_0000 == ENTITY_TYPE_BASE {
        0x7100_0000 | (entity & 0x00FF_FFFF)
    } else {
        crate::library::STATE_BASE | (package & 0xFFFF)
    };
    let instance = if host.saved_state(state).is_some() {
        vm.instance_from_state(host, state, 0)?
    } else if entity != 0
        && host
            .saved_state(crate::library::STATE_BASE | (package & 0xFFFF))
            .is_some()
    {
        vm.instance_from_state(host, crate::library::STATE_BASE | (package & 0xFFFF), 0)?
    } else {
        let prog = std::sync::Arc::clone(vm.program());
        let id = vm.new_object(host, package, &prog)?;
        let f = vm.heap.field(id)?;
        vm.heap.set_field(id, f | crate::graph::field::PROCESS)?;
        id
    };
    Ok(vm.create_process(instance))
}

/// A value handed to a sending function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    Int(u32),
    LongInt(u64),
    /// A float, as its bits.
    Float(u32),
    /// A long float, as its bits.
    LongFloat(u64),
    /// A reference (an object already in the table).
    Ref(u32),
}

/// Call a sending function, as script does to send a call to the other side: the result slot,
/// the parameters after the interface last first, the interface the call goes through, the
/// sending function's package's singleton as the receiver, then the function itself, named
/// outright. The function packs the parameters into a buffer and hands it to the interface's
/// send native, which gives it to the host.
// D2-00968
pub fn send_call(
    vm: &mut Vm,
    host: &mut dyn Host,
    process: usize,
    sender: FunctionId,
    interface: u32,
    params: &[Param],
) -> Result<Status, Stop> {
    let mut cx = vm.new_context(process);
    cx.push(0, tag::INT)?;
    for p in params.iter().rev() {
        match *p {
            Param::Int(v) => cx.push(v, tag::INT)?,
            Param::LongInt(v) => cx.push_long(v, tag::LONG_INT)?,
            Param::Float(v) => cx.push(v, tag::FLOAT)?,
            Param::LongFloat(v) => cx.push_long(v, tag::DOUBLE)?,
            Param::Ref(v) => cx.push(v, tag::REFERENCE)?,
        }
    }
    cx.push(interface, tag::REFERENCE)?;
    let package = sender.package();
    let this = vm.heap.singleton(crate::library::STATE_BASE | package);
    cx.push(this, tag::REFERENCE)?;
    vm.call(host, &mut cx, package, sender.direct())
}

/// The parameters a call's decoded arguments stand for, in declaration order: the buffer holds
/// them last first, and an object is read into the table.
pub fn params_of(vm: &mut Vm, args: &[Arg]) -> Result<Vec<Param>, Stop> {
    let mut out = Vec::with_capacity(args.len());
    for a in args.iter().rev() {
        out.push(match a {
            Arg::Int(v) => Param::Int(*v),
            Arg::LongInt(v) => Param::LongInt(*v),
            Arg::Float(v) => Param::Float(*v),
            Arg::LongFloat(v) => Param::LongFloat(*v),
            Arg::Object(g) => Param::Ref(vm.heap.import(g, None)?),
        });
    }
    Ok(out)
}
