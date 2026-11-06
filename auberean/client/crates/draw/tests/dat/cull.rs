//! What a frame seen from inside shows at the moment, walked through the doorways from the
//! camera's cell: the cave of block 0x6967, whose first room has a doorway out to the land, and the
//! doorways of every cell of the September 2013 cell file.

use auberean_assets::mesh::CellMesh;
use auberean_draw::cull::Sight;
use auberean_draw::scene::{Cull, Frame, SceneBuilder, Spot};
use auberean_primitives::Vec3;

use crate::common::install;

/// The cave's first room seen from `at` looking along `heading`, `pitch` above the horizon, and
/// what it shows at the moment in a picture four thirds as wide as it is high.
fn cave(builder: &mut SceneBuilder<'_>, at: Vec3, heading: f32, pitch: f32) -> (Frame, Sight) {
    let block = (0x69, 0x67);
    let spot = Spot {
        block,
        position: at,
        heading,
        pitch,
        time_of_day: 0.5,
        day: None,
    };
    let (f, _) = builder.inside_frame(&spot, 60.0, block, 0x0100);
    let rooms = f.rooms.clone().expect("the rooms about the camera");
    let sight = rooms.sight(&f.camera, 4.0 / 3.0);
    (f, sight)
}

/// From the cave's first room the land outside is drawn only while the doorway out shows: facing
/// it, the walk finds a view out through it, and the land, its sky and the objects outdoors within
/// that view are drawn (not those outside it); facing into the cave, no view out is found and none
/// of them is. The room the camera stands in is drawn either way.
///
/// Behaviour: draw.inside.the-land-shows-only-through-a-doorway-in-view
#[test]
fn the_land_shows_only_through_a_doorway_in_view() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    let at = Vec3::new(109.75, 112.97, 100.8);
    let (f, toward) = cave(&mut builder, at, 180.0, 0.0);
    let outdoor: Vec<_> = f
        .objects
        .iter()
        .filter(|b| matches!(b.cull, Cull::Box { .. }))
        .collect();
    let shown = outdoor.iter().filter(|b| toward.shows(b)).count();
    println!(
        "facing the doorway: {} views out, {shown} of {} outdoor draws, cells {:?}",
        toward.outside.len(),
        outdoor.len(),
        toward.drawn
    );
    assert!(
        toward.outdoors() && toward.sky(),
        "the land through the doorway"
    );
    assert!(
        shown > 0 && shown < outdoor.len(),
        "{shown} of {}",
        outdoor.len()
    );
    assert!(toward.drawn.contains(&0x0100), "the camera's room");
    let (_, away) = cave(&mut builder, at, 0.0, 0.0);
    println!(
        "facing into the cave: {} views out, cells {:?}",
        away.outside.len(),
        away.drawn
    );
    assert!(!away.outdoors() && !away.sky(), "no land behind the camera");
    assert!(outdoor.iter().all(|b| !away.shows(b)));
    assert!(away.drawn.contains(&0x0100), "the camera's room");
}

