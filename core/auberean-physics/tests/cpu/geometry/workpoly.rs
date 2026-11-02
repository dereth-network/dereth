use auberean_assets::mesh::MeshPolygon;
use dereth_primitives::Vec3;

use super::workpoly_cases::CASES;
use super::{bits, check};
use auberean_physics::cells::WorkPolygon;
use auberean_physics::geom::Matrix4;
use auberean_physics::pack::{polygon_size, write_polygon, Writer};

/// A clipping polygon's area and its corners carried through a matrix, and a physics polygon's
/// binary size and form, answer as the September 2013 client's own code does, to the bit.
///
/// Behaviour: geometry.workpoly.areas-projections-and-packing-answer-as-the-client-does
#[test]
fn work_polygons_and_physics_polygon_packing_answer_as_the_client_does() {
    check(CASES, |label, x, inp| {
        let k = inp[0] as usize;
        // The packing cases give only the corner count.
        let given = if x.len() > 1 { k } else { 0 };
        let corners: Vec<Vec3> = (0..given)
            .map(|i| Vec3::new(x[1 + 3 * i], x[2 + 3 * i], x[3 + 3 * i]))
            .collect();
        let poly = MeshPolygon {
            id: 0x1234,
            vertices: (0..u16::try_from(k).unwrap_or(0)).collect(),
        };
        let out = match label {
            "area" => bits(&[WorkPolygon { corners }.area()]),
            "projected" => {
                let mut m = [0.0; 16];
                m.copy_from_slice(&x[1 + 3 * k..17 + 3 * k]);
                let p = WorkPolygon::projected(&corners, &Matrix4(m));
                let mut out = vec![u32::try_from(p.corners.len()).unwrap_or(0)];
                for c in &p.corners {
                    out.extend(bits(&[c.x, c.y, c.z]));
                }
                out
            }
            "pack_size" => vec![u32::try_from(polygon_size(&poly)).unwrap_or(0)],
            "pack" => {
                let mut w = Writer::new();
                write_polygon(&mut w, &poly);
                let b = w.as_bytes();
                let n = u32::try_from(b.len()).unwrap_or(0);
                let mut out = vec![u32::try_from(polygon_size(&poly)).unwrap_or(0), n];
                out.extend(
                    b.chunks(4)
                        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])),
                );
                out
            }
            _ => return None,
        };
        Some(out)
    });
}
