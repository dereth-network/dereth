//! What the look reads from the player's files: Asheron's Call 2's entities down to their meshes,
//! materials and pictures, and its region's ground; and Asheron's Call's setups' collision.

use std::path::Path;

use auberean_assets::entity::Entity;
use auberean_assets::material::{Material, MaterialInstance, MaterialModifier, DIFFUSE_MAP};
use auberean_assets::mesh::{Mesh, StoredVertex};
use auberean_assets::picture::{Picture, Texture};
use auberean_assets::setup::Setup as Ac2Setup;
use auberean_assets::visual::VisualDesc;
use auberean_assets::Context;
use auberean_dat::locate::DatName;
use auberean_dat::DatFile;
use auberean_world::records::{RegionSet, WorldRecords};
use dereth_assets::{Decode, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_render_cpu::pixel_format::PixelFormatId;

/// The player's Asheron's Call 2 files, open.
pub struct Ac2 {
    records: WorldRecords,
}

impl std::fmt::Debug for Ac2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ac2")
    }
}

/// One mesh of an entity, as the look draws it: its vertices, and the triangles of its most
/// detailed level by material.
#[derive(Debug, Clone)]
pub struct MeshPart {
    pub id: u32,
    /// Its materials (material instances).
    pub materials: Vec<u32>,
    pub vertices: Vec<StoredVertex>,
    /// The most detailed level's triangles: the material's place in `materials`, and the indices.
    pub fragments: Vec<(usize, Vec<u16>)>,
}

/// A picture expanded to red, green, blue and alpha, a row at a time from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub texels: Vec<u8>,
}

impl Rgba {
    /// The share of its texels that are clear (alpha under a half).
    #[must_use]
    pub fn clear_share(&self) -> f64 {
        let clear = self
            .texels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|t| t[3] < 128)
            .count();
        clear as f64 / f64::from((self.width * self.height).max(1))
    }
}

impl Ac2 {
    /// The install in `dir`: its portal and first region's cell files, and its high-resolution
    /// pictures when it has them.
    ///
    /// # Errors
    /// A file that will not open, or a portal file with no property list.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let open = |n: DatName| {
            let p = n.in_dir(dir);
            DatFile::open(&p).map_err(|e| format!("{}: {e}", p.display()))
        };
        let portal = open(DatName::Portal)?;
        let cell = open(DatName::Cell(1))?;
        let highres = open(DatName::HighRes).ok();
        let ctx = Context::from_portal(&portal)
            .map_err(|e| format!("the portal file's property list: {e:?}"))?;
        Ok(Self {
            records: WorldRecords::new(portal, cell, highres, ctx),
        })
    }

    /// The region: its terrain kinds, surfaces and roads.
    ///
    /// # Errors
    /// The region will not read.
    pub fn region(&self) -> Result<RegionSet, String> {
        self.records
            .region()
            .map_err(|e| format!("the region: {e:?}"))
    }

    /// The picture a ground material is drawn with first (0 for none).
    #[must_use]
    pub fn ground_picture(&self, material: u32) -> u32 {
        self.records
            .get::<Material>(material)
            .and_then(|m| m.first_texture())
            .map_or(0, |t| t.0)
    }

    /// The entity's meshes at rest: each part group's setup's meshes in order, each with at least
    /// four vertices and a level of detail, at its most detailed level (the first of the most
    /// triangles).
    ///
    /// # Errors
    /// A record on the way will not read.
    pub fn meshes(&self, entity: u32) -> Result<Vec<MeshPart>, String> {
        let e: Entity = self
            .records
            .get(entity)
            .ok_or_else(|| format!("entity {entity:08X} will not read"))?;
        let vd_id = e.object.unwrap_or(0);
        let vd: VisualDesc = self
            .records
            .get(vd_id)
            .ok_or_else(|| format!("entity {entity:08X}: its description {vd_id:08X}"))?;
        let mut out = Vec::new();
        for (_, setup) in vd.setups() {
            let s: Ac2Setup = self
                .records
                .get(setup.0)
                .ok_or_else(|| format!("entity {entity:08X}: setup {:08X}", setup.0))?;
            for mesh in &s.meshes {
                let m: Mesh = self
                    .records
                    .get(mesh.0)
                    .ok_or_else(|| format!("entity {entity:08X}: mesh {:08X}", mesh.0))?;
                let Some(vertices) = m.vertices.attributes() else {
                    continue;
                };
                if m.levels.is_empty() || vertices.len() < 4 {
                    continue;
                }
                let total = |l: &auberean_assets::mesh::DetailLevel| {
                    l.fragments.iter().map(|f| f.indices.len()).sum::<usize>()
                };
                let mut best = &m.levels[0];
                for l in &m.levels[1..] {
                    if total(l) > total(best) {
                        best = l;
                    }
                }
                out.push(MeshPart {
                    id: mesh.0,
                    materials: m.materials.iter().map(|d| d.0).collect(),
                    vertices,
                    fragments: best
                        .fragments
                        .iter()
                        .map(|f| (f.material as usize, f.indices.to_vec()))
                        .collect(),
                });
            }
        }
        Ok(out)
    }

    /// A material instance's diffuse picture: the first its modifiers name, in their order.
    #[must_use]
    pub fn diffuse(&self, instance: u32) -> Option<u32> {
        let mi: MaterialInstance = self.records.get(instance)?;
        mi.modifiers.iter().find_map(|m| {
            self.records
                .get::<MaterialModifier>(m.0)
                .and_then(|md| md.texture(DIFFUSE_MAP))
                .map(|t| t.0)
        })
    }

    /// The largest level of picture list `list` no more than `limit` texels a side, expanded.
    #[must_use]
    pub fn picture(&self, list: u32, limit: u32) -> Option<Rgba> {
        let t: Texture = self.records.get(list)?;
        t.levels.iter().find_map(|level| {
            let p: Picture = self.records.get(level.0)?;
            (p.width.max(p.height) <= limit)
                .then(|| expand(&p))
                .flatten()
        })
    }
}

