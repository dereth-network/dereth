use dereth_primitives::{Frame, Quat, Vec3};

use super::boxes_cases::CASES;
use super::{bits, check};
use auberean_physics::geom::{BBox, BoxOps, Heading};
use auberean_physics::pack::{
    write_bbox, write_frame, write_quat, Writer, BBOX_SIZE, FRAME_SIZE, QUAT_SIZE,
};

/// The words written, after the size answered and the bytes used, as the client's table gives
/// them.
fn written(size: usize, w: &Writer) -> Vec<u32> {
    let b = w.as_bytes();
    #[allow(clippy::cast_possible_truncation)]
    let mut out = vec![size as u32, b.len() as u32];
    out.extend(
        b.chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])),
    );
    out
}

/// Boxes made from two corners, put in order, scaled about their centers and measured for their
/// radius; headings as radians either side of north and turned on by another; and the binary form
/// of a box, a rotation and a frame all answer as the September 2013 client's own code does, to
/// the bit.
///
/// Behaviour: geometry.box.every-operation-answers-as-the-client-does
#[test]
fn every_box_heading_and_packing_operation_answers_as_the_client_does() {
    check(CASES, |label, x, _| {
        let v = |i: usize| Vec3::new(x[i], x[i + 1], x[i + 2]);
        let bb = |b: &BBox| bits(&[b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z]);
        let q = |i: usize| Quat::new(x[i], x[i + 1], x[i + 2], x[i + 3]);
        let out = match label {
            "from_corners" => bb(&BBox::from_corners(v(0), v(3))),
            "make_valid" => {
                let mut b = BBox::new(v(0), v(3));
                b.make_valid();
                bb(&b)
            }
            "scale_centered" => {
                let mut b = BBox::new(v(0), v(3));
                b.scale_centered(v(6));
                bb(&b)
            }
            "radius" => bits(&[BBox::new(v(0), v(3)).radius()]),
            "pack_bbox" => {
                let mut w = Writer::new();
                write_bbox(&mut w, &BBox::new(v(0), v(3)));
                written(BBOX_SIZE, &w)
            }
            "radians_centered" => bits(&[Heading::new(x[0]).radians_centered()]),
            "heading_plus" => bits(&[Heading::new(x[0]).plus(Heading::new(x[1])).degrees()]),
            "pack_quat" => {
                let mut w = Writer::new();
                write_quat(&mut w, q(0));
                written(QUAT_SIZE, &w)
            }
            "pack_frame" => {
                let mut w = Writer::new();
                write_frame(&mut w, &Frame::new(v(0), q(3)));
                written(FRAME_SIZE, &w)
            }
            _ => return None,
        };
        Some(out)
    });
}
