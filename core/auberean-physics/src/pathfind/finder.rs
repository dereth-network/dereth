//! Path requests: what a search starts from and aims for, how big the body is, which answers it
//! wants, and (for the ordinary request) what it must avoid and how it prices a step.
//!
//! The answers a request can want (bits): 1, the end node reached; 2, the first node farther
//! along than a minimum distance; 4, the node whose estimate to the end is least; 8, the node
//! whose estimate is greatest. A request may also require (bits) 1, water at every node it takes,
//! or 2, no water at any.

use std::collections::BTreeMap;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::db::{NodeRef, PathMapDb, PathMaps};
use super::node::{Link, Node, Spot};
use super::route::Path;
use super::search::{Marker, SearchHost};
use crate::geom::Ray;

/// The answers a request can want.
pub mod solution {
    pub const END: u32 = 1;
    pub const MIN_DISTANCE: u32 = 2;
    pub const NEAREST: u32 = 4;
    pub const FARTHEST: u32 = 8;
}

/// What a request can require of every node it takes.
pub mod require {
    pub const WATER: u32 = 1;
    pub const NO_WATER: u32 = 2;
}

/// The longest a request searches by default, in nodes taken.
pub const DEFAULT_MAX_NODES: u32 = 300;

/// A size a body is given as: rounded to whole metres within 1 to 5, less 0.1.
fn body_size(v: f32) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let s = (f64::from(v) + 0.5).floor() as f32;
    s.clamp(1.0, 5.0) - 0.1
}

fn null_position() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

/// What every request holds.
// D2-13547
#[derive(Clone, Debug)]
pub struct FinderBase {
    pub start: Position,
    pub end: Position,
    pub start_id: u32,
    pub start_node: Option<NodeRef>,
    pub end_id: u32,
    pub end_node: Option<NodeRef>,
    /// The answers wanted ([`solution`] bits).
    pub soln: u32,
    pub term_soln: u32,
    /// What every node taken must be ([`require`] bits).
    pub prop_soln: u32,
    pub max_nodes: u32,
    /// The body's height and width (-1: any).
    pub height: f32,
    pub width: f32,
    /// The marker kept for each answer during a search.
    pub marker_solutions: BTreeMap<u32, usize>,
    /// The way found for each answer.
    pub path_solutions: BTreeMap<u32, Path>,
}

impl Default for FinderBase {
    // D2-13545
    fn default() -> Self {
        Self {
            start: null_position(),
            end: null_position(),
            start_id: 0,
            start_node: None,
            end_id: 0,
            end_node: None,
            soln: 0,
            term_soln: 0,
            prop_soln: 0,
            max_nodes: DEFAULT_MAX_NODES,
            height: -1.0,
            width: -1.0,
            marker_solutions: BTreeMap::new(),
            path_solutions: BTreeMap::new(),
        }
    }
}

impl FinderBase {
    /// The body's height: rounded to whole metres within 1 to 5, less 0.1.
    // D2-13516
    pub fn set_height(&mut self, h: f32) {
        self.height = body_size(h);
    }

    /// The body's width, likewise.
    pub fn set_width(&mut self, w: f32) {
        self.width = body_size(w);
    }

    /// Starts from a node (0: none); answers whether it was found.
    pub fn set_start<S: PathMaps + ?Sized>(
        &mut self,
        id: u32,
        db: &mut PathMapDb,
        source: &mut S,
    ) -> bool {
        self.start_id = id;
        if id == 0 {
            self.start_node = None;
            return false;
        }
        self.start_node = db.node(source, id);
        self.start_node.is_some()
    }

    /// Ends at a node (0: none, which is not a failure); answers whether it was found.
    pub fn set_end<S: PathMaps + ?Sized>(
        &mut self,
        id: u32,
        db: &mut PathMapDb,
        source: &mut S,
    ) -> bool {
        self.end_id = id;
        if id == 0 {
            self.end_node = None;
            return true;
        }
        self.end_node = db.node(source, id);
        self.end_node.is_some()
    }

