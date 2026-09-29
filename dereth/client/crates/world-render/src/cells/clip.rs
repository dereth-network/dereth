//! The screen-space polygon clip the portal traversal runs on.
//!
//! The screen-space clip of a portal opening, the software polygon clipper and the step that
//! appends a clipped polygon to a view.
//!
//! The arithmetic below preserves the exact branch order for view copying, portal clipping and
//! clip-plane construction.
//!
//! Treating every portal as "visible" instead changes **3 of 20,504 pixels**, all within 6 px
//! of an opening. It is what turns "the traversal reaches
//! every cell some portal chain touches" into "the traversal reaches every cell whose portal chain
//! still has a hole left in it after the openings have been intersected on the screen".
//!
//! # What a view polygon is
//!
//! Copying a view appends one polygon to a cell's `portal_view.view`. A cell reachable through
//! several portals accumulates several, and everything in it is tested against all of them. The
//! polygon is a screen-space outline in **pixels, y down**, with the first vertex repeated at the
//! end (`points[count] = points[0]`) — the repeat is not counted in the vertex count and is not kept
//! here either, because [`poly_clip_finish`] walks the edges cyclically.
//!
//! The client also stores, per kept vertex, a world-space plane through the eye
//! (`N = cross(dir[i+1], dir[i])`, `d = -(N · viewpoint)`) which the view-cone test applies to object
//! spheres against — see [`EyeTransform`], [`eye_planes`] and [`ViewPoly::planes`]. Without
//! them there is no per-object frustum test at all.
//!
//! # The eye planes
//!
//! The view-copy tail does three things after the polygon is
//! simplified and stored:
//!
//! 1. turns each **kept pixel** back into a world-space direction from the eye. There are two
//!    branches, on a mode word; the shipped client initialises that word to **1** and **nothing
//!    in the client writes it** (it is only ever read), so the
//!    live branch is always the first one
//!    and the `Xaxis`/`Yaxis`/`Zaxis` arithmetic in the `else` is **dead code**. Only the live one
//!    is transcribed here; see [`EyeTransform::direction`].
//! 2. repeats direction 0 after the last, so vertex `n-1`'s plane wraps.
//! 3. per kept vertex `i`, `N = cross(dir[i+1], dir[i])`, normalised **only when some component's
//!    magnitude reaches `0.0002`**, then `d = -(N · viewpoint)` — every plane passes through the
//!    eye, so the set is a cone and not a frustum. [`eye_planes`].
//!
//! The plane is stored *inside* the vertex: a 24-byte view vertex holding the point, then the
//! plane normal, then `d`. The clipper reads that layout back independently, one vertex at a
//! time — a writer and a reader that could have disagreed about the layout and do not.

use dereth_primitives::Vec3;

use crate::Plane;

/// One vertex of a polygon on its way through the clipper.
///
/// The client's is `{ xw, yw, zw, w }`: the position **already multiplied by the viewport
/// transform**, so `xw / w` is a pixel coordinate and not an NDC
/// one. `zw / w` is the device depth. [`ScreenPoint::from_clip`] does that conversion, because what
/// a D3D12 caller has is `view_proj * v`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScreenPoint {
    pub xw: f32,
    pub yw: f32,
    pub zw: f32,
    pub w: f32,
}

impl ScreenPoint {
    /// Build the perspective-transform output from a clip-space position and viewport extent.
    ///
    /// D3D clip space has `x, y ∈ [-w, w]` and `z ∈ [0, w]`; the viewport maps that to
    /// `x_px = (x/w · ½ + ½)·W` and `y_px = (½ − y/w · ½)·H`, y down. Multiplying back through by
    /// `w` keeps the point homogeneous, which is what lets the clipper interpolate without a divide.
    #[must_use]
    pub fn from_clip(clip: [f32; 4], width: f32, height: f32) -> Self {
        let w = clip[3];
        Self {
            xw: (clip[0] + w) * 0.5 * width,
            yw: (w - clip[1]) * 0.5 * height,
            zw: clip[2],
            w,
        }
    }

    /// The perspective divide. The view copy does this in place before it simplifies.
    #[must_use]
    pub fn screen(self) -> (f32, f32) {
        if self.w == 1.0 {
            (self.xw, self.yw)
        } else {
            (self.xw / self.w, self.yw / self.w)
        }
    }

    /// The point as the clip-space quad [`crate::cells::clip`] hands back to a caller that wants to
    /// draw it — `(x·w, y·w, z·w, w)` in NDC, the inverse of [`Self::from_clip`].
    #[must_use]
    pub fn to_clip(self, width: f32, height: f32) -> [f32; 4] {
        [
            self.xw * 2.0 / width - self.w,
            self.w - self.yw * 2.0 / height,
            self.zw,
            self.w,
        ]
    }
}

/// The clipper's near-plane threshold, `0.0002` -- the same `Render` epsilon the sidedness
/// test uses, here as a minimum homogeneous `w`.
pub const CLIP_MIN_W: f32 = 2.0e-4;

