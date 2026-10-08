//! Outdoor and interior cells, their runtime object lists, and cell-list expansion.
//!
//! Transcribed against the client's own object-cell, land-cell and environment-cell code.
//!
//! Two things here are easy to get wrong:
//!
//! * `find_cell_list`'s expansion loop **mutates `arr.num_cells` while iterating**, so the newly
//!   added cells also get their `find_transit_cells` called. That is how a transition through two
//!   portals in one frame works. Do not snapshot the count.
//! * The visible lookup never loads a cell; the general lookup may. Almost all physics uses the
//!   visible form, which is why physics silently stops at the edge of the loaded window instead of
//!   stalling on disk I/O. [`LandSource`] is the visible-only form.

use std::sync::Arc;

use dereth_primitives::{CellId, Frame, ObjectId, Position, Quat, Vec3};

use crate::arena::PhysHandle;
use crate::geom::sphere::Sphere;
use crate::geom::BBoxExt;
use crate::geom::Bounding;
use crate::geom::PlaneExt;
use crate::globals::{CELL_SIZE, EPSILON, HALF_SQUARE_LENGTH};
use crate::land::{LandblockCollision, WaterType};
use crate::landdefs;
use crate::math::{self, V3};
use crate::source::{EnvCellGeometry, LandSource, SetupGeometry};

/// A resolved cell. The client has a class hierarchy; the two leaves that matter to physics are a
/// land cell (two terrain triangles, possibly owning a building) and an env cell (a BSP and a
/// portal list), so this is an enum rather than a trait object.
#[derive(Debug, Clone)]
pub enum Cell {
    Land {
        id: CellId,
        block: Arc<LandblockCollision>,
    },
    Env {
        id: CellId,
        geom: Arc<EnvCellGeometry>,
    },
}

impl Cell {
    #[must_use]
    pub fn id(&self) -> CellId {
        match self {
            Self::Land { id, .. } | Self::Env { id, .. } => *id,
        }
    }

    #[must_use]
    pub fn is_land(&self) -> bool {
        matches!(self, Self::Land { .. })
    }

    /// Return the cell's own frame. Outdoors it is the land cell's **centre**
    /// ([`Self::land_pos`]); indoors it is the env cell's placement frame inside the landblock.
    #[must_use]
    pub fn pos(&self) -> Position {
        match self {
            Self::Land { id, .. } => Self::land_pos(*id),
            Self::Env { id, geom } => Position::new(*id, geom.frame),
        }
    }

    /// **What a land cell's `pos` actually holds, taken from the only code in the client that
    /// writes it.**
    ///
    /// Two consumers look as if they rest on opposite readings of this field -- one on the cell
    /// centre, one on an identity frame. Neither consumer can say which is true; the writer
    /// can. The land cells are constructed as a raw array, and
    /// initialization touches only `polygons` and `in_view`, so a land
    /// cell's `pos` starts at its default: id 0, identity quaternion,
    /// origin `(0, 0, 0)`. Exactly **one** function then writes it, the landblock's polygon
    /// construction:
    ///
    /// ```text
    /// lcell[i].pos.objcell_id            = <the packed cell id>
    /// lcell[i].pos.frame.origin.x       = (2*cellX + 1) * half_square_length
    /// lcell[i].pos.frame.origin.y       = (2*cellY + 1) * half_square_length
    /// ```
    ///
    /// Here `half_square_length` = 12.0. Nothing writes `.z` -- it keeps the constructor's zero --
    /// and nothing writes the quaternion, so the rotation stays identity.
    /// Cell setup writes the cell's landblock pointer and nothing else; the UV setup only fills
    /// `land_uvs`. So
    /// the answer is **the cell centre in x and y, `z = 0`, identity rotation, plus the cell id**
    /// -- a centre rather than an identity frame.
    ///
    /// **The two consumers do not conflict, and the reason is in the transform rather than in the
    /// frame.** The land cell's move-restriction handler calls the box test against this `pos`
    /// and windows the result on
    /// `±half_square_length`, which is a window centred on the cell centre: it needs the centre and
    /// it reads the origin. The building transit search hands the same `pos` to the bounding
    /// box's local-to-global transform, which goes through the position's own local-to-global
    /// transform -- and that function reads only the position's `objcell_id`, for the block offset,
    /// and never touches its `frame`. So the box arm never sees the centre at all; it gets landblock
    /// metres, which is what its `cell_of(v) - ox` arithmetic requires. See
    /// the bounding-box local-to-global transform, which is the statement that separates them.
    ///
    /// One accessor is therefore correct for both, and there is no second convention to name.
    #[must_use]
    pub fn land_pos(id: CellId) -> Position {
        let (cx, cy) = landdefs::cell_index_to_xy(id.index());
        #[allow(clippy::cast_possible_wrap)]
        let origin = landdefs::land_cell_origin(cx as i32, cy as i32);
        Position::new(id, Frame::new(origin, Quat::IDENTITY))
    }

    /// Return whether `local` lies in this cell. A land cell contains the point when it finds a
    /// terrain polygon. An environment cell first transforms the point into cell-local space and
    /// tests its cell BSP; a cell with **no portals at all** returns false outright.
    #[must_use]
    pub fn point_in_cell(&self, local: Vec3) -> bool {
        match self {
            Self::Land { id, block } => block.find_terrain_poly(id.index(), local).is_some(),
            Self::Env { geom, .. } => {
                if geom.portals.is_empty() {
                    return false;
                }
                let p = math::globaltolocal(&geom.frame, local);
                geom.cell_bsp
                    .as_ref()
                    .is_none_or(|b| b.point_inside_cell_bsp(p))
            }
        }
    }

    /// Land cells have no cell BSP, so they
    /// report `PARTIALLY_INSIDE` when the centre is inside and `OUTSIDE` otherwise.
    #[must_use]
    pub fn sphere_intersects_cell(&self, local_sphere: &Sphere) -> Bounding {
        match self {
            Self::Land { .. } => {
                if self.point_in_cell(local_sphere.center) {
                    Bounding::PartiallyInside
                } else {
                    Bounding::Outside
                }
            }
            Self::Env { geom, .. } => geom.cell_bsp.as_ref().map_or(Bounding::Outside, |b| {
                b.sphere_intersects_cell_bsp(local_sphere)
            }),
        }
    }

