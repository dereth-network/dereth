//! A body follows the client's own sub-steps to the last bit: walking up a steep slope, sledding
//! off a slope's crest, through the air and back onto the slope, walking across the corner of
//! four landblocks, and walking straight on. Each sub-step starts from the client's state, takes
//! the client's animation offset and time step, and must end with the client's position,
//! velocity and achieved velocity exactly; a single rounding taken at the wrong precision anywhere
//! in the step shows as one unit in the last place.
//! Fixture: the client's recorded sub-steps (as bit patterns) and the height-table indices of the
//! landblocks they cross, under the client's human body.

use dereth_physics::math;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, Position, Quat};

use crate::common::client_steps::{
    assert_follows_the_client, bits, land, place, v3, Start, SubStep,
};
use crate::common::physics_fixture::flat_world;

/// The landblock under the steep slope.
const STEEP: [u8; 81] = [
    50, 44, 46, 49, 51, 54, 56, 58, 61, 50, 54, 58, 60, 64, 67, 69, 71, 74, 57, 72, 72, 71, 75, 79,
    82, 84, 87, 63, 72, 72, 81, 86, 90, 94, 96, 99, 65, 72, 88, 88, 94, 98, 101, 105, 109, 65, 73,
    88, 88, 101, 104, 107, 109, 113, 64, 73, 83, 92, 105, 106, 108, 109, 112, 64, 73, 81, 91, 103,
    107, 109, 111, 113, 66, 73, 80, 86, 104, 108, 110, 112, 112,
];

/// Up a 48 degree slope, where the body is lifted off the ground it sinks into, moved along it and
/// set back down on it in every sub-step. Rounding any of those as the client does not leaves the
/// body one unit in the last place higher or lower, and its achieved velocity off by far more.
///
/// Behaviour: physics.slope.a-body-walking-up-a-steep-slope-stands-at-the-clients-height
#[test]
fn a_body_walking_up_a_steep_slope_stands_at_the_clients_height_to_the_last_bit() {
    let first = Start {
        cell: 0xbf1a0008,
        origin: [0x40400000, 0x43300000, 0x42f28290],
        rotation: [0x3f485945, 0x00000000, 0x00000000, 0xbf1f5ccb],
        velocity: [0x00000000, 0x00000000, 0x00000000],
        acceleration: [0x00000000, 0x00000000, 0x00000000],
        contact_plane: [0xbf397652, 0xbe2b3224, 0x3f2b3224, 0xc245f1fa],
        sliding_normal: [0x00000000, 0x00000000, 0x00000000],
        state: 0x400408,
        transient_state: 0x83,
    };
    let steps = [
        SubStep {
            quantum: 0x3d888888,
            offset: [0x320435df, 0x3db44813, 0x00000000],
            origin: [0x4042746d, 0x43300245, 0x42f298f9],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3f13498d, 0x3e082c01, 0x3f281381],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x3278abec, 0x3e298b28, 0x00000000],
            origin: [0x4047126d, 0x43300689, 0x42f2c31f],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3f8a8401, 0x3e7ff001, 0x3f9e0e81],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x3274cfe6, 0x3e26e98d, 0x00000000],
            origin: [0x404b9e16, 0x43300abc, 0x42f2ec9d],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3f885dcf, 0x3e7bf401, 0x3f9b9881],
        },
        SubStep {
            quantum: 0x3d088890,
            offset: [0x3201181c, 0x3db0083b, 0x00000000],
            origin: [0x404e03b6, 0x43300cf3, 0x42f3027f],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3f8fd178, 0x3e84e3f9, 0x3fa41ef7],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32841104, 0x3e3415d3, 0x00000000],
            origin: [0x4052eb33, 0x4330117b, 0x42f32f44],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3f9320a7, 0x3e87f001, 0x3fa7e2c1],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32a62934, 0x3e62935f, 0x00000000],
            origin: [0x405916d0, 0x4330172f, 0x42f36798],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3fb91c67, 0x3eab1801, 0x3fd33b01],
        },
    ];
    assert_follows_the_client(
        &mut land(&[(LandblockId::new(0xBF, 0x1A), STEEP)]),
        &first,
        &steps,
    );

    // Further up, where the step along the slope is the client's only when the distance into the
    // slope that is taken off it is rounded once.
    let later = Start {
        cell: 0xbf1a0008,
        origin: [0x410190c3, 0x43312d99, 0x42fe24f7],
        rotation: [0x3f485945, 0x00000000, 0x00000000, 0xbf1f5ccb],
        velocity: [0x00000000, 0x00000000, 0x00000000],
        acceleration: [0x00000000, 0x00000000, 0x00000000],
        contact_plane: [0xbf397652, 0xbe2b3224, 0x3f2b3224, 0xc245f1fa],
        sliding_normal: [0x00000000, 0x00000000, 0x00000000],
        state: 0x400408,
        transient_state: 0x83,
    };
    let steps = [
        SubStep {
            quantum: 0x3d888888,
            offset: [0x3316310b, 0x3eccccab, 0x00000000],
            origin: [0x41045aa9, 0x433137e8, 0x42fe8acb],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x402751e9, 0x3f1aa101, 0x403eed81],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32c84164, 0x3e888872, 0x00000000],
            origin: [0x41063696, 0x43313ec8, 0x42feceae],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3fdf1719, 0x3ece4001, 0x3ffe9341],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32c84164, 0x3e888872, 0x00000000],
            origin: [0x41081283, 0x433145a8, 0x42ff1291],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x3fdf1719, 0x3ece4001, 0x3ffe9341],
        },
    ];
    assert_follows_the_client(
        &mut land(&[(LandblockId::new(0xBF, 0x1A), STEEP)]),
        &later,
        &steps,
    );
}

