//! The remaster's assets, read from the dats through the scene's own decode path, with no device.
//!
//! [`DatAssets`] is the remaster's [`AssetSource`]: it builds each mesh from the same polygon
//! fans the scene bakes, each material from the same surface setup, each picture from the same
//! lookup, palette and expansion, and each ground picture from the same merge, so what the
//! remaster draws with is what Retail Look draws with. Only the device is left out: a picture
//! comes back as its levels, block-compressed pictures with the chain the scene builds for them
//! and expanded ones with a box-filtered chain down to one texel.
//!
//! It reads; it never writes anything the scene draws from.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use dereth_assets::region::{Region, TexMerge};
use dereth_assets::world::CellLandblock;
use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, TextureData, TextureFormat};
use dereth_remaster::assets::terrain::{BroadPicture, Splat, SplatOverlay};
use dereth_remaster::assets::{
    kind, Appearance, AssetSource, BlendFactor, CellPieceKey, Material, MaterialKey, Mesh,
    PaletteRecipe, SubsetInfo, SurfaceRef, TerrainCell, TextureKey, Use, Vertex,
};
use dereth_remaster::scene::{BlockId, PartLook, Terrain};
use dereth_render::pso::{PipelineKey, SurfaceContext};
use dereth_render::surface::{surface_type as st, Surface as RenderState, SurfaceHandler};
use dereth_render::texture::ImageScale;
use dereth_render::vertex::VertexFormat;
use dereth_terrain::land::merge::Bgra8;
use dereth_terrain::land::merge::TerrainTextureSource;

use super::{DatTerrainTextures, PaletteComposition, TextureStore};

/// The remaster's source of assets over one set of dats.
pub struct DatAssets<'a> {
    store: &'a RetailDatStore,
    textures: TextureStore<'a>,
    /// The environment texture detail the pictures are read at.
    detail: u32,
    /// The texture scale pictures are made at, as the scene's own surfaces are.
    image_scale: ImageScale,
    /// The region whose land surface the ground's cells are keyed by, and its texture merge.
    region: Option<Region>,
    /// The region whose sky the sky's objects come from.
    sky_region: Option<Region>,
    tex_merge: Option<TexMerge>,
    /// The terrain types the land surface has a picture for, one bit each: a vertex of another
    /// type is drawn as its neighbours are.
    drawn: u32,
    /// The files the ground's pictures are read from, when they are not `store`.
    ground_store: Option<&'a RetailDatStore>,
    /// The ground pictures' scale shift, as the scene's merge cache has it.
    merge_shift: u32,
    /// Decoded terrain tiles, shared by every cell composite.
    tiles: RefCell<HashMap<DataId, Option<Arc<Bgra8>>>>,
    // ORDER-OK: a memo, looked up by key only.
    palettised: HashMap<DataId, bool>,
    /// The art pack, and what of it is drawn.
    pack: Option<(
        Arc<super::remaster_pack::PackAssets>,
        dereth_remaster::assets::pack::PackSettings,
    )>,
}

impl std::fmt::Debug for DatAssets<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatAssets")
            .field("image_scale", &self.image_scale)
            .field("merge_shift", &self.merge_shift)
            .finish_non_exhaustive()
    }
}

impl<'a> DatAssets<'a> {
    /// A source over `store` at the environment texture detail `detail` and the texture scale
    /// `image_scale`, with no ground.
    pub fn new(store: &'a RetailDatStore, detail: u32, image_scale: ImageScale) -> Self {
        Self {
            store,
            textures: TextureStore::with_environment_texture_detail(store, detail),
            detail,
            image_scale,
            region: None,
            sky_region: None,
            tex_merge: None,
            drawn: u32::MAX,
            ground_store: None,
            merge_shift: 0,
            tiles: RefCell::new(HashMap::new()),
            palettised: HashMap::new(),
            pack: None,
        }
    }

    /// This source with `pack` drawn as `on` says: its models, their surfaces and pictures, and
    /// its pictures for retail surfaces.
    #[must_use]
    pub(crate) fn with_pack(
        mut self,
        pack: Option<Arc<super::remaster_pack::PackAssets>>,
        on: dereth_remaster::assets::pack::PackSettings,
    ) -> Self {
        self.pack = pack.filter(|_| on.any()).map(|p| (p, on));
        self
    }

