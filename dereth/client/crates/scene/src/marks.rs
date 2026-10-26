//! Marks a front end places in the world: lines, outlines and dots, drawn after the world and
//! against its depth, so a wall or a hill in front of a mark hides it. A mark that asks to stay
//! on top is drawn over everything instead.
//!
//! A line is a ribbon that faces the camera and keeps its width in pixels at any distance; a
//! single point is a square dot of that side. The geometry is built here, with no device, so it
//! can be checked without one.

/// One mark: points in the world's own space (Z up), joined in order. A mark of one point is a
/// dot.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldMark {
    pub points: Vec<[f32; 3]>,
    /// `0xAARRGGBB`.
    pub color: u32,
    /// The line's width, or the dot's side, in back-buffer pixels.
    pub width: f32,
    /// Drawn over whatever stands in front of it.
    pub on_top: bool,
}

/// Bytes per vertex: position (three floats), colour, texture coordinate (two floats).
pub const MARK_VERTEX_BYTES: usize = 24;

/// The nearest a mark's point is taken to be, in metres, when its width is worked out: a point
/// at or behind the camera would otherwise have no width or a negative one.
const NEAREST: f32 = 0.05;

/// A Z-up world point in the device's Y-up order.
fn device_order(p: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::new(p[0], p[2], p[1])
}

fn push(out: &mut Vec<u8>, p: glam::Vec3, color: u32) {
    for f in [p.x, p.y, p.z] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out.extend_from_slice(&color.to_le_bytes());
    out.extend_from_slice(&0.5f32.to_le_bytes());
    out.extend_from_slice(&0.5f32.to_le_bytes());
}

fn quad(out: &mut Vec<u8>, c: [glam::Vec3; 4], color: u32) {
    for i in [0, 1, 2, 0, 2, 3] {
        push(out, c[i], color);
    }
}