    /// Starts from a place: kept, and the node under it looked up.
    pub fn set_start_position<S: PathMaps + ?Sized>(
        &mut self,
        p: &Position,
        db: &mut PathMapDb,
        source: &mut S,
    ) -> bool {
        self.start = *p;
        let id = db.node_id_at(source, &Spot::of(p));
        self.start_id = id;
        if id == 0 {
            self.start_node = None;
            return false;
        }
        self.start_node = db.node(source, id);
        self.start_node.is_some()
    }

    /// Ends at a place: kept, and the node under it looked up (none is not a failure).
    pub fn set_end_position<S: PathMaps + ?Sized>(
        &mut self,
        p: &Position,
        db: &mut PathMapDb,
        source: &mut S,
    ) -> bool {
        self.end = *p;
        let id = db.node_id_at(source, &Spot::of(p));
        self.end_id = id;
        if id == 0 {
            self.end_node = None;
            return true;
        }
        self.end_node = db.node(source, id);
        self.end_node.is_some()
    }

    // D2-13528
    #[must_use]
    pub fn has_any_solutions(&self) -> bool {
        !self.path_solutions.is_empty()
    }

    // D2-13529
    #[must_use]
    pub fn has_marker_for_solution(&self, soln: u32) -> bool {
        self.marker_solutions.contains_key(&soln)
    }

    // D2-13532
    pub fn set_marker_for_solution(&mut self, soln: u32, marker: usize) {
        self.marker_solutions.insert(soln, marker);
    }

    /// The way found for an answer.
    #[must_use]
    pub fn inq_solution(&self, soln: u32) -> Option<&Path> {
        self.path_solutions.get(&soln)
    }

    /// Turns each kept marker of an answer wanted into a way: the nodes from the start to it, and
    /// each link's doorway narrowed for the body's width. The kept markers are then forgotten.
    // D2-13539
    pub fn setup_solutions(&mut self, markers: &[Marker]) {
        self.path_solutions.clear();
        for (&soln, &m) in &self.marker_solutions {
            if self.soln & soln == 0 {
                continue;
            }
            let mut path = Path::new();
            path.start = self.start;
            path.end = self.end;
            let mut at = Some(m);
            while let Some(i) = at {
                let mk = &markers[i];
                path.nodes.insert(0, mk.node.id);
                if let Some(link) = &mk.link {
                    let mut ray = Ray::default();
                    link.portal_constrained(&mut ray, self.width);
                    path.portals.insert(0, ray);
                }
                at = mk.prev;
            }
            self.path_solutions.insert(soln, path);
        }
        self.marker_solutions.clear();
    }
}

/// A request, as the search sees it.
pub trait Finder {
    fn base(&self) -> &FinderBase;
    fn base_mut(&mut self) -> &mut FinderBase;
    /// 0 when the request can be searched, else why not.
    fn verify_data(&self) -> u32;
    /// Sets up the start's marker.
    fn set_start_marker(&mut self, m: &mut Marker, host: &mut dyn SearchHost);
    /// Looks at a marker taken off the heap (number `which`), keeping it for the answers it
    /// satisfies; answers whether the search ends here.
    fn check_node(&mut self, m: &Marker, which: usize) -> bool;
    /// Whether the search may take `link` from `from` to `to`.
    fn is_acceptable(&self, from: &Node, link: &Link, to: &Node) -> bool;
    /// What a link costs (`dest`: its node when it is of the same map).
    fn cost(&mut self, link: &Link, dest: Option<&Node>, host: &mut dyn SearchHost) -> f32;
    /// The estimate from a node on.
    fn estimate(&mut self, node: &Node, host: &mut dyn SearchHost) -> f32;
    /// Sets the order a marker is opened in.
    fn update_marker(&self, m: &mut Marker);
    fn is_random(&self) -> bool;
    /// Turns the kept markers into ways.
    fn setup_solutions(&mut self, markers: &[Marker]) {
        self.base_mut().setup_solutions(markers);
    }
}

