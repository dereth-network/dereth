//! Turning a [`LandblockMesh`] into `DrawBatch`es.
//!
//! The landscape polygon emitters (one triangle and both triangles), the distance-based alpha,
//! the sidedness test with an epsilon, and the block draw.
//!
//! Triangle emission preserves the original renderer's branch and submission order.
//!
//! Vertex positions are **block-local** (0..192); the block's world position comes from the pushed
//! block position and the installed block-relative frame. Terrain z is
//! lowered by the z-fight adjustment (0.01) so decals and objects sitting exactly on the ground do not
//! z-fight, and the same constant is the epsilon fed to the back-face test.

use dereth_primitives::{
    DrawBatch, Frame, MeshData, MeshHandle, RenderBackend, TextureHandle, Vec3,
};

use crate::consts::{LAND_UVS, TEXTURE_U_SIZE, TEXTURE_V_SIZE, Z_FIGHT_TERRAIN_ADJUST};
use crate::land::mesh::{LandPolygon, LandblockMesh};
use crate::narrow::u32_of;
use crate::Plane;

/// `Sidedness` — the three-way plane-side answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sidedness {
    Positive,
    InPlane,
    Negative,
}

/// The sidedness test with an epsilon:
/// `f = N·p + d + adjust`; `f > eps` is POSITIVE, `f < -eps` is NEGATIVE, else IN_PLANE.
///
/// The land path passes `adjust` = the z-fight adjustment and `eps = 0.0002`.
#[must_use]
pub fn which_side2(plane: &Plane, p: Vec3, adjust: f32, eps: f32) -> Sidedness {
    let f = p.x * plane.normal.x + p.y * plane.normal.y + p.z * plane.normal.z + plane.d + adjust;
    if f > eps {
        Sidedness::Positive
    } else if -eps <= f {
        Sidedness::InPlane
    } else {
        Sidedness::Negative
    }
}

/// Detail-texture fade by view-space depth.
///
/// `z < 10 -> 255`, `z > 50 -> 0`, otherwise a linear ramp truncated to an integer;
/// `255*(50 - z)/40` is the only ramp consistent with the two endpoints.
/// UNVERIFIED: the exact multiplier remains an open question.
#[must_use]
pub fn get_alpha_for_z(z: f32) -> u8 {
    if z < 10.0 {
        return 255;
    }
    if z > 50.0 {
        return 0;
    }
    // LINT-OK: the value is in 0..=255 by the two guards above.
    dereth_primitives::num::to_i32(6.375 * (50.0 - z)).clamp(0, 255) as u8
}

/// One emitted land vertex, FVF `0x142` (`XYZ|DIFFUSE|TEX1`), stride 0x18.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandVertex {
    /// `(v.x, v.y, v.z - adjust)`, `adjust` the 0.01 z-fight adjustment.
    pub pos: [f32; 3],
    /// `0xFF000000 | (r<<16) | (g<<8) | b`.
    pub diffuse: u32,
    /// The land UV for this corner, scaled by the texture's U and V size.
    pub uv: [f32; 2],
}

impl LandVertex {
    /// The 24 bytes `DrawPrimitiveUP` receives, in FVF order and little-endian.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 24] {
        let mut b = [0u8; 24];
        b[0..4].copy_from_slice(&self.pos[0].to_le_bytes());
        b[4..8].copy_from_slice(&self.pos[1].to_le_bytes());
        b[8..12].copy_from_slice(&self.pos[2].to_le_bytes());
        b[12..16].copy_from_slice(&self.diffuse.to_le_bytes());
        b[16..20].copy_from_slice(&self.uv[0].to_le_bytes());
        b[20..24].copy_from_slice(&self.uv[1].to_le_bytes());
        b
    }
}

/// The FVF stride the land path uses: `0x18` = 24 bytes.
pub const LAND_VERTEX_STRIDE: u32 = 24;

/// Build the three vertices of one terrain triangle.
#[must_use]
pub fn triangle_vertices(m: &LandblockMesh, p: &LandPolygon) -> [LandVertex; 3] {
    let mut out = [LandVertex {
        pos: [0.0; 3],
        diffuse: 0xFF00_0000,
        uv: [0.0; 2],
    }; 3];
    for k in 0..3 {
        let vi = p.v[k] as usize;
        let v = m.vertices[vi];
        let c = m.colours.get(vi).copied().unwrap_or([255, 255, 255]);
        let uv = LAND_UVS[p.uv_indices[k] as usize];
        out[k] = LandVertex {
            pos: [v.x, v.y, v.z - Z_FIGHT_TERRAIN_ADJUST],
            diffuse: 0xFF00_0000
                | (u32::from(c[0]) << 16)
                | (u32::from(c[1]) << 8)
                | u32::from(c[2]),
            uv: [TEXTURE_U_SIZE * uv[0], TEXTURE_V_SIZE * uv[1]],
        };
    }
    out
}

