//! What a frame seen from inside shows at the moment: the walk from the camera's cell through the
//! doorways, each one narrowing the view to what can be seen through it.
//!
//! The camera's view is five planes: four sides and the near plane. From the camera's cell the
//! walk takes each doorway that some of shows through the view (the doorway's polygon clipped to
//! it), narrows the view to the clipped doorway (one plane through the camera along each of its
//! edges) and walks on into the cell beyond with it; a doorway out adds its narrowed view to the
//! views out onto the land. A cell whose box lies wholly outside the view is not entered. The
//! cells and their objects reached are drawn: a cell when it is the camera's or one the camera's
//! cell sees, an object when its box, doubled, meets one of the views its cell was entered by.
//!
//! The view's sides are looser than the picture: their half angles are the vertical field of view
//! times the picture's shape over two across, and ninety degrees less half the field of view up
//! and down, as the client makes them.
//!
//! Cells that are never culled are taken together with the cells drawn with them, each whole, the
//! doorways out of the group adding their whole views out, the cells beyond them walked on.
//!
//! A cell past the interior fog's far distance (every corner of its box at least that far from the
//! camera) is left out and the frame's background is painted the fog's color, but only with the
//! distance the cell itself was given when it last held the camera: the camera's cell takes its
//! block's distance each frame and keeps it, and a cell the camera has never stood in is never
//! left out for the fog.
//!
//! From outside, each view onto the land looks into the cells of the blocks about the camera that
//! touch the outside: a cell whose box meets the view, and which is never culled or has a doorway
//! some of which the view shows (a doorway out for a camera on the land, one between cells for a
//! camera inside), starts a walk of its own, once a frame, with that view. The cells such a walk
//! reaches are drawn when they are its first cell or that cell sees them; what stands in them as
//! it does for a camera inside.
//!
//! A body is taken from the cells it touches: drawn when one of them is and its box, doubled, meets
//! a view that cell was entered by. One standing on the land is drawn when its box, doubled, meets
//! a view onto the land.

use std::collections::{HashMap, HashSet};

use auberean_physics::cells::{Sidedness, WorkPolygon};
use auberean_primitives::Vec3;
use auberean_world::math::Mat4;
use dereth_primitives::num::math::{cosf, sinf};
use dereth_primitives::shape::Plane;
use dereth_primitives::Vec3 as V;

use crate::scene::{Camera, Cull, ObjectBatch};

/// The first low word of an interior cell.
const FIRST_INTERIOR: u32 = 0x100;

/// The most cells a body may touch and still be culled by them.
pub const BODY_CELLS: usize = 6;

/// How far off a plane a corner may lie and still count as on it, when a doorway is clipped to a
/// view.
const CLIP_EPS: f32 = 0.01;
/// How far off a box may lie outside a plane and still count as meeting it.
const BOX_EPS: f32 = 0.000_2;
/// The most corners a doorway's view is made from: a doorway clipped to more keeps the view it
/// was seen through.
const MOST_CORNERS: usize = 32;

fn v(p: Vec3) -> V {
    V::new(p.x, p.y, p.z)
}

