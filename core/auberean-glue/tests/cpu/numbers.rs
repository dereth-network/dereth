//! Dice: the engine's random number generator and the rolls drawn from it; the math natives; a
//! random selection table filled and chosen from.
//! Fixture: the natives called as the machine calls one (the result slot, the arguments last
//! first, the receiver), on a host drawing from a generator in the state every run starts in; a
//! new selection table, as a script's `new` makes one, and one read from its packed form.

use auberean_glue::selection::{Entry, SelectionTable, TABLE};
use auberean_script::native_object::{EngineValue, NativeObject};
use std::sync::Arc;

use auberean_glue::numbers::{roll_float, roll_int, Random};
use auberean_script::context::Context;
use auberean_script::native::{Binding, Call};
use auberean_script::opcode::op;
use auberean_script::value::{tag, NativeId};
use auberean_script::{Program, Status, Vm};

use crate::asm::{library, w, TestHost};

/// **The generator draws the engine's own sequence from the state every process starts in**,
/// and rolls between two bounds take either order, both ends included for integers.
///
/// Behaviour: script.glue.dice-follow-the-engine-generator
#[test]
fn the_generator_draws_the_engines_sequence_and_rolls_span_their_bounds() {
    // Draw number and the bits of the double drawn, from the engine's own generator run from
    // its first state.
    let want: [(usize, u64); 10] = [
        (1, 0x3fe4_f92b_b4ad_7706),
        (2, 0x3fc9_ba35_d215_a7bd),
        (3, 0x3fec_988d_fa3d_4e48),
        (4, 0x3fd2_0a6d_dafa_ecf3),
        (5, 0x3fe0_cccd_9a68_0088),
        (6, 0x3fd4_1aa7_3059_b306),
        (33, 0x3fec_3e19_91c1_3cfb),
        (100, 0x3fb8_a64a_2a5e_6d40),
        (1000, 0x3fa2_b077_5869_2f41),
        (2000, 0x3fca_e594_13dc_7455),
    ];
    let g = Random::new();
    let mut n = 0;
    for (at, bits) in want {
        let mut x = 0.0;
        while n < at {
            x = g.draw();
            n += 1;
        }
        assert_eq!(x.to_bits(), bits, "draw {at}");
    }

    assert_eq!(roll_int(0.0, 5, 2), 2);
    assert_eq!(roll_int(0.999_999_88, 2, 5), 5);
    // The whole range's span wraps to zero: every draw gives the low bound.
    assert_eq!(roll_int(0.999_999_88, i32::MIN, i32::MAX), i32::MIN);
    // A span past the largest integer counts as unsigned.
    assert_eq!(roll_int(0.5, -1, i32::MAX - 1), (1 << 30) - 1);
    assert_eq!(roll_float(0.0, 2.0, 1.0), 1.0);
    assert_eq!(roll_float(0.5, 1.0, 2.0), 1.5);
    assert!(roll_float(0.5, f32::NAN, 1.0).is_nan());
}

const RANDOM: u32 = 0x5F;

/// `count` calls of dice native `number` with `args` (`fp[-2]` first) from the state every run
/// starts in: the word each left in its result slot.
fn rolls(number: u32, args: &[u32], count: usize) -> Vec<u32> {
    let lib = library(&[w(op::END)], &[], &[]);
    let table = auberean_glue::natives(&lib);
    let prog = Arc::new(Program::link(lib, table).expect("links"));
    let mut vm = Vm::new(Arc::clone(&prog));
    let mut host = TestHost::default();
    let id = NativeId::new(RANDOM, number).0;
    let Some(Binding::Ported(f)) = prog.table.get(id) else {
        panic!("native {id:#x} is not ported");
    };
    (0..count)
        .map(|_| {
            let mut cx = Context::new(usize::MAX, 0);
            cx.push(0, tag::INT).unwrap();
            for a in args.iter().rev() {
                cx.push(*a, tag::MIXED).unwrap();
            }
            cx.push(0, tag::REFERENCE).unwrap();
            cx.fp = cx.sp;
            let status = f(&mut Call {
                vm: &mut vm,
                cx: &mut cx,
                host: &mut host,
                native: id,
            })
            .expect("the native runs");
            assert_eq!(status, Status::Success);
            cx.slot(0).unwrap().0
        })
        .collect()
}

