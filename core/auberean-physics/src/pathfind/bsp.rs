//! The tree a path map finds a node by: each cell of the map has a tree of planes whose leaves
//! name the node whose patch of floor lies there. A point on a plane (within 0.0002) belongs to
//! its positive side.

use std::cmp::Ordering;
use std::collections::HashMap;

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::Vec3;

use super::map::PathMap;
use crate::geom::{side_of, Plane, PlaneOps, Polygon, Ray, Side};
use crate::geom::{EPS, EPS_SQ};
use crate::pack::{write_vec3, Writer};

/// A part of a tree: a split, or a leaf naming a node.
#[derive(Clone, Debug, PartialEq)]
pub enum Bsp {
    Node(Box<BspNode>),
    Leaf(BspLeaf),
}

/// A leaf: the node whose floor is here.
// D2-13571, D2-13576
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BspLeaf {
    pub node_id: u32,
}

/// Which children are splits rather than leaves.
pub mod bits {
    pub const POS_IS_NODE: u32 = 1;
    pub const NEG_IS_NODE: u32 = 2;
}

/// A split: a plane, the part on its positive side and the part on its negative side.
// D2-13597, D2-13637
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BspNode {
    pub bits: u32,
    pub plane: Plane,
    pub pos: Option<Bsp>,
    pub neg: Option<Bsp>,
}

impl BspLeaf {
    /// Forgets its node.
    // D2-13577
    pub fn destroy(&mut self) {
        self.node_id = 0;
    }

    // D2-13583
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { node_id: r.u32()? })
    }

    /// Numbers the leaves in order: this one takes the counter, which moves on.
    // D2-18965
    pub fn label_all_leaves(&mut self, counter: &mut u32) -> bool {
        self.node_id = *counter;
        *counter = counter.wrapping_add(1);
        true
    }

    /// Renames its node by the table; a node not in it fails and keeps its name.
    // D2-18969
    pub fn update_node_ids(&mut self, renames: &HashMap<u32, u32>) -> bool {
        match renames.get(&self.node_id) {
            Some(&id) => {
                self.node_id = id;
                true
            }
            None => false,
        }
    }

    /// Looks up its node's floor patch and does nothing with it (the client's check of a leaf
    /// against the cell's planes, and against the planes above it, compiled to this).
    // D2-18966
    pub fn check_cell_planes(&self, map: &PathMap) {
        let _ = map.node_polygon(self.node_id);
    }

    /// A leaf has nothing below it to remove.
    #[must_use]
    pub fn remove_leaf(&self) -> bool {
        false
    }

    /// A segment through the tree reaches this leaf: its node is listed.
    pub fn split_by_segment(&self, out: &mut Vec<u32>) -> bool {
        out.push(self.node_id);
        true
    }

    /// A volume through the tree reaches this leaf: its node is listed.
    pub fn calc_nodes_in_volume(&self, out: &mut Vec<u32>) -> bool {
        out.push(self.node_id);
        true
    }

    /// A line starting at a corner of its node's floor patch and running along one of that
    /// corner's edges: the edge, from the line's start (at the corner's height) to across from
    /// the neighbouring corner (rising to its height), is listed with the node. A line starting
    /// elsewhere (more than 0.0002 across the ground from every corner), along neither edge, or
    /// whose end is nearer the corner than the neighbour is lists nothing. Of two neighbours on
    /// the line, the one nearer its end is taken. Fails without the node or its patch.
    // D2-18968
    pub fn find_node_list(&self, line: &Ray, out: &mut Vec<NodeEdge>, map: &PathMap) -> bool {
        if map.node(self.node_id).is_none() {
            return false;
        }
        let Some(poly) = map.node_polygon(self.node_id) else {
            return false;
        };
        let n = poly.len();
        let a = line.point;
        let b = line.end_point();
        let (mut at, mut nearest) = (0usize, f32::MAX);
        for (i, v) in poly.iter().enumerate() {
            let (dx, dy) = (v.x - a.x, v.y - a.y);
            let d = dx * dx + dy * dy;
            if d < nearest {
                at = i;
                nearest = d;
            }
        }
        let (prev, next) = if at != 0 {
            (at - 1, if at == n - 1 { 0 } else { at + 1 })
        } else {
            (n.wrapping_sub(1), 1)
        };
        if nearest > EPS_SQ {
            return true;
        }
        let (Some(&v), Some(&vp), Some(&vn)) = (poly.get(at), poly.get(prev), poly.get(next))
        else {
            return true;
        };
        let plane = Plane::vertical(a, b);
        let side = |p: Vec3| {
            let d = plane.normal.x * p.x + plane.normal.z * p.z + plane.normal.y * p.y + plane.d;
            side_of(f64::from(d))
        };
        let (sp, sn) = (side(vp), side(vn));
        if sp != Side::InPlane && sn != Side::InPlane {
            return true;
        }
        let far = |p: Vec3| {
            let (dx, dy) = (p.x - b.x, p.y - b.y);
            dx * dx + dy * dy
        };
        let (dp, dn) = (far(vp), far(vn));
        let (c, dc) = match (sp == Side::InPlane, sn == Side::InPlane) {
            (true, true)
                if !matches!(dp.partial_cmp(&dn), Some(Ordering::Less | Ordering::Equal)) =>
            {
                (vn, dn)
            }
            (true, _) => (vp, dp),
            _ => (vn, dn),
        };
        if far(v) < dc {
            return true;
        }
        let d = line.dir;
        let t = d.z * c.z + d.x * c.x + d.y * c.y - (a.x * d.x + a.y * d.y + a.z * d.z);
        let start = Vec3::new(a.x, a.y, v.z);
        let edge = Ray::from_offset(
            start,
            Vec3::new((a.x + t * d.x) - a.x, (a.y + t * d.y) - a.y, c.z - v.z),
        );
        out.push(NodeEdge {
            edge,
            node: Some(self.node_id),
        });
        true
    }

    /// The floor patch cut down to this leaf: kept as its node's, or (`check`) compared with the
    /// one kept, the answer unused.
    pub fn calc_node_polygons(&self, poly: &Polygon, map: &mut PathMap, check: bool) -> bool {
        if check {
            let _ = map.check_node_polygon(poly, self.node_id);
        } else {
            let _ = map.set_node_polygon(poly.clone(), self.node_id);
        }
        true
    }
}