/// Decide which of a cell's two triangles face the
/// viewer.
///
/// Returns the triangles to draw, in the client's order: when both face the viewer it takes the
/// two-triangle path with polygon 0 first; when only one does it takes the single-triangle path.
/// A cell whose triangles both face away emits nothing at all.
#[must_use]
pub fn visible_triangles(
    m: &LandblockMesh,
    i: usize,
    j: usize,
    viewpoint: Vec3,
) -> Vec<&LandPolygon> {
    let (a, b) = m.cell_polygons(i, j);
    let sa = which_side2(
        &a.plane,
        viewpoint,
        Z_FIGHT_TERRAIN_ADJUST,
        crate::consts::EPSILON,
    );
    let sb = which_side2(
        &b.plane,
        viewpoint,
        Z_FIGHT_TERRAIN_ADJUST,
        crate::consts::EPSILON,
    );
    match (sa == Sidedness::Positive, sb == Sidedness::Positive) {
        (true, true) => vec![a, b],
        (true, false) => vec![a],
        (false, true) => vec![b],
        (false, false) => Vec::new(),
    }
}

/// Draw one land cell: upload the visible triangles and submit
/// them as one batch, which is what `DrawPrimitiveUP(D3DPT_TRIANGLELIST, n, …)` does.
///
/// The terrain is deliberately **not** batched into vertex buffers by the client: every land cell
/// is one or two `DrawPrimitiveUP` calls. What matters for fidelity is the submission order, which
/// this preserves exactly; a rebuild is free to batch a whole block behind that.
pub fn draw_land_cell(
    m: &LandblockMesh,
    i: usize,
    j: usize,
    viewpoint: Vec3,
    transform: Frame,
    texture: Option<TextureHandle>,
    r: &mut dyn RenderBackend,
) -> Option<(MeshHandle, DrawBatch)> {
    let tris = visible_triangles(m, i, j, viewpoint);
    if tris.is_empty() {
        return None;
    }
    let mut vertices = Vec::with_capacity(tris.len() * 3 * LAND_VERTEX_STRIDE as usize);
    let mut indices = Vec::with_capacity(tris.len() * 3);
    for p in &tris {
        for v in triangle_vertices(m, p) {
            // LINT-OK: index arithmetic, at most six vertices per cell.
            indices.push(u32_of(indices.len()));
            vertices.extend_from_slice(&v.to_bytes());
        }
    }
    // LINT-OK: index arithmetic, at most six vertices per cell.
    let count = u32_of(indices.len());
    let mesh = r.upload_mesh(&MeshData {
        vertices,
        indices,
        stride: LAND_VERTEX_STRIDE,
    });
    let batch = DrawBatch {
        mesh,
        texture,
        transform,
        range: 0..count,
    };
    r.draw(&batch);
    Some((mesh, batch))
}

#[cfg(test)]
mod tests {
    // Index arithmetic in a test loop, bounded by the loop itself.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;

    /// Oracle: the retail guards — `< 10` returns `0xFF`,
    /// `> 50` returns 0, and the middle is the linear ramp. Note `z == 50` takes the ramp (which
    /// yields 0) rather than the early return, because the guard is a strict `50.0 < z`.
    #[test]
    fn detail_alpha_matches_the_two_guards_and_the_ramp() {
        assert_eq!(get_alpha_for_z(0.0), 255);
        assert_eq!(get_alpha_for_z(9.999), 255);
        assert_eq!(get_alpha_for_z(10.0), 255, "the ramp starts at 255");
        assert_eq!(get_alpha_for_z(30.0), 127);
        assert_eq!(
            get_alpha_for_z(50.0),
            0,
            "z == 50 takes the ramp, not the early return"
        );
        assert_eq!(get_alpha_for_z(50.001), 0);
        assert_eq!(get_alpha_for_z(1000.0), 0);
        // Monotone non-increasing across the whole range.
        let mut prev = 255u8;
        for i in 0..600u32 {
            let a = get_alpha_for_z(f32::from(i as u16) * 0.1);
            assert!(a <= prev, "alpha rose at z={}", f32::from(i as u16) * 0.1);
            prev = a;
        }
    }

