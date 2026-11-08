//! The art pack as the remaster draws it: the pack the application opened, its switches, and the
//! pack's models, surfaces and pictures made as the remaster's entries ([`PackAssets`]).
//!
//! The remaster names the pack's entries past every retail id
//! ([`dereth_remaster::assets::pack`]). This module answers for those names over the opened pack:
//! - a model's levels drawn into the view become meshes: their triangles in glTF's order, which
//!   turns as retail's does, each part a surface of its own, each vertex carrying its color times
//!   its part's color factor;
//! - a part's material becomes a surface: its base color picture, its cut, both its faces where
//!   it is foliage or two-sided, drawn under its vertices' colors;
//! - a picture becomes a texture. A cut-out picture is made again from its file when the pack is a
//!   folder, its cut-away texels filled from its edge first
//!   ([`dereth_remaster::assets::pack::fill_cut_away`]), on a thread of its own from when the pack
//!   is given, and kept; any other picture is taken as the pack made it.
//!
//! A retail surface whose picture the pack draws a material for takes the material's picture,
//! while the materials' switch is on.
//!
//! Roughness, occlusion and metalness reach the remaster in glTF's layout, one picture for the
//! three: occlusion in red, roughness in green, metalness in blue. A model's metal-roughness
//! picture is in it already. A material's or a ground's surface picture, which the pack's loader
//! makes with the roughness in red and the occlusion in green, is turned to it as it is made
//! ([`orm_of_surface`]).

use glam::{Vec2, Vec3};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::art_pack::images::{self, BlockFormat};
use crate::art_pack::{ArtPack, Image};
use dereth_primitives::{TextureData, TextureFormat};
use dereth_remaster::assets::pack::{
    fill_cut_away, of_pack_mesh, of_pack_picture, of_pack_surface, pack_mesh, pack_picture,
    pack_surface, Category, PackLevel, PackModel, PackSettings,
};
use dereth_remaster::assets::terrain::{GroundMaps, LayerSlopes, SplatBank, SteepTile};
use dereth_remaster::assets::{
    kind::Kind, BlendFactor, Material, MaterialMaps, Mesh, SubsetInfo, SurfaceRef, TextureKey,
    Vertex,
};
use dereth_remaster::look::{Layer, LookToml};
use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

/// What of the art pack the remaster holds, against what its swaps drew.
pub use dereth_remaster::assets::pack::PackCounts as RemasterPackCounts;
/// What of the art pack the remaster draws.
pub use dereth_remaster::assets::pack::PackSettings as RemasterPacks;
/// What the art pack's far forest did in the remaster.
pub use dereth_remaster::render::forest::ForestStats as RemasterForestStats;

/// Where the pictures of the pack's materials for retail pictures are named from, after its
/// models' pictures; their normal pictures; and their roughness and occlusion.
const MATERIAL_PICTURES: u32 = 0x80_0000;
const MATERIAL_NORMALS: u32 = 0xC0_0000;
const MATERIAL_SURFACES: u32 = 0xE0_0000;
/// Where the pictures of the pack's grounds are named from; their normal pictures; and their
/// roughness and occlusion.
const GROUND_PICTURES: u32 = 0xA0_0000;
const GROUND_NORMALS: u32 = 0xA8_0000;
const GROUND_SURFACES: u32 = 0xB0_0000;

/// A pack picture as a texture key.
fn picture_key(picture: u32) -> TextureKey {
    TextureKey::Dat {
        picture: pack_picture(picture),
        palette: 0,
    }
}

/// Where the remaster reads the pack's far pictures from: each far model's color and normal
/// pictures, by their place in the pack.
#[derive(Debug)]
struct PackFarPictures {
    pack: Arc<ArtPack>,
    images: Vec<[u32; 2]>,
}

impl dereth_remaster::scene::FarPictures for PackFarPictures {
    fn pictures(&self, model: usize) -> Option<[TextureData; 2]> {
        let [base, normal] = *self.images.get(model)?;
        Some([
            texture_of(self.pack.images.get(base as usize)?)?,
            texture_of(self.pack.images.get(normal as usize)?)?,
        ])
    }
}

/// The opened pack, as the remaster's entries are made from it.
#[derive(Debug)]
pub(crate) struct PackAssets {
    pack: Arc<ArtPack>,
    /// The folder it was opened from, when it is one: its cut-out pictures are made again from
    /// their files.
    folder: Option<PathBuf>,
    /// Each model's place in the pack, by the retail object it stands for.
    by_id: HashMap<u32, u32>,
    /// Each material's place in the pack, by the retail picture it stands for.
    by_texture: HashMap<u32, u32>,
    /// Each model's levels drawn into the view, by their place among its levels.
    view_levels: Vec<Vec<usize>>,
    /// The cut-out pictures made again, by their place in the pack: made on a thread of their own
    /// from when the pack is given, and here whenever an entry needs one first.
    filled: Arc<Mutex<HashMap<u32, Arc<Image>>>>,
}

