//! The cells a box touches, found the way the client finds the cells an object in the world reaches:
//! from the cell the object stands in, inside through every doorway whose polygon the box reaches
//! (and on through the cells beyond), and outside every land cell under the box.
//!
//! The box is given in the landblock frame of the object's own cell. Inside, a doorway is a
//! candidate when its plane lies within the box's half diagonal of the box's center; it is reached
//! when some of its polygon, carried into the landblock's frame, survives clipping to each of the
//! box's six faces (within 1 cm). A doorway with no cell beyond leads outside. Outside, the box's
//! corners are carried to the land cells they fall in, and every land cell between them is
//! touched, loaded or not.

use std::collections::BTreeSet;

use dereth_primitives::shape::{CylSphere, Sphere};
use dereth_primitives::{CellId, Position, Vec3};

use crate::geom::{self, BBox, BoxOps, Plane, EPS};
use crate::interior::is_interior;
use crate::land::{land_cell_id, Land, CELLS_PER_SIDE, CELL_LENGTH};

/// Land cells across the map on each axis: the region sizes the map at 255 landblocks a side,
/// the most the client takes.
const LAND_SIDE: i64 = 255 * 8;
/// The last land cell number of a block, and the first and last interior cell numbers.
const LAST_LAND_CELL: u32 = 64;
pub use auberean_primitives::land::FIRST_INTERIOR_CELL;
const LAST_INTERIOR_CELL: u32 = 0xFFFD;
/// The number a whole landblock is filed under.
const BLOCK_CELL: u32 = 0xFFFF;
/// The cell beyond a doorway out, as the shared core marks it.
pub const OUTDOORS: u32 = 0xFFFF_FFFF;
/// How far outside a face a clipped corner may lie and still count as on it.
const CLIP_EPS: f32 = 0.01;

/// Where a polygon lies against a plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sidedness {
    Positive,
    Negative,
    InPlane,
    Crossing,
}

/// A polygon being clipped: its corners in order. A new one has none.
// D2-04182
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkPolygon {
    pub corners: Vec<Vec3>,
}

impl WorkPolygon {
    /// The corners of a polygon `points` carried through the matrix `m` (row vectors, the
    /// fourth column the divisor), each divided through.
    // D2-04190
    #[must_use]
    pub fn projected(points: &[Vec3], m: &geom::Matrix4) -> Self {
        let m = |i: usize| wide(m.0[i]);
        let corners = points
            .iter()
            .map(|p| {
                let (x, y, z) = (wide(p.x), wide(p.y), wide(p.z));
                let k = 1.0 / (((z * m(11) + x * m(3)) + y * m(7)) + m(15));
                Vec3::new(
                    narrow((((z * m(8) + y * m(4)) + x * m(0)) + m(12)) * k),
                    narrow((((z * m(9) + x * m(1)) + y * m(5)) + m(13)) * k),
                    narrow((((z * m(10) + y * m(6)) + x * m(2)) + m(14)) * k),
                )
            })
            .collect();
        Self { corners }
    }

    /// The polygon's area: half the length of the sum of its edges' cross products.
    // D2-04184
    #[must_use]
    pub fn area(&self) -> f32 {
        let v = &self.corners;
        let k = v.len();
        let (mut xy, mut yz, mut zx) = (0.0_f64, 0.0_f64, 0.0_f64);
        for i in 0..k {
            let (a, b) = (v[i], v[(i + 1) % k]);
            let sx = wide(narrow(wide(b.x) + wide(a.x)));
            xy += (wide(a.y) - wide(b.y)) * sx * 0.5;
            yz += (wide(b.y) + wide(a.y)) * (wide(a.z) - wide(b.z)) * 0.5;
            zx += (wide(b.z) - wide(a.z)) * sx * 0.5;
        }
        narrow((zx * zx + yz * yz + xy * xy).sqrt())
    }

