//! A skinned body: one mesh drawn in place of a setup's parts, each vertex carried by up to four
//! of those parts, so the body bends where its parts meet instead of breaking apart.
//!
//! This is Dereth's own record, not a retail one. It sits in the render-mesh id range
//! (`0x19000000`–`0x19FFFFFF`), which the shipped files never fill, at the id
//! [`skinned_body_id`] gives its setup: a setup has a skinned body exactly when a record is
//! there, and nothing about the setup itself changes. A setup with one keeps its own parts and
//! their models, so whatever does not draw skinned bodies (the server, collision, an older
//! client) sees an ordinary composite object.
//!
//! The body is posed by the parts' frames as the setup's animations place them, each relative
//! to the object: a vertex goes where its parts' frames now take it from where their frames in
//! [`SkinnedBody::bind`] held it, weighed by its weights.
//!
//! The record, little-endian:
//!
//! | field | |
//! |---|---|
//! | id | `u32`, the record's own |
//! | setup | `u32`, the setup whose parts pose it |
//! | part count, then each part's bind frame | `u32`, then origin and rotation (w, x, y, z), seven `f32` each |
//! | surface count, then each surface | `u32`, then a `0x08` surface id each |
//! | vertex count, then each vertex | `u32`; position and normal, three `f32` each; u and v, `f32`; four part indices, `u16`; four weights, `f32` |
//! | group count, then each group | `u32`; its surface's index, `u32`; its flags, `u32` (bit 0: drawn from both sides; bits 8 to 15: its level of detail); its index count, `u32`; that many `u32` vertex indices, three to a triangle, wound as a model's polygons are |
//! | level count, then each level's distance | only when the body has levels of detail past its fullest: `u32`, then an `f32` each |
//!
//! Its pictures repeat past their edges, as a tiled model's do.
//!
//! **Levels of detail.** A body may carry coarser versions of itself, as more groups over the
//! same vertices: each group names the level it belongs to, 0 for the fullest. Level `k` is
//! drawn from [`SkinnedBody::levels`]`[k - 1]` meters from the eye on (at the object's own size;
//! a larger object switches farther away), and only one level's groups are drawn at a time.

use dereth_dat::{Cursor, DbType};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::error::AssetError;
use crate::Decode;

/// The first id of the range skinned bodies are kept in.
pub const SKINNED_BODY_BASE: u32 = 0x1900_0000;

/// The id the skinned body of `setup` is kept at, if it has one: the range's first byte over
/// the setup's own low sixteen bits.
#[must_use]
pub fn skinned_body_id(setup: DataId) -> DataId {
    DataId(SKINNED_BODY_BASE | (setup.raw() & 0xFFFF))
}

/// One vertex of a skinned body, in the object's space at the bind pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinnedVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
    /// The parts that carry it, by index into the setup's parts.
    pub parts: [u16; 4],
    /// How much each carries; they sum to one, and an unused slot weighs nothing.
    pub weights: [f32; 4],
}

/// The triangles of a skinned body drawn with one surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinnedGroup {
    /// The surface, by index into [`SkinnedBody::surfaces`].
    pub surface: u32,
    /// Whether its triangles are drawn from both sides, as a two-sided polygon is (a wing's
    /// membrane, seen from in front and from behind).
    pub two_sided: bool,
    /// Vertex indices, three to a triangle.
    pub indices: Vec<u32>,
    /// The level of detail it belongs to: 0 for the fullest.
    pub level: u8,
}

/// A decoded skinned body.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinnedBody {
    pub id: DataId,
    /// The setup whose parts pose it.
    pub setup: DataId,
    /// Each part's frame, relative to the object, in the pose the vertices are bound in.
    pub bind: Vec<Frame>,
    pub surfaces: Vec<DataId>,
    pub vertices: Vec<SkinnedVertex>,
    pub groups: Vec<SkinnedGroup>,
    /// How far from the eye each level of detail past the fullest is drawn from, meters at the
    /// object's own size, nearest first; empty for a body drawn at one level.
    pub levels: Vec<f32>,
}

