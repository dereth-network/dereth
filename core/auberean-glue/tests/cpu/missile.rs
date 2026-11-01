//! A missile filled from script: its record, holding place, aim and what it passes through.

// Hand-written programs a few dozen words long: every length fits in 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use auberean_glue::missile::{
    PhysicsValue, COLLISION_PARAMETERS, ETHEREAL_PHYSICS_TYPE, MISSILE_INFO,
    MISSILE_TARGETING_PARAMETERS,
};
use auberean_script::context::Context;
use auberean_script::opcode::op;
use auberean_script::value::{tag, FunctionId};
use auberean_script::{Program, Status, Vm};

use crate::asm::{i, library, small, w, Pkg, TestHost};

const THING: u32 = 0x10;
const VECTOR: u32 = 0x70;

/// What a native call pushes: a small integer, a whole word with its tag, or a local.
#[derive(Clone, Copy)]
enum Arg {
    Small(i16),
    Word(u32, u8),
    Local(i16),
}

fn push(b: &mut Vec<u32>, a: Arg) {
    match a {
        Arg::Small(v) => b.push(small(tag::INT, v)),
        Arg::Word(v, t) => b.extend([w(op::PUSH) | u32::from(t) << 8, v]),
        Arg::Local(k) => b.push(i(op::LOAD, k)),
    }
}

/// A native call on local `this`: `args` from `fp[-2]` outwards, a result slot below them when
/// `result`, which is left on the stack.
fn call(b: &mut Vec<u32>, this: i16, args: &[Arg], package: u32, number: u32, result: bool) {
    if result {
        b.push(small(tag::INT, 0));
    }
    for a in args.iter().rev() {
        push(b, *a);
    }
    b.push(i(op::LOAD, this));
    b.extend([w(op::CALL_NATIVE), (package << 16) | number]);
    b.push(i(op::POP_MANY, args.len() as i16 + 1));
}

fn store(b: &mut Vec<u32>, k: i16) {
    b.extend([small(tag::INT, k), w(op::STORE), w(op::POP)]);
}

fn new(b: &mut Vec<u32>, ty: u32, k: i16) {
    b.extend([w(op::NEW_NATIVE), ty]);
    store(b, k);
}

fn finish(b: &mut Vec<u32>, k: i16) {
    b.extend([
        i(op::LOAD, k),
        small(tag::INT, -2),
        w(op::STORE),
        w(op::LEAVE),
    ]);
}

fn program(body: Vec<u32>) -> Arc<Program> {
    let mut code = vec![w(op::END), i(op::ENTER, 0), w(op::LEAVE)];
    let f1 = code.len() as u32;
    code.extend(body);
    let packages = [Pkg {
        name: "Thing",
        id: THING,
        size: 1,
        parent: None,
        functions: vec![1, f1],
        native: false,
    }];
    let lib = library(&code, &packages, &[]);
    let table = auberean_glue::natives(&lib);
    Arc::new(Program::link(lib, table).expect("links"))
}

fn run(prog: &Arc<Program>) -> (Vm, u32) {
    let mut vm = Vm::new(Arc::clone(prog));
    vm.budget = 100_000;
    let mut host = TestHost::default();
    let this = vm.new_object(&mut host, THING, prog).unwrap();
    let mut cx = Context::new(usize::MAX, 0);
    cx.push(0, tag::INT).unwrap();
    cx.push(this, tag::REFERENCE).unwrap();
    let r = vm.call(
        &mut host,
        &mut cx,
        THING,
        FunctionId::new(THING, 1).direct(),
    );
    assert_eq!(r, Ok(Status::Terminated), "{r:?}");
    (vm, cx.data[0])
}

fn value(vm: &Vm, r: u32) -> PhysicsValue {
    PhysicsValue::of(vm.heap.native(r).unwrap()).expect("a physics value")
}

const TARGET: u64 = 0x8000_0001_0000_0002;

fn float(v: f32) -> Arg {
    Arg::Word(v.to_bits(), tag::FLOAT)
}

/// A missile, its aim and its collision settings made and filled, the aim and the settings
/// copied into the missile, then the aim changed again; locals: 0 the missile, 1 the aim, 2 the
/// settings, 3 the aim's offset, 4 the answer of adding an object to pass through.
fn fill_a_missile() -> Vec<u32> {
    let mut b = vec![i(op::ENTER, 5)];
    new(&mut b, MISSILE_INFO, 0);
    new(&mut b, MISSILE_TARGETING_PARAMETERS, 1);
    new(&mut b, COLLISION_PARAMETERS, 2);
    new(&mut b, VECTOR, 3);
    call(
        &mut b,
        3,
        &[float(1.0), float(2.0), float(3.0)],
        VECTOR,
        6,
        false,
    );
    call(
        &mut b,
        0,
        &[Arg::Word(0x7000_0123, tag::INT)],
        MISSILE_INFO,
        1,
        false,
    );
    call(&mut b, 0, &[Arg::Small(2)], MISSILE_INFO, 2, false);
    // Thrown takes the low byte: 0x100 is no.
    call(&mut b, 0, &[Arg::Small(0x100)], MISSILE_INFO, 5, false);
    call(&mut b, 0, &[Arg::Small(1)], MISSILE_INFO, 6, false);
    // The target: its id (high word nearest the receiver), half its height up, the offset
    // scaled (any word but 0), the offset.
    let (lo, hi) = (TARGET as u32, (TARGET >> 32) as u32);
    call(
        &mut b,
        1,
        &[
            Arg::Word(hi, tag::LONG_INT),
            Arg::Word(lo, tag::LONG_INT),
            float(0.5),
            Arg::Small(2),
            Arg::Local(3),
        ],
        MISSILE_TARGETING_PARAMETERS,
        1,
        false,
    );
    // A signalling not-a-number for the starting speed.
    call(
        &mut b,
        1,
        &[Arg::Word(0x7F80_0001, tag::FLOAT)],
        MISSILE_TARGETING_PARAMETERS,
        3,
        false,
    );
    call(
        &mut b,
        2,
        &[Arg::Word(0x8000_0001, tag::INT)],
        COLLISION_PARAMETERS,
        0x14,
        false,
    );
    call(
        &mut b,
        2,
        &[Arg::Word(hi, tag::LONG_INT), Arg::Word(lo, tag::LONG_INT)],
        COLLISION_PARAMETERS,
        1,
        true,
    );
    store(&mut b, 4);
    call(&mut b, 0, &[Arg::Local(1)], MISSILE_INFO, 4, false);
    call(&mut b, 0, &[Arg::Local(2)], MISSILE_INFO, 7, false);
    call(
        &mut b,
        1,
        &[float(45.0)],
        MISSILE_TARGETING_PARAMETERS,
        4,
        false,
    );
    b
}

