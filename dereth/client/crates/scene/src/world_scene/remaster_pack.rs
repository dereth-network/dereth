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
//! - a picture becomes a texture. A cut-out picture is made again from its file when the pack it
//!   came from is a folder, its cut-away texels filled from its edge first
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
use dereth_animation::parts::{PartArray, PhysicsPart};
use dereth_primitives::{TextureData, TextureFormat};
use dereth_remaster::assets::pack::{
    fill_cut_away, of_pack_mesh, of_pack_picture, of_pack_surface, pack_mesh, pack_picture,
    pack_surface, Category, PackLevel, PackModel, PackSettings,
};
use dereth_remaster::assets::skin::{of_skin_surface, skin_surface, SkinModel, Slot};
use dereth_remaster::assets::terrain::{GroundMaps, LayerSlopes, SplatBank, SteepTile};
use dereth_remaster::assets::{
    kind::Kind, BlendFactor, Material, MaterialMaps, Mesh, Subset, SubsetInfo, SurfaceRef,
    TextureKey, Vertex,
};
use dereth_remaster::look::{Layer, LookToml};
use dereth_remaster::scene::Who;
use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

/// What of the art pack the remaster holds, against what its swaps drew.
pub use dereth_remaster::assets::pack::PackCounts as RemasterPackCounts;
/// What of the art pack the remaster draws.
pub use dereth_remaster::assets::pack::PackSettings as RemasterPacks;
/// What the far forest did in the remaster.
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
    /// their files. A picture whose source names a folder of its own (a stack's) is made from
    /// that one.
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
    /// Each body's place in the pack and the palettes it is drawn for, by the setup it is drawn
    /// for, in the pack's order.
    skin_by_setup: HashMap<u32, Vec<(Vec<u32>, u32)>>,
    /// Each body as the remaster draws it, made when it is first asked for.
    skin_models: Mutex<HashMap<u32, Option<Arc<SkinModel>>>>,
    /// How high each body stands playing an animation at its object's scale
    /// ([`SkinModel::top_playing`]), by its place in the pack, the animation and that scale, and
    /// the points each model is made of, by its place in the pack: as a name over them is placed,
    /// made when they are first asked for.
    skin_tops: Mutex<HashMap<SkinTop, Option<f32>>>,
    model_points: Mutex<HashMap<u32, Option<Arc<[Point]>>>>,
}

/// A point of a body or a model, as the world measures its objects.
type Point = dereth_primitives::Vec3;