impl SkinnedBody {
    /// The level of detail drawn `distance` meters from the eye, the object at `scale`: the last
    /// whose distance it has reached, 0 nearer than all.
    #[must_use]
    pub fn level_at(levels: &[f32], distance: f32, scale: f32) -> usize {
        let d = distance / scale.max(1e-3);
        levels.iter().take_while(|from| d >= **from).count()
    }
}

impl Decode for SkinnedBody {
    const TYPE: DbType = DbType::RenderMesh;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let setup = c.data_id()?;
        let n = c.u32()? as usize;
        let bind = (0..n).map(|_| c.frame()).collect::<Result<Vec<_>, _>>()?;
        let n = c.u32()? as usize;
        let surfaces = (0..n).map(|_| c.data_id()).collect::<Result<Vec<_>, _>>()?;
        let n = c.u32()? as usize;
        let mut vertices = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            let position = c.vec3()?;
            let normal = c.vec3()?;
            let uv = [c.f32()?, c.f32()?];
            let parts = [c.u16()?, c.u16()?, c.u16()?, c.u16()?];
            let weights = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
            vertices.push(SkinnedVertex {
                position,
                normal,
                uv,
                parts,
                weights,
            });
        }
        let n = c.u32()? as usize;
        let mut groups = Vec::with_capacity(n.min(1 << 12));
        for _ in 0..n {
            let surface = c.u32()?;
            let flags = c.u32()?;
            let k = c.u32()? as usize;
            let indices = (0..k).map(|_| c.u32()).collect::<Result<Vec<_>, _>>()?;
            groups.push(SkinnedGroup {
                surface,
                two_sided: flags & 1 != 0,
                indices,
                #[allow(clippy::cast_possible_truncation)] // bits 8 to 15
                level: (flags >> 8) as u8,
            });
        }
        let levels = if c.remaining() > 0 {
            let n = c.u32()? as usize;
            (0..n).map(|_| c.f32()).collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        Ok(Self {
            id,
            setup,
            bind,
            surfaces,
            vertices,
            groups,
            levels,
        })
    }
}

