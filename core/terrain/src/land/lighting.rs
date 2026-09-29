//! The landblock's own vertex-lighting pass.
//!
//! The vertex-lighting calculation follows the landblock structure and polygon code.
//!
//! Terrain never uses a D3D light: fixed-function lighting is switched off before every land
//! polygon and the whole shading is this pre-baked vertex diffuse.
//!
//! The sunlight vector is **not normalized**; sky lighting returns a
//! vector whose *length is `dir_bright`*, so `n · sunlight` already carries the brightness.
//! Normalising it flattens the entire day cycle.

use dereth_primitives::Vec3;

use crate::consts::EPSILON;
use crate::land::mesh::LandblockMesh;

/// The four inputs to landscape lighting. `ambient_color` and `sunlight_color` are packed
/// `0x00RRGGBB` in the client; they are kept unpacked here because every use divides by 255.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandscapeLighting {
    /// Ambient level, constructed at `0.4`.
    pub ambient_level: f32,
    /// Ambient color, `0..=255` per channel.
    pub ambient_color: [u8; 3],
    /// Sunlight is a **direction with magnitude**. Constructor default `(1.2, 0.0, 0.5)`.
    pub sunlight: Vec3,
    /// Sunlight color, `0..=255` per channel.
    pub sunlight_color: [u8; 3],
}

impl Default for LandscapeLighting {
    /// The landscape-lighting defaults: `ambient_level = 0.4`, `sunlight = (1.2, 0.0, 0.5)`.
    /// These are the retail landscape-lighting defaults.
    fn default() -> Self {
        Self {
            ambient_level: 0.4,
            ambient_color: [255, 255, 255],
            sunlight: Vec3::new(1.2, 0.0, 0.5),
            sunlight_color: [255, 255, 255],
        }
    }
}

impl LandscapeLighting {
    /// Set world ambient light to `|sunlight| * 0.2 + ambient_level` with `ambient_color` — the
    /// ambient that *objects* see, which is brighter than the terrain's by 20 % of the sun's
    /// magnitude.
    #[must_use]
    pub fn world_ambient_level(&self) -> f32 {
        self.sunlight.magnitude() * 0.2 + self.ambient_level
    }
}

/// Step 1-3 of `calc_lighting`: accumulate every incident triangle's plane normal at each vertex,
/// then normalise, falling back to `(0, 0, 1)` when the sum is shorter than 0.0002.
///
/// **The accumulated normals are the polygons' stored plane normals, which `make_plane` has already
/// normalised.** An earlier description claimed the sum was over *unnormalised* normals "so
/// larger triangles weigh more"; the implementation adds `plane.N` verbatim and divides the sum
/// by its length before storing it, so every triangle weighs the same.
#[must_use]
pub fn vertex_normals(m: &LandblockMesh) -> Vec<Vec3> {
    let n = m.vertices.len();
    let mut acc = vec![Vec3::ZERO; n];
    for p in &m.polygons {
        for &vi in &p.v {
            let a = &mut acc[vi as usize];
            a.x += p.plane.normal.x;
            a.y += p.plane.normal.y;
            a.z += p.plane.normal.z;
        }
    }
    for a in &mut acc {
        let len = a.magnitude();
        if len < EPSILON {
            *a = Vec3::new(0.0, 0.0, 1.0);
        } else {
            let inv = 1.0 / len;
            *a = Vec3::new(a.x * inv, a.y * inv, a.z * inv);
        }
    }
    acc
}

/// The landblock lighting pass, per vertex:
///
/// ```text
/// d = max(0, n . D)
/// r = min(1, (S.r/255)*d + (A.r/255)*L)      (and g, b)
/// ```
///
/// Returns floats in `[0, 1]`, which is what the client's per-vertex lighting array holds. The byte form the vertex
/// diffuse carries is [`calc_lighting`].
#[must_use]
pub fn calc_lighting_f32(m: &LandblockMesh, l: &LandscapeLighting) -> Vec<[f32; 3]> {
    const INV_255: f32 = 0.003_921_569; // the client's literal, not 1.0/255.0
    let a = l.ambient_color.map(|c| f32::from(c) * INV_255);
    let s = l.sunlight_color.map(|c| f32::from(c) * INV_255);
    let amb = [
        a[0] * l.ambient_level,
        a[1] * l.ambient_level,
        a[2] * l.ambient_level,
    ];
    vertex_normals(m)
        .into_iter()
        .map(|n| {
            let d = n.dot(l.sunlight).max(0.0);
            [
                (s[0] * d + amb[0]).min(1.0),
                (s[1] * d + amb[1]).min(1.0),
                (s[2] * d + amb[2]).min(1.0),
            ]
        })
        .collect()
}

