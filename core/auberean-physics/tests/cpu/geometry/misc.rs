use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::misc_cases::CASES;
use super::{bits, check};
use auberean_physics::geom::position::set_from_map_coords;
use auberean_physics::geom::{unit_vector_to_u24, CylSphere, CylinderOps};

/// A cylinder scaled, a direction packed into three bytes and a position moved to map
/// coordinates answer as the September 2013 client's own code does, to the bit.
///
/// Behaviour: geometry.misc.cylinders-directions-and-map-positions-answer-as-the-client-does
#[test]
fn cylinders_directions_and_map_positions_answer_as_the_client_does() {
    check(CASES, |label, x, _| {
        let v = |i: usize| Vec3::new(x[i], x[i + 1], x[i + 2]);
        let out = match label {
            "cylinder_scale" => {
                let mut c = CylSphere {
                    low_pt: v(0),
                    height: x[3],
                    radius: x[4],
                };
                c.scale(v(5));
                bits(&[c.low_pt.x, c.low_pt.y, c.low_pt.z, c.height, c.radius])
            }
            "unit_vector_to_u24" => vec![unit_vector_to_u24(v(0))],
            "from_map_coords" => {
                let mut p = Position::new(
                    CellId(7),
                    Frame::new(Vec3::new(9.0, 9.0, 9.0), Quat::IDENTITY),
                );
                let ok = set_from_map_coords(&mut p, x[0], x[1]);
                let (o, q) = (p.frame.origin, p.frame.rotation);
                [
                    vec![u32::from(ok), p.cell.0],
                    bits(&[o.x, o.y, o.z, q.w, q.x, q.y, q.z]),
                ]
                .concat()
            }
            _ => return None,
        };
        Some(out)
    });
}