impl PackAssets {
    /// The pack `pack`, opened from `folder` when it was a folder.
    pub(crate) fn new(pack: Arc<ArtPack>, folder: Option<PathBuf>) -> Self {
        let folder = folder.filter(|f| f.is_dir());
        let by_id = pack
            .models
            .iter()
            .zip(0u32..)
            .map(|(m, i)| (m.id.0, i))
            .collect();
        let by_texture = pack
            .materials
            .iter()
            .zip(0u32..)
            .flat_map(|(m, i)| m.textures.iter().map(move |t| (t.0, i)))
            .collect();
        let view_levels = pack
            .models
            .iter()
            .map(|m| {
                m.lods
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| !l.shadows_only && l.views.is_none())
                    .map(|(k, _)| k)
                    .collect()
            })
            .collect();
        let filled = Arc::new(Mutex::new(HashMap::new()));
        if let Some(folder) = folder.clone() {
            // Every cut-out picture made again ahead of the frames that will want it.
            let (pack, filled) = (Arc::clone(&pack), Arc::clone(&filled));
            let made = std::thread::Builder::new()
                .name("remaster pack pictures".to_owned())
                .spawn(move || {
                    for (k, s) in (0u32..).zip(&pack.image_sources) {
                        if s.cutoff.is_none() || s.views {
                            continue;
                        }
                        let have = filled.lock().map_or(
                            true,
                            |f: std::sync::MutexGuard<'_, HashMap<u32, Arc<Image>>>| {
                                f.contains_key(&k)
                            },
                        );
                        if have {
                            continue;
                        }
                        if let Ok(image) = Self::filled(&folder, &s.path, s.srgb, s.cutoff) {
                            if let Ok(mut f) = filled.lock() {
                                f.entry(k).or_insert_with(|| Arc::new(image));
                            }
                        }
                    }
                });
            if let Err(e) = made {
                tracing::warn!(target: "dereth::remaster", "the pack's pictures are made as they are wanted: {e}");
            }
        }
        Self {
            pack,
            folder,
            by_id,
            by_texture,
            view_levels,
            filled,
        }
    }

    /// The opened pack.
    pub(crate) fn pack(&self) -> &Arc<ArtPack> {
        &self.pack
    }

    /// The pack's far pictures: of each model, the last level of pictures with a quad to draw and
    /// both its pictures.
    pub(crate) fn far_pack(&self) -> dereth_remaster::scene::FarPack {
        let mut models = Vec::new();
        let mut images = Vec::new();
        for m in &self.pack.models {
            let level = m.lods.iter().rev().find_map(|l| {
                let p = l.primitives.first()?;
                let views = l.views?;
                (p.indices.len() == 6 && !p.vertices.is_empty()).then_some((p, views))
            });
            let Some((p, views)) = level else {
                continue;
            };
            let (Some(base), Some(normal)) = (p.material.base_color_image, p.material.normal_image)
            else {
                continue;
            };
            let (mut half, mut foot, mut top) = (0.0f32, f32::MAX, f32::MIN);
            for v in p
                .vertices
                .as_chunks::<{ crate::art_pack::VERTEX_FLOATS }>()
                .0
            {
                half = half.max(v[0].abs());
                foot = foot.min(v[2]);
                top = top.max(v[2]);
            }
            let count = views.count.max(1);
            models.push(dereth_remaster::scene::FarPackModel {
                id: m.id.0,
                bounds: (glam::Vec3::from_array(m.bounds.0), m.bounds.1),
                quad: [half, foot.min(top), top.max(foot)],
                views: [count, views.columns.clamp(1, count)],
                color: display_color(p.material.base_color),
                cut: p.material.alpha_cutoff.unwrap_or(0.0),
            });
            images.push([base, normal]);
        }
        dereth_remaster::scene::FarPack {
            serial: self.pack.serial,
            models,
            pictures: Arc::new(PackFarPictures {
                pack: Arc::clone(&self.pack),
                images,
            }),
        }
    }

    /// The pack's model for the object `id`, when its kind is switched on.
    pub(crate) fn model(&self, id: u32, on: PackSettings) -> Option<PackModel> {
        let index = *self.by_id.get(&id)?;
        let m = self.pack.models.get(index as usize)?;
        let category = match m.category {
            crate::art_pack::Category::Tree => Category::Tree,
            crate::art_pack::Category::Plant => Category::Plant,
            crate::art_pack::Category::Rock => Category::Rock,
            crate::art_pack::Category::Water => Category::Water,
        };
        if !on.models(category) {
            return None;
        }
        let levels: Vec<PackLevel> = self.view_levels[index as usize]
            .iter()
            .enumerate()
            .filter_map(|(k, l)| {
                Some(PackLevel {
                    mesh: pack_mesh(index, u8::try_from(k).ok()?),
                    distance: m.lods[*l].distance,
                })
            })
            .collect();
        (!levels.is_empty()).then_some(PackModel { category, levels })
    }

    /// The mesh a pack mesh id names: its level's parts, each a surface of its own, their
    /// triangles in glTF's order.
    pub(crate) fn mesh(&self, id: u32) -> Option<Mesh> {
        let (index, level) = of_pack_mesh(id)?;
        let model = self.pack.models.get(index as usize)?;
        let lod = &model.lods[*self.view_levels[index as usize].get(usize::from(level))?];
        let floats = crate::art_pack::VERTEX_FLOATS;
        let lists = lod
            .primitives
            .iter()
            .enumerate()
            .filter_map(|(k, p)| {
                let part = u8::try_from(k).ok()?;
                // The part's color factor where a picture is drawn under it: with no picture the
                // surface's one color is the factor.
                let factor = if p.material.base_color_image.is_some() {
                    p.material.base_color
                } else {
                    [1.0; 4]
                };
                let vertex = |i: u32| {
                    let v = p
                        .vertices
                        .get(i as usize * floats..(i as usize + 1) * floats)?;
                    let vertex = Vertex {
                        position: [v[0], v[1], v[2]],
                        normal: [v[3], v[4], v[5]],
                        uv: [v[10], v[11]],
                    };
                    let color = [
                        v[12] * factor[0],
                        v[13] * factor[1],
                        v[14] * factor[2],
                        v[15] * factor[3],
                    ];
                    Some((vertex, display_color(color)))
                };
                // Counter-clockwise seen from the front, as glTF has them and as retail's own
                // triangles turn: kept in their order.
                let mut triangles = Vec::with_capacity(p.indices.len());
                for t in p.indices.as_chunks::<3>().0 {
                    triangles.extend([vertex(t[0])?, vertex(t[1])?, vertex(t[2])?]);
                }
                let two_sided =
                    p.material.double_sided || (model.foliage && p.material.alpha_cutoff.is_some());
                Some((
                    SubsetInfo {
                        surface: SurfaceRef::Surface(pack_surface(index, level, part)),
                        two_sided,
                        tiled: true,
                    },
                    triangles,
                ))
            })
            .collect::<Vec<_>>();
        let mut mesh = Mesh::from_colored_triangle_lists(lists);
        let (centre, radius) = model.bounds;
        mesh.sphere = Some((centre, radius));
        mesh.sort_center = centre;
        // How deep in the crown each vertex lies goes in the length of its tangent's last value,
        // whose sign is all anything else reads of it.
        if let Some(crown) = Crown::of_model(model) {
            let mut tangents = dereth_remaster::assets::tangents(&mesh);
            for (t, v) in tangents.iter_mut().zip(&mesh.vertices) {
                t[3] *= 1.0 + crown.deep(Vec3::from(v.position));
            }
            mesh.tangents = tangents;
        }
        Some(mesh)
    }

    /// The surface a pack surface id names: its part's base color picture (or its color where it
    /// has none), its cut, drawn as the model's kind.
    pub(crate) fn material(&self, id: u32) -> Option<Material> {
        let (index, level, part) = of_pack_surface(id)?;
        let model = self.pack.models.get(index as usize)?;
        let lod = &model.lods[*self.view_levels[index as usize].get(usize::from(level))?];
        let m = lod.primitives.get(usize::from(part))?.material;
        let texture = match m.base_color_image {
            Some(i) => TextureKey::Dat {
                picture: pack_picture(i),
                palette: 0,
            },
            None => TextureKey::Solid(argb(display_color(m.base_color))),
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // A cut between 0 and 1, to its byte.
        let alpha_test = m
            .alpha_cutoff
            .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        Some(Material {
            kind: if model.foliage && m.alpha_cutoff.is_some() {
                Kind::Foliage
            } else {
                Kind::Architecture
            },
            surface_type: 0,
            data_category: None,
            texture: Some(texture),
            clip_map: false,
            alpha_test,
            vertex_alpha: 255,
            vertex_colored: true,
            translucent: None,
            additive: false,
            blended: false,
            src_blend: BlendFactor::One,
            dst_blend: BlendFactor::Zero,
            z_write: true,
            fogged: true,
            solid: m
                .base_color_image
                .is_none()
                .then(|| argb(display_color(m.base_color))),
            luminosity: 0.0,
            diffuse: 1.0,
            // Its normal picture and its metal-roughness picture (glTF's: the roughness in its
            // green), where it has them, and its roughness factor, where it is not the roughest.
            maps: (m.normal_image.is_some() || m.metal_rough_image.is_some() || m.roughness < 1.0)
                .then(|| MaterialMaps {
                    normal: m.normal_image.map(picture_key),
                    orm: m.metal_rough_image.map(picture_key),
                    normal_scale: m.normal_scale,
                    roughness: m.roughness,
                }),
        })
    }

    /// The picture the pack draws in place of the retail picture (texture record) `texture`,
    /// when it has a material for it; the material's normal picture (bent already by its
    /// strength); and its roughness and occlusion, in glTF's layout ([`orm_of_surface`]).
    pub(crate) fn material_picture(&self, texture: u32) -> Option<(TextureKey, MaterialMaps)> {
        let i = self.by_texture.get(&texture)?;
        Some((
            picture_key(MATERIAL_PICTURES + i),
            MaterialMaps {
                normal: Some(picture_key(MATERIAL_NORMALS + i)),
                orm: Some(picture_key(MATERIAL_SURFACES + i)),
                normal_scale: 1.0,
                roughness: 1.0,
            },
        ))
    }

    /// The pack's tile for the retail ground `ground` (a terrain type, or the road's), and how
    /// many times it repeats across a ground cell, when the pack lays that ground.
    pub(crate) fn ground_tile(&self, ground: u32) -> Option<(TextureKey, u32)> {
        let i = self.ground_of(ground)?;
        let size = self.pack.grounds[i].size.max(0.01);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // A cell's repeats of a picture of a few metres, a small whole number.
        let repeats = (dereth_terrain::consts::CELL_SIZE / size)
            .round()
            .clamp(1.0, 64.0) as u32;
        Some((
            picture_key(GROUND_PICTURES + u32::try_from(i).ok()?),
            repeats,
        ))
    }

    /// The normal and surface pictures of the pack's tile for the retail ground `ground`, laid
    /// as [`Self::ground_tile`]'s picture is.
    pub(crate) fn ground_maps(&self, ground: u32) -> Option<GroundMaps> {
        let i = self.ground_of(ground)?;
        let at = u32::try_from(i).ok()?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // A strength of a few at most, in thousandths.
        let normal_scale = (self.pack.grounds[i].normal_scale.max(0.0) * 1000.0).round() as u32;
        Some(GroundMaps {
            normal: Some(picture_key(GROUND_NORMALS + at)),
            surface: Some(picture_key(GROUND_SURFACES + at)),
            normal_scale,
        })
    }

    /// The pack's ground laid on the flat of the retail ground `ground` (a terrain type, or the
    /// road's), when it lays that ground.
    fn ground_of(&self, ground: u32) -> Option<usize> {
        let g = if ground == dereth_terrain::consts::ROAD_TERRAIN_TYPE {
            crate::art_pack::Ground::Road
        } else {
            crate::art_pack::Ground::Type(u8::try_from(ground).ok().filter(|t| *t < 32)?)
        };
        self.pack
            .grounds
            .iter()
            .position(|p| !p.steep && p.grounds.contains(&g))
    }

    /// What the pack lays by the water in a ground cell of the terrain types `grounds`: over a
    /// muddy bank the first of them neither water nor mud nor marsh, or the grass the knobs name;
    /// in place of retail's water the first of them that is not water, or the bed; and the bed the
    /// knobs name. `None` where the pack lays neither the grass nor the bed.
    pub(crate) fn bank_of(&self, grounds: &[u32]) -> Option<SplatBank> {
        use dereth_remaster::assets::kind::{is_mud_type, is_water_type};
        use dereth_terrain::land::drawn::{Knob, Knobs};
        let knobs = Knobs::default();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a terrain type
        let (grass, bed) = (
            knobs.get(Knob::BankGrass) as u32,
            knobs.get(Knob::WaterBed) as u32,
        );
        let laid = |g: u32| self.ground_of(g).and_then(|i| self.steep_tile(i, false));
        let beside = |keep: &dyn Fn(u32) -> bool| {
            grounds
                .iter()
                .copied()
                .find(|g| keep(*g) && self.ground_of(*g).is_some())
        };
        let land = beside(&|g| !is_water_type(g));
        Some(SplatBank {
            grass: laid(beside(&|g| !is_water_type(g) && !is_mud_type(g)).unwrap_or(grass))?,
            land: laid(land.unwrap_or(bed))?,
            land_is_mud: land.is_some_and(is_mud_type),
            bed: laid(bed)?,
        })
    }

    /// The pack's ground `i` as a picture laid on steep ground; `own` when it is a ground's own
    /// material for its slopes.
    fn steep_tile(&self, i: usize, own: bool) -> Option<SteepTile> {
        let at = u32::try_from(i).ok()?;
        let g = self.pack.grounds.get(i)?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // A cell's repeats of a picture of a few metres, and a strength of a few at most.
        let (tiling, normal_scale) = (
            (dereth_terrain::consts::CELL_SIZE / g.size.max(0.01))
                .round()
                .clamp(1.0, 64.0) as u32,
            (g.normal_scale.max(0.0) * 1000.0).round() as u32,
        );
        Some(SteepTile {
            tile: picture_key(GROUND_PICTURES + at),
            tiling,
            maps: Some(GroundMaps {
                normal: Some(picture_key(GROUND_NORMALS + at)),
                surface: Some(picture_key(GROUND_SURFACES + at)),
                normal_scale,
            }),
            own,
        })
    }

    /// What the pack lays on the steep ground of the retail ground `ground`: its own material for
    /// that ground's slopes where it has one, otherwise the generic scree and rock; nothing where
    /// the pack does not lay that ground.
    pub(crate) fn slopes_of(&self, ground: u32) -> LayerSlopes {
        if self.ground_of(ground).is_none() {
            return LayerSlopes::default();
        }
        let of_type = u8::try_from(ground).ok().map(crate::art_pack::Ground::Type);
        let own = of_type.and_then(|g| {
            self.pack
                .grounds
                .iter()
                .position(|p| p.steep && p.grounds.contains(&g))
        });
        let knobs = dereth_terrain::land::drawn::Knobs::default();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a terrain type
        let generic = |k| self.ground_of(knobs.get(k) as u32);
        let laid = |generic_ground: Option<usize>| match own {
            Some(i) => self.steep_tile(i, true),
            None => generic_ground.and_then(|i| self.steep_tile(i, false)),
        };
        LayerSlopes {
            scree: laid(generic(dereth_terrain::land::drawn::Knob::ScreeGround)),
            rock: laid(generic(dereth_terrain::land::drawn::Knob::RockGround)),
            snow: matches!(ground, 2 | 15 | 27),
        }
    }

    /// The texture a pack picture id names.
    pub(crate) fn texture(&self, id: u32) -> Option<TextureData> {
        let k = of_pack_picture(id)?;
        if k >= MATERIAL_SURFACES {
            let m = self.pack.materials.get((k - MATERIAL_SURFACES) as usize)?;
            return texture_of(&orm_of_surface(&m.surface));
        }
        if k >= MATERIAL_NORMALS {
            let m = self.pack.materials.get((k - MATERIAL_NORMALS) as usize)?;
            return texture_of(&m.normal);
        }
        if k >= GROUND_SURFACES {
            let g = self.pack.grounds.get((k - GROUND_SURFACES) as usize)?;
            return texture_of(&orm_of_surface(&g.surface));
        }
        if k >= GROUND_NORMALS {
            let g = self.pack.grounds.get((k - GROUND_NORMALS) as usize)?;
            return texture_of(&g.normal);
        }
        if k >= GROUND_PICTURES {
            let g = self.pack.grounds.get((k - GROUND_PICTURES) as usize)?;
            return texture_of(&g.albedo);
        }
        if k >= MATERIAL_PICTURES {
            let m = self.pack.materials.get((k - MATERIAL_PICTURES) as usize)?;
            return texture_of(&m.albedo);
        }
        let made = self.pack.images.get(k as usize)?;
        let source = self.pack.image_sources.get(k as usize);
        match (source, self.folder.as_ref()) {
            (Some(s), Some(folder)) if s.cutoff.is_some() && !s.views => {
                // A cut-out picture made again from its file, its cut-away texels filled first:
                // as the thread made it, or here when it has not yet.
                let ready = self.filled.lock().ok().and_then(|f| f.get(&k).cloned());
                if let Some(image) = ready {
                    return texture_of(&image);
                }
                match Self::filled(folder, &s.path, s.srgb, s.cutoff) {
                    Ok(image) => {
                        let image = Arc::new(image);
                        if let Ok(mut f) = self.filled.lock() {
                            f.insert(k, Arc::clone(&image));
                        }
                        texture_of(&image)
                    }
                    Err(e) => {
                        tracing::warn!(target: "dereth::remaster", "{e}; drawn as the pack made it");
                        texture_of(made)
                    }
                }
            }
            _ => texture_of(made),
        }
    }

    /// The cut-out picture at `path` in `folder`, its cut-away texels filled from its edge,
    /// stretched to whole blocks, given its levels and compressed, as the pack makes its
    /// pictures.
    fn filled(
        folder: &std::path::Path,
        path: &str,
        srgb: bool,
        cutoff: Option<f32>,
    ) -> Result<Image, String> {
        let bytes = std::fs::read(folder.join(path)).map_err(|e| format!("{path}: {e}"))?;
        let (w, h, rgba) = images::decode_png(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let (w, h, mut rgba) = images::to_whole_blocks(w, h, rgba);
        if let Some(c) = cutoff {
            fill_cut_away(&mut rgba, w, h, c);
        }
        Ok(images::compressed(
            images::with_levels(w, h, rgba, srgb, cutoff),
            BlockFormat::Bc7,
        ))
    }

    /// The look the pack's `[lighting]` section says, when it has one that reads.
    fn lighting(&self) -> Option<LookToml> {
        let text = self.pack.lighting.as_deref()?;
        match LookToml::parse(text) {
            Ok(look) => Some(look),
            Err(e) => {
                tracing::warn!(target: "dereth::remaster", "the art pack's lighting is left out: {e}");
                None
            }
        }
    }
}

/// A pack's color, its red, green and blue in linear light as glTF has them, in the display values
/// the remaster multiplies a picture's texels by; its alpha as it is. The curve is a power near
/// enough that a linear picture times a linear color and the display values' product agree.
fn display_color(c: [f32; 4]) -> [f32; 4] {
    let display = |v: f32| {
        if v <= 0.003_130_8 {
            v.max(0.0) * 12.92
        } else {
            1.055 * dereth_primitives::num::math::powf(v, 1.0 / 2.4) - 0.055
        }
    };
    [display(c[0]), display(c[1]), display(c[2]), c[3]]
}

/// `[r, g, b, a]` (0 to 1) as `0xAARRGGBB`.
fn argb(c: [f32; 4]) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // A color part between 0 and 1, to its byte.
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(c[3]) << 24 | byte(c[0]) << 16 | byte(c[1]) << 8 | byte(c[2])
}

