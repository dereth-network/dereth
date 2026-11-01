//! Skinning: a mesh's vertices moved by up to four bones each, on the CPU, as the client blends
//! them.
//!
//! A skinned mesh's vertex format names how many bone influences each vertex carries (bits 16 to
//! 23). They are stored last in the vertex, after the texture coordinates: that many one-byte bone
//! indices, then as many 32-bit weights. A vertex is moved by the weighted sum of its bones'
//! palette transforms; a mesh with no influences moves whole with the bone it hangs from.

use auberean_assets::mesh::VertexArray;
use auberean_primitives::Vec3;
use auberean_world::math::Mat4;

/// The most bone influences a vertex blends.
pub const MAX_INFLUENCES: usize = 4;

/// One vertex's bone influences: up to four bone indices with their weights, unused ones zero.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Influences {
    pub count: u8,
    pub bones: [u8; MAX_INFLUENCES],
    pub weights: [f32; MAX_INFLUENCES],
}

/// How many bone influences each vertex of format `format` carries.
#[must_use]
pub fn influence_count(format: u32) -> usize {
    ((format >> 16) & 0xFF) as usize
}

/// Where a vertex's influences start: after its position, blend weights, normal, point size, two
/// colors, three extra vectors and texture coordinates.
fn influence_offset(format: u32) -> usize {
    let mut at = 0usize;
    if format & 0xE != 0 {
        at += 12;
    }
    at += 4 * match format & 0xE {
        6 => 1,
        8 => 2,
        10 => 3,
        12 => 4,
        14 => 5,
        _ => 0,
    };
    for (bit, size) in [
        (0x10, 12),
        (0x20, 4),
        (0x40, 4),
        (0x80, 4),
        (0x1000_0000, 12),
        (0x2000_0000, 12),
        (0x4000_0000, 12),
    ] {
        if format & bit != 0 {
            at += size;
        }
    }
    at + 8 * ((format & 0xF00) >> 8) as usize
}

/// Every vertex's influences, or `None` when the array carries none or its bytes do not fit its
/// format. Only the first four of a vertex's influences are kept.
// D2-09879
#[must_use]
pub fn influences(v: &VertexArray) -> Option<Vec<Influences>> {
    let n = influence_count(v.format);
    if n == 0 {
        return None;
    }
    let stride = auberean_assets::mesh::vertex_size(v.format);
    let count = usize::try_from(v.count).ok()?;
    if stride == 0 || v.bytes.len() != stride * count {
        return None;
    }
    let at = influence_offset(v.format);
    if at + 5 * n > stride {
        return None;
    }
    let keep = n.min(MAX_INFLUENCES);
    Some(
        v.bytes
            .chunks_exact(stride)
            .map(|b| {
                let mut out = Influences {
                    count: u8::try_from(keep).unwrap_or(4),
                    ..Influences::default()
                };
                for k in 0..keep {
                    out.bones[k] = b[at + k];
                    let w = at + n + 4 * k;
                    out.weights[k] = f32::from_le_bytes([b[w], b[w + 1], b[w + 2], b[w + 3]]);
                }
                out
            })
            .collect(),
    )
}

fn add_scaled(acc: Vec3, v: Vec3, w: f32) -> Vec3 {
    Vec3::new(acc.x + v.x * w, acc.y + v.y * w, acc.z + v.z * w)
}

/// Positions and normals moved by `palette`: each the weighted sum of its bones' transforms of it.
/// A bone index outside the palette counts as the identity.
#[must_use]
pub fn skin(
    positions: &[Vec3],
    normals: &[Vec3],
    influences: &[Influences],
    palette: &[Mat4],
) -> (Vec<Vec3>, Vec<Vec3>) {
    let mut out_p = Vec::with_capacity(positions.len());
    let mut out_n = Vec::with_capacity(normals.len());
    for (i, p) in positions.iter().enumerate() {
        let inf = influences.get(i).copied().unwrap_or_default();
        let n = normals.get(i).copied().unwrap_or(Vec3::ZERO);
        let (mut sp, mut sn) = (Vec3::ZERO, Vec3::ZERO);
        let mut total = 0.0;
        for k in 0..usize::from(inf.count) {
            let w = inf.weights[k];
            if w == 0.0 {
                continue;
            }
            let m = palette
                .get(usize::from(inf.bones[k]))
                .copied()
                .unwrap_or(Mat4::IDENTITY);
            sp = add_scaled(sp, m.point(*p), w);
            sn = add_scaled(sn, m.direction(n), w);
            total += w;
        }
        if total == 0.0 {
            sp = *p;
            sn = n;
        }
        out_p.push(sp);
        out_n.push(sn);
    }
    (out_p, out_n)
}
