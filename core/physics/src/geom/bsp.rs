//! Binary-space-partition trees: traversal over interior, portal and leaf nodes.
//!
//! Source: `docs/formats/10-gfxobj.md` for the record layout, the node tags, how often each tag
//! occurs in retail data and what each of the three tree kinds is for. The paragraphs below
//! summarise the question each entry point answers, its constants and its callees.
//!
//! A tree is an arena of nodes with node 0 as the root. A node is one of three kinds: an
//! **interior** node (a splitting plane and up to two children), a **portal** — which carries a
//! portal-polygon list for the renderer and behaves as an ordinary interior node in every physics
//! query — or a **leaf**, which carries a polygon list and, in a physics tree, a `solid` flag.
//! The plane is `dot(N, p) + d`, positive side `> 0`. Three shapes of tree share that record set:
//!
//! * a **cell** tree is a chain of positive-child-only nodes ending in a childless terminator.
//!   A point is in the cell when it is on the positive side of, or within `EPSILON` of, every
//!   plane in the chain, so a cell is a convex polyhedron written as an intersection of
//!   half-spaces. [`BspTree::point_inside_cell_bsp`], [`BspTree::sphere_intersects_cell_bsp`] and
//!   [`BspTree::box_intersects_cell_bsp`] are the three queries over it, and all three walk
//!   [`BspTree::cell_chain`] and nothing else.
//! * a **physics** tree is a solid-leaf BSP: interior nodes partition space, leaves hold the
//!   collision polygons, and every answer comes out of a leaf. Everything else here reads it.
//! * a **drawing** tree orders polygons for the painter's algorithm. Nothing in physics reads it.
//!
//! Two walk shapes cover every query:
//!
//! * a **point** walk is one descent and never backtracks: the sign of the plane at the point
//!   picks a single child ([`BspNode::child_towards`]), and the walk ends at the first leaf.
//! * a **sphere** walk is a depth-first visit, positive child before negative (bar
//!   [`BspTree::sphere_intersects_solid_poly`], which takes the half holding the centre first),
//!   pruned twice —
//!   first by the node's own bounding sphere (a node whose sphere misses the query sphere
//!   contains nothing that can be hit), then by the plane: a centre standing further than the
//!   sphere's *reach* from the plane leaves the far half-space untouched, so only one child is
//!   visited. A straddling sphere visits both. [`split`] is that second prune, and `reach` is
//!   the radius shortened by `EPSILON` everywhere except where noted.
//!
//! Transition-driven collision searches need a complete transition and live in
//! [`crate::transition::collide`].
//!
//! The collision walk's two-sphere branch operand order is not yet known. That affects
//! [`crate::transition::collide`], not this module.

use dereth_primitives::Vec3;

use crate::geom::plane::{Plane, Sidedness};
use crate::geom::polygon::Polygon;
use crate::geom::sphere::Sphere;
use crate::geom::Bounding;
use crate::geom::PlaneExt;
use crate::globals::EPSILON;
use crate::math::V3;

/// The slack `sphere_intersects_cell_bsp` adds to the query radius.
///
/// It is the one place a sphere walk *widens* the sphere instead of shortening it by `EPSILON`,
/// and the constant is fifty times larger. A cell boundary is the wall of the room the object is
/// standing in, and reporting a sphere as touching the wall a centimetre before it does is what
/// keeps an object from being handed a cell it is about to leave.
const CELL_BSP_FUDGE: f32 = 0.01;

/// Where a walk needs a child the tree does not have.
///
/// Every interior node in retail physics data has both children — that is what
/// [`BspTree::interior_nodes_have_both_children`] states and what the dat test
/// `every_retail_physics_bsp_converts_traverses_and_terminates` establishes over
/// `client_portal.dat` — so the client dispatches through the child pointer unguarded and this
/// case cannot arise on shipped data. A tree built from a malformed record could still reach it,
/// and "nothing here" is the only answer that cannot make a caller move an object it should not:
/// a sphere walk simply does not enqueue the absent child, and a point walk that runs off the
/// bottom of the tree reports no intersection.
const MISSING_CHILD_IS_EMPTY: bool = true;

/// Which half-spaces of a splitting plane a sphere can occupy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Halves {
    /// Clear of the plane on the positive side: only `pos_child` is reachable.
    Positive,
    /// Clear of the plane on the negative side: only `neg_child` is reachable.
    Negative,
    /// The sphere crosses the plane, so both children hold part of it.
    Both,
}

/// The sphere walks' plane prune. `reach` is how far the sphere extends past its centre for this
/// query; a centre at least that far from the plane cannot put any of the sphere on the far side.
///
/// The signed distance comes back with it because the straddling case still needs its sign: the
/// half-space holding the centre is the one a `center_check` flag follows. `Positive` is tested
/// first, so a non-positive `reach` — a sphere smaller than `EPSILON` — resolves a centre exactly
/// on the plane to the positive child rather than to both.
fn split(plane: &Plane, center: Vec3, reach: f32) -> (f32, Halves) {
    let dist = plane.dot_point(center);
    let halves = if dist >= reach {
        Halves::Positive
    } else if dist <= -reach {
        Halves::Negative
    } else {
        Halves::Both
    };
    (dist, halves)
}