    /// Clips to the positive side of `plane`: corners more than `eps` off it are on one side, the
    /// rest in it. With corners on both sides the polygon keeps those in the plane and on the
    /// positive side, with the crossing of each edge that leaves one side for the other between
    /// them. With none on the positive side it is in the plane when `keep_in_plane` and none is on
    /// the negative side, and negative otherwise; with none on the negative side, positive.
    // D2-04191
    pub fn clip(&mut self, plane: &Plane, keep_in_plane: bool, eps: f32) -> Sidedness {
        let n = self.corners.len();
        let dist: Vec<f32> = self
            .corners
            .iter()
            .map(|c| plane.normal.x * c.x + c.y * plane.normal.y + c.z * plane.normal.z + plane.d)
            .collect();
        // 0 positive, 1 negative, 2 in the plane.
        let side: Vec<u8> = dist
            .iter()
            .map(|&d| {
                if d <= eps {
                    if -eps <= d {
                        2
                    } else {
                        1
                    }
                } else {
                    0
                }
            })
            .collect();
        let count = |k: u8| side.iter().filter(|&&s| s == k).count();
        let (positive, negative) = (count(0), count(1));
        if positive == 0 {
            return if keep_in_plane && negative == 0 {
                Sidedness::InPlane
            } else {
                Sidedness::Negative
            };
        }
        if negative == 0 {
            return Sidedness::Positive;
        }
        let mut out = Vec::with_capacity(n + 2);
        for i in 0..n {
            let j = (i + 1) % n;
            if side[i] == 2 {
                out.push(self.corners[i]);
                continue;
            }
            if side[i] == 0 {
                out.push(self.corners[i]);
            }
            if side[i] != side[j] && side[j] != 2 {
                let (a, b) = (self.corners[i], self.corners[j]);
                let t = dist[i] / (dist[i] - dist[j]);
                out.push(Vec3::new(
                    (b.x - a.x) * t + a.x,
                    (b.y - a.y) * t + a.y,
                    (b.z - a.z) * t + a.z,
                ));
            }
        }
        self.corners = out;
        Sidedness::Crossing
    }
}

/// A search's state: the box, the cell it started from, the cells touched so far (each once, in
/// the order found), whether the land cells were added, and the interior cells already walked.
struct Search<'a> {
    land: &'a Land,
    origin: CellId,
    bbox: BBox,
    cells: Vec<CellId>,
    added_outside: bool,
    walked: BTreeSet<u32>,
}

impl Search<'_> {
    // D2-05807
    fn add(&mut self, id: CellId) {
        if !self.cells.contains(&id) {
            self.cells.push(id);
        }
    }

    /// An interior cell's doorways the box reaches, and the cells beyond them, once each.
    // D2-03556, D2-03522
    fn transit(&mut self, id: CellId) {
        if !self.walked.insert(id.0) {
            return;
        }
        let Some(cell) = self.land.cell(id) else {
            return;
        };
        let center = self.bbox.center();
        let dz = self.bbox.max.z - self.bbox.min.z;
        let hx = (self.bbox.max.x - self.bbox.min.x) * 0.5;
        let hy = (self.bbox.max.y - self.bbox.min.y) * 0.5;
        let reach = (hx * hx + hy * hy + dz * 0.5 * dz * 0.5).sqrt();
        let local = geom::point_to_local(&cell.frame, center);
        let mut outside = false;
        for portal in &cell.portals {
            let p = &portal.portal.plane;
            if (local.x * p.normal.x + local.z * p.normal.z + local.y * p.normal.y + p.d).abs()
                >= reach + EPS
            {
                continue;
            }
            let mut poly = WorkPolygon {
                corners: portal
                    .portal
                    .vertices
                    .iter()
                    .map(|v| geom::point_to_global(&cell.frame, *v))
                    .collect(),
            };
            let reached = (0..6).all(|i| {
                let f = self.bbox.face(i);
                let inward = Plane::new(Vec3::new(-f.normal.x, -f.normal.y, -f.normal.z), -f.d);
                poly.clip(&inward, true, CLIP_EPS) != Sidedness::Negative
            });
            if !reached {
                continue;
            }
            if portal.other_cell_id == OUTDOORS {
                outside = true;
            } else {
                let other = CellId(portal.other_cell_id);
                self.add(other);
                self.transit(other);
            }
        }
        if outside {
            self.outside();
        }
    }

    /// Every land cell under the box, once per search.
    // D2-03834
    fn outside(&mut self) {
        if self.added_outside {
            return;
        }
        self.added_outside = true;
        for id in outside_cells(self.origin, &self.bbox) {
            self.add(id);
        }
    }
}