/// A mark's triangles, six vertices a segment (or a dot), in the device's order and world space.
///
/// `view` is the camera's view matrix, `fov_y` its vertical field of view in radians and
/// `viewport_height` the 3D view's height in pixels: together they say how many metres a pixel
/// spans at a point's distance. Each point is lifted by half the line's width there, so a line
/// drawn along the ground sits on it rather than half inside it.
#[must_use]
pub fn mark_vertices(
    view: &glam::Mat4,
    fov_y: f32,
    viewport_height: u32,
    mark: &WorldMark,
) -> Vec<u8> {
    let mut out = Vec::new();
    #[allow(clippy::cast_precision_loss)] // a viewport extent, at most a few thousand
    let rows = viewport_height.max(1) as f32;
    let per_pixel = 2.0 * dereth_primitives::num::math::tanf(fov_y * 0.5) / rows;
    let inverse = view.inverse();
    let eye = inverse.transform_point3(glam::Vec3::ZERO);
    let (right, up) = (
        inverse.transform_vector3(glam::Vec3::X).normalize_or_zero(),
        inverse.transform_vector3(glam::Vec3::Y).normalize_or_zero(),
    );
    // Half the width, in metres, at a point's distance from the camera.
    let half = |p: glam::Vec3| {
        let depth = view.transform_point3(p).z.max(NEAREST);
        mark.width.max(1.0) * 0.5 * per_pixel * depth
    };
    let lifted: Vec<glam::Vec3> = mark
        .points
        .iter()
        .map(|&p| {
            let at = device_order(p);
            at + glam::Vec3::Y * (half(at) + 0.02)
        })
        .collect();
    if let [p] = lifted.as_slice() {
        let h = half(*p);
        let (r, u) = (right * h, up * h);
        quad(
            &mut out,
            [*p - r - u, *p + r - u, *p + r + u, *p - r + u],
            mark.color,
        );
        return out;
    }
    for w in lifted.windows(2) {
        let (a, b) = (w[0], w[1]);
        let along = b - a;
        if along.length_squared() < 1e-8 {
            continue;
        }
        let side = |p: glam::Vec3| {
            let s = along.cross(eye - p).normalize_or_zero();
            if s == glam::Vec3::ZERO {
                right
            } else {
                s
            }
        };
        let (sa, sb) = (side(a) * half(a), side(b) * half(b));
        quad(&mut out, [a - sa, a + sa, b + sb, b - sb], mark.color);
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the geometry of a drawing aid with no retail counterpart; the pictures
    //! it makes are checked in the plugin overlay's live capture)

    use super::{mark_vertices, WorldMark, MARK_VERTEX_BYTES};

    fn positions(bytes: &[u8]) -> Vec<glam::Vec3> {
        bytes
            .chunks(MARK_VERTEX_BYTES)
            .map(|v| {
                let f = |i: usize| f32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]);
                glam::Vec3::new(f(0), f(4), f(8))
            })
            .collect()
    }

    /// A camera at the origin looking along +Z in device order (north in the world), level.
    fn view() -> glam::Mat4 {
        glam::Mat4::IDENTITY
    }

    #[test]
    fn a_line_keeps_its_pixel_width_at_any_distance() {
        let mark = |y: f32| WorldMark {
            points: vec![[-1.0, y, 0.0], [1.0, y, 0.0]],
            color: 0xFFFF_0000,
            width: 4.0,
            on_top: false,
        };
        let fov = std::f32::consts::FRAC_PI_2;
        let width_at = |y: f32| {
            let p = positions(&mark_vertices(&view(), fov, 600, &mark(y)));
            assert_eq!(p.len(), 6, "one segment is two triangles");
            (p[1].y - p[0].y).abs()
        };
        // At 90 degrees over 600 rows a pixel spans 2·d/600 metres.
        let near = width_at(10.0);
        let far = width_at(40.0);
        assert!((near - 4.0 * 2.0 * 10.0 / 600.0).abs() < 1e-4, "{near}");
        assert!(
            (far / near - 4.0).abs() < 1e-3,
            "four times as far is four times as wide"
        );
    }

    #[test]
    fn a_line_on_the_ground_is_lifted_clear_of_it() {
        let mark = WorldMark {
            points: vec![[-1.0, 10.0, 0.0], [1.0, 10.0, 0.0]],
            color: 0xFFFF_FFFF,
            width: 6.0,
            on_top: false,
        };
        let p = positions(&mark_vertices(
            &view(),
            std::f32::consts::FRAC_PI_2,
            600,
            &mark,
        ));
        assert!(p.iter().all(|v| v.y >= 0.0), "{p:?}");
    }

    #[test]
    fn one_point_is_a_square_dot_facing_the_camera() {
        let mark = WorldMark {
            points: vec![[0.0, 20.0, 1.0]],
            color: 0xFF00_FF00,
            width: 6.0,
            on_top: true,
        };
        let p = positions(&mark_vertices(
            &view(),
            std::f32::consts::FRAC_PI_2,
            600,
            &mark,
        ));
        assert_eq!(p.len(), 6);
        assert!(
            p.iter().all(|v| (v.z - 20.0).abs() < 1e-4),
            "flat to the camera"
        );
        let w = p.iter().map(|v| v.x).fold(f32::MIN, f32::max)
            - p.iter().map(|v| v.x).fold(f32::MAX, f32::min);
        let h = p.iter().map(|v| v.y).fold(f32::MIN, f32::max)
            - p.iter().map(|v| v.y).fold(f32::MAX, f32::min);
        assert!((w - h).abs() < 1e-4 && (w - 6.0 * 2.0 * 20.0 / 600.0).abs() < 1e-4);
    }

    #[test]
    fn a_mark_with_no_length_draws_nothing() {
        let mark = WorldMark {
            points: vec![[0.0, 5.0, 0.0], [0.0, 5.0, 0.0]],
            color: 0xFFFF_FFFF,
            width: 2.0,
            on_top: false,
        };
        assert!(mark_vertices(&view(), 1.0, 600, &mark).is_empty());
    }
}