/// [`calc_lighting_f32`] quantized for the packed land-polygon vertex colors:
/// `colour = 0xFF000000 | (r<<16) | (g<<8) | b`, from the vertex's lighting times 255, **truncated**.
#[must_use]
pub fn calc_lighting(m: &LandblockMesh, l: &LandscapeLighting) -> Vec<[u8; 3]> {
    calc_lighting_f32(m, l)
        .into_iter()
        .map(|c| {
            c.map(|v| {
                // The client's float-to-byte conversion truncates toward zero.
                let i = dereth_primitives::num::to_i32(v * 255.0);
                // LINT-OK: v is clamped to [0, 1] above, so i is in 0..=255.
                i.clamp(0, 255) as u8
            })
        })
        .collect()
}

/// Fill [`LandblockMesh::colours`] in place — `generate`'s "recompute the vertex lighting" step,
/// which runs after every geometry rebuild and again whenever the landscape-lighting setter changes the
/// sun.
pub fn bake_lighting(m: &mut LandblockMesh, l: &LandscapeLighting) {
    m.colours = calc_lighting(m, l);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::land::mesh::{make_plane, LandPolygon};
    use crate::Plane;
    use dereth_primitives::LandblockId;

    fn flat_mesh(z: f32) -> LandblockMesh {
        let svc = 9usize;
        let mut vertices = Vec::new();
        for i in 0..svc {
            for j in 0..svc {
                #[allow(clippy::cast_precision_loss)]
                vertices.push(Vec3::new(i as f32 * 24.0, j as f32 * 24.0, z));
            }
        }
        let mut polygons = Vec::new();
        for i in 0..8usize {
            for j in 0..8usize {
                for tri in crate::land::mesh::cell_triangles(svc, i, j, true) {
                    polygons.push(LandPolygon {
                        v: tri,
                        plane: make_plane(
                            vertices[tri[0] as usize],
                            vertices[tri[1] as usize],
                            vertices[tri[2] as usize],
                        ),
                        surface: 0,
                        uv_indices: [0, 1, 2],
                        stippling: 0,
                    });
                }
            }
        }
        LandblockMesh {
            id: LandblockId(0),
            side_cell_count: 8,
            side_vertex_count: 9,
            trans_dir: crate::land::mesh::Direction::InViewerBlock,
            vertices,
            sw_to_ne_cut: vec![true; 64],
            polygons,
            cell_keys: Vec::new(),
            cell_water: Vec::new(),
            water_type: crate::land::water::WaterType::NotWater,
            colours: Vec::new(),
            max_zval: 0.0,
            min_zval: 0.0,
        }
    }

    /// Oracle: every vertex of flat ground has normal
    /// `(0,0,1)` and therefore `d = sunlight.z`. With the landscape-lighting defaults
    /// (`sunlight = (1.2, 0, 0.5)`, `ambient_level = 0.4`, both colours white) the arithmetic is
    /// `min(1, 1.0*0.5 + 1.0*0.4) = 0.9` on every channel, and `0.9*255 = 229.5` truncates to 229.
    #[test]
    fn flat_ground_lights_to_the_hand_computed_value() {
        let m = flat_mesh(37.0);
        let l = LandscapeLighting::default();
        let f = calc_lighting_f32(&m, &l);
        assert_eq!(f.len(), 81);
        for c in &f {
            for &v in c {
                assert!((v - 0.9).abs() < 1e-6, "{v}");
            }
        }
        assert!(calc_lighting(&m, &l).iter().all(|&c| c == [229, 229, 229]));
    }

    /// Oracle: the sun vector's *length is `dir_bright`*, so scaling it must scale the lit term. If
    /// someone normalises `sunlight`, this
    /// test fails, and it is the whole day cycle that would otherwise flatten silently.
    #[test]
    fn the_sun_vector_carries_brightness_and_must_not_be_normalised() {
        let m = flat_mesh(0.0);
        let dim = LandscapeLighting {
            ambient_level: 0.0,
            ambient_color: [0, 0, 0],
            sunlight: Vec3::new(0.0, 0.0, 0.25),
            sunlight_color: [255, 255, 255],
        };
        let bright = LandscapeLighting {
            sunlight: Vec3::new(0.0, 0.0, 0.5),
            ..dim
        };
        assert!((calc_lighting_f32(&m, &dim)[0][0] - 0.25).abs() < 1e-6);
        assert!((calc_lighting_f32(&m, &bright)[0][0] - 0.5).abs() < 1e-6);
        // A normalised sun would give the same answer for both, which is exactly the bug.
        assert_ne!(
            calc_lighting_f32(&m, &dim)[0],
            calc_lighting_f32(&m, &bright)[0]
        );
    }

    /// Oracle: vertex lighting normalises each accumulated normal, uses `(0, 0, 1)` below length
    /// `0.0002`, and then applies the `d = max(0, ...)` clamp.
    #[test]
    fn degenerate_normals_fall_back_and_backfacing_light_clamps_to_zero() {
        let mut m = flat_mesh(0.0);
        // Two triangles with exactly opposing planes sum to zero at their shared vertices.
        m.polygons = vec![
            LandPolygon {
                v: [0, 1, 2],
                plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: 0.0,
                },
                surface: 0,
                uv_indices: [0, 1, 2],
                stippling: 0,
            },
            LandPolygon {
                v: [0, 1, 2],
                plane: Plane {
                    normal: Vec3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                surface: 0,
                uv_indices: [0, 1, 2],
                stippling: 0,
            },
        ];
        assert_eq!(vertex_normals(&m)[0], Vec3::new(0.0, 0.0, 1.0));
        // Every other vertex has no incident triangle at all, which is the same degenerate case.
        assert_eq!(vertex_normals(&m)[80], Vec3::new(0.0, 0.0, 1.0));

        // A sun below the horizon contributes nothing, leaving pure ambient.
        let l = LandscapeLighting {
            ambient_level: 0.5,
            ambient_color: [255, 255, 255],
            sunlight: Vec3::new(0.0, 0.0, -3.0),
            sunlight_color: [255, 255, 255],
        };
        assert!((calc_lighting_f32(&m, &l)[0][0] - 0.5).abs() < 1e-6);
    }

    /// Oracle: the recovered landscape-lighting behavior adds the `|sunlight| * 0.2 +
    /// ambient_level` boost only for *objects*. With the constructor defaults
    /// `|(1.2, 0, 0.5)| = 1.3`, so the object ambient is `0.26 + 0.4 = 0.66` against the terrain's
    /// 0.4.
    #[test]
    fn objects_see_a_brighter_ambient_than_the_terrain() {
        let l = LandscapeLighting::default();
        assert!((l.sunlight.magnitude() - 1.3).abs() < 1e-6);
        assert!((l.world_ambient_level() - 0.66).abs() < 1e-6);
        assert!(l.world_ambient_level() > l.ambient_level);
    }

    /// Oracle: vertex lighting uses a `min(1, ...)` per-channel clamp and truncating byte
    /// land-polygon color conversion. Full white must land on 255, not wrap.
    #[test]
    fn channels_clamp_at_one_and_quantise_by_truncation() {
        let m = flat_mesh(0.0);
        let l = LandscapeLighting {
            ambient_level: 5.0,
            ambient_color: [255, 255, 255],
            sunlight: Vec3::new(0.0, 0.0, 9.0),
            sunlight_color: [255, 255, 255],
        };
        assert_eq!(calc_lighting_f32(&m, &l)[0], [1.0, 1.0, 1.0]);
        assert_eq!(calc_lighting(&m, &l)[0], [255, 255, 255]);
    }
}