/// The land cells under a box in the landblock frame of `origin`, from the one its least corner
/// falls in to the one its greatest falls in, across then up, each on the map; none when either
/// corner is off it.
// D2-03834, D2-03832
pub fn outside_cells(origin: CellId, bbox: &BBox) -> Vec<CellId> {
    let (Some((x0, y0)), Some((x1, y1))) = (
        outside_lcoord(origin, bbox.min),
        outside_lcoord(origin, bbox.max),
    ) else {
        return Vec::new();
    };
    if x0 > x1 {
        return Vec::new();
    }
    (x0..=x1)
        .flat_map(|x| (y0..=y1).filter_map(move |y| lcoord_to_cell(x, y)))
        .collect()
}

/// The world land coordinates of the land cell a point in `cell`'s landblock frame falls in, or
/// none off the map or for a cell that is neither land nor interior nor a whole block. Offsets
/// within 0.0002 of a block's edge count as on it.
// D2-13215, D2-13212, D2-02556
fn outside_lcoord(cell: CellId, v: Vec3) -> Option<(i64, i64)> {
    let low = cell.0 & 0xFFFF;
    if !((1..=LAST_LAND_CELL).contains(&low)
        || (FIRST_INTERIOR_CELL..=LAST_INTERIOR_CELL).contains(&low)
        || low == BLOCK_CELL)
    {
        return None;
    }
    let x = if v.x.abs() < EPS { 0.0 } else { v.x };
    let y = if v.y.abs() < EPS { 0.0 } else { v.y };
    let side = i64::try_from(CELLS_PER_SIDE).unwrap_or(8);
    let bx = i64::from(cell.0 >> 24) * side;
    let by = i64::from((cell.0 >> 16) & 0xFF) * side;
    #[allow(clippy::cast_possible_truncation)]
    let lx = bx + f64::from(x / CELL_LENGTH).floor() as i64;
    #[allow(clippy::cast_possible_truncation)]
    let ly = by + f64::from(y / CELL_LENGTH).floor() as i64;
    ((0..LAND_SIDE).contains(&lx) && (0..LAND_SIDE).contains(&ly)).then_some((lx, ly))
}

/// The land cell at world land coordinates `(x, y)`.
// D2-02555
fn lcoord_to_cell(x: i64, y: i64) -> Option<CellId> {
    if !(0..LAND_SIDE).contains(&x) || !(0..LAND_SIDE).contains(&y) {
        return None;
    }
    let side = i64::try_from(CELLS_PER_SIDE).ok()?;
    let (bx, by) = (u8::try_from(x / side).ok()?, u8::try_from(y / side).ok()?);
    let (cx, cy) = (
        usize::try_from(x % side).ok()?,
        usize::try_from(y % side).ok()?,
    );
    Some(land_cell_id(bx, by, cx, cy))
}

/// Brings a place outside into the land cell its point falls in, the point carried into that
/// cell's landblock frame; the cell becomes 0 (and false) when the point is off the map or the
/// cell is no kind of cell.
#[must_use]
pub fn adjust_to_outside(cell: &mut CellId, origin: &mut Vec3) -> bool {
    if origin.x.abs() < EPS {
        origin.x = 0.0;
    }
    if origin.y.abs() < EPS {
        origin.y = 0.0;
    }
    let Some((lx, ly)) = outside_lcoord(*cell, *origin) else {
        *cell = CellId(0);
        return false;
    };
    let Some(to) = lcoord_to_cell(lx, ly) else {
        *cell = CellId(0);
        return false;
    };
    let from_x = i64::from(cell.0 >> 24);
    let from_y = i64::from((cell.0 >> 16) & 0xFF);
    let to_x = i64::from(to.0 >> 24);
    let to_y = i64::from((to.0 >> 16) & 0xFF);
    #[allow(clippy::cast_precision_loss)]
    {
        origin.x -= (to_x - from_x) as f32 * crate::rules::BLOCK_LENGTH;
        origin.y -= (to_y - from_y) as f32 * crate::rules::BLOCK_LENGTH;
    }
    *cell = to;
    true
}