fn sub(a: V, b: V) -> V {
    V::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn add(a: V, b: V) -> V {
    V::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn scale(a: V, k: f32) -> V {
    V::new(a.x * k, a.y * k, a.z * k)
}

fn dot(a: V, b: V) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: V, b: V) -> V {
    V::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

fn unit(a: V) -> V {
    let l = dot(a, a).sqrt();
    if l > 0.0 {
        scale(a, 1.0 / l)
    } else {
        a
    }
}

/// Where a box lies against a view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Outside,
    Partly,
    Inside,
}

/// A view: planes whose positive side is inside, all through the camera but the near plane.
#[derive(Clone, Debug, PartialEq)]
pub struct Frustum {
    pub planes: Vec<Plane>,
    /// Where the camera stands.
    pub eye: V,
}

impl Frustum {
    /// The camera's view from `eye` along its `right`, `forward` and `up`: half angles of
    /// `across` and `high` degrees over two, the sides making them across and the top and bottom
    /// at ninety degrees less from the forward axis, and the near plane `near` ahead.
    // D2-10632, D2-10635
    #[must_use]
    pub fn camera(eye: V, right: V, forward: V, up: V, across: f32, high: f32, near: f32) -> Self {
        let h = across * 0.008_726_646;
        let w = high * 0.008_726_646;
        let (sh, ch, sw, cw) = (sinf(h), cosf(h), sinf(w), cosf(w));
        let axes =
            |r: f32, f: f32, u: f32| add(add(scale(right, r), scale(forward, f)), scale(up, u));
        let mut planes: Vec<Plane> = [
            axes(ch, sh, 0.0),
            axes(-ch, sh, 0.0),
            axes(0.0, cw, -sw),
            axes(0.0, cw, sw),
        ]
        .into_iter()
        .map(|n| Plane::new(n, -dot(n, eye)))
        .collect();
        planes.push(Plane::new(
            forward,
            -dot(forward, add(eye, scale(forward, near))),
        ));
        Self { planes, eye }
    }

    /// The view of `camera`, the picture `aspect` times as wide as it is high.
    #[must_use]
    pub fn of_camera(camera: &Camera, aspect: f32) -> Self {
        let (h, p) = (camera.heading.to_radians(), camera.pitch.to_radians());
        let forward = Vec3::new(sinf(h) * cosf(p), cosf(h) * cosf(p), sinf(p));
        let (right, up) = crate::scene::view_axes(forward, camera.roll);
        let (right, forward, up) = (v(right), v(forward), v(up));
        Self::camera(
            v(camera.position),
            right,
            forward,
            up,
            aspect * camera.fov_y,
            camera.fov_y,
            camera.near,
        )
    }

    /// The view through `corners` (a doorway clipped to this view) from this view's camera: one
    /// plane through the camera along each edge, inside where a polygon running clockwise as the
    /// camera sees it lies; none for a polygon of too many corners.
    // D2-10636
    #[must_use]
    pub fn through(&self, corners: &[V]) -> Option<Self> {
        let n = corners.len();
        if n >= MOST_CORNERS {
            return None;
        }
        let c = self.eye;
        let planes = (0..n)
            .map(|i| {
                let a = sub(corners[i], c);
                let b = sub(corners[(i + 1) % n], c);
                let normal = unit(cross(a, b));
                Plane::new(normal, -dot(normal, c))
            })
            .collect();
        Some(Self { planes, eye: c })
    }

    /// The part of the polygon `corners` this view shows, when any of it shows: the whole of it
    /// when the camera stands within a centimeter of its plane, else it clipped to every plane (to
    /// all but the last when `all` is false), none when it lies outside one.
    // D2-10638, D2-04183
    #[must_use]
    pub fn check_polygon(&self, corners: &[V], all: bool) -> Option<Vec<V>> {
        let plane = polygon_plane(corners);
        if (dot(plane.normal, self.eye) + plane.d).abs() <= CLIP_EPS {
            return Some(corners.to_vec());
        }
        let k = if all {
            self.planes.len()
        } else {
            self.planes.len().saturating_sub(1)
        };
        let mut w = WorkPolygon {
            corners: corners.to_vec(),
        };
        for p in &self.planes[..k] {
            if w.clip(p, true, CLIP_EPS) == Sidedness::Negative {
                return None;
            }
        }
        Some(w.corners)
    }

    /// Where the box from `lo` to `hi` lies against this view: outside when its most positive
    /// corner lies outside a plane, inside when its most negative corner lies inside every plane.
    // D2-10633
    #[must_use]
    pub fn relation(&self, lo: V, hi: V) -> Relation {
        let mut inside = true;
        for p in &self.planes {
            let n = p.normal;
            let pick = |k: f32, l: f32, h: f32| if k < 0.0 { (h, l) } else { (l, h) };
            let (px, qx) = pick(n.x, lo.x, hi.x);
            let (py, qy) = pick(n.y, lo.y, hi.y);
            let (pz, qz) = pick(n.z, lo.z, hi.z);
            if dot(n, V::new(qx, qy, qz)) + p.d < -BOX_EPS {
                return Relation::Outside;
            }
            if inside && dot(n, V::new(px, py, pz)) + p.d < -BOX_EPS {
                inside = false;
            }
        }
        if inside {
            Relation::Inside
        } else {
            Relation::Partly
        }
    }
}

/// The plane of the polygon `corners`: the unit sum of the fan's cross products from the first
/// corner, through the corners' mean.
fn polygon_plane(corners: &[V]) -> Plane {
    let mut n = V::new(0.0, 0.0, 0.0);
    if let Some(&first) = corners.first() {
        for k in 1..corners.len().saturating_sub(1) {
            n = add(n, cross(sub(corners[k], first), sub(corners[k + 1], first)));
        }
    }
    let n = unit(n);
    #[allow(clippy::cast_precision_loss)] // a doorway has a handful of corners
    let count = corners.len().max(1) as f32;
    let sum: f32 = corners.iter().map(|&c| dot(n, c)).sum();
    Plane::new(n, -sum / count)
}

/// What lies beyond a doorway.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beyond {
    /// The land outside.
    Outside,
    /// A cell of the block, by its low word.
    Cell(u16),
}

