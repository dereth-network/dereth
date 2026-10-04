//! Portal traversal and the indoor path.
//!
//! The indoor portal view: drawing inside a cell, constructing the view, initialising a cell,
//! the to-do list, clipping the portals, adding a view to them, the seen flags, the draw-list
//! adjustment, the cell placement and the cell draw itself.
//!
//! The indoor path preserves portal traversal and frame ordering.
//!
//! Two things here are visible behaviour rather than optimisation, and both are asserted as
//! *present* rather than optimised away:
//!
//! * **the Z clear plus the portal depth stamps** — without them the outdoors seen through a
//!   doorway z-fights with the interior;
//! * the far→near ordering of `cell_draw_list`, which is what the alpha list inherits.
//!
//! What is **not** here: the view copy's screen-space polygon clipping and its 1-pixel /
//! collinearity simplification. Those need the projection matrix, which is the renderer's. This module
//! owns the graph traversal and the ordering, and takes clipping as a predicate.
//!
//! # The outdoor half
//!
//! Everything above is the path a viewer *inside* a building takes. A viewer outside one reaches
//! the same interior cells through the building's own drawing BSP, and that is the second half of
//! this module: [`build_draw_portals_only`], [`draw_portal`], and the building-view traversal.
//!
//! The building path points the outdoor view's portal list at the landblock's `BuildInfo::portals`
//! and runs the building's drawing BSP twice, with `mode = 1` then `mode = 2`. Each portal node
//! passes its indexed polygon to the portal draw step.
//!
//! **The outdoor view is constructed with the draw-landscape flag clear**; the indoor view sets it,
//! and the portal clip only ever appends to `outside_view` when it is set. So on the
//! building path the cell draw always sees `outside_view.view_count == 0` and its first four steps —
//! the outdoor pass, the flush, the Z clear and the portal depth stamps — **never run at all**.
//! The outdoor half's depth repair is a different mechanism: the `mode = 1` pass, which stamps
//! every visible portal opening at a *constant* device depth of 0.999999. See [`PortalMode`].

use std::collections::BTreeMap;

use dereth_assets::common::BspTree;
use dereth_primitives::{CellId, Vec3};

use crate::Plane;

/// One portal of an env cell, as the traversal walks it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellPortal {
    /// Which side of the portal plane the owning cell is on, 0 or 1.
    pub portal_side: u8,
    /// `0xFFFFFFFF` means the portal opens to the **outdoors**.
    pub other_cell_id: u32,
    /// The matching portal index on the far side, or negative for none.
    pub other_portal_id: i32,
    /// When false, the two sides of the portal do not coincide and the other side's portal clip must run.
    pub exact_match: bool,
    /// The squared distances from the viewpoint to each of the portal polygon's vertices, which
    /// `init_cell` folds into `max_indist`.
    pub vertex_dist_sq: [f32; 4],
    /// `plane.N · viewpoint + plane.d`, in cell space. `init_cell` turns it into `inflag`.
    pub viewpoint_side_distance: f32,
}

/// `other_cell_id` for a portal that opens outdoors.
pub const OUTDOORS: u32 = 0xFFFF_FFFF;

/// Per-cell traversal state held at the top of the portal-view stack.
#[derive(Debug, Clone, Default)]
pub struct PortalView {
    /// How many view polygons this cell has accumulated. A cell reachable through several portals
    /// gets several, and its contents are tested against all of them.
    pub view_count: usize,
    pub update_count: usize,
    pub cell_view_done: bool,
    /// The largest squared distance from the viewpoint to any vertex of any portal through which
    /// this cell was entered. The traversal's sort key.
    pub max_indist: f32,
    /// Per portal: "faces the viewer and could be entered".
    pub inflag: Vec<bool>,
    /// Per portal: "already handled for the current set of view polygons".
    pub seen: Vec<bool>,
}

/// One env cell as far as the traversal is concerned.
#[derive(Debug, Clone)]
pub struct TraversalCell {
    pub id: CellId,
    pub portals: Vec<CellPortal>,
}

/// The result of one `construct_view`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConstructedView {
    /// `cell_draw_list`, **nearest first**. The cell draw walks it backwards, so the drawing order
    /// is far→near.
    pub cell_draw_list: Vec<CellId>,
    /// How many view polygons `outside_view` accumulated. Non-zero means some portal chain reached
    /// the outdoors, which is what triggers the outdoor pass, the Z clear and the depth stamps.
    pub outside_view_count: usize,
    /// `outside_view.view[0 .. view_count]`, the screen outlines themselves.
    ///
    /// The outdoor arm of clipping the portals:
    ///
    /// ```text
    ///   get_clip(...)                  ; the portal's screen-space clip
    ///   if (nothing is left of the opening) contribute nothing
    ///   if (other_cell_id == 0xFFFFFFFF and the mode word is 0) take the stamp arm
    ///   else copy the clipped screen points into outside_view
    /// ```
    ///
    /// That mode word is **1** in the shipped client and is only ever read — by the test
    /// above — so the arm that ships copies the clipped screen points into `outside_view`:
    /// `outside_view` carries the **clipped opening outline**, not the stamp arm's
    /// full-screen default (a view copy with no points). The mesh draw's portal arm then
    /// cone-tests every mesh of the outdoor pass against these, with the answers combined by
    /// [`crate::objects::draw::draw_mesh_view_list`]).
    ///
    /// **Empty on [`construct_view_from`]**, which takes clipping as a `bool` and has no polygons
    /// to give — and empty is *not* "no views", because `outside_view_count` is the separate
    /// statement. A caller that has a count but no polygons is the unclipped traversal and must
    /// keep treating the outdoor pass as full-screen.
    pub outside_views: Vec<crate::cells::clip::ViewPoly>,
    /// The last portal view for each reached cell: the screen
    /// outlines *that* cell is seen through.
    ///
    /// All three of the cell draw's interior loops read it, and each reads the
    /// **top of the stack**. Every loop sets up view index `v` by making the top portal view's
    /// polygon array (the last entry of `portal_view`) the current view.
    ///
    /// * Loop 1, the indoor portal stamps: set up the view, then draw only the portals whose
    ///   `other_cell_id` is `0xFFFFFFFF` with `draw_portal_poly(poly, false)` (mask 6).
    /// * Loop 2, the cell meshes: skip a cell that has no views, set up the view, draw the env
    ///   cell.
    /// * Loop 3, the cell objects: take the top portal view and draw the cell's objects.
    ///
    /// Loop 3 is the one that culls: everything it submits goes down
    /// the mesh draw's **portal** arm and is tested against these
    /// polygons before the selection-ray test. Loop 2 does **not** -- the environment-cell draw
    /// classifies subsets and never
    /// calls the view-cone test -- so a cell's own mesh is drawn once per view with no test, which is
    /// overdraw of identical geometry and not a cull.
    ///
    /// **Empty on [`construct_view_from`]**, the unclipped traversal, which has no polygons to give.
    pub cell_views: BTreeMap<u32, Vec<crate::cells::clip::ViewPoly>>,
}

impl ConstructedView {
    /// Cell-draw iteration order: **far to near**, i.e. `cell_draw_list` reversed.
    #[must_use]
    pub fn draw_order(&self) -> Vec<CellId> {
        self.cell_draw_list.iter().rev().copied().collect()
    }
}

