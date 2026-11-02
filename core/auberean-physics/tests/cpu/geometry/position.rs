use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::position_cases::CASES;
use super::{bits, check};
use auberean_physics::cells::{
    block_id_at, cell_distance_squared, in_bounds, in_same_block, interpolate, map_coords_to_cell,
    within_block,
};
use auberean_physics::geom::position::{
    adjust_position_to_outside, origin_in_block_of, outside_cell, pitch_to, position_heading,
    positions_differ, quadrant_of, set_position_heading, PositionOffset,
};
use auberean_physics::geom::Heading;

fn pos_bits(p: &Position) -> Vec<u32> {
    let (o, q) = (p.frame.origin, p.frame.rotation);
    [vec![p.cell.0], bits(&[o.x, o.y, o.z, q.w, q.x, q.y, q.z])].concat()
}

/// Every position and land operation answers as the September 2013 client's own code does, to
/// the bit: a position's heading and its turn to another, whether two differ, the land cell
/// outside one, another's origin in its landblock, the pitch toward another and the quadrant it
/// stands in, a position sent as a cell and offset; and whether land coordinates lie on the map,
/// whether two cells share a landblock, whether a point lies over its block, the block at block
/// coordinates, the land cell at map coordinates, two cells' squared distance and a value blended
/// across a block from its corners.
///
/// Behaviour: geometry.position.every-operation-answers-as-the-client-does
#[test]
fn every_position_operation_answers_as_the_client_does() {
    check(CASES, |label, x, inp| {
        let v = |i: usize| Vec3::new(x[i], x[i + 1], x[i + 2]);
        let pos = |i: usize| {
            Position::new(
                CellId(inp[i]),
                Frame::new(v(i + 1), Quat::new(x[i + 4], x[i + 5], x[i + 6], x[i + 7])),
            )
        };
        #[allow(clippy::cast_possible_wrap)]
        let int = |i: usize| inp[i] as i32;
        let out = match label {
            "heading" => bits(&[position_heading(&pos(0)).degrees()]),
            "set_heading" => {
                let mut p = pos(0);
                set_position_heading(&mut p, Heading::new(x[8]));
                pos_bits(&p)
            }
            "adjust_to_outside" => {
                let mut p = pos(0);
                adjust_position_to_outside(&mut p);
                pos_bits(&p)[..4].to_vec()
            }
            "outside_cell" => vec![outside_cell(&pos(0)).0],
            "offset_from" => {
                let o = PositionOffset::from_position(&pos(0));
                [vec![o.cell.0], bits(&[o.offset.x, o.offset.y, o.offset.z])].concat()
            }
            "offset_to" | "position_from_offset" => {
                let o = PositionOffset {
                    cell: CellId(inp[0]),
                    offset: v(1),
                };
                pos_bits(&o.to_position())
            }
            "differ" => vec![u32::from(positions_differ(&pos(0), &pos(8)))],
            "origin_in_block_of" => {
                let g = origin_in_block_of(&pos(0), &pos(8));
                bits(&[g.x, g.y, g.z])
            }
            "pitch_to" => bits(&[pitch_to(&pos(0), &pos(8))]),
            "quadrant" => vec![quadrant_of(&pos(0), x[8], &pos(9))],
            "in_bounds" => vec![u32::from(in_bounds(int(0), int(1)))],
            "block_id_at" => vec![block_id_at(int(0), int(1))],
            "in_same_block" => vec![u32::from(in_same_block(CellId(inp[0]), CellId(inp[1])))],
            "cell_distance_squared" => {
                bits(&[cell_distance_squared(CellId(inp[0]), CellId(inp[1]))])
            }
            "within_block" => vec![u32::from(within_block(v(0)))],
            "interpolate" => bits(&[interpolate(v(0), x[3], x[4], x[5], x[6])]),
            "map_coords_to_cell" => match map_coords_to_cell(x[0], x[1]) {
                Some(c) => vec![1, c.0],
                None => vec![0, 0],
            },
            _ => return None,
        };
        Some(out)
    });
}