/// Every doorway of every interior cell of the cell file is stored with its polygon running
/// clockwise as seen from inside its cell (the turn of its corners points out of the cell), and
/// every one names side 1: the view through a doorway, made from its corners in their order, holds
/// what lies beyond it, as the walk needs, with no doorway turned round.
///
/// Behaviour: draw.inside.every-doorway-runs-clockwise-from-inside-its-cell
#[test]
fn every_doorway_runs_clockwise_from_inside_its_cell() {
    let r = &install().records;
    let (mut out, mut other, mut sides) = (0, Vec::new(), 0);
    for (x, y) in r.landblock_coordinates() {
        let Some(info) = r.landblock_info(x, y) else {
            continue;
        };
        for low in 0x100..0x100 + info.cell_count {
            let low = u16::try_from(low).expect("a cell number");
            let Some(c) = r.cell(x, y, low) else {
                continue;
            };
            let Some(m) = r.get::<CellMesh>(c.mesh.0) else {
                continue;
            };
            let v = &m.mesh.vertices;
            let stride = auberean_assets::mesh::vertex_size(v.format);
            let f = |b: &[u8], k: usize| f32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]);
            let at: Vec<[f32; 3]> = v
                .bytes
                .chunks_exact(stride)
                .map(|b| [f(b, 0), f(b, 4), f(b, 8)])
                .collect();
            let b = v.bounds;
            let middle = [
                (b.min.x + b.max.x) / 2.0,
                (b.min.y + b.max.y) / 2.0,
                (b.min.z + b.max.z) / 2.0,
            ];
            for p in &c.portals {
                let Some(mp) = m.portals.get(usize::from(p.portal)) else {
                    continue;
                };
                sides += usize::from(mp.side != 1);
                let Some(poly) = m.mesh.physics_polygons.get(usize::from(mp.polygon)) else {
                    continue;
                };
                let pts: Vec<[f32; 3]> = poly
                    .vertices
                    .iter()
                    .filter_map(|&k| at.get(usize::from(k)).copied())
                    .collect();
                let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                let cross = |a: [f32; 3], b: [f32; 3]| {
                    [
                        a[1] * b[2] - a[2] * b[1],
                        a[2] * b[0] - a[0] * b[2],
                        a[0] * b[1] - a[1] * b[0],
                    ]
                };
                let mut turn = [0.0f32; 3];
                let mut sum = [0.0f32; 3];
                for (k, q) in pts.iter().enumerate() {
                    sum = [sum[0] + q[0], sum[1] + q[1], sum[2] + q[2]];
                    if k + 2 < pts.len() {
                        let t = cross(sub(pts[k + 1], pts[0]), sub(pts[k + 2], pts[0]));
                        turn = [turn[0] + t[0], turn[1] + t[1], turn[2] + t[2]];
                    }
                }
                #[allow(clippy::cast_precision_loss)]
                let n = pts.len().max(1) as f32;
                let away = sub([sum[0] / n, sum[1] / n, sum[2] / n], middle);
                if turn[0] * away[0] + turn[1] * away[1] + turn[2] * away[2] > 0.0 {
                    out += 1;
                } else {
                    other.push((x, y, low, p.portal));
                }
            }
        }
    }
    println!("{out} doorways turn out of their cells");
    assert!(out > 170_000, "{out}");
    assert!(other.is_empty(), "{} turn inward: {other:X?}", other.len());
    assert_eq!(sides, 0);
}

/// On the land before a cave's mouth (block `0x6967`), the rooms show through the mouth while it is
/// in view: facing it the first room is drawn, and what is drawn outdoors is too; with its back to
/// it no room is, and everything else on the land still is.
///
/// Behaviour: draw.inside.a-caves-rooms-show-through-its-mouth
#[test]
fn the_rooms_of_a_cave_show_through_its_mouth() {
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let block = (0x69, 0x67);
    let spot = |heading: f32| Spot {
        block,
        position: Vec3::new(109.75, 92.0, 101.0),
        heading,
        pitch: 0.0,
        time_of_day: 0.5,
        day: None,
    };
    let looked = |f: &Frame| {
        let sight = f.sight(4.0 / 3.0).expect("rooms to look into");
        let drawn = f
            .objects
            .iter()
            .filter(|b| {
                matches!(b.cull, Cull::Looked { .. } | Cull::LookedIn { .. }) && sight.shows(b)
            })
            .count();
        let land = f
            .objects
            .iter()
            .filter(|b| matches!(b.cull, Cull::Box { .. }))
            .all(|b| sight.shows(b));
        (drawn, land)
    };
    let mut seen = Vec::new();
    for heading in (0..360).step_by(45) {
        #[allow(clippy::cast_precision_loss)]
        let f = builder.frame(&spot(heading as f32), 60.0);
        let (drawn, land) = looked(&f);
        assert!(land, "everything on the land stays drawn");
        seen.push(drawn);
    }
    println!("rooms' draws by heading, every 45 degrees: {seen:?}");
    assert!(seen.iter().any(|&d| d > 0), "the mouth in view: {seen:?}");
    assert!(seen.contains(&0), "the mouth out of view: {seen:?}");
}