/// **The normal, percentile and skewed dice give the client's own numbers**: the ratio of
/// uniforms over two draws a try, 1 to 100 from one draw, and a normal number about a mean kept
/// between two bounds (two tries, else the mean; a deviation of 0 is half the distance from the
/// upper bound to the mean), each answer the same bits as the 0930 natives give from the same
/// start, run in an emulator.
///
/// Behaviour: script.glue.the-normal-and-skewed-dice-roll-as-the-client-does
#[test]
fn the_normal_and_skewed_dice_roll_as_the_client_does() {
    let f = f32::to_bits;
    let cases: [(u32, Vec<u32>, [u32; 12]); 5] = [
        (
            0x04,
            vec![f(0.0), f(1.0)],
            [
                0xbf48_5cda,
                0xbed6_6517,
                0xbf1b_7e6d,
                0xbf46_1483,
                0x3e91_07fe,
                0x3ec1_ef37,
                0x3e85_4a69,
                0xbe64_9d8f,
                0x3e60_5a9d,
                0x3f68_edb4,
                0xbe15_a302,
                0x3f3b_e900,
            ],
        ),
        (
            0x04,
            vec![f(5.0), f(2.0)],
            [
                0x405b_d193,
                0x4085_335d,
                0x4072_40ca,
                0x405c_f5be,
                0x40b2_2100,
                0x40b8_3de7,
                0x40b0_a94d,
                0x4091_b627,
                0x40ae_05aa,
                0x40da_3b6d,
                0x4096_a5d0,
                0x40ce_fa40,
            ],
        ),
        (
            0x01,
            vec![],
            [66, 21, 90, 29, 53, 32, 45, 30, 56, 60, 93, 71],
        ),
        (
            0x05,
            vec![f(0.0), f(10.0), f(9.0)],
            [
                0x4109_bd19,
                0x410c_a66c,
                0x410b_240d,
                0x4109_cf5c,
                0x4112_4420,
                0x4113_07bd,
                0x4112_152a,
                0x410e_36c5,
                0x4111_c0b5,
                0x4117_476e,
                0x410e_d4ba,
                0x4115_df48,
            ],
        ),
        (
            0x06,
            vec![f(0.0), f(10.0), f(5.0), f(4.0)],
            [
                0x3fef_464b,
                0x4054_cd74,
                0x4024_8193,
                0x3ff3_d6f9,
                0x40c4_4200,
                0x40d0_7bce,
                0x40c1_529a,
                0x4083_6c4e,
                0x40bc_0b54,
                0x410a_3b6d,
                0x408d_4ba0,
                0x40fd_f480,
            ],
        ),
    ];
    for (number, args, want) in cases {
        assert_eq!(
            rolls(number, &args, 12),
            want,
            "native {number:#x} {args:x?}"
        );
    }
}

const MATH: u32 = 0x44;

/// A double native of `args` (each a double, `fp[-3..-2]` the first), its answer's bits.
fn math(b: &mut crate::call::Bench, number: u32, args: &[f64]) -> u64 {
    let mut words = Vec::new();
    for a in args {
        let (lo, hi) = auberean_script::value::split(a.to_bits());
        words.push(hi);
        words.push(lo);
    }
    let (status, slots) = b.call(MATH, number, auberean_script::value::NULL, &words, 2);
    assert_eq!(status, Status::Success);
    u64::from(slots[1].0) | u64::from(slots[0].0) << 32
}