    /// The cell's box test, which is the bounding-box overlap and nothing else.
    ///
    /// The client only ever asks this of an environment cell — the one call site uses the cell a
    /// portal leads to, which is by construction another interior cell. A land cell has no
    /// interior cell structure and so
    /// no answer to give; it reports the box's own containment the way
    /// [`Self::sphere_intersects_cell`] does above, and no shipped path reaches it.
    #[must_use]
    pub fn box_intersects_cell(&self, local_box: &crate::geom::BBox) -> bool {
        match self {
            Self::Land { .. } => {
                self.point_in_cell(local_box.min) || self.point_in_cell(local_box.max)
            }
            Self::Env { geom, .. } => geom
                .cell_bsp
                .as_ref()
                .is_some_and(|b| b.box_intersects_cell_bsp(local_box.min, local_box.max)),
        }
    }

    /// The **whole block**'s water type, or `NOT_WATER` when the landblock is not known. The
    /// transition code tests this value to refuse entry into deep sea.
    #[must_use]
    pub fn block_water_type(&self) -> WaterType {
        match self {
            Self::Land { block, .. } => block.water_type,
            Self::Env { .. } => WaterType::NotWater,
        }
    }

    /// Per-cell water type.
    #[must_use]
    pub fn water_type(&self) -> WaterType {
        match self {
            Self::Land { id, block } => block.cell_water_type(id.index()).unwrap_or_default(),
            Self::Env { .. } => WaterType::NotWater,
        }
    }

    /// The cell's water depth.
    #[must_use]
    pub fn water_depth(&self, local: Vec3) -> f32 {
        match self {
            Self::Land { id, block } => block.get_water_depth(*id, local),
            Self::Env { .. } => crate::globals::WATER_DEPTH_NONE,
        }
    }

    /// Return the ids of the cells visible from this one.
    #[must_use]
    pub fn stab_list(&self) -> &[CellId] {
        match self {
            Self::Land { .. } => &[],
            Self::Env { geom, .. } => &geom.stab_list,
        }
    }

    /// An axis-aligned unit normal that pushes
    /// the object back toward this cell, using `half_square_length = 12`.
    ///
    /// The environment-cell slide-plane query is a stub returning 1 with no normal, so an
    /// interior cell blocks without a slide direction.
    #[must_use]
    pub fn handle_move_restriction(&self, curr_pos: &Position) -> Option<Vec3> {
        match self {
            Self::Land { .. } => {
                let d = math::get_offset(&self.pos(), curr_pos);
                Some(if d.y < -HALF_SQUARE_LENGTH {
                    Vec3::new(0.0, -1.0, 0.0)
                } else if d.y > HALF_SQUARE_LENGTH {
                    Vec3::new(0.0, 1.0, 0.0)
                } else if d.x >= -HALF_SQUARE_LENGTH {
                    Vec3::new(1.0, 0.0, 0.0)
                } else {
                    Vec3::new(-1.0, 0.0, 0.0)
                })
            }
            Self::Env { .. } => None,
        }
    }
}

/// `CELLINFO` — `{ cell_id, cell }`. The pointer is `None` for a cell that was named by id but is
/// not resident, which `find_transit_cells` does deliberately.
#[derive(Debug, Clone)]
pub struct CellInfo {
    pub cell_id: CellId,
    pub cell: Option<Cell>,
}

/// The cell-list builder clamps its sphere count to ten. The single-sphere form
/// passes a count of one.
pub const MAX_CELL_LIST_SPHERES: usize = 10;

/// The spheres an object's cylinder spheres hand to the cell-list search: **all** of them up to
/// [`MAX_CELL_LIST_SPHERES`], each centred on its low point with its radius, height discarded,
/// placed in the position's block space.
///
/// They are the setup's own cylinder spheres, **unscaled**: the object's scale does not enter
/// the cell search, though collision with the same cylinders does scale them. So a tree drawn at
/// twice its setup's size is registered in the cells its setup-sized trunk reaches, and a trunk
/// that reaches over a cell line only once scaled is not listed in the cell beyond it.
#[must_use]
pub fn cylinder_cell_spheres(geometry: &SetupGeometry, pos: &Position) -> Vec<Sphere> {
    let m = math::l2g(pos.frame.rotation);
    geometry
        .cyl_spheres
        .iter()
        .take(MAX_CELL_LIST_SPHERES)
        .map(|c| {
            Sphere::new(
                math::localtoglobalvec(m, c.low_pt).add(pos.frame.origin),
                c.radius,
            )
        })
        .collect()
}

/// The cell search result and controls: outside flag, load policy, count and cell entries.
#[derive(Debug, Clone, Default)]
pub struct CellArray {
    pub added_outside: bool,
    /// When set the search will not fault in unloaded cells, and the pruning pass runs.
    pub do_not_load_cells: bool,
    pub cells: Vec<CellInfo>,
}

impl CellArray {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.added_outside = false;
    }

    /// De-duplicates by id and grows by 8.
    ///
    /// Note it de-duplicates by **id**, so a cell first added by id with no pointer keeps its
    /// null pointer even when a later call has one.
    pub fn add_cell(&mut self, id: CellId, cell: Option<Cell>) {
        if self.cells.iter().any(|c| c.cell_id == id) {
            return;
        }
        self.cells.push(CellInfo { cell_id: id, cell });
    }

    /// Remove one cell from the array -- swap-remove.
    pub fn remove_cell(&mut self, i: usize) {
        self.cells.swap_remove(i);
    }

    #[must_use]
    pub fn contains(&self, id: CellId) -> bool {
        self.cells.iter().any(|c| c.cell_id == id)
    }
}

/// Per-cell runtime state: object and shadow lists, plus the restriction object. The
/// landblock loader owns the cells, so this state is stored separately in a table
/// keyed by cell id.
#[derive(Debug, Default, Clone)]
pub struct CellRuntime {
    /// Objects whose **origin** is in this cell.
    pub object_list: Vec<PhysHandle>,
    /// **The collision list**: every object whose geometry overlaps this cell.
    pub shadow_object_list: Vec<PhysHandle>,
    /// The "gate keeper" for this cell.
    pub restriction_obj: Option<ObjectId>,
}

/// Resolves cell ids against a [`LandSource`]: outdoor ids use the resident landscape
/// window and indoor ids use the visible-cell table. Neither lookup loads data.
#[derive(Clone)]
pub struct CellResolver<'a> {
    pub land: &'a dyn LandSource,
}

impl std::fmt::Debug for CellResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CellResolver")
    }
}

impl<'a> CellResolver<'a> {
    #[must_use]
    pub fn new(land: &'a dyn LandSource) -> Self {
        Self { land }
    }