/// Prepare a cell the first time it is reached.
///
/// ```text
/// for p in 0 .. num_portals-1:
///     if p == entered_portal and inflag[p] == 0: inflag[p] = 1 ; seen[p] = 1
///     else:
///         seen[p] = 0
///         d = portal.plane.N · viewpoint + plane.d
///         side = (d > 0.0002) ? 0 : (d < -0.0002 ? 1 : IN_PLANE)
///         inflag[p] = (side != IN_PLANE and side != portal_side) ? 1 : 0
///     if inflag[p]: max_indist = max over the portal's vertices of |viewpoint - v|²
/// ```
///
/// `inflag` means "this portal faces the viewer and could be entered"; `side != portal_side` is
/// exactly "the viewer is on the far side, looking in".
pub fn init_cell(cell: &TraversalCell, entered_portal: Option<usize>, pv: &mut PortalView) -> bool {
    if pv.view_count == 0 {
        return false; // nothing sees this cell
    }
    pv.cell_view_done = false;
    pv.inflag.resize(cell.portals.len(), false);
    pv.seen.resize(cell.portals.len(), false);
    let mut max_indist = 0.0f32;
    for (p, portal) in cell.portals.iter().enumerate() {
        if entered_portal == Some(p) && !pv.inflag[p] {
            pv.inflag[p] = true;
            pv.seen[p] = true;
        } else {
            pv.seen[p] = false;
            let d = portal.viewpoint_side_distance;
            let side = if d > crate::consts::EPSILON {
                Some(0u8)
            } else if d < -crate::consts::EPSILON {
                Some(1u8)
            } else {
                None // IN_PLANE
            };
            pv.inflag[p] = side.is_some_and(|s| s != portal.portal_side);
        }
        if pv.inflag[p] {
            for &d2 in &portal.vertex_dist_sq {
                max_indist = max_indist.max(d2);
            }
        }
    }
    pv.max_indist = max_indist;
    pv.update_count = pv.view_count;
    true
}

/// Insert into the to-do list keeping it **sorted by descending
/// `dist`**. `construct_view` pops the **last** entry, so cells are visited **nearest first**.
pub fn ins_cell_todo_list(todo: &mut Vec<(CellId, f32)>, cell: CellId, dist: f32) {
    let i = todo.partition_point(|&(_, d)| d > dist);
    todo.insert(i, (cell, dist));
}

/// The breadth-first traversal.
///
/// ```text
/// outside_view.view_count = 0 ; master_timestamp++ ; cell_todo_num = cell_draw_num = 0
/// init_cell(cell, entered_portal) ; ins_cell_todo_list(cell, 0.0)
/// while cell_todo_num > 0:
///     pop the LAST entry of cell_todo_list          // sorted descending, so this is the nearest
///     append it to cell_draw_list ; cell_view_done = 1
///     if clip the exit portals of c: hand the views to the neighbours of c
/// ```
///
/// `clip` stands in for the portal clip, `get_clip` and the view copy: it answers "does the view
/// polygon survive the clip through this portal", which is the part that needs the projection and
/// belongs to the renderer. Returning `true` unconditionally gives the unclipped traversal, which visits
/// a superset of the cells and in the same order.
pub fn construct_view(
    cells: &BTreeMap<u32, TraversalCell>,
    start: CellId,
    clip: &dyn Fn(CellId, usize) -> bool,
) -> ConstructedView {
    construct_view_from(cells, start, None, None, clip)
}

/// The breadth-first traversal with the entry portal and the live
/// `portal_view` set named.
///
/// [`construct_view`] passes the client's `0xFFFF` sentinel ("none") and no stab list, which is
/// what the client does for a viewer standing in the cell. The **building**
/// path names both:
///
/// * the entry portal, because the building overload's tail is
///   `construct_view(other, portal.other_portal_id)`, and that portal must be marked `inflag`
///   and `seen` by [`init_cell`] so the traversal does not immediately walk back out of the window
///   it came in through;
/// * `live`, the cells `portal.stab_list` pushes views for. Only a cell that has had a view
///   pushed on it has a `portal_view` at all, and both the portal clip and the view hand-off refuse a
///   neighbour with no views — so the **stab list bounds how far the traversal can go**.
///   `None` means "every cell is live", which is the unbounded traversal: a superset, in the same
///   order.
pub fn construct_view_from(
    cells: &BTreeMap<u32, TraversalCell>,
    start: CellId,
    entered_portal: Option<usize>,
    live: Option<&std::collections::BTreeSet<u32>>,
    clip: &dyn Fn(CellId, usize) -> bool,
) -> ConstructedView {
    let mut views: BTreeMap<u32, PortalView> = BTreeMap::new();
    let mut out = ConstructedView::default();
    let Some(first) = cells.get(&start.0) else {
        return out;
    };

    let mut pv = PortalView {
        view_count: 1,
        ..PortalView::default()
    };
    if !init_cell(first, entered_portal, &mut pv) {
        return out;
    }
    views.insert(start.0, pv);

    let mut todo: Vec<(CellId, f32)> = Vec::new();
    ins_cell_todo_list(&mut todo, start, 0.0);

    // The client's guard against a pathological portal graph; the traversal is bounded by the cell
    // count in practice because a cell is only re-queued when it gains a new view polygon.
    let mut guard = cells.len() * 8 + 16;
    while let Some((id, _)) = todo.pop() {
        guard = guard.saturating_sub(1);
        if guard == 0 {
            break;
        }
        let Some(c) = cells.get(&id.0) else { break };
        out.cell_draw_list.push(id);
        if let Some(pv) = views.get_mut(&id.0) {
            pv.cell_view_done = true;
        }

        // The portal clip: "each portal that is `seen` but not `inflag` (i.e. an exit portal not yet
        // propagated)". `init_cell`'s tail sets `seen` on every `inflag == 0` portal, so the exits
        // are exactly the portals with **inflag == 0** — the ones whose plane the viewer is on the
        // cell's own side of, which is what "looking out through it" means. The entry portal has
        // `inflag == 1` and is therefore never re-propagated.
        let mut propagate: Vec<CellPortal> = Vec::new();
        for (p, portal) in c.portals.iter().enumerate() {
            let inflag = views
                .get(&id.0)
                .is_some_and(|v| v.inflag.get(p).copied().unwrap_or(false));
            if inflag {
                continue; // the way in, not a way out
            }
            if !clip(id, p) {
                continue; // clipped away entirely
            }
            if portal.other_cell_id == OUTDOORS {
                out.outside_view_count += 1;
                continue;
            }
            propagate.push(*portal);
        }

        // The view hand-off: give the clipped polygon to each neighbour.
        for portal in propagate {
            let Some(neighbour) = cells.get(&portal.other_cell_id) else {
                continue;
            };
            // The stab-list push gave a `portal_view` only to the stab cells; a neighbour without one is
            // skipped by the hand-off's has-a-view test, and the traversal stops
            // there.
            if live.is_some_and(|l| !l.contains(&portal.other_cell_id)) {
                continue;
            }
            let entry = views.entry(portal.other_cell_id).or_default();
            let first_visit = entry.update_count == 0;
            entry.view_count += 1;
            if first_visit {
                let mut pv = std::mem::take(entry);
                let entered = usize::try_from(portal.other_portal_id).ok();
                if init_cell(neighbour, entered, &mut pv) {
                    let d = pv.max_indist;
                    views.insert(portal.other_cell_id, pv);
                    ins_cell_todo_list(&mut todo, CellId(portal.other_cell_id), d);
                } else {
                    views.insert(portal.other_cell_id, pv);
                }
            } else if entry.update_count < entry.view_count {
                // add_to_cell: the neighbour already has views but gained a new one. If it was
                // already drawn, the draw-list adjustment moves it *before* the cell it was entered from, so
                // the far->near invariant survives a shorter route found later.
                entry.update_count = entry.view_count;
                if entry.cell_view_done {
                    adjust_draw_list(&mut out.cell_draw_list, CellId(portal.other_cell_id), id);
                }
            }
            // set_other_seen: mark the matching portal on the other side as seen, so the traversal
            // does not immediately come back through it.
            if let (Ok(op), Some(v)) = (
                usize::try_from(portal.other_portal_id),
                views.get_mut(&portal.other_cell_id),
            ) {
                if op < v.seen.len() {
                    v.seen[op] = true;
                }
            }
        }
    }
    out
}

