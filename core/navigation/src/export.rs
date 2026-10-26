//! Pictures and records of a region and a route, for looking at.

use std::fmt::Write as _;

use dereth_primitives::num::floor_to_i32;
use dereth_primitives::Position;

use crate::path::{NavPath, WaypointAction};
use crate::region::Region;

/// The most grid columns a picture draws across; a larger area is drawn coarser.
const PICTURE_COLUMNS: i32 = 800;

/// A top-down picture of a region: clear places shaded by height, places blocked by static
/// objects in orange, other blocked places in grey, the route in red and the places a body
/// actually stood (`trail`) in blue. With a route, the picture is of the route and 8 m round it;
/// without one, of the whole region, drawn coarser where it is large.
#[must_use]
pub fn svg(region: &Region, path: Option<&NavPath>, trail: &[Position]) -> String {
    let s = region.spacing();
    // The area drawn, in grid columns.
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    let mut grow = |x: f32, y: f32| {
        bounds = Some(bounds.map_or((x, y, x, y), |(a, b, c, d)| {
            (a.min(x), b.min(y), c.max(x), d.max(y))
        }));
    };
    if let Some(path) = path {
        for p in path
            .legs
            .iter()
            .flatten()
            .chain(path.waypoints.iter().map(|w| &w.position))
        {
            let q = region.to_region(p);
            grow(q.x - 8.0, q.y - 8.0);
            grow(q.x + 8.0, q.y + 8.0);
        }
    } else {
        for id in region.places().take(region.report().nodes) {
            let q = region.node_point(id);
            grow(q.x, q.y);
        }
        if let Some((x0, y0, x1, y1)) = region.window() {
            #[allow(clippy::cast_precision_loss)]
            {
                let o = region.to_region(&Position::new(
                    dereth_primitives::CellId((u32::from(region.origin_block().0) << 16) | 1),
                    dereth_primitives::Frame::default(),
                ));
                grow(o.x, o.y);
                grow(
                    o.x + (x1 - x0 + 1) as f32 * 192.0,
                    o.y + (y1 - y0 + 1) as f32 * 192.0,
                );
            }
        }
    }
    let Some((bx0, by0, bx1, by1)) = bounds else {
        return String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    };
    let (gx0, gy0) = (floor_to_i32(bx0 / s), floor_to_i32(by0 / s));
    let (gx1, gy1) = (floor_to_i32(bx1 / s) + 1, floor_to_i32(by1 / s) + 1);
    let stride = ((gx1 - gx0).max(gy1 - gy0) / PICTURE_COLUMNS + 1).max(1);
    #[allow(clippy::cast_precision_loss)]
    let cell = 8.0 / stride as f32;
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (
        ((gx1 - gx0) / stride + 1) as f32 * cell,
        ((gy1 - gy0) / stride + 1) as f32 * cell,
    );
    let px = cell / (s * stride as f32);
    // North up: y grows upward in the world, downward on the page.
    let tx = |x: f32| (x - bx0) * px;
    let ty = |y: f32| (by1 - y) * px;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" \
         style=\"background:#111\">"
    );
    // Height range, for the shading.
    let (mut z0, mut z1) = (f32::MAX, f32::MIN);
    let mut cells: Vec<(f32, f32, f32, u8)> = Vec::new();
    let mut gy = gy0;
    while gy <= gy1 {
        let mut gx = gx0;
        while gx <= gx1 {
            // The highest place of the column decides its colour.
            let mut best: Option<(f32, u8)> = None;
            for id in region.column(gx, gy) {
                let p = region.place(id);
                let kind = if p.clear {
                    0
                } else if p.blocked_by_static {
                    1
                } else {
                    2
                };
                if best.is_none_or(|(z, _)| p.z > z) {
                    best = Some((p.z, kind));
                }
            }
            if let Some((z, kind)) = best {
                if kind == 0 {
                    z0 = z0.min(z);
                    z1 = z1.max(z);
                }
                #[allow(clippy::cast_precision_loss)]
                cells.push((gx as f32 * s, gy as f32 * s, z, kind));
            }
            gx += stride;
        }
        gy += stride;
    }
    let span = (z1 - z0).max(1.0);
    for (x, y, z, kind) in cells {
        let fill = match kind {
            0 => {
                let t = (z - z0) / span;
                let g = 60.0 + 180.0 * t;
                format!("rgb({:.0},{:.0},{:.0})", g * 0.6, g, g * 0.6)
            }
            1 => String::from("#d98c1f"),
            _ => String::from("#444"),
        };
        let _ = writeln!(
            out,
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{cell:.1}\" height=\"{cell:.1}\" fill=\"{fill}\"/>",
            tx(x) - cell * 0.5,
            ty(y) - cell * 0.5,
        );
    }
    let line = |out: &mut String, pts: &[Position], colour: &str, width: u32| {
        let v: Vec<String> = pts
            .iter()
            .map(|p| {
                let q = region.to_region(p);
                format!("{:.1},{:.1}", tx(q.x), ty(q.y))
            })
            .collect();
        let _ = writeln!(
            out,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{colour}\" stroke-width=\"{width}\"/>",
            v.join(" ")
        );
    };
    if !trail.is_empty() {
        line(&mut out, trail, "#39f", 3);
    }
    if let Some(path) = path {
        let pts: Vec<Position> = path.waypoints.iter().map(|w| w.position).collect();
        line(&mut out, &pts, "#f33", 2);
        for wp in &path.waypoints {
            let q = region.to_region(&wp.position);
            let _ = writeln!(
                out,
                "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" fill=\"{}\"/>",
                tx(q.x),
                ty(q.y),
                if wp.action.is_some() { "#ff0" } else { "#f33" }
            );
        }
    }
    out.push_str("</svg>\n");
    out
}

/// A route as JSON: its waypoints (cell, x, y, z, action), length and goal offset.
#[must_use]
pub fn path_json(path: &NavPath) -> String {
    let mut out = String::from("{\"waypoints\":[");
    for (i, w) in path.waypoints.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let o = w.position.frame.origin;
        let action = match w.action {
            Some(WaypointAction::OpenDoor(id)) => format!("{{\"open_door\":{}}}", id.0),
            None => String::from("null"),
        };
        let _ = write!(
            out,
            "{{\"cell\":\"0x{:08X}\",\"x\":{:.3},\"y\":{:.3},\"z\":{:.3},\"action\":{action}}}",
            w.position.cell.0, o.x, o.y, o.z
        );
    }
    let _ = write!(
        out,
        "],\"length\":{:.3},\"goal_offset\":{:.3}}}",
        path.length(),
        path.goal_offset
    );
    out
}

/// A region's cells and counts as JSON.
#[must_use]
pub fn region_json(region: &Region) -> String {
    let r = region.report();
    let cells: Vec<String> = region
        .cells()
        .map(|c| format!("\"0x{:08X}\"", c.0))
        .collect();
    format!(
        "{{\"cells\":[{}],\"landblocks\":{},\"buildings\":{},\"floors\":{},\"solids\":{},\
         \"statics\":{},\"nodes\":{},\"clear_nodes\":{},\"static_blocked_nodes\":{},\
         \"ground\":{},\"clear_ground\":{},\"steep_ground\":{},\"water_ground\":{},\
         \"spacing\":{},\"bytes\":{}}}",
        cells.join(","),
        r.landblocks,
        r.buildings,
        r.floors,
        r.solids,
        r.statics,
        r.nodes,
        r.clear_nodes,
        r.static_blocked_nodes,
        r.ground,
        r.clear_ground,
        r.steep_ground,
        r.water_ground,
        region.spacing(),
        region.memory_bytes()
    )
}
