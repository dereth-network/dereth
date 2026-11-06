//! Where things are on the screen: the free camera's view of the world as the world's device
//! draws it (the same eye, look and field of view), points projected to pixels and pixels turned
//! back into rays, and places in a landblock as points on the world's one grid.
//!
//! The world's grid: meters east and north of the world's south-west corner, and up. A landblock
//! `XXYY` is 160 meters a side, its south-west corner at `(XX * 160, YY * 160)`.

use dereth_builder::camera::FlyCamera;

/// A landblock's side, meters.
pub const BLOCK: f32 = 160.0;
/// The nearest the device draws, meters.
pub const NEAR: f32 = 0.1;

/// A point of landblock `lb` (meters from its south-west corner) on the world's grid.
#[must_use]
pub fn global(lb: u16, at: [f32; 3]) -> [f32; 3] {
    let [bx, by] = lb.to_be_bytes().map(f32::from);
    [bx * BLOCK + at[0], by * BLOCK + at[1], at[2]]
}

/// A point of the world's grid as a landblock and a point in it.
#[must_use]
pub fn local(g: [f32; 3]) -> (u16, [f32; 3]) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let block = |v: f32| (v / BLOCK).floor().clamp(0.0, 255.0) as u8;
    let (bx, by) = (block(g[0]), block(g[1]));
    let lb = u16::from(bx) << 8 | u16::from(by);
    (
        lb,
        [
            g[0] - f32::from(bx) * BLOCK,
            g[1] - f32::from(by) * BLOCK,
            g[2],
        ],
    )
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    if l == 0.0 {
        v
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

/// The camera's view of the world on a window of `size` pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// The eye, on the world's grid.
    pub eye: [f32; 3],
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    /// The vertical field of view, degrees.
    pub fov_y: f32,
    pub size: (u32, u32),
}

impl View {
    /// The free camera's view.
    #[must_use]
    pub fn of(camera: &FlyCamera, fov_y: f32, size: (u32, u32)) -> Self {
        let f = camera.forward();
        let forward = norm([f.x, f.y, f.z]);
        let right = norm(cross(forward, [0.0, 0.0, 1.0]));
        let up = cross(right, forward);
        Self {
            eye: [camera.position.x, camera.position.y, camera.position.z],
            forward,
            right,
            up,
            fov_y,
            size,
        }
    }

    /// The heading the device draws from: degrees clockwise from north.
    #[must_use]
    pub fn heading(&self) -> f32 {
        dereth_primitives::num::math::atan2f(self.forward[0], self.forward[1])
            .to_degrees()
            .rem_euclid(360.0)
    }

    /// The pitch the device draws at: degrees above level.
    #[must_use]
    pub fn pitch(&self) -> f32 {
        dereth_primitives::num::math::asinf(self.forward[2].clamp(-1.0, 1.0)).to_degrees()
    }

    fn scales(&self) -> (f32, f32) {
        let h = 1.0 / dereth_primitives::num::math::tanf(self.fov_y.to_radians() * 0.5);
        #[allow(clippy::cast_precision_loss)]
        let aspect = self.size.0.max(1) as f32 / self.size.1.max(1) as f32;
        (h / aspect, h)
    }

    /// Where a point of the world's grid falls on the window, in pixels; `None` behind the eye.
    #[must_use]
    pub fn project(&self, p: [f32; 3]) -> Option<[f32; 2]> {
        let d = sub(p, self.eye);
        let z = dot(d, self.forward);
        if z < NEAR {
            return None;
        }
        let (sx, sy) = self.scales();
        let nx = dot(d, self.right) / z * sx;
        let ny = dot(d, self.up) / z * sy;
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        Some([(nx + 1.0) * 0.5 * w, (1.0 - ny) * 0.5 * h])
    }

    /// The way from the eye through a pixel of the window, a unit vector.
    #[must_use]
    pub fn ray(&self, px: [f32; 2]) -> [f32; 3] {
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (self.size.0.max(1) as f32, self.size.1.max(1) as f32);
        let nx = px[0] / w * 2.0 - 1.0;
        let ny = 1.0 - px[1] / h * 2.0;
        let (sx, sy) = self.scales();
        let (a, b) = (nx / sx, ny / sy);
        norm([
            self.forward[0] + self.right[0] * a + self.up[0] * b,
            self.forward[1] + self.right[1] * a + self.up[1] * b,
            self.forward[2] + self.right[2] * a + self.up[2] * b,
        ])
    }
}