/// A sled pushed up off an 11 degree slope flies, falls back onto the slope with a slight bounce
/// and slides on. The flight follows the client's integration of velocity and gravity, the
/// landing its bounce and the slide its friction, each to the last bit.
///
/// Behaviour: physics.sledding.a-sled-flies-lands-and-slides-as-the-clients
#[test]
fn a_pushed_sled_flies_lands_and_slides_as_the_clients_to_the_last_bit() {
    let mut world = land(&[(
        LandblockId::new(0x7D, 0xF5),
        [
            23, 22, 21, 20, 18, 16, 14, 12, 10, 25, 24, 23, 22, 20, 18, 16, 14, 12, 27, 26, 25, 24,
            22, 20, 18, 16, 14, 29, 28, 27, 26, 24, 22, 21, 18, 16, 31, 30, 29, 28, 26, 25, 23, 21,
            19, 32, 32, 31, 30, 29, 27, 25, 24, 21, 34, 33, 33, 32, 31, 29, 28, 26, 24, 36, 35, 34,
            34, 33, 31, 30, 28, 26, 38, 37, 37, 37, 37, 36, 32, 30, 28,
        ],
    )]);
    let start = Start {
        cell: 0x7df50019,
        origin: [0x42b38a03, 0x415f61a3, 0x426f366c],
        rotation: [0x3f800000, 0x00000000, 0x00000000, 0x80000000],
        velocity: [0x00000000, 0x408a58e0, 0x4027a052],
        acceleration: [0x00000000, 0x00000000, 0xc11ccccd],
        contact_plane: [0xbe27c759, 0x3da7c758, 0x3f7bab05, 0xc234e2eb],
        sliding_normal: [0x00000000, 0x3f800000, 0x00000000],
        state: 0xc00408,
        transient_state: 0x81,
    };
    let steps = [
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32644a90, 0x3e1ba5e3, 0x00000000],
            origin: [0x42b38a03, 0x4163fe33, 0x426fd2ec],
            velocity: [0x00000000, 0x408a58e0, 0x3ffba037],
            cached_velocity: [0x00000000, 0x408a58e1, 0x4012b801],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32e44a90, 0x3e9ba5e3, 0x00000000],
            origin: [0x42b38a03, 0x41689ac3, 0x427042d2],
            velocity: [0x00000000, 0x408a58e0, 0x3fa7ffca],
            cached_velocity: [0x00000000, 0x408a58e1, 0x3fd1cf41],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32644a90, 0x3e1ba5e3, 0x00000000],
            origin: [0x42b38a03, 0x416d3753, 0x4270861f],
            velocity: [0x00000000, 0x408a58e0, 0x3f28beba],
            cached_velocity: [0x00000000, 0x408a58e1, 0x3f7c60c1],
        },
        SubStep {
            quantum: 0x3d088890,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b38a03, 0x416f859b, 0x4270970c],
            velocity: [0x00000000, 0x408a58e0, 0x3eaa3c90],
            cached_velocity: [0x00000000, 0x408a58d9, 0x3efde2f2],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b38a03, 0x4174222b, 0x42709772],
            velocity: [0x00000000, 0x408a58e0, 0xbea44524],
            cached_velocity: [0x00000000, 0x408a58e1, 0x3bbf4001],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b38a03, 0x4178bebb, 0x42706b3e],
            velocity: [0x00000000, 0x408a58e0, 0xbf79636c],
            cached_velocity: [0x00000000, 0x408a58e1, 0xbf25c301],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b38a03, 0x417d5b4b, 0x42701270],
            velocity: [0x00000000, 0x408a58e0, 0xbfd05223],
            cached_velocity: [0x00000000, 0x408a58e1, 0xbfa68241],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b38a03, 0x4180fbed, 0x426f8d08],
            velocity: [0x00000000, 0x408a58e0, 0xc011f948],
            cached_velocity: [0x00000000, 0x408a58c3, 0xbffa2301],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b38a03, 0x41834a35, 0x426edb07],
            velocity: [0x00000000, 0x408a58e0, 0xc03bc97e],
            cached_velocity: [0x00000000, 0x408a58e1, 0xc026e0f1],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b38a03, 0x4185987d, 0x426e4cc5],
            velocity: [0xbf0bba66, 0x40931486, 0xbea010e0],
            cached_velocity: [0x00000000, 0x408a58e1, 0xc0055de1],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b376bc, 0x4188049c, 0x426e2c81],
            velocity: [0xbf031c02, 0x40907fa7, 0xbf8eb7fa],
            cached_velocity: [0xbf109481, 0x40915745, 0xbef1fe01],
        },
        SubStep {
            quantum: 0x3d088890,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b36bd4, 0x41893b0e, 0x426e1bef],
            velocity: [0xbf1ce0d9, 0x40911960, 0xbf4e8180],
            cached_velocity: [0xbf2397f7, 0x40918568, 0xbef88df3],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b353a7, 0x418ba422, 0x426dfa2a],
            velocity: [0xbf27d540, 0x408fc938, 0xbf918a9a],
            cached_velocity: [0xbf355181, 0x4090a0b1, 0xbefd4581],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x00000000, 0x00000000, 0x00000000],
            origin: [0x42b33845, 0x418e0b3c, 0x426dd768],
            velocity: [0xbf3fe5a8, 0x408f525e, 0xbf93645a],
            cached_velocity: [0xbf4d5f01, 0x40902a19, 0xbf025781],
        },
    ];
    assert_follows_the_client(&mut world, &start, &steps);
}