/// A missile keeps its record and holding place as given, thrown and copy-of-the-weapon from each
/// argument's low byte, and copies of its aim and its collision settings as they were when set:
/// the aim strikes an object at a fraction of its height with a scaled offset, its starting speed
/// as the floating-point unit leaves it; a mask's low word set alone carries its sign into the
/// high word and marks nothing set; passing through an object answers 1 and lists it placed and
/// moving, the lists starting solid.
///
/// Behaviour: script.glue.missile-parts-copied-into-a-missile
#[test]
fn a_missile_takes_copies_of_its_aim_and_what_it_passes_through() {
    let mut b = fill_a_missile();
    finish(&mut b, 0);
    let (vm, r) = run(&program(b));
    let PhysicsValue::Missile(m) = value(&vm, r) else {
        panic!("a missile")
    };
    assert_eq!((m.entity, m.holding_location), (0x7000_0123, 2));
    assert!(!m.thrown);
    assert!(m.clone);
    let a = &m.targeting;
    // An object to strike and a starting speed; the pitch set after the copy is not here.
    assert_eq!(a.bits, 0x1 | 0x4);
    assert_eq!(a.target, TARGET);
    assert_eq!(a.target_height, 0.5);
    assert!(a.target_scaled);
    assert_eq!(
        [a.target_offset.x, a.target_offset.y, a.target_offset.z],
        [1.0, 2.0, 3.0]
    );
    assert_eq!(a.initial_speed.to_bits(), 0x7FC0_0001);
    let c = &m.collision;
    assert_eq!(c.physics_type.mask, 0xFFFF_FFFF_8000_0001);
    // Only the two object lists are marked set.
    assert_eq!(c.bits, 0x8 | 0x2);
    assert_eq!(c.placement_objects.ethereal, [TARGET]);
    assert_eq!(c.movement_objects.ethereal, [TARGET]);
    assert_eq!(c.placement_objects.mask, u64::MAX);

    let mut b = fill_a_missile();
    finish(&mut b, 4);
    let (_, answer) = run(&program(b));
    assert_eq!(answer, 1);
}

/// A mask of physics types changed by a property is first cleared and marked set; a property the
/// install's list does not name as a flag sets nothing and answers 0. Reading a mask answers a
/// new object holding a copy; a mask made ethereal to all and set whole is copied and marked set.
///
/// Behaviour: script.glue.a-mask-changed-by-property-is-cleared-first
#[test]
fn a_mask_changed_by_a_property_is_cleared_first_and_a_whole_mask_is_copied() {
    let body = |last: i16| {
        let mut b = vec![i(op::ENTER, 4)];
        new(&mut b, COLLISION_PARAMETERS, 0);
        call(
            &mut b,
            0,
            &[Arg::Small(5)],
            COLLISION_PARAMETERS,
            0x14,
            false,
        );
        // No property list here: 0x4000006F is no flag.
        call(
            &mut b,
            0,
            &[Arg::Word(0x4000_006F, tag::INT)],
            COLLISION_PARAMETERS,
            7,
            true,
        );
        store(&mut b, 1);
        new(&mut b, ETHEREAL_PHYSICS_TYPE, 2);
        call(&mut b, 2, &[], ETHEREAL_PHYSICS_TYPE, 5, false);
        call(&mut b, 0, &[Arg::Local(2)], COLLISION_PARAMETERS, 6, false);
        // The copy read back, then the original made solid: the copy keeps every bit.
        call(&mut b, 0, &[], COLLISION_PARAMETERS, 9, true);
        store(&mut b, 3);
        call(&mut b, 2, &[], ETHEREAL_PHYSICS_TYPE, 1, false);
        finish(&mut b, last);
        b
    };
    let (vm, r) = run(&program(body(0)));
    let PhysicsValue::Collision(c) = value(&vm, r) else {
        panic!("collision settings")
    };
    assert_eq!(c.physics_type.mask, 0);
    assert_eq!(c.placement_physics.mask, u64::MAX);
    assert_eq!(c.bits, 0x10 | 0x4);
    let (_, answer) = run(&program(body(1)));
    assert_eq!(answer, 0);
    let (vm, r) = run(&program(body(3)));
    let PhysicsValue::Types(t) = value(&vm, r) else {
        panic!("a mask")
    };
    assert_eq!(t.mask, u64::MAX);
}