/// The ordinary request: a way of least distance (or, moving away, of most), avoiding slopes
/// steeper than allowed, entering water or solid ground from off it, headroom and ways too small
/// for the body, and (when asked) leaving one landblock.
// D2-13561
#[derive(Clone, Debug)]
pub struct NormalFinder {
    pub base: FinderBase,
    /// The least and greatest estimates seen.
    pub min_h: f32,
    pub max_h: f32,
    /// The distance past which the minimum-distance answer is found.
    pub min_g: f32,
    /// The distance past which the search ends.
    pub max_g: f32,
    pub slope_constraint: u32,
    pub solid_constrained: bool,
    pub water_constrained: bool,
    pub block_constrained: bool,
    /// The landblock it must stay in (the cell id's top half).
    pub block: u32,
    /// The most a step's cost and estimate are scaled by at random (0: not at all).
    pub randomize: f32,
    pub move_away: bool,
}

impl Default for NormalFinder {
    // D2-13549
    fn default() -> Self {
        Self {
            base: FinderBase::default(),
            min_h: f32::MAX,
            max_h: -f32::MAX,
            min_g: f32::MAX,
            max_g: 300.0,
            slope_constraint: 1,
            solid_constrained: true,
            water_constrained: true,
            block_constrained: false,
            block: 0,
            randomize: 0.0,
            move_away: false,
        }
    }
}

impl NormalFinder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes on another request: everything but where it ends, which is not copied (its end
    /// node is).
    // D2-13551, D2-13546
    pub fn assign_from(&mut self, other: &NormalFinder) {
        let end = self.base.end;
        self.base = other.base.clone();
        self.base.end = end;
        self.min_h = other.min_h;
        self.max_h = other.max_h;
        self.min_g = other.min_g;
        self.max_g = other.max_g;
        self.slope_constraint = other.slope_constraint;
        self.solid_constrained = other.solid_constrained;
        self.water_constrained = other.water_constrained;
        self.block_constrained = other.block_constrained;
        self.block = other.block;
        self.randomize = other.randomize;
        self.move_away = other.move_away;
    }

    /// A copy of another request, ending nowhere (see [`NormalFinder::assign_from`]).
    #[must_use]
    pub fn copy_of(other: &NormalFinder) -> Self {
        let mut f = Self::new();
        f.assign_from(other);
        f
    }
}

/// The landblock part of a cell id.
const BLOCK_PART_SHIFT: u32 = 16;

impl Finder for NormalFinder {
    fn base(&self) -> &FinderBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut FinderBase {
        &mut self.base
    }

    /// 3 without a start node; 2 wanting the end reached without an end node.
    // D2-13552
    fn verify_data(&self) -> u32 {
        let b = &self.base;
        if b.start_id == 0 || b.start_node.is_none() {
            return 3;
        }
        if b.soln & solution::END != 0 && (b.end_id == 0 || b.end_node.is_none()) {
            return 2;
        }
        0
    }

    /// No cost so far, the estimate from the start node.
    // D2-13553
    fn set_start_marker(&mut self, m: &mut Marker, host: &mut dyn SearchHost) {
        m.prev = None;
        m.dist = 0.0;
        let start = self.base.start_node.clone();
        m.h = match start {
            Some(n) => self.estimate(n.node(), host),
            None => 0.0,
        };
        self.update_marker(m);
    }