/// An edge of a node's floor patch a line crosses, and the node.
// D2-13582
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct NodeEdge {
    pub edge: Ray,
    pub node: Option<u32>,
}

impl Bsp {
    /// The node whose floor holds `p`.
    // D2-18963, D2-13608
    #[must_use]
    pub fn find_node_id(&self, p: Vec3) -> u32 {
        match self {
            Bsp::Leaf(l) => l.node_id,
            Bsp::Node(n) => n.find_node_id(p),
        }
    }

    /// A leaf's node; a split names none.
    #[must_use]
    pub fn node_id(&self) -> u32 {
        match self {
            Bsp::Leaf(l) => l.node_id,
            Bsp::Node(_) => 0,
        }
    }

    #[must_use]
    pub fn is_leaf(&self) -> bool {
        matches!(self, Bsp::Leaf(_))
    }

    /// Copies this part's shape onto `into` (a copy of a leaf takes only its node).
    // D2-18964, D2-13600
    pub fn copy_to(&self, into: &mut Bsp) -> bool {
        match (self, into) {
            (Bsp::Leaf(a), Bsp::Leaf(b)) => {
                b.node_id = a.node_id;
                true
            }
            (Bsp::Node(a), Bsp::Node(b)) => a.copy_to(b),
            _ => true,
        }
    }

    /// Clears it: a leaf forgets its node, a split drops both children.
    pub fn destroy(&mut self) {
        match self {
            Bsp::Leaf(l) => l.destroy(),
            Bsp::Node(n) => n.destroy(),
        }
    }

    /// The part's own layout: a leaf's node, or a split's (see [`BspNode::read`]).
    pub fn read(r: &mut Reader<'_>, leaf: bool) -> Result<Self> {
        Ok(if leaf {
            Bsp::Leaf(BspLeaf::read(r)?)
        } else {
            Bsp::Node(Box::new(BspNode::read(r)?))
        })
    }

    pub fn write(&self, w: &mut Writer) -> bool {
        match self {
            Bsp::Leaf(l) => {
                w.u32(l.node_id);
                true
            }
            Bsp::Node(n) => n.write(w),
        }
    }

    /// Numbers the leaves from `counter` on, positive side first.
    pub fn label_all_leaves(&mut self, counter: &mut u32) -> bool {
        match self {
            Bsp::Leaf(l) => l.label_all_leaves(counter),
            Bsp::Node(n) => n.label_all_leaves(counter),
        }
    }

    /// Renames every leaf's node by the table.
    pub fn update_node_ids(&mut self, renames: &HashMap<u32, u32>) -> bool {
        match self {
            Bsp::Leaf(l) => l.update_node_ids(renames),
            Bsp::Node(n) => n.update_node_ids(renames),
        }
    }