/// **The math natives answer as the floating-point unit does**: sine, cosine, tangent, the arc
/// functions, logarithms, the exponential (through two to the power's rounded base 2 multiple),
/// the square root and the remainder, each a double; a sine, cosine or tangent of a number from 2
/// to the 63rd is the number itself; an invalid operation (the sine of infinity, the exponential of
/// infinity, a remainder by 0, the root of a negative) answers the unit's own not-a-number; a
/// quotient is mapped into -1 to 1 with the client's own two over pi, and is 0 over 0.
///
/// Behaviour: script.glue.math-as-the-floating-point-unit
#[test]
fn the_math_natives_answer_as_the_floating_point_unit_does() {
    let mut b = crate::call::Bench::new();
    let close = |bits: u64, want: f64| {
        let got = f64::from_bits(bits);
        assert!(
            (got - want).abs() <= want.abs() * 4e-16,
            "{got} against {want}"
        );
    };
    close(math(&mut b, 0x01, &[0.5]), 0.479_425_538_604_203);
    close(math(&mut b, 0x02, &[0.5]), 0.877_582_561_890_372_8);
    close(math(&mut b, 0x03, &[0.5]), 0.546_302_489_843_790_5);
    close(math(&mut b, 0x04, &[0.5]), std::f64::consts::FRAC_PI_3);
    close(math(&mut b, 0x05, &[0.5]), std::f64::consts::FRAC_PI_6);
    close(math(&mut b, 0x06, &[1.0]), std::f64::consts::FRAC_PI_4);
    close(math(&mut b, 0x0B, &[std::f64::consts::E]), 1.0);
    close(math(&mut b, 0x0C, &[1000.0]), 3.0);
    assert_eq!(
        f64::from_bits(math(&mut b, 0x0D, &[1.0])),
        std::f64::consts::E
    );
    assert_eq!(f64::from_bits(math(&mut b, 0x0D, &[0.0])), 1.0);
    assert_eq!(f64::from_bits(math(&mut b, 0x0D, &[1000.0])), f64::INFINITY);
    assert_eq!(f64::from_bits(math(&mut b, 0x0D, &[-1000.0])), 0.0);
    assert_eq!(
        f64::from_bits(math(&mut b, 0x0F, &[2.0])),
        std::f64::consts::SQRT_2
    );
    assert_eq!(f64::from_bits(math(&mut b, 0x11, &[5.5, 2.0])), 1.5);
    assert_eq!(f64::from_bits(math(&mut b, 0x11, &[-5.5, 2.0])), -1.5);
    // Two over pi as the client holds it: a little more than the nearest double.
    let two_over_pi = 0.636_619_772_367_584;
    close(
        math(&mut b, 0x10, &[1.0, 1.0]),
        std::f64::consts::FRAC_PI_4 * two_over_pi,
    );
    close(
        math(&mut b, 0x10, &[-3.0, 2.0]),
        dereth_primitives::num::math::atan(-1.5) * two_over_pi,
    );
    assert_eq!(math(&mut b, 0x10, &[1.0, 0.0]), 0);

    // What the unit does that a library does not.
    let big = 9_223_372_036_854_775_808.0;
    assert_eq!(f64::from_bits(math(&mut b, 0x01, &[big])), big);
    assert_eq!(f64::from_bits(math(&mut b, 0x02, &[-big])), -big);
    let indefinite = 0xFFF8_0000_0000_0000;
    assert_eq!(math(&mut b, 0x01, &[f64::INFINITY]), indefinite);
    assert_eq!(math(&mut b, 0x0D, &[f64::INFINITY]), indefinite);
    assert_eq!(math(&mut b, 0x11, &[1.0, 0.0]), indefinite);
    assert_eq!(math(&mut b, 0x0F, &[-1.0]), indefinite);
    // A not-a-number given comes back quieted.
    assert_eq!(
        math(&mut b, 0x01, &[f64::from_bits(0x7FF0_0000_0000_0001)]),
        0x7FF8_0000_0000_0001
    );
}

// A random selection table.

fn table(b: &crate::call::Bench, r: u32) -> SelectionTable {
    match b.vm.heap.native(r).unwrap() {
        NativeObject::Engine(e) => e.get::<SelectionTable>().expect("a table").clone(),
        o => panic!("not a table: {o:?}"),
    }
}

