//! An outline: a simple polygon as the client builds the pieces of its path maps from: corners in order, each
//! edge running from the corner before to the corner itself (the first edge from the last corner),
//! with its area, its plane, the upright plane of each edge, and the clean-ups that drop short
//! edges and corners that make it concave or add nothing.
//!
//! Each sum is worked in double precision and rounded where the client stores a float.

use dereth_primitives::Vec3;

use super::{BBox, BoxOps, Plane, PlaneOps, Ray, EPS};

fn w(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn n(x: f64) -> f32 {
    x as f32
}

/// A polygon's corners in order. A new one has none; a copy has the same corners.
// D2-03802, D2-03811
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub vertices: Vec<Vec3>,
}

impl Outline {
    /// Edge `i`: the ray from the corner before `i` (the last, for the first) to corner `i`.
    // D2-13395
    #[must_use]
    pub fn edge(&self, i: usize) -> Option<Ray> {
        let v = &self.vertices;
        if i >= v.len() {
            return None;
        }
        let prev = v[if i == 0 { v.len() } else { i } - 1];
        let off = Vec3::new(v[i].x - prev.x, v[i].y - prev.y, v[i].z - prev.z);
        Some(Ray::from_offset(prev, off))
    }

    /// Replaces edge `i` by its middle: the corner before it moves there and corner `i` goes
    /// (the first edge's middle replaces the first corner, and the last corner goes).
    // D2-13396
    pub fn replace_edge_with_midpoint(&mut self, i: usize) -> bool {
        if let Some(e) = self.edge(i) {
            let c = e.center_point();
            if i == 0 {
                self.vertices[0] = c;
                self.vertices.pop();
            } else {
                self.vertices[i - 1] = c;
                self.vertices.remove(i);
            }
        }
        true
    }

    /// The upright plane through edge `i`; none for an edge 0.0002 long or shorter.
    // D2-13397
    #[must_use]
    pub fn edge_plane(&self, i: usize) -> Option<Plane> {
        let e = self.edge(i)?;
        if e.length <= EPS {
            return None;
        }
        let l = w(e.length);
        let p = e.point;
        let end = Vec3::new(
            n(w(p.x) + w(e.dir.x) * l),
            n(w(p.y) + w(e.dir.y) * l),
            n(w(p.z) + w(n(w(e.dir.z) * l))),
        );
        Some(Plane::vertical(p, end))
    }

    /// Whether every corner lies within `eps` of `plane`.
    #[must_use]
    pub fn check_if_in_plane(&self, plane: &Plane, eps: f32) -> bool {
        let m = plane.normal;
        self.vertices.iter().all(|v| {
            let d = ((w(m.x) * w(v.x) + w(v.z) * w(m.z)) + w(v.y) * w(m.y)) + w(plane.d);
            d.abs() <= w(eps) || d.is_nan()
        })
    }

    /// The box around the corners; none for no corners.
    // D2-13402
    #[must_use]
    pub fn bounds(&self) -> Option<BBox> {
        if self.vertices.is_empty() {
            return None;
        }
        let mut b = BBox::for_adjustment();
        for &v in &self.vertices {
            b.adjust_bbox(v);
        }
        Some(b)
    }

    /// Drops edges shorter than `min`, each replaced by its middle, from the first; a single
    /// corner is no polygon, and a polygon down to two corners with a short edge between them
    /// keeps them, or with `clear` loses them. False when an edge was not found.
    pub fn remove_short_edges(&mut self, min: f32, clear: bool) -> bool {
        let mut ok = true;
        if self.vertices.len() == 1 {
            self.vertices.clear();
        }
        let mut i = 0;
        while i < self.vertices.len() {
            let e = self.edge(i);
            ok &= e.is_some();
            let e = e.unwrap_or_default();
            let ex = n(w(e.point.x) + w(e.dir.x) * w(e.length));
            let dy = n(w(e.point.y) - (w(e.point.y) + w(e.dir.y) * w(e.length)));
            let dx = w(e.point.x) - w(ex);
            if w(min) * w(min) <= w(dy) * w(dy) + dx * dx {
                i += 1;
            } else {
                if self.vertices.len() == 2 {
                    if clear {
                        self.vertices.clear();
                    }
                    return ok;
                }
                self.replace_edge_with_midpoint(i);
            }
        }
        ok
    }