/// A doorway of a cell: its polygon in the cell mesh's own frame, and what lies beyond.
#[derive(Clone, Debug, PartialEq)]
pub struct Doorway {
    pub corners: Vec<Vec3>,
    pub beyond: Beyond,
}

/// A cell the camera's cell sees, as the walk takes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Room {
    /// The cell mesh's frame to the frame's world.
    pub place: Mat4,
    /// The box about the cell in the frame's world.
    pub lo: Vec3,
    pub hi: Vec3,
    /// Never culled: taken with the cells drawn with it.
    pub no_cull: bool,
    /// Open to the sky.
    pub sky: bool,
    pub doorways: Vec<Doorway>,
    /// The cells drawn with it, by low word.
    pub shared: Vec<u16>,
    /// The cells it sees, by low word.
    pub visible: Vec<u16>,
    /// The interior fog's far distance it was given when it last held the camera.
    pub fog_cut: Option<f32>,
}

impl Room {
    /// Whether a doorway leads out onto the land.
    // D2-03515
    #[must_use]
    pub fn touches_outside(&self) -> bool {
        self.doorways
            .iter()
            .any(|d| matches!(d.beyond, Beyond::Outside))
    }
}

/// The cells never culled taken together: the doorways out of them (each in its own cell's mesh
/// frame) and the culled cells beyond them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub cells: Vec<u16>,
    pub outside: Vec<Vec<Vec3>>,
    pub exits: Vec<u16>,
}

/// The cells of a frame seen from inside: the camera's cell and the cells it sees, as the walk
/// takes them.
#[derive(Clone, Debug, PartialEq)]
pub struct Rooms {
    /// Their block.
    pub block: (i32, i32),
    /// The camera's cell.
    pub root: u16,
    /// The cells the camera's cell sees: those drawn when reached, with it.
    pub stab: Vec<u16>,
    pub cells: HashMap<u16, Room>,
    /// Each never-culled cell's group.
    pub group_of: HashMap<u16, usize>,
    pub groups: Vec<Group>,
    /// The interior fog's color, as its background is painted.
    pub fog_color: [f32; 4],
}