/// The bounding-sphere intersection used to prune every node visited by a sphere walk.
///
/// Two spheres that exactly touch count as intersecting: the test is `|d|² - sum² < EPSILON`, an
/// inclusive comparison against a small positive number rather than against zero. Some
/// reimplementations test `|d|² < sum²`; the client's inclusive form is what the tests pin.
#[must_use]
pub fn spheres_intersect(a: &Sphere, b: &Sphere) -> bool {
    let sum = a.radius + b.radius;
    a.center.sub(b.center).mag2() - sum * sum < EPSILON
}

/// The node kind. Portal nodes behave as ordinary interior nodes during physics
/// traversal; leaves supply the terminal geometry tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BspNodeKind {
    Node,
    Portal,
    Leaf { leaf_index: i32, solid: bool },
}

/// One node of a BSP tree, in an arena. Node 0 is the root.
#[derive(Debug, Clone, PartialEq)]
pub struct BspNode {
    pub sphere: Sphere,
    pub splitting_plane: Plane,
    pub pos_child: Option<u32>,
    pub neg_child: Option<u32>,
    pub kind: BspNodeKind,
    /// Indices into [`BspTree::polygons`].
    pub in_polys: Vec<u32>,
}

impl BspNode {
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        matches!(self.kind, BspNodeKind::Leaf { .. })
    }

    /// A physics leaf marked as lying inside solid matter. A `center_check` query takes that flag
    /// as its answer without consulting the leaf's polygons at all.
    fn is_solid_leaf(&self) -> bool {
        matches!(self.kind, BspNodeKind::Leaf { solid: true, .. })
    }

    /// The single child a **point** descends into. The sign of the plane decides and there is no
    /// epsilon: a point exactly on the plane goes negative, because the test is `> 0`.
    fn child_towards(&self, point: Vec3) -> Option<u32> {
        if self.splitting_plane.dot_point(point) > 0.0 {
            self.pos_child
        } else {
            self.neg_child
        }
    }

    /// The children a sphere walk enqueues, **negative first**, so that a LIFO work stack pops the
    /// positive child — and finishes its whole subtree — before it reaches the negative one.
    fn children_for(&self, halves: Halves) -> [Option<u32>; 2] {
        match halves {
            Halves::Positive => [None, self.pos_child],
            Halves::Negative => [None, self.neg_child],
            Halves::Both => [self.neg_child, self.pos_child],
        }
    }
}

/// A single root plus the polygon pool indexed by its leaves.
///
/// The original client shares polygons with the owning environment-cell geometry or
/// graphics object. This tree owns a copy because physics does not mutate them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BspTree {
    pub nodes: Vec<BspNode>,
    pub polygons: Vec<Polygon>,
}

impl BspTree {
    #[must_use]
    pub fn root(&self) -> Option<&BspNode> {
        self.nodes.first()
    }

    fn node(&self, i: u32) -> &BspNode {
        &self.nodes[i as usize]
    }

    /// The root's index, or `None` for an empty tree. Every walk below starts here, and an empty
    /// tree therefore answers "nothing" without a special case of its own.
    fn root_index(&self) -> Option<u32> {
        (!self.nodes.is_empty()).then_some(0)
    }

    /// A sphere walk's work stack, seeded with the root.
    fn work_stack(&self) -> Vec<u32> {
        self.root_index().into_iter().collect()
    }