/// The `31`-vertex cap the view copy applies after the simplification.
pub const MAX_VIEW_VERTICES: usize = 31;

/// The view copy's normalisation threshold, the float — `0.0002`, kept separate from
/// [`CLIP_MIN_W`]'s constant, because they are two separate constants in the original
/// that happen to be equal.
pub const EYE_PLANE_EPSILON: f32 = 2.0e-4;

/// Inputs to the plane-building tail: the world-space viewer position and the projection and view
/// matrices from the global render state used for portal clipping.
///
/// It is a parameter here and a set of globals there. The viewer's world space is written by the
/// viewpoint update; the view-to-clip and world-to-view matrices
/// of the global render state are the projection and view matrices of the frame being drawn, and
/// they **must** be the frame's own, or the cone is a cone of somewhere the player is not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EyeTransform {
    /// Camera origin in the same space as the object
    /// centres [`crate::cells::cull::viewcone_check`] tests are expressed in.
    pub viewpoint: Vec3,
    /// `D3DXMatrixInverse` of the world-to-view matrix, in **D3D row-major order**: `_rc` is
    /// `inv_view[4 * (r - 1) + (c - 1)]`. Only the top-left 3×3 is read, because a direction has no
    /// translation — which is the screen-to-view transform's own omission of the `_41.._43` row.
    pub inv_view: [f32; 16],
    /// The view-to-clip matrix's `_11`.
    pub proj_11: f32,
    /// The view-to-clip matrix's `_22`.
    pub proj_22: f32,
    /// Current render-device viewport width.
    pub width: f32,
    /// Current render-device viewport height.
    pub height: f32,
}

impl EyeTransform {
    /// One pixel back to a
    /// world-space direction from the eye.
    ///
    /// ```text
    /// a = ((2x)/W - 1) / P._11
    /// b = (-1 / P._22) * ((2y)/H - 1)
    /// out.x = V'._11*a + V'._21*b + V'._31
    /// out.z = V'._12*a + V'._22*b + V'._32          // note: z from the D3D y row
    /// out.y = V'._13*a + V'._23*b + V'._33          // and y from the D3D z row
    /// ```
    ///
    /// with `V'` the inverse of the world-to-view matrix. The last two assignments are the Z-up ↔
    /// Y-up swap, the same one `dereth_render::camera::swap_forward_and_up` applies on the way in —
    /// written out here in the client's own order so the transposition is visible rather than
    /// delegated.
    ///
    /// `b`'s leading minus is what makes the result y-**down** in pixels and y-up in NDC; without
    /// it the top and bottom eye planes swap and the cone is inside out.
    #[must_use]
    pub fn direction(&self, px: f32, py: f32) -> Vec3 {
        let v = &self.inv_view;
        let a = ((px + px) / self.width - 1.0) / self.proj_11;
        let b = (-1.0 / self.proj_22) * ((py + py) / self.height - 1.0);
        Vec3::new(
            v[0] * a + v[4] * b + v[8],
            v[2] * a + v[6] * b + v[10],
            v[1] * a + v[5] * b + v[9],
        )
    }
}

/// Build one plane through the eye per kept vertex.
///
/// `points` are the polygon's kept pixels, **without** the repeat of the first that the client
/// stores after them; the wrap is done here by indexing `(i + 1) % n`, which is that repeat.
///
/// Per vertex `i`:
///
/// ```text
/// N = cross(dir[i+1], dir[i])
/// if |N.x| >= 0.0002 or |N.y| >= 0.0002 or |N.z| >= 0.0002:  N *= 1 / |N|
/// d = -(N · viewpoint)
/// ```
///
/// The normalisation is **conditional**, and a degenerate edge (two vertices projecting to the same
/// ray) therefore keeps its near-zero normal rather than being divided by zero. That is a real
/// state: `viewcone_check`'s `d < -r` can never fire on such a plane, so a degenerate edge admits
/// everything, which is the permissive direction and is the client's.
#[must_use]
pub fn eye_planes(points: &[(f32, f32)], eye: &EyeTransform) -> Vec<Plane> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let dirs: Vec<Vec3> = points.iter().map(|&(x, y)| eye.direction(x, y)).collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = dirs[i];
        let b = dirs[(i + 1) % n];
        // `cross(dir[i+1], dir[i])`, component by component in the client's own operand order.
        let mut nx = a.z * b.y - a.y * b.z;
        let mut ny = b.z * a.x - a.z * b.x;
        let mut nz = a.y * b.x - b.y * a.x;
        if nx.abs() >= EYE_PLANE_EPSILON
            || ny.abs() >= EYE_PLANE_EPSILON
            || nz.abs() >= EYE_PLANE_EPSILON
        {
            let inv = 1.0 / (nx * nx + ny * ny + nz * nz).sqrt();
            nx *= inv;
            ny *= inv;
            nz *= inv;
        }
        let normal = Vec3::new(nx, ny, nz);
        out.push(Plane {
            normal,
            d: -normal.dot(eye.viewpoint),
        });
    }
    out
}