    /// Resolve a cell id to a visible cell.
    ///
    /// ```text
    ///   if (id == 0)      -> NULL
    ///   if (id >= 0x100)  -> fetch a visible environment cell
    ///   else              -> fetch a land cell
    /// ```
    ///
    /// **The outdoor arm stops at the resident window, and it stops here.**
    /// The landscape lookup immediately refuses a missing resident window or an invalid
    /// global id. It also refuses a low-16-bit cell index at or above `0x100`. For a valid
    /// land-cell index, each window coordinate is `(mid_radius*8 - centre + req) / 8`,
    /// using the resident-window centre and requested land-cell coordinate. A negative
    /// coordinate or one at or above `mid_width` is outside the window and returns no
    /// cell. An empty slot or a block whose load state is not 8 also returns no cell.
    ///
    /// So the window is the `mid_width = 2*mid_radius + 1` square of landblocks centred on
    /// `loaded_cell_id`, and **every** cell lookup physics does inherits that refusal: of the nine
    /// callers in the retail client, the object's position adjustment,
    /// the detection manager's cell-list update and the cell-list rebuild come through this
    /// function, and the outdoor arm of the cell lookup, the cell-list search,
    /// the outside-cell add, the cell-block add and the transit search come through the land
    /// cell's own lookup and reach the same test. The general cell lookup's *indoor* arm is the
    /// only one in the client that loads.
    ///
    /// [`LandSource::landblock`] answers the geometry question and an implementation may read it
    /// off disk; [`LandSource::landblock_resident`] is this residency question, and a source
    /// whose window is its whole world has the default collapse the two. The resident-window
    /// tests measure it.
    #[must_use]
    pub fn get_visible(&self, id: CellId) -> Option<Cell> {
        if id.0 == 0 {
            return None;
        }
        if landdefs::is_outdoors(id) {
            if id.index() == 0 || id.index() > 0x40 {
                return None;
            }
            // The window bounds, the empty slot and a load state other than 8.
            // Asked before the geometry, as `get_landblock` asks it before dereferencing a slot.
            if !self.land.landblock_resident(id.landblock()) {
                return None;
            }
            let block = self.land.landblock(id.landblock())?;
            Some(Cell::Land { id, block })
        } else {
            let geom = self.land.env_cell(id)?;
            Some(Cell::Env { id, geom })
        }
    }

    /// Which interior cell contains `point`.
    ///
    /// `point` is in the **starting cell's** landblock space. The block offset between `start` and
    /// each candidate is applied here before the candidate's containment test.
    ///
    /// The client's second argument chooses the stab list: with it set the search walks
    /// `stab_list` through the
    /// visible-cell table, and without it the **portal list**. `AdjustPosition` always passes 1,
    /// which is the only call site this crate has, so the portal arm is not reproduced — it would
    /// need the portal list, and no caller reaches it.
    #[must_use]
    pub fn find_visible_child_cell(&self, start: &Cell, point: Vec3) -> Option<Cell> {
        if start.point_in_cell(point) {
            return Some(start.clone());
        }
        for id in start.stab_list() {
            let Some(c) = self.get_visible(*id) else {
                continue;
            };
            let local = point.sub(landdefs::get_block_offset(start.id(), c.id()));
            if c.point_in_cell(local) {
                return Some(c);
            }
        }
        None
    }

    /// Bounds-check the **global** cell coordinates,
    /// compute the id inline, resolve, and add.
    fn add_outside_cell(&self, arr: &mut CellArray, x: i32, y: i32) {
        if !landdefs::in_bounds(x, y) {
            return;
        }
        let id = landdefs::lcoord_to_gid(x, y);
        if id.0 == 0 {
            return;
        }
        let cell = self.get_visible(id);
        arr.add_cell(id, cell);
    }

