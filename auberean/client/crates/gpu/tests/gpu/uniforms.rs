//! A frame's draws and their uniforms. Fixture: made-up flat squares in a grid, each one texel of
//! the picture, seen with no fog.

use auberean_primitives::{Quat, Vec3};
use auberean_world::math::Mat4;

use crate::common::{checked, renderer};
use crate::fade::{frame_of, wall};

const SIDE: u32 = 64;
/// More draws than one of the device's buffers of uniforms holds.
const DRAWS: u32 = 2500;

/// However many draws a frame has, each shows its own uniforms: here its own gray, on either side
/// of where one buffer of uniforms gives way to the next.
///
/// Behaviour: draw.mesh.every-draw-of-a-frame-shows-its-own-uniforms
#[test]
fn every_draw_of_a_frame_shows_its_own_uniforms() {
    let mut device = renderer();
    let gray = |i: u32| i % 100 + 20;
    // A wall 10 m away fills a picture this tall, from a 60 degree view (the tangent of 30 degrees).
    let half = 10.0 * 0.577_350_3_f32;
    let cell = half / SIDE as f32;
    let objects = (0..DRAWS)
        .map(|i| {
            let (x, y) = (i % SIDE, i / SIDE);
            let at = |k: u32| (2.0 * k as f32 + 1.0) * cell - half;
            let mut b = wall(10.0, gray(i) as f32 / 255.0, false, 10.0);
            b.transform = Mat4::from_parts(
                Vec3::new(at(x), 0.0, half - (2.0 * y as f32 + 1.0) * cell),
                Quat::IDENTITY,
                Vec3::new(cell / 100.0, 1.0, cell / 100.0),
            );
            b
        })
        .collect();
    let frame = frame_of(objects);
    let mut load = |_: u32| None;
    let picture = checked(&mut device, |d| {
        d.render(&frame, SIDE, SIDE, &mut load).expect("read back")
    });
    for i in 0..DRAWS {
        let (x, y) = (i % SIDE, i / SIDE);
        let shown = i32::from(picture.rgba[((y * SIDE + x) * 4) as usize]);
        let want = 2 * gray(i) as i32;
        assert!(
            (shown - want).abs() <= 2,
            "draw {i}: {shown} against {want}"
        );
    }
}