/// Where a ray from `from` along `dir` first meets the ground, by `ground` (its height under a
/// point of the world's grid, `None` where there is none): stepped every `step` meters out to
/// `reach`, then halved down to a centimeter.
#[must_use]
pub fn hit_ground(
    from: [f32; 3],
    dir: [f32; 3],
    reach: f32,
    step: f32,
    ground: &mut dyn FnMut(f32, f32) -> Option<f32>,
) -> Option<[f32; 3]> {
    let at = |t: f32| {
        [
            from[0] + dir[0] * t,
            from[1] + dir[1] * t,
            from[2] + dir[2] * t,
        ]
    };
    let mut below = |p: [f32; 3]| ground(p[0], p[1]).map(|g| p[2] <= g);
    let mut last = 0.0;
    let mut t = step;
    while t <= reach {
        if below(at(t)) == Some(true) {
            let (mut lo, mut hi) = (last, t);
            while hi - lo > 0.01 {
                let mid = (lo + hi) * 0.5;
                if below(at(mid)) == Some(true) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let p = at(hi);
            return Some([p[0], p[1], ground(p[0], p[1]).unwrap_or(p[2])]);
        }
        last = t;
        t += step;
    }
    None
}

/// The rotation (scalar first) a heading in degrees gives, as the world database turns a row.
#[must_use]
pub fn heading_rotation(degrees: f32) -> [f32; 4] {
    yalain_world_db::compile::heading_rotation(degrees)
}

/// How far along `dir` from `from` a line first meets a flat polygon of these corners: in lengths of
/// `dir`, ahead of `from`, from either side.
#[must_use]
pub fn ray_polygon(from: [f32; 3], dir: [f32; 3], corners: &[[f32; 3]]) -> Option<f32> {
    let &[first, ref rest @ ..] = corners else {
        return None;
    };
    rest.windows(2)
        .filter_map(|w| {
            let (a, b) = (sub(w[0], first), sub(w[1], first));
            let p = cross(dir, b);
            let det = dot(a, p);
            if det.abs() < 1e-7 {
                return None;
            }
            let to = sub(from, first);
            let u = dot(to, p) / det;
            let q = cross(to, a);
            let v = dot(dir, q) / det;
            let t = dot(b, q) / det;
            (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0).then_some(t)
        })
        .min_by(f32::total_cmp)
}

/// A point of the world's grid as a point of landblock `lb`, however far from its middle: a
/// dungeon's rooms run on past their block's own ground.
#[must_use]
pub fn local_in(lb: u16, g: [f32; 3]) -> [f32; 3] {
    let [bx, by] = lb.to_be_bytes().map(f32::from);
    [g[0] - bx * BLOCK, g[1] - by * BLOCK, g[2]]
}

/// How far along `dir` a line from `from` first meets a box; 0 from inside it.
#[must_use]
pub fn ray_box(from: [f32; 3], dir: [f32; 3], lo: [f32; 3], hi: [f32; 3]) -> Option<f32> {
    let (mut near, mut far) = (0.0_f32, f32::MAX);
    for i in 0..3 {
        if dir[i].abs() < 1e-7 {
            if from[i] < lo[i] || from[i] > hi[i] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[i] - from[i]) / dir[i], (hi[i] - from[i]) / dir[i]);
        near = near.max(a.min(b));
        far = far.min(a.max(b));
        if near > far {
            return None;
        }
    }
    Some(near)
}

/// A point turned by the opposite of a rotation (scalar first).
#[must_use]
pub fn unrotated(q: [f32; 4], p: [f32; 3]) -> [f32; 3] {
    let u = [-q[1], -q[2], -q[3]];
    let t = cross(u, p);
    let t = [2.0 * t[0], 2.0 * t[1], 2.0 * t[2]];
    let c = cross(u, t);
    [
        p[0] + q[0] * t[0] + c[0],
        p[1] + q[0] * t[1] + c[1],
        p[2] + q[0] * t[2] + c[2],
    ]
}

/// Whether a triangle is level enough to stand on, whichever way round it is wound.
#[must_use]
pub fn is_level(t: &[[f32; 3]; 3]) -> bool {
    let n = norm(cross(sub(t[1], t[0]), sub(t[2], t[0])));
    n[2].abs() > 0.5
}
