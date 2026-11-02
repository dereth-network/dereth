use dereth_primitives::Vec3;

use super::polygon_cases::CASES;
use super::{bits, check};
use auberean_physics::geom::polygon::Outline;
use auberean_physics::geom::{Plane, Ray};

fn polygon(x: &[f32], inp: &[u32]) -> (Outline, usize) {
    let k = inp[0] as usize;
    let vertices = (0..k)
        .map(|i| Vec3::new(x[1 + 3 * i], x[2 + 3 * i], x[3 + 3 * i]))
        .collect();
    (Outline { vertices }, 1 + 3 * k)
}

fn corners(p: &Outline) -> Vec<u32> {
    let mut out = vec![u32::try_from(p.vertices.len()).unwrap_or(0)];
    for v in &p.vertices {
        out.extend(bits(&[v.x, v.y, v.z]));
    }
    out
}

/// Every outline operation answers as the September 2013 client's own code does, to the
/// bit: an edge as a ray, an edge's upright plane, an edge replaced by its middle, whether the
/// corners lie in a plane, the box around them, short edges dropped, whether a point lies outside,
/// the area in space and on the ground, whether the polygon is degenerate or convex, and corners
/// on straight runs or concave turns dropped.
///
/// Behaviour: geometry.outline.every-operation-answers-as-the-client-does
#[test]
fn every_outline_operation_answers_as_the_client_does() {
    check(CASES, |label, x, inp| {
        let (mut p, at) = polygon(x, inp);
        let arg = |i: usize| inp[at + i];
        let f = |i: usize| x[at + i];
        let flag = |i: usize| arg(i) != 0;
        let ray_bits = |r: &Ray| {
            bits(&[
                r.point.x, r.point.y, r.point.z, r.dir.x, r.dir.y, r.dir.z, r.length,
            ])
        };
        let out = match label {
            "edge" => match p.edge(arg(0) as usize) {
                Some(r) => [vec![1], ray_bits(&r)].concat(),
                None => [vec![0], bits(&[9.0; 7])].concat(),
            },
            "edge_plane" => match p.edge_plane(arg(0) as usize) {
                Some(pl) => [
                    vec![1],
                    bits(&[pl.normal.x, pl.normal.y, pl.normal.z, pl.d]),
                ]
                .concat(),
                None => [vec![0], bits(&[9.0; 4])].concat(),
            },
            "replace_edge_with_midpoint" => {
                p.replace_edge_with_midpoint(arg(0) as usize);
                corners(&p)
            }
            "area" => bits(&[p.area()]),
            "area_2d" => bits(&[p.area_2d()]),
            "is_degenerate" => vec![u32::from(p.is_degenerate(f(0)))],
            "bounds" => match p.bounds() {
                Some(b) => [
                    vec![1],
                    bits(&[b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z]),
                ]
                .concat(),
                None => [vec![0], bits(&[9.0; 6])].concat(),
            },
            "check_if_in_plane" => {
                let pl = Plane::new(Vec3::new(f(0), f(1), f(2)), f(3));
                vec![u32::from(p.check_if_in_plane(&pl, f(4)))]
            }
            "is_convex" => vec![u32::from(p.is_convex(flag(0), f(1), f(2)))],
            "is_outside" => {
                let q = Vec3::new(f(3), f(4), f(5));
                vec![u32::from(p.is_outside(q, flag(0), f(1), f(2)))]
            }
            "remove_short_edges" => {
                let ok = p.remove_short_edges(f(0), flag(1));
                [vec![u32::from(ok)], corners(&p)].concat()
            }
            "remove_colinear_and_concave_points" => {
                let ok = p.remove_colinear_and_concave_points(flag(0), f(1));
                [vec![u32::from(ok)], corners(&p)].concat()
            }
            _ => return None,
        };
        Some(out)
    });
}