    /// Oracle: the retail comparison — `f > eps` POSITIVE,
    /// `-eps <= f` IN_PLANE, else NEGATIVE, with the `adjust` term added *inside* `f`.
    #[test]
    fn which_side2_adds_the_adjust_before_comparing() {
        let p = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        assert_eq!(
            which_side2(&p, Vec3::new(0.0, 0.0, 1.0), 0.0, 0.0002),
            Sidedness::Positive
        );
        assert_eq!(
            which_side2(&p, Vec3::new(0.0, 0.0, -1.0), 0.0, 0.0002),
            Sidedness::Negative
        );
        assert_eq!(which_side2(&p, Vec3::ZERO, 0.0, 0.0002), Sidedness::InPlane);
        // A viewer exactly on the plane is POSITIVE once the z-fight adjust is added, which is why
        // terrain seen edge-on still draws.
        assert_eq!(
            which_side2(&p, Vec3::ZERO, Z_FIGHT_TERRAIN_ADJUST, 0.0002),
            Sidedness::Positive
        );
    }

    /// Oracle: emitted positions use `(v.x, v.y, v.z - Z_FIGHT_TERRAIN_ADJUST)` and
    /// `colour = 0xFF000000 | (r<<16) | (g<<8) | b`. The z bias is drawn-only: the mesh keeps true
    /// z so physics and scenery placement read the real ground.
    #[test]
    fn emitted_vertices_are_biased_down_by_the_zfight_constant() {
        let mut m = crate::land::tests_support::flat_mesh(100.0);
        m.colours = vec![[1, 2, 3]; m.vertices.len()];
        let (a, _) = m.cell_polygons(0, 0);
        let v = triangle_vertices(&m, a);
        assert_eq!(m.vertices[0].z, 100.0, "the mesh keeps true z");
        assert!(
            (v[0].pos[2] - 99.99).abs() < 1e-4,
            "the drawn vertex is 0.01 lower"
        );
        assert_eq!(v[0].diffuse, 0xFF01_0203);
        assert_eq!(v[0].to_bytes().len(), LAND_VERTEX_STRIDE as usize);
    }

    /// Oracle: [`visible_triangles`]'s four-way branch. A viewer
    /// above flat ground sees both triangles; a viewer below sees neither.
    #[test]
    fn back_face_culling_takes_the_documented_branch() {
        let m = crate::land::tests_support::flat_mesh(0.0);
        let above = visible_triangles(&m, 0, 0, Vec3::new(12.0, 12.0, 50.0));
        assert_eq!(above.len(), 2, "both triangles face a viewer above");
        let below = visible_triangles(&m, 0, 0, Vec3::new(12.0, 12.0, -50.0));
        assert!(below.is_empty(), "neither faces a viewer underground");
        // Polygon 0 comes first when both are drawn -- the two-triangle path's order.
        let (a, _) = m.cell_polygons(0, 0);
        assert!(std::ptr::eq(above[0], a));
    }

    /// A cell submits one batch of the visible triangles.
    #[test]
    fn a_cell_submits_one_batch_of_the_visible_triangles() {
        let mut m = crate::land::tests_support::flat_mesh(0.0);
        m.colours = vec![[255, 255, 255]; m.vertices.len()];
        let mut r = crate::testing::Recorder::new();
        let out = draw_land_cell(
            &m,
            0,
            0,
            Vec3::new(12.0, 12.0, 20.0),
            Frame::default(),
            None,
            &mut r,
        );
        assert!(out.is_some());
        assert_eq!(r.draws.len(), 1, "one DrawPrimitiveUP per cell");
        assert_eq!(r.draws[0].range, 0..6, "two triangles");
        assert_eq!(
            r.uploads[0].1,
            6 * LAND_VERTEX_STRIDE as usize,
            "stride 0x18 per vertex"
        );
        // A cell facing away emits nothing at all.
        let mut r2 = crate::testing::Recorder::new();
        assert!(draw_land_cell(
            &m,
            0,
            0,
            Vec3::new(12.0, 12.0, -20.0),
            Frame::default(),
            None,
            &mut r2
        )
        .is_none());
        assert!(r2.draws.is_empty());
    }
}
