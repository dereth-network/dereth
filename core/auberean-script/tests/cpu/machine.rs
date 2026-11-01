//! The interpreter on hand-written programs.

// Hand-written programs and tables a few hundred entries long: every length and offset fits in
// 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use auberean_script::context::Context;
use auberean_script::native::{Binding, Call, NativeTable, Status, Stop};
use auberean_script::opcode::op;
use auberean_script::value::{tag, FunctionId, NativeId};
use auberean_script::{Program, Vm};

use crate::asm::{i, library, program, small, w, Pkg, TestHost};

/// The package every test program's functions live in, and a native package.
const THING: u32 = 0x10;
const TOOLS: u32 = 0x20;

/// A program whose package `THING` has function 0 (an empty initializer) and the given bodies
/// as functions 1, 2, ...; offset 0 holds an end word.
fn thing(bodies: &[Vec<u32>], natives: NativeTable, tools: usize) -> Arc<Program> {
    let mut code = vec![w(op::END), i(op::ENTER, 0), w(op::LEAVE)];
    let mut functions = vec![1];
    for b in bodies {
        functions.push(code.len() as u32);
        code.extend(b);
    }
    let packages = [
        Pkg {
            name: "Thing",
            id: THING,
            size: 2,
            parent: None,
            functions,
            native: false,
        },
        Pkg {
            name: "Tools",
            id: TOOLS,
            size: 0,
            parent: None,
            functions: vec![0; tools],
            native: true,
        },
    ];
    program(library(&code, &packages, &["hi"]), natives)
}

/// Call function `f` of a fresh `THING` with integer arguments (first argument first) and return
/// the status and the result slot.
fn call(prog: &Arc<Program>, f: u32, args: &[u32]) -> (Result<Status, Stop>, u32, u8) {
    let mut vm = Vm::new(Arc::clone(prog));
    vm.budget = 100_000;
    let mut host = TestHost::default();
    let this = vm.new_object(&mut host, THING, prog).unwrap();
    let mut cx = Context::new(usize::MAX, 0);
    cx.push(0, tag::INT).unwrap();
    for a in args.iter().rev() {
        cx.push(*a, tag::INT).unwrap();
    }
    cx.push(this, tag::REFERENCE).unwrap();
    let r = vm.call(
        &mut host,
        &mut cx,
        THING,
        FunctionId::new(THING, f).direct(),
    );
    (r, cx.data[0], cx.tags[0])
}

/// A body computing `a OP b` of its two parameters into its result slot.
fn binary(o: u32) -> Vec<u32> {
    vec![
        i(op::ENTER, 0),
        i(op::LOAD, -2),
        i(op::LOAD, -3),
        o,
        small(tag::INT, -4),
        w(op::STORE),
        w(op::LEAVE),
    ]
}

fn int(prog: &Arc<Program>, f: u32, a: i32, b: i32) -> Result<i32, Stop> {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let (r, v, _) = call(prog, f, &[a as u32, b as u32]);
    assert_eq!(r?, Status::Terminated);
    #[allow(clippy::cast_possible_wrap)]
    Ok(v as i32)
}

/// Behaviour: script.interp.integer-arithmetic
#[test]
fn integer_arithmetic_wraps_truncates_and_masks_shift_counts() {
    let ops = [
        op::SUB,
        op::DIV,
        op::REM,
        op::SHL,
        op::SHR,
        op::USHR,
        op::MUL,
        op::LT,
        op::ULT,
    ];
    let bodies: Vec<Vec<u32>> = ops.iter().map(|o| binary(w(*o))).collect();
    let p = thing(&bodies, NativeTable::new(), 0);
    assert_eq!(int(&p, 1, i32::MIN, 1).unwrap(), i32::MAX);
    assert_eq!(int(&p, 2, -7, 2).unwrap(), -3);
    assert_eq!(int(&p, 3, -7, 2).unwrap(), -1);
    assert_eq!(int(&p, 4, 1, 33).unwrap(), 2);
    assert_eq!(int(&p, 5, -16, 2).unwrap(), -4);
    assert_eq!(int(&p, 6, -16, 28).unwrap(), 15);
    assert_eq!(int(&p, 7, 0x10000, 0x10000).unwrap(), 0);
    assert_eq!(int(&p, 8, -1, 0).unwrap(), 1);
    assert_eq!(int(&p, 9, -1, 0).unwrap(), 0);
    assert_eq!(int(&p, 2, 1, 0), Err(Stop::DivideByZero));
}