impl Rooms {
    /// The groups of the never-culled cells among `self.cells`, made in `order` (the camera's cell
    /// and the cells it sees): a cell already in a group joins it; another makes one of itself and
    /// the cells drawn with it, whose doorways out are the group's and whose doorways into a culled
    /// cell lead out of it.
    // D2-03607
    pub fn make_groups(&mut self, order: &[u16]) {
        for &low in order {
            let Some(room) = self.cells.get(&low) else {
                continue;
            };
            if !room.no_cull || self.group_of.contains_key(&low) {
                continue;
            }
            if let Some(k) = self.groups.iter().position(|g| g.cells.contains(&low)) {
                self.group_of.insert(low, k);
                continue;
            }
            let mut g = Group {
                cells: vec![low],
                ..Group::default()
            };
            for &m in &room.shared {
                let Some(member) = self.cells.get(&m) else {
                    continue;
                };
                if !g.cells.contains(&m) {
                    g.cells.push(m);
                }
                for d in &member.doorways {
                    match d.beyond {
                        Beyond::Outside => g.outside.push(d.corners.clone()),
                        Beyond::Cell(o) => {
                            if self.cells.get(&o).is_some_and(|c| !c.no_cull)
                                && !g.exits.contains(&o)
                            {
                                g.exits.push(o);
                            }
                        }
                    }
                }
            }
            self.group_of.insert(low, self.groups.len());
            self.groups.push(g);
        }
    }

    /// What the camera sees from its cell at the moment: `camera` in the frame's world, the
    /// picture `aspect` times as wide as it is high.
    // D2-03601, D2-11032
    #[must_use]
    pub fn sight(&self, camera: &Camera, aspect: f32) -> Sight {
        let view = Frustum::of_camera(camera, aspect);
        let mut walk = Walk {
            rooms: self,
            in_walk: HashSet::new(),
            sight: Sight::default(),
        };
        walk.cell(self.root, &view, true);
        walk.sight.drawn = std::iter::once(self.root)
            .chain(self.stab.iter().copied())
            .filter(|c| walk.sight.entered.contains_key(c))
            .collect();
        walk.sight.fog_color = self.fog_color;
        walk.sight.block = Some(self.block);
        walk.sight
    }

    /// Whether some of cell `room` shows through `view`, a view onto the land: its box meets the
    /// view, and it is never culled or a doorway of it shows. A camera on the land looks by the
    /// doorways out; a camera inside (`viewer_inside`), out through a doorway of its own, by those
    /// between cells.
    // D2-03576
    #[must_use]
    pub fn shows_from_outside(room: &Room, view: &Frustum, viewer_inside: bool) -> bool {
        if view.relation(v(room.lo), v(room.hi)) == Relation::Outside {
            return false;
        }
        room.no_cull
            || room.doorways.iter().any(|d| {
                matches!(d.beyond, Beyond::Outside) != viewer_inside
                    && view
                        .check_polygon(&Walk::placed(&d.corners, &room.place), false)
                        .is_some()
            })
    }
}

/// A block's cells a view onto the land may look into: those it lists as seen from outside, in
/// its order.
#[derive(Clone, Debug, PartialEq)]
pub struct LookIn {
    pub block: (i32, i32),
    pub rooms: Rooms,
    pub order: Vec<u16>,
}

/// What `views`, the views onto the land, look into among `blocks`: the walk each cell shown
/// starts, the first view to show it taking it; `viewer_inside` when the camera stands in a cell.
/// Also whether a walk found a cell open to the sky.
// D2-03646, D2-03551, D2-03552
#[must_use]
pub fn look_into(
    blocks: &[LookIn],
    views: &[Frustum],
    viewer_inside: bool,
) -> (HashMap<(i32, i32), Seen>, bool) {
    let mut seen: HashMap<(i32, i32), Seen> = HashMap::new();
    let mut gathered: HashSet<((i32, i32), u16)> = HashSet::new();
    let mut sky = false;
    for view in views {
        for look in blocks {
            for &low in &look.order {
                let Some(room) = look.rooms.cells.get(&low) else {
                    continue;
                };
                if gathered.contains(&(look.block, low))
                    || !room.touches_outside()
                    || !Rooms::shows_from_outside(room, view, viewer_inside)
                {
                    continue;
                }
                gathered.insert((look.block, low));
                let mut walk = Walk {
                    rooms: &look.rooms,
                    in_walk: HashSet::new(),
                    sight: Sight::default(),
                };
                walk.cell(low, view, true);
                sky |= walk.sight.sky;
                let into = seen.entry(look.block).or_default();
                for (cell, views) in walk.sight.entered {
                    if cell == low || room.visible.contains(&cell) {
                        into.drawn.insert(cell);
                    }
                    into.entered.entry(cell).or_default().extend(views);
                }
            }
        }
    }
    (seen, sky)
}