/// One view polygon: the screen outline a cell is visible through, in pixels, y down.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewPoly {
    /// The kept vertices, **without** the repeat of the first that the client stores after them.
    pub points: Vec<(f32, f32)>,
    pub xmin: f32,
    pub xmax: f32,
    pub ymin: f32,
    pub ymax: f32,
    /// One world-space plane through the eye per entry of [`Self::points`], in the same order —
    /// the client stores them *inside* the vertices. These are the per-vertex planes the view-cone
    /// check ([`crate::cells::cull::viewcone_check`]) reads, and they are what makes a per-object cull
    /// possible at all; see [`eye_planes`].
    pub planes: Vec<Plane>,
}

impl ViewPoly {
    /// The full-screen quad
    /// `(0, H) (W, H) (W, 0) (0, 0)`, which the default view installs and which the
    /// **outdoor** pass therefore clips a building's openings against.
    ///
    /// The winding is the client's and is load-bearing: [`poly_clip_finish`]'s "inside" test is a
    /// signed cross product, and this order is what makes it mean "within the viewport".
    ///
    /// The no-screen-points arm rejoins the common tail, so this quad gets its
    /// four eye planes exactly like a clipped opening does — and with `W`/`H` the whole viewport
    /// they are the view frustum's four side planes. That is not an approximation of the frustum:
    /// it is the client's own polygon put through the client's own plane builder, and the frustum
    /// is what falls out.
    #[must_use]
    pub fn full_screen(width: f32, height: f32, eye: &EyeTransform) -> Self {
        let points = vec![(0.0, height), (width, height), (width, 0.0), (0.0, 0.0)];
        let planes = eye_planes(&points, eye);
        Self {
            points,
            xmin: 0.0,
            xmax: width,
            ymin: 0.0,
            ymax: height,
            planes,
        }
    }

    /// Whether the polygon is a usable view at all: the view copy fails below three vertices.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.len() < 3
    }
}

/// The software clipper.
///
/// Sutherland–Hodgman in homogeneous screen space, in two stages:
///
/// 1. **the near plane**, `w >= CLIP_MIN_W`. Skipped entirely when every input vertex already satisfies
///    it, which is the client's leading `while` loop; when it runs and leaves fewer than three
///    vertices the polygon is gone.
/// 2. **one pass per edge of the current view polygon**, in the order
///    `(v0 → v[n-1]), (v[n-1] → v[n-2]), …, (v1 → v0)` — the outline walked backwards. The
///    half-space test for a point `P` and an edge from `a` with direction `e` is
///
///    ```text
///    f = (P.xw − a.x·P.w)·e.y − (P.yw − a.y·P.w)·e.x       // = w · ((P/w − a) × e)
///    inside ⟺ f <= 0
///    ```
///
///    which is true for a negative *and* for a zero, false only for a positive.
///
/// The client ping-pongs between two buffers, and each pass writes its output reversed; the parity
/// works out even for every input, so the winding of the
/// result is the winding of the input and this implementation does not reverse at all.
///
/// A `view` with fewer than three vertices clips everything away, which is what an exhausted view
/// polygon means.
#[must_use]
pub fn poly_clip_finish(subject: &[ScreenPoint], view: &ViewPoly) -> Vec<ScreenPoint> {
    if subject.len() < 3 || view.points.len() < 3 {
        return Vec::new();
    }

    // 1. The near plane. The scan stops at the first vertex with `w < CLIP_MIN_W` — it runs from
    //    the end and only a vertex that fails it makes the clip run at all.
    let mut poly: Vec<ScreenPoint> = if subject.iter().any(|p| p.w < CLIP_MIN_W) {
        let out = clip_half_space(subject, |p| p.w - CLIP_MIN_W);
        if out.len() < 3 {
            return Vec::new();
        }
        out
    } else {
        subject.to_vec()
    };

    // 2. The view's own edges, `(v0 → v[n-1])` then `(v[k] → v[k-1])` down to `(v1 → v0)`.
    let n = view.points.len();
    for k in 0..n {
        let (from, to) = if k == 0 {
            (0, n - 1)
        } else {
            (n - k, n - k - 1)
        };
        let (ax, ay) = view.points[from];
        let (ex, ey) = (view.points[to].0 - ax, view.points[to].1 - ay);
        // The sign convention is "inside is f <= 0", so the distance function handed to the shared
        // clipper is negated: it keeps `d >= 0`.
        poly = clip_half_space(&poly, |p| {
            (p.yw - ay * p.w).mul_add(ex, -((p.xw - ax * p.w) * ey))
        });
        if poly.len() < 3 {
            return Vec::new();
        }
    }
    poly
}