/// A body's place in the pack, the slot it plays and its object's scale (each axis's bits).
type SkinTop = (u32, Option<Slot>, [u32; 3]);

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
        if folder.is_some() || pack.image_sources.iter().any(|s| s.folder.is_some()) {
            let folder = folder.clone();
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
                        let Some(folder) = s.folder.as_ref().or(folder.as_ref()) else {
                            continue;
                        };
                        if let Ok(image) = Self::filled(folder, &s.path, s.srgb, s.cutoff) {
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
        let mut skin_by_setup: HashMap<u32, Vec<(Vec<u32>, u32)>> = HashMap::new();
        for (k, i) in pack.skins.iter().zip(0u32..) {
            skin_by_setup
                .entry(k.setup.0)
                .or_default()
                .push((k.palettes.clone(), i));
        }
        Self {
            pack,
            folder,
            by_id,
            by_texture,
            view_levels,
            filled,
            skin_by_setup,
            skin_models: Mutex::new(HashMap::new()),
            skin_tops: Mutex::new(HashMap::new()),
            model_points: Mutex::new(HashMap::new()),
        }
    }

    /// How high over its object's origin the body `skin` stands drawn in place of `parts` while
    /// it plays its clip for `slot` ([`SkinModel::top_playing`]), the parts at their setup's rest
    /// and scaled as the object is.
    pub(crate) fn skin_top(
        &self,
        skin: &SkinModel,
        parts: &PartArray,
        slot: Option<Slot>,
    ) -> Option<f32> {
        let s = parts.scale;
        let key = (
            skin.skin,
            slot,
            [s.x.to_bits(), s.y.to_bits(), s.z.to_bits()],
        );
        if let Some(hit) = self
            .skin_tops
            .lock()
            .ok()
            .and_then(|m| m.get(&key).copied())
        {
            return hit;
        }
        let made = (|| {
            let rest = dereth_client_runtime::body_top::rest_frames(parts)?;
            let at: Vec<glam::Mat4> = parts
                .parts
                .iter()
                .zip(&rest)
                .map(|(p, f)| {
                    super::views::frame_matrix(f)
                        * glam::Mat4::from_scale(Vec3::new(
                            p.gfxobj_scale.x,
                            p.gfxobj_scale.y,
                            p.gfxobj_scale.z,
                        ))
                })
                .collect();
            let frame = glam::Mat4::from_scale(Vec3::new(s.x, s.y, s.z));
            skin.top_playing(&at, frame, slot)
        })();
        if let Ok(mut m) = self.skin_tops.lock() {
            m.insert(key, made);
        }
        made
    }

    /// The points the pack's model for the object `id` is made of at its nearest level, in the
    /// object's own space, when its kind is switched on.
    pub(crate) fn model_points(&self, id: u32, on: PackSettings) -> Option<Arc<[Point]>> {
        self.model(id, on)?;
        let index = *self.by_id.get(&id)?;
        let made = || -> Option<Arc<[Point]>> {
            let model = self.pack.models.get(index as usize)?;
            let lod = &model.lods[*self.view_levels[index as usize].first()?];
            let floats = crate::art_pack::VERTEX_FLOATS;
            let points: Vec<Point> = lod
                .primitives
                .iter()
                .flat_map(|p| p.vertices.chunks_exact(floats))
                .map(|v| Point::new(v[0], v[1], v[2]))
                .collect();
            (!points.is_empty()).then(|| Arc::from(points))
        };
        let Ok(mut memo) = self.model_points.lock() else {
            return made();
        };
        memo.entry(index).or_insert_with(made).clone()
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
                foliage: m.foliage,
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
        let mut levels: Vec<PackLevel> = self.view_levels[index as usize]
            .iter()
            .enumerate()
            .filter_map(|(k, l)| {
                Some(PackLevel {
                    mesh: pack_mesh(index, u8::try_from(k).ok()?),
                    distance: m.lods[*l].distance,
                })
            })
            .collect();
        // Ground cover grows only out to its last level's distance: past it, a level that draws
        // nothing.
        let cover = dereth_assets::art_pack::is_cover(m.id);
        if !levels.is_empty() && cover {
            levels.push(PackLevel {
                mesh: 0,
                distance: f32::INFINITY,
            });
        }
        (!levels.is_empty()).then_some(PackModel {
            category,
            levels,
            cover,
        })
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
        // whose sign is all anything else reads of it; how far the wind bends a growing model's
        // vertex goes in that length's whole part ([`bend_in_tangent`]).
        let crown = Crown::of_model(model);
        let top = if model.foliage {
            mesh.vertices
                .iter()
                .fold(0.0f32, |t, v| t.max(v.position[2]))
        } else {
            0.0
        };
        if crown.is_some() || top > 0.0 {
            let mut tangents = dereth_remaster::assets::tangents(&mesh);
            for (t, v) in tangents.iter_mut().zip(&mesh.vertices) {
                let p = Vec3::from(v.position);
                let deep = crown.as_ref().map_or(0.0, |c| c.deep(p));
                t[3] *= bend_in_tangent(deep, bend(p.z, top));
            }
            mesh.tangents = tangents;
        }
        Some(mesh)
    }

    /// The body drawn in place of the parts of the objects built on `setup`, when the pack has
    /// one: its primitives as one mesh in its bind space, each a surface of its own, each vertex carrying its color times its primitive's color factor.
    #[cfg(test)]
    pub(crate) fn skin(&self, setup: u32) -> Option<Arc<SkinModel>> {
        self.skin_for(setup, &[], Who::default())
    }

    /// The body drawn in place of the parts of an object built on `setup` whose look lays
    /// `palettes` over its parts: the first of the setup's bodies drawn for one of them, else the
    /// one drawn for its other objects; none for an object the pack keeps as it is (`who` it is:
    /// made from a weenie it lists, or, where it keeps them, having held a launcher).
    pub(crate) fn skin_for(
        &self,
        setup: u32,
        palettes: &[u32],
        who: Who,
    ) -> Option<Arc<SkinModel>> {
        let kept = who.weenie.is_some_and(|w| self.pack.keep.contains(&w))
            || (who.launcher && self.pack.keep_launchers);
        if kept {
            return None;
        }
        let rows = self.skin_by_setup.get(&setup)?;
        let index = rows
            .iter()
            .find(|(p, _)| p.iter().any(|x| palettes.contains(x)))
            .or_else(|| rows.iter().find(|(p, _)| p.is_empty()))
            .map(|(_, i)| *i)?;
        if let Some(made) = self.skin_models.lock().ok()?.get(&index) {
            return made.clone();
        }
        let made = self.make_skin(index).map(Arc::new);
        if made.is_none() {
            tracing::warn!(target: "dereth::remaster", "the pack's body for {setup:#010X} could not be drawn");
        }
        self.skin_models.lock().ok()?.insert(index, made.clone());
        made
    }

    fn make_skin(&self, index: u32) -> Option<SkinModel> {
        let skin = self.pack.skins.get(index as usize)?;
        let mut mesh = Mesh::default();
        let mut parts = Vec::new();
        let mut weights = Vec::new();
        let mut dqs: Vec<f32> = Vec::new();
        let none = crate::art_pack::PackSkinExtras::default();
        for (k, (p, (ps, ws))) in skin.primitives.iter().zip(&skin.influences).enumerate() {
            let first = u32::try_from(mesh.vertices.len()).ok()?;
            let extras = skin.extras.get(k).unwrap_or(&none);
            let factor = if p.material.base_color_image.is_some() && extras.surface.is_none() {
                p.material.base_color
            } else {
                [1.0; 4]
            };
            let count = p.vertices.len() / crate::art_pack::VERTEX_FLOATS;
            if !skin.extras.iter().all(|e| e.dqs.is_empty()) {
                dqs.extend((0..count).map(|i| extras.dqs.get(i).copied().unwrap_or(0.0)));
            }
            for v in p
                .vertices
                .as_chunks::<{ crate::art_pack::VERTEX_FLOATS }>()
                .0
            {
                mesh.vertices.push(Vertex {
                    position: [v[0], v[1], v[2]],
                    normal: [v[3], v[4], v[5]],
                    uv: [v[10], v[11]],
                });
                mesh.colors.push(display_color([
                    v[12] * factor[0],
                    v[13] * factor[1],
                    v[14] * factor[2],
                    v[15] * factor[3],
                ]));
            }
            parts.extend(ps.iter().copied());
            weights.extend(ws.iter().copied());
            let start = u32::try_from(mesh.indices.len()).ok()?;
            // Counter-clockwise seen from the front, as glTF has them and retail's are.
            mesh.indices.extend(p.indices.iter().map(|i| first + i));
            mesh.subsets.push(Subset {
                first_index: start,
                index_count: u32::try_from(mesh.indices.len()).ok()? - start,
                surface: SurfaceRef::Surface(match extras.surface {
                    Some(s) => s,
                    None => skin_surface(index, u8::try_from(k).ok()?),
                }),
                two_sided: p.material.double_sided,
                tiled: extras.surface.is_none() || extras.tiled,
            });
        }
        let rig = dereth_remaster::assets::skin::Rig {
            dqs,
            gfxobjs: skin.gfxobjs.clone(),
            retail: skin.extras.iter().any(|e| e.surface.is_some()),
            bones: skin
                .bones
                .iter()
                .map(|b| dereth_remaster::assets::skin::Bone::new(b.inverse_bind))
                .collect(),
            clips: skin
                .clips
                .iter()
                .filter_map(|(slot, c)| {
                    let slot = dereth_remaster::assets::skin::Slot::named(slot)?;
                    Some((
                        slot,
                        dereth_remaster::assets::skin::Clip {
                            steps: Arc::clone(&c.steps),
                            scales: Arc::clone(&c.scales),
                            seconds: c.seconds,
                            stride: c.stride,
                            hit: c.hit,
                        },
                    ))
                })
                .collect(),
        };
        SkinModel::new(index, mesh, parts, weights, &skin.inverse_binds).map(|m| m.with_rig(rig))
    }

    /// The surface a pack surface id names: its part's base color picture (or its color where it
    /// has none), its cut, drawn as the model's kind; or a body's primitive's, drawn as a
    /// character.
    pub(crate) fn material(&self, id: u32) -> Option<Material> {
        if let Some((index, primitive)) = of_skin_surface(id) {
            let skin = self.pack.skins.get(index as usize)?;
            let m = skin.primitives.get(usize::from(primitive))?.material;
            return Some(Self::material_of(&m, Kind::Character));
        }
        let (index, level, part) = of_pack_surface(id)?;
        let model = self.pack.models.get(index as usize)?;
        let lod = &model.lods[*self.view_levels[index as usize].get(usize::from(level))?];
        let m = lod.primitives.get(usize::from(part))?.material;
        let kind = if model.foliage && m.alpha_cutoff.is_some() {
            Kind::Foliage
        } else {
            Kind::Architecture
        };
        Some(Self::material_of(&m, kind))
    }

    /// A pack material as the remaster's, drawn as `kind`.
    fn material_of(m: &crate::art_pack::PackMaterial, kind: Kind) -> Material {
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
        Material {
            kind,
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
        }
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
        let folder = source.and_then(|s| s.folder.as_ref().or(self.folder.as_ref()));
        match (source, folder) {
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
/// The height over its origin of the top of what `pack` draws, as `on` says, in place of the
/// parts `parts` of an object built on `setup`, being `who`, standing in the slot `looping`
/// (its stance's idle): the pack's body for its setup and palettes, as high as it reaches in its
/// clip for that slot (at rest without one), with any parts it is not drawn in place of drawn as
/// they are; or, at rest, the parts it has models for drawn as those models. `own` gives the points the
/// files hold for each graphics object. `None` when the pack draws the object as the files have
/// it.
fn drawn_name_height(
    pack: &PackAssets,
    on: PackSettings,
    (setup, who, looping): (Option<u32>, Who, Option<Slot>),
    parts: &PartArray,
    own: &dyn Fn(dereth_primitives::DataId) -> Option<Arc<[Point]>>,
) -> Option<f32> {
    use dereth_client_runtime::body_top::rest_top_drawn;
    let own_of = |p: &PhysicsPart| (p.gfxobj_id.0 != 0).then(|| own(p.gfxobj_id)).flatten();
    let body = setup
        .filter(|_| on.characters)
        .and_then(|s| pack.skin_for(s, &skin_palettes(parts), who));
    if let Some(skin) = body {
        // The body, and the parts it does not follow, which draw as they are.
        let rest = rest_top_drawn(parts, |i, p| {
            (!skin.follows(i)).then(|| own_of(p)).flatten()
        });
        return match (pack.skin_top(&skin, parts, looping), rest) {
            (Some(b), Some(r)) => Some(b.max(r)),
            (b, r) => b.or(r),
        };
    }
    let swapped = std::cell::Cell::new(false);
    let top = rest_top_drawn(parts, |_, p| {
        if p.gfxobj_id.0 == 0 {
            return None;
        }
        match pack.model_points(p.gfxobj_id.0, on) {
            Some(points) => {
                swapped.set(true);
                Some(points)
            }
            None => own_of(p),
        }
    });
    top.filter(|_| swapped.get())
}

/// The palettes the remaster picks a body for `parts` by: those its description lays, and those
/// its parts carry.
fn skin_palettes(parts: &PartArray) -> Vec<u32> {
    let mut out: Vec<u32> = parts.described_palettes.iter().map(|p| p.0).collect();
    for o in parts
        .parts
        .iter()
        .filter_map(|p| p.surface_overrides.as_ref())
    {
        for id in o
            .shift_palette
            .iter()
            .chain(o.subpalettes.iter().map(|r| &r.palette_set))
        {
            if !out.contains(&id.0) {
                out.push(id.0);
            }
        }
    }
    out
}

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
    /// The height over its origin of the top of what the art pack draws in place of the parts
    /// of the server's object `id`, made from `weenie` and having held a launcher or not, which
    /// `driver` moves ([`drawn_name_height`]); `own` gives the points the files hold for each
    /// graphics object. `None` while the remaster is off, and when the pack draws the object as
    /// the files have it.
    pub(crate) fn pack_name_height(
        &self,
        (id, (weenie, launcher)): (dereth_primitives::ObjectId, (Option<u32>, bool)),
        driver: &dereth_animation::driver::MotionDriver,
        own: &dyn Fn(dereth_primitives::DataId) -> Option<Arc<[Point]>>,
    ) -> Option<f32> {
        if !self.remaster_on() {
            return None;
        }
        let view = self.remaster_view.borrow();
        let on = view.packs;
        let pack = view.pack.as_ref().filter(|_| on.any())?;
        let setup = self.object_draws.get(&id).map(|d| d.setup.0);
        // The body stands as high as its stance's idle reaches.
        let looping = Some(super::remaster_slots::idle_of(
            driver.motion_table.state.style,
        ));
        let who = Who { weenie, launcher };
        drawn_name_height(pack, on, (setup, who, looping), &driver.part_array, own)
    }

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

/// How far the wind bends a growing model at its top, a share of its height, at a wind of a
/// meter a second.
const BEND_AT_TOP: f32 = 0.02;

/// How far the wind bends the vertex `z` meters up a growing model `top` meters tall, meters at a
/// wind of a meter a second: nothing at its foot or below, rising with the square of the height,
/// so the trunk stays put at the ground and the crown sways.
fn bend(z: f32, top: f32) -> f32 {
    if top <= 0.0 || z <= 0.0 {
        return 0.0;
    }
    let h = (z / top).min(1.0);
    h * h * top * BEND_AT_TOP
}

/// A tangent's last value's length carrying how deep in the crown a vertex lies (`deep`, 0 to 1)
/// and how far the wind bends it (`bend`, meters at a meter a second): `1 + deep` plus twice the
/// bend in whole millimeters. The G-buffer takes the bend out again before anything else reads
/// the length.
fn bend_in_tangent(deep: f32, bend: f32) -> f32 {
    1.0 + deep.clamp(0.0, 1.0) + 2.0 * (bend * 1000.0).round()
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

    /// A growing model bends nothing at its foot or below and most at its top, and the bend
    /// and the crown's depth both come back out of the tangent's length as the G-buffer reads
    /// them: the bend as whole millimeters, the depth to within a 255th.
    #[test]
    fn a_trees_bend_rises_to_its_top_and_rides_in_its_tangent() {
        assert_eq!(bend(0.0, 30.0), 0.0);
        assert_eq!(bend(-1.0, 30.0), 0.0);
        assert!((bend(30.0, 30.0) - 30.0 * BEND_AT_TOP).abs() < 1e-6);
        assert!((bend(15.0, 30.0) - 30.0 * BEND_AT_TOP / 4.0).abs() < 1e-6);
        assert_eq!(bend(10.0, 0.0), 0.0, "not growing: no bend");
        for (deep, b) in [(0.0, 0.0), (1.0, 0.0), (0.0, 0.6), (1.0, 0.6), (0.37, 2.5)] {
            let a = bend_in_tangent(deep, b);
            // The G-buffer's arithmetic.
            let q = ((a - 1.0) / 2.0 + 1e-3).floor();
            let back = (a - 1.0 - 2.0 * q).clamp(0.0, 1.0);
            assert!(
                (q / 1000.0 - b).abs() < 1e-3,
                "bend {b} came back {}",
                q / 1000.0
            );
            assert!(
                (back - deep).abs() < 1.0 / 255.0,
                "depth {deep} came back {back}"
            );
        }
    }

    /// Instrument: the time posing a pack's body takes on the CPU, as the remaster poses it each
    /// frame (its joints, every vertex blended, the vertex rows written), timed over many poses:
    /// the least and the median, which a busy machine moves less than one frame's count does.
    /// `DERETH_ART_PACK` (a pack folder with a `[[skin]]`) and `DERETH_SKIN_SETUP` (hex).
    #[test]
    #[ignore = "instrument: times a pack body's posing; run with --ignored --nocapture"]
    fn time_posing_a_packs_body() {
        let pack = std::env::var("DERETH_ART_PACK").expect("DERETH_ART_PACK");
        let setup = std::env::var("DERETH_SKIN_SETUP").unwrap_or_else(|_| "0x02000001".to_owned());
        let setup = u32::from_str_radix(setup.trim_start_matches("0x"), 16).expect("a hex setup");
        let path = PathBuf::from(pack);
        let art = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
        let assets = PackAssets::new(art, Some(path));
        let model = assets.skin(setup).expect("the pack's body");
        // Each part turned a little from its bind, alternately, so the joints bend.
        let parts: Vec<glam::Mat4> = model
            .inverse_binds
            .iter()
            .enumerate()
            .map(|(k, b)| {
                #[allow(clippy::cast_precision_loss)]
                let turn = glam::Mat4::from_rotation_x(0.45 * (k % 3) as f32);
                b.map_or(glam::Mat4::IDENTITY, |b| turn * b.inverse())
            })
            .collect();
        let mut times = Vec::new();
        let mut rows = Vec::new();
        for _ in 0..300 {
            let started = std::time::Instant::now();
            let joints = model.joints(&parts).expect("joints");
            let posed = model.posed(&joints);
            rows.clear();
            for (k, (p, n, t)) in posed.iter().enumerate() {
                let uv = model.mesh.vertices[k].uv;
                for f in p.iter().chain(n).chain(&uv).chain(t).chain(&[1.0f32; 4]) {
                    rows.extend_from_slice(&f.to_le_bytes());
                }
            }
            times.push(started.elapsed().as_secs_f64() * 1e3);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{} vertices, {} dual-quaternion: least {:.3} ms, median {:.3} ms a pose; {} bytes of rows",
            model.mesh.vertices.len(),
            model.rig.dqs.iter().filter(|s| **s > 0.0).count(),
            times[0],
            times[times.len() / 2],
            rows.len()
        );
    }

    /// A pack of a body three metres tall for one setup, one drawn only for a palette on another,
    /// and a rock model a metre tall for one graphics object; and an object's parts at rest, each
    /// a part drawing `gfxobjs[i]` placed `heights[i]` over its origin, the object `scale` times
    /// its size.
    mod drawn {
        use super::*;
        use dereth_animation::data::{AnimFrame, SetupData};
        use dereth_primitives::{DataId, Frame, Quat};

        pub(super) const TALL: u32 = 0x0200_0A01;
        pub(super) const ROBED: u32 = 0x0200_0A02;
        pub(super) const ROBE: u32 = 0x0400_0A01;
        pub(super) const ROCK: u32 = 0x0100_0A01;
        /// A weenie the pack keeps as it is.
        pub(super) const KEPT: u32 = 19_196;

        fn bytes(v: impl IntoIterator<Item = f32>) -> Vec<u8> {
            v.into_iter().flat_map(f32::to_le_bytes).collect()
        }

        /// A triangle `high` metres tall, skinned whole to `part_00` standing at its foot.
        fn body(high: f32) -> (Vec<u8>, Vec<u8>) {
            let mut bin = bytes([0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, high, 0.0]);
            bin.extend([0u8; 12]);
            bin.extend(bytes([1.0, 0.0, 0.0, 0.0].repeat(3)));
            bin.extend(bytes(glam::Mat4::IDENTITY.to_cols_array()));
            bin.extend([0u8, 0, 1, 0, 2, 0, 0, 0]);
            let json = format!(
                r#"{{"asset": {{"version": "2.0"}}, "scenes": [{{"nodes": [0, 1]}}],
                "nodes": [{{"name": "part_00"}}, {{"mesh": 0, "skin": 0}}],
                "skins": [{{"joints": [0], "inverseBindMatrices": 3}}],
                "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "JOINTS_0": 1,
                    "WEIGHTS_0": 2}}, "indices": 4}}]}}],
                "buffers": [{{"uri": "body.bin", "byteLength": {}}}],
                "bufferViews": [
                    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
                    {{"buffer": 0, "byteOffset": 36, "byteLength": 12}},
                    {{"buffer": 0, "byteOffset": 48, "byteLength": 48}},
                    {{"buffer": 0, "byteOffset": 96, "byteLength": 64}},
                    {{"buffer": 0, "byteOffset": 160, "byteLength": 6}}
                ],
                "accessors": [
                    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}},
                    {{"bufferView": 1, "componentType": 5121, "count": 3, "type": "VEC4"}},
                    {{"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4"}},
                    {{"bufferView": 3, "componentType": 5126, "count": 1, "type": "MAT4"}},
                    {{"bufferView": 4, "componentType": 5123, "count": 3, "type": "SCALAR"}}
                ]}}"#,
                bin.len()
            );
            (json.into_bytes(), bin)
        }

        pub(super) fn pack() -> PackAssets {
            pack_keeping(false)
        }

        /// [`pack`], keeping objects that have held a launcher as they are where `launchers`.
        pub(super) fn pack_keeping(launchers: bool) -> PackAssets {
            let credit = "author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
            let manifest = format!(
                "[pack]\nname = \"Heights\"\nformat = 1\n\
                 [[skin]]\nsetup = \"{TALL:#010X}\"\nfile = \"tall/body.gltf\"\n{credit}\
                 [[skin]]\nsetup = \"{ROBED:#010X}\"\nfile = \"robed/body.gltf\"\n\
                 palettes = [\"{ROBE:#010X}\"]\n{credit}\
                 [[model]]\nid = \"{ROCK:#010X}\"\ncategory = \"rock\"\n{credit}\
                 [[model.lod]]\nfile = \"rock/lod0.gltf\"\n\
                 [keep]\nweenies = [{KEPT}]\nlaunchers = {launchers}\n"
            );
            let (tall, tall_bin) = body(3.0);
            let (robed, robed_bin) = body(2.5);
            let rock = br#"{"asset": {"version": "2.0"}, "scenes": [{"nodes": [0]}],
                "nodes": [{"mesh": 0}],
                "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
                "buffers": [{"uri": "rock.bin", "byteLength": 36}],
                "bufferViews": [{"buffer": 0, "byteLength": 36}],
                "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3,
                    "type": "VEC3"}]}"#;
            let files: HashMap<&str, Vec<u8>> = [
                ("tall/body.gltf", tall),
                ("tall/body.bin", tall_bin),
                ("robed/body.gltf", robed),
                ("robed/body.bin", robed_bin),
                ("rock/lod0.gltf", rock.to_vec()),
                (
                    "rock/rock.bin",
                    bytes([0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
                ),
            ]
            .into_iter()
            .collect();
            let art = ArtPack::open(&manifest, &mut |path| {
                files
                    .get(path)
                    .cloned()
                    .ok_or_else(|| format!("{path}: missing"))
            })
            .expect("the pack opens");
            assert!(art.problems.is_empty(), "{:?}", art.problems);
            PackAssets::new(Arc::new(art), None)
        }

        pub(super) fn parts(gfxobjs: &[u32], heights: &[f32], scale: f32) -> PartArray {
            let frames = heights
                .iter()
                .map(|z| Frame::new(Point::new(0.0, 0.0, *z), Quat::IDENTITY))
                .collect();
            let setup = SetupData {
                parts: gfxobjs.iter().map(|g| DataId(*g)).collect(),
                placement_frames: [(
                    0x65,
                    AnimFrame {
                        frames,
                        hooks: Vec::new(),
                    },
                )]
                .into(),
                ..SetupData::default()
            };
            let mut parts = PartArray::new();
            parts.setup = Some(Arc::new(setup));
            parts.scale = Point::new(scale, scale, scale);
            parts.parts = gfxobjs
                .iter()
                .map(|g| {
                    let mut p = PhysicsPart::new(DataId(*g));
                    p.gfxobj_scale = Point::new(scale, scale, scale);
                    p
                })
                .collect();
            parts
        }

        /// A graphics object of the files: a point half a metre up.
        pub(super) fn own(_: DataId) -> Option<Arc<[Point]>> {
            Some(Arc::from(vec![Point::new(0.0, 0.0, 0.5)]))
        }
    }

    #[test]
    fn a_name_stands_over_what_the_pack_draws_in_place_of_a_body_scaled_as_the_body_is() {
        use drawn::{own, parts, ROBE, ROBED, ROCK, TALL};
        let pack = drawn::pack();
        let on = PackSettings::all();
        let near = |a: Option<f32>, b: f32| a.is_some_and(|a| (a - b).abs() < 1e-4);
        // The body three metres tall, drawn through a part standing a fifth of a metre up: its
        // top over where the object stands, scaled as the object is, wherever that part is. A
        // part it is not drawn in place of keeps drawing its own, and is measured with it.
        let body = parts(&[0x0100_0001, 0x0100_0002], &[0.2, 0.0], 1.0);
        let h = drawn_name_height(&pack, on, (Some(TALL), Who::default(), None), &body, &own);
        assert!(near(h, 3.0), "{h:?}");
        let big = parts(&[0x0100_0001], &[0.2], 1.5);
        let h = drawn_name_height(&pack, on, (Some(TALL), Who::default(), None), &big, &own);
        assert!(near(h, 4.5), "{h:?}");
        let low = parts(&[0x0100_0001, 0x0100_0002], &[0.2, 4.0], 1.0);
        let h = drawn_name_height(&pack, on, (Some(TALL), Who::default(), None), &low, &own);
        assert!(near(h, 4.5), "a part over the body: {h:?}");
        // A body drawn only for a palette: over it where its part lays that palette, as the files
        // have it elsewhere.
        let mut robed = parts(&[0x0100_0001], &[0.0], 1.0);
        assert_eq!(
            drawn_name_height(&pack, on, (Some(ROBED), Who::default(), None), &robed, &own),
            None
        );
        robed.parts[0].use_palette(dereth_primitives::DataId(ROBE), &[]);
        let h = drawn_name_height(&pack, on, (Some(ROBED), Who::default(), None), &robed, &own);
        assert!(near(h, 2.5), "{h:?}");
        // The bodies off: as the files have it.
        let no_bodies = PackSettings {
            characters: false,
            ..on
        };
        assert_eq!(
            drawn_name_height(
                &pack,
                no_bodies,
                (Some(TALL), Who::default(), None),
                &body,
                &own
            ),
            None
        );
        // A part drawn as the pack's model: the model's top over it, the rest measured as they
        // are; with rocks off, or no part the pack has a model for, as the files have it.
        let stone = parts(&[ROCK, 0x0100_0002], &[0.5, 0.0], 2.0);
        let h = drawn_name_height(&pack, on, (None, Who::default(), None), &stone, &own);
        assert!(near(h, 1.0 + 2.0), "{h:?}");
        let no_rocks = PackSettings { rocks: false, ..on };
        assert_eq!(
            drawn_name_height(&pack, no_rocks, (None, Who::default(), None), &stone, &own),
            None
        );
        let plain = parts(&[0x0100_0002], &[0.0], 1.0);
        assert_eq!(
            drawn_name_height(&pack, on, (None, Who::default(), None), &plain, &own),
            None
        );
    }

    #[test]
    fn a_pack_draws_no_body_for_an_object_it_keeps() {
        use drawn::{own, parts, KEPT, TALL};
        let who = |weenie: Option<u32>, launcher: bool| Who { weenie, launcher };
        let pack = drawn::pack();
        assert!(pack.skin_for(TALL, &[], who(Some(KEPT), false)).is_none());
        assert!(pack
            .skin_for(TALL, &[], who(Some(KEPT + 1), false))
            .is_some());
        assert!(pack.skin_for(TALL, &[], who(None, false)).is_some());
        // Having held a launcher: kept only by a pack that keeps them.
        assert!(pack.skin_for(TALL, &[], who(None, true)).is_some());
        let shooters = drawn::pack_keeping(true);
        assert!(shooters.skin_for(TALL, &[], who(None, true)).is_none());
        assert!(shooters
            .skin_for(TALL, &[], who(Some(KEPT + 1), false))
            .is_some());
        // Its name stands where the files' parts put it.
        let body = parts(&[0x0100_0001], &[0.2], 1.0);
        let on = PackSettings::all();
        for (pack, kept) in [
            (&pack, who(Some(KEPT), false)),
            (&shooters, who(None, true)),
        ] {
            assert_eq!(
                drawn_name_height(pack, on, (Some(TALL), kept, None), &body, &own),
                None
            );
        }
    }
}