/// Behaviour: script.interp.float-comparisons
#[test]
fn float_comparisons_are_false_on_nan_but_inequality_or_and_and_count_it_as_not_zero() {
    let ops = [
        op::FLT,
        op::FGE,
        op::FEQ,
        op::FNE,
        op::FOR,
        op::FAND,
        op::FGT,
    ];
    let bodies: Vec<Vec<u32>> = ops.iter().map(|o| binary(w(*o))).collect();
    let p = thing(&bodies, NativeTable::new(), 0);
    let f = |x: f32| x.to_bits();
    let nan = f(f32::NAN);
    let run = |k: u32, a: u32, b: u32| {
        let (r, v, t) = call(&p, k, &[a, b]);
        assert_eq!(r.unwrap(), Status::Terminated);
        assert_eq!(t, tag::INT);
        v
    };
    assert_eq!(run(1, f(1.0), f(2.0)), 1);
    assert_eq!(run(1, nan, f(2.0)), 0);
    assert_eq!(run(2, nan, nan), 0);
    assert_eq!(run(3, nan, nan), 0);
    assert_eq!(run(4, nan, nan), 1);
    assert_eq!(run(4, f(0.0), f(-0.0)), 0);
    assert_eq!(run(5, nan, f(0.0)), 1);
    assert_eq!(run(6, nan, f(1.0)), 1);
    assert_eq!(run(6, f(0.0), nan), 0);
    assert_eq!(run(7, f(3.0), f(2.0)), 1);
}

/// Behaviour: script.interp.long-values
#[test]
fn long_values_take_two_slots_and_shifts_of_64_or_more_clear_them() {
    // f1(): push a 64-bit constant (high word first in the code), add a sign-extended small
    // value, shift left by 64, and return the low word of each step through a narrowing.
    let body = vec![
        i(op::ENTER, 0),
        w(op::PUSH) | 0x80 | u32::from(tag::INT) << 8,
        1,
        0xFFFF_FFFF,
        small(tag::INT, 1) | 0x80,
        w(op::ADD) | 0x80,
        // (2^32 + 2^32 - 1) + 1 = 2^33: low word 0, high word 2.
        w(op::WIDEN_INT) | 0x80,
        small(tag::INT, -4),
        w(op::STORE),
        w(op::LEAVE),
    ];
    let shifted = vec![
        i(op::ENTER, 0),
        small(tag::INT, 7) | 0x80,
        small(tag::INT, 64) | 0x80,
        w(op::SHL) | 0x80,
        small(tag::INT, 7) | 0x80,
        w(op::ADD) | 0x80,
        w(op::WIDEN_INT) | 0x80,
        small(tag::INT, -4),
        w(op::STORE),
        w(op::LEAVE),
    ];
    let p = thing(&[body, shifted], NativeTable::new(), 0);
    let (r, v, t) = call(&p, 1, &[0, 0]);
    assert_eq!((r.unwrap(), v, t), (Status::Terminated, 0, tag::INT));
    let (r, v, _) = call(&p, 2, &[0, 0]);
    assert_eq!((r.unwrap(), v), (Status::Terminated, 7));
}

