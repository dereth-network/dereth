//! Dice: the engine's random number generator and the rolls drawn from it.
//! Fixture: the natives called as the machine calls one (the result slot, the arguments last
//! first, the receiver), on a host drawing from a generator in the state every run starts in.

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