/// A pack picture as the remaster's texture: its blocks as they are, or its texels in blue,
/// green, red, alpha order.
fn texture_of(image: &Image) -> Option<TextureData> {
    let format = match image.format {
        Some(BlockFormat::Bc7) => TextureFormat::Bc7,
        Some(BlockFormat::Bc5) => TextureFormat::Bc5,
        Some(BlockFormat::Bc4) => return None,
        None => TextureFormat::Bgra8,
    };
    let levels = image
        .levels
        .iter()
        .map(|l| {
            let mut l = l.as_ref().clone();
            if format == TextureFormat::Bgra8 {
                for t in l.as_chunks_mut::<4>().0 {
                    t.swap(0, 2);
                }
            }
            l
        })
        .collect();
    Some(TextureData {
        width: image.width,
        height: image.height,
        format,
        levels,
    })
}

/// A pack's surface picture (the roughness in red and the occlusion in green, as the pack's
/// loader makes a material's or a ground's) in glTF's layout: the occlusion in red, the roughness
/// in green, and no metalness in blue. A block picture holds the two parts as two blocks of eight
/// bytes, red's first, so swapping them swaps the parts exactly; blue reads as nothing.
pub(crate) fn orm_of_surface(surface: &Image) -> Image {
    let levels = surface
        .levels
        .iter()
        .map(|l| {
            let mut l = l.as_ref().clone();
            match surface.format {
                Some(BlockFormat::Bc5) => {
                    for block in l.as_chunks_mut::<16>().0 {
                        let (red, green) = block.split_at_mut(8);
                        red.swap_with_slice(green);
                    }
                }
                Some(_) => {}
                None => {
                    for t in l.as_chunks_mut::<4>().0 {
                        *t = [t[1], t[0], 0, t[3]];
                    }
                }
            }
            Arc::new(l)
        })
        .collect();
    Image {
        levels,
        ..surface.clone()
    }
}