/// Construct a view with the screen-space clip actually performed.
///
/// [`construct_view_from`] takes clipping as a `bool` predicate; answered "visible" throughout,
/// it walks a **superset** of the cells, in the same order. This is the same traversal
/// with the real clip body — compute the opening, clip its polygon, then copy the resulting view —
/// so a cell is reached only while some chain of openings still has a hole left in it on the screen.
///
/// * `initial` is the view polygon the starting cell is seen through. Indoors that is
///   the default view's full-screen quad (`ViewPoly::full_screen`); on the **building** path it
///   is the opening's own clipped outline, which is what the client handed to
///   view copy into the other cell's top `portal_view` before recursing.
/// * `project` is the client's vertex transform for each vertex of cell `c`'s portal `p`: the
///   portal polygon in screen-space homogeneous coordinates, **unclipped and unreversed**. It is the
///   caller's because the projection matrix is the renderer's and the cell's placement is the client's.
///
/// A cell reachable through several portals accumulates several view polygons, exactly as
/// `portal_view.view` does, and its exits are clipped against every one of them.
pub fn construct_view_clipped(
    cells: &BTreeMap<u32, TraversalCell>,
    start: CellId,
    entered_portal: Option<usize>,
    live: Option<&std::collections::BTreeSet<u32>>,
    initial: &crate::cells::clip::ViewPoly,
    eye: &crate::cells::clip::EyeTransform,
    project: &dyn Fn(CellId, usize) -> Vec<crate::cells::clip::ScreenPoint>,
) -> ConstructedView {
    use crate::cells::clip::{copy_view, get_clip, ViewPoly};

    let mut out = ConstructedView::default();
    if initial.is_empty() {
        return out;
    }
    let Some(first) = cells.get(&start.0) else {
        return out;
    };

    // `portal_view.view` per cell: the screen outlines it is visible through.
    let mut views: BTreeMap<u32, Vec<ViewPoly>> = BTreeMap::new();
    let mut state: BTreeMap<u32, PortalView> = BTreeMap::new();

    let mut pv = PortalView {
        view_count: 1,
        ..PortalView::default()
    };
    if !init_cell(first, entered_portal, &mut pv) {
        return out;
    }
    state.insert(start.0, pv);
    views.insert(start.0, vec![initial.clone()]);

    let mut todo: Vec<(CellId, f32)> = Vec::new();
    ins_cell_todo_list(&mut todo, start, 0.0);

    let mut guard = cells.len() * 8 + 16;
    while let Some((id, _)) = todo.pop() {
        guard = guard.saturating_sub(1);
        if guard == 0 {
            break;
        }
        let Some(c) = cells.get(&id.0) else { break };
        out.cell_draw_list.push(id);
        if let Some(pv) = state.get_mut(&id.0) {
            pv.cell_view_done = true;
        }

        // The portal clip: every view polygon of this cell, against every exit portal.
        let here = views.get(&id.0).cloned().unwrap_or_default();
        let mut propagate: Vec<(CellPortal, ViewPoly)> = Vec::new();
        for (p, portal) in c.portals.iter().enumerate() {
            let inflag = state
                .get(&id.0)
                .is_some_and(|v| v.inflag.get(p).copied().unwrap_or(false));
            if inflag {
                continue; // the way in, not a way out
            }
            let screen = project(id, p);
            if screen.len() < 3 {
                continue;
            }
            for view in &here {
                // `get_clip(portal_side, portal polygon, out, &n, 1)`, then `if n == 0: skip`.
                let clipped = get_clip(&screen, portal.portal_side != 0, view);
                let Some(next) = copy_view(&clipped, eye) else {
                    continue;
                };
                if portal.other_cell_id == OUTDOORS {
                    // The outdoor arm: copy the clipped screen points into `outside_view` — the clipped outline
                    // itself, because the mode word is 1 in the shipped client.
                    out.outside_view_count += 1;
                    out.outside_views.push(next);
                    continue;
                }
                propagate.push((*portal, next));
            }
        }

        // Add the view to the portals of the cell just processed.
        for (portal, view) in propagate {
            let Some(neighbour) = cells.get(&portal.other_cell_id) else {
                continue;
            };
            if live.is_some_and(|l| !l.contains(&portal.other_cell_id)) {
                continue;
            }
            views.entry(portal.other_cell_id).or_default().push(view);
            let entry = state.entry(portal.other_cell_id).or_default();
            let first_visit = entry.update_count == 0;
            entry.view_count += 1;
            if first_visit {
                let mut pv = std::mem::take(entry);
                let entered = usize::try_from(portal.other_portal_id).ok();
                if init_cell(neighbour, entered, &mut pv) {
                    let d = pv.max_indist;
                    state.insert(portal.other_cell_id, pv);
                    ins_cell_todo_list(&mut todo, CellId(portal.other_cell_id), d);
                } else {
                    state.insert(portal.other_cell_id, pv);
                }
            } else if entry.update_count < entry.view_count {
                entry.update_count = entry.view_count;
                if entry.cell_view_done {
                    adjust_draw_list(&mut out.cell_draw_list, CellId(portal.other_cell_id), id);
                }
            }
            if let (Ok(op), Some(v)) = (
                usize::try_from(portal.other_portal_id),
                state.get_mut(&portal.other_cell_id),
            ) {
                if op < v.seen.len() {
                    v.seen[op] = true;
                }
            }
        }
    }
    // `views` holds the last portal view: the polygons each cell
    // was reached through, which installs one at a time and which
    // the cell draw's three interior loops read. Only the cells the walk actually drew are published:
    // an entry for a cell that never made `cell_draw_list` would be a view for a cell no loop runs.
    out.cell_views = out
        .cell_draw_list
        .iter()
        .filter_map(|id| views.get(&id.0).map(|v| (id.0, v.clone())))
        .collect();
    out
}

/// Move `cell` so it appears **before** the cell it
/// was entered from, inserting it if absent. Returns whether it moved.
pub fn adjust_draw_list(list: &mut Vec<CellId>, cell: CellId, entered_from: CellId) -> bool {
    let Some(target) = list.iter().position(|&c| c == entered_from) else {
        return false;
    };
    match list.iter().position(|&c| c == cell) {
        Some(i) if i < target => false,
        Some(i) => {
            let v = list.remove(i);
            let target = list.iter().position(|&c| c == entered_from).unwrap_or(0);
            list.insert(target, v);
            true
        }
        None => {
            list.insert(target, cell);
            true
        }
    }
}

/// One step of the indoor frame, in the client's order.
///
/// The sequence is the indoor half of the frame order and is asserted as a *sequence*, because the Z clear
/// and the depth stamps are visible behaviour rather than an optimisation: without them the
/// outdoors seen through a doorway z-fights with the interior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndoorStep {
    /// Enable sunlight, point the portal list at `outside_view`, and draw the landscape — the whole outdoor path,
    /// clipped to the portal polygons.
    OutdoorsThroughPortals,
    /// Flush the alpha list at 0.0 and bump the frame stamp.
    FlushBeforeClear,
    /// The render device's clear with flag `4` — **the Z buffer only**.
    ZClear,
    /// `draw_portal_poly` for every portal whose `other_cell_id == -1`, far→near: stamps the
    /// portal's depth back into the cleared Z buffer.
    PortalDepthStamps,
    /// `use_sunlight_set(0)` — every hardware slot disabled — then all lighting is restored.
    IndoorLighting,
    /// Env-cell meshes, far→near.
    EnvCellMeshes,
    /// Objects per cell, far→near.
    Objects,
    /// `flush_alpha_list(0.0)`, back in the normal-mode render.
    FlushAlphaList,
}

/// Cell-draw step sequence for one frame.
///
/// When no portal chain reached the outdoors (`outside_view.view_count == 0`) the first four steps
/// are absent: there is nothing behind the doorway to z-fight with, so there is nothing to clear.
#[must_use]
pub fn indoor_steps(view: &ConstructedView) -> Vec<IndoorStep> {
    let mut s = Vec::new();
    if view.outside_view_count != 0 {
        s.push(IndoorStep::OutdoorsThroughPortals);
        s.push(IndoorStep::FlushBeforeClear);
        s.push(IndoorStep::ZClear);
        s.push(IndoorStep::PortalDepthStamps);
    }
    s.push(IndoorStep::IndoorLighting);
    s.push(IndoorStep::EnvCellMeshes);
    s.push(IndoorStep::Objects);
    s.push(IndoorStep::FlushAlphaList);
    s
}