    /// A rectangle of [`Self::add_outside_cell`].
    pub fn add_cell_block(&self, arr: &mut CellArray, x0: i32, y0: i32, x1: i32, y1: i32) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.add_outside_cell(arr, x, y);
            }
        }
    }

    /// The 3 to 8 neighbours a sphere
    /// straddling a cell boundary needs, in **global** land-cell coordinates so that it crosses
    /// landblock boundaries transparently.
    fn check_add_cell_boundary(
        &self,
        arr: &mut CellArray,
        p: Vec3,
        cx: i32,
        cy: i32,
        lo: f32,
        hi: f32,
    ) {
        if p.x > lo {
            self.add_outside_cell(arr, cx + 1, cy);
            if p.y > lo {
                self.add_outside_cell(arr, cx + 1, cy + 1);
            }
            if p.y < hi {
                self.add_outside_cell(arr, cx + 1, cy - 1);
            }
        }
        if p.x < hi {
            self.add_outside_cell(arr, cx - 1, cy);
            if p.y > lo {
                self.add_outside_cell(arr, cx - 1, cy + 1);
            }
            if p.y < hi {
                self.add_outside_cell(arr, cx - 1, cy - 1);
            }
        }
        if p.y > lo {
            self.add_outside_cell(arr, cx, cy + 1);
        }
        if p.y < hi {
            self.add_outside_cell(arr, cx, cy - 1);
        }
    }

    /// The outdoor cell-list builder.
    pub fn add_all_outside_cells(&self, arr: &mut CellArray, pos: &Position, spheres: &[Sphere]) {
        if arr.added_outside {
            return;
        }
        arr.added_outside = true;
        if spheres.is_empty() {
            let mut id = pos.cell;
            let mut o = pos.frame.origin;
            if landdefs::adjust_to_outside(&mut id, &mut o) {
                if let Some((x, y)) = landdefs::gid_to_lcoord(id) {
                    self.add_outside_cell(arr, x, y);
                }
            }
            return;
        }
        for s in spheres {
            let mut id = pos.cell;
            let mut o = s.center;
            if !landdefs::adjust_to_outside(&mut id, &mut o) {
                break;
            }
            let cx = landdefs::cell_of(o.x);
            let cy = landdefs::cell_of(o.y);
            #[allow(clippy::cast_precision_loss)]
            let local = Vec3::new(
                o.x - cx as f32 * CELL_SIZE,
                o.y - cy as f32 * CELL_SIZE,
                0.0,
            );
            let lo = CELL_SIZE - s.radius;
            let hi = s.radius;
            if let Some((x, y)) = landdefs::gid_to_lcoord(id) {
                self.add_outside_cell(arr, x, y);
                self.check_add_cell_boundary(arr, local, x, y, lo, hi);
            }
        }
    }

    /// The interior expansion, one portal at a time.
    ///
    /// Returns whether the outdoors must also be added, which the caller does *after* the portal
    /// loop (the original sets a flag and calls `add_all_outside_cells` at the end).
    fn env_find_transit_cells(
        &self,
        arr: &mut CellArray,
        geom: &EnvCellGeometry,
        self_pos: &Position,
        spheres: &[Sphere],
        hits_interior: &mut bool,
    ) -> bool {
        let mut needs_outside = false;
        for portal in &geom.portals {
            if portal.other_cell_id == 0xFFFF_FFFF {
                for s in spheres {
                    let local = math::globaltolocal(&self_pos.frame, s.center);
                    let d = portal.plane().dot_point(local);
                    if -(s.radius + EPSILON) < d && d < s.radius + EPSILON {
                        needs_outside = true;
                        break;
                    }
                }
                continue;
            }
            let other_id = CellId(portal.other_cell_id);
            // The other-cell lookup distinguishes "load it" from
            // "visible only"; a `LandSource` is the visible-only form either way, so both arms
            // resolve identically here.
            let other = self.get_visible(other_id);
            match other {
                None => {
                    // Not loaded: add by id only, with the portal-side test.
                    for s in spheres {
                        let local = math::globaltolocal(&self_pos.frame, s.center);
                        let d = portal.plane().dot_point(local);
                        let r = s.radius + EPSILON;
                        if (d > -r && portal.portal_side) || (d < r && !portal.portal_side) {
                            arr.add_cell(other_id, None);
                            break;
                        }
                    }
                }
                Some(other_cell) => {
                    let other_pos = other_cell.pos();
                    for s in spheres {
                        let local = math::globaltolocal(&other_pos.frame, s.center);
                        let probe = Sphere::new(local, s.radius);
                        if other_cell.sphere_intersects_cell(&probe) != Bounding::Outside {
                            arr.add_cell(other_id, Some(other_cell.clone()));
                            *hits_interior = true;
                            break;
                        }
                    }
                }
            }
        }
        needs_outside
    }

    /// The entry from a *land* cell into a building's interior.
    ///
    /// For each sphere, transform into the interior cell's frame and test its cell BSP. The first
    /// non-`OUTSIDE` result adds the cell and sets
    /// `hits_interior_cell`. Driven per land
    /// cell by the sort cell's transit-cell search, which reaches the building's own.
    fn check_building_transit(
        &self,
        arr: &mut CellArray,
        land_cell: CellId,
        spheres: &[Sphere],
        hits_interior: &mut bool,
    ) {
        for id in self.land.building_cells(land_cell) {
            if arr.contains(id) {
                continue;
            }
            let Some(other) = self.get_visible(id) else {
                continue;
            };
            let other_pos = other.pos();
            for s in spheres {
                let local = math::globaltolocal(&other_pos.frame, s.center);
                let probe = Sphere::new(local, s.radius);
                if other.sphere_intersects_cell(&probe) != Bounding::Outside {
                    arr.add_cell(id, Some(other.clone()));
                    *hits_interior = true;
                    break;
                }
            }
        }
    }

    /// The `Sidedness` value the client compares against.
    ///
    /// **The client's enum is `POSITIVE = 0, NEGATIVE = 1, IN_PLANE = 2, CROSSING = 3`**,
    /// and [`crate::geom::Sidedness`]'s discriminants are the other
    /// way round, so the client's equality comparison cannot be written as a
    /// numeric equality here. `portal_side` is stored as the raw client bit
    /// (`((~flags) >> 1) & 1`), so `true` is the client's `NEGATIVE`.
    fn portal_side_of(portal_side: bool) -> crate::geom::Sidedness {
        if portal_side {
            crate::geom::Sidedness::Negative
        } else {
            crate::geom::Sidedness::Positive
        }
    }

    /// The **bounding-box** interior expansion, the
    /// sibling of [`Self::env_find_transit_cells`] that [`Self::find_bbox_cell_list`] drives.
    ///
    /// Two tests per part, in this order:
    ///
    /// 1. a **proximity** prescreen on the part's physics sphere (or its `drawing_sphere`) against
    ///    the portal plane: `POSITIVE` continues when `d <= r`,
    ///    `NEGATIVE` when `d >= -r`, and any other `Sidedness` continues unconditionally. It is the
    ///    **same**
    ///    polarity as the sphere path, whose one-line form is
    ///    `(d > -r && side == 1) || (d < r && side == 0)` -- a part whose bounding sphere does not
    ///    reach the plane cannot carry the object through it;
    /// 2. a classification of the part's bounding box in this cell's frame, which decides whether
    ///    the other cell is reached when the box is **not entirely on `portal_side`**.
    ///
    /// The same box is then tested in the *other* cell's frame before that cell is added. The part
    /// loop breaks either way because one part is enough to carry the portal.
    fn env_find_transit_cells_bbox(
        &self,
        arr: &mut CellArray,
        geom: &EnvCellGeometry,
        self_pos: &Position,
        parts: &[crate::source::PhysicsPart],
    ) -> bool {
        let mut needs_outside = false;
        for portal in &geom.portals {
            let plane = portal.plane();
            let side = Self::portal_side_of(portal.portal_side);
            for part in parts {
                // The part's graphics object's `physics_sphere`, falling back to `drawing_sphere`. A part
                // with neither is skipped entirely, body and all.
                let Some(sphere) = part.physics_sphere().or(part.drawing_sphere) else {
                    continue;
                };
                let local = math::localtolocal(self_pos, &part.pos, sphere.center);
                let d = plane.dot_point(local);
                let r = sphere.radius + EPSILON;
                let prescreen = match side {
                    crate::geom::Sidedness::Positive => d <= r,
                    crate::geom::Sidedness::Negative => d >= -r,
                    crate::geom::Sidedness::Crossing => true,
                };
                if !prescreen {
                    continue;
                }
                let Some(bb) = part.bound_box else { continue };
                let here = bb.local_to_local(&part.pos, self_pos);
                if plane.intersect_box(here.min, here.max) == side {
                    continue;
                }
                if portal.other_cell_id == 0xFFFF_FFFF {
                    needs_outside = true;
                    break;
                }
                let other_id = CellId(portal.other_cell_id);
                match self.get_visible(other_id) {
                    // Not resident: named by id only, exactly as the sphere path does.
                    None => arr.add_cell(other_id, None),
                    Some(other) => {
                        let there = bb.local_to_local(&part.pos, &other.pos());
                        if !other.box_intersects_cell(&there) {
                            continue;
                        }
                        arr.add_cell(other_id, Some(other));
                    }
                }
                break;
            }
        }
        needs_outside
    }

    /// The bounding-box outdoor builder.
    ///
    /// The client takes the **union** of every part's box over the whole part array, in land-cell
    /// units relative to the object's own cell, and adds that rectangle in one
    /// [`Self::add_cell_block`]. Billboard parts use their bounding
    /// *sphere* instead of their box, and read it off the part's own origin without the transform
    /// the box arm applies; [`crate::source::PhysicsPart::always_2d`] records that the arm is
    /// reachable in the shipped data.
    ///
    /// **The box arm's destination frame is the land cell's own position**; see the comment at
    /// the `land` binding below for the derivation and for what a wrong frame costs.
    fn add_all_outside_cells_bbox(
        &self,
        arr: &mut CellArray,
        pos: &Position,
        parts: &[crate::source::PhysicsPart],
    ) {
        if arr.added_outside {
            return;
        }
        arr.added_outside = true;
        let Some(first) = parts.first() else { return };
        let mut id = pos.cell;
        let mut o = first.pos.frame.origin;
        if !landdefs::adjust_to_outside(&mut id, &mut o) {
            return;
        }
        let Some((bx, by)) = landdefs::gid_to_lcoord(id) else {
            return;
        };
        // The client hands the **land cell's own `pos`** to the bounding-box transform. The result
        // is in landblock-relative metres because position adjustment reads only the
        // destination's `objcell_id`. That is
        // what `cell_of(v) - ox` below needs: `ox` is the object's own cell index *within its
        // landblock*, so the subtraction is a **cell delta** only when `cell_of(v)` is itself a
        // within-block index.
        //
        // **`land_cell_origin(0, 0)`, the centre of the block's cell (0, 0), would be half a cell
        // off in x and y.** With it, a 2 m box standing at the exact centre of its own cell puts
        // `cell_of` one cell low, so the object registers **four** cells -- its own and its
        // `(-1, -1)` neighbours -- and a box reaching 22 m east, a whole cell further, still
        // registers those same four, because `cell_of(g.max.x) - ox` comes back `0`. Wrong in
        // both directions at once, which is why neither an over- nor an under-registration test
        // alone finds it.
        //
        // **This is the client's own call.** A hand-built identity `Position` gets the right
        // numbers for the wrong reason and quietly asserts that the land-cell position is an
        // identity frame, while `handle_move_restriction` two hundred lines up depends on its
        // being the cell centre. Both use [`Cell::land_pos`], and it is `local_to_global` -- not
        // the frame -- that makes the centre invisible here. See `Cell::land_pos` for the
        // derivation from the landblock's polygon construction.
        let land = Cell::land_pos(id);
        let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (0_i32, 0_i32, 0_i32, 0_i32);
        // The origin cell's coordinates **within its landblock**: `((id & 0xFFFF) - 1) >> 3` and
        // `(id - 1) & 7`, which is what the client subtracts before taking the min and max.
        let ox = i32::try_from(((id.0 & 0xFFFF).saturating_sub(1)) >> 3).unwrap_or(0);
        let oy = i32::try_from(id.0.wrapping_sub(1) & 7).unwrap_or(0);
        for part in parts {
            // A billboard part has no
            // meaningful bounding box, so the client takes its bounding **sphere** instead --
            // `physics_sphere ?: drawing_sphere` -- and reads the extent straight off the part's
            // own frame origin, **without** the transform that the box arm applies. Both
            // asymmetries are the client's and both are reproduced;
            // [`crate::source::PhysicsPart::always_2d`] records that this is reachable.
            let (min_x, min_y, max_x, max_y) = if part.always_2d() {
                let Some(s) = part.physics_sphere().or(part.drawing_sphere) else {
                    continue;
                };
                let o = part.pos.frame.origin;
                (
                    o.x - s.radius,
                    o.y - s.radius,
                    o.x + s.radius,
                    o.y + s.radius,
                )
            } else {
                let Some(bb) = part.bound_box else { continue };
                let g = bb.local_to_global(&part.pos, &land);
                (g.min.x, g.min.y, g.max.x, g.max.y)
            };
            lo_x = lo_x.min(landdefs::cell_of(min_x) - ox);
            lo_y = lo_y.min(landdefs::cell_of(min_y) - oy);
            hi_x = hi_x.max(landdefs::cell_of(max_x) - ox);
            hi_y = hi_y.max(landdefs::cell_of(max_y) - oy);
        }
        self.add_cell_block(arr, bx + lo_x, by + lo_y, bx + hi_x, by + hi_y);
    }

    /// The building's transit-cell search reached through the box form of
    /// [`Self::check_building_transit`], and it inherits that method's shape: the client walks the
    /// building's own portal list, this walks the land cell's building cells.
    fn check_building_transit_bbox(
        &self,
        arr: &mut CellArray,
        land_cell: CellId,
        parts: &[crate::source::PhysicsPart],
    ) {
        for id in self.land.building_cells(land_cell) {
            if arr.contains(id) {
                continue;
            }
            let Some(other) = self.get_visible(id) else {
                continue;
            };
            let other_pos = other.pos();
            for part in parts {
                let Some(bb) = part.bound_box else { continue };
                let there = bb.local_to_local(&part.pos, &other_pos);
                if other.box_intersects_cell(&there) {
                    arr.add_cell(id, Some(other.clone()));
                    break;
                }
            }
        }
    }

    /// This is **the arm `calc_cross_cells` takes for every object that carries a physics
    /// BSP**.
    ///
    /// The first test is on the object's `HAS_PHYSICS_BSP_PS` state bit, which the client raises
    /// from its scan of the parts. So a door, chest, lever, or any other mesh object never reaches
    /// the sorting sphere: its shadow set is
    /// its own cell plus every cell its parts' **bounding boxes** reach through a portal.
    ///
    /// The body is three lines: clear the array, add the object's own cell, then walk the array
    /// calling the part array's static cross-cell step --
    /// `find_transit_cells(num_parts, parts, cellarray)` -- on each entry. The index walks a list
    /// that **grows during iteration**, so a box reaching through two portals in a row is
    /// followed, exactly as in [`Self::find_cell_list`].
    pub fn find_bbox_cell_list(
        &self,
        pos: &Position,
        parts: &[crate::source::PhysicsPart],
        arr: &mut CellArray,
    ) {
        arr.cells.clear();
        arr.added_outside = false;
        let start = self.get_visible(pos.cell);
        arr.add_cell(pos.cell, start.clone());
        if start.is_none() || parts.is_empty() {
            // Proceeds only with a cell, a part array and a non-zero cell count.
            return;
        }
        let mut i = 0;
        while i < arr.cells.len() {
            let entry = arr.cells[i].clone();
            if let Some(c) = entry.cell {
                match &c {
                    Cell::Land { id, .. } => {
                        let id = *id;
                        self.add_all_outside_cells_bbox(arr, pos, parts);
                        self.check_building_transit_bbox(arr, id, parts);
                    }
                    Cell::Env { geom, .. } => {
                        let cell_pos = c.pos();
                        if self.env_find_transit_cells_bbox(arr, geom, &cell_pos, parts) {
                            self.add_all_outside_cells_bbox(arr, pos, parts);
                        }
                    }
                }
            }
            i += 1;
        }
    }

    /// The cells a **static** object is registered in when it is added to its cell. This is not
    /// the rule a moving object is registered by ([`crate::PhysicsWorld::calc_cross_cells`]).
    ///
    /// A static whose parts carry no physics BSP but which has cylinder spheres hands those
    /// spheres to [`Self::find_cell_list`] ([`cylinder_cell_spheres`]). **Every other static**
    /// takes [`Self::find_bbox_cell_list`] over its parts, including one with only collision
    /// spheres or none at all. A moving object in that last case is registered by its sorting
    /// sphere instead, which can reach a cell its parts' boxes do not, and miss one they do.
    ///
    /// `has_physics_bsp` is the object's `HAS_PHYSICS_BSP_PS` state; `part_scale` and
    /// `part_frames` pose the parts as [`SetupGeometry::placed_part_posed`] does (`None` is the
    /// placement). `part_scale` is the scale the parts were posed at, which for a static being
    /// added to its cell is 1: generated scenery takes its scale only after it has been added and
    /// registered. The cylinder spheres take no scale at all ([`cylinder_cell_spheres`]). The
    /// search never loads a cell.
    pub fn find_static_cell_list(
        &self,
        pos: &Position,
        geometry: &SetupGeometry,
        (has_physics_bsp, part_scale, part_frames): (bool, f32, Option<&[Frame]>),
        arr: &mut CellArray,
    ) {
        arr.do_not_load_cells = true;
        if !has_physics_bsp && !geometry.cyl_spheres.is_empty() {
            let spheres = cylinder_cell_spheres(geometry, pos);
            let mut interior = false;
            self.find_cell_list(pos, &spheres, arr, false, &mut interior);
        } else {
            let parts: Vec<crate::source::PhysicsPart> = (0..geometry.parts.len())
                .filter_map(|i| geometry.placed_part_posed(i, pos, part_scale, part_frames))
                .collect();
            self.find_bbox_cell_list(pos, &parts, arr);
        }
    }

    /// The single entry point.
    ///
    /// Returns the cell that actually contains the first sphere's centre, which is what
    /// `check_other_cells` assigns to `check_cell`.
    pub fn find_cell_list(
        &self,
        pos: &Position,
        spheres: &[Sphere],
        arr: &mut CellArray,
        want_container: bool,
        hits_interior_cell: &mut bool,
    ) -> Option<Cell> {
        arr.cells.clear();
        arr.added_outside = false;
        let id = pos.cell;
        let cell = if id.0 != 0 {
            self.get_visible(id)
        } else {
            None
        };

        if landdefs::is_outdoors(id) {
            self.add_all_outside_cells(arr, pos, spheres);
        } else {
            *hits_interior_cell = true;
            arr.add_cell(id, cell.clone());
        }

        let start = cell?;
        if spheres.is_empty() {
            return None;
        }

        // 1. Expand. The loop index walks a list that GROWS during iteration: newly added cells
        //    also get their find_transit_cells called, which is how a transition through two
        //    portals in one frame works.
        let mut i = 0;
        while i < arr.cells.len() {
            let entry = arr.cells[i].clone();
            if let Some(c) = entry.cell {
                match &c {
                    // The land-cell path is `add_all_outside_cells` **plus**
                    // the building-portal walk. Without the second
                    // half an object standing outdoors can never reach a building's interior, which
                    // is both "you cannot go inside" and "walls do not stop you".
                    Cell::Land { id, .. } => {
                        let id = *id;
                        self.add_all_outside_cells(arr, pos, spheres);
                        self.check_building_transit(arr, id, spheres, hits_interior_cell);
                    }
                    Cell::Env { geom, .. } => {
                        let cell_pos = c.pos();
                        let needs_outside = self.env_find_transit_cells(
                            arr,
                            geom,
                            &cell_pos,
                            spheres,
                            hits_interior_cell,
                        );
                        if needs_outside {
                            self.add_all_outside_cells(arr, pos, spheres);
                        }
                    }
                }
            }
            i += 1;
        }

        // 2. Pick the cell that actually contains the first sphere's centre. An interior cell
        //    wins immediately and stops the search.
        let mut container = None;
        if want_container {
            for info in &arr.cells {
                let Some(c) = info.cell.as_ref() else {
                    continue;
                };
                let local = spheres[0]
                    .center
                    .sub(landdefs::get_block_offset(pos.cell, c.id()));
                if c.point_in_cell(local) {
                    container = Some(c.clone());
                    if !landdefs::is_outdoors(c.id()) {
                        *hits_interior_cell = true;
                        break;
                    }
                }
            }
        }

        // 3. Prune: with do_not_load_cells set and having started indoors, drop any cell that is
        //    neither our own nor in our stab list. This is what stops a static object or a
        //    detection sphere from dragging in unloaded interiors.
        if arr.do_not_load_cells && !landdefs::is_outdoors(pos.cell) {
            let own = start.id();
            let stabs: Vec<CellId> = start.stab_list().to_vec();
            let mut k = 0;
            while k < arr.cells.len() {
                let id2 = arr.cells[k].cell_id;
                if id2 != own && !stabs.contains(&id2) {
                    arr.remove_cell(k);
                } else {
                    k += 1;
                }
            }
        }

        container
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::StaticLandSource;
    use dereth_primitives::LandblockId;

    // Oracle: the recovered cell and landblock behavior sections
    // "Locating the cell for a position", "Outdoor expansion" and "Interior expansion", each of
    // which describes the retail behaviour of the function named in the doc comment.

    fn world() -> StaticLandSource {
        let mut s = StaticLandSource::linear();
        // A 3x3 patch of flat blocks around (0xA9, 0xB4) so boundary expansion has somewhere to
        // go in every direction.
        for dx in -1_i32..=1 {
            for dy in -1_i32..=1 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let id = LandblockId::new((0xA9 + dx) as u8, (0xB4 + dy) as u8);
                s.add_flat_block(id, 10);
            }
        }
        s
    }

    fn at(cell: CellId, x: f32, y: f32, z: f32) -> Position {
        Position::new(cell, Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
    }

    #[test]
    fn cell_array_deduplicates_by_id_and_swap_removes() {
        let mut a = CellArray::new();
        a.add_cell(CellId(1), None);
        a.add_cell(CellId(2), None);
        a.add_cell(CellId(1), None);
        assert_eq!(a.len(), 2);
        a.add_cell(CellId(3), None);
        a.remove_cell(0);
        // swap-remove puts the last element in slot 0
        assert_eq!(a.cells[0].cell_id, CellId(3));
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn a_sphere_in_the_middle_of_a_cell_pulls_in_only_that_cell() {
        let land = world();
        let r = CellResolver::new(&land);
        let mut arr = CellArray::new();
        let mut interior = false;
        let lb = LandblockId::new(0xA9, 0xB4);
        // cell (3, 3) spans x, y in [72, 96); the centre is (84, 84).
        let pos = at(lb.cell(3 * 8 + 3 + 1), 84.0, 84.0, 0.0);
        let container = r.find_cell_list(
            &pos,
            &[Sphere::new(Vec3::new(84.0, 84.0, 0.0), 0.5)],
            &mut arr,
            true,
            &mut interior,
        );
        assert_eq!(
            arr.len(),
            1,
            "{:?}",
            arr.cells.iter().map(|c| c.cell_id).collect::<Vec<_>>()
        );
        assert_eq!(container.map(|c| c.id()), Some(lb.cell(3 * 8 + 3 + 1)));
        assert!(!interior);
    }

    #[test]
    fn a_sphere_on_a_cell_edge_pulls_in_the_neighbour() {
        let land = world();
        let r = CellResolver::new(&land);
        let lb = LandblockId::new(0xA9, 0xB4);
        let mut arr = CellArray::new();
        let mut interior = false;
        // Sitting 0.2 m short of the x = 96 boundary with a 0.5 m radius.
        let pos = at(lb.cell(3 * 8 + 3 + 1), 95.8, 84.0, 0.0);
        r.find_cell_list(
            &pos,
            &[Sphere::new(Vec3::new(95.8, 84.0, 0.0), 0.5)],
            &mut arr,
            false,
            &mut interior,
        );
        let ids: Vec<u16> = arr.cells.iter().map(|c| c.cell_id.index()).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert!(ids.contains(&(3 * 8 + 3 + 1)));
        assert!(
            ids.contains(&(4 * 8 + 3 + 1)),
            "the +X neighbour must be pulled in: {ids:?}"
        );
    }

    #[test]
    fn a_sphere_on_a_cell_corner_pulls_in_four_cells() {
        let land = world();
        let r = CellResolver::new(&land);
        let lb = LandblockId::new(0xA9, 0xB4);
        let mut arr = CellArray::new();
        let mut interior = false;
        // Right on the (96, 96) corner.
        let pos = at(lb.cell(3 * 8 + 3 + 1), 95.9, 95.9, 0.0);
        r.find_cell_list(
            &pos,
            &[Sphere::new(Vec3::new(95.9, 95.9, 0.0), 0.5)],
            &mut arr,
            false,
            &mut interior,
        );
        assert_eq!(
            arr.len(),
            4,
            "{:?}",
            arr.cells.iter().map(|c| c.cell_id).collect::<Vec<_>>()
        );
    }

    /// Block-edge continuity, mechanism 3: the expansion is done in **global** land-cell
    /// coordinates, so a sphere straddling a landblock boundary picks up the neighbouring block's
    /// cells transparently.
    #[test]
    fn expansion_crosses_a_landblock_boundary() {
        let land = world();
        let r = CellResolver::new(&land);
        let lb = LandblockId::new(0xA9, 0xB4);
        let mut arr = CellArray::new();
        let mut interior = false;
        // Cell (7, 3) is the last column of the block; x = 191.9 is 0.1 m short of the seam.
        let pos = at(lb.cell(7 * 8 + 3 + 1), 191.9, 84.0, 0.0);
        r.find_cell_list(
            &pos,
            &[Sphere::new(Vec3::new(191.9, 84.0, 0.0), 0.5)],
            &mut arr,
            false,
            &mut interior,
        );
        let blocks: Vec<u16> = arr.cells.iter().map(|c| c.cell_id.landblock().0).collect();
        assert!(blocks.contains(&lb.0));
        assert!(
            blocks.contains(&LandblockId::new(0xAA, 0xB4).0),
            "the east neighbour's cells must appear: {blocks:?}"
        );
    }

    #[test]
    fn a_cell_in_an_unloaded_block_is_simply_absent() {
        // Cell lookup returns NULL outside the window, and physics stops there rather
        // than stalling on I/O.
        let mut land = StaticLandSource::linear();
        land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10);
        let r = CellResolver::new(&land);
        let lb = LandblockId::new(0xA9, 0xB4);
        let mut arr = CellArray::new();
        let mut interior = false;
        let pos = at(lb.cell(7 * 8 + 3 + 1), 191.9, 84.0, 0.0);
        r.find_cell_list(
            &pos,
            &[Sphere::new(Vec3::new(191.9, 84.0, 0.0), 0.5)],
            &mut arr,
            false,
            &mut interior,
        );
        // The neighbour's id is still added (add_outside_cell adds by id), but with no cell.
        let unresolved = arr.cells.iter().filter(|c| c.cell.is_none()).count();
        assert!(
            unresolved >= 1,
            "the missing block's cells must be present but unresolved"
        );
    }

    /// Land cell origin is the cell centre.
    #[test]
    fn land_cell_origin_is_the_cell_centre() {
        let land = world();
        let r = CellResolver::new(&land);
        let lb = LandblockId::new(0xA9, 0xB4);
        let c = r.get_visible(lb.cell(1)).expect("cell 1");
        assert_eq!(c.pos().frame.origin, Vec3::new(12.0, 12.0, 0.0));
        let c = r.get_visible(lb.cell(3 * 8 + 3 + 1)).expect("cell (3,3)");
        assert_eq!(c.pos().frame.origin, Vec3::new(84.0, 84.0, 0.0));

        // `(2*cellX + 1) * half_square_length` over every cell of a block, so the centre is
        // asserted as the rule rather than at two stations, and `z` -- which `ConstructPolygons`
        // never writes -- stays at the constructor's zero.
        for cx in 0..8_u32 {
            for cy in 0..8_u32 {
                let id = lb.cell(u16::try_from(cx * 8 + cy + 1).expect("in range"));
                let p = Cell::land_pos(id);
                assert_eq!(p.cell, id, "the cell id travels with the frame");
                #[allow(clippy::cast_precision_loss)]
                let want = Vec3::new((2 * cx + 1) as f32 * 12.0, (2 * cy + 1) as f32 * 12.0, 0.0);
                assert_eq!(p.frame.origin, want, "cell ({cx}, {cy})");
                assert_eq!(
                    p.frame.rotation,
                    Quat::IDENTITY,
                    "the quaternion is never written"
                );
            }
        }
        // And the resolved cell agrees with the free accessor, so the two consumers cannot drift.
        let c = r.get_visible(lb.cell(1)).expect("cell 1");
        assert_eq!(c.pos(), Cell::land_pos(lb.cell(1)));
    }

    /// The two consumers of a land cells pos read different parts of it.
    #[test]
    fn the_two_consumers_of_a_land_cells_pos_read_different_parts_of_it() {
        let land = world();
        let r = CellResolver::new(&land);
        let lb = LandblockId::new(0xA9, 0xB4);
        let id = lb.cell(1);
        let cell = r.get_visible(id).expect("cell 1");
        let real = Cell::land_pos(id);
        assert_eq!(real.frame.origin, Vec3::new(12.0, 12.0, 0.0));

        // (a) the restriction normal is a window centred on the origin. A point at (12, 20) is
        // 8 m north of the centre, inside the +-12 m y band, so the answer is the +X push.
        let probe = at(id, 12.0, 20.0, 0.0);
        assert_eq!(
            cell.handle_move_restriction(&probe),
            Some(Vec3::new(1.0, 0.0, 0.0))
        );
        // Had `pos` been the identity frame the very same point would sit 20 m out, **past** the
        // band, and this consumer would answer the +Y push instead. Stated by running the same
        // arithmetic against the identity origin, so the dependence is measured rather than
        // asserted: this is the half that says the origin is load-bearing here.
        let d = math::get_offset(&Position::new(id, Frame::default()), &probe);
        assert!(
            d.y > HALF_SQUARE_LENGTH,
            "against an identity origin the same point leaves the band ({d:?}), so this consumer \
             would give the other answer -- which is why `pos` cannot be an identity frame"
        );
        let centred = math::get_offset(&real, &probe);
        assert!(
            centred.y.abs() <= HALF_SQUARE_LENGTH,
            "and against the real centre it stays inside it ({centred:?})"
        );

        // (b) the box expansion cannot tell the two apart, because `local_to_global` never reads
        // the frame. Same part, same box, the real `pos` and a zero-origin one.
        let part = Position::new(id, Frame::new(Vec3::new(30.0, 40.0, 0.0), Quat::IDENTITY));
        let bb = crate::geom::BBox::new(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0));
        assert_eq!(
            bb.local_to_global(&part, &real),
            bb.local_to_global(&part, &Position::new(id, Frame::default())),
            "the outdoor box arm reads the cell id and not the frame, so the centre is invisible \
             to it"
        );
    }

    #[test]
    fn handle_move_restriction_pushes_back_toward_the_cell_centre() {
        let land = world();
        let r = CellResolver::new(&land);
        let lb = LandblockId::new(0xA9, 0xB4);
        let cell = r.get_visible(lb.cell(1)).expect("cell 1"); // centre (12, 12)
                                                               // Far south of the centre: pushed -Y.
        let n = cell.handle_move_restriction(&at(lb.cell(1), 12.0, -5.0, 0.0));
        assert_eq!(n, Some(Vec3::new(0.0, -1.0, 0.0)));
        // Far north: +Y.
        let n = cell.handle_move_restriction(&at(lb.cell(1), 12.0, 30.0, 0.0));
        assert_eq!(n, Some(Vec3::new(0.0, 1.0, 0.0)));
        // Within the Y band and east of the centre: +X.
        let n = cell.handle_move_restriction(&at(lb.cell(1), 20.0, 12.0, 0.0));
        assert_eq!(n, Some(Vec3::new(1.0, 0.0, 0.0)));
        // Within the Y band and far west: -X.
        let n = cell.handle_move_restriction(&at(lb.cell(1), -5.0, 12.0, 0.0));
        assert_eq!(n, Some(Vec3::new(-1.0, 0.0, 0.0)));
    }

    #[test]
    fn an_env_cell_with_no_portals_is_never_entered() {
        // Portal enumeration returns 0 outright when the cell has no portals.
        let mut land = StaticLandSource::linear();
        let id = CellId(0xA9B4_0100);
        land.add_cell(EnvCellGeometry {
            id,
            ..EnvCellGeometry::default()
        });
        let r = CellResolver::new(&land);
        let c = r.get_visible(id).expect("resolved");
        assert!(!c.point_in_cell(Vec3::ZERO));
    }

    #[test]
    fn an_interior_position_seeds_the_array_with_its_own_cell_and_sets_the_flag() {
        let mut land = StaticLandSource::linear();
        let id = CellId(0xA9B4_0100);
        land.add_cell(EnvCellGeometry {
            id,
            ..EnvCellGeometry::default()
        });
        let r = CellResolver::new(&land);
        let mut arr = CellArray::new();
        let mut interior = false;
        r.find_cell_list(
            &at(id, 0.0, 0.0, 0.0),
            &[Sphere::new(Vec3::ZERO, 0.5)],
            &mut arr,
            false,
            &mut interior,
        );
        assert!(
            interior,
            "hits_interior_cell must be set for an indoor start"
        );
        assert_eq!(arr.len(), 1);
        assert_eq!(arr.cells[0].cell_id, id);
    }

    #[test]
    fn the_pruning_pass_drops_cells_outside_the_stab_list() {
        let mut land = StaticLandSource::linear();
        let home = CellId(0xA9B4_0100);
        let neighbour = CellId(0xA9B4_0101);
        // A portal from `home` to `neighbour`, but `neighbour` is not in home's stab list.
        let portal_poly = crate::geom::Polygon::new(vec![
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
        ]);
        land.add_cell(EnvCellGeometry {
            id: home,
            portals: vec![crate::source::CellPortal {
                other_cell_id: neighbour.0,
                portal: portal_poly,
                portal_side: true,
                other_portal_id: 0,
                exact_match: false,
            }],
            stab_list: vec![home],
            ..EnvCellGeometry::default()
        });
        let r = CellResolver::new(&land);
        let mut arr = CellArray::new();
        arr.do_not_load_cells = true;
        let mut interior = false;
        r.find_cell_list(
            &at(home, 0.0, 0.0, 0.0),
            &[Sphere::new(Vec3::ZERO, 0.5)],
            &mut arr,
            false,
            &mut interior,
        );
        let ids: Vec<CellId> = arr.cells.iter().map(|c| c.cell_id).collect();
        assert_eq!(
            ids,
            vec![home],
            "the neighbour is not in the stab list: {ids:?}"
        );
    }
}