impl SceneDraw {
    /// The art pack the application opened (`None` for none), and the folder it was opened from:
    /// the remaster draws what of it its switches say, from when it is next turned on.
    pub fn set_remaster_pack(&mut self, pack: Option<Arc<ArtPack>>, folder: Option<PathBuf>) {
        let view = self.remaster_view.get_mut();
        view.pack = pack.map(|p| Arc::new(PackAssets::new(p, folder)));
        let lighting = view
            .pack
            .as_ref()
            .filter(|_| view.packs.any())
            .and_then(|p| p.lighting());
        view.looks.set(Layer::ArtPack, lighting);
    }

    /// What of the art pack the remaster draws. A change remakes the remaster's scene, so every
    /// entry is made again as the switches now say; the pack's lighting is laid over retail's
    /// light while any of it is drawn.
    pub fn set_remaster_packs(&mut self, packs: RemasterPacks, gpu: &mut Gpu) {
        let view = self.remaster_view.get_mut();
        if view.packs == packs {
            return;
        }
        view.packs = packs;
        let store = view.store.clone();
        let lighting = view
            .pack
            .as_ref()
            .filter(|_| packs.any())
            .and_then(|p| p.lighting());
        if view.looks.set(Layer::ArtPack, lighting) {
            let looks = view.looks.clone();
            if let Some(r) = remaster(gpu) {
                r.renderer().set_looks(looks);
            }
        }
        if let (true, Some(store)) = (self.remaster_on(), store) {
            let restart = self
                .set_remaster(false, &store, gpu)
                .and_then(|()| self.set_remaster(true, &store, gpu));
            if let Err(e) = restart {
                tracing::warn!(target: "dereth::remaster", "the remaster did not start again: {e:?}");
            }
        }
    }

