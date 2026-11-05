//! What the last world frame showed, for the interface: the camera it was drawn from and each
//! body drawn in it with the box its drawn shape fills, so a panel can find where a body is on
//! the screen (a name over its head, the selection's marks) and a click can find the body under
//! the pointer.
//!
//! Every place is in the frame's block's coordinates (meters, z up). A body's box is in its own
//! frame, turned with its heading: what its parts fill as they are drawn this frame, so it moves
//! with the body's animation.

use std::ops::Range;

use auberean_draw::scene::{widen, ObjectBatch};
use auberean_primitives::Vec3;
use auberean_world::math;
use dereth_primitives::num::math as fm;

/// The camera a world frame was drawn from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeenCamera {
    pub position: Vec3,
    /// Degrees clockwise from north (+y).
    pub heading: f32,
    /// Degrees above level.
    pub pitch: f32,
    /// The vertical field of view, degrees.
    pub fov_y: f32,
    pub near: f32,
}

impl SeenCamera {
    /// Its forward, right and up axes.
    #[must_use]
    pub fn axes(&self) -> (Vec3, Vec3, Vec3) {
        let (h, p) = (self.heading.to_radians(), self.pitch.to_radians());
        let f = Vec3::new(
            fm::sinf(h) * fm::cosf(p),
            fm::cosf(h) * fm::cosf(p),
            fm::sinf(p),
        );
        let r = math::normalize(math::cross(f, Vec3::new(0.0, 0.0, 1.0)));
        let u = math::cross(r, f);
        (f, r, u)
    }

    /// Where a point shows on a screen `width` by `height` pixels: its x and y from the top left
    /// and its distance along the camera's view; `None` when it is not in front of the camera.
    #[must_use]
    pub fn project(&self, p: Vec3, width: f32, height: f32) -> Option<(f32, f32, f32)> {
        let (f, r, u) = self.axes();
        let d = math::sub(p, self.position);
        let z = math::dot(d, f);
        if z <= self.near.max(1e-4) || height <= 0.0 {
            return None;
        }
        let t = fm::tanf((self.fov_y * 0.5).to_radians());
        let aspect = width / height;
        let nx = math::dot(d, r) / (z * t * aspect);
        let ny = math::dot(d, u) / (z * t);
        Some(((nx + 1.0) * 0.5 * width, (1.0 - ny) * 0.5 * height, z))
    }
}

/// A body drawn in the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct SeenBody {
    /// Its object's id.
    pub id: u64,
    /// Where it stands, and its heading (degrees clockwise from north).
    pub origin: Vec3,
    pub heading: f32,
    /// The box its drawn shape fills, in its own frame turned with its heading: least and most
    /// corners. `None` when nothing of it was drawn.
    pub bounds: Option<(Vec3, Vec3)>,
    /// Its animation has faded it out (its opacity under 0.0002): it is not to be seen.
    pub faded: bool,
}

impl SeenBody {
    /// A point of the body's own frame in the block's.
    #[must_use]
    pub fn to_block(&self, p: Vec3) -> Vec3 {
        let (s, c) = (
            fm::sinf(self.heading.to_radians()),
            fm::cosf(self.heading.to_radians()),
        );
        Vec3::new(
            self.origin.x + p.x * c + p.y * s,
            self.origin.y - p.x * s + p.y * c,
            self.origin.z + p.z,
        )
    }

    /// A point of the block's frame in the body's own.
    #[must_use]
    pub fn to_body(&self, p: Vec3) -> Vec3 {
        self.to_body_turned(p, self.turn())
    }

    /// The sine and cosine of the body's heading.
    fn turn(&self) -> (f32, f32) {
        (
            fm::sinf(self.heading.to_radians()),
            fm::cosf(self.heading.to_radians()),
        )
    }

    /// [`Self::to_body`], given [`Self::turn`].
    fn to_body_turned(&self, p: Vec3, (s, c): (f32, f32)) -> Vec3 {
        let d = math::sub(p, self.origin);
        Vec3::new(d.x * c - d.y * s, d.x * s + d.y * c, d.z)
    }