// ---------------------------------------------------------------------------------------------
// The outdoor half: buildings.
// ---------------------------------------------------------------------------------------------

/// `Sidedness` — the renderer's three-valued side test, with culling epsilon `0.0002`.
///
/// The same three values `init_cell` computes for a cell portal; named here because the building
/// overload of `construct_view` both *tests* the sidedness and *passes it on* to `get_clip`, which
/// reverses the portal polygon's screen order for a `Negative` side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sidedness {
    Positive,
    Negative,
    InPlane,
}

/// `d > 0.0002 ? POSITIVE : (d < -0.0002 ? NEGATIVE : IN_PLANE)`.
///
/// Written in the client's own branch order — `if (d <= 0.0002) { NEGATIVE unless -0.0002 <= d }` —
/// which is what the cell's own portal walk and the portals-only walk both compile to.
#[must_use]
pub fn sidedness(d: f32) -> Sidedness {
    if d <= crate::consts::EPSILON {
        if -crate::consts::EPSILON <= d {
            Sidedness::InPlane
        } else {
            Sidedness::Negative
        }
    } else {
        Sidedness::Positive
    }
}

/// One building-portal entry from the landblock's portal list.
///
/// The shipped records are `BuildInfo::portals` in the block's
/// landblock-info record. Unlike a cell portal it carries **no polygon**: the polygon comes from the
/// building graphics object's drawing BSP, and the portal polygon's index is what joins the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingPortal {
    /// `portal_side = ((~flags) >> 1) & 1`, the same polarity as a cell portal's.
    pub portal_side: u8,
    /// The interior cell this opening leads into, **with the landblock base already OR-ed in**.
    pub other_cell_id: u32,
    /// The matching cell-portal index in that cell, or negative for none.
    pub other_portal_id: i32,
    /// Cells whose portal-view stacks are pushed while
    /// this portal is being processed.
    ///
    /// It is a **bound**, not bookkeeping: the stab-list push is the only thing that pushes a
    /// portal view on this path, so a cell outside the list has no views, and both the portal clip and
    /// the view hand-off skip a neighbour whose `portal_view` is not live. That is what stops
    /// one building's opening reaching the next building's rooms. Pass it to
    /// [`construct_view_from`]'s `live` argument.
    pub stab_list: Vec<CellId>,
}

/// A portal polygon — one entry of a portal BSP node's `in_portals`.
///
/// Each drawing-BSP portal entry is `(polygon index, portal_index)`,
/// both `i16` on disk. `polygon` indexes the enclosing graphics object's polygons;
/// `portal_index` indexes
/// the view's outdoor-portal list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortalPoly {
    pub polygon: usize,
    pub portal_index: usize,
}

/// The mode threaded through the drawing-BSP portal-only traversal.
///
/// The building draw runs the tree with `1` and then `2`,
/// and those are the only two values any caller ever produces. The implementation has a
/// third branch for `mode == 3` (stamp the polygon when `construct_view` *fails*); nothing in the
/// shipped client reaches it, so it is documented here and not modelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalMode {
    /// `1` — stamp the opening's depth. `draw_portal_poly(poly, true)`, whose `true` selects
    /// the debug mask. **In retail it is 7.** Bit 0
    /// makes the stamped depth the constant `0.999999` instead of the polygon's own, bit 1 forces
    /// the vertex alpha to 0 so no colour is written, and bit 2 enables the depth write — with
    /// `DEPTHTEST_ALWAYS` and `CULLMODE_NONE`. So this pass *resets the depth inside every visible
    /// opening to the far plane*, and that is the outdoor half's counterpart of the indoor path's
    /// `ZClear` + `PortalDepthStamps`: it undoes the depth the landscape already wrote behind the
    /// building, so the interior drawn next is not occluded by it.
    StampDepth,
    /// `2` — build the interior's view and draw its cells. No stamp (`if (mode != 2)` guards it).
    BuildView,
}

/// Every portal polygon yielded by a building's drawing BSP,
/// yields, in visit order.
///
/// The recursive and iterative portal-only implementations perform the same walk:
///
/// ```text
/// d = splitting_plane.N · viewpoint + splitting_plane.d
/// POSITIVE  (d >  0.0002): visit neg_node ; emit this node's portals ; continue with pos_node
/// NEGATIVE  (d < -0.0002): visit pos_node ; emit this node's portals ; continue with neg_node
/// IN_PLANE           else: visit pos_node ;                            continue with neg_node
/// a child tagged LEAF terminates that branch
/// ```
///
/// The half-space on the **far side of the viewer is visited first**, which is painter order, and a
/// portal BSP node's own portals are emitted **between** its two sub-trees — that is what interleaves
/// the interior cells with the shell geometry at the right depth. `IN_PLANE` emits nothing:
/// A portal BSP node's third arm visits the same two children as `NEGATIVE`'s and skips the emission.
///
/// `viewpoint` is in the **building's** space, because portal drawing runs inside the
/// building's own position push.
#[must_use]
pub fn build_draw_portals_only(tree: &BspTree, viewpoint: Vec3) -> Vec<PortalPoly> {
    /// `'PORT'` and `'LEAF'`, as they sit in the file.
    const TAG_PORT: u32 = 0x504F_5254;
    const TAG_LEAF: u32 = 0x4C45_4146;

    enum Task {
        Visit(u32),
        /// A portal BSP node's own `in_portals`, emitted between its two sub-trees.
        Emit(u32),
    }

    let mut out = Vec::new();
    if tree.nodes.is_empty() {
        return out;
    }
    let mut stack = vec![Task::Visit(0)];
    // The decoder builds a genuine arena tree — every node is pushed once and linked once — so this
    // cannot cycle; the budget is belt and braces against a malformed record.
    let mut budget = tree.nodes.len().saturating_mul(4) + 16;
    while let Some(task) = stack.pop() {
        budget = budget.saturating_sub(1);
        if budget == 0 {
            break;
        }
        match task {
            Task::Emit(i) => {
                let Some(n) = tree.nodes.get(i as usize) else {
                    continue;
                };
                for &(polygon, portal_index) in &n.in_portals {
                    let (Ok(polygon), Ok(portal_index)) =
                        (usize::try_from(polygon), usize::try_from(portal_index))
                    else {
                        continue; // a negative index is not a portal
                    };
                    out.push(PortalPoly {
                        polygon,
                        portal_index,
                    });
                }
            }
            Task::Visit(i) => {
                let Some(n) = tree.nodes.get(i as usize) else {
                    continue;
                };
                if n.tag == TAG_LEAF {
                    continue;
                }
                let plane = n.plane.map_or_else(Plane::default, |p| Plane {
                    normal: p.normal,
                    d: p.d,
                });
                let (first, second, emit) = match sidedness(plane.dot_point(viewpoint)) {
                    Sidedness::Positive => (n.neg_child, n.pos_child, true),
                    Sidedness::Negative => (n.pos_child, n.neg_child, true),
                    Sidedness::InPlane => (n.pos_child, n.neg_child, false),
                };
                // LIFO, so push in the reverse of the order they must run in.
                if let Some(c) = second {
                    stack.push(Task::Visit(c));
                }
                if emit && n.tag == TAG_PORT {
                    stack.push(Task::Emit(i));
                }
                if let Some(c) = first {
                    stack.push(Task::Visit(c));
                }
            }
        }
    }
    out
}

/// Result of drawing one portal: at most a depth stamp and at most one interior traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PortalDraw {
    /// Polygon index into the building
    /// graphics object's polygons to stamp. Present only in [`PortalMode::StampDepth`].
    pub stamp: Option<usize>,
    /// `construct_view(other, other_portal_id)` followed by the cell draw. Present only in
    /// [`PortalMode::BuildView`], and only when the portal survived the sidedness test, the clip
    /// and the destination cell's visibility test.
    pub interior: Option<Interior>,
}