/// What culls a body standing in cell `cell` (a full id), touching `touching` too, drawn by
/// `batches`: on the land its box, inside the interior cells it touches of its cell's block with
/// its box; each box twice its size about its middle. Drawn whatever the camera sees when it has
/// no vertex or touches too many cells to name.
// D2-03577, D2-03583, D2-03646
#[must_use]
pub fn body_cull(cell: u32, touching: &[u32], batches: &[ObjectBatch]) -> Cull {
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut last: Option<(usize, Mat4)> = None;
    for b in batches {
        let here = (b.places(), b.transform);
        if last == Some(here) {
            continue;
        }
        last = Some(here);
        b.each_position(|[x, y, z]| {
            let w = b.transform.point(Vec3::new(x, y, z));
            lo = Vec3::new(lo.x.min(w.x), lo.y.min(w.y), lo.z.min(w.z));
            hi = Vec3::new(hi.x.max(w.x), hi.y.max(w.y), hi.z.max(w.z));
        });
    }
    if lo.x > hi.x {
        return Cull::Never;
    }
    let twice = |l: f32, h: f32| (l + 0.5 * (l - h), h + 0.5 * (h - l));
    let (lx, hx) = twice(lo.x, hi.x);
    let (ly, hy) = twice(lo.y, hi.y);
    let (lz, hz) = twice(lo.z, hi.z);
    let (lo, hi) = (Vec3::new(lx, ly, lz), Vec3::new(hx, hy, hz));
    if cell & 0xFFFF < FIRST_INTERIOR {
        return Cull::Box { lo, hi };
    }
    let mut cells = [0u16; BODY_CELLS];
    let mut n = 0;
    for &c in std::iter::once(&cell).chain(touching) {
        if c >> 16 != cell >> 16 || c & 0xFFFF < FIRST_INTERIOR {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)] // the low word
        let low = c as u16;
        if cells[..n].contains(&low) {
            continue;
        }
        if n == BODY_CELLS {
            return Cull::Never;
        }
        cells[n] = low;
        n += 1;
    }
    #[allow(clippy::cast_possible_wrap)] // a block's coordinates are a byte each
    let block = ((cell >> 24) as i32, ((cell >> 16) & 0xFF) as i32);
    Cull::Body {
        block,
        cells,
        lo,
        hi,
    }
}

/// What a block's cells show of themselves to the views onto the land.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    /// The cells reached, each with the views it was entered by.
    pub entered: HashMap<u16, Vec<Frustum>>,
    /// The cells drawn.
    pub drawn: HashSet<u16>,
}

/// What a frame seen from inside shows at the moment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sight {
    /// The cells reached, each with the views it was entered by.
    pub entered: HashMap<u16, Vec<Frustum>>,
    /// The cells drawn: those reached of the camera's cell and the cells it sees.
    pub drawn: HashSet<u16>,
    /// The views out onto the land.
    pub outside: Vec<Frustum>,
    /// A cell reached is open to the sky.
    pub sky: bool,
    /// A cell was left out for the interior fog: the background is painted its color.
    pub fog: bool,
    pub fog_color: [f32; 4],
    /// The camera stands on the land: all of it is in view, whatever its boxes.
    pub on_land: bool,
    /// The block of the camera's cell.
    pub block: Option<(i32, i32)>,
    /// What the views onto the land look into, by block.
    pub seen: HashMap<(i32, i32), Seen>,
}