/// Behaviour: script.interp.virtual-dispatch
#[test]
fn a_call_runs_the_receivers_slot_and_an_empty_slot_runs_nothing() {
    // Base: f1 returns 1. Derived: its slot 1 returns 2. A call of the base's function 1 on a
    // derived instance runs the derived body; slot 2 of the derived table is empty.
    let ret = |v: i16| {
        vec![
            i(op::ENTER, 0),
            small(tag::INT, v),
            small(tag::INT, -2),
            w(op::STORE),
            w(op::LEAVE),
        ]
    };
    let mut code = vec![w(op::END), i(op::ENTER, 0), w(op::LEAVE)];
    let base_f1 = code.len() as u32;
    code.extend(ret(1));
    let derived_f1 = code.len() as u32;
    code.extend(ret(2));
    let caller = code.len() as u32;
    // caller(): new Derived; push result slot under it; call base f1 through it; return it.
    code.extend([
        i(op::ENTER, 0),
        small(tag::INT, 0),
        w(op::NEW),
        0x12,
        w(op::CALL),
        (0x11 << 16) | 1,
        w(op::CALL),
        (0x11 << 16) | 2,
        w(op::POP),
        small(tag::INT, -2),
        w(op::STORE),
        w(op::LEAVE),
    ]);
    let packages = [
        Pkg {
            name: "Base",
            id: 0x11,
            size: 0,
            parent: None,
            functions: vec![1, base_f1, 1],
            native: false,
        },
        Pkg {
            name: "Derived",
            id: 0x12,
            size: 0,
            parent: Some(0),
            functions: vec![1, derived_f1, u32::MAX],
            native: false,
        },
        Pkg {
            name: "Thing",
            id: THING,
            size: 0,
            parent: None,
            functions: vec![1, caller],
            native: false,
        },
    ];
    let p = program(library(&code, &packages, &[]), NativeTable::new());
    let (r, v, _) = call(&p, 1, &[]);
    assert_eq!((r.unwrap(), v), (Status::Terminated, 2));
}

/// Behaviour: script.interp.switch
#[test]
fn a_switch_jumps_from_the_end_of_its_case_table() {
    // Cases 3 -> return 30, 9 -> return 90, default -> return 1.
    let body = vec![
        i(op::ENTER, 0),
        i(op::LOAD, -2),
        w(op::SWITCH),
        24,
        2,
        3,
        0,
        9,
        8,
        small(tag::INT, 30),
        i(op::JUMP, 20),
        small(tag::INT, 90),
        i(op::JUMP, 12),
        small(tag::INT, 7),
        i(op::JUMP, 4),
        small(tag::INT, 1),
        small(tag::INT, -3),
        w(op::STORE),
        w(op::LEAVE),
    ];
    let p = thing(&[body], NativeTable::new(), 0);
    for (arg, want) in [(3, 30), (9, 90), (5, 1)] {
        let (r, v, _) = call(&p, 1, &[arg]);
        assert_eq!((r.unwrap(), v), (Status::Terminated, want), "case {arg}");
    }
}

fn add_seven_through_reference(c: &mut Call<'_>) -> Result<Status, Stop> {
    // add(this, ref out): writes *out += 7 through the reference, returns nothing.
    let r = c.arg(0)?;
    let prog = Arc::clone(c.vm.program());
    let (v, _) = c.vm.read_slot(c.host, c.cx, r, 0, &prog)?;
    c.vm.write_slot(c.host, c.cx, r, 0, Some(v + 7), tag::INT, &prog)?;
    Ok(Status::Success)
}

/// Behaviour: script.interp.references
#[test]
fn fields_load_and_store_through_references_and_slot_references_work_as_out_parameters() {
    // f1(): local 0 = 5; tools.add(&local0); this.field1 = local0; return this.field1.
    let body = vec![
        i(op::ENTER, 1),
        small(tag::INT, 5),
        small(tag::INT, 0),
        w(op::STORE),
        w(op::POP),
        i(op::SLOT_REF, 0),
        w(op::LOAD_THIS),
        w(op::CALL_NATIVE),
        (TOOLS << 16) | 1,
        i(op::POP_MANY, 2),
        i(op::LOAD, 0),
        w(op::LOAD_THIS),
        i(op::FIELD_STORE, 1),
        w(op::POP),
        w(op::LOAD_THIS),
        i(op::FIELD_LOAD, 1),
        small(tag::INT, -2),
        w(op::STORE),
        w(op::LEAVE),
    ];
    let mut t = NativeTable::new();
    t.bind(
        NativeId::new(TOOLS, 1),
        Binding::Ported(add_seven_through_reference),
    );
    let p = thing(&[body], t, 2);
    let (r, v, tg) = call(&p, 1, &[]);
    assert_eq!((r.unwrap(), v, tg), (Status::Terminated, 12, tag::INT));
}