    /// A source reading `store` as `draw` does: its texture detail and scale, its region and its
    /// ground merge.
    pub fn for_scene(store: &'a RetailDatStore, draw: &'a super::SceneDraw) -> Self {
        let land = &draw.land;
        let mut s = Self::new(
            store,
            land.bake.environment_texture_detail,
            land.bake.image_scale,
        );
        s.region = Some((**land.ground.as_ref().unwrap_or(&land.region)).clone());
        s.sky_region = Some((*land.region).clone());
        s.tex_merge = Some(land.tex_merge.clone());
        s.drawn = land.drawn;
        s.ground_store = land.ground_store.as_ref();
        s.merge_shift = land.merge.shift;
        s
    }

    fn palettised(&mut self, id: DataId) -> bool {
        if let Some(p) = self.palettised.get(&id) {
            return *p;
        }
        let p = self.textures.is_palettised(id).unwrap_or(false);
        self.palettised.insert(id, p);
        p
    }

    /// A recipe as the scene's palette builder takes it.
    fn composition(recipe: &PaletteRecipe) -> PaletteComposition {
        PaletteComposition {
            base: DataId(recipe.base),
            ranges: recipe
                .ranges
                .iter()
                .map(|&(set, offset, count)| (DataId(set), offset, count))
                .collect(),
            from_look: Vec::new(),
        }
    }

    /// A decoded picture scaled as the scene scales it, with its levels.
    fn levels(&self, data: &TextureData) -> Option<TextureData> {
        let data = dereth_render::texture::scale_surface(data, self.image_scale).ok()?;
        chain(data)
    }
}

/// A picture with every level it is drawn with: a block-compressed one with the chain the scene
/// builds for it on the CPU, an expanded one box-filtered down to one texel, as a device makes
/// the chain of a single-level picture.
fn chain(data: TextureData) -> Option<TextureData> {
    if let Some(c) = dereth_render::mip::compressed_system_chain(&data).ok()? {
        return Some(c);
    }
    if data.format != TextureFormat::Bgra8 || data.levels.len() != 1 {
        return Some(data);
    }
    let mut out = data;
    let (mut w, mut h) = (out.width, out.height);
    while w > 1 || h > 1 {
        let last = out.levels.last()?;
        let (next, nw, nh) = dereth_render::mip::box_filter_bgra8(last, w, h).ok()?;
        out.levels.push(next);
        (w, h) = (nw, nh);
    }
    Some(out)
}

/// A mesh from the scene's surface groups: each group a subset, in order, its surface named by
/// `surface`.
fn mesh_of(
    groups: &[dereth_client_runtime::models::SurfaceGroup],
    surface: impl Fn(Option<DataId>) -> SurfaceRef,
) -> Mesh {
    Mesh::from_triangle_lists(groups.iter().map(|g| {
        let vertices = g
            .vertices
            .iter()
            .enumerate()
            .map(|(i, (p, u, v))| {
                let n = g
                    .normals
                    .get(i)
                    .copied()
                    .unwrap_or(dereth_primitives::Vec3::ZERO);
                Vertex {
                    position: [p.x, p.y, p.z],
                    normal: [n.x, n.y, n.z],
                    uv: [*u, *v],
                }
            })
            .collect();
        (
            SubsetInfo {
                surface: surface(g.surface),
                two_sided: g.two_sided,
                tiled: g.tiled,
            },
            vertices,
        )
    }))
}

fn blend(b: dereth_render::pso::Blend) -> BlendFactor {
    use dereth_render::pso::Blend;
    match b {
        Blend::Zero => BlendFactor::Zero,
        Blend::SrcAlpha => BlendFactor::SrcAlpha,
        Blend::InvSrcAlpha => BlendFactor::InvSrcAlpha,
        // Surface setup chooses among the four; the rest are the interface's and the detail
        // pass's, never a surface's.
        _ => BlendFactor::One,
    }
}