    /// Whether `p` lies outside some edge longer than `min` by at least `margin` (outside is the
    /// edges' positive side when `positive`, else their negative).
    // D2-13404
    #[must_use]
    pub fn is_outside(&self, p: Vec3, positive: bool, min: f32, margin: f32) -> bool {
        for i in 0..self.vertices.len() {
            if self.edge(i).is_some_and(|e| e.length < min) {
                continue;
            }
            if let Some(pl) = self.edge_plane(i) {
                let m = pl.normal;
                let mut d = ((w(m.x) * w(p.x) + w(m.y) * w(p.y)) + w(m.z) * w(p.z)) + w(pl.d);
                if !positive {
                    d = -d;
                }
                if w(margin) <= d {
                    return true;
                }
            }
        }
        false
    }

    /// The polygon's area: half the length of the sum of its edges' cross products, each axis
    /// pair summed around the corners.
    // D2-13399
    #[must_use]
    pub fn area(&self) -> f32 {
        let v = &self.vertices;
        let k = v.len();
        if k <= 2 {
            return 0.0;
        }
        let (mut xy, mut yz, mut zx) = (0.0_f64, 0.0_f64, 0.0_f64);
        for i in 0..k {
            let (a, b) = (v[i], v[(i + 1) % k]);
            let sx = w(n(w(a.x) + w(b.x)));
            xy += (w(a.y) - w(b.y)) * sx * 0.5;
            yz += (w(b.y) + w(a.y)) * (w(a.z) - w(b.z)) * 0.5;
            zx += (w(b.z) - w(a.z)) * sx * 0.5;
        }
        n((zx * zx + yz * yz + xy * xy).sqrt())
    }

    /// The area of the polygon's shadow on the ground.
    #[must_use]
    pub fn area_2d(&self) -> f32 {
        let v = &self.vertices;
        let k = v.len();
        if k <= 2 {
            return 0.0;
        }
        let mut a2 = 0.0_f64;
        for i in 0..k {
            let (a, b) = (v[i], v[(i + 1) % k]);
            a2 += (w(b.x) + w(a.x)) * (w(a.y) - w(b.y)) * 0.5;
        }
        n(a2.abs())
    }

    /// Whether the polygon's area is under `min`.
    #[must_use]
    pub fn is_degenerate(&self, min: f32) -> bool {
        self.area() < min
    }

    /// Whether the polygon is convex: of some area, and no corner at least `margin` outside any
    /// edge at least `min` long (outside the edges' negative side when `positive`, else their
    /// positive).
    #[must_use]
    pub fn is_convex(&self, positive: bool, min: f32, margin: f32) -> bool {
        if self.area() < EPS {
            return false;
        }
        for i in 0..self.vertices.len() {
            let long = self.edge(i).is_none_or(|e| {
                let ex = n(w(e.point.x) + w(e.dir.x) * w(e.length));
                let ey = n(w(e.point.y) + w(e.dir.y) * w(e.length));
                let (dx, dy) = (n(w(e.point.x) - w(ex)), w(e.point.y) - w(ey));
                w(min) * w(min) <= dy * dy + w(dx) * w(dx)
            });
            if !long {
                continue;
            }
            let Some(pl) = self.edge_plane(i) else {
                continue;
            };
            let m = pl.normal;
            for v in &self.vertices {
                let mut d = ((w(m.x) * w(v.x) + w(m.z) * w(v.z)) + w(m.y) * w(v.y)) + w(pl.d);
                if !positive {
                    d = -d;
                }
                if w(margin) <= d {
                    return false;
                }
            }
        }
        true
    }

    /// Drops each corner `i` whose next corner lies past `-margin` of edge `i`'s plane (its
    /// negative side when `positive` is false) where the edges into and out of corner `i` run
    /// the same way across the ground: a corner on a straight run or a concave turn. False when
    /// an edge plane was not found.
    pub fn remove_colinear_and_concave_points(&mut self, positive: bool, margin: f32) -> bool {
        let mut ok = true;
        let mut i = 0;
        while i < self.vertices.len() {
            let k = self.vertices.len();
            if k < 3 {
                return ok;
            }
            let pl = self.edge_plane(i);
            ok &= pl.is_some();
            let pl = pl.unwrap_or_default();
            let next = (i + 1) % k;
            let v = self.vertices[next];
            let m = pl.normal;
            let mut d = ((w(m.x) * w(v.x) + w(m.z) * w(v.z)) + w(m.y) * w(v.y)) + w(pl.d);
            if !positive {
                d = -d;
            }
            let mut step = true;
            if -w(margin) < d {
                let a = self.edge(i).unwrap_or_default();
                let b = self.edge(next).unwrap_or_default();
                if (w(b.dir.x) * w(a.dir.x) + w(b.dir.y) * w(a.dir.y)) > 0.0 {
                    self.vertices.remove(i);
                    step = false;
                }
            }
            if step {
                i += 1;
            }
        }
        ok
    }
}