/// Walking diagonally away from the corner where four landblocks meet, on level ground: the
/// body's position close to its landblock's origin shows the client's rounding of each step's
/// offset, turned into the world by the body's facing.
///
/// Behaviour: physics.motion.a-body-walking-on-level-ground-moves-as-the-clients
#[test]
fn a_body_walking_from_a_landblock_corner_moves_as_the_clients_to_the_last_bit() {
    let level = [49; 81];
    let mut world = land(&[
        (LandblockId::new(0xB2, 0xB5), level),
        (LandblockId::new(0xB2, 0xB6), level),
        (LandblockId::new(0xB3, 0xB5), level),
        (LandblockId::new(0xB3, 0xB6), level),
    ]);
    let start = Start {
        cell: 0xb3b60001,
        origin: [0x3c288000, 0x3c288000, 0x42c40290],
        rotation: [0x3f6c835f, 0x00000000, 0x00000000, 0xbec3ef15],
        velocity: [0x00000000, 0x00000000, 0x00000000],
        acceleration: [0x00000000, 0x00000000, 0x00000000],
        contact_plane: [0x00000000, 0x00000000, 0x3f800000, 0xc2c40000],
        sliding_normal: [0x00000000, 0x00000000, 0x00000000],
        state: 0x400408,
        transient_state: 0x83,
    };
    let steps = [
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32c84164, 0x3e888872, 0x00000000],
            origin: [0x3e4b9e3a, 0x3e4b9e38, 0x42c40290],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x403504d7, 0x403504d5, 0x00000000],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32c84164, 0x3e888872, 0x00000000],
            origin: [0x3ec65a3a, 0x3ec65a38, 0x42c40290],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x403504d7, 0x403504d5, 0x00000000],
        },
        SubStep {
            quantum: 0x3d888888,
            offset: [0x32c84164, 0x3e888872, 0x00000000],
            origin: [0x3f1372ab, 0x3f1372aa, 0x42c40290],
            velocity: [0x00000000, 0x00000000, 0x00000000],
            cached_velocity: [0x403504d5, 0x403504d5, 0x00000000],
        },
    ];
    assert_follows_the_client(&mut world, &start, &steps);
}