    /// What of the art pack the remaster draws.
    #[must_use]
    pub fn remaster_packs(&self) -> RemasterPacks {
        self.remaster_view.borrow().packs
    }

    /// What of the art pack the remaster holds, against what its swaps drew; `None` while the
    /// remaster is off.
    #[must_use]
    pub fn remaster_pack_counts(&self, gpu: &mut Gpu) -> Option<RemasterPackCounts> {
        Some(remaster(gpu)?.renderer().pack_counts())
    }

    /// The art pack's name, when the remaster has one to draw.
    #[must_use]
    pub fn remaster_pack_name(&self) -> Option<String> {
        self.remaster_view
            .borrow()
            .pack
            .as_ref()
            .map(|p| p.pack.name.clone())
    }
}

/// How many heights a crown's reach is kept at.
const CROWN_BANDS: usize = 8;

/// The shape of a plant's crown, from its leaves, in the model's own space (height up): the axis it
/// stands round, the heights of its lowest and highest leaves, and how far the leaves reach from
/// the axis in each of [`CROWN_BANDS`] bands of height evenly between.
///
/// A leaf near the axis, or under a part of the crown wider than its own reach, is deep in the
/// crown: it sees little of the sky between the leaves round it, and little of the sun.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Crown {
    axis: Vec2,
    base: f32,
    top: f32,
    reach: [f32; CROWN_BANDS],
}