/// The cells a box in the landblock frame of `origin` touches, starting from `origin`: an interior
/// cell, then the cells through its doorways the box reaches; a land cell, every land cell under
/// the box.
// D2-03847
#[must_use]
pub fn find_cells(land: &Land, origin: CellId, bbox: BBox) -> Vec<CellId> {
    let mut s = Search {
        land,
        origin,
        bbox,
        cells: Vec::new(),
        added_outside: false,
        walked: BTreeSet::new(),
    };
    if origin.0 == 0 {
        return s.cells;
    }
    if is_interior(origin) {
        s.add(origin);
        s.transit(origin);
    } else {
        s.outside();
    }
    s.cells
}

/// The cells a box in `pos`'s own frame touches.
// D2-03861
#[must_use]
pub fn find_cells_of_box(land: &Land, pos: &Position, local: &BBox) -> Vec<CellId> {
    find_cells(land, pos.cell, local.local_to_global(pos, pos))
}

/// The cells the box around spheres in `pos`'s own frame touches.
// D2-03849, D2-03860
#[must_use]
pub fn find_cells_of_spheres(land: &Land, pos: &Position, spheres: &[Sphere]) -> Vec<CellId> {
    find_cells_of_box(land, pos, &BBox::of_spheres(spheres))
}

/// The cells the box around upright cylinders in `pos`'s own frame touches.
// D2-03848
#[must_use]
pub fn find_cells_of_cylinders(
    land: &Land,
    pos: &Position,
    cylinders: &[CylSphere],
) -> Vec<CellId> {
    find_cells_of_box(land, pos, &BBox::of_cylinders(cylinders))
}

/// Which of the eight landblocks around one a line goes on into, as the client numbers them.
pub mod direction {
    pub const NORTH: u32 = 1;
    pub const SOUTH: u32 = 2;
    pub const EAST: u32 = 3;
    pub const WEST: u32 = 4;
    pub const NORTHWEST: u32 = 5;
    pub const SOUTHWEST: u32 = 6;
    pub const NORTHEAST: u32 = 7;
    pub const SOUTHEAST: u32 = 8;
}

/// Land cells to a landblock's side, as a shift.
const BLOCK_SHIFT: u32 = 3;

fn wide(v: f32) -> f64 {
    f64::from(v)
}

#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// The landblocks east and north to go toward `dir` (0 stays put); none for no direction.
// D2-03610
#[must_use]
pub fn block_step(dir: u32) -> Option<(i64, i64)> {
    Some(match dir {
        0 => (0, 0),
        direction::NORTH => (0, 1),
        direction::SOUTH => (0, -1),
        direction::EAST => (1, 0),
        direction::WEST => (-1, 0),
        direction::NORTHWEST => (-1, 1),
        direction::SOUTHWEST => (-1, -1),
        direction::NORTHEAST => (1, 1),
        direction::SOUTHEAST => (1, -1),
        _ => return None,
    })
}

/// The offset from a landblock's frame to the frame of the landblock toward `dir`, meters.
// D2-13213
#[must_use]
pub fn block_step_offset(dir: u32) -> Vec3 {
    let (x, y) = block_step(dir).unwrap_or((0, 0));
    #[allow(clippy::cast_precision_loss)]
    Vec3::new(
        x as f32 * crate::rules::BLOCK_LENGTH,
        y as f32 * crate::rules::BLOCK_LENGTH,
        0.0,
    )
}

/// A point brought inside a landblock's square: no further than 0.0002 short of its far sides, not
/// below 0 on its near ones.
// D2-13211
pub fn move_into_block(v: &mut Vec3) {
    let far = wide(crate::rules::BLOCK_LENGTH) - wide(EPS);
    if far < wide(v.x) {
        v.x = narrow(far);
    }
    if far < wide(v.y) {
        v.y = narrow(far);
    }
    if v.x < 0.0 {
        v.x = 0.0;
    }
    if v.y < 0.0 {
        v.y = 0.0;
    }
}

