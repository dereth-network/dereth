//! What the last world frame showed, for the interface: the camera it was drawn from and each
//! body drawn in it with the box its drawn shape fills, so a panel can find where a body is on
//! the screen (a name over its head, the selection's marks) and a click can find the body under
//! the pointer.
//!
//! Every place is in the frame's block's coordinates (meters, z up). A body's box is in its own
//! frame, turned with its heading: what its parts fill as they are drawn this frame, so it moves
//! with the body's animation.

use std::ops::Range;

use auberean_draw::scene::ObjectBatch;
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
        let (s, c) = (
            fm::sinf(self.heading.to_radians()),
            fm::cosf(self.heading.to_radians()),
        );
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
    /// The interior cells in the visible set, each cell's origin (its record's position), in the
    /// player's landblock's coordinates: outdoors, those the near window's landblocks see from
    /// outside; inside, the player's cell and those it sees, and when it sees the outdoors those
    /// the window's landblocks see from outside too. Only cells the land holds.
    pub cells: Vec<Vec3>,
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
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    let mut any = false;
    for b in batches {
        for v in b.vertices.iter() {
            let w = b
                .transform
                .point(Vec3::new(v.position[0], v.position[1], v.position[2]));
            let p = body.to_body(w);
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            any = true;
        }
    }
    any.then_some((lo, hi))
}

/// The body a ray from the camera through the pointer meets first, by the triangles drawn for
/// each body (`drawn`: each body's id and its batches' range in `batches`). The pointer is in
/// window pixels of a window `size`, taken at the pixel's middle.
// D2-11014, D2-10561, D2-10613, D2-06114
#[must_use]
pub fn pick(
    camera: &SeenCamera,
    size: (f32, f32),
    pointer: (f32, f32),
    batches: &[ObjectBatch],
    drawn: &[(u64, Range<usize>)],
) -> Option<u64> {
    let (w, h) = size;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let cx = (2.0 * pointer.0 + 1.0) / w - 1.0;
    let cy = 1.0 - (2.0 * pointer.1 + 1.0) / h;
    let t = fm::tanf((camera.fov_y * 0.5).to_radians());
    let (f, r, u) = camera.axes();
    let dir = math::normalize(math::add(
        f,
        math::add(math::scale(r, cx * t * w / h), math::scale(u, cy * t)),
    ));
    let from = camera.position;
    let mut best: Option<(f32, u64)> = None;
    for (id, range) in drawn {
        let Some(own) = batches.get(range.clone()) else {
            continue;
        };
        for b in own {
            let tris = crate::view::triangles(b);
            let length = crate::view::RAY_LENGTH;
            if let Some(hit) = crate::view::ray_intersects_tri_list(from, dir, length, tris, true) {
                if best.is_none_or(|(bd, _)| hit.fraction < bd) {
                    best = Some((hit.fraction, *id));
                }
            }
        }
    }
    best.map(|(_, id)| id)
}