    /// The eight corners of a box of the body's own frame, in the block's.
    #[must_use]
    pub fn corners(&self, (lo, hi): (Vec3, Vec3)) -> [Vec3; 8] {
        let mut out = [Vec3::new(0.0, 0.0, 0.0); 8];
        for (i, c) in out.iter_mut().enumerate() {
            *c = self.to_block(Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            ));
        }
        out
    }
}

/// What the last world frame showed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    /// The camera, once a world frame was drawn.
    pub camera: Option<SeenCamera>,
    /// The block the frame's places are in.
    pub block: (i32, i32),
    pub bodies: Vec<SeenBody>,
    /// What the ambient sound hears about the player (`None` without a player in the world).
    pub hearing: Option<SeenHearing>,
}

/// Where the player is heard from, as the cells about it stand after this frame's update.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeenHearing {
    /// The player's cell id (`0xXXYYnnnn`) and place, in the coordinates of the player's
    /// landblock (the block the world's frame is drawn in).
    pub cell: u32,
    pub place: Vec3,
    /// The cells about the player are being waited for (a placement blocking): the ambient
    /// update skips this frame.
    pub blocking: bool,
    /// The interior cells in the visible set, each with the kind of ambient sound it names and its
    /// origin (its record's position), in the player's landblock's coordinates: outdoors, those the near window's landblocks see from
    /// outside; inside, the player's cell and those it sees, and when it sees the outdoors those
    /// the window's landblocks see from outside too. Only cells the land holds.
    pub cells: Vec<(u32, Vec3)>,
    /// The landscape's points are heard: outdoors, or the player's interior cell is not in the
    /// visible set or sees the outdoors.
    pub outside: bool,
    /// The near window's landblocks (x, y), those the window holds now (none while shut in).
    pub blocks: Vec<(u8, u8)>,
}

impl Seen {
    /// The body of object `id`, if it was drawn.
    #[must_use]
    pub fn body(&self, id: u64) -> Option<&SeenBody> {
        self.bodies.iter().find(|b| b.id == id)
    }
}

/// The box a body's batches fill in its own frame.
#[must_use]
pub fn bounds_of(body: &SeenBody, batches: &[ObjectBatch]) -> Option<(Vec3, Vec3)> {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut any = false;
    let turn = body.turn();
    // The layers and fragments of one part share its vertices, in one place.
    let mut done: Vec<(usize, [[f32; 4]; 4])> = Vec::new();
    for b in batches {
        let this = (b.places(), b.transform.0);
        if done.contains(&this) {
            continue;
        }
        done.push(this);
        b.each_position(|[x, y, z]| {
            let w = b.transform.point(Vec3::new(x, y, z));
            let p = body.to_body_turned(w, turn);
            widen(&mut lo, &mut hi, [p.x, p.y, p.z]);
            any = true;
        });
    }
    let v = |[x, y, z]: [f32; 3]| Vec3::new(x, y, z);
    any.then_some((v(lo), v(hi)))
}

/// The square of clip space a press of the pointer selects on a screen `size`: from the pixel up
/// and to the left of the one it is on, to that one, each by its corner: least and most x, then y.
// D2-10536, D2-10570
#[must_use]
pub fn selection_bounds((w, h): (f32, f32), (x, y): (f32, f32)) -> (f32, f32, f32, f32) {
    let clip = |x: f32, y: f32| ((x + x) / w - 1.0 - 1.0 / w, -((y + y) / h - 1.0) - 1.0 / h);
    let (a, b) = (clip(x, y), clip(x - 1.0, y - 1.0));
    (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1))
}

/// The ray from the camera through the least corner of what the pointer selects.
// D2-10586, D2-10576
fn selection_ray(camera: &SeenCamera, (w, h): (f32, f32), pointer: (f32, f32)) -> Vec3 {
    let (cx, cy, ..) = selection_bounds((w, h), pointer);
    let t = fm::tanf((camera.fov_y * 0.5).to_radians());
    let (f, r, u) = camera.axes();
    math::normalize(math::add(
        f,
        math::add(math::scale(r, cx * t * w / h), math::scale(u, cy * t)),
    ))
}

