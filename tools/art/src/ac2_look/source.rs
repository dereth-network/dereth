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

/// One level's triangles by material: the material's place in its mesh's materials, and the
/// indices.
pub type Fragments = Vec<(usize, Vec<u16>)>;

/// One mesh of an entity, as the look draws it: its vertices, the triangles of its most detailed
/// level by material, and its coarser levels with the distances Asheron's Call 2 draws them from.
#[derive(Debug, Clone)]
pub struct MeshPart {
    pub id: u32,
    /// Its materials (material instances).
    pub materials: Vec<u32>,
    pub vertices: Vec<StoredVertex>,
    /// The most detailed level's triangles: the material's place in `materials`, and the indices.
    pub fragments: Fragments,
    /// Its coarser levels, each after the one before, and the distance from the eye each is
    /// drawn from at the highest model detail, at the entity's own size.
    pub coarser: Vec<(f64, Fragments)>,
}

/// The highest of the three model details, whose distances and levels the look takes.
const MODEL_DETAIL: usize = 2;

/// The places of a mesh's levels marked for the highest model detail, in their order: all of
/// them when none is marked.
fn marked_levels(levels: &[auberean_assets::mesh::DetailLevel]) -> Vec<usize> {
    let bit = 1u32 << MODEL_DETAIL;
    let marked: Vec<usize> = (0..levels.len())
        .filter(|&k| levels[k].detail & bit != 0)
        .collect();
    if marked.is_empty() {
        (0..levels.len()).collect()
    } else {
        marked
    }
}

/// How far from the eye Asheron's Call 2 draws a part whose setup gives `d` at detail `t` (1 full
/// to 0 least): its detail falls from full at its near distance, through half at its ideal one,
/// to none at its far one.
fn distance_at_detail(d: &auberean_assets::setup::DetailDistances, t: f64) -> f64 {
    let (near, ideal, far) = (
        f64::from(d.near_degrade),
        f64::from(d.ideal_degrade),
        f64::from(d.far_degrade),
    );
    if t >= 0.5 {
        near + (ideal - near) * 2.0 * (1.0 - t)
    } else {
        ideal + (far - ideal) * (1.0 - 2.0 * t)
    }
}

