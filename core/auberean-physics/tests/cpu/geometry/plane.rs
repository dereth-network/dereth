use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::plane_cases::CASES;
use super::{bits, check};
use auberean_physics::geom::{Plane, PlaneOps, Ray};

/// Every plane operation answers as the September 2013 client's own code does, to the bit: a
/// point's height on the plane (both ways the client works it), whether a plane is one, a plane
/// carried from one position's frame into another's landblock, where and how far along a ray meets
/// the plane, where a segment crosses it, a ray's span clipped to its positive side, and a ray cut
/// in two by it.
///
/// Behaviour: geometry.plane.every-operation-answers-as-the-client-does
#[test]
fn every_plane_operation_answers_as_the_client_does() {
    check(CASES, |label, x, inp| {
        let v = |i: usize| Vec3::new(x[i], x[i + 1], x[i + 2]);
        let plane = Plane::new(v(0), x[3]);
        let ray = |i: usize| Ray {
            point: v(i),
            dir: v(i + 3),
            length: x[i + 6],
        };
        let ray_bits = |r: &Ray| {
            bits(&[
                r.point.x, r.point.y, r.point.z, r.dir.x, r.dir.y, r.dir.z, r.length,
            ])
        };
        let out = match label {
            "set_height" => {
                let mut p = v(4);
                let ok = plane.set_height(&mut p);
                let mut o = bits(&[p.x, p.y, p.z]);
                o.push(u32::from(ok));
                o
            }
            "snap_to_plane" => {
                let mut p = v(4);
                plane.snap_to_plane(&mut p);
                bits(&[p.x, p.y, p.z])
            }
            "intersect_line_segment" => match plane.intersect_line_segment(v(4), v(7)) {
                Some(p) => [vec![1], bits(&[p.x, p.y, p.z])].concat(),
                None => [vec![0], bits(&[9.0, 9.0, 9.0])].concat(),
            },
            "time_of_intersection" => match plane.time_of_intersection(&ray(4)) {
                Some((t, ahead)) => {
                    let b = t.to_bits();
                    #[allow(clippy::cast_possible_truncation)]
                    let o = vec![u32::from(ahead), b as u32, (b >> 32) as u32];
                    o
                }
                None => vec![0, 0x1111_1111, 0x1111_1111],
            },
            "find_intercept" => match plane.find_intercept(&ray(4)) {
                Some(p) => [vec![1], bits(&[p.x, p.y, p.z])].concat(),
                None => [vec![0], bits(&[9.0, 9.0, 9.0])].concat(),
            },
            "clip_ray_to_positive" => {
                let (mut a, mut b) = (x[11], x[12]);
                let ok = plane.clip_ray_to_positive(&ray(4), &mut a, &mut b);
                vec![u32::from(ok), a.to_bits(), b.to_bits()]
            }
            "cut_ray" => {
                let fill = |k: f32| Ray {
                    point: Vec3::new(k, k, k),
                    dir: Vec3::new(k, k, k),
                    length: k,
                };
                let (mut p, mut q) = (fill(9.0), fill(8.0));
                let (ok, hp, hn) = match plane.cut_ray(&ray(4), &mut p, &mut q) {
                    Some((hp, hn)) => (1, u32::from(hp), u32::from(hn)),
                    None => (0, 7, 7),
                };
                [vec![ok, hp, hn], ray_bits(&p), ray_bits(&q)].concat()
            }
            "local_to_global" => {
                let pos = |i: usize| {
                    Position::new(
                        CellId(inp[i]),
                        Frame::new(v(i + 1), Quat::new(x[i + 4], x[i + 5], x[i + 6], x[i + 7])),
                    )
                };
                let g = plane.local_to_global(&pos(4), &pos(12));
                bits(&[g.normal.x, g.normal.y, g.normal.z, g.d])
            }
            "is_valid" => vec![u32::from(plane.is_valid())],
            _ => return None,
        };
        Some(out)
    });
}