/// A picture's texels as red, green, blue and alpha: block-compressed pictures and the 24- and
/// 32-bit ones; `None` for any other format.
fn expand(p: &Picture) -> Option<Rgba> {
    let n = (p.width * p.height) as usize;
    let texels = match PixelFormatId::from_raw(p.format) {
        f @ (PixelFormatId::Dxt1
        | PixelFormatId::Dxt2
        | PixelFormatId::Dxt3
        | PixelFormatId::Dxt4
        | PixelFormatId::Dxt5) => {
            let mut bgra = dereth_render_cpu::dxt::decode(f, &p.bytes, p.width, p.height).ok()?;
            for t in bgra.as_chunks_mut::<4>().0 {
                t.swap(0, 2);
            }
            bgra
        }
        PixelFormatId::A8R8G8B8 => {
            let src = p.bytes.get(..n * 4)?;
            src.as_chunks::<4>()
                .0
                .iter()
                .flat_map(|t| [t[2], t[1], t[0], t[3]])
                .collect()
        }
        PixelFormatId::R8G8B8 => {
            let src = p.bytes.get(..n * 3)?;
            src.as_chunks::<3>()
                .0
                .iter()
                .flat_map(|t| [t[2], t[1], t[0], 255])
                .collect()
        }
        _ => return None,
    };
    Some(Rgba {
        width: p.width,
        height: p.height,
        texels,
    })
}

/// A setup's collision: its cylinders (`[x, y, z, radius, height]`, z the base) and its spheres
/// (`[x, y, z, radius]`), in its own frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shape {
    pub cylinders: Vec<[f64; 5]>,
    pub spheres: Vec<[f64; 4]>,
}

/// A stored single-precision number as the decimal it is written as, read back in double
/// precision: the collision's numbers as they were measured.
fn decimal(v: f32) -> f64 {
    format!("{v}").parse().unwrap_or(f64::from(v))
}

/// Setup `id`'s collision from the Asheron's Call files, with only its first `solid` spheres:
/// the ones that collide.
///
/// # Errors
/// The setup will not read.
pub fn collision(store: &RetailDatStore, id: u32, solid: usize) -> Result<Shape, String> {
    let bytes = store
        .read_typed(DbType::Setup, DataId(id))
        .map_err(|e| format!("setup {id:08X}: {e}"))?;
    let s = Setup::decode_payload_in(store.era_of(DataId(id)), DataId(id), &bytes)
        .map_err(|e| format!("setup {id:08X}: {e}"))?;
    Ok(Shape {
        cylinders: s
            .cylspheres
            .iter()
            .map(|c| {
                [
                    decimal(c.low_pt.x),
                    decimal(c.low_pt.y),
                    decimal(c.low_pt.z),
                    decimal(c.radius),
                    decimal(c.height),
                ]
            })
            .collect(),
        spheres: s
            .spheres
            .iter()
            .take(solid)
            .map(|c| {
                [
                    decimal(c.center.x),
                    decimal(c.center.y),
                    decimal(c.center.z),
                    decimal(c.radius),
                ]
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's reading)
    use super::*;

    #[test]
    fn a_collision_number_reads_as_the_decimal_it_was_measured_as() {
        assert_eq!(decimal(1.15), 1.15);
        assert_eq!(decimal(-3.62772), -3.62772);
        assert_eq!(decimal(0.0), 0.0);
    }

    #[test]
    fn a_wide_picture_swaps_its_blue_and_red_and_a_narrow_one_is_opaque() {
        let wide = Picture {
            width: 1,
            height: 1,
            format: 0x15,
            bytes: vec![1, 2, 3, 4],
            ..Picture::default()
        };
        assert_eq!(expand(&wide).expect("expands").texels, [3, 2, 1, 4]);
        let narrow = Picture {
            format: 0x14,
            bytes: vec![1, 2, 3],
            ..wide
        };
        assert_eq!(expand(&narrow).expect("expands").texels, [3, 2, 1, 255]);
        let rgba = Rgba {
            width: 2,
            height: 1,
            texels: vec![0, 0, 0, 0, 0, 0, 0, 255],
        };
        assert!((rgba.clear_share() - 0.5).abs() < 1e-12);
    }
}