    /// The leaves' floor patches looked up.
    pub fn check_cell_planes(&self, map: &PathMap) {
        match self {
            Bsp::Leaf(l) => l.check_cell_planes(map),
            Bsp::Node(n) => n.check_cell_planes(map),
        }
    }

    /// The leaves' floor patches looked up, each side with its planes.
    pub fn check_nodes_vs_planes(&self, map: &PathMap, planes: &[Plane]) {
        match self {
            Bsp::Leaf(l) => l.check_cell_planes(map),
            Bsp::Node(n) => n.check_nodes_vs_planes(map, planes),
        }
    }

    /// The planes bounding the leaf that holds `p`, each facing it, root first.
    pub fn get_planes(&self, p: Vec3, out: &mut Vec<Plane>) {
        if let Bsp::Node(n) = self {
            n.get_planes(p, out);
        }
    }
}

impl BspNode {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops both children.
    // D2-13599
    pub fn destroy(&mut self) {
        self.pos = None;
        self.neg = None;
    }

    /// Lets go of both children without dropping them: they are handed back.
    pub fn clear_children_without_deleting(&mut self) -> (Option<Bsp>, Option<Bsp>) {
        (self.pos.take(), self.neg.take())
    }

    // D2-13601
    pub fn set_plane(&mut self, plane: Plane) {
        self.plane = plane;
    }

    /// A new split on the positive side.
    // D2-13602
    pub fn make_pos_node(&mut self) -> &mut BspNode {
        self.bits |= bits::POS_IS_NODE;
        self.pos = Some(Bsp::Node(Box::default()));
        match self.pos.as_mut() {
            Some(Bsp::Node(n)) => n,
            _ => unreachable!(),
        }
    }

    /// A new split on the negative side.
    // D2-13603
    pub fn make_neg_node(&mut self) -> &mut BspNode {
        self.bits |= bits::NEG_IS_NODE;
        self.neg = Some(Bsp::Node(Box::default()));
        match self.neg.as_mut() {
            Some(Bsp::Node(n)) => n,
            _ => unreachable!(),
        }
    }

    /// A new leaf naming `node_id` on the positive side.
    // D2-13604
    pub fn make_pos_leaf(&mut self, node_id: u32) {
        self.pos = Some(Bsp::Leaf(BspLeaf { node_id }));
        self.bits &= !bits::POS_IS_NODE;
    }

    /// A new leaf naming `node_id` on the negative side.
    // D2-13605
    pub fn make_neg_leaf(&mut self, node_id: u32) {
        self.neg = Some(Bsp::Leaf(BspLeaf { node_id }));
        self.bits &= !bits::NEG_IS_NODE;
    }

    /// Marks a side's child as a split (`pos` true: the positive side).
    pub fn update_leaf_to_node(&mut self, pos: bool) -> bool {
        self.bits |= if pos {
            bits::POS_IS_NODE
        } else {
            bits::NEG_IS_NODE
        };
        true
    }

    // D2-18970
    #[must_use]
    pub fn pos_is_leaf(&self) -> bool {
        self.bits & bits::POS_IS_NODE == 0
    }

    // D2-13616
    #[must_use]
    pub fn neg_is_leaf(&self) -> bool {
        self.bits & bits::NEG_IS_NODE == 0
    }

    /// The side of the plane `p` is on: the negative side only past 0.0002 below it.
    // D2-13608
    #[must_use]
    pub fn find_node_id(&self, p: Vec3) -> u32 {
        let child = if self.plane.dist_wide(p) < -f64::from(EPS) {
            &self.neg
        } else {
            &self.pos
        };
        child.as_ref().map_or(0, |c| c.find_node_id(p))
    }

    /// The side a point is cut to: off the plane by its own side; on a level plane (within
    /// 0.0002 of facing straight up or down), by the side of the point 0.1 m above it; else the
    /// positive side.
    #[must_use]
    pub fn get_split_side(&self, p: Vec3) -> Side {
        let n = self.plane.normal;
        let d = p.x * n.x + n.z * p.z + n.y * p.y + self.plane.d;
        if EPS < d {
            return Side::Positive;
        }
        if -EPS <= d {
            if 1.0 - n.z.abs() < EPS {
                let above = Vec3::new(p.x, p.y, p.z + 0.1);
                let s = self.plane.which_side(above, EPS);
                if s != Side::InPlane {
                    return s;
                }
            }
            return Side::Positive;
        }
        Side::Negative
    }