impl Sight {
    /// What a camera on the land sees at the moment: its `view` onto the land, and what that looks
    /// into among `blocks`.
    #[must_use]
    pub fn on_land(view: Frustum, blocks: &[LookIn]) -> Self {
        let (seen, _) = look_into(blocks, std::slice::from_ref(&view), false);
        Self {
            outside: vec![view],
            on_land: true,
            seen,
            ..Self::default()
        }
    }

    /// Look through the views out of a camera's cell into `blocks`.
    pub fn look_out(&mut self, blocks: &[LookIn]) {
        let (seen, sky) = look_into(blocks, &self.outside, true);
        self.seen = seen;
        self.sky |= sky;
    }

    /// Whether the land, its water and the light outdoors are drawn: a view out onto it.
    #[must_use]
    pub fn outdoors(&self) -> bool {
        !self.outside.is_empty()
    }

    /// Whether the sky is drawn: a view out, or a cell open to it reached.
    #[must_use]
    pub fn sky(&self) -> bool {
        self.sky || self.outdoors()
    }

    /// Whether draw `b` is drawn.
    // D2-03592, D2-03590
    #[must_use]
    pub fn shows(&self, b: &ObjectBatch) -> bool {
        self.shows_cull(b.cull)
    }

    /// Whether a draw culled by `cull` is drawn.
    #[must_use]
    pub fn shows_cull(&self, cull: Cull) -> bool {
        match cull {
            Cull::Never => true,
            Cull::Cell(c) => self.drawn.contains(&c),
            Cull::InCell { cell, lo, hi } => {
                self.drawn.contains(&cell)
                    && self.entered.get(&cell).is_some_and(|views| {
                        views
                            .iter()
                            .any(|f| f.relation(v(lo), v(hi)) != Relation::Outside)
                    })
            }
            Cull::Box { lo, hi } => {
                self.on_land
                    || self
                        .outside
                        .iter()
                        .any(|f| f.relation(v(lo), v(hi)) != Relation::Outside)
            }
            Cull::Looked { block, cell } => self
                .seen
                .get(&block)
                .is_some_and(|s| s.drawn.contains(&cell)),
            Cull::LookedIn {
                block,
                cell,
                lo,
                hi,
            } => self.seen.get(&block).is_some_and(|s| {
                s.drawn.contains(&cell)
                    && s.entered.get(&cell).is_some_and(|views| {
                        views
                            .iter()
                            .any(|f| f.relation(v(lo), v(hi)) != Relation::Outside)
                    })
            }),
            Cull::Body {
                block,
                cells,
                lo,
                hi,
            } => {
                let meets = |views: &Vec<Frustum>| {
                    views
                        .iter()
                        .any(|f| f.relation(v(lo), v(hi)) != Relation::Outside)
                };
                let inside = self.block == Some(block);
                cells.iter().filter(|&&c| c != 0).any(|c| {
                    (inside && self.drawn.contains(c) && self.entered.get(c).is_some_and(meets))
                        || self.seen.get(&block).is_some_and(|s| {
                            s.drawn.contains(c) && s.entered.get(c).is_some_and(meets)
                        })
                })
            }
        }
    }
}

/// The walk's state: the cells being walked through (a cell is not entered again while the walk
/// is in it, but may be by another way afterwards), and what was seen.
struct Walk<'a> {
    rooms: &'a Rooms,
    in_walk: HashSet<u16>,
    sight: Sight,
}