/// The share of the ray from the camera through the pointer at which it first meets a body's drawn
/// triangles.
// D2-05963
fn trace_ray(from: Vec3, dir: Vec3, own: &[ObjectBatch]) -> Option<f32> {
    let mut best = None;
    for b in own {
        let tris = crate::view::triangles(b);
        let length = crate::view::RAY_LENGTH;
        if let Some(hit) = crate::view::ray_intersects_tri_list(from, dir, length, tris, true) {
            if best.is_none_or(|bd| hit.fraction < bd) {
                best = Some(hit.fraction);
            }
        }
    }
    best
}

/// The body a ray from the camera through the pointer meets first, by the triangles drawn for
/// each body (`drawn`: each body's id and its batches' range in `batches`). The pointer is in
/// window pixels of a window `size`.
// D2-11014
#[must_use]
pub fn pick(
    camera: &SeenCamera,
    size: (f32, f32),
    pointer: (f32, f32),
    batches: &[ObjectBatch],
    drawn: &[(u64, Range<usize>)],
) -> Option<u64> {
    if size.0 <= 0.0 || size.1 <= 0.0 {
        return None;
    }
    let dir = selection_ray(camera, size, pointer);
    let mut best: Option<(f32, u64)> = None;
    for (id, range) in drawn {
        let Some(own) = batches.get(range.clone()) else {
            continue;
        };
        if let Some(fraction) = trace_ray(camera.position, dir, own) {
            if best.is_none_or(|(bd, _)| fraction < bd) {
                best = Some((fraction, *id));
            }
        }
    }
    best.map(|(_, id)| id)
}

/// How a press in the world may fall on a body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Selectable {
    /// Not at all: an object the client made itself, or the player and what it holds with the
    /// camera at its eyes.
    No,
    /// Any body: by its triangles, or failing all of those by its box.
    Yes,
    /// The player's own body: only by its triangles, and behind everything else there.
    Player,
}

/// The least deep body yet, with its depth.
type Nearest = Option<(f32, u64)>;

/// How far back the player's own body stands among what a press meets.
const PLAYER_DEPTH: f32 = 999_999.0;

/// The box a body's drawn parts fill as they stand this frame, in its own frame: each part's own
/// box of its triangles taken into the body's frame by its eight corners. `None` with nothing drawn.
// D2-06119, D2-06625, D2-06626, D2-06680, D2-05535
pub(crate) fn render_box(body: &SeenBody, own: &[ObjectBatch]) -> Option<(Vec3, Vec3)> {
    let mut out: Option<(Vec3, Vec3)> = None;
    // A part's materials lie next to each other, under one placement.
    for part in own.chunk_by(|a, b| a.transform == b.transform) {
        let mut bounds: Option<([f32; 3], [f32; 3])> = None;
        for b in part {
            b.each_position(|at| {
                let (lo, hi) = bounds.get_or_insert((at, at));
                for i in 0..3 {
                    lo[i] = lo[i].min(at[i]);
                    hi[i] = hi[i].max(at[i]);
                }
            });
        }
        let Some((lo, hi)) = bounds else {
            continue;
        };
        for i in 0..8 {
            let at = |bit: usize, axis: usize| if i & bit == 0 { lo[axis] } else { hi[axis] };
            let w = part[0]
                .transform
                .point(Vec3::new(at(1, 0), at(2, 1), at(4, 2)));
            let p = body.to_body(w);
            let (a, b) = out.get_or_insert((p, p));
            *a = Vec3::new(a.x.min(p.x), a.y.min(p.y), a.z.min(p.z));
            *b = Vec3::new(b.x.max(p.x), b.y.max(p.y), b.z.max(p.z));
        }
    }
    out
}

