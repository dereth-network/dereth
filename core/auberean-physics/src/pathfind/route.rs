//! A way found: where it starts and ends, the nodes it passes in order, and the doorway each link
//! passes (narrowed for the body), and turning it into positions to walk.

use std::fmt::Write as _;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::db::{PathMapDb, PathMaps};
use super::node::Spot;
use crate::geom;
use crate::geom::Ray;

/// A way found (copied whole, as the client copies it).
// D2-04484, D2-04532, D2-13534, D2-04523
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub start: Position,
    pub end: Position,
    pub dist: f32,
    pub ratio: f32,
    pub time: f64,
    /// The nodes, start first.
    pub nodes: Vec<u32>,
    /// The doorway of each link taken, in order.
    pub portals: Vec<Ray>,
}

fn null_position() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

impl Default for Path {
    fn default() -> Self {
        Self {
            start: null_position(),
            end: null_position(),
            dist: 0.0,
            ratio: 0.0,
            time: 0.0,
            nodes: Vec::new(),
            portals: Vec::new(),
        }
    }
}

impl Path {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Turns each position to face the next; the last faces the way the one before it faced to
    /// reach it.
    // D2-13423
    pub fn smooth_position_list(list: &mut [Position]) {
        let n = list.len();
        for i in 0..n {
            if i + 1 < n {
                let h = geom::heading_to(&list[i], &list[i + 1]);
                geom::set_heading(&mut list[i].frame, h);
            } else if n > 1 {
                let h = geom::heading_to(&list[i - 1], &list[i]);
                geom::set_heading(&mut list[i].frame, h);
            }
        }
    }

    /// The way as text: its node count and distance, then each node.
    // D2-13427, D2-13428
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        let _ = write!(
            s,
            "path: nodes: {} dist: {:.6}\nnode list:\n",
            self.nodes.len(),
            self.dist
        );
        for n in &self.nodes {
            let _ = writeln!(s, " 0x{n:08X}");
        }
        s
    }

    /// The positions to walk: each node's place, but in the middle of the doorway of the link
    /// taken out of it where there is one (the node's cell kept), and the way's own end in place
    /// of the end node. Each faces the next. None when a node's place cannot be found.
    pub fn to_position_list<S: PathMaps + ?Sized>(
        &self,
        db: &mut PathMapDb,
        source: &mut S,
    ) -> Option<Vec<Position>> {
        if self.nodes.is_empty() {
            return None;
        }
        let end_set = self.end.cell.0 != 0
            || self.end.frame.origin != Vec3::ZERO
            || self.end.frame.rotation != Quat::IDENTITY;
        let end_id = if end_set {
            db.node_id_at(source, &Spot::of(&self.end))
        } else {
            0
        };
        let mut portals = self.portals.iter();
        let mut out = Vec::with_capacity(self.nodes.len());
        for &id in &self.nodes {
            if end_id != 0 && id == end_id {
                out.push(self.end);
                continue;
            }
            let mut p = db.query_position(source, id)?.to_position();
            if let Some(r) = portals.next() {
                p.frame.origin = r.center_point();
            }
            out.push(p);
        }
        Self::smooth_position_list(&mut out);
        Some(out)
    }
}