    /// Copies the shape below this split onto `into`: the plane, then each side as a split (copied
    /// in turn) or a leaf with the same node.
    // D2-13600
    pub fn copy_to(&self, into: &mut BspNode) -> bool {
        into.set_plane(self.plane);
        let mut ok = true;
        if self.pos_is_leaf() {
            into.make_pos_leaf(self.pos.as_ref().map_or(0, Bsp::node_id));
        } else {
            let to = into.make_pos_node();
            ok = match &self.pos {
                Some(Bsp::Node(n)) => n.copy_to(to),
                _ => true,
            };
        }
        if self.neg_is_leaf() {
            into.make_neg_leaf(self.neg.as_ref().map_or(0, Bsp::node_id));
            return ok;
        }
        let to = into.make_neg_node();
        let neg = match &self.neg {
            Some(Bsp::Node(n)) => n.copy_to(to),
            _ => true,
        };
        ok & neg
    }

    /// Takes `child` as its positive (`pos`) or negative child, noting whether it is a split.
    pub fn set_as_child(&mut self, child: Option<Bsp>, pos: bool) -> bool {
        let Some(child) = child else {
            return false;
        };
        let leaf = child.is_leaf();
        if pos {
            self.pos = Some(child);
            if leaf {
                self.bits &= !bits::POS_IS_NODE;
            } else {
                self.bits |= bits::POS_IS_NODE;
            }
        } else {
            self.neg = Some(child);
            if leaf {
                self.bits &= !bits::NEG_IS_NODE;
            } else {
                self.bits |= bits::NEG_IS_NODE;
            }
        }
        true
    }

    /// Numbers the leaves below, positive side first; a split missing a child fails.
    // D2-13614
    pub fn label_all_leaves(&mut self, counter: &mut u32) -> bool {
        match (self.pos.as_mut(), self.neg.as_mut()) {
            (Some(p), Some(n)) => {
                let a = p.label_all_leaves(counter);
                let b = n.label_all_leaves(counter);
                a & b
            }
            _ => false,
        }
    }

    /// Renames the leaves' nodes by the table; every leaf is tried.
    // D2-13623
    pub fn update_node_ids(&mut self, renames: &HashMap<u32, u32>) -> bool {
        let a = self
            .pos
            .as_mut()
            .is_some_and(|c| c.update_node_ids(renames));
        let b = self
            .neg
            .as_mut()
            .is_some_and(|c| c.update_node_ids(renames));
        a & b
    }

    /// The planes bounding the leaf that holds `p`, each facing it, root first.
    // D2-13640
    pub fn get_planes(&self, p: Vec3, out: &mut Vec<Plane>) {
        let d = f64::from(self.plane.normal.x) * f64::from(p.x)
            + f64::from(self.plane.normal.y) * f64::from(p.y)
            + f64::from(self.plane.normal.z) * f64::from(p.z)
            + f64::from(self.plane.d);
        if side_of(d) == Side::Negative {
            out.push(Plane {
                normal: Vec3::new(
                    -self.plane.normal.x,
                    -self.plane.normal.y,
                    -self.plane.normal.z,
                ),
                d: -self.plane.d,
            });
            if let Some(c) = &self.neg {
                c.get_planes(p, out);
            }
            return;
        }
        out.push(self.plane);
        if let Some(c) = &self.pos {
            c.get_planes(p, out);
        }
    }

    /// The leaves' floor patches looked up (see [`BspLeaf::check_cell_planes`]); a split missing
    /// a child looks at neither.
    // D2-13624
    pub fn check_cell_planes(&self, map: &PathMap) {
        if let (Some(p), Some(n)) = (&self.pos, &self.neg) {
            p.check_cell_planes(map);
            n.check_cell_planes(map);
        }
    }

    /// Each side handed the planes above it plus this one, facing that side; the leaves only look
    /// their patches up.
    // D2-13639
    pub fn check_nodes_vs_planes(&self, map: &PathMap, planes: &[Plane]) {
        let mut pos = planes.to_vec();
        let mut neg = planes.to_vec();
        pos.push(self.plane);
        neg.push(Plane {
            normal: Vec3::new(
                -self.plane.normal.x,
                -self.plane.normal.y,
                -self.plane.normal.z,
            ),
            d: -self.plane.d,
        });
        if let (Some(p), Some(n)) = (&self.pos, &self.neg) {
            p.check_nodes_vs_planes(map, &pos);
            n.check_nodes_vs_planes(map, &neg);
        }
    }

