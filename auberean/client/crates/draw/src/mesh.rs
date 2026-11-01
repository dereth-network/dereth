//! A mesh's vertices read out of its stored vertex array, and the level of detail drawn at a
//! distance.

use auberean_assets::mesh::{Mesh, VertexArray};
use auberean_assets::setup::DetailDistances;

/// One vertex as drawing uses it.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// Alpha, red, green, blue packed in a word.
    pub diffuse: u32,
    /// Up to four coordinate sets.
    pub uv: [[f32; 2]; 4],
    /// The surface's own frame for its normal map: the directions in which its first coordinate
    /// set's u and v grow (zero when the format has none).
    pub tangent_u: [f32; 3],
    pub tangent_v: [f32; 3],
    /// Where the light is worked out per vertex, the direction its normal map is lit from, in the
    /// surface's own frame (u, v, normal); zero elsewhere.
    pub to_light: [f32; 3],
}

/// Where each element sits in a stored vertex, from the format word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub stride: usize,
    pub position: Option<usize>,
    pub normal: Option<usize>,
    pub diffuse: Option<usize>,
    /// The three tangent-space vectors, where present.
    pub tangents: [Option<usize>; 3],
    pub uv: usize,
    pub uv_sets: usize,
}

/// The layout of vertex format `format`, in stored order: position, blend weights, normal, point
/// size, diffuse, specular, the three tangent-space vectors, the coordinate pairs, the matrix
/// palette; padded past the next 32 bytes when the padding flag is set.
// D2-13121
#[must_use]
pub fn layout(format: u32) -> Layout {
    let mut at = 0usize;
    let position = (format & 0xE != 0).then(|| {
        let p = at;
        at += 12;
        p
    });
    at += match format & 0xE {
        6 => 4,
        8 => 8,
        10 => 12,
        12 => 16,
        14 => 20,
        _ => 0,
    };
    let normal = (format & 0x10 != 0).then(|| {
        let p = at;
        at += 12;
        p
    });
    if format & 0x20 != 0 {
        at += 4;
    }
    let diffuse = (format & 0x40 != 0).then(|| {
        let p = at;
        at += 4;
        p
    });
    if format & 0x80 != 0 {
        at += 4;
    }
    let mut tangents = [None; 3];
    for (k, bit) in [0x1000_0000, 0x2000_0000, 0x4000_0000]
        .into_iter()
        .enumerate()
    {
        if format & bit != 0 {
            tangents[k] = Some(at);
            at += 12;
        }
    }
    let uv = at;
    let uv_sets = ((format & 0xF00) >> 8) as usize;
    at += uv_sets * 8;
    at += ((format >> 16) & 0xFF) as usize * 5;
    if format & 0x0100_0000 != 0 {
        at = (at / 32 + 1) * 32;
    }
    Layout {
        stride: at,
        position,
        normal,
        diffuse,
        tangents,
        uv,
        uv_sets,
    }
}

fn f32_at(b: &[u8], k: usize) -> f32 {
    b.get(k..k + 4)
        .map_or(0.0, |s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn vec3_at(b: &[u8], k: usize) -> [f32; 3] {
    [f32_at(b, k), f32_at(b, k + 4), f32_at(b, k + 8)]
}

/// The vertices of a stored array, or `None` when its bytes do not fit its format and count.
#[must_use]
pub fn vertices(v: &VertexArray) -> Option<Vec<MeshVertex>> {
    let l = layout(v.format);
    let count = usize::try_from(v.count).ok()?;
    if l.stride == 0 || v.bytes.len() != l.stride * count {
        return None;
    }
    Some(
        v.bytes
            .chunks_exact(l.stride)
            .map(|b| {
                let mut out = MeshVertex {
                    diffuse: 0xFFFF_FFFF,
                    ..MeshVertex::default()
                };
                if let Some(p) = l.position {
                    out.position = vec3_at(b, p);
                }
                if let Some(p) = l.normal {
                    out.normal = vec3_at(b, p);
                }
                if let Some(p) = l.diffuse {
                    out.diffuse = u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]]);
                }
                if let Some(p) = l.tangents[0] {
                    out.tangent_u = vec3_at(b, p);
                }
                if let Some(p) = l.tangents[1] {
                    out.tangent_v = vec3_at(b, p);
                }
                for k in 0..l.uv_sets.min(4) {
                    out.uv[k] = [f32_at(b, l.uv + 8 * k), f32_at(b, l.uv + 8 * k + 4)];
                }
                out
            })
            .collect(),
    )
}

/// How much detail an object at `distance` is drawn with, 1 (full) to 0 (least), from its setup's
/// distances; `None` when it is past its visible distance.
// D2-06658
#[must_use]
pub fn detail_at(d: &DetailDistances, distance: f32) -> Option<f32> {
    let visible = d.visible.max(20.0);
    if distance > visible {
        return None;
    }
    let safe = |x: f32| if x == 0.0 { 0.0002 } else { x };
    let t = if distance >= d.ideal_degrade {
        0.5 * (1.0 - (distance - d.ideal_degrade) / safe(d.far_degrade - d.ideal_degrade))
    } else {
        0.5 + 0.5 * (1.0 - (distance - d.near_degrade) / safe(d.ideal_degrade - d.near_degrade))
    };
    Some(t.clamp(0.0, 1.0))
}

/// The detail level drawn at detail `t` with model detail `model_detail` (0 to 2): of the levels
/// marked for that model detail (all of them when none is), the one `(1 - t)` of the way down.
// D2-05510
#[must_use]
pub fn level_at(mesh: &Mesh, t: f32, model_detail: u32) -> Option<usize> {
    let bit = 1u32 << model_detail;
    let mut marked: Vec<usize> = (0..mesh.levels.len())
        .filter(|&k| mesh.levels[k].detail & bit != 0)
        .collect();
    if marked.is_empty() {
        marked = (0..mesh.levels.len()).collect();
    }
    let last = marked.len().checked_sub(1)?;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let k = ((1.0 - t) * last as f32) as usize;
    marked.get(k.min(last)).copied()
}