impl Walk<'_> {
    /// Whether the box from `lo` to `hi` lies wholly past the fog's far distance `far`: every
    /// corner at least that far from the camera.
    // D2-03488
    fn past_fog(&self, eye: V, lo: Vec3, hi: Vec3, far: f32) -> bool {
        (0..8).all(|k| {
            let c = V::new(
                if k & 1 == 0 { lo.x } else { hi.x },
                if k & 2 == 0 { lo.y } else { hi.y },
                if k & 4 == 0 { lo.z } else { hi.z },
            );
            let d = sub(c, eye);
            dot(d, d) >= far * far
        })
    }

    /// The doorway `corners` (in its cell mesh's frame) as the frame's world has it.
    fn placed(corners: &[Vec3], place: &Mat4) -> Vec<V> {
        corners.iter().map(|&c| v(place.point(c))).collect()
    }

    /// Enter cell `low` with the view `view`.
    // D2-03589, D2-03587, D2-03518
    fn cell(&mut self, low: u16, view: &Frustum, root: bool) {
        if self.in_walk.contains(&low) {
            return;
        }
        let rooms = self.rooms;
        let Some(room) = rooms.cells.get(&low) else {
            return;
        };
        if view.relation(v(room.lo), v(room.hi)) == Relation::Outside {
            return;
        }
        let fog_cut = if root { None } else { room.fog_cut };
        if let Some(far) = fog_cut {
            if self.past_fog(view.eye, room.lo, room.hi, far) {
                self.sight.fog = true;
                return;
            }
        }
        self.in_walk.insert(low);
        self.sight
            .entered
            .entry(low)
            .or_default()
            .push(view.clone());
        self.sight.sky |= room.sky;
        if room.no_cull {
            self.group(low, room, view);
        } else {
            for d in &room.doorways {
                let beyond = match d.beyond {
                    Beyond::Outside => None,
                    Beyond::Cell(o) => match rooms.cells.get(&o) {
                        Some(r) => Some((o, r)),
                        None => continue,
                    },
                };
                let no_cull = beyond.is_some_and(|(_, r)| r.no_cull);
                let corners = Self::placed(&d.corners, &room.place);
                let shown = if no_cull {
                    None
                } else {
                    match view.check_polygon(&corners, !root) {
                        Some(c) => Some(c),
                        None => continue,
                    }
                };
                let through = shown.and_then(|c| view.through(&c));
                let child = through.as_ref().unwrap_or(view);
                match beyond {
                    None => self.sight.outside.push(child.clone()),
                    Some((o, _)) => self.cell(o, child, false),
                }
            }
        }
        self.in_walk.remove(&low);
    }

    /// Enter the group of never-culled cell `low` (`room`) with `view`: every cell of the group
    /// whole (no view checked), the group's doorways out each adding its whole view, the culled
    /// cells beyond walked on with `view`.
    // D2-03588
    fn group(&mut self, low: u16, room: &Room, view: &Frustum) {
        let rooms = self.rooms;
        let Some(g) = rooms.group_of.get(&low).and_then(|&k| rooms.groups.get(k)) else {
            return;
        };
        let mut members = Vec::new();
        for &m in &room.shared {
            let Some(member) = rooms.cells.get(&m) else {
                continue;
            };
            self.in_walk.insert(m);
            members.push(m);
            if let Some(far) = room.fog_cut {
                if self.past_fog(view.eye, member.lo, member.hi, far) {
                    self.sight.fog = true;
                    continue;
                }
            }
            // the cell the group was entered by holds this view already
            if m != low {
                self.sight.entered.entry(m).or_default().push(view.clone());
            }
            self.sight.sky |= member.sky;
        }
        for corners in &g.outside {
            let placed = Self::placed(corners, &room.place);
            let out = view.through(&placed).unwrap_or_else(|| view.clone());
            self.sight.outside.push(out);
        }
        for &e in &g.exits {
            if rooms.cells.get(&e).is_some_and(|r| !r.no_cull) {
                self.cell(e, view, false);
            }
        }
        for m in members {
            self.in_walk.remove(&m);
        }
    }
}