    /// The cell-boundary chain: node 0 and every positive child after it, ending at the
    /// terminator that has none. A cell tree is nothing but this chain, so the three cell queries
    /// each reduce to a predicate over it.
    fn cell_chain(&self) -> impl Iterator<Item = &BspNode> + '_ {
        let mut cursor = self.root_index();
        std::iter::from_fn(move || {
            let n = self.node(cursor?);
            cursor = n.pos_child;
            Some(n)
        })
    }

    /// Cell BSP: is the point inside the cell?
    ///
    /// The cell is the intersection of the chain's positive half-spaces, so the point is inside
    /// exactly when no plane in the chain puts it strictly behind — within `EPSILON` of a plane
    /// still counts as inside. An empty tree bounds nothing and answers "inside".
    #[must_use]
    pub fn point_inside_cell_bsp(&self, point: Vec3) -> bool {
        self.cell_chain()
            .all(|n| n.splitting_plane.which_side(point, EPSILON) != Sidedness::Negative)
    }

    /// The sphere-versus-cell-BSP test.
    ///
    /// The same chain, widened by [`CELL_BSP_FUDGE`] rather than narrowed by `EPSILON`: a plane
    /// that leaves the whole widened sphere behind it puts the sphere `Outside` at once, a plane
    /// the sphere crosses can only downgrade the answer to `PartiallyInside`, and a sphere that
    /// clears every plane is `EntirelyInside`.
    #[must_use]
    pub fn sphere_intersects_cell_bsp(&self, sphere: &Sphere) -> Bounding {
        let check_rad = sphere.radius + CELL_BSP_FUDGE;
        let mut straddled = false;
        for n in self.cell_chain() {
            let dist = n.splitting_plane.dot_point(sphere.center);
            if dist <= -check_rad {
                return Bounding::Outside;
            }
            straddled |= dist < check_rad;
        }
        if straddled {
            Bounding::PartiallyInside
        } else {
            Bounding::EntirelyInside
        }
    }

    /// The box-versus-cell-BSP test.
    ///
    /// A plane excludes the box only when it leaves **every** corner strictly behind it; one
    /// corner in front or on the plane is enough to keep the box. The cheap test on `min` alone
    /// comes first, because a plane that does not even exclude that corner cannot exclude the box
    /// and most planes in a chain do not.
    #[must_use]
    pub fn box_intersects_cell_bsp(&self, min: Vec3, max: Vec3) -> bool {
        let corners = [
            Vec3::new(min.x, min.y, min.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(min.x, max.y, max.z),
            Vec3::new(max.x, max.y, max.z),
        ];
        !self.cell_chain().any(|n| {
            let plane = &n.splitting_plane;
            plane.dot_point(min) < -EPSILON
                && corners
                    .iter()
                    .all(|&c| plane.which_side(c, EPSILON) == Sidedness::Negative)
        })
    }

    /// Physics BSP: is the point in solid?
    ///
    /// One descent, and the leaf it lands in decides: a leaf holding collision polygons is solid,
    /// an empty leaf is not. The `solid` flag plays no part — only the sphere queries consult it.
    #[must_use]
    pub fn point_intersects_solid(&self, point: Vec3) -> bool {
        let mut cursor = self.root_index();
        while let Some(idx) = cursor {
            let n = self.node(idx);
            if n.is_leaf() {
                return !n.in_polys.is_empty();
            }
            cursor = n.child_towards(point);
        }
        !MISSING_CHILD_IS_EMPTY
    }

    /// The first polygon a sphere hits, together
    /// with its contact point.
    ///
    /// Each leaf polygon is tested for two separate things. **Overlap** names the polygon in
    /// `hit_poly`; only an overlap the movement runs *into* is a hit, and only a hit ends the
    /// walk. A polygon left named by a `false` return is the back-facing case, and that is the
    /// whole of the client's back-face signal: the collision step reads
    /// `returned || hit_poly.is_some()` in its default and `PATH_CLIPPED` arms and routes the
    /// remainder to its `CONTACT` arm, which is why this writes the out-parameter before it
    /// applies the direction gate.
    pub fn sphere_intersects_poly(
        &self,
        sphere: &Sphere,
        movement: Vec3,
        hit_poly: &mut Option<u32>,
        contact_point: &mut Vec3,
    ) -> bool {
        let reach = sphere.radius - EPSILON;
        let mut work = self.work_stack();
        while let Some(idx) = work.pop() {
            let n = self.node(idx);
            if !spheres_intersect(&n.sphere, sphere) {
                continue;
            }
            if n.is_leaf() {
                for &p in &n.in_polys {
                    let (overlaps, hits) = self.polygons[p as usize].pos_hits_sphere_parts(
                        sphere,
                        movement,
                        contact_point,
                    );
                    if overlaps {
                        *hit_poly = Some(p);
                    }
                    if hits {
                        return true;
                    }
                }
                continue;
            }
            let (_, halves) = split(&n.splitting_plane, sphere.center, reach);
            work.extend(n.children_for(halves).into_iter().flatten());
        }
        false
    }

    /// Does the sphere touch solid matter?
    ///
    /// `center_check` is the "an unpartitioned solid leaf counts as filled" flag: with it set, a
    /// leaf marked `solid` answers yes on the flag alone, and only geometry can answer without
    /// it. The flag follows the sphere's **centre** down the tree — on a straddling node the half
    /// holding the centre inherits it and the other half is visited with it cleared — because the
    /// flag is a statement about where the centre is, not about the whole sphere.
    ///
    /// A leaf prunes in a different order from an interior node: an empty polygon list and then
    /// the `solid` flag are both settled before the leaf's bounding sphere is consulted, so a
    /// solid leaf standing well away from the sphere still answers on its flag.
    #[must_use]
    pub fn sphere_intersects_solid(&self, sphere: &Sphere, center_check: bool) -> bool {
        let reach = sphere.radius - EPSILON;
        let mut work: Vec<(u32, bool)> = self
            .root_index()
            .map(|r| (r, center_check))
            .into_iter()
            .collect();
        while let Some((idx, inherited)) = work.pop() {
            let n = self.node(idx);
            if n.is_leaf() {
                if n.in_polys.is_empty() {
                    continue;
                }
                if inherited && n.is_solid_leaf() {
                    return true;
                }
                if !spheres_intersect(&n.sphere, sphere) {
                    continue;
                }
                if n.in_polys
                    .iter()
                    .any(|&p| self.polygons[p as usize].hits_sphere(sphere))
                {
                    return true;
                }
                continue;
            }
            if !spheres_intersect(&n.sphere, sphere) {
                continue;
            }
            let (dist, halves) = split(&n.splitting_plane, sphere.center, reach);
            match halves {
                Halves::Positive => work.extend(n.pos_child.map(|c| (c, inherited))),
                Halves::Negative => work.extend(n.neg_child.map(|c| (c, inherited))),
                Halves::Both => {
                    let centre_is_positive = dist >= 0.0;
                    work.extend(n.neg_child.map(|c| (c, inherited && !centre_is_positive)));
                    work.extend(n.pos_child.map(|c| (c, inherited && centre_is_positive)));
                }
            }
        }
        false
    }

    /// As above but naming the polygon,
    /// which is what `placement_insert` pushes the sphere off.
    ///
    /// The `radius` argument is **not** `sphere.radius`: `placement_insert` passes the unscaled
    /// radius while the sphere carries the scaled one, and it is the unscaled one that sets the
    /// plane prune's reach.
    ///
    /// Two things separate this from [`Self::sphere_intersects_solid`]. A straddling node walks
    /// the **near** half first — the half holding the centre — and stops the moment a polygon has
    /// been named, answering `center_solid` rather than the polygon's own `true`; only if nothing
    /// was named does it fall through to the far half, with `center_check` cleared. And a solid
    /// leaf sets `center_solid` instead of returning: the caller wants the polygon to push off,
    /// so "the centre is in solid" is reported separately from "here is what it is in".
    pub fn sphere_intersects_solid_poly(
        &self,
        sphere: &Sphere,
        radius: f32,
        center_solid: &mut bool,
        hit_poly: &mut Option<u32>,
        center_check: bool,
    ) -> bool {
        match self.root_index() {
            Some(root) => self.solid_poly_walk(
                root,
                sphere,
                radius - EPSILON,
                center_solid,
                hit_poly,
                center_check,
            ),
            None => false,
        }
    }

    fn solid_poly_walk(
        &self,
        idx: u32,
        sphere: &Sphere,
        reach: f32,
        center_solid: &mut bool,
        hit_poly: &mut Option<u32>,
        center_check: bool,
    ) -> bool {
        let n = self.node(idx);
        if n.is_leaf() {
            if n.in_polys.is_empty() {
                return false;
            }
            if center_check && n.is_solid_leaf() {
                *center_solid = true;
            }
            if !spheres_intersect(&n.sphere, sphere) {
                return *center_solid;
            }
            return match n
                .in_polys
                .iter()
                .find(|&&p| self.polygons[p as usize].hits_sphere(sphere))
            {
                Some(&p) => {
                    *hit_poly = Some(p);
                    true
                }
                None => *center_solid,
            };
        }
        if !spheres_intersect(&n.sphere, sphere) {
            return false;
        }
        let (dist, halves) = split(&n.splitting_plane, sphere.center, reach);
        let (near, far) = match halves {
            Halves::Positive => (n.pos_child, None),
            Halves::Negative => (n.neg_child, None),
            Halves::Both if dist <= 0.0 => (n.neg_child, n.pos_child),
            Halves::Both => (n.pos_child, n.neg_child),
        };
        let near_answer = match near {
            Some(c) => self.solid_poly_walk(c, sphere, reach, center_solid, hit_poly, center_check),
            None => !MISSING_CHILD_IS_EMPTY,
        };
        if halves != Halves::Both {
            return near_answer;
        }
        if hit_poly.is_some() {
            return *center_solid;
        }
        match far {
            Some(c) => self.solid_poly_walk(c, sphere, reach, center_solid, hit_poly, false),
            None => !MISSING_CHILD_IS_EMPTY,
        }
    }

    /// Is there a walkable polygon under the sphere along
    /// `up`?
    ///
    /// A polygon qualifies only if it passes both gates: `walkable_hits_sphere` for the slope
    /// against `walkable_allowance`, and `check_small_walkable` for the quarter-radius footprint.
    /// The first qualifying polygon ends the walk; nothing is recorded.
    #[must_use]
    pub fn hits_walkable(&self, sphere: &Sphere, up: Vec3, walkable_allowance: f32) -> bool {
        let reach = sphere.radius - EPSILON;
        let mut work = self.work_stack();
        while let Some(idx) = work.pop() {
            let n = self.node(idx);
            if !spheres_intersect(&n.sphere, sphere) {
                continue;
            }
            if n.is_leaf() {
                let found = n.in_polys.iter().any(|&p| {
                    let poly = &self.polygons[p as usize];
                    poly.walkable_hits_sphere(sphere, up, walkable_allowance)
                        && poly.check_small_walkable(sphere, up)
                });
                if found {
                    return true;
                }
                continue;
            }
            let (_, halves) = split(&n.splitting_plane, sphere.center, reach);
            work.extend(n.children_for(halves).into_iter().flatten());
        }
        false
    }

    /// Find the walkable polygon under the sphere and
    /// adjust the sphere onto it, consuming `walk_interp`.
    ///
    /// Unlike [`Self::hits_walkable`] this does **not** short-circuit: every leaf polygon that is
    /// both walkable and adjustable overwrites `hit_poly`, so the last one wins. The walk is also
    /// the one place a sphere moves underneath it — each node re-reads `valid_pos`, so a leaf that
    /// has already lifted the sphere changes what the rest of the tree is tested against, and the
    /// `walk_interp` ratchet is what keeps that converging.
    #[allow(clippy::too_many_arguments)]
    pub fn find_walkable(
        &self,
        valid_pos: &mut Sphere,
        hit_poly: &mut Option<u32>,
        movement: Vec3,
        up: Vec3,
        walkable_allowance: f32,
        walk_interp: &mut f32,
        changed: &mut bool,
    ) {
        let mut work = self.work_stack();
        while let Some(idx) = work.pop() {
            let n = self.node(idx);
            if !spheres_intersect(&n.sphere, valid_pos) {
                continue;
            }
            if n.is_leaf() {
                for &p in &n.in_polys {
                    let poly = &self.polygons[p as usize];
                    if !poly.walkable_hits_sphere(valid_pos, up, walkable_allowance) {
                        continue;
                    }
                    if let Some(interp) = poly.adjust_sphere_to_plane(
                        &mut valid_pos.center,
                        valid_pos.radius,
                        movement,
                        *walk_interp,
                    ) {
                        *walk_interp = interp;
                        *changed = true;
                        *hit_poly = Some(p);
                    }
                }
                continue;
            }
            let (_, halves) = split(
                &n.splitting_plane,
                valid_pos.center,
                valid_pos.radius - EPSILON,
            );
            work.extend(n.children_for(halves).into_iter().flatten());
        }
    }

    /// Exact contact refinement: advance to the plane and
    /// test, then bisect, with **one shared iteration counter capped at 15 across both loops** and
    /// a bracket tolerance of `0.02`.
    ///
    /// Checked against retail behaviour; three details are easy to get wrong. Its one caller in
    /// the client is the `PATH_CLIPPED` collision arm.
    ///
    /// | what | wrong reading | retail |
    /// |---|---|---|
    /// | bisection assignment | hit ⇒ `lower = mid`, free ⇒ `upper = mid` | hit ⇒ **`upper = mid`**, free ⇒ **`lower = mid`** |
    /// | iteration budget | 15 + 15 | **15 total**: one counter is zeroed once and incremented by both loops |
    /// | `t == 1.0` | moved and tested | a comparison against `1.0` short-circuits **straight to the bisection** — that is `adjust_sphere_to_poly`'s "already interpenetrating" answer |
    ///
    /// The assignment is the load-bearing one: inverted, it makes `lower` the largest *hitting*
    /// `t`, so the function returns `true` having placed the sphere **inside** the surface. The
    /// caller then re-tests, hits again, and transition insertion's three-by-three
    /// attempt budget runs out — a full rollback of the move instead of a glide along the wall.
    ///
    /// The brackets are `double` in the client, and each component of the current position plus
    /// `movement * t` is computed in extended precision and stored back as a float, which is what
    /// the `f64` scalars and per-component multiply below reproduce.
    pub fn adjust_to_plane(
        &self,
        sphere: &mut Sphere,
        cur_pos: Vec3,
        hit_poly: &mut Option<u32>,
        contact_point: &mut Vec3,
    ) -> bool {
        /// The current position plus `movement * t` with `t` a double, as the client computes it.
        fn at(cur_pos: Vec3, movement: Vec3, t: f64) -> Vec3 {
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the client multiplies by a double `t` and stores the result as a float —
            // the narrowing is the arithmetic, not a value conversion.
            Vec3::new(
                (f64::from(cur_pos.x) + f64::from(movement.x) * t) as f32,
                (f64::from(cur_pos.y) + f64::from(movement.y) * t) as f32,
                (f64::from(cur_pos.z) + f64::from(movement.z) * t) as f32,
            )
        }

        let movement = sphere.center.sub(cur_pos);
        let mut lower = 0.0_f64;
        let mut upper = 1.0_f64;
        let mut i = 0_u32;
        loop {
            let poly = match *hit_poly {
                Some(p) => &self.polygons[p as usize],
                None => return false,
            };
            let t = poly.adjust_sphere_to_poly(sphere.radius, cur_pos, movement);
            if t == 1.0 {
                // Already interpenetrating: bisect the whole bracket instead.
                break;
            }
            sphere.center = at(cur_pos, movement, f64::from(t));
            if !self.sphere_intersects_poly(sphere, movement, hit_poly, contact_point) {
                lower = f64::from(t);
                break;
            }
            upper = f64::from(t);
            i += 1;
            if i >= 15 {
                return false;
            }
        }
        while i < 15 {
            let mid = (lower + upper) * 0.5;
            sphere.center = at(cur_pos, movement, mid);
            if self.sphere_intersects_poly(sphere, movement, hit_poly, contact_point) {
                upper = mid;
            } else {
                lower = mid;
            }
            if upper - lower < 0.02 {
                break;
            }
            i += 1;
        }
        sphere.center = at(cur_pos, movement, lower);
        true
    }

    /// Every node a point query visits, in visit order, ending at the leaf it lands in.
    /// Test-only instrumentation: the original has no such function, and a BSP traversal test
    /// compares this against an independent walk of the same tree.
    #[must_use]
    pub fn leaf_path_for_point(&self, point: Vec3) -> Vec<u32> {
        let mut path = Vec::new();
        let mut cursor = self.root_index();
        while let Some(idx) = cursor {
            path.push(idx);
            let n = self.node(idx);
            if n.is_leaf() {
                break;
            }
            cursor = n.child_towards(point);
        }
        path
    }

    /// The deepest path in the tree, used to prove that the traversals cannot run away on retail
    /// data.
    #[must_use]
    pub fn max_depth(&self) -> usize {
        let mut best = 0_usize;
        let mut work: Vec<(u32, usize)> = self.root_index().map(|r| (r, 1)).into_iter().collect();
        while let Some((idx, depth)) = work.pop() {
            best = best.max(depth);
            let n = self.node(idx);
            work.extend(n.pos_child.map(|c| (c, depth + 1)));
            work.extend(n.neg_child.map(|c| (c, depth + 1)));
        }
        best
    }

    /// True when every interior node has both children — the condition under which the client's
    /// unguarded child dispatch is safe and [`MISSING_CHILD_IS_EMPTY`] never applies.
    #[must_use]
    pub fn interior_nodes_have_both_children(&self) -> bool {
        self.nodes
            .iter()
            .all(|n| n.is_leaf() || (n.pos_child.is_some() && n.neg_child.is_some()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These pin the behaviour contract in the compatibility requirements: the constants
    // (`EPSILON`, `CELL_BSP_FUDGE`, the fifteen-iteration budget, the `0.02` bracket), the
    // three-way split and which child each of its outcomes visits, the leaf tests, and the two
    // places the client's answer is the permissive one.

    /// A hand-built tree: one splitting plane at z = 0, a solid leaf below it holding a single
    /// square polygon, and an empty leaf above.
    fn two_leaf_tree() -> BspTree {
        let floor = Polygon::new(vec![
            Vec3::new(-5.0, -5.0, 0.0),
            Vec3::new(5.0, -5.0, 0.0),
            Vec3::new(5.0, 5.0, 0.0),
            Vec3::new(-5.0, 5.0, 0.0),
        ]);
        let big = Sphere::new(Vec3::ZERO, 100.0);
        BspTree {
            nodes: vec![
                BspNode {
                    sphere: big,
                    splitting_plane: Plane {
                        normal: Vec3::new(0.0, 0.0, 1.0),
                        d: 0.0,
                    },
                    pos_child: Some(1),
                    neg_child: Some(2),
                    kind: BspNodeKind::Node,
                    in_polys: vec![],
                },
                // The leaves carry a plane too: the cell-BSP walks do not special-case a leaf,
                // so a degenerate plane here would change what they report. Real leaves have a
                // real plane; these are placed far in front of everything.
                BspNode {
                    sphere: big,
                    splitting_plane: Plane {
                        normal: Vec3::new(0.0, 0.0, 1.0),
                        d: 1000.0,
                    },
                    pos_child: None,
                    neg_child: None,
                    kind: BspNodeKind::Leaf {
                        leaf_index: 0,
                        solid: false,
                    },
                    in_polys: vec![],
                },
                BspNode {
                    sphere: big,
                    splitting_plane: Plane {
                        normal: Vec3::new(0.0, 0.0, 1.0),
                        d: 1000.0,
                    },
                    pos_child: None,
                    neg_child: None,
                    kind: BspNodeKind::Leaf {
                        leaf_index: 1,
                        solid: true,
                    },
                    in_polys: vec![0],
                },
            ],
            polygons: vec![floor],
        }
    }

    /// Difference 1 of 2 between the client and the reimplementations: exactly touching spheres
    /// intersect. Some reimplementations test `|d|² < sum²`; the client's inclusive form —
    /// `|d|² - sum² < 2e-4` — is what this pins, and it is very slightly more permissive.
    #[test]
    fn spheres_that_exactly_touch_count_as_intersecting() {
        let a = Sphere::new(Vec3::ZERO, 1.0);
        // Exactly touching: |d|^2 - sum^2 == 0, which is < 2e-4, so the client says yes.
        let b = Sphere::new(Vec3::new(2.0, 0.0, 0.0), 1.0);
        assert!(spheres_intersect(&a, &b));
        let far = Sphere::new(Vec3::new(2.01, 0.0, 0.0), 1.0);
        assert!(!spheres_intersect(&a, &far));
    }

    #[test]
    fn point_intersects_solid_descends_by_the_splitting_plane() {
        let t = two_leaf_tree();
        // Above the plane: the empty leaf, which has no polygons.
        assert!(!t.point_intersects_solid(Vec3::new(0.0, 0.0, 5.0)));
        // Below: the leaf holding the floor polygon.
        assert!(t.point_intersects_solid(Vec3::new(0.0, 0.0, -5.0)));
        // Exactly on the plane: `> 0.0` is false, so it goes negative.
        assert!(t.point_intersects_solid(Vec3::ZERO));
    }

    #[test]
    fn point_inside_cell_bsp_walks_only_the_positive_chain() {
        let t = two_leaf_tree();
        // Front of the plane -> positive child (a leaf with no positive child) -> inside.
        assert!(t.point_inside_cell_bsp(Vec3::new(0.0, 0.0, 5.0)));
        // Behind -> immediately outside.
        assert!(!t.point_inside_cell_bsp(Vec3::new(0.0, 0.0, -5.0)));
        // Within the 2e-4 band -> Crossing, which is treated as inside.
        assert!(t.point_inside_cell_bsp(Vec3::new(0.0, 0.0, 0.000_1)));
    }

    #[test]
    fn sphere_intersects_cell_bsp_reports_all_three_bounding_types() {
        let t = two_leaf_tree();
        // Well in front: the positive child is a leaf with no children -> ENTIRELY_INSIDE.
        assert_eq!(
            t.sphere_intersects_cell_bsp(&Sphere::new(Vec3::new(0.0, 0.0, 5.0), 1.0)),
            Bounding::EntirelyInside
        );
        // Well behind: OUTSIDE.
        assert_eq!(
            t.sphere_intersects_cell_bsp(&Sphere::new(Vec3::new(0.0, 0.0, -5.0), 1.0)),
            Bounding::Outside
        );
        // Straddling: PARTIALLY_INSIDE.
        assert_eq!(
            t.sphere_intersects_cell_bsp(&Sphere::new(Vec3::ZERO, 1.0)),
            Bounding::PartiallyInside
        );
    }

    #[test]
    fn sphere_intersects_cell_bsp_uses_the_hundredth_fudge_not_the_epsilon() {
        let t = two_leaf_tree();
        // A sphere of radius 1 centred 1.005 behind the plane: dist = -1.005, checkRad = 1.01,
        // so -1.005 > -1.01 and it is PARTIALLY_INSIDE. With the 2e-4 epsilon it would be
        // OUTSIDE. That difference is the point of the constant.
        assert_eq!(
            t.sphere_intersects_cell_bsp(&Sphere::new(Vec3::new(0.0, 0.0, -1.005), 1.0)),
            Bounding::PartiallyInside
        );
        assert_eq!(
            t.sphere_intersects_cell_bsp(&Sphere::new(Vec3::new(0.0, 0.0, -1.02), 1.0)),
            Bounding::Outside
        );
    }

    #[test]
    fn sphere_intersects_solid_finds_the_floor_polygon() {
        let t = two_leaf_tree();
        // A sphere sitting on the floor, straddling the splitting plane.
        assert!(t.sphere_intersects_solid(&Sphere::new(Vec3::new(0.0, 0.0, 0.0), 1.0), false));
        // Far above: no.
        assert!(!t.sphere_intersects_solid(&Sphere::new(Vec3::new(0.0, 0.0, 50.0), 1.0), false));
        // Far to the side, still below the plane but off the polygon: no.
        assert!(!t.sphere_intersects_solid(&Sphere::new(Vec3::new(50.0, 0.0, -1.0), 1.0), false));
    }

    #[test]
    fn sphere_intersects_solid_honours_center_check_on_a_solid_leaf() {
        let mut t = two_leaf_tree();
        // Give the solid leaf a polygon list but move it out of reach: with center_check the
        // solid flag alone answers "yes", without it the geometry has to.
        t.nodes[2].sphere = Sphere::new(Vec3::new(1000.0, 0.0, 0.0), 1.0);
        let s = Sphere::new(Vec3::new(0.0, 0.0, -5.0), 1.0);
        assert!(
            t.sphere_intersects_solid(&s, true),
            "the solid leaf answers on its flag"
        );
        assert!(
            !t.sphere_intersects_solid(&s, false),
            "without center_check the geometry decides"
        );
    }

    #[test]
    fn sphere_intersects_poly_names_the_polygon_and_requires_inward_movement() {
        let t = two_leaf_tree();
        let s = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 1.0);
        let mut hit = None;
        let mut cp = Vec3::ZERO;
        assert!(t.sphere_intersects_poly(&s, Vec3::new(0.0, 0.0, -1.0), &mut hit, &mut cp));
        assert_eq!(hit, Some(0));
        let mut hit2 = None;
        assert!(!t.sphere_intersects_poly(&s, Vec3::new(0.0, 0.0, 1.0), &mut hit2, &mut cp));
        assert_eq!(
            hit2,
            Some(0),
            "a back-facing overlap returns 0 with the polygon recorded"
        );
        // And a sphere that touches nothing records nothing, so the assertion above is about the
        // direction and not about the walk visiting the leaf.
        let far = Sphere::new(Vec3::new(0.0, 0.0, 40.0), 1.0);
        let mut hit3 = None;
        assert!(!t.sphere_intersects_poly(&far, Vec3::new(0.0, 0.0, -1.0), &mut hit3, &mut cp));
        assert_eq!(hit3, None);
    }

    /// Adjust to plane places the sphere exactly on the surface.
    #[test]
    fn adjust_to_plane_places_the_sphere_exactly_on_the_surface() {
        let t = two_leaf_tree();
        let cur = Vec3::new(0.0, 0.0, 3.0);
        let mut s = Sphere::new(Vec3::new(0.0, 0.0, -1.0), 1.0);
        let mut hit = Some(0);
        let mut cp = Vec3::ZERO;
        assert!(t.adjust_to_plane(&mut s, cur, &mut hit, &mut cp));
        assert!(
            (s.center.z - 1.0).abs() < 0.05,
            "the sphere should rest one radius above the floor, not at {}",
            s.center.z
        );
    }

    /// The `fcom` against `1.0`:
    /// answers **1.0** for a sphere that already interpenetrates, and `adjust_to_plane`
    /// short-circuits straight to the bisection rather than moving to `t = 1.0` and testing there.
    ///
    /// Without that short-circuit the loop re-derives the same `1.0` every pass, exhausts its
    /// fifteen-iteration budget and returns **false** — the caller then reports a hard collision
    /// where the client reports an adjustment.
    #[test]
    fn adjust_to_plane_short_circuits_a_sphere_that_already_interpenetrates() {
        let t = two_leaf_tree();
        let cur = Vec3::new(0.0, 0.0, 0.5);
        let mut s = Sphere::new(Vec3::new(0.0, 0.0, 0.2), 1.0);
        let mut hit = Some(0);
        let mut cp = Vec3::ZERO;
        assert!(
            t.adjust_to_plane(&mut s, cur, &mut hit, &mut cp),
            "the client adjusts, not collides"
        );
        assert!(
            (s.center.z - cur.z).abs() < 0.02,
            "with nowhere clear to go the sphere stays at `curPos`, not at {}",
            s.center.z
        );
    }

    #[test]
    fn hits_walkable_requires_the_up_vector_to_clear_the_allowance() {
        let t = two_leaf_tree();
        let s = Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0);
        let up = Vec3::new(0.0, 0.0, 1.0);
        assert!(t.hits_walkable(&s, up, crate::globals::LANDING_Z));
        // An allowance above 1.0 can never be cleared by a unit up vector on a flat floor.
        assert!(!t.hits_walkable(&s, up, 1.5));
    }

    #[test]
    fn find_walkable_moves_the_sphere_onto_the_floor_and_records_the_polygon() {
        let t = two_leaf_tree();
        let mut s = Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0);
        let mut hit = None;
        let mut interp = 1.0_f32;
        let mut changed = false;
        t.find_walkable(
            &mut s,
            &mut hit,
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
            crate::globals::LANDING_Z,
            &mut interp,
            &mut changed,
        );
        assert!(changed, "the sphere must be reported as adjusted");
        assert_eq!(hit, Some(0));
        assert!(interp < 1.0, "walk_interp is consumed: {interp}");
        assert!(
            (s.center.z - 1.0).abs() < 1e-4,
            "resting on the floor: {:?}",
            s.center
        );
    }

    #[test]
    fn traversal_terminates_on_a_deep_degenerate_chain() {
        // 5000 interior nodes chained through pos_child, ending in a leaf. Every traversal must
        // finish; this is the cheap stand-in for "no cycles, no unbounded recursion".
        let mut nodes = Vec::new();
        for i in 0..5000_u32 {
            nodes.push(BspNode {
                sphere: Sphere::new(Vec3::ZERO, 1000.0),
                splitting_plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: -f32::from(u16::try_from(i).unwrap_or(0)),
                },
                pos_child: Some(i + 1),
                neg_child: Some(5000),
                kind: BspNodeKind::Node,
                in_polys: vec![],
            });
        }
        nodes.push(BspNode {
            sphere: Sphere::new(Vec3::ZERO, 1000.0),
            splitting_plane: Plane::default(),
            pos_child: None,
            neg_child: None,
            kind: BspNodeKind::Leaf {
                leaf_index: 0,
                solid: false,
            },
            in_polys: vec![],
        });
        let t = BspTree {
            nodes,
            polygons: vec![],
        };
        assert_eq!(t.max_depth(), 5001);
        assert!(t.point_inside_cell_bsp(Vec3::new(0.0, 0.0, 1e9)));
        assert!(!t.point_intersects_solid(Vec3::new(0.0, 0.0, -1e9)));
        assert_eq!(t.leaf_path_for_point(Vec3::new(0.0, 0.0, -1.0)).len(), 2);
    }

    #[test]
    fn empty_tree_is_inert() {
        let t = BspTree::default();
        assert!(t.point_inside_cell_bsp(Vec3::ZERO));
        assert!(!t.point_intersects_solid(Vec3::ZERO));
        assert!(!t.sphere_intersects_solid(&Sphere::new(Vec3::ZERO, 1.0), true));
        assert_eq!(
            t.sphere_intersects_cell_bsp(&Sphere::new(Vec3::ZERO, 1.0)),
            Bounding::EntirelyInside
        );
        assert_eq!(t.max_depth(), 0);
    }
}