    /// The upright plane through a segment, its normal the segment's direction turned a quarter
    /// clockwise seen from above; a segment rising straight up (or of no direction) makes none.
    // D2-13613
    pub fn make_plane_from_segment(&mut self, seg: &Ray) -> bool {
        let v = Vec3::new(seg.dir.y, -seg.dir.x, 0.0);
        if v.x.abs() < EPS && v.y.abs() < EPS {
            return false;
        }
        let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        let k = 1.0 / len;
        let n = Vec3::new(v.x * k, v.y * k, v.z * k);
        self.plane = Plane {
            normal: n,
            d: -((n.z * seg.point.z + n.y * seg.point.y) + n.x * seg.point.x),
        };
        true
    }

    /// Which children are splits, each child (a split or a leaf by those bits), then the plane.
    // D2-13630
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mut n = Self::new();
        n.bits = r.u32()?;
        n.pos = Some(Bsp::read(r, n.pos_is_leaf())?);
        n.neg = Some(Bsp::read(r, n.neg_is_leaf())?);
        n.plane = Plane {
            normal: crate::pack::read_vec3(r)?,
            d: r.f32()?,
        };
        Ok(n)
    }

    pub fn write(&self, w: &mut Writer) -> bool {
        w.u32(self.bits);
        let (Some(p), Some(n)) = (&self.pos, &self.neg) else {
            return false;
        };
        if !p.write(w) || !n.write(w) {
            return false;
        }
        write_vec3(w, self.plane.normal);
        w.f32(self.plane.d);
        true
    }
}

/// One cell's tree.
// D2-13568
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BspTree {
    pub id: u32,
    /// Whether the whole tree is one leaf.
    pub is_leaf: bool,
    pub root: Option<Bsp>,
}

impl BspTree {
    /// The node whose floor holds `p` (0 for an empty tree).
    // D2-13565
    #[must_use]
    pub fn find_node_id(&self, p: Vec3) -> u32 {
        self.root.as_ref().map_or(0, |r| r.find_node_id(p))
    }

    /// Hands over the root, leaving the tree empty.
    pub fn take_root(&mut self) -> Option<Bsp> {
        self.root.take()
    }

    /// The id, whether the root is a leaf, then the root.
    // D2-13569
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u32()?;
        let is_leaf = r.u32()? != 0;
        let root = Bsp::read(r, is_leaf)?;
        Ok(Self {
            id,
            is_leaf,
            root: Some(root),
        })
    }

    pub fn write(&self, w: &mut Writer) -> bool {
        w.u32(self.id);
        w.u32(u32::from(self.is_leaf));
        self.root.as_ref().is_some_and(|r| r.write(w))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the path-map builder's edge finding and plane making, on a made-up patch)
    use super::*;
    use crate::pathfind::node::Node;

    fn map_with_square() -> PathMap {
        let mut m = PathMap::new(0x1010_FFFD);
        let mut n = Node::new();
        n.id = 7;
        assert!(m.add_path_node(n));
        let square = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(10.0, 10.0, 1.0),
            Vec3::new(0.0, 10.0, 0.0),
        ];
        assert!(m.set_node_polygon(square, 7));
        m
    }

    #[test]
    fn a_line_along_a_patch_edge_lists_the_edge() {
        let m = map_with_square();
        let leaf = BspLeaf { node_id: 7 };
        let mut out = Vec::new();
        // From the corner at the origin east, past the next corner.
        let line = Ray::segment(Vec3::ZERO, Vec3::new(15.0, 0.0, 0.0));
        assert!(leaf.find_node_list(&line, &mut out, &m));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].node, Some(7));
        assert_eq!(out[0].edge.point, Vec3::ZERO);
        assert!((out[0].edge.length - 10.0).abs() < 1e-5);
        // Short of the next corner: nothing.
        let short = Ray::segment(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0));
        assert!(leaf.find_node_list(&short, &mut out, &m));
        assert_eq!(out.len(), 1);
        // Across the patch, along no edge: nothing.
        let across = Ray::segment(Vec3::ZERO, Vec3::new(10.0, 10.0, 0.0));
        assert!(leaf.find_node_list(&across, &mut out, &m));
        assert_eq!(out.len(), 1);
        // Not from a corner: nothing.
        let off = Ray::segment(Vec3::new(5.0, 0.0, 0.0), Vec3::new(15.0, 0.0, 0.0));
        assert!(leaf.find_node_list(&off, &mut out, &m));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_segment_makes_the_upright_plane_through_it() {
        let mut n = BspNode::new();
        assert!(n.make_plane_from_segment(&Ray::segment(
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(4.0, 2.0, 0.0)
        )));
        assert_eq!(n.plane.normal, Vec3::new(0.0, -1.0, 0.0));
        assert_eq!(n.plane.d, 2.0);
        assert!(!n.make_plane_from_segment(&Ray::segment(Vec3::ZERO, Vec3::new(0.0, 0.0, 3.0))));
    }
}
