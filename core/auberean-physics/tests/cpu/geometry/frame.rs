use auberean_physics::geom::matrix::set_rotation_axes_and_normalize;
use auberean_physics::geom::{
    frame_facing, frame_matrix, frame_pitch, frame_slerp, global_to_local, local_to_global,
    orient_frame, point_to_global, point_to_local, quat_transform, rotation_axis,
    rotation_from_axes, set_pitch, set_vector_heading,
};
use dereth_primitives::{Frame, Quat, Vec3};

use super::frame_cases::CASES;
use super::{bits, check, f, Case};

fn frame_bits(fr: &Frame) -> Vec<u32> {
    let (o, q) = (fr.origin, fr.rotation);
    bits(&[o.x, o.y, o.z, q.w, q.x, q.y, q.z])
}

/// Every frame and rotation operation answers as the September 2013 client's own code does, to the
/// bit: a vector turned by a rotation, the rotation about an axis, the rotation of three axes (as
/// given, and made unit length), a vector or a point carried into and out of a frame (by the
/// frame's own copies of these and by a position's), a frame's matrix, the direction it faces and
/// its pitch, a frame set from the directions it faces and has above it, turned to face along a
/// vector, and tilted to a pitch.
///
/// Behaviour: geometry.frame.every-operation-answers-as-the-client-does
#[test]
fn every_frame_operation_answers_as_the_client_does() {
    let cases: Vec<Case> = CASES
        .iter()
        .copied()
        .filter(|c| c.0 != "frame_slerp")
        .collect();
    check(&cases, |label, x, _| {
        let v = |i: usize| Vec3::new(x[i], x[i + 1], x[i + 2]);
        let q = |i: usize| Quat::new(x[i], x[i + 1], x[i + 2], x[i + 3]);
        let fr = || Frame::new(v(0), q(3));
        let vb = |r: Vec3| bits(&[r.x, r.y, r.z]);
        let qb = |r: Quat| bits(&[r.w, r.x, r.y, r.z]);
        let axes = || {
            let mut m = [0.0; 9];
            m.copy_from_slice(&x[..9]);
            m
        };
        let out = match label {
            "quat_transform" => vb(quat_transform(q(0), v(4))),
            "local_to_global" => vb(local_to_global(&fr(), v(7))),
            "global_to_local" => vb(global_to_local(&fr(), v(7))),
            "point_to_global" => vb(point_to_global(&fr(), v(7))),
            "point_to_local" => vb(point_to_local(&fr(), v(7))),
            "frame_matrix" => bits(&frame_matrix(&fr()).0),
            "frame_facing" => vb(frame_facing(&fr())),
            "frame_pitch" => bits(&[frame_pitch(&fr())]),
            "set_pitch" => {
                let mut a = fr();
                set_pitch(&mut a, x[7]);
                frame_bits(&a)
            }
            "set_vector_heading" => {
                let mut a = fr();
                set_vector_heading(&mut a, v(7));
                frame_bits(&a)
            }
            "orient_frame" => {
                let mut a = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
                orient_frame(&mut a, v(0), v(3));
                frame_bits(&a)
            }
            "rotation_axis" => qb(rotation_axis(v(0), x[3])),
            "rotation_from_axes" => qb(rotation_from_axes(&axes())),
            "rotation_from_axes_normalized" => qb(set_rotation_axes_and_normalize(&axes())),
            _ => return None,
        };
        Some(out)
    });
}

/// The frame part of the way between two others takes its origin along the line to the bit, as
/// the client works it, and its rotation along the shorter arc within half a millionth (the
/// client's arc comes from its vector library's own routine).
///
/// Behaviour: geometry.frame.the-frame-between-two-takes-the-line-and-the-shorter-arc
#[test]
fn the_frame_between_two_takes_the_line_and_the_shorter_arc() {
    let mut wrong = Vec::new();
    for &(_, inputs, want) in CASES.iter().filter(|c| c.0 == "frame_slerp") {
        let x: Vec<f32> = inputs.iter().map(|&b| f(b)).collect();
        let a = Frame::new(
            Vec3::new(x[0], x[1], x[2]),
            Quat::new(x[3], x[4], x[5], x[6]),
        );
        let b = Frame::new(
            Vec3::new(x[7], x[8], x[9]),
            Quat::new(x[10], x[11], x[12], x[13]),
        );
        let got = frame_bits(&frame_slerp(&a, &b, x[14]));
        let origin_same = got[..3] == want[..3];
        let turn_close = got[3..]
            .iter()
            .zip(&want[3..])
            .all(|(&g, &w)| (f(g) - f(w)).abs() <= 5e-7);
        if !(origin_same && turn_close) {
            wrong.push(format!(
                "{x:?}: client {:?}, port {:?}",
                want.iter().map(|&b| f(b)).collect::<Vec<_>>(),
                got.iter().map(|&b| f(b)).collect::<Vec<_>>()
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