/// Cuts `ray`, in a landblock's frame, where it leaves the landblock: `piece` becomes the part
/// inside, `ray` the rest in the frame of the landblock beside it (its start brought inside that
/// landblock), and `dir` that landblock's direction. False when the ray ends inside the landblock
/// (`piece` is then the whole ray), and when it starts outside it by more than 0.0002 or leaves by
/// a corner it does not head across (`piece` is then left as it was).
// D2-13216
pub fn block_ray_clip(ray: &mut geom::Ray, piece: &mut geom::Ray, dir: &mut u32) -> bool {
    let side = crate::rules::BLOCK_LENGTH;
    let p = ray.point;
    let len = ray.length;
    let end = Vec3::new(
        narrow(wide(len) * wide(ray.dir.x) + wide(p.x)),
        len * ray.dir.y + p.y,
        len * ray.dir.z + p.z,
    );
    let low = -EPS;
    let high = EPS + side;
    if !(p.x >= low && p.x <= high && p.y >= low && p.y <= high) {
        return false;
    }
    // How far along the ray it leaves across x and across y.
    let mut tx = 1.0_f64;
    if end.x < low {
        tx = wide(p.x) / (wide(p.x) - wide(end.x));
    } else if end.x > high {
        tx = (wide(side) - wide(p.x)) / (wide(end.x) - wide(p.x));
    }
    let mut ty = 1.0_f64;
    if end.y < low {
        ty = wide(p.y) / (wide(p.y) - wide(end.y));
    } else if end.y > high {
        ty = (wide(side) - wide(p.y)) / (wide(end.y) - wide(p.y));
    }
    let whole = 1.0 - wide(EPS);
    if tx >= whole && ty >= whole {
        *piece = *ray;
        return false;
    }
    let cut = if tx <= ty {
        let t = narrow(tx);
        let along = narrow((wide(end.x) - wide(p.x)) * wide(t));
        let across = narrow((wide(end.y) - wide(p.y)) * wide(t));
        let up = end.z - p.z;
        Vec3::new(
            along + p.x,
            across + p.y,
            narrow(wide(up) * wide(t) + wide(p.z)),
        )
    } else {
        let (sx, sy, sz) = (end.x - p.x, end.y - p.y, end.z - p.z);
        let along = narrow(wide(sx) * ty);
        let across = narrow(wide(sy) * ty);
        Vec3::new(along + p.x, across + p.y, narrow(ty * wide(sz) + wide(p.z)))
    };
    piece.init_line_segment(p, cut);
    // Unordered directions count as heading west and south.
    let west = ray.dir.x.is_nan() || ray.dir.x < 0.0;
    let south = ray.dir.y.is_nan() || ray.dir.y < 0.0;
    let near_top = narrow(wide(side) - wide(EPS));
    *dir = if cut.x < EPS {
        if cut.y < EPS {
            match (west, south) {
                (false, false) => return false,
                (false, true) => direction::SOUTH,
                (true, false) => direction::WEST,
                (true, true) => direction::SOUTHWEST,
            }
        } else if wide(side) - wide(EPS) < wide(cut.y) {
            match (west, south) {
                (false, false) => direction::NORTH,
                (false, true) => return false,
                (true, false) => direction::NORTHWEST,
                (true, true) => direction::WEST,
            }
        } else {
            direction::WEST
        }
    } else if cut.x > near_top {
        if cut.y < EPS {
            match (west, south) {
                (false, false) => direction::EAST,
                (false, true) => direction::SOUTHEAST,
                (true, false) => return false,
                (true, true) => direction::SOUTH,
            }
        } else if cut.y > near_top {
            match (west, south) {
                (false, false) => direction::NORTHEAST,
                (false, true) => direction::EAST,
                (true, false) => direction::NORTH,
                (true, true) => return false,
            }
        } else {
            direction::EAST
        }
    } else if cut.y < EPS {
        direction::SOUTH
    } else if cut.y > near_top {
        direction::NORTH
    } else {
        return false;
    };
    let off = block_step_offset(*dir);
    let mut start = Vec3::new(cut.x - off.x, cut.y - off.y, cut.z - off.z);
    move_into_block(&mut start);
    let rest = Vec3::new(end.x - off.x, end.y - off.y, end.z - off.z);
    ray.init_line_segment(start, rest);
    true
}