fn suspend(_: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(Status::Interrupted)
}

/// Behaviour: script.interp.native-suspends-context
#[test]
fn a_native_can_leave_the_context_suspended_to_be_resumed_after_the_call() {
    let body = vec![
        i(op::ENTER, 0),
        small(tag::INT, 0),
        w(op::LOAD_THIS),
        w(op::CALL_NATIVE),
        (TOOLS << 16) | 1,
        i(op::POP_MANY, 1),
        small(tag::INT, -2),
        w(op::STORE),
        w(op::LEAVE),
    ];
    let mut t = NativeTable::new();
    t.bind(NativeId::new(TOOLS, 1), Binding::Ported(suspend));
    let p = thing(&[body], t, 2);
    let mut vm = Vm::new(Arc::clone(&p));
    let mut host = TestHost::default();
    let this = vm.new_object(&mut host, THING, &p).unwrap();
    let mut cx = Context::new(usize::MAX, 0);
    cx.push(0, tag::INT).unwrap();
    cx.push(this, tag::REFERENCE).unwrap();
    let r = vm
        .call(
            &mut host,
            &mut cx,
            THING,
            FunctionId::new(THING, 1).direct(),
        )
        .unwrap();
    assert_eq!(r, Status::Interrupted);
    assert_eq!(cx.waiting_in, NativeId::new(TOOLS, 1).0);
    // The engine writes the native's result, then resumes the context after the call.
    cx.set(-2, 41, tag::MIXED).unwrap();
    assert_eq!(vm.resume(&mut host, &mut cx).unwrap(), Status::Terminated);
    assert_eq!(cx.data[0], 41);
}

/// Behaviour: script.interp.not-ported-native-stops
#[test]
fn a_native_not_written_yet_stops_the_run_naming_it() {
    let body = vec![
        i(op::ENTER, 0),
        w(op::LOAD_THIS),
        w(op::CALL_NATIVE),
        (TOOLS << 16) | 1,
        w(op::LEAVE),
    ];
    let mut t = NativeTable::new();
    t.bind(NativeId::new(TOOLS, 1), Binding::NotPorted("a port id"));
    let p = thing(&[body], t, 2);
    let (r, _, _) = call(&p, 1, &[]);
    assert_eq!(
        r,
        Err(Stop::NotPorted {
            native: NativeId::new(TOOLS, 1).0,
            port: "a port id"
        })
    );
}

/// Behaviour: script.interp.step-budget
#[test]
fn a_run_that_uses_its_step_budget_stops_instead_of_hanging() {
    let body = vec![i(op::ENTER, 0), i(op::JUMP, -4)];
    let p = thing(&[body], NativeTable::new(), 0);
    let (r, _, _) = call(&p, 1, &[]);
    assert_eq!(r, Err(Stop::Budget));
}

/// Behaviour: script.interp.bounds-checked
#[test]
fn runaway_recursion_stops_at_the_call_stack_limit() {
    let body = vec![
        i(op::ENTER, 0),
        w(op::LOAD_THIS),
        w(op::CALL),
        (THING << 16) | 1,
        w(op::LEAVE),
    ];
    let p = thing(&[body], NativeTable::new(), 0);
    let (r, _, _) = call(&p, 1, &[]);
    assert_eq!(r, Err(Stop::CallStackOverflow));
}

/// Behaviour: script.interp.string-literals
#[test]
fn a_string_literal_is_one_shared_object_however_often_it_is_named() {
    let body = vec![
        i(op::ENTER, 0),
        w(op::NEW_STRING),
        0,
        w(op::NEW_STRING),
        0,
        w(op::REF_EQ),
        small(tag::INT, -2),
        w(op::STORE),
        w(op::LEAVE),
    ];
    let p = thing(&[body], NativeTable::new(), 0);
    let (r, v, _) = call(&p, 1, &[]);
    assert_eq!((r.unwrap(), v), (Status::Terminated, 1));
}