impl SkinnedBody {
    /// The record, the exact inverse of its decoder.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(
            16 + self.bind.len() * 28 + self.surfaces.len() * 4 + self.vertices.len() * 56,
        );
        let u32_ = |o: &mut Vec<u8>, v: u32| o.extend_from_slice(&v.to_le_bytes());
        let f32_ = |o: &mut Vec<u8>, v: f32| o.extend_from_slice(&v.to_bits().to_le_bytes());
        let count = |o: &mut Vec<u8>, n: usize| {
            o.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
        };
        u32_(&mut o, self.id.raw());
        u32_(&mut o, self.setup.raw());
        count(&mut o, self.bind.len());
        for f in &self.bind {
            for v in [
                f.origin.x,
                f.origin.y,
                f.origin.z,
                f.rotation.w,
                f.rotation.x,
                f.rotation.y,
                f.rotation.z,
            ] {
                f32_(&mut o, v);
            }
        }
        count(&mut o, self.surfaces.len());
        for s in &self.surfaces {
            u32_(&mut o, s.raw());
        }
        count(&mut o, self.vertices.len());
        for v in &self.vertices {
            for x in [v.position.x, v.position.y, v.position.z] {
                f32_(&mut o, x);
            }
            for x in [v.normal.x, v.normal.y, v.normal.z] {
                f32_(&mut o, x);
            }
            f32_(&mut o, v.uv[0]);
            f32_(&mut o, v.uv[1]);
            for p in v.parts {
                o.extend_from_slice(&p.to_le_bytes());
            }
            for w in v.weights {
                f32_(&mut o, w);
            }
        }
        count(&mut o, self.groups.len());
        for g in &self.groups {
            u32_(&mut o, g.surface);
            u32_(&mut o, u32::from(g.two_sided) | u32::from(g.level) << 8);
            count(&mut o, g.indices.len());
            for i in &g.indices {
                u32_(&mut o, *i);
            }
        }
        if !self.levels.is_empty() {
            count(&mut o, self.levels.len());
            for l in &self.levels {
                f32_(&mut o, *l);
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the skinned body record's format)
    use super::*;
    use dereth_primitives::Quat;

    fn body() -> SkinnedBody {
        SkinnedBody {
            id: DataId(0x1900_C001),
            setup: DataId(0x0200_C001),
            bind: vec![
                Frame::default(),
                Frame::new(Vec3::new(0.0, 0.5, 1.0), Quat::new(0.6, 0.0, 0.0, 0.8)),
            ],
            surfaces: vec![DataId(0x0800_C000)],
            vertices: (0..3u16)
                .map(|k| SkinnedVertex {
                    position: Vec3::new(f32::from(k), 0.0, 1.0),
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    uv: [0.25, 0.75],
                    parts: [0, 1, 0, 0],
                    weights: [0.75, 0.25, 0.0, 0.0],
                })
                .collect(),
            groups: vec![SkinnedGroup {
                surface: 0,
                two_sided: true,
                indices: vec![0, 1, 2],
                level: 0,
            }],
            levels: Vec::new(),
        }
    }

    #[test]
    fn a_skinned_body_reads_back_as_it_was_written() {
        let b = body();
        let bytes = b.encode();
        assert_eq!(SkinnedBody::decode_payload(b.id, &bytes).unwrap(), b);
    }

    /// A body with a coarser level reads back with it, and one without is written as a body was
    /// before levels of detail, so records already written read the same.
    #[test]
    fn a_skinned_bodys_levels_of_detail_read_back_and_a_body_without_them_is_unchanged() {
        let one = body();
        let mut two = body();
        two.groups.push(SkinnedGroup {
            surface: 0,
            two_sided: false,
            indices: vec![0, 2, 1],
            level: 1,
        });
        two.levels = vec![12.5];
        let bytes = two.encode();
        assert_eq!(SkinnedBody::decode_payload(two.id, &bytes).unwrap(), two);
        let plain = one.encode();
        assert_eq!(
            plain.len(),
            two.encode().len() - 4 - 4 - 4 - 4 * 3 - 4 - 4,
            "no level table without levels"
        );
    }

    #[test]
    fn the_level_drawn_is_the_last_whose_distance_is_reached_at_the_objects_size() {
        let levels = [10.0, 20.0, 40.0];
        assert_eq!(SkinnedBody::level_at(&levels, 5.0, 1.0), 0);
        assert_eq!(SkinnedBody::level_at(&levels, 10.0, 1.0), 1);
        assert_eq!(SkinnedBody::level_at(&levels, 39.0, 1.0), 2);
        assert_eq!(SkinnedBody::level_at(&levels, 400.0, 1.0), 3);
        assert_eq!(
            SkinnedBody::level_at(&levels, 39.0, 2.0),
            1,
            "twice the size"
        );
        assert_eq!(SkinnedBody::level_at(&[], 400.0, 1.0), 0);
    }

    #[test]
    fn a_setups_skinned_body_is_kept_in_the_render_mesh_range_at_its_low_bits() {
        assert_eq!(skinned_body_id(DataId(0x0200_C0A1)), DataId(0x1900_C0A1));
        assert_eq!(
            dereth_dat::divine_type(skinned_body_id(DataId(0x0200_0001))),
            Some(DbType::RenderMesh)
        );
    }

    #[test]
    fn a_short_skinned_body_is_refused() {
        let bytes = body().encode();
        assert!(
            SkinnedBody::decode_payload(DataId(0x1900_C001), &bytes[..bytes.len() - 2]).is_err()
        );
    }
}