/// Whether a body's box lies under what the pointer selects: at least one of its corners has to
/// stand on the screen, and the square of clip space its corners fill has to meet the pointer's.
/// How deep it reaches if it does (the greatest depth among its corners, deeper than any in front
/// of the camera for a corner behind it).
// D2-10561, D2-10533, D2-03479
#[must_use]
pub fn check_object_selection(
    camera: &SeenCamera,
    body: &SeenBody,
    bounds: (Vec3, Vec3),
    aspect: f32,
    selection: (f32, f32, f32, f32),
) -> Option<f32> {
    let (f, r, u) = camera.axes();
    let t = fm::tanf((camera.fov_y * 0.5).to_radians());
    let project = |corner: Vec3| {
        let d = math::sub(corner, camera.position);
        let w = math::dot(d, f);
        (
            math::dot(d, r) / (w * t * aspect),
            math::dot(d, u) / (w * t),
            w,
        )
    };
    box_under_pointer(body.corners(bounds), camera.near, project, selection)
}

/// [`check_object_selection`] for a box's `corners` and a way to `project` a point to clip space,
/// with its distance along the view.
pub(crate) fn box_under_pointer(
    corners: [Vec3; 8],
    near: f32,
    project: impl Fn(Vec3) -> (f32, f32, f32),
    (min_x, min_y, max_x, max_y): (f32, f32, f32, f32),
) -> Option<f32> {
    let (mut on_screen, mut depth) = (false, f32::NEG_INFINITY);
    let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for corner in corners {
        let (x, y, w) = project(corner);
        on_screen |= w >= 0.0 && x.abs() <= 1.0 && y.abs() <= 1.0;
        depth = depth.max(1.0 - near / w);
        lo = (lo.0.min(x), lo.1.min(y));
        hi = (hi.0.max(x), hi.1.max(y));
    }
    (on_screen && lo.0 <= max_x && min_x <= hi.0 && lo.1 <= max_y && min_y <= hi.1).then_some(depth)
}

/// The body a press of the pointer falls on in the world frame: of the bodies under it by their
/// boxes, the nearest whose drawn triangles the ray from the camera meets; failing any, the one
/// whose box reaches least deep. A press finds nothing on the bodies it may not fall on.
// D2-10586, D2-10612, D2-10613, D2-10561, D2-06114
#[must_use]
pub fn select(
    camera: &SeenCamera,
    size: (f32, f32),
    pointer: (f32, f32),
    batches: &[ObjectBatch],
    drawn: &[(u64, Range<usize>)],
    bodies: &[SeenBody],
    selectable: impl Fn(u64) -> Selectable,
) -> Option<u64> {
    if size.0 <= 0.0 || size.1 <= 0.0 {
        return None;
    }
    let bounds = selection_bounds(size, pointer);
    let dir = selection_ray(camera, size, pointer);
    let (mut met, mut under): (Nearest, Nearest) = (None, None);
    let nearer = |best: &mut Nearest, depth: f32, id: u64| {
        if best.is_none_or(|(d, _)| depth < d) {
            *best = Some((depth, id));
        }
    };
    for (id, range) in drawn {
        let kind = selectable(*id);
        if kind == Selectable::No {
            continue;
        }
        let (Some(own), Some(body)) = (
            batches.get(range.clone()),
            bodies.iter().find(|b| b.id == *id),
        ) else {
            continue;
        };
        let Some(boxed) = render_box(body, own) else {
            continue;
        };
        let Some(depth) = check_object_selection(camera, body, boxed, size.0 / size.1, bounds)
        else {
            continue;
        };
        match (trace_ray(camera.position, dir, own), kind) {
            (Some(_), Selectable::Player) => nearer(&mut met, PLAYER_DEPTH, *id),
            (Some(fraction), _) => nearer(&mut met, fraction, *id),
            (None, Selectable::Player) => {}
            (None, _) => nearer(&mut under, depth, *id),
        }
    }
    met.or(under).map(|(_, id)| id)
}