/// One Sutherland–Hodgman pass: keep the vertices with `d(p) >= 0` and split every crossing edge at
/// `t = d0 / (d0 − d1)`, interpolating all four homogeneous components the way the client does,
/// `(b − a) · t + a` per component.
fn clip_half_space(poly: &[ScreenPoint], d: impl Fn(&ScreenPoint) -> f32) -> Vec<ScreenPoint> {
    let mut out: Vec<ScreenPoint> = Vec::with_capacity(poly.len() + 4);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let (da, db) = (d(&a), d(&b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out.push(ScreenPoint {
                xw: (b.xw - a.xw).mul_add(t, a.xw),
                yw: (b.yw - a.yw).mul_add(t, a.yw),
                zw: (b.zw - a.zw).mul_add(t, a.zw),
                w: (b.w - a.w).mul_add(t, a.w),
            });
        }
    }
    out
}

/// The screen-space clip of one portal opening against one side.
///
/// Transforms the portal polygon's vertices (the caller's job here — `screen` is already
/// the vertex transform's output), **reverses their order when the requested side is not `POSITIVE`**, and
/// with `clip != 0` runs [`poly_clip_finish`] against the current view.
///
/// The reversal is not cosmetic: [`poly_clip_finish`]'s half-space test is signed, so a polygon seen
/// from its negative side arrives wound the wrong way and would clip to nothing.
#[must_use]
pub fn get_clip(screen: &[ScreenPoint], negative_side: bool, view: &ViewPoly) -> Vec<ScreenPoint> {
    let mut pts: Vec<ScreenPoint> = screen.to_vec();
    if negative_side {
        pts.reverse();
    }
    poly_clip_finish(&pts, view)
}