    /// Past the greatest distance the search ends. A node without the water required (or with
    /// water forbidden) is passed over. Otherwise the marker is kept as the nearest or farthest
    /// by estimate when it is; the first farther than the minimum distance is kept and ends the
    /// search; and the end node is kept and ends it.
    // D2-13554
    fn check_node(&mut self, m: &Marker, which: usize) -> bool {
        if m.dist > self.max_g {
            return true;
        }
        let node = m.node.node();
        let water = node.data.map(|d| d.water);
        if self.base.prop_soln & require::WATER != 0 {
            if water != Some(true) {
                return false;
            }
        } else if self.base.prop_soln & require::NO_WATER != 0 && water == Some(true) {
            return false;
        }
        let soln = self.base.soln;
        if soln & solution::NEAREST != 0 && m.h < self.min_h {
            self.min_h = m.h;
            self.base.set_marker_for_solution(solution::NEAREST, which);
        }
        if soln & solution::FARTHEST != 0 && m.h > self.max_h {
            self.max_h = m.h;
            self.base.set_marker_for_solution(solution::FARTHEST, which);
        }
        if soln & solution::MIN_DISTANCE != 0
            && m.dist > self.min_g
            && !self.base.has_marker_for_solution(solution::MIN_DISTANCE)
        {
            self.base
                .set_marker_for_solution(solution::MIN_DISTANCE, which);
            return true;
        }
        if soln & solution::END != 0 && self.base.end_id != 0 && node.id == self.base.end_id {
            self.base.set_marker_for_solution(solution::END, which);
            return true;
        }
        false
    }

    /// Refused: a node steeper than allowed; water entered from off it (when constrained); solid
    /// ground entered from off it, or (still from off it) headroom at the node, or a way's width
    /// or headroom, smaller than the body (when solid-constrained; -1 on either side passes); a
    /// node outside the landblock (when constrained to one).
    // D2-13555
    fn is_acceptable(&self, from: &Node, link: &Link, to: &Node) -> bool {
        let to_data = to.data;
        let slope = to_data.map_or(0, |d| u32::from(d.slope));
        if self.slope_constraint < slope {
            return false;
        }
        let from_data = from.data;
        let from_water = from_data.is_some_and(|d| d.water);
        if !from_water && self.water_constrained && to_data.is_some_and(|d| d.water) {
            return false;
        }
        let from_solid = from_data.is_some_and(|d| d.solid);
        if !from_solid && self.solid_constrained {
            if to_data.is_some_and(|d| d.solid) {
                return false;
            }
            let (h, w) = (self.base.height, self.base.width);
            #[allow(clippy::float_cmp)]
            if let Some(d) = to_data {
                if h != -1.0 && d.height != -1.0 && d.height < h {
                    return false;
                }
            }
            if let Some(ld) = link.data {
                #[allow(clippy::float_cmp)]
                if w != -1.0 && ld.width != -1.0 && ld.width < w {
                    return false;
                }
                #[allow(clippy::float_cmp)]
                if h != -1.0 && ld.height != -1.0 && ld.height < h {
                    return false;
                }
            }
        }
        if self.block_constrained && self.block != to.center.cell.0 >> BLOCK_PART_SHIFT {
            return false;
        }
        true
    }

    /// The link's distance, doubled into water or solid ground of the same map (each; halved
    /// instead moving away), then scaled by a random 0 to `randomize` when randomized.
    // D2-13556
    fn cost(&mut self, link: &Link, dest: Option<&Node>, host: &mut dyn SearchHost) -> f32 {
        let mut c = link.data.map_or(0.0, |d| d.distance);
        if let Some(n) = dest {
            let f = if self.move_away { 0.5 } else { 2.0 };
            if let Some(d) = n.data {
                if d.water {
                    c *= f;
                }
                if d.solid {
                    c *= f;
                }
            }
        }
        if self.randomize == 0.0 {
            return c;
        }
        host.roll_dice(0.0, self.randomize) * c
    }

    /// The straight-line distance to the end node, scaled by a random 0 to `randomize` when
    /// randomized; with no end node, the random number alone.
    // D2-13557
    fn estimate(&mut self, node: &Node, host: &mut dyn SearchHost) -> f32 {
        let Some(end) = self.base.end_node.clone() else {
            return host.roll_dice(0.0, self.randomize);
        };
        let d = node.distance(end.node());
        if self.randomize == 0.0 {
            return d;
        }
        host.roll_dice(0.0, self.randomize) * d
    }

    /// Cost plus estimate, or moving away their negation.
    // D2-13558
    fn update_marker(&self, m: &mut Marker) {
        m.f = if self.move_away {
            -m.dist - m.h
        } else {
            m.h + m.dist
        };
    }

    // D2-13550
    fn is_random(&self) -> bool {
        self.randomize != 0.0
    }
}