impl Crown {
    /// The crown of leaves whose corners are `points`; `None` for fewer than a handful, or for
    /// leaves that stand no higher than a hand's breadth.
    fn of(points: &[Vec3]) -> Option<Self> {
        if points.len() < 8 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)] // a model's vertex count
        let n = points.len() as f32;
        let axis = points.iter().fold(Vec2::ZERO, |a, p| a + p.truncate()) / n;
        let (base, top) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.z), hi.max(p.z))
        });
        if top - base < 0.1 {
            return None;
        }
        let mut bands: [Vec<f32>; CROWN_BANDS] = Default::default();
        for p in points {
            // LINT-OK: a height within the crown, scaled to a band and floored.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let band = (((p.z - base) / (top - base)) * CROWN_BANDS as f32) as usize;
            bands[band.min(CROWN_BANDS - 1)].push((p.truncate() - axis).length());
        }
        // Nine in ten of a band's corners lie within its reach; a band without leaves takes the
        // reach of the nearest bands that have them.
        let held: [Option<f32>; CROWN_BANDS] = std::array::from_fn(|i| {
            let band = &mut bands[i];
            band.sort_by(f32::total_cmp);
            band.get(band.len() * 9 / 10)
                .or_else(|| band.last())
                .copied()
        });
        let reach = std::array::from_fn(|i| {
            held[i].unwrap_or_else(|| {
                let below = held[..i].iter().rev().find_map(|r| *r);
                let above = held[i..].iter().find_map(|r| *r);
                match (below, above) {
                    (Some(a), Some(b)) => (a + b) * 0.5,
                    (Some(r), None) | (None, Some(r)) => r,
                    (None, None) => 0.0,
                }
            })
        });
        Some(Self {
            axis,
            base,
            top,
            reach,
        })
    }

    /// The crown of `model`'s cut-out leaves at its finest level; `None` for a model that is not
    /// cut-out growth.
    fn of_model(model: &crate::art_pack::PackModel) -> Option<Self> {
        if !model.foliage {
            return None;
        }
        let floats = crate::art_pack::VERTEX_FLOATS;
        let points: Vec<Vec3> = model
            .lods
            .first()?
            .primitives
            .iter()
            .filter(|p| p.material.alpha_cutoff.is_some())
            .flat_map(|p| p.vertices.chunks_exact(floats))
            .map(|v| Vec3::new(v[0], v[1], v[2]))
            .collect();
        Self::of(&points)
    }

    /// How deep in the crown a point in the model's own space lies, 0 on the outside to 1 deep
    /// inside: near the axis for its height, or under a part of the crown that reaches farther out.
    fn deep(&self, p: Vec3) -> f32 {
        let r = (p.truncate() - self.axis).length();
        #[allow(clippy::cast_precision_loss)]
        let last = (CROWN_BANDS - 1) as f32;
        #[allow(clippy::cast_precision_loss)]
        let t = ((p.z - self.base) / (self.top - self.base) * CROWN_BANDS as f32 - 0.5)
            .clamp(0.0, last);
        // LINT-OK: floored within the bands.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = t as usize;
        let next = (k + 1).min(CROWN_BANDS - 1);
        let reach = self.reach[k] + (self.reach[next] - self.reach[k]) * (t - t.floor());
        let inward = (1.0 - r / reach.max(0.05)).clamp(0.0, 1.0);
        let over = self.reach[k + 1..].iter().fold(0.0f32, |a, b| a.max(*b));
        let under = ((over - r) / over.max(0.05)).clamp(0.0, 1.0);
        inward.max(under * 0.7)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A crown of leaves in a cone, wide at the foot: a leaf at its axis lies deep, one at its rim
    /// lies out in the open, and one under the wide part from higher up is in its shade, where a
    /// leaf as far out with nothing over it is not.
    #[test]
    fn a_leaf_lies_deep_near_a_crowns_axis_and_under_its_wide_part() {
        let mut points = Vec::new();
        for z in 0..8 {
            #[allow(clippy::cast_precision_loss)]
            let (z, reach) = (z as f32 + 0.5, 4.0 - z as f32 * 0.4);
            for (x, y) in [
                (1.0, 0.0),
                (-1.0, 0.0),
                (0.0, 1.0),
                (0.0, -1.0),
                (0.7, 0.7),
                (-0.7, 0.7),
                (0.7, -0.7),
                (-0.7, -0.7),
            ] {
                points.push(Vec3::new(reach * x, reach * y, z));
            }
        }
        let crown = Crown::of(&points).expect("the crown");
        assert!(crown.deep(Vec3::new(0.0, 0.0, 4.0)) > 0.9);
        assert!(crown.deep(Vec3::new(4.0, 0.0, 0.5)) < 0.05);
        let top = Crown::of(
            &points
                .iter()
                .map(|p| Vec3::new(p.x, p.y, 8.0 - p.z))
                .collect::<Vec<_>>(),
        )
        .expect("turned over");
        assert!(
            top.deep(Vec3::new(2.0, 0.0, 1.0)) > crown.deep(Vec3::new(2.0, 0.0, 7.0)) - 0.2,
            "the wide part shades what lies under it"
        );
        assert!(Crown::of(&points[..4]).is_none(), "too few leaves");
    }

    /// A pack's linear color reads as display values, alpha untouched: black and white stay, a
    /// middling part brightens as sRGB does.
    #[test]
    fn a_pack_color_is_multiplied_in_display_values() {
        let [r, g, b, a] = display_color([0.0, 1.0, 0.5, 0.25]);
        assert_eq!(r, 0.0);
        assert!((g - 1.0).abs() < 1e-5);
        assert!((b - 0.735).abs() < 1e-3, "{b}");
        assert_eq!(a, 0.25);
    }

    /// A color's parts to `0xAARRGGBB`, and a picture's texels to blue, green, red, alpha.
    #[test]
    fn a_pack_color_and_picture_take_the_remasters_order() {
        assert_eq!(argb([1.0, 0.5, 0.0, 1.0]), 0xFFFF_8000);
        let image = Image {
            width: 1,
            height: 1,
            srgb: true,
            levels: vec![Arc::new(vec![10, 20, 30, 40])],
            format: None,
        };
        let t = texture_of(&image).expect("a texture");
        assert_eq!(t.format, TextureFormat::Bgra8);
        assert_eq!(t.levels, vec![vec![30, 20, 10, 40]]);
        let bc7 = Image {
            format: Some(BlockFormat::Bc7),
            levels: vec![Arc::new(vec![7; 16])],
            width: 4,
            height: 4,
            ..image
        };
        let t = texture_of(&bc7).expect("a texture");
        assert_eq!((t.format, t.levels[0].len()), (TextureFormat::Bc7, 16));
    }

    /// A surface picture (roughness in red, occlusion in green), as block pictures and as four
    /// bytes a texel, reads in glTF's layout once turned: occlusion in red, roughness in green,
    /// no metalness in blue.
    #[test]
    fn a_surface_picture_turns_to_occlusion_roughness_and_metalness() {
        let rgba: Vec<u8> = (0..16u8)
            .flat_map(|i| [40 + i * 8, 250 - i * 4, 0, 255])
            .collect();
        let plain = Image {
            width: 4,
            height: 4,
            srgb: false,
            levels: vec![Arc::new(rgba.clone())],
            format: None,
        };
        let blocks = images::compressed(plain.clone(), BlockFormat::Bc5);
        assert_eq!(blocks.format, Some(BlockFormat::Bc5));
        let before = dereth_render::block_codec::decode(BlockFormat::Bc5, 4, 4, &blocks.levels[0]);
        for (image, surface) in [
            (orm_of_surface(&plain), rgba),
            (orm_of_surface(&blocks), before),
        ] {
            let texels = match image.format {
                Some(f) => dereth_render::block_codec::decode(f, 4, 4, &image.levels[0]),
                None => image.levels[0].as_ref().clone(),
            };
            for (t, s) in texels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(surface.as_chunks::<4>().0)
            {
                assert_eq!([t[0], t[1], t[2]], [s[1], s[0], 0]);
            }
        }
    }
}