/// The land coordinates of landblock `block`'s corner; none for no landblock or one off the map.
// D2-01966
fn block_lcoord(block: u32) -> Option<(i64, i64)> {
    if block == 0 {
        return None;
    }
    let x = i64::from(block >> 24) << BLOCK_SHIFT;
    let y = i64::from((block >> 16) & 0xFF) << BLOCK_SHIFT;
    ((0..LAND_SIDE).contains(&x) && (0..LAND_SIDE).contains(&y)).then_some((x, y))
}

/// The landblock holding land coordinates `(x, y)`; 0 off the map.
// D2-03630
fn block_gid(x: i64, y: i64) -> u32 {
    if !(0..LAND_SIDE).contains(&x) || !(0..LAND_SIDE).contains(&y) {
        return 0;
    }
    let (bx, by) = (
        u32::try_from(x >> BLOCK_SHIFT).unwrap_or(0),
        u32::try_from(y >> BLOCK_SHIFT).unwrap_or(0),
    );
    (bx << 8 | by) << 16 | BLOCK_CELL
}

/// The landblock `dx` east and `dy` north of `block`; 0 off the map.
// D2-03631
#[must_use]
pub fn block_at(block: u32, dx: i64, dy: i64) -> u32 {
    let side = 1 << BLOCK_SHIFT;
    block_lcoord(block).map_or(0, |(x, y)| block_gid(x + side * dx, y + side * dy))
}

/// The landblock beside `block` toward `dir`; 0 off the map.
// D2-03657
#[must_use]
pub fn block_beside(block: u32, dir: u32) -> u32 {
    let (dx, dy) = block_step(dir).unwrap_or((0, 0));
    block_at(block, dx, dy)
}

/// Whether `block` names a whole landblock on the map.
// D2-03505
fn valid_block(block: u32) -> bool {
    block == block & 0xFFFF_0000 | BLOCK_CELL && block_lcoord(block).is_some()
}

/// The landblock coordinates of a cell id's high word.
// D2-03506
fn block_coord(part: u32) -> Option<(i64, i64)> {
    if part & 0xFFFF_0000 != 0 || !valid_block(part << 16 | BLOCK_CELL) {
        return None;
    }
    Some((i64::from(part >> 8), i64::from(part & 0xFF)))
}

/// The landblocks a box in the landblock frame of `cell` reaches: those it overlaps by more than
/// 0.0002, and always `cell`'s own; column by column from the west, each from the south; none
/// off the map.
// D2-13219
#[must_use]
pub fn block_list(cell: CellId, area: &BBox) -> Vec<u32> {
    let mut out = Vec::new();
    let Some((bx, by)) = block_coord(cell.0 >> 16) else {
        return out;
    };
    let side = wide(crate::rules::BLOCK_LENGTH);
    #[allow(clippy::cast_possible_truncation)]
    let steps = |v: f64| (v / side).floor() as i64;
    let x0 = steps(wide(EPS) + wide(area.min.x)).min(0) + bx;
    let y0 = steps(wide(EPS + area.min.y)).min(0) + by;
    let x1 = steps(wide(area.max.x - EPS)).max(0) + bx;
    let y1 = steps(wide(area.max.y - EPS)).max(0) + by;
    for x in x0..=x1 {
        for y in y0..=y1 {
            let (lx, ly) = (x << BLOCK_SHIFT, y << BLOCK_SHIFT);
            if (0..LAND_SIDE).contains(&lx) && (0..LAND_SIDE).contains(&ly) {
                if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) {
                    out.push((x << 8 | y) << 16 | BLOCK_CELL);
                }
            }
        }
    }
    out
}

/// Whether land coordinates `(x, y)` lie on the map.
// D2-01965
#[must_use]
pub fn in_bounds(x: i32, y: i32) -> bool {
    (0..LAND_SIDE).contains(&i64::from(x)) && (0..LAND_SIDE).contains(&i64::from(y))
}