/// **A random selection table picks each entry once, by weight, until it starts over**: an entry
/// of a weight of 0 or less is not added; the subtree weights are kept as entries are added; a
/// choice rolls the engine's dice from 1 to the total weight and walks down from the root by the
/// running sum (the root's weight, then left or right); the chosen entry leaves the table and the
/// last takes its place; an empty table answers no and leaves the word as it was; starting over
/// puts the chosen back in the order they were chosen. A table read from its packed form chooses
/// the same, and packs back to the same bytes.
///
/// Behaviour: script.glue.a-random-selection-table-picks-each-entry-once
#[test]
fn a_random_selection_table_picks_each_entry_once_by_weight() {
    let mut b = crate::call::Bench::new();
    let t = b.object(TABLE, NativeObject::Opaque { ty: TABLE });
    for (object, weight) in [(10, 1), (20, 2), (30, 3), (40, 0), (50, u32::MAX)] {
        assert_eq!(
            b.call(TABLE, 0x01, t, &[object, weight], 0).0,
            Status::Success
        );
    }
    let entries = |l: &[(u32, u32, u32)]| {
        l.iter()
            .map(|&(object, weight, subtree)| Entry {
                object,
                weight,
                subtree,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        table(&b, t).unused,
        entries(&[(10, 1, 6), (20, 2, 2), (30, 3, 3)])
    );
    assert_eq!(b.answer(TABLE, 0x05, t, &[]) & 0xFF, 0, "not done");

    // The dice draw 0.655..., 0.201..., 0.893... from the first state: rolls of 4 in 1-6, 1 in
    // 1-3 and 2 in 1-2.
    let mut chosen = Vec::new();
    for _ in 0..3 {
        let (status, slots) = b.call_with_out(TABLE, 0x07, t, 0, 1);
        assert_eq!(status, Status::Success);
        assert_eq!(slots[0].0 & 0xFF, 1);
        chosen.push(table(&b, t).used.last().unwrap().object);
    }
    assert_eq!(chosen, [30, 10, 20]);
    assert_eq!(b.answer(TABLE, 0x05, t, &[]) & 0xFF, 1, "done");
    let (_, slots) = b.call_with_out(TABLE, 0x07, t, 0x77, 1);
    assert_eq!(slots[0].0 & 0xFF, 0, "nothing left");

    // Starting over puts them back in the order chosen.
    b.call(TABLE, 0x06, t, &[], 0);
    assert_eq!(
        table(&b, t).unused,
        entries(&[(30, 3, 6), (10, 1, 1), (20, 2, 2)])
    );
    assert!(table(&b, t).used.is_empty());

    // Fast adds then the weights worked out once.
    let f = b.object(TABLE, NativeObject::Opaque { ty: TABLE });
    for (object, weight) in [(1, 5), (2, 6), (3, 7), (4, 8)] {
        b.call(TABLE, 0x02, f, &[object, weight], 0);
    }
    assert_eq!(table(&b, f).unused[0].subtree, 0);
    b.call(TABLE, 0x03, f, &[], 0);
    assert_eq!(
        table(&b, f).unused,
        entries(&[(1, 5, 26), (2, 6, 14), (3, 7, 7), (4, 8, 8)])
    );
    b.call(TABLE, 0x04, f, &[], 0);
    assert!(table(&b, f).unused.is_empty());

    // From the packed form.
    let packed = table(&b, t).pack().unwrap();
    let p = b.object(
        TABLE,
        NativeObject::Packed {
            ty: TABLE,
            bytes: packed.clone(),
        },
    );
    b.call_with_out(TABLE, 0x07, p, 0, 1);
    assert_eq!(table(&b, p).used.len(), 1);
    b.call(TABLE, 0x06, p, &[], 0);
    let SelectionTable { unused, .. } = table(&b, p);
    assert_eq!(unused.len(), 3);
    assert_eq!(SelectionTable::unpack(&packed), Some(table(&b, t)));
}

/// The wall clock answers seconds since 1970 from the host's clock, and -1 (the C library's
/// answer when there is no time to be had) without one; the computed time is the game's clock.
///
/// Behaviour: script.glue.the-wall-clock-and-the-computed-time
#[test]
fn the_wall_clock_comes_from_the_host_and_answers_minus_one_without_one() {
    const TIMER: u32 = 0x6C;
    let mut b = crate::call::Bench::new();
    let (status, slots) = b.call(TIMER, 0x03, 0, &[], 1);
    assert_eq!(
        (status, slots[0].0),
        (auberean_script::Status::Success, u32::MAX)
    );
    b.host.real_time = Some(1_484_006_400);
    let (_, slots) = b.call(TIMER, 0x03, 0, &[], 1);
    assert_eq!(slots[0].0, 1_484_006_400);
    // The computed time is a 64-bit float in two slots, as the frame's time is.
    let (status, slots) = b.call(TIMER, 0x01, 0, &[], 2);
    assert_eq!(status, auberean_script::Status::Success);
    assert_eq!((slots[0].0, slots[1].0), (0, 0), "the host's clock reads 0");
}