impl AssetSource for DatAssets<'_> {
    fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
        if dereth_remaster::assets::pack::of_pack_mesh(id).is_some() {
            return self.pack.as_ref().and_then(|(p, _)| p.mesh(id));
        }
        let id = DataId(id);
        let bytes = self.store.read_typed(DbType::GfxObj, id).ok()?;
        let obj =
            dereth_assets::GfxObj::decode_payload_in(self.store.era_of(id), id, &bytes).ok()?;
        let groups = dereth_client_runtime::models::triangulate(&obj);
        if groups.is_empty() {
            return None;
        }
        let mut mesh = mesh_of(&groups, |s| {
            s.map_or(SurfaceRef::Missing, |s| SurfaceRef::Surface(s.0))
        });
        mesh.sphere = dereth_world_data::setup::drawing_sphere(&obj)
            .map(|s| ([s.center.x, s.center.y, s.center.z], s.radius));
        mesh.sort_center = [obj.sort_center.x, obj.sort_center.y, obj.sort_center.z];
        Some(mesh)
    }

    fn cell_piece(&mut self, key: CellPieceKey) -> Option<Mesh> {
        let id = DataId(key.environment);
        let bytes = self.store.read_typed(DbType::Environment, id).ok()?;
        let env = dereth_assets::geometry::Environment::decode_payload_in(
            self.store.era_of(id),
            id,
            &bytes,
        )
        .ok()?;
        let cs = env.cells.get(usize::from(key.structure))?;
        // The piece's polygons name surfaces by their place in a cell's list; each place stands
        // for itself here, and every cell built from the piece puts its own surfaces there.
        let slots = cs
            .polygons
            .iter()
            .map(|p| p.pos_surface)
            .max()
            .map_or(0, |m| u32::from(m) + 1);
        let surfaces: Vec<DataId> = (0..slots).map(DataId).collect();
        let groups = dereth_client_runtime::models::triangulate_cell(cs, &surfaces);
        if groups.is_empty() {
            return None;
        }
        Some(mesh_of(&groups, |s| {
            s.and_then(|s| u16::try_from(s.0).ok())
                .map_or(SurfaceRef::Missing, SurfaceRef::Slot)
        }))
    }

    fn appearance(&mut self, surface: u32, look: Option<&PartLook>) -> Appearance {
        let Some(look) = look else {
            return Appearance::default();
        };
        let Some(decoded) = super::appearance::read_surface(self.store, DataId(surface)) else {
            return Appearance::default();
        };
        let orig = decoded.orig_texture_id;
        // A texture swap names the surface's own texture; a shift palette changes only a
        // palettised picture. Either is left out where it cannot change a pixel.
        let texture = orig.and_then(|o| {
            look.textures
                .iter()
                .find(|(old, _)| *old == o.0)
                .map(|(_, new)| *new)
        });
        let effective = texture.map(DataId).or(orig);
        let palette = match (look.palette, effective) {
            (Some(base), Some(tex)) if self.palettised(tex) => Some(PaletteRecipe {
                base,
                ranges: look.ranges.clone(),
            }),
            _ => None,
        };
        Appearance { texture, palette }
    }

    fn material(&mut self, key: &MaterialKey, used: Use) -> Material {
        if let Some(m) = self
            .pack
            .as_ref()
            .and_then(|(p, _)| p.material(key.surface))
        {
            return m;
        }
        let surface = (key.surface != 0).then_some(DataId(key.surface));
        let decoded = surface.and_then(|id| super::appearance::read_surface(self.store, id));
        let clip_map = decoded
            .as_ref()
            .is_some_and(|s| s.surface_type & st::BASE1_CLIPMAP != 0);
        let shift = key
            .appearance
            .palette
            .as_ref()
            .and_then(|r| Self::composition(r).build(&self.textures))
            .map(|(p, _)| p);
        let texture_id = key.appearance.texture.map(DataId).or(surface);
        let (combined, palettised) =
            super::appearance::combined_key(&self.textures, texture_id, shift.as_ref());
        let picture = texture_id.and_then(|id| self.textures.resolve(id).ok());
        let texture = picture.as_ref().map(|(rsid, rs, _)| {
            match (&key.appearance.palette, shift.is_some() && palettised) {
                (Some(recipe), true) => TextureKey::Shifted {
                    picture: rsid.0,
                    palette: recipe.clone(),
                    clip_map: clip_map && palettised,
                },
                _ => {
                    debug_assert!(!combined.is_uncached() || rsid.0 == 0);
                    TextureKey::Dat {
                        picture: rsid.0,
                        palette: rs.default_palette_id.map_or(0, |p| p.0),
                    }
                }
            }
        });
        let state = decoded
            .as_ref()
            .map_or_else(RenderState::default, |s| RenderState {
                r#type: s.surface_type,
                handler: SurfaceHandler::Database,
                color_value: s.color_value.unwrap_or(0),
                translucency: s.translucency,
                luminosity: s.luminosity,
                diffuse: s.diffuse,
            });
        let ss = PipelineKey::state_from_surface(
            &state,
            SurfaceContext {
                vertex_format: VertexFormat::XyzNormalDiffuseTex1,
                texture_is_set: texture.is_some(),
                texture_has_palette: palettised,
                lighting: true,
                ..SurfaceContext::default()
            },
        );
        let texture = texture.or_else(|| ss.solid_color.map(TextureKey::Solid));
        // The pack's picture in place of the surface's, while its materials are drawn: for a
        // surface drawn under no shift palette, by its own texture or the one swapped in.
        let pack_picture = self
            .pack
            .as_ref()
            .filter(|(_, on)| on.materials)
            .and_then(|(p, _)| {
                if key.appearance.palette.is_some() {
                    return None;
                }
                let record = key.appearance.texture.or_else(|| {
                    decoded
                        .as_ref()
                        .and_then(|s| s.orig_texture_id)
                        .map(|t| t.0)
                })?;
                p.material_picture(record)
            });
        let (texture, maps) = match pack_picture {
            Some((picture, maps)) => (Some(picture), Some(maps)),
            None => (texture, None),
        };
        let mut m = Material {
            kind: kind::Kind::Item,
            surface_type: state.r#type,
            data_category: picture.as_ref().map(|(_, rs, _)| rs.data_category),
            texture,
            clip_map: clip_map && palettised,
            alpha_test: ss.key.alpha_test.then_some(ss.alpha_ref),
            vertex_alpha: ss.vertex_alpha,
            vertex_colored: false,
            translucent: (state.r#type & st::TRANSLUCENT != 0).then_some(ss.vertex_alpha),
            additive: state.r#type & st::ADDITIVE != 0,
            blended: ss.key.alpha_blend,
            src_blend: blend(ss.key.src_blend),
            dst_blend: blend(ss.key.dst_blend),
            z_write: ss.key.z_write,
            fogged: state.r#type & st::ADDITIVE == 0,
            solid: ss.solid_color,
            luminosity: state.luminosity,
            diffuse: state.diffuse,
            maps,
        };
        m.kind = kind::of(&m, used);
        m
    }

    fn texture(&mut self, key: &TextureKey, clip_map: bool) -> Option<TextureData> {
        if let (TextureKey::Dat { picture, .. }, Some((p, _))) = (key, self.pack.as_ref()) {
            if dereth_remaster::assets::pack::of_pack_picture(*picture).is_some() {
                return p.texture(*picture);
            }
        }
        match key {
            TextureKey::Dat { picture, .. } => {
                let data = self
                    .textures
                    .texture_data_shifted(DataId(*picture), clip_map, None)
                    .ok()?;
                self.levels(&data)
            }
            TextureKey::Shifted {
                picture,
                palette,
                clip_map,
            } => {
                let (p, _) = Self::composition(palette).build(&self.textures)?;
                let data = self
                    .textures
                    .texture_data_shifted(DataId(*picture), *clip_map, Some(&p))
                    .ok()?;
                self.levels(&data)
            }
            TextureKey::Solid(argb) => Some(TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![argb.to_le_bytes().to_vec()],
            }),
            TextureKey::Terrain(key) => {
                let tm = self.tex_merge.as_ref()?;
                let plan = dereth_terrain::land::merge::merge_plan(
                    tm,
                    dereth_terrain::land::merge::MergeKey(*key),
                    self.merge_shift,
                );
                let mut memo = self.tiles.borrow_mut();
                let sources = DatTerrainTextures {
                    textures: TextureStore::with_environment_texture_detail(
                        self.ground_store.unwrap_or(self.store),
                        self.detail,
                    ),
                    memo: RefCell::new(&mut memo),
                };
                let img = dereth_terrain::land::merge::execute_merge_plan(&plan, &sources);
                chain(TextureData {
                    width: img.width,
                    height: img.height,
                    format: TextureFormat::Bgra8,
                    levels: vec![img.into_bytes()],
                })
            }
            TextureKey::TerrainSource { picture, texels } => {
                let mut memo = self.tiles.borrow_mut();
                let sources = DatTerrainTextures {
                    textures: TextureStore::with_environment_texture_detail(
                        self.ground_store.unwrap_or(self.store),
                        self.detail,
                    ),
                    memo: RefCell::new(&mut memo),
                };
                let img = sources.image(DataId(*picture))?;
                // Picked by the stride the composite takes, as the composite carries it.
                let shrunk = dereth_terrain::land::merge::source_at_scale(&img, *texels, 1);
                let img = shrunk.as_ref().unwrap_or(&img);
                chain(TextureData {
                    width: img.width,
                    height: img.height,
                    format: TextureFormat::Bgra8,
                    levels: vec![img.pixels.as_flattened().to_vec()],
                })
            }
        }
    }

    fn ground_colors(&mut self) -> Option<[[f32; 4]; 32]> {
        let tm = self.tex_merge.as_ref()?;
        let mut memo = self.tiles.borrow_mut();
        let sources = DatTerrainTextures {
            textures: TextureStore::with_environment_texture_detail(
                self.ground_store.unwrap_or(self.store),
                self.detail,
            ),
            memo: RefCell::new(&mut memo),
        };
        let mut out = [[0.5, 0.5, 0.5, 1.0]; 32];
        for (t, color) in (0u32..).zip(out.iter_mut()) {
            let Some(desc) = dereth_terrain::land::merge::get_terrain_tex(tm, t) else {
                continue;
            };
            let Some(img) = sources.image(desc.tex_gid) else {
                continue;
            };
            let n = img.pixels.len().max(1);
            let mut sum = [0u64; 3];
            for p in &img.pixels {
                // BGRA.
                sum[0] += u64::from(p[2]);
                sum[1] += u64::from(p[1]);
                sum[2] += u64::from(p[0]);
            }
            #[allow(clippy::cast_precision_loss)] // a picture's pixels, at most a few million
            let mean = |s: u64| s as f32 / n as f32 / 255.0;
            *color = [mean(sum[0]), mean(sum[1]), mean(sum[2]), 1.0];
        }
        Some(out)
    }

    fn splat(&mut self, key: u32) -> Option<Splat> {
        let tm = self.tex_merge.as_ref()?;
        let plan = dereth_terrain::land::merge::merge_plan(
            tm,
            dereth_terrain::land::merge::MergeKey(key),
            self.merge_shift,
        );
        // The same layers by terrain type.
        let grounds = dereth_terrain::land::merge::ground_plan(
            tm,
            dereth_terrain::land::merge::MergeKey(key),
        );
        // Each layer at the texels the composite gives each of its repeats.
        let size = plan.size;
        let source = |id: DataId, tiling: u32| TextureKey::TerrainSource {
            picture: id.0,
            texels: (size / tiling.max(1)).max(1),
        };
        let mut splat = Splat {
            base: plan.base.map(|id| source(id, plan.base_tiling)),
            base_tiling: plan.base_tiling,
            base_ground: grounds.base,
            base_maps: None,
            base_broad: None,
            slopes: Vec::new(),
            bank: None,
            overlays: plan
                .overlays
                .iter()
                .enumerate()
                .map(|(k, o)| SplatOverlay {
                    alpha: source(o.alpha, 1),
                    rotation: o.rotation as u8,
                    tile: o.tex.map(|id| source(id, o.tiling)),
                    tiling: o.tiling,
                    ground: grounds.overlays.get(k).and_then(|g| g.ground),
                    maps: None,
                    broad: None,
                })
                .collect(),
        };
        // The pack's tiles in place of the terrain types' it lays, while its ground is drawn, with
        // their normal and surface pictures: retail's alpha maps blend them as they blend
        // retail's.
        if let Some((p, _)) = self.pack.as_ref().filter(|(_, on)| on.ground) {
            if let Some((tile, repeats)) = splat.base_ground.and_then(|g| p.ground_tile(g)) {
                splat.base_broad = splat.base.take().map(|picture| BroadPicture {
                    picture,
                    tiling: splat.base_tiling,
                });
                splat.base = Some(tile);
                splat.base_tiling = repeats;
                splat.base_maps = splat.base_ground.and_then(|g| p.ground_maps(g));
            }
            for o in &mut splat.overlays {
                if let Some((tile, repeats)) = o.ground.and_then(|g| p.ground_tile(g)) {
                    o.broad = o.tile.take().map(|picture| BroadPicture {
                        picture,
                        tiling: o.tiling,
                    });
                    o.tile = Some(tile);
                    o.tiling = repeats;
                    o.maps = o.ground.and_then(|g| p.ground_maps(g));
                }
            }
            let grounds: Vec<u32> = std::iter::once(splat.base_ground)
                .chain(splat.overlays.iter().map(|o| o.ground))
                .flatten()
                .collect();
            splat.bank = p.bank_of(&grounds);
            if self.pack.as_ref().is_some_and(|(_, on)| on.slopes) {
                splat.slopes = std::iter::once(splat.base_ground)
                    .chain(splat.overlays.iter().map(|o| o.ground))
                    .map(|g| g.map(|g| p.slopes_of(g)).unwrap_or_default())
                    .collect();
            }
        }
        Some(splat)
    }

    fn sky_objects(&mut self) -> Vec<u32> {
        // Every sky object's own GfxObj or Setup, and every one a time of day can put in its
        // place, as the scene's sky loads them.
        let mut out = std::collections::BTreeSet::new();
        let Some(sky) = self.sky_region.as_ref().and_then(|r| r.sky_info.as_ref()) else {
            return Vec::new();
        };
        for g in &sky.day_groups {
            for o in &g.sky_objects {
                if o.default_gfx_object.0 != 0 {
                    out.insert(o.default_gfx_object.0);
                }
            }
            for t in &g.sky_time {
                for r in &t.sky_obj_replace {
                    if r.gfx_obj_id.0 != 0 {
                        out.insert(r.gfx_obj_id.0);
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    fn pack_model(&mut self, id: u32) -> Option<dereth_remaster::assets::pack::PackModel> {
        let (p, on) = self.pack.as_ref()?;
        p.model(id, *on)
    }

    fn sky(&mut self, id: u32) -> Option<Mesh> {
        // A sky object never moves its parts, so each part's place is folded into its vertices,
        // as the scene's sky is built.
        let mut groups = Vec::new();
        for part in dereth_client_runtime::models::resolve_parts(self.store, DataId(id)) {
            let rot = dereth_terrain::math::l2g(part.frame.rotation);
            for mut g in dereth_client_runtime::models::build_gfxobj(self.store, part.gfxobj) {
                for v in &mut g.vertices {
                    v.0 = dereth_terrain::math::localtoglobal(&part.frame, v.0);
                }
                for n in &mut g.normals {
                    *n = dereth_terrain::math::localtoglobalvec(rot, *n);
                }
                groups.push(g);
            }
        }
        if groups.is_empty() {
            return None;
        }
        Some(mesh_of(&groups, |s| {
            s.map_or(SurfaceRef::Missing, |s| SurfaceRef::Surface(s.0))
        }))
    }

    fn terrain_cells(&mut self, block: BlockId, terrain: &Terrain) -> Vec<TerrainCell> {
        let Some(region) = self.region.as_ref() else {
            return Vec::new();
        };
        let lb = CellLandblock {
            id: dereth_world_data::landblock::landblock_did(block),
            lbi_exists: 0,
            terrain: terrain.words,
            height: [0; 81],
        };
        // A vertex whose terrain the land surface has no picture for is keyed as its
        // neighbours are, as the block's mesh is.
        let filled = dereth_terrain::land::fill::fill_undrawn_terrain(&lb, self.drawn);
        let lb = filled.as_ref().unwrap_or(&lb);
        let side = usize::from(terrain.side);
        let mut out = Vec::with_capacity(side * side);
        for i in 0..side {
            for j in 0..side {
                let (key, rotation, _) =
                    dereth_terrain::land::merge::cell_rotation(lb, region, side, i, j);
                out.push(TerrainCell {
                    texture: TextureKey::Terrain(key.0),
                    rotation: u8::try_from(rotation.index()).unwrap_or(0),
                    kind: kind::of_ground(dereth_remaster::assets::terrain::cell_corners(
                        &terrain.words,
                        side,
                        i,
                        j,
                    )),
                });
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "remaster_assets_tests.rs"]
mod tests;