/// The velocity a body achieved in a sub-step is the client's to the last bit: a diagonal step
/// of equal parts east and north comes out a unit in the last place apart, as the client divides
/// the two by the time step at different precisions.
///
/// Behaviour: physics.motion.a-bodys-achieved-velocity-is-the-clients
#[test]
fn a_bodys_achieved_velocity_is_the_clients_to_the_last_bit() {
    let cell = CellId(0xB2B5_0040);
    let at = |b| Position::new(cell, Frame::new(v3(b), Quat::IDENTITY));
    let from = at([0x432f_c057, 0x432f_c057, 0x42c4_0290]);
    let to = at([0x432f_d311, 0x432f_d311, 0x42c4_0290]);
    let quantum = f64::from(f32::from_bits(0x3d13_74c0));
    let achieved = [0x4002_0ba8, 0x4002_0ba7, 0x0000_0000];
    assert_eq!(bits(math::achieved_velocity(&from, &to, quantum)), achieved);

    // And the body that took that step keeps it.
    let start = Start {
        cell: 0xb2b5_0040,
        origin: [0x432f_c057, 0x432f_c057, 0x42c4_0290],
        rotation: [0x3f6c_835f, 0x0000_0000, 0x0000_0000, 0xbec3_ef15],
        velocity: [0; 3],
        acceleration: [0; 3],
        contact_plane: [0x0000_0000, 0x0000_0000, 0x3f80_0000, 0xc2c4_0000],
        sliding_normal: [0; 3],
        state: 0x40_0408,
        transient_state: 0x83,
    };
    let step = SubStep {
        quantum: 0x3d13_74c0,
        offset: [0x321b_60d0, 0x3dd3_dfb1, 0x0000_0000],
        origin: [0x432f_d311, 0x432f_d311, 0x42c4_0290],
        velocity: [0; 3],
        cached_velocity: achieved,
    };
    assert_follows_the_client(
        &mut land(&[(LandblockId::new(0xB2, 0xB5), [49; 81])]),
        &start,
        &[step],
    );
}

/// A body facing east keeps the client's rotation to the last bit while it
/// walks straight on: each step's turn (none) is blended in and the rotation made unit length as
/// the client does it, which leaves this one as it was, where rounding every step would nudge it
/// up by one unit in the last place.
///
/// Behaviour: physics.motion.a-body-walking-straight-keeps-the-clients-facing
#[test]
fn a_body_walking_straight_keeps_the_clients_facing_to_the_last_bit() {
    let mut world = flat_world();
    let facing = [0x3f35_04f3, 0x0000_0000, 0x0000_0000, 0xbf35_04f3];
    let start = Start {
        cell: 0xa9b4_0021,
        origin: [0x4220_0000, 0x4220_0000, 0x41a0_147b],
        rotation: facing,
        velocity: [0; 3],
        acceleration: [0; 3],
        contact_plane: [0x0000_0000, 0x0000_0000, 0x3f80_0000, 0xc1a0_0000],
        sliding_normal: [0; 3],
        state: 0x40_0408,
        transient_state: 0x83,
    };
    let step = Frame::new(v3([0x32c8_4164, 0x3e88_8872, 0]), Quat::IDENTITY);
    let h = place(&mut world, &start, vec![step; 4]);
    let x0 = world.get(h).expect("live").position.frame.origin.x;
    for i in 0..4 {
        world.get_mut(h).expect("live").update_time = 0.0;
        world.update_object(h, LocalTime(1.0 / 15.0));
        let q = world.get(h).expect("live").position.frame.rotation;
        assert_eq!(
            [q.w, q.x, q.y, q.z].map(f32::to_bits),
            facing,
            "sub-step {i}: {q:?}"
        );
    }
    let x = world.get(h).expect("live").position.frame.origin.x;
    assert!(x > x0 + 0.5, "and it walked east: {x0} -> {x}");
}