/// Whether two cells are in the same landblock: two cells 0 are; a cell 0 and another are not.
// D2-01967
#[must_use]
pub fn in_same_block(a: CellId, b: CellId) -> bool {
    match (a.0, b.0) {
        (0, 0) => true,
        (0, _) | (_, 0) => false,
        (a, b) => a >> 16 == b >> 16,
    }
}

/// Whether a point in a landblock's frame lies over it: from 0 (included) to 160 m (excluded)
/// east and north.
#[must_use]
pub fn within_block(v: Vec3) -> bool {
    0.0 <= v.x && 0.0 <= v.y && v.x < crate::rules::BLOCK_LENGTH && v.y < crate::rules::BLOCK_LENGTH
}

/// The whole-block cell of the landblock at block coordinates `(x, y)`; 0 off the map.
#[must_use]
pub fn block_id_at(x: i32, y: i32) -> u32 {
    let (lx, ly) = (x.wrapping_shl(BLOCK_SHIFT), y.wrapping_shl(BLOCK_SHIFT));
    if !in_bounds(lx, ly) {
        return 0;
    }
    #[allow(clippy::cast_sign_loss)]
    let (bx, by) = ((lx >> BLOCK_SHIFT) as u32, (ly >> BLOCK_SHIFT) as u32);
    (bx << 8 | by) << 16 | BLOCK_CELL
}

/// The land cell at map coordinates (east, north, each -100 to 100 across the map's width or
/// length): the land cell whose coordinates are the nearest whole numbers to the point's; none
/// off the map.
// D2-13192
#[must_use]
pub fn map_coords_to_cell(east: f32, north: f32) -> Option<CellId> {
    #[allow(clippy::cast_precision_loss)]
    let side = LAND_SIDE as f64;
    // Across is worked from the half width kept as a float, up from the half length kept wide.
    let half_x = wide(narrow(side * 0.5));
    let half_y = side * 0.5;
    let at = |v: f32, half: f64| (wide(v) * wide(0.01_f32) * half + half + 0.5).floor();
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = (at(east, half_x) as i64, at(north, half_y) as i64);
    lcoord_to_cell(x, y).filter(|c| c.0 != 0)
}

/// The squared distance between two land cells' corners, in meters; when either is not a land
/// cell on the map, the length of the map in meters.
// D2-13193
#[must_use]
pub fn cell_distance_squared(a: CellId, b: CellId) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let far = (LAND_SIDE as f32) * CELL_LENGTH;
    let (Some((ax, ay)), Some((bx, by))) = (land_lcoord(a), land_lcoord(b)) else {
        return far;
    };
    let (dx, dy) = (ax - bx, ay - by);
    // The client sums the squares as a 32-bit whole number; on the map they stay well within it.
    let d = f64::from(i32::try_from(dx * dx + dy * dy).unwrap_or(i32::MAX));
    narrow(d * wide(CELL_LENGTH) * wide(CELL_LENGTH))
}

/// A land cell's world land coordinates; none for a cell that is not a land cell on the map.
fn land_lcoord(cell: CellId) -> Option<(i64, i64)> {
    let low = cell.0 & 0xFFFF;
    if !(1..=LAST_LAND_CELL).contains(&low) {
        return None;
    }
    let (bx, by) = block_lcoord(cell.0 | BLOCK_CELL)?;
    let c = i64::from(low - 1);
    Some((bx + (c >> 3), by + (c & 7)))
}

/// The value at a point of a landblock from the four at its corners (south-west, south-east,
/// north-west, north-east), blended across the block (the point held to it).
// D2-13210
#[must_use]
pub fn interpolate(v: Vec3, sw: f32, se: f32, nw: f32, ne: f32) -> f32 {
    let k = 1.0 / wide(crate::rules::BLOCK_LENGTH);
    let clamp = |t: f32| {
        if 0.0 <= t {
            if 1.0 < t {
                1.0
            } else {
                t
            }
        } else {
            0.0
        }
    };
    let (u, t) = (clamp(narrow(k * wide(v.x))), clamp(narrow(k * wide(v.y))));
    let (u, t) = (wide(u), wide(t));
    narrow(
        (t * wide(ne) + (1.0 - t) * wide(se)) * u
            + (1.0 - u) * (t * wide(nw) + wide(sw) * (1.0 - t)),
    )
}