/// The distances from which Asheron's Call 2 draws each of `count` marked levels after the first
/// (`count - 1` of them): level `i` of `count` is drawn once the detail falls to `1 - i / (count -
/// 1)`.
#[must_use]
pub fn level_distances(d: &auberean_assets::setup::DetailDistances, count: usize) -> Vec<f64> {
    if count < 2 {
        return Vec::new();
    }
    let last = (count - 1) as f64;
    (1..count)
        .map(|i| distance_at_detail(d, 1.0 - i as f64 / last))
        .collect()
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

    /// The record `id`, read as a `T`; `None` when it is missing or will not read.
    #[must_use]
    pub fn record<T: auberean_assets::Decode>(&self, id: u32) -> Option<T> {
        self.records.get(id)
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

    /// The picture a ground material is drawn with first (0 for none): a ground's base picture,
    /// or its detail picture.
    #[must_use]
    pub fn ground_picture(&self, material: u32) -> u32 {
        self.records
            .get::<Material>(material)
            .and_then(|m| m.first_texture())
            .map_or(0, |t| t.0)
    }

    /// The entity's meshes at rest: each part group's setup's meshes in order, each with at least
    /// four vertices and a level of detail, at its most detailed level (the first of the most
    /// triangles), with the levels marked for the highest model detail that come after it and the
    /// distances its setup draws them from.
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
                let mut best = 0;
                for (k, l) in m.levels.iter().enumerate().skip(1) {
                    if total(l) > total(&m.levels[best]) {
                        best = k;
                    }
                }
                let fragments = |l: &auberean_assets::mesh::DetailLevel| -> Fragments {
                    l.fragments
                        .iter()
                        .map(|f| (f.material as usize, f.indices.to_vec()))
                        .collect()
                };
                // The marked levels after the most detailed, from where it stands among them.
                let marked = marked_levels(&m.levels);
                let from = marked.iter().position(|k| *k == best).unwrap_or(0);
                let distances = level_distances(&s.detail[MODEL_DETAIL], marked.len());
                let coarser = marked
                    .iter()
                    .enumerate()
                    .skip(from + 1)
                    .filter_map(|(i, k)| {
                        let l = &m.levels[*k];
                        (total(l) > 0).then(|| (distances[i - 1], fragments(l)))
                    })
                    .collect();
                out.push(MeshPart {
                    id: mesh.0,
                    materials: m.materials.iter().map(|d| d.0).collect(),
                    vertices,
                    fragments: fragments(&m.levels[best]),
                    coarser,
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

    /// The sides of each level of picture list `list`, in its order; empty when it will not read.
    #[must_use]
    pub fn levels(&self, list: u32) -> Vec<(u32, u32)> {
        let Some(t) = self.records.get::<Texture>(list) else {
            return Vec::new();
        };
        t.levels
            .iter()
            .filter_map(|l| self.records.get::<Picture>(l.0))
            .map(|p| (p.width, p.height))
            .collect()
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

    /// Every picture the look takes from this machine's AC2 install (each ground's, its detail
    /// picture, the road's, every tree, rock and cover's, the clouds) is its list's largest level.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn every_picture_the_look_takes_is_its_largest_level() {
        let t = super::super::tables::Tables::built_in().expect("the tables");
        let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("opens");
        let region = ac2.region().expect("the region");
        let mut lists = std::collections::BTreeSet::new();
        let mut group = |g: &auberean_assets::region::MaterialGroup| {
            for m in g.base.iter().chain(&g.detail) {
                lists.insert(ac2.ground_picture(m.material.0));
            }
        };
        for g in &t.terrain.ground {
            let kind = region.terrain.terrains.iter().find(|k| k.index == g.kind);
            let surface = kind.and_then(|k| {
                region
                    .surface
                    .surfaces
                    .iter()
                    .find(|s| s.index == k.surface)
            });
            for grp in &surface.expect("the kind's surface").groups {
                group(grp);
            }
        }
        for r in &region.surface.roads {
            for grp in &r.groups {
                group(grp);
            }
        }
        let entities = t
            .trees
            .model
            .iter()
            .chain(&t.rocks.model)
            .map(|m| m.entity)
            .chain(t.trees.cover.iter().map(|c| c.entity));
        for e in entities {
            for part in ac2.meshes(e).expect("the entity's meshes") {
                lists.extend(part.materials.iter().filter_map(|mi| ac2.diffuse(*mi)));
            }
        }
        lists.insert(t.sky.clouds);
        lists.remove(&0);
        assert!(lists.len() > 70, "{}", lists.len());
        for list in lists {
            let levels = ac2.levels(list);
            let largest = levels.iter().map(|(w, h)| w * h).max().expect("a level");
            let p = ac2
                .picture(list, t.terrain.picture_limit)
                .unwrap_or_else(|| panic!("{list:08X} gives no picture"));
            assert_eq!(p.width * p.height, largest, "{list:08X}: {levels:?}");
        }
    }

    /// A part's levels come in where Asheron's Call 2's detail reaches them: the second of five
    /// halfway from the near distance to the ideal one, the middle at the ideal, the fourth
    /// halfway on to the far, and the last at the far.
    #[test]
    fn a_parts_levels_come_in_where_its_detail_reaches_them() {
        let d = auberean_assets::setup::DetailDistances {
            visible: 400.0,
            near_degrade: 20.0,
            ideal_degrade: 60.0,
            far_degrade: 160.0,
            high_detail: 0.0,
            medium_detail: 0.0,
        };
        assert_eq!(level_distances(&d, 5), vec![40.0, 60.0, 110.0, 160.0]);
        assert_eq!(level_distances(&d, 2), vec![160.0]);
        assert!(level_distances(&d, 1).is_empty());
    }

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