/// The interior traversal one visible building portal opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interior {
    pub cell: CellId,
    /// `portal.other_portal_id` — the cell portal of `cell` that this opening *is*, which
    /// [`construct_view_from`] marks as the way in.
    pub entered_portal: Option<usize>,
}

/// Portal drawing and the building-specific view construction form one decision:
///
/// ```text
/// portal = outdoor_portal_list[portal_poly.portal_index]
/// push a view onto every cell in portal.stab_list
/// // the recursion:
/// d  = poly.plane.N · frame.viewpoint + poly.plane.d
/// sd = (d > 0.0002) ? POSITIVE : (d < -0.0002 ? NEGATIVE : IN_PLANE)
/// if (portal.portal_side == 0 and sd != POSITIVE) or (portal.portal_side != 0 and sd != NEGATIVE):
///     return 0
/// get_clip(sd, poly, screen_points, &n, side)
/// if n and (other = visible resident cell for portal.other_cell_id) and
///    the view copy of screen_points into other's top portal_view succeeds:
///     if mode != 2: draw_portal_poly(poly, mode == 1)
///     pop the position
///     if mode != 1: construct_view(other, portal.other_portal_id)
///     return 1
/// return 0
/// // back in draw_portal:
/// if ok and mode != 1: draw the cells
/// pop those views again
/// ```
///
/// The sidedness gate is the whole visibility rule, and it is not a back-face test in disguise: it
/// says the viewer must be on the **outward** face of the opening — `portal_side == 0` wants the
/// viewer on the polygon's positive side, `portal_side == 1` on its negative side. That is the
/// mirror of `init_cell`'s indoor `side != portal_side`, because indoors the viewer is on the
/// *inside*.
///
/// `plane_distance` is `poly.plane.N · viewpoint + poly.plane.d` in the **building's** space and
/// is supplied by the caller because physics owns the plane type. `clip` stands in for the
/// screen-space polygon clip and view copy, which need
/// the projection matrix (the renderer's); returning `true` opens a superset of the portals, in the same
/// order.
#[must_use]
pub fn draw_portal(
    poly: PortalPoly,
    portals: &[BuildingPortal],
    plane_distance: f32,
    mode: PortalMode,
    clip: &dyn Fn(Sidedness) -> bool,
    visible: &dyn Fn(u32) -> bool,
) -> PortalDraw {
    let Some(bp) = portals.get(poly.portal_index) else {
        return PortalDraw::default();
    };
    let sd = sidedness(plane_distance);
    let facing = if bp.portal_side == 0 {
        sd == Sidedness::Positive
    } else {
        sd == Sidedness::Negative
    };
    if !facing || !clip(sd) || !visible(bp.other_cell_id) {
        // `draw_portal`'s failure branch stamps the polygon only when `mode == 3`, which nothing
        // reaches; for both shipped modes a refused portal draws nothing at all.
        return PortalDraw::default();
    }
    match mode {
        PortalMode::StampDepth => PortalDraw {
            stamp: Some(poly.polygon),
            interior: None,
        },
        PortalMode::BuildView => PortalDraw {
            stamp: None,
            interior: Some(Interior {
                cell: CellId(bp.other_cell_id),
                entered_portal: usize::try_from(bp.other_portal_id).ok(),
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portal(other: u32, other_portal: i32, side: u8, facing: f32, dist: f32) -> CellPortal {
        CellPortal {
            portal_side: side,
            other_cell_id: other,
            other_portal_id: other_portal,
            exact_match: true,
            vertex_dist_sq: [dist; 4],
            viewpoint_side_distance: facing,
        }
    }

    /// A three-cell corridor: 0x100 -- 0x101 -- 0x102, with 0x102 opening outdoors.
    ///
    /// Every portal has `portal_side == 0` and a positive `viewpoint_side_distance`, so the viewer
    /// is on the cell's own side of each portal plane: `side == portal_side`, `inflag == 0`, and the
    /// portal is an **exit**. That is the ordinary "standing in a room looking out through a
    /// doorway" configuration.
    fn corridor() -> BTreeMap<u32, TraversalCell> {
        let mut m = BTreeMap::new();
        m.insert(
            0x0001_0100,
            TraversalCell {
                id: CellId(0x0001_0100),
                portals: vec![portal(0x0001_0101, 0, 0, 1.0, 100.0)],
            },
        );
        m.insert(
            0x0001_0101,
            TraversalCell {
                id: CellId(0x0001_0101),
                portals: vec![
                    portal(0x0001_0100, 0, 0, 1.0, 100.0),
                    portal(0x0001_0102, 0, 0, 1.0, 400.0),
                ],
            },
        );
        m.insert(
            0x0001_0102,
            TraversalCell {
                id: CellId(0x0001_0102),
                portals: vec![
                    portal(0x0001_0101, 1, 0, 1.0, 400.0),
                    portal(OUTDOORS, -1, 0, 1.0, 900.0),
                ],
            },
        );
        m
    }

    /// Oracle: insertion keeps `cell_todo_list`
    /// **sorted by descending `dist`**, and the loop pops from the end — so cells are visited
    /// **nearest first**.
    #[test]
    fn the_todo_list_is_sorted_descending_so_the_nearest_pops_first() {
        let mut todo = Vec::new();
        for (id, d) in [(1u32, 400.0f32), (2, 100.0), (3, 900.0), (4, 250.0)] {
            ins_cell_todo_list(&mut todo, CellId(id), d);
        }
        assert_eq!(
            todo.iter().map(|&(c, _)| c.0).collect::<Vec<_>>(),
            vec![3, 1, 4, 2],
            "descending by distance"
        );
        assert_eq!(
            todo.pop().map(|(c, _)| c.0),
            Some(2),
            "the nearest is popped first"
        );
    }

    /// Oracle: the traversal ordering rule — `cell_draw_list`
    /// ends up nearest-first and `draw_cells` walks it backwards, so cells draw **far to near**.
    #[test]
    fn the_traversal_visits_nearest_first_and_draws_far_to_near() {
        let cells = corridor();
        let v = construct_view(&cells, CellId(0x0001_0100), &|_, _| true);
        assert_eq!(
            v.cell_draw_list
                .iter()
                .map(|c| c.0 & 0xFFFF)
                .collect::<Vec<_>>(),
            vec![0x100, 0x101, 0x102],
            "nearest first: the viewer's own cell, then its neighbour, then the far one"
        );
        assert_eq!(
            v.draw_order()
                .iter()
                .map(|c| c.0 & 0xFFFF)
                .collect::<Vec<_>>(),
            vec![0x102, 0x101, 0x100],
            "draw_cells walks it backwards: far to near"
        );
    }

    /// Oracle: if `n == 0`, the opening is clipped away entirely. A closed
    /// portal stops the traversal dead, which is the whole point of the indoor visibility system:
    /// without it a dungeon draws every loaded cell.
    #[test]
    fn a_portal_that_clips_away_stops_the_traversal() {
        let cells = corridor();
        // Close the portal out of the first cell.
        let v = construct_view(&cells, CellId(0x0001_0100), &|c, _| c.0 != 0x0001_0100);
        assert_eq!(
            v.cell_draw_list.len(),
            1,
            "only the viewer's own cell is visited"
        );
        assert_eq!(v.outside_view_count, 0);
        // With every portal open the whole corridor is reached and the outdoors is seen.
        let v = construct_view(&cells, CellId(0x0001_0100), &|_, _| true);
        assert_eq!(v.cell_draw_list.len(), 3);
        assert_eq!(
            v.outside_view_count, 1,
            "one portal chain reached the outdoors"
        );
    }

    /// Oracle: the recovered indoor order is outdoors-through-portals → **Z clear** → portal depth
    /// stamps → environment
    /// cells far→near → objects → alpha list.
    ///
    /// "The Z-clear + portal depth stamp is visible behaviour, not an optimisation: without it the
    /// outdoors seen through a doorway z-fights with the interior." So this asserts both steps are
    /// **present**, and present in that order.
    #[test]
    fn the_indoor_sequence_keeps_the_z_clear_and_the_depth_stamps() {
        let cells = corridor();
        let v = construct_view(&cells, CellId(0x0001_0100), &|_, _| true);
        let steps = indoor_steps(&v);
        assert_eq!(
            steps,
            vec![
                IndoorStep::OutdoorsThroughPortals,
                IndoorStep::FlushBeforeClear,
                IndoorStep::ZClear,
                IndoorStep::PortalDepthStamps,
                IndoorStep::IndoorLighting,
                IndoorStep::EnvCellMeshes,
                IndoorStep::Objects,
                IndoorStep::FlushAlphaList,
            ]
        );
        let z = steps
            .iter()
            .position(|s| *s == IndoorStep::ZClear)
            .expect("a Z clear");
        let stamp = steps
            .iter()
            .position(|s| *s == IndoorStep::PortalDepthStamps)
            .expect("depth stamps");
        let env = steps
            .iter()
            .position(|s| *s == IndoorStep::EnvCellMeshes)
            .expect("env cell meshes");
        assert!(
            z < stamp && stamp < env,
            "clear, then stamp, then draw the interior"
        );
        assert_eq!(
            *steps.last().expect("non-empty"),
            IndoorStep::FlushAlphaList
        );
    }

    /// Oracle: `draw_cells` step 1 — the outdoor pass, the Z clear and the depth stamps all hang
    /// off `outside_view.view_count != 0`. A sealed dungeon skips all four.
    #[test]
    fn a_sealed_dungeon_needs_no_z_clear() {
        let mut cells = corridor();
        // Seal the outdoor portal by making it lead to another cell.
        if let Some(c) = cells.get_mut(&0x0001_0102) {
            c.portals[1].other_cell_id = 0x0001_0100;
        }
        let v = construct_view(&cells, CellId(0x0001_0100), &|_, _| true);
        assert_eq!(v.outside_view_count, 0);
        let steps = indoor_steps(&v);
        assert!(!steps.contains(&IndoorStep::ZClear));
        assert!(!steps.contains(&IndoorStep::PortalDepthStamps));
        assert_eq!(steps[0], IndoorStep::IndoorLighting);
    }

    /// Oracle: adjustment moves a cell so that it appears **before** the
    /// cell it was entered from in `cell_draw_list` (inserting it if absent). That is what keeps
    /// the far→near invariant when a cell turns out to be reachable by a shorter route later.
    #[test]
    fn adjust_draw_list_moves_a_cell_ahead_of_the_one_that_reached_it() {
        let c = |n: u32| CellId(n);
        let mut list = vec![c(1), c(2), c(3), c(4)];
        assert!(
            adjust_draw_list(&mut list, c(4), c(2)),
            "4 moves ahead of 2"
        );
        assert_eq!(list, vec![c(1), c(4), c(2), c(3)]);
        assert!(
            !adjust_draw_list(&mut list, c(1), c(3)),
            "already ahead: no move"
        );
        assert_eq!(list, vec![c(1), c(4), c(2), c(3)]);
        // A cell not in the list yet is inserted at the target's position.
        assert!(adjust_draw_list(&mut list, c(9), c(3)));
        assert_eq!(list, vec![c(1), c(4), c(2), c(9), c(3)]);
        // A target that is not in the list at all is a no-op.
        assert!(!adjust_draw_list(&mut list, c(7), c(42)));
    }

    /// Oracle: `inflag` means the viewer is on the far side, looking
    /// in, i.e. `side != IN_PLANE and side != portal_side`, with the 0.0002 epsilon making a
    /// viewer exactly in the portal plane `IN_PLANE` and therefore not an entry.
    #[test]
    fn init_cell_computes_inflag_from_the_viewer_side() {
        let cell = TraversalCell {
            id: CellId(1),
            portals: vec![
                portal(2, 0, 1, 1.0, 10.0),  // d > 0 -> side 0 != portal_side 1 -> inflag
                portal(3, 0, 0, 1.0, 20.0),  // d > 0 -> side 0 == portal_side 0 -> no
                portal(4, 0, 0, -1.0, 30.0), // d < 0 -> side 1 != portal_side 0 -> inflag
                portal(5, 0, 1, 0.0, 40.0),  // IN_PLANE -> never an entry
            ],
        };
        let mut pv = PortalView {
            view_count: 1,
            ..PortalView::default()
        };
        assert!(init_cell(&cell, None, &mut pv));
        assert_eq!(pv.inflag, vec![true, false, true, false]);
        // max_indist is the largest vertex distance over the *entered* portals only.
        assert_eq!(
            pv.max_indist, 30.0,
            "portal 3's 40.0 does not count: it is not an entry"
        );
        // A cell nothing sees is refused outright.
        let mut empty = PortalView::default();
        assert!(!init_cell(&cell, None, &mut empty));
    }

    use dereth_assets::common::{BspNode, BspTree, Plane as RawPlane};

    const TAG_PORT: u32 = 0x504F_5254;
    const TAG_LEAF: u32 = 0x4C45_4146;
    /// `BPnN`, both children.
    const TAG_NODE: u32 = 0x4250_6E4E;

    fn plane(nx: f32, ny: f32, nz: f32, d: f32) -> Option<RawPlane> {
        Some(RawPlane {
            normal: Vec3::new(nx, ny, nz),
            d,
        })
    }

    fn bld(side: u8, other: u32, other_portal: i32) -> BuildingPortal {
        BuildingPortal {
            portal_side: side,
            other_cell_id: other,
            other_portal_id: other_portal,
            stab_list: Vec::new(),
        }
    }

    /// Oracle: `if (portal.portal_side == 0 and sd !=
    /// POSITIVE) or (portal_side != 0 and sd != NEGATIVE): return 0`, with the `0.0002` epsilon
    /// in `sidedness`.
    ///
    /// This is the sentence "you can see into a window only from outside it". A viewer on the
    /// wrong side of the opening — which is where every *other* building's windows are — opens
    /// nothing.
    #[test]
    fn a_building_portal_opens_only_from_the_side_its_portal_side_names() {
        let portals = [bld(0, 0xA9B4_0116, 1), bld(1, 0xA9B4_0118, 0)];
        let poly0 = PortalPoly {
            polygon: 0,
            portal_index: 0,
        };
        let poly1 = PortalPoly {
            polygon: 1,
            portal_index: 1,
        };
        let yes = |_: Sidedness| true;
        let seen = |_: u32| true;

        // portal_side 0 wants POSITIVE.
        let d = draw_portal(poly0, &portals, 1.0, PortalMode::BuildView, &yes, &seen);
        assert_eq!(d.interior.map(|i| i.cell.0), Some(0xA9B4_0116));
        assert_eq!(
            draw_portal(poly0, &portals, -1.0, PortalMode::BuildView, &yes, &seen),
            PortalDraw::default(),
            "the far side of the opening opens nothing"
        );
        // portal_side 1 wants NEGATIVE, i.e. exactly the mirror.
        assert!(
            draw_portal(poly1, &portals, -1.0, PortalMode::BuildView, &yes, &seen)
                .interior
                .is_some()
        );
        assert_eq!(
            draw_portal(poly1, &portals, 1.0, PortalMode::BuildView, &yes, &seen),
            PortalDraw::default()
        );
        // IN_PLANE is neither, for either polarity: |d| <= 0.0002.
        for p in [poly0, poly1] {
            assert_eq!(
                draw_portal(p, &portals, 0.000_19, PortalMode::BuildView, &yes, &seen),
                PortalDraw::default(),
                "a viewer in the portal's own plane sees through nothing"
            );
        }
        // And the entered portal is `other_portal_id`, which is what stops the interior traversal
        // walking straight back out of the window it came in through.
        assert_eq!(d.interior.and_then(|i| i.entered_portal), Some(1));
    }

    /// Oracle: a clip producing `n == 0`, or a destination-cell lookup returning no resident cell,
    /// is a reason to open nothing. The lookup never
    /// loads from disk, so a building whose cells are not resident draws no interior at all.
    #[test]
    fn a_clipped_away_or_unloaded_portal_opens_nothing() {
        let portals = [bld(0, 0xA9B4_0116, 1)];
        let poly = PortalPoly {
            polygon: 0,
            portal_index: 0,
        };
        let yes = |_: Sidedness| true;
        assert_eq!(
            draw_portal(
                poly,
                &portals,
                1.0,
                PortalMode::BuildView,
                &|_| false,
                &|_| true
            ),
            PortalDraw::default(),
            "clipped away entirely"
        );
        assert_eq!(
            draw_portal(poly, &portals, 1.0, PortalMode::BuildView, &yes, &|_| false),
            PortalDraw::default(),
            "get_visible said the cell is not loaded"
        );
        // An out-of-range portal_index is refused rather than panicking.
        let bad = PortalPoly {
            polygon: 0,
            portal_index: 9,
        };
        assert_eq!(
            draw_portal(bad, &portals, 1.0, PortalMode::BuildView, &yes, &|_| true),
            PortalDraw::default()
        );
    }

    /// Oracle: `if (mode != 2) draw_portal_poly(...)`
    /// and `if (mode != 1) construct_view(other, ...)`. Mode 1 stamps and does not recurse; mode 2
    /// recurses and does not stamp. Both are the *same* portal, visited twice.
    #[test]
    fn mode_one_stamps_the_depth_and_mode_two_draws_the_interior() {
        let portals = [bld(0, 0xA9B4_0116, 1)];
        let poly = PortalPoly {
            polygon: 7,
            portal_index: 0,
        };
        let yes = |_: Sidedness| true;
        let seen = |_: u32| true;
        let one = draw_portal(poly, &portals, 1.0, PortalMode::StampDepth, &yes, &seen);
        assert_eq!(one.stamp, Some(7));
        assert_eq!(
            one.interior, None,
            "mode 1 never recurses into the interior"
        );
        let two = draw_portal(poly, &portals, 1.0, PortalMode::BuildView, &yes, &seen);
        assert_eq!(two.stamp, None, "mode 2 never stamps");
        assert!(two.interior.is_some());
    }

    /// Oracle: the portals-only walk and the full one -- "the half-space on the far side of the
    /// viewer is visited first", and a portal BSP node's portals are emitted **between** its two
    /// sub-trees.
    ///
    /// The tree here is a `PORT` root splitting on `x` with a `PORT` child on each side, so the
    /// visit order flips with the sign of the viewer's `x` and the root's own portal lands in the
    /// middle either way.
    #[test]
    fn the_drawing_bsp_visits_the_far_half_space_first() {
        let node = |tag: u32, pl, pos, neg, ports: Vec<(i16, i16)>| BspNode {
            tag,
            plane: pl,
            pos_child: pos,
            neg_child: neg,
            in_portals: ports,
            ..BspNode::default()
        };
        let tree = BspTree {
            nodes: vec![
                node(
                    TAG_PORT,
                    plane(1.0, 0.0, 0.0, 0.0),
                    Some(1),
                    Some(2),
                    vec![(5, 5)],
                ),
                node(
                    TAG_PORT,
                    plane(0.0, 1.0, 0.0, -10.0),
                    None,
                    None,
                    vec![(1, 1)],
                ),
                node(
                    TAG_PORT,
                    plane(0.0, 1.0, 0.0, -10.0),
                    None,
                    None,
                    vec![(2, 2)],
                ),
            ],
        };
        // x > 0: POSITIVE at the root -> neg child (index 2) first, then the root's own, then pos.
        let order = build_draw_portals_only(&tree, Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(
            order.iter().map(|p| p.portal_index).collect::<Vec<_>>(),
            vec![2, 5, 1],
            "far half-space, then this node's portals, then the near half-space"
        );
        // x < 0: NEGATIVE -> the mirror.
        let order = build_draw_portals_only(&tree, Vec3::new(-5.0, 0.0, 0.0));
        assert_eq!(
            order.iter().map(|p| p.portal_index).collect::<Vec<_>>(),
            vec![1, 5, 2]
        );
        // Exactly in the splitting plane: IN_PLANE visits pos then neg and emits **nothing** of
        // its own, which is the portal tree's third arm.
        let order = build_draw_portals_only(&tree, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(
            order.iter().map(|p| p.portal_index).collect::<Vec<_>>(),
            vec![1, 2],
            "the root's own portals are skipped in the plane"
        );
    }

    /// Oracle: `build_draw_portals_only` — "a child tagged LEAF terminates the branch", and a
    /// plain BSP node emits no portals however many `in_portals` it were to carry (only
    /// the portal-node form has the list at all).
    #[test]
    fn a_leaf_terminates_and_a_plain_node_emits_nothing() {
        let tree = BspTree {
            nodes: vec![
                BspNode {
                    tag: TAG_NODE,
                    plane: plane(1.0, 0.0, 0.0, 0.0),
                    pos_child: Some(1),
                    neg_child: Some(2),
                    in_portals: vec![(9, 9)],
                    ..BspNode::default()
                },
                BspNode {
                    tag: TAG_LEAF,
                    ..BspNode::default()
                },
                BspNode {
                    tag: TAG_PORT,
                    plane: plane(0.0, 1.0, 0.0, 0.0),
                    in_portals: vec![(3, 3)],
                    ..BspNode::default()
                },
            ],
        };
        let order = build_draw_portals_only(&tree, Vec3::new(5.0, 1.0, 0.0));
        assert_eq!(
            order.iter().map(|p| p.portal_index).collect::<Vec<_>>(),
            vec![3],
            "the PORT child contributes; the BSP branch root does not, and the LEAF stops"
        );
        assert!(build_draw_portals_only(&BspTree::default(), Vec3::ZERO).is_empty());
    }

    /// Oracle: called as `construct_view(other,
    /// portal.other_portal_id)` — the entry portal is marked `inflag`/`seen`, so the traversal
    /// does not immediately propagate back out through the window it entered by.
    #[test]
    fn entering_a_cell_through_a_named_portal_does_not_walk_straight_back_out() {
        // One cell with two portals: 0 is the window we came in through, 1 leads deeper.
        let mut cells = BTreeMap::new();
        cells.insert(
            0x0001_0100,
            TraversalCell {
                id: CellId(0x0001_0100),
                portals: vec![
                    portal(OUTDOORS, -1, 0, 1.0, 100.0),
                    portal(0x0001_0101, 0, 0, 1.0, 200.0),
                ],
            },
        );
        cells.insert(
            0x0001_0101,
            TraversalCell {
                id: CellId(0x0001_0101),
                portals: vec![portal(0x0001_0100, 1, 0, 1.0, 200.0)],
            },
        );
        // Entered through portal 0 (the window): it is the way *in* and is not re-propagated, so
        // `outside_view` never gains a polygon even though portal 0 opens outdoors.
        let v = construct_view_from(&cells, CellId(0x0001_0100), Some(0), None, &|_, _| true);
        assert_eq!(v.outside_view_count, 0, "the entry portal is not an exit");
        assert_eq!(
            v.cell_draw_list
                .iter()
                .map(|c| c.0 & 0xFFFF)
                .collect::<Vec<_>>(),
            vec![0x100, 0x101],
            "the traversal still goes deeper into the building"
        );
        // Without the entry portal named, the same cell looks straight back out of its window.
        let v = construct_view_from(&cells, CellId(0x0001_0100), None, None, &|_, _| true);
        assert_eq!(v.outside_view_count, 1);
    }
}

#[cfg(test)]
mod clipped_traversal_tests {
    use super::*;
    use crate::cells::clip::{EyeTransform, ScreenPoint, ViewPoly};

    const W: f32 = 640.0;
    const H: f32 = 480.0;

    const EYE: EyeTransform = EyeTransform {
        viewpoint: dereth_primitives::Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        #[rustfmt::skip]
        inv_view: [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        proj_11: 1.0,
        proj_22: 1.0,
        width: W,
        height: H,
    };

    fn portal(other: u32, other_portal: i32) -> CellPortal {
        CellPortal {
            portal_side: 0,
            other_cell_id: other,
            other_portal_id: other_portal,
            exact_match: true,
            vertex_dist_sq: [100.0; 4],
            // Positive: the viewer is on the cell's own side, so this is an **exit**.
            viewpoint_side_distance: 1.0,
        }
    }

    /// A screen-space quad in the transform-start convention: pixels, y down,
    /// `w = 1`, wound the way `copy_view`'s full-screen quad is.
    fn opening(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<ScreenPoint> {
        [(x0, y1), (x1, y1), (x1, y0), (x0, y0)]
            .into_iter()
            .map(|(x, y)| ScreenPoint {
                xw: x,
                yw: y,
                zw: 0.5,
                w: 1.0,
            })
            .collect()
    }

    /// 0x100 -- 0x101 -- 0x102, each door leading deeper. Portal 0 of a cell is the way back.
    fn corridor() -> BTreeMap<u32, TraversalCell> {
        let mut m = BTreeMap::new();
        m.insert(
            0x0001_0100,
            TraversalCell {
                id: CellId(0x0001_0100),
                portals: vec![portal(0x0001_0101, 0)],
            },
        );
        m.insert(
            0x0001_0101,
            TraversalCell {
                id: CellId(0x0001_0101),
                portals: vec![portal(0x0001_0100, 0), portal(0x0001_0102, 0)],
            },
        );
        m.insert(
            0x0001_0102,
            TraversalCell {
                id: CellId(0x0001_0102),
                portals: vec![portal(0x0001_0101, 1)],
            },
        );
        m
    }

    /// Oracle: after `get_clip(...)`, `n == 0` means the opening was clipped away
    /// entirely, and the view copy returns 0 below three vertices.
    ///
    /// **This is the unit's claim.** Two doors that both project onto the screen and overlap let the
    /// traversal through to the third room; move the second door so its outline no longer meets what
    /// is left of the first, and the third room is not reached at all — even though the portal graph
    /// is unchanged and the unclipped traversal ([`construct_view_from`]) still walks it.
    #[test]
    fn a_second_door_that_misses_the_first_stops_the_traversal() {
        let cells = corridor();
        let full = ViewPoly::full_screen(W, H, &EYE);

        // Overlapping doors: 0x100's door covers x 100..300, 0x101's covers x 200..400.
        let overlapping = |c: CellId, p: usize| -> Vec<ScreenPoint> {
            match (c.0, p) {
                (0x0001_0100, 0) => opening(100.0, 100.0, 300.0, 300.0),
                (0x0001_0101, 1) => opening(200.0, 100.0, 400.0, 300.0),
                _ => opening(0.0, 0.0, 1.0, 1.0),
            }
        };
        let v = construct_view_clipped(
            &cells,
            CellId(0x0001_0100),
            None,
            None,
            &full,
            &EYE,
            &overlapping,
        );
        assert_eq!(
            v.cell_draw_list
                .iter()
                .map(|c| c.0 & 0xFFFF)
                .collect::<Vec<_>>(),
            vec![0x100, 0x101, 0x102],
            "the whole corridor is visible through two overlapping doors"
        );

        // The same graph with the second door moved clear of the first on the screen.
        let disjoint = |c: CellId, p: usize| -> Vec<ScreenPoint> {
            match (c.0, p) {
                (0x0001_0100, 0) => opening(100.0, 100.0, 300.0, 300.0),
                (0x0001_0101, 1) => opening(400.0, 100.0, 600.0, 300.0),
                _ => opening(0.0, 0.0, 1.0, 1.0),
            }
        };
        let v = construct_view_clipped(
            &cells,
            CellId(0x0001_0100),
            None,
            None,
            &full,
            &EYE,
            &disjoint,
        );
        assert_eq!(
            v.cell_draw_list
                .iter()
                .map(|c| c.0 & 0xFFFF)
                .collect::<Vec<_>>(),
            vec![0x100, 0x101],
            "nothing of the second door is left inside the first, so the far room is not reached"
        );
        let unclipped = construct_view_from(&cells, CellId(0x0001_0100), None, None, &|_, _| true);
        assert_eq!(unclipped.cell_draw_list.len(), 3);
    }

    /// Oracle: view copying applies the 1-pixel simplification and the 3-vertex minimum. A door that
    /// has shrunk below a pixel on the screen is not a door, and the room behind it is not drawn.
    #[test]
    fn a_sub_pixel_door_opens_nothing() {
        let cells = corridor();
        let full = ViewPoly::full_screen(W, H, &EYE);
        let tiny = |c: CellId, p: usize| -> Vec<ScreenPoint> {
            match (c.0, p) {
                (0x0001_0100, 0) => opening(100.0, 100.0, 100.4, 100.4),
                _ => opening(0.0, 0.0, 640.0, 480.0),
            }
        };
        let v = construct_view_clipped(&cells, CellId(0x0001_0100), None, None, &full, &EYE, &tiny);
        assert_eq!(v.cell_draw_list.len(), 1, "only the viewer's own cell");
    }

    /// Oracle: `clip_portals` again — a portal whose polygon is entirely off the viewport clips to
    /// nothing against the default view's full-screen quad. This is the case the stub could not
    /// express at all: an opening behind the camera, or beside it, still opened its interior.
    #[test]
    fn a_door_off_the_side_of_the_screen_opens_nothing() {
        let cells = corridor();
        let full = ViewPoly::full_screen(W, H, &EYE);
        let offscreen = |c: CellId, p: usize| -> Vec<ScreenPoint> {
            match (c.0, p) {
                (0x0001_0100, 0) => opening(-400.0, 100.0, -200.0, 300.0),
                _ => opening(0.0, 0.0, 640.0, 480.0),
            }
        };
        let v = construct_view_clipped(
            &cells,
            CellId(0x0001_0100),
            None,
            None,
            &full,
            &EYE,
            &offscreen,
        );
        assert_eq!(v.cell_draw_list.len(), 1);
    }

    /// Oracle: portal drawing ends with `construct_view(other, portal.other_portal_id)` —
    /// the entry portal is `inflag`/`seen` and is never re-propagated, and the stab-list view push
    /// bounds how far the traversal can go. Both survive the clip being real, which is what says
    /// this is the same traversal and not a second one.
    #[test]
    fn the_entry_portal_and_the_stab_list_still_bound_the_clipped_traversal() {
        let cells = corridor();
        let full = ViewPoly::full_screen(W, H, &EYE);
        let wide = |_: CellId, _: usize| opening(50.0, 50.0, 600.0, 400.0);

        // Entered through portal 0 of 0x101 (the way back to 0x100): 0x100 is not revisited.
        let v = construct_view_clipped(
            &cells,
            CellId(0x0001_0101),
            Some(0),
            None,
            &full,
            &EYE,
            &wide,
        );
        assert_eq!(
            v.cell_draw_list
                .iter()
                .map(|c| c.0 & 0xFFFF)
                .collect::<Vec<_>>(),
            vec![0x101, 0x102],
            "the traversal does not walk back out of the door it came in through"
        );

        // And a stab list that names only the first two cells stops at the second.
        let live: std::collections::BTreeSet<u32> =
            [0x0001_0100, 0x0001_0101].into_iter().collect();
        let v = construct_view_clipped(
            &cells,
            CellId(0x0001_0100),
            None,
            Some(&live),
            &full,
            &EYE,
            &wide,
        );
        assert_eq!(
            v.cell_draw_list
                .iter()
                .map(|c| c.0 & 0xFFFF)
                .collect::<Vec<_>>(),
            vec![0x100, 0x101]
        );
    }

    /// Oracle: `n == 0` returns 0. An initial view that is
    /// already empty is a portal that clipped away, and it opens nothing.
    #[test]
    fn an_empty_initial_view_draws_no_cell_at_all() {
        let cells = corridor();
        let empty = ViewPoly::default();
        let wide = |_: CellId, _: usize| opening(50.0, 50.0, 600.0, 400.0);
        let v =
            construct_view_clipped(&cells, CellId(0x0001_0100), None, None, &empty, &EYE, &wide);
        assert!(v.cell_draw_list.is_empty());
    }
}