/// Append one polygon to a view, or refuse.
///
/// The input is perspective-divided and then **simplified**, and the simplification is the whole
/// reason this function is not just a copy:
///
/// * a vertex within **1.0 pixel** of the previously kept one, in both x and y, is dropped;
/// * of a run of three, the middle is dropped when the triangle they form has `|cross|` smaller than
///   the larger of the two axis distances from the last accepted vertex — the near-collinear test;
/// * fewer than **3** survivors and the portal is invisible: the view copy fails, which is
///   the portal clipper's "clipped away entirely";
/// * at most **31** are kept.
///
/// The wrap-around gets its own two tests after the loop — the last vertex against the first, and
/// then the first itself against the first accepted one — which is why this is transcribed in the
/// client's own order rather than written as a tidy filter. The view copy also stores `|x|` and `|y|`
/// rather than `x` and `y`; that is reproduced, and it is a no-op for any polygon that came out of a
/// clip against a view inside the viewport.
#[must_use]
pub fn copy_view(screen: &[ScreenPoint], eye: &EyeTransform) -> Option<ViewPoly> {
    let n = screen.len();
    if n == 0 {
        return None;
    }
    let p: Vec<(f32, f32)> = screen.iter().map(|s| s.screen()).collect();

    let mut keep = vec![false; n];
    keep[0] = true;
    let mut count: i32 = 1;
    // The last vertex that cleared the one-pixel test.
    let mut prev = 0usize;
    // The last vertex the collinearity test accepted.
    let mut last_accepted = 0usize;
    // The first vertex accepted after vertex 0.
    let mut first_run = 0usize;

    for i in 1..n {
        let big = (p[i].0 - p[prev].0).abs() > 1.0 || (p[i].1 - p[prev].1).abs() > 1.0;
        keep[i] = big;
        if !big {
            continue;
        }
        if prev < 1 {
            count += 1;
            first_run = i;
        } else {
            let a = p[last_accepted];
            let d = (a.0 - p[i].0).abs().max((a.1 - p[i].1).abs());
            let cross = ((a.0 - p[prev].0) * (p[prev].1 - p[i].1)
                - (a.1 - p[prev].1) * (p[prev].0 - p[i].0))
                .abs();
            if d <= cross {
                count += 1;
                last_accepted = prev;
            } else {
                keep[prev] = false;
                if first_run == prev {
                    first_run = i;
                }
            }
        }
        prev = i;
    }

    // The wrap: the last surviving vertex against the first.
    let last = prev;
    let big = (p[0].0 - p[last].0).abs() > 1.0 || (p[0].1 - p[last].1).abs() > 1.0;
    keep[last] = big;
    let mut dropped_last = !big;
    if big {
        let a = p[last_accepted];
        let d = (a.0 - p[0].0).abs().max((a.1 - p[0].1).abs());
        let cross = ((a.0 - p[last].0) * (p[last].1 - p[0].1)
            - (p[last].0 - p[0].0) * (a.1 - p[last].1))
            .abs();
        if cross < d {
            keep[last] = false;
            dropped_last = true;
        }
    }
    if dropped_last {
        count -= 1;
    } else {
        last_accepted = last;
    }

    // And vertex 0 itself, against the first vertex accepted after it.
    if first_run > 0 {
        let a = p[last_accepted];
        let f1 = p[first_run];
        let d = (a.0 - f1.0).abs().max((a.1 - f1.1).abs());
        let cross = ((p[0].1 - f1.1) * (a.0 - p[0].0) - (p[0].0 - f1.0) * (a.1 - p[0].1)).abs();
        if cross < d {
            count -= 1;
            keep[0] = false;
        }
    }

    if count < 3 {
        return None;
    }
    let cap = (count as usize).min(MAX_VIEW_VERTICES);

    let mut points: Vec<(f32, f32)> = Vec::with_capacity(cap);
    for i in 0..n {
        if keep[i] && points.len() < cap {
            points.push((p[i].0.abs(), p[i].1.abs()));
        }
    }
    if points.len() < 3 {
        return None;
    }
    let (mut xmin, mut xmax) = (f32::MAX, f32::MIN);
    let (mut ymin, mut ymax) = (f32::MAX, f32::MIN);
    for &(x, y) in &points {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    // The common tail, which the no-screen-points arm also falls into: the bounding box,
    // then one eye plane per kept vertex. The `NULL` arm is [`ViewPoly::full_screen`].
    let planes = eye_planes(&points, eye);
    Some(ViewPoly {
        points,
        xmin,
        xmax,
        ymin,
        ymax,
        planes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> ScreenPoint {
        ScreenPoint {
            xw: x,
            yw: y,
            zw: 0.5,
            w: 1.0,
        }
    }

    fn quad(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<ScreenPoint> {
        // The same winding as `ViewPoly::full_screen`: bottom-left, bottom-right, top-right,
        // top-left in a y-down screen.
        vec![pt(x0, y1), pt(x1, y1), pt(x1, y0), pt(x0, y0)]
    }

    /// A camera at the origin looking down **+y** with **+z** up, `P._11 = P._22 = 1` (a square
    /// 90° frustum) and the identity view matrix.
    ///
    /// With this eye [`EyeTransform::direction`] reduces to `(ndc_x, 1, ndc_y)`, so the four
    /// corners of the screen are `(∓1, 1, ∓1)` and the full-screen view's planes are the four
    /// diagonal walls of a square pyramid — which is checkable by hand and is what the tests below
    /// do rather than round-tripping through a matrix library.
    fn eye(width: f32, height: f32) -> EyeTransform {
        EyeTransform {
            viewpoint: Vec3::ZERO,
            #[rustfmt::skip]
            inv_view: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
            proj_11: 1.0,
            proj_22: 1.0,
            width,
            height,
        }
    }

    /// Oracle: the `screenPts == NULL` arm -- "builds the full-screen
    /// quad `(0, H) (W, H) (W, 0) (0, 0)`", which the default view installs.
    ///
    /// The winding is asserted through the clipper rather than by inspection, because the winding is
    /// only meaningful as the sign convention of the half-space test: a polygon wholly inside the
    /// viewport must survive the full-screen view unchanged, and one wholly outside must vanish.
    #[test]
    fn the_full_screen_view_keeps_what_is_on_the_screen_and_drops_what_is_not() {
        let view = ViewPoly::full_screen(640.0, 480.0, &eye(640.0, 480.0));
        let inside = quad(100.0, 100.0, 200.0, 200.0);
        let clipped = poly_clip_finish(&inside, &view);
        assert_eq!(
            clipped.len(),
            4,
            "a quad inside the viewport survives whole"
        );
        for (a, b) in clipped.iter().zip(inside.iter()) {
            assert!((a.xw - b.xw).abs() < 1.0e-3 && (a.yw - b.yw).abs() < 1.0e-3);
        }
        // Entirely off the left edge.
        assert!(poly_clip_finish(&quad(-300.0, 100.0, -100.0, 200.0), &view).is_empty());
        // Entirely below the bottom.
        assert!(poly_clip_finish(&quad(100.0, 600.0, 200.0, 700.0), &view).is_empty());
    }

    /// Oracle: the same clipper, straddling an edge. Sutherland–Hodgman splits the crossing edges at
    /// `t = d0 / (d0 - d1)`, so a quad half off the left of the screen comes back clipped to
    /// `x = 0` — not dropped, and not passed through whole.
    #[test]
    fn a_polygon_across_an_edge_is_cut_at_the_edge() {
        let view = ViewPoly::full_screen(640.0, 480.0, &eye(640.0, 480.0));
        let clipped = poly_clip_finish(&quad(-100.0, 100.0, 200.0, 200.0), &view);
        assert!(clipped.len() >= 3, "the part on the screen survives");
        for p in &clipped {
            let (x, _) = p.screen();
            assert!(
                x >= -1.0e-3,
                "a vertex at x = {x} is off the left of the viewport"
            );
        }
        let max_x = clipped
            .iter()
            .map(|p| p.screen().0)
            .fold(f32::MIN, f32::max);
        assert!(
            (max_x - 200.0).abs() < 1.0e-3,
            "the right edge is untouched, got {max_x}"
        );
    }

    /// Oracle: the clipper -- "if n == 0: skip // clipped away entirely", reached by
    /// clipping one opening's outline against another's. This is the mechanism the whole unit is
    /// about: two openings that do not overlap on the screen let nothing through.
    #[test]
    fn two_openings_that_do_not_overlap_close_the_chain() {
        let first =
            copy_view(&quad(100.0, 100.0, 200.0, 200.0), &eye(640.0, 480.0)).expect("a view");
        // Overlapping: the intersection survives and is the overlap, not either whole.
        let through = poly_clip_finish(&quad(150.0, 150.0, 300.0, 300.0), &first);
        assert!(through.len() >= 3);
        let xs: Vec<f32> = through.iter().map(|p| p.screen().0).collect();
        assert!(xs.iter().copied().fold(f32::MIN, f32::max) <= 200.0 + 1.0e-3);
        assert!(xs.iter().copied().fold(f32::MAX, f32::min) >= 150.0 - 1.0e-3);
        // Disjoint: nothing at all.
        assert!(poly_clip_finish(&quad(300.0, 300.0, 400.0, 400.0), &first).is_empty());
    }

    /// Oracle: the polygon clipper's leading loop, which stops at the first vertex whose `w` is
    /// below the clip threshold `0.0002`. A vertex behind the eye has to be cut on the near plane
    /// before the perspective divide, or it is projected through the singularity and lands on the
    /// wrong side of the screen.
    #[test]
    fn a_vertex_behind_the_eye_is_cut_on_the_near_plane() {
        let view = ViewPoly::full_screen(640.0, 480.0, &eye(640.0, 480.0));
        let mut poly = quad(100.0, 100.0, 200.0, 200.0);
        // Push one vertex behind the eye, keeping its homogeneous coordinates consistent.
        poly[0] = ScreenPoint {
            xw: -100.0,
            yw: -200.0,
            zw: -0.5,
            w: -1.0,
        };
        let clipped = poly_clip_finish(&poly, &view);
        assert!(clipped.len() >= 3, "the part in front of the eye survives");
        for p in &clipped {
            assert!(
                p.w >= CLIP_MIN_W - 1.0e-9,
                "a vertex with w = {} survived",
                p.w
            );
        }
        // Every vertex behind the eye: nothing survives.
        let behind: Vec<ScreenPoint> = poly
            .iter()
            .map(|p| ScreenPoint {
                xw: -p.xw,
                yw: -p.yw,
                zw: -p.zw,
                w: -1.0,
            })
            .collect();
        assert!(poly_clip_finish(&behind, &view).is_empty());
    }

    /// Oracle: view copying drops a vertex if it is within **1.0 pixel** of the
    /// previous kept vertex in both x and y, and fewer than **3** surviving vertices make
    /// `copy_view` return 0 because the portal is invisible.
    ///
    /// A portal that has shrunk to less than a pixel is exactly the case the 3-vertex minimum
    /// exists for: without it a distant window keeps opening its interior for ever.
    #[test]
    fn a_sub_pixel_polygon_simplifies_below_three_vertices_and_closes() {
        // A quad 0.5 px on a side: every vertex is within one pixel of the last.
        assert_eq!(
            copy_view(&quad(100.0, 100.0, 100.5, 100.5), &eye(640.0, 480.0)),
            None
        );
        // Grown past a pixel it survives.
        let v = copy_view(&quad(100.0, 100.0, 104.0, 104.0), &eye(640.0, 480.0))
            .expect("a four-pixel quad is visible");
        assert_eq!(v.points.len(), 4);
        assert_eq!(
            (v.xmin, v.xmax, v.ymin, v.ymax),
            (100.0, 104.0, 100.0, 104.0)
        );
    }

    /// Oracle: the view-copy near-collinear rule drops a vertex if the triangle it
    /// forms with its neighbours has |cross product| smaller than the larger of the two axis
    /// distances to the previous kept vertex.
    ///
    /// Four collinear points are a line, not a polygon: the simplification takes the interior ones
    /// out and what is left cannot be a view.
    #[test]
    fn collinear_vertices_are_dropped_and_a_line_is_not_a_view() {
        let line = vec![pt(0.0, 0.0), pt(50.0, 0.0), pt(100.0, 0.0), pt(150.0, 0.0)];
        assert_eq!(
            copy_view(&line, &eye(640.0, 480.0)),
            None,
            "a straight line has no interior"
        );
        // The same four points with the third pushed well off the line are a real quadrilateral.
        let bent = vec![pt(0.0, 0.0), pt(50.0, 0.0), pt(100.0, 60.0), pt(150.0, 0.0)];
        let v = copy_view(&bent, &eye(640.0, 480.0)).expect("a bent outline is a polygon");
        assert!(v.points.len() >= 3);
    }

    /// Oracle: the clip -- "reverses the order when the requested side is
    /// `NEGATIVE`".
    ///
    /// The reversal does **not** decide whether the polygon survives *this* clip: Sutherland–Hodgman
    /// tests each vertex of the subject against the window, and that is blind to the subject's
    /// winding. What it decides is whether the result is usable as the **next** view polygon, since
    /// the half-space test's sign convention is the window's winding. A portal seen from its
    /// negative side projects wound the other way round, and the portal clipper hands its output
    /// straight to `copy_view` for the neighbour cell — so without the reversal the neighbour gets
    /// a view that clips everything away, and the chain dies one cell early.
    #[test]
    fn the_negative_side_is_reversed_so_the_neighbours_view_is_wound_the_right_way() {
        let view = ViewPoly::full_screen(640.0, 480.0, &eye(640.0, 480.0));
        // A polygon wound the *other* way round than `full_screen`, i.e. as a portal on its
        // negative side projects.
        let backwards: Vec<ScreenPoint> =
            quad(100.0, 100.0, 200.0, 200.0).into_iter().rev().collect();
        let inner = quad(120.0, 120.0, 180.0, 180.0);

        let as_positive =
            copy_view(&get_clip(&backwards, false, &view), &eye(640.0, 480.0)).expect("a polygon");
        assert!(
            poly_clip_finish(&inner, &as_positive).is_empty(),
            "taken as POSITIVE the outline is inside out and admits nothing"
        );
        let as_negative =
            copy_view(&get_clip(&backwards, true, &view), &eye(640.0, 480.0)).expect("a polygon");
        assert_eq!(
            poly_clip_finish(&inner, &as_negative).len(),
            4,
            "reversed, it is a window again and what is inside it passes"
        );
    }

    /// A pixel becomes a world ray with the screen y flip.
    #[test]
    fn a_pixel_becomes_a_world_ray_with_the_screen_y_flip() {
        let e = eye(640.0, 480.0);
        // The centre of the screen is straight ahead.
        let c = e.direction(320.0, 240.0);
        assert!(c.x.abs() < 1.0e-6 && c.z.abs() < 1.0e-6, "{c:?}");
        assert!((c.y - 1.0).abs() < 1.0e-6, "{c:?}");
        // The top of the screen is **up**, not down.
        let top = e.direction(320.0, 0.0);
        assert!(
            (top.z - 1.0).abs() < 1.0e-6,
            "screen y = 0 must map to +z: {top:?}"
        );
        let bottom = e.direction(320.0, 480.0);
        assert!((bottom.z + 1.0).abs() < 1.0e-6, "{bottom:?}");
        // And the right of the screen is +x.
        let right = e.direction(640.0, 240.0);
        assert!((right.x - 1.0).abs() < 1.0e-6, "{right:?}");
        assert!((e.direction(0.0, 240.0).x + 1.0).abs() < 1.0e-6);
        // The projection scales divide, so a narrower field of view gives a shallower ray.
        let narrow = EyeTransform {
            proj_11: 2.0,
            proj_22: 2.0,
            ..e
        };
        assert!((narrow.direction(640.0, 240.0).x - 0.5).abs() < 1.0e-6);
    }

    /// The full screen planes are a cone through the eye with inside positive.
    #[test]
    fn the_full_screen_planes_are_a_cone_through_the_eye_with_inside_positive() {
        let e = eye(640.0, 480.0);
        let v = ViewPoly::full_screen(640.0, 480.0, &e);
        assert_eq!(v.planes.len(), 4, "one plane per kept vertex");

        // Every plane passes through the eye. Taken with a viewpoint that is **not** the origin,
        // because at the origin `d` is 0 whichever sign the transcription used.
        let moved = EyeTransform {
            viewpoint: Vec3::new(7.0, -3.0, 11.0),
            ..e
        };
        let mv = ViewPoly::full_screen(640.0, 480.0, &moved);
        for (i, p) in mv.planes.iter().enumerate() {
            assert!(
                p.dot_point(moved.viewpoint).abs() < 1.0e-4,
                "plane {i}: {p:?}"
            );
        }

        // Inside is positive: a point 100 m straight ahead clears all four.
        let ahead = Vec3::new(0.0, 100.0, 0.0);
        for (i, p) in v.planes.iter().enumerate() {
            assert!(
                p.dot_point(ahead) > 0.0,
                "plane {i} rejects the middle of the screen: {p:?}"
            );
        }
        // And each plane rejects the half-space beyond its own screen edge. At 100 m ahead the
        // pyramid is 100 m half-width, so 200 m off-axis is outside exactly one plane.
        for (off, name) in [
            (Vec3::new(-200.0, 100.0, 0.0), "left"),
            (Vec3::new(200.0, 100.0, 0.0), "right"),
            (Vec3::new(0.0, 100.0, 200.0), "above"),
            (Vec3::new(0.0, 100.0, -200.0), "below"),
        ] {
            let out = v.planes.iter().filter(|p| p.dot_point(off) < 0.0).count();
            assert_eq!(out, 1, "{name} of the screen is outside exactly one plane");
        }
        // What the near plane is *for*, stated as a measurement rather than as a guess. For this
        // symmetric eye the four side planes are `y > |x|` and `y > |z|`, so they already imply
        // `y > 0` and the back half is rejected by them alone — the near plane's whole
        // contribution is the `znear` offset. The eye's own position is the discriminating case:
        // it lies **on** all four side planes (they pass through it, `d = -(N · viewpoint)`), so
        // they admit it, and `viewer_world_space.CY` is the only thing that does not.
        for (i, p) in v.planes.iter().enumerate() {
            assert!(
                p.dot_point(e.viewpoint).abs() < 1.0e-5,
                "plane {i} does not contain the eye: {p:?}"
            );
        }
        let near =
            crate::cells::cull::viewer_near_plane(e.viewpoint, Vec3::new(0.0, 1.0, 0.0), 0.1);
        assert!(
            near.dot_point(e.viewpoint) < 0.0,
            "only CY puts the eye outside the cone"
        );
        assert_eq!(
            v.planes
                .iter()
                .filter(|p| p.dot_point(Vec3::new(0.0, -100.0, 0.0)) < 0.0)
                .count(),
            4
        );
    }

    /// A degenerate edge keeps its unnormalised normal and rejects nothing.
    #[test]
    fn a_degenerate_edge_keeps_its_unnormalised_normal_and_rejects_nothing() {
        let e = eye(640.0, 480.0);
        // Three points, two of which are the same pixel: the shared edge's cross product is zero.
        let planes = eye_planes(&[(100.0, 100.0), (100.0, 100.0), (300.0, 300.0)], &e);
        assert_eq!(planes.len(), 3);
        let degenerate = &planes[0];
        assert!(
            degenerate.normal.magnitude() < EYE_PLANE_EPSILON,
            "{degenerate:?}"
        );
        assert!(degenerate.normal.x.is_finite() && degenerate.normal.y.is_finite());
        for p in [
            Vec3::new(0.0, 100.0, 0.0),
            Vec3::new(0.0, -100.0, 0.0),
            Vec3::new(500.0, 1.0, 0.0),
        ] {
            assert!(
                degenerate.dot_point(p) >= 0.0,
                "a zero plane must admit {p:?}"
            );
        }
        // A real edge in the same polygon is normalised.
        let live = planes
            .iter()
            .find(|p| p.normal.magnitude() > 0.5)
            .expect("one real edge");
        assert!((live.normal.magnitude() - 1.0).abs() < 1.0e-4, "{live:?}");
    }

    /// Copy view gives one plane per kept vertex.
    #[test]
    fn copy_view_gives_one_plane_per_kept_vertex() {
        let e = eye(640.0, 480.0);
        // Five input vertices, one of which is within a pixel of its predecessor. The **winding is
        // the client's** — bottom-left, bottom-right, top-right, top-left in a y-down screen, the
        // same order `ViewPoly::full_screen` uses — and it is load-bearing twice over: for
        // `poly_clip_finish`'s signed half-space test *and* for these planes, whose normals point
        // inward only for this order. Reversing the negative side is what
        // keeps a portal seen from behind from producing an inside-out cone.
        let poly = vec![
            pt(100.0, 300.0),
            pt(100.3, 300.3),
            pt(300.0, 300.0),
            pt(300.0, 100.0),
            pt(100.0, 100.0),
        ];
        let v = copy_view(&poly, &e).expect("a quadrilateral");
        assert_eq!(v.points.len(), 4, "the sub-pixel vertex is dropped");
        assert_eq!(v.planes.len(), v.points.len());
        // And the planes are the polygon's, not the screen's: a point outside the opening but on
        // the screen is rejected. (100..300, 100..300) of a 640x480 screen is up and left of
        // centre, so a ray through the screen centre misses it.
        let c = e.direction(320.0, 240.0);
        let centre = Vec3::new(c.x * 50.0, c.y * 50.0, c.z * 50.0);
        assert!(
            v.planes.iter().any(|p| p.dot_point(centre) < 0.0),
            "the middle of the screen is outside this opening"
        );
        let t = e.direction(200.0, 200.0);
        let through = Vec3::new(t.x * 50.0, t.y * 50.0, t.z * 50.0);
        for (i, p) in v.planes.iter().enumerate() {
            assert!(
                p.dot_point(through) > 0.0,
                "plane {i} rejects a ray through the opening"
            );
        }
    }

    /// Oracle: `ScreenPoint::from_clip` against the transform-start contract — clip space
    /// `x, y ∈ [-w, w]`, `z ∈ [0, w]` maps to pixels with y **down**. The centre of clip space is
    /// the centre of the screen and `y = +w` is the top row.
    #[test]
    fn clip_space_maps_to_pixels_with_y_down() {
        let p = ScreenPoint::from_clip([0.0, 0.0, 0.5, 1.0], 640.0, 480.0);
        assert_eq!(p.screen(), (320.0, 240.0));
        let top = ScreenPoint::from_clip([0.0, 1.0, 0.5, 1.0], 640.0, 480.0);
        assert_eq!(
            top.screen(),
            (320.0, 0.0),
            "clip +y is the top of the screen"
        );
        // And it round-trips, which is what lets the depth stamp be handed back to the device.
        let q = ScreenPoint::from_clip([0.3, -0.4, 0.25, 2.0], 640.0, 480.0);
        let back = q.to_clip(640.0, 480.0);
        for (a, b) in back.iter().zip([0.3f32, -0.4, 0.25, 2.0].iter()) {
            assert!((a - b).abs() < 1.0e-4, "{back:?}");
        }
    }
}
