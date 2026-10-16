//! Art packs: models for retail objects, drawn by the high-fidelity presentation in place of the
//! objects they replace, and the one lookup that says which look draws an object.
//!
//! **Which look draws an object** ([`Looks::of`]): the art pack's model, when a pack is drawn and
//! has one for the object; otherwise the classic look's, when that look is chosen and stands for
//! the object; otherwise the world's own. Only pixels follow from it. The object's collision, its
//! physics and everything the server sees are the world's whichever look draws it.
//!
//! **A pack is a folder** ([`ArtPack::open_dir`]): `manifest.toml` at its top
//! ([`dereth_assets::art_pack`]), naming glTF 2.0 models by path, with their pictures beside them.
//! Everything is read through one function of a path ([`ArtPack::open`]), so a pack packed into a
//! data file later is read the same way, each path a record. A model that will not read is left
//! out with a line saying why, and the rest of the pack is drawn.
//!
//! **A pack may lay the ground** ([`PackGround`]): a material for each retail terrain type it
//! covers, its pictures packed three to a material and brought to one size, so the presentation
//! holds them all in three arrays.
//!
//! **A pack may draw its own pictures in place of retail ones** ([`TextureMaterial`]): each
//! material names the retail pictures it stands for, and wherever a retail object or room is
//! drawn with one of them, the presentation draws it with the material on the object's own
//! coordinates and surface flags.
//!
//! **Pictures are compressed as they are read** ([`images::compressed`]): a model's pictures in
//! BC7, a ground material's colour and height in BC7, its normal in BC5, and its roughness and
//! occlusion in BC5 at half its size, a quarter of the memory or less of eight bits a channel.
//! A model's picture whose sides are not whole blocks of four is first stretched to the next
//! whole block on each side.

pub mod gltf;
pub mod images;
pub mod materials;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dereth_assets::art_pack::{ArtPackManifest, Credit, ManifestGround, MANIFEST_FILE};

pub use dereth_assets::art_pack::Ground;
use dereth_primitives::{DataId, Frame};

pub use gltf::VERTEX_FLOATS;
pub use images::Image;
pub use materials::TextureMaterial;

/// Which look draws an outdoor object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Look {
    /// The art pack's model, by the high-fidelity presentation.
    ArtPack,
    /// The classic look: another era's record of the object.
    Classic,
    /// The world's own record.
    Retail,
}

/// The looks an outdoor object may be drawn with, each asked in turn.
#[derive(Clone, Copy)]
pub struct Looks<'a> {
    /// Whether the art pack draws a model for an object, while a pack is drawn.
    pub pack: Option<&'a dyn Fn(DataId) -> bool>,
    /// Whether the classic look stands for an object (`true` for a building shell), while that
    /// look is chosen.
    pub classic: Option<&'a dyn Fn(DataId, bool) -> bool>,
}

impl std::fmt::Debug for Looks<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Looks")
            .field("pack", &self.pack.is_some())
            .field("classic", &self.classic.is_some())
            .finish()
    }
}

impl Looks<'_> {
    /// The look that draws the object `id`, `building` for a building shell.
    #[must_use]
    pub fn of(&self, id: DataId, building: bool) -> Look {
        if self.pack.is_some_and(|has| has(id)) {
            Look::ArtPack
        } else if self.classic.is_some_and(|takes| takes(id, building)) {
            Look::Classic
        } else {
            Look::Retail
        }
    }
}

/// One outdoor object the art pack has a model for, where it stands in its block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackPlacement {
    /// The retail object.
    pub id: DataId,
    /// Its place and turn, block-local.
    pub frame: Frame,
    /// Its scale, the same on every axis.
    pub scale: f32,
}

/// A metallic-roughness material, its pictures by index into [`ArtPack::images`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackMaterial {
    /// The base colour factor, linear, with alpha.
    pub base_color: [f32; 4],
    /// The base colour picture.
    pub base_color_image: Option<u32>,
    /// The tangent-space normal picture.
    pub normal_image: Option<u32>,
    /// The normal picture's strength.
    pub normal_scale: f32,
    /// The metallic-roughness picture.
    pub metal_rough_image: Option<u32>,
    /// The metalness factor.
    pub metallic: f32,
    /// The roughness factor.
    pub roughness: f32,
    /// The occlusion picture.
    pub occlusion_image: Option<u32>,
    /// The occlusion's strength.
    pub occlusion_strength: f32,
    /// The alpha a pixel is cut below, for a cut-out material.
    pub alpha_cutoff: Option<f32>,
    /// Whether both faces are drawn.
    pub double_sided: bool,
}

/// One part of a level of detail: triangles of one material.
#[derive(Debug, Clone, PartialEq)]
pub struct PackPrimitive {
    /// [`VERTEX_FLOATS`] floats a vertex, model space, height up.
    pub vertices: Arc<Vec<f32>>,
    /// Three a triangle, counter-clockwise seen from the front.
    pub indices: Arc<Vec<u32>>,
    /// How it is drawn.
    pub material: PackMaterial,
}

/// One level of detail.
#[derive(Debug, Clone, PartialEq)]
pub struct PackLod {
    /// How far from the eye it is drawn, metres; infinite for a last level with no limit.
    pub distance: f32,
    /// Its parts.
    pub primitives: Vec<PackPrimitive>,
    /// For a level that is pictures of the model seen from round it, how they are laid out
    /// ([`dereth_assets::art_pack`] says how such a level is drawn).
    pub views: Option<dereth_assets::art_pack::Views>,
    /// Drawn into the sun's shadows alone, never into the view.
    pub shadows_only: bool,
}

/// One model of a pack.
#[derive(Debug, Clone, PartialEq)]
pub struct PackModel {
    /// The retail object it is drawn in place of.
    pub id: DataId,
    /// Cut-out growth: its cut-out materials (needles, leaves) are drawn from both sides and lit
    /// through from behind.
    pub foliage: bool,
    /// Its levels of detail, nearest first.
    pub lods: Vec<PackLod>,
    /// A sphere holding every level, model space: centre and radius.
    pub bounds: ([f32; 3], f32),
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// One ground material of a pack, its pictures packed for drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct PackGround {
    /// The retail grounds it is drawn for.
    pub grounds: Vec<Ground>,
    /// The base colour, in the display's encoding, with the height in alpha.
    pub albedo: Image,
    /// The normal's east and north parts (-1..1 as 0..1), in red and green, the base colour's
    /// size.
    pub normal: Image,
    /// The roughness and the ambient occlusion, in red and green, half the base colour's size.
    pub surface: Image,
    /// How many metres one repeat of the pictures covers.
    pub size: f32,
    /// How strongly the normal picture bends the normal.
    pub normal_scale: f32,
    /// Who made it, with what, and under which license.
    pub credit: Credit,
}

/// What a ground material takes where it names no picture: a level height, a flat normal, a
/// rough surface and no occlusion.
const GROUND_DEFAULTS: [u8; 4] = [128, 128, 230, 255];

/// A ground's three pictures from its base colour with the height in alpha (`albedo`), its
/// normal in red and green (`normal`), both `side` texels a side, and its roughness and
/// occlusion in red and green (`surface`), at half that, with their levels and compressed.
///
/// The base colour's smaller levels are averaged in the display's encoding, as retail's ground
/// pictures are, so a material far off takes the tone retail's picture takes there.
fn ground_pictures(side: u32, albedo: Vec<u8>, normal: Vec<u8>, surface: &[u8]) -> [Image; 3] {
    let half = (side / 2).max(1);
    let surface = resampled(side, side, surface, half);
    let albedo = Image {
        srgb: true,
        ..images::with_levels(side, side, albedo, false, None)
    };
    [
        images::compressed(albedo, images::BlockFormat::Bc7),
        images::compressed(
            images::with_levels(side, side, normal, false, None),
            images::BlockFormat::Bc5,
        ),
        images::compressed(
            images::with_levels(half, half, surface, false, None),
            images::BlockFormat::Bc5,
        ),
    ]
}

/// `rgba` (`w` by `h`) resampled to `size` by `size`, wrapping at its edges (a ground picture
/// repeats), each texel the bilinear mix of the four nearest.
fn resampled(w: u32, h: u32, rgba: &[u8], size: u32) -> Vec<u8> {
    if (w, h) == (size, size) {
        return rgba.to_vec();
    }
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let n = size.max(1) as usize;
    let mut out = Vec::with_capacity(n * n * 4);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // Texel coordinates of pictures far smaller than a float's exact range.
    for y in 0..n {
        let fy = ((y as f32 + 0.5) * h as f32 / n as f32 - 0.5).rem_euclid(h as f32);
        let (y0, ty) = (fy.floor() as usize % h, fy.fract());
        let y1 = (y0 + 1) % h;
        for x in 0..n {
            let fx = ((x as f32 + 0.5) * w as f32 / n as f32 - 0.5).rem_euclid(w as f32);
            let (x0, tx) = (fx.floor() as usize % w, fx.fract());
            let x1 = (x0 + 1) % w;
            for c in 0..4 {
                let at = |xx: usize, yy: usize| f32::from(rgba[(yy * w + xx) * 4 + c]);
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
                let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
                out.push((top + (bottom - top) * ty + 0.5).clamp(0.0, 255.0) as u8);
            }
        }
    }
    out
}

/// The files of one ground material, as read.
struct GroundFiles {
    base: Vec<u8>,
    normal: Option<Vec<u8>>,
    height: Option<Vec<u8>>,
    roughness: Option<Vec<u8>>,
    occlusion: Option<Vec<u8>>,
}

impl GroundFiles {
    /// The files `entry` names, read through `read`.
    fn read(
        entry: &ManifestGround,
        read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Self, String> {
        let base = read(&entry.base_color)?;
        let mut channel = |path: &Option<String>| path.as_deref().map(&mut *read).transpose();
        Ok(Self {
            base,
            normal: channel(&entry.normal)?,
            height: channel(&entry.height)?,
            roughness: channel(&entry.roughness)?,
            occlusion: channel(&entry.occlusion)?,
        })
    }
}

/// The ground material `entry` from its `files`, its pictures brought to `size` by `size` texels
/// (its base colour's own size when `size` is `None`).
fn ground(
    entry: &ManifestGround,
    files: &GroundFiles,
    size: Option<u32>,
) -> Result<PackGround, String> {
    let picture = |bytes: &[u8], path: &str| -> Result<(u32, u32, Vec<u8>), String> {
        images::decode_png(bytes).map_err(|e| format!("{path}: {e}"))
    };
    let (w, h, base) = picture(&files.base, &entry.base_color)?;
    if w == 0 || h == 0 {
        return Err(format!("{}: an empty picture", entry.base_color));
    }
    let side = size.unwrap_or(w.max(h));
    let base = resampled(w, h, &base, side);
    let channel =
        |bytes: &Option<Vec<u8>>, path: &Option<String>| -> Result<Option<Vec<u8>>, String> {
            let (Some(bytes), Some(path)) = (bytes, path) else {
                return Ok(None);
            };
            let (w, h, rgba) = picture(bytes, path)?;
            Ok(Some(resampled(w, h, &rgba, side)))
        };
    let normal = channel(&files.normal, &entry.normal)?;
    let height = channel(&files.height, &entry.height)?;
    let roughness = channel(&files.roughness, &entry.roughness)?;
    let occlusion = channel(&files.occlusion, &entry.occlusion)?;
    let texels = (side as usize) * (side as usize);
    let mut albedo = Vec::with_capacity(texels * 4);
    let mut bent = Vec::with_capacity(texels * 4);
    let mut surface = Vec::with_capacity(texels * 4);
    let red = |p: &Option<Vec<u8>>, i: usize, c: usize, default: u8| {
        p.as_ref().map_or(default, |p| p[i * 4 + c])
    };
    for i in 0..texels {
        albedo.extend([
            base[i * 4],
            base[i * 4 + 1],
            base[i * 4 + 2],
            red(&height, i, 0, GROUND_DEFAULTS[0]),
        ]);
        bent.extend([
            red(&normal, i, 0, GROUND_DEFAULTS[0]),
            red(&normal, i, 1, GROUND_DEFAULTS[1]),
            0,
            255,
        ]);
        surface.extend([
            red(&roughness, i, 0, GROUND_DEFAULTS[2]),
            red(&occlusion, i, 0, GROUND_DEFAULTS[3]),
            0,
            255,
        ]);
    }
    let [albedo, normal, surface] = ground_pictures(side, albedo, bent, &surface);
    Ok(PackGround {
        grounds: entry.grounds.clone(),
        albedo,
        normal,
        surface,
        size: entry.size,
        normal_scale: entry.normal_scale,
        credit: entry.credit.clone(),
    })
}

/// The terrain types drawn as water, whose retail pictures keep a wet sheen when the pack has no
/// material for them.
const WATER: [u32; 6] = [16, 17, 18, 19, 20, 22];

/// The world's own ground an art pack's ground is laid with: retail's blend masks, and retail's
/// pictures for the grounds the pack has no material for, each brought to the size of the pack's
/// materials. Read from the files when a pack is set or the ground changes.
#[derive(Debug, Clone, PartialEq)]
pub struct RetailGround {
    /// A number of its own, so a ground read again is told from the one before.
    pub serial: u64,
    /// The masks, each with its id, in the order the cells' layers name them: the red keeps
    /// what lies under a layer (255) or takes the layer (0), compressed in BC4.
    pub masks: Vec<(DataId, Image)>,
    /// Retail's pictures for the grounds the pack has no material for: the grounds each is drawn
    /// for (terrain types, 32 the road), the picture, its repeats across a cell, and a surface
    /// picture of a level, unbent ground (wet for water).
    pub pictures: Vec<RetailPicture>,
    /// Each retail ground's own picture shrunk to [`BROAD_SIZE`] texels a side (terrain types 0 to
    /// 31, then the road), with its repeats across a cell: its patches of light and dark and of
    /// colour, broader than a fifth of a metre, which the pack's materials are laid out in so they
    /// keep the character of the ground they replace.
    pub broad: Vec<(Image, u32)>,
}

/// A retail picture as read: its width, height and texels, and the same with its levels.
type Decoded = (u32, u32, Vec<u8>, Image);

/// The texels a side of a ground's broad picture ([`RetailGround::broad`]).
pub const BROAD_SIZE: u32 = 64;

/// `image`'s level `size` texels wide, as a picture of its own with its smaller levels; a plain
/// grey picture where it has none.
fn broad_level(image: Option<&Image>, size: u32) -> Image {
    let level = image.and_then(|i| {
        let k = i
            .levels
            .iter()
            .position(|l| l.len() == (size as usize) * (size as usize) * 4)?;
        Some(i.levels[k].as_ref().clone())
    });
    let rgba =
        level.unwrap_or_else(|| [128, 128, 128, 255].repeat((size as usize) * (size as usize)));
    images::with_levels(size, size, rgba, true, None)
}

/// One of retail's ground pictures, laid where the pack has no material, in the pictures of a
/// pack's material ([`PackGround`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RetailPicture {
    /// The retail grounds it is drawn for: terrain types, 32 the road.
    pub grounds: Vec<u8>,
    /// The picture, with a level height in alpha.
    pub albedo: Image,
    /// A flat normal.
    pub normal: Image,
    /// Its roughness and no occlusion.
    pub surface: Image,
    /// Its repeats across a cell.
    pub tiling: u32,
}

impl RetailGround {
    /// The ground `pack` is laid with over the texture-merge land surface `tm`, its pictures read
    /// through `picture` (four bytes a texel, red first).
    #[must_use]
    pub fn of(
        tm: &dereth_assets::region::TexMerge,
        pack: &ArtPack,
        picture: &mut dyn FnMut(DataId) -> Option<(u32, u32, Vec<u8>)>,
    ) -> Self {
        use dereth_terrain::consts::ROAD_TERRAIN_TYPE;
        use dereth_terrain::land::merge::get_terrain_tex;
        let mut masks: Vec<(DataId, Image)> = Vec::new();
        for m in tm
            .corner_terrain_maps
            .iter()
            .chain(&tm.side_terrain_maps)
            .chain(&tm.road_maps)
        {
            if masks.iter().any(|(id, _)| *id == m.tex_gid) {
                continue;
            }
            if let Some((w, h, rgba)) = picture(m.tex_gid) {
                let alpha = rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|t| [t[3], 0, 0, 255])
                    .collect();
                masks.push((
                    m.tex_gid,
                    images::compressed(
                        images::with_levels(w, h, alpha, false, None),
                        images::BlockFormat::Bc4,
                    ),
                ));
            }
        }
        let size = pack.grounds.first().map_or(512, |g| g.albedo.width);
        // Each retail picture once, decoded with its levels.
        let mut decoded: Vec<(DataId, Option<Decoded>)> = Vec::new();
        let mut broad = Vec::with_capacity(33);
        for g in 0..=ROAD_TERRAIN_TYPE {
            let desc = get_terrain_tex(tm, g);
            let got = desc.and_then(|d| {
                if !decoded.iter().any(|(id, _)| *id == d.tex_gid) {
                    let p = picture(d.tex_gid).map(|(w, h, rgba)| {
                        let i = images::with_levels(w, h, rgba.clone(), true, None);
                        (w, h, rgba, i)
                    });
                    decoded.push((d.tex_gid, p));
                }
                decoded
                    .iter()
                    .find(|(id, _)| *id == d.tex_gid)
                    .and_then(|(_, p)| p.as_ref())
            });
            broad.push((
                broad_level(got.map(|(_, _, _, i)| i), BROAD_SIZE),
                desc.map_or(1, |d| d.tex_tiling.max(1)),
            ));
        }
        // Each picture the pack has no material for, once, with the grounds it is drawn for and
        // whether the first of them is water.
        let mut wanted: Vec<(DataId, Vec<u8>, u32, bool)> = Vec::new();
        for g in 0..=ROAD_TERRAIN_TYPE {
            let ground = if g == ROAD_TERRAIN_TYPE {
                Ground::Road
            } else {
                // LINT-OK: a terrain type, 0 to 31.
                #[allow(clippy::cast_possible_truncation)]
                Ground::Type(g as u8)
            };
            if pack.ground(ground).is_some() {
                continue;
            }
            let Some(desc) = get_terrain_tex(tm, g) else {
                continue;
            };
            // LINT-OK: a terrain type or the road, 0 to 32.
            #[allow(clippy::cast_possible_truncation)]
            let number = g as u8;
            if let Some((_, grounds, _, _)) = wanted.iter_mut().find(|(id, ..)| *id == desc.tex_gid)
            {
                grounds.push(number);
                continue;
            }
            wanted.push((
                desc.tex_gid,
                vec![number],
                desc.tex_tiling.max(1),
                WATER.contains(&g),
            ));
        }
        let pictures = images::par_map(wanted, &|(id, grounds, tiling, water)| {
            let (w, h, rgba, _) = decoded
                .iter()
                .find(|(d, _)| *d == id)
                .and_then(|(_, p)| p.as_ref())?;
            let mut albedo = resampled(*w, *h, rgba, size);
            for t in albedo.as_chunks_mut::<4>().0 {
                t[3] = 128;
            }
            let roughness = if water { 100 } else { 220 };
            let texels = (size as usize) * (size as usize);
            let flat = [128, 128, 0, 255].repeat(texels);
            let surface = [roughness, 255, 0, 255].repeat(texels);
            let [albedo, normal, surface] = ground_pictures(size, albedo, flat, &surface);
            Some(RetailPicture {
                grounds,
                albedo,
                normal,
                surface,
                tiling,
            })
        });
        Self {
            serial: next_serial(),
            masks,
            pictures: pictures.into_iter().flatten().collect(),
            broad,
        }
    }

    /// The index the cells' layers name mask `id` by.
    #[must_use]
    pub fn mask(&self, id: DataId) -> Option<u8> {
        self.masks
            .iter()
            .position(|(m, _)| *m == id)
            .and_then(|i| u8::try_from(i).ok())
    }
}

/// An open art pack: its models and their pictures, decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtPack {
    /// The pack's name.
    pub name: String,
    /// A number of its own, so a pack opened again is told from the one before.
    pub serial: u64,
    /// The models that read.
    pub models: Vec<PackModel>,
    /// The pictures the materials name.
    pub images: Vec<Image>,
    /// The ground materials that read, all their pictures one size.
    pub grounds: Vec<PackGround>,
    /// The materials for retail pictures that read.
    pub materials: Vec<TextureMaterial>,
    /// What would not read, a line each.
    pub problems: Vec<String>,
}

/// Why a pack could not be opened at all, as a sentence.
pub type OpenError = String;

/// A picture of a pack as read, to be decoded.
struct PendingPicture {
    path: String,
    bytes: Vec<u8>,
    srgb: bool,
    cutoff: Option<f32>,
}

/// The next pack's serial.
fn next_serial() -> u64 {
    static SERIAL: AtomicU64 = AtomicU64::new(1);
    SERIAL.fetch_add(1, Ordering::Relaxed)
}

/// A sphere holding every vertex of `lods`.
fn bounds(lods: &[PackLod]) -> ([f32; 3], f32) {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    let positions = || {
        lods.iter()
            .flat_map(|l| &l.primitives)
            .flat_map(|p| p.vertices.as_chunks::<VERTEX_FLOATS>().0.iter())
    };
    for v in positions() {
        for c in 0..3 {
            lo[c] = lo[c].min(v[c]);
            hi[c] = hi[c].max(v[c]);
        }
    }
    if lo[0] > hi[0] {
        return ([0.0; 3], 0.0);
    }
    let centre = [0, 1, 2].map(|c| (lo[c] + hi[c]) * 0.5);
    let radius = positions()
        .map(|v| {
            let d = [0, 1, 2].map(|c| v[c] - centre[c]);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .fold(0.0f32, f32::max);
    (centre, radius)
}

impl ArtPack {
    /// The pack in the folder `dir`.
    ///
    /// # Errors
    /// The manifest will not read, or names no model that reads.
    pub fn open_dir(dir: &Path) -> Result<Self, OpenError> {
        let manifest = std::fs::read_to_string(dir.join(MANIFEST_FILE))
            .map_err(|e| format!("{}: {e}", dir.join(MANIFEST_FILE).display()))?;
        Self::open(&manifest, &mut |path| {
            std::fs::read(dir.join(path)).map_err(|e| format!("{path}: {e}"))
        })
    }

    /// The pack whose manifest is `manifest`, every file it names read through `read` by its
    /// path relative to the pack.
    ///
    /// # Errors
    /// The manifest will not read, or names no model that reads.
    pub fn open(
        manifest: &str,
        read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<Self, OpenError> {
        let manifest = ArtPackManifest::parse(manifest).map_err(|e| e.to_string())?;
        let mut problems = Vec::new();
        let mut models = Vec::new();
        // Each picture once, by path and whether it holds colour, read here and decoded below.
        let mut pending: Vec<PendingPicture> = Vec::new();
        let mut by_path: HashMap<(String, bool), u32> = HashMap::new();
        for entry in &manifest.models {
            let label = gltf::label(entry.id);
            let mut lods = Vec::with_capacity(entry.lods.len());
            let mut failed = None;
            for lod in &entry.lods {
                let read_model = |read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>| {
                    let json = read(&lod.file)?;
                    gltf::decode(&json, &lod.file, read)
                };
                match read_model(read) {
                    Ok(primitives) => lods.push((lod, primitives)),
                    Err(e) => {
                        failed = Some(e);
                        break;
                    }
                }
            }
            if let Some(e) = failed {
                problems.push(format!("{label}: {e}"));
                continue;
            }
            let mut picture = |path: &Option<String>,
                               srgb: bool,
                               cutoff: Option<f32>|
             -> Result<Option<u32>, String> {
                let Some(path) = path else {
                    return Ok(None);
                };
                if let Some(i) = by_path.get(&(path.clone(), srgb)) {
                    return Ok(Some(*i));
                }
                let bytes = read(path)?;
                let i = u32::try_from(pending.len()).map_err(|_| "too many pictures".to_owned())?;
                pending.push(PendingPicture {
                    path: path.clone(),
                    bytes,
                    srgb,
                    cutoff,
                });
                by_path.insert((path.clone(), srgb), i);
                Ok(Some(i))
            };
            let mut built = Vec::with_capacity(lods.len());
            let mut broken = None;
            for (lod, primitives) in lods {
                let mut parts = Vec::with_capacity(primitives.len());
                for p in primitives {
                    let m = &p.material;
                    let material = (|| -> Result<PackMaterial, String> {
                        Ok(PackMaterial {
                            base_color: m.base_color,
                            base_color_image: picture(&m.base_color_image, true, m.alpha_cutoff)?,
                            normal_image: picture(&m.normal_image, false, None)?,
                            normal_scale: m.normal_scale,
                            metal_rough_image: picture(&m.metal_rough_image, false, None)?,
                            metallic: m.metallic,
                            roughness: m.roughness,
                            occlusion_image: picture(&m.occlusion_image, false, None)?,
                            occlusion_strength: m.occlusion_strength,
                            alpha_cutoff: m.alpha_cutoff,
                            double_sided: m.double_sided,
                        })
                    })();
                    match material {
                        Ok(material) => parts.push(PackPrimitive {
                            vertices: Arc::new(p.vertices),
                            indices: Arc::new(p.indices),
                            material,
                        }),
                        Err(e) => {
                            broken = Some(e);
                            break;
                        }
                    }
                }
                if broken.is_some() {
                    break;
                }
                built.push(PackLod {
                    distance: lod.distance.unwrap_or(f32::INFINITY),
                    primitives: parts,
                    views: lod.views,
                    shadows_only: lod.shadows_only,
                });
            }
            if let Some(e) = broken {
                problems.push(format!("{label}: {e}"));
                continue;
            }
            models.push(PackModel {
                id: entry.id,
                foliage: entry.foliage,
                bounds: bounds(&built),
                lods: built,
                credit: entry.credit.clone(),
            });
        }
        // The pictures decoded, given their levels and compressed, on every thread at once; a
        // model whose picture will not decode is left out.
        let decoded = images::par_map(pending, &|p| {
            let (w, h, rgba) =
                images::decode_png(&p.bytes).map_err(|e| format!("{}: {e}", p.path))?;
            let (w, h, rgba) = images::to_whole_blocks(w, h, rgba);
            Ok::<_, String>(images::compressed(
                images::with_levels(w, h, rgba, p.srgb, p.cutoff),
                images::BlockFormat::Bc7,
            ))
        });
        let failed: HashMap<u32, String> = (0u32..)
            .zip(&decoded)
            .filter_map(|(i, d)| d.as_ref().err().map(|e| (i, e.clone())))
            .collect();
        models.retain(|m: &PackModel| {
            let Some(e) = m
                .lods
                .iter()
                .flat_map(|l| &l.primitives)
                .flat_map(|p| {
                    let m = &p.material;
                    [
                        m.base_color_image,
                        m.normal_image,
                        m.metal_rough_image,
                        m.occlusion_image,
                    ]
                })
                .flatten()
                .find_map(|i| failed.get(&i))
            else {
                return true;
            };
            problems.push(format!("{}: {e}", gltf::label(m.id)));
            false
        });
        let images: Vec<Image> = decoded
            .into_iter()
            .map(|d| d.unwrap_or_else(|_| images::with_levels(1, 1, vec![0; 4], false, None)))
            .collect();
        if models.is_empty() && !manifest.models.is_empty() {
            return Err(format!(
                "no model of the pack would read: {}",
                problems.join("; ")
            ));
        }
        // Every ground at the first one's size, so they are drawn from one array: the files read
        // here, the pictures made on every thread at once.
        let files: Vec<(&ManifestGround, Result<GroundFiles, String>)> = manifest
            .grounds
            .iter()
            .map(|entry| (entry, GroundFiles::read(entry, read)))
            .collect();
        let side = files.iter().find_map(|(_, f)| {
            let (w, h) = images::png_size(&f.as_ref().ok()?.base).ok()?;
            Some(w.max(h))
        });
        let made = images::par_map(files, &|(entry, f)| {
            (entry, f.and_then(|f| ground(entry, &f, side)))
        });
        let mut grounds: Vec<PackGround> = Vec::with_capacity(made.len());
        for (entry, g) in made {
            match g {
                Ok(g) => grounds.push(g),
                Err(e) => problems.push(format!("ground {:?}: {e}", entry.grounds)),
            }
        }
        // The materials for retail pictures: the files read here, the pictures made on every
        // thread at once.
        let files: Vec<_> = manifest
            .materials
            .iter()
            .map(|entry| (entry, materials::MaterialFiles::read(entry, read)))
            .collect();
        let made = images::par_map(files, &|(entry, f)| {
            (entry, f.and_then(|f| materials::material(entry, &f)))
        });
        let mut retextured: Vec<TextureMaterial> = Vec::with_capacity(made.len());
        for (entry, m) in made {
            match m {
                Ok(m) => retextured.push(m),
                Err(e) => problems.push(format!(
                    "material {}: {e}",
                    entry
                        .textures
                        .iter()
                        .map(|t| format!("{:#010X}", t.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
        Ok(Self {
            name: manifest.name,
            serial: next_serial(),
            models,
            images,
            grounds,
            materials: retextured,
            problems,
        })
    }

    /// Whether the pack has a model for `id`.
    #[must_use]
    pub fn has(&self, id: DataId) -> bool {
        self.models.iter().any(|m| m.id == id)
    }

    /// The index of the pack's material for the retail picture `texture`, if it has one.
    #[must_use]
    pub fn material_of(&self, texture: DataId) -> Option<usize> {
        self.materials
            .iter()
            .position(|m| m.textures.contains(&texture))
    }

    /// The pack's material for `ground`, if it has one.
    #[must_use]
    pub fn ground(&self, ground: Ground) -> Option<&PackGround> {
        self.grounds.iter().find(|g| g.grounds.contains(&ground))
    }

    /// How many triangles every level of every model holds.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.models
            .iter()
            .flat_map(|m| &m.lods)
            .flat_map(|l| &l.primitives)
            .map(|p| p.indices.len() / 3)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    #[test]
    fn the_art_pack_answers_first_then_the_classic_look_then_the_world() {
        let tree = DataId(0x0200_0258);
        let rock = DataId(0x0200_05C9);
        let house = DataId(0x0200_0001);
        let pack_has = |id: DataId| id == tree;
        let classic_takes = |id: DataId, building: bool| id == rock || (id == house && building);
        let both = Looks {
            pack: Some(&pack_has),
            classic: Some(&classic_takes),
        };
        assert_eq!(both.of(tree, false), Look::ArtPack);
        assert_eq!(both.of(rock, false), Look::Classic);
        assert_eq!(both.of(house, true), Look::Classic);
        assert_eq!(both.of(house, false), Look::Retail);
        // The pack off (the retail look asked for, or no presentation that draws it): the
        // classic look and the world answer as they do without a pack.
        let no_pack = Looks {
            pack: None,
            classic: Some(&classic_takes),
        };
        assert_eq!(no_pack.of(tree, false), Look::Retail);
        assert_eq!(no_pack.of(rock, false), Look::Classic);
        // No classic look chosen.
        let pack_only = Looks {
            pack: Some(&pack_has),
            classic: None,
        };
        assert_eq!(pack_only.of(tree, false), Look::ArtPack);
        assert_eq!(pack_only.of(rock, false), Look::Retail);
        let neither = Looks {
            pack: None,
            classic: None,
        };
        assert_eq!(neither.of(tree, false), Look::Retail);
    }

    /// A pack of one model with two levels, a triangle each, its picture shared.
    fn files() -> HashMap<String, Vec<u8>> {
        let mut bin = Vec::new();
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 3.0, 0.0]] {
            for c in p {
                bin.extend(c.to_le_bytes());
            }
        }
        let gltf = r#"{"asset": {"version": "2.0"}, "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "material": 0}]}],
            "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
            "textures": [{"source": 0}], "images": [{"uri": "bark.png"}],
            "buffers": [{"uri": "tri.bin", "byteLength": 36}],
            "bufferViews": [{"buffer": 0, "byteLength": 36}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}]}"#;
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(&[200, 100, 50, 255]).expect("data");
        }
        let mut f = HashMap::new();
        f.insert("tree/lod0.gltf".to_owned(), gltf.as_bytes().to_vec());
        f.insert("tree/lod1.gltf".to_owned(), gltf.as_bytes().to_vec());
        f.insert("tree/tri.bin".to_owned(), bin);
        f.insert("tree/bark.png".to_owned(), png_bytes);
        f
    }

    const MANIFEST: &str = r#"
[pack]
name = "Test"
format = 1
[[model]]
id = "0x02000258"
foliage = true
author = "a"
tool = "t"
license = "CC0-1.0"
[[model.lod]]
file = "tree/lod0.gltf"
distance = 30
[[model.lod]]
file = "tree/lod1.gltf"
[[model]]
id = "0x020005C9"
author = "a"
tool = "t"
license = "CC0-1.0"
[[model.lod]]
file = "rock/missing.gltf"
"#;

    #[test]
    fn a_pack_reads_its_models_through_one_function_of_a_path_and_leaves_out_what_will_not_read() {
        let files = files();
        let mut asked: Vec<String> = Vec::new();
        let pack = ArtPack::open(MANIFEST, &mut |path| {
            asked.push(path.to_owned());
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens");
        assert_eq!(pack.name, "Test");
        assert!(pack.has(DataId(0x0200_0258)));
        assert!(!pack.has(DataId(0x0200_05C9)), "its model would not read");
        assert_eq!(pack.problems.len(), 1);
        assert!(
            pack.problems[0].contains("0x020005C9"),
            "{:?}",
            pack.problems
        );
        let tree = &pack.models[0];
        assert!(tree.foliage);
        assert_eq!(
            tree.lods.iter().map(|l| l.distance).collect::<Vec<_>>(),
            [30.0, f32::INFINITY]
        );
        // Both levels name the same picture: it is read once.
        assert_eq!(pack.images.len(), 1);
        assert_eq!(asked.iter().filter(|p| *p == "tree/bark.png").count(), 1);
        assert_eq!(
            tree.lods[1].primitives[0].material.base_color_image,
            Some(0)
        );
        assert!(pack.images[0].srgb);
        // One texel: stretched to a whole block and compressed.
        assert_eq!(pack.images[0].format, Some(images::BlockFormat::Bc7));
        assert_eq!((pack.images[0].width, pack.images[0].height), (4, 4));
        assert_eq!(pack.triangles(), 2);
        // The triangle stands three metres high, upright.
        let (centre, radius) = tree.bounds;
        assert!((centre[2] - 1.5).abs() < 1e-6, "{centre:?}");
        assert!(radius > 1.5 && radius < 1.6, "{radius}");
        // A manifest whose every model fails opens nothing.
        let none = ArtPack::open(
            &MANIFEST.replace("tree/lod0.gltf", "nowhere.gltf"),
            &mut |p| {
                if p.starts_with("tree/") {
                    files.get(p).cloned().ok_or_else(|| p.to_owned())
                } else {
                    Err(format!("{p}: missing"))
                }
            },
        );
        assert!(none.is_err());
        // A model whose picture will not decode is left out with a line naming the picture.
        let mut broken = files.clone();
        broken.insert("tree/bark.png".to_owned(), b"not a png".to_vec());
        let e = ArtPack::open(MANIFEST, &mut |p| {
            broken
                .get(p)
                .cloned()
                .ok_or_else(|| format!("{p}: missing"))
        })
        .expect_err("no model reads");
        assert!(e.contains("0x02000258: tree/bark.png"), "{e}");
    }

    fn png_of(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            w.write_image_data(rgba).expect("data");
        }
        bytes
    }

    #[test]
    fn a_material_for_a_retail_picture_keeps_its_alpha_and_is_read_as_the_retail_picture_is() {
        let mut files = HashMap::new();
        // An eight by eight picture whose left half is cut away (alpha 0), as a retail cut-out
        // picture is; a normal picture leaning toward the picture's right.
        let mut base = Vec::new();
        for _ in 0..8 {
            for x in 0..8 {
                base.extend([120, 90, 60, if x < 4 { 0 } else { 255 }]);
            }
        }
        files.insert("m/wall.png".to_owned(), png_of(8, 8, &base));
        files.insert(
            "m/wall_n.png".to_owned(),
            png_of(8, 8, &[160, 128, 230, 255].repeat(64)),
        );
        files.insert(
            "m/wall_r.png".to_owned(),
            png_of(8, 8, &[50, 0, 0, 255].repeat(64)),
        );
        // A picture whose sides are not whole blocks is stretched to them.
        files.insert(
            "m/floor.png".to_owned(),
            png_of(6, 5, &[10, 200, 30, 255].repeat(30)),
        );
        let manifest = r#"
[pack]
name = "Materials"
format = 1
[[material]]
texture = ["0x0500287C", "0x0500287D"]
base_color = "m/wall.png"
normal = "m/wall_n.png"
roughness = "m/wall_r.png"
normal_scale = 2.0
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05002752"
base_color = "m/floor.png"
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05001CB4"
base_color = "m/missing.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let pack = ArtPack::open(manifest, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens");
        assert_eq!(pack.materials.len(), 2, "{:?}", pack.problems);
        assert!(
            pack.problems[0].contains("0x05001CB4"),
            "{:?}",
            pack.problems
        );
        assert_eq!(pack.material_of(DataId(0x0500_287D)), Some(0));
        assert_eq!(pack.material_of(DataId(0x0500_2752)), Some(1));
        assert_eq!(pack.material_of(DataId(0x0500_1CB4)), None);
        let wall = &pack.materials[0];
        // Read as stored, as the retail picture is, and compressed in blocks.
        assert!(!wall.albedo.srgb);
        assert_eq!(wall.albedo.format, Some(images::BlockFormat::Bc7));
        let texels = dereth_render::block_codec::decode(
            images::BlockFormat::Bc7,
            8,
            8,
            &wall.albedo.levels[0],
        );
        // The cut-away half keeps its alpha, the rest its color.
        for y in 0..8 {
            for x in 0..8 {
                let t = &texels[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4];
                if x < 4 {
                    assert!(t[3] < 8, "{x},{y}: {t:?}");
                } else {
                    assert!(t[3] > 247, "{x},{y}: {t:?}");
                    assert!(t[0].abs_diff(120) <= 4 && t[2].abs_diff(60) <= 4, "{t:?}");
                }
            }
        }
        // The normal leans twice as far as its picture: 160 is 0.25 of the way; bent, 0.5.
        let normal = dereth_render::block_codec::decode(
            images::BlockFormat::Bc5,
            8,
            8,
            &wall.normal.levels[0],
        );
        let x = f32::from(normal[0]) / 255.0 * 2.0 - 1.0;
        assert!((x - 0.5).abs() < 0.04, "{x}");
        // The roughness and occlusion at half the size; occlusion none where it names none.
        assert_eq!((wall.surface.width, wall.surface.height), (4, 4));
        let surface = dereth_render::block_codec::decode(
            images::BlockFormat::Bc5,
            4,
            4,
            &wall.surface.levels[0],
        );
        assert!(
            surface[0].abs_diff(50) <= 3 && surface[1] > 250,
            "{:?}",
            &surface[..4]
        );
        let floor = &pack.materials[1];
        assert_eq!((floor.albedo.width, floor.albedo.height), (8, 8));
    }

    #[test]
    fn a_ground_material_packs_its_pictures_three_to_a_material_at_one_size() {
        let mut files = HashMap::new();
        files.insert(
            "g/a.png".to_owned(),
            png_of(2, 2, &[200, 100, 50, 255].repeat(4)),
        );
        files.insert(
            "g/a_h.png".to_owned(),
            png_of(2, 2, &[90, 0, 0, 255].repeat(4)),
        );
        files.insert(
            "g/a_n.png".to_owned(),
            png_of(2, 2, &[140, 120, 255, 255].repeat(4)),
        );
        // The second material's pictures are larger: they are brought to the first's size.
        files.insert(
            "g/b.png".to_owned(),
            png_of(4, 4, &[10, 20, 30, 255].repeat(16)),
        );
        files.insert(
            "g/b_r.png".to_owned(),
            png_of(4, 4, &[60, 0, 0, 255].repeat(16)),
        );
        let manifest = r#"
[pack]
name = "Ground"
format = 1
[[terrain]]
ground = [1, 9]
base_color = "g/a.png"
height = "g/a_h.png"
normal = "g/a_n.png"
size = 4
author = "a"
tool = "t"
license = "CC0-1.0"
[[terrain]]
ground = "road"
base_color = "g/b.png"
roughness = "g/b_r.png"
size = 8
author = "a"
tool = "t"
license = "CC0-1.0"
[[terrain]]
ground = 3
base_color = "g/missing.png"
size = 8
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let pack = ArtPack::open(manifest, &mut |p| {
            files.get(p).cloned().ok_or_else(|| format!("{p}: missing"))
        })
        .expect("a pack of grounds alone opens");
        assert!(pack.models.is_empty());
        assert_eq!(pack.grounds.len(), 2, "{:?}", pack.problems);
        assert!(
            pack.problems[0].contains("missing.png"),
            "{:?}",
            pack.problems
        );
        // Two texels a side are not whole blocks: the pictures stay four bytes a texel.
        let grass = pack.ground(Ground::Type(9)).expect("type 9");
        assert_eq!(&grass.albedo.levels[0][..4], &[200, 100, 50, 90]);
        assert_eq!(&grass.normal.levels[0][..4], &[140, 120, 0, 255]);
        // The roughness and occlusion at half the size: one texel.
        assert_eq!((grass.surface.width, grass.surface.height), (1, 1));
        assert_eq!(&grass.surface.levels[0][..4], &[230, 255, 0, 255]);
        assert!(grass.albedo.srgb && !grass.normal.srgb && !grass.surface.srgb);
        assert_eq!(grass.albedo.format, None);
        let road = pack.ground(Ground::Road).expect("the road");
        assert_eq!((road.albedo.width, road.albedo.height), (2, 2));
        assert_eq!(&road.albedo.levels[0][..4], &[10, 20, 30, 128]);
        assert_eq!(&road.normal.levels[0][..4], &[128, 128, 0, 255]);
        assert_eq!(&road.surface.levels[0][..4], &[60, 255, 0, 255]);
        assert!(pack.ground(Ground::Type(3)).is_none());
    }

    #[test]
    fn a_ground_materials_pictures_are_compressed_in_blocks_and_read_back_closely() {
        use images::BlockFormat;
        // Eight texels a side: whole blocks.
        let mut files = HashMap::new();
        let ramp = |c: [u8; 3]| -> Vec<u8> {
            (0..64u8)
                .flat_map(|k| {
                    [
                        c[0].saturating_add(k),
                        c[1],
                        c[2].saturating_add(k / 2),
                        255,
                    ]
                })
                .collect()
        };
        files.insert("g/a.png".to_owned(), png_of(8, 8, &ramp([100, 140, 20])));
        files.insert("g/a_h.png".to_owned(), png_of(8, 8, &ramp([60, 0, 0])));
        files.insert("g/a_n.png".to_owned(), png_of(8, 8, &ramp([110, 150, 255])));
        files.insert("g/a_r.png".to_owned(), png_of(8, 8, &ramp([180, 0, 0])));
        let manifest = r#"
[pack]
name = "Ground"
format = 1
[[terrain]]
ground = 1
base_color = "g/a.png"
height = "g/a_h.png"
normal = "g/a_n.png"
roughness = "g/a_r.png"
size = 4
author = "a"
tool = "t"
license = "CC0-1.0"
"#;
        let pack = ArtPack::open(manifest, &mut |p| {
            files.get(p).cloned().ok_or_else(|| format!("{p}: missing"))
        })
        .expect("the pack opens");
        let g = pack.ground(Ground::Type(1)).expect("type 1");
        assert_eq!(g.albedo.format, Some(BlockFormat::Bc7));
        assert_eq!(g.normal.format, Some(BlockFormat::Bc5));
        assert_eq!(g.surface.format, Some(BlockFormat::Bc5));
        assert_eq!((g.surface.width, g.albedo.width), (4, 8));
        // A quarter of four bytes a texel: two by two blocks of sixteen bytes; the half-size
        // surface one block; every level down to one texel, a block each.
        assert_eq!(g.albedo.levels[0].len(), 4 * 16);
        assert_eq!(g.albedo.levels.len(), 4);
        assert_eq!(g.surface.levels[0].len(), 16);
        let decode = |i: &Image| {
            dereth_render::block_codec::decode(
                i.format.expect("blocks"),
                i.width,
                i.height,
                &i.levels[0],
            )
        };
        let close = |a: &[u8], b: &[u8], channels: &[usize]| {
            a.chunks(4)
                .zip(b.chunks(4))
                .all(|(p, q)| channels.iter().all(|c| p[*c].abs_diff(q[*c]) <= 6))
        };
        let base = ramp([100, 140, 20]);
        let height = ramp([60, 0, 0]);
        let want: Vec<u8> = base
            .chunks(4)
            .zip(height.chunks(4))
            .flat_map(|(c, h)| [c[0], c[1], c[2], h[0]])
            .collect();
        assert!(close(&decode(&g.albedo), &want, &[0, 1, 2, 3]));
        let normal = ramp([110, 150, 255]);
        assert!(close(&decode(&g.normal), &normal, &[0, 1]));
        // The roughness in red, nothing of the occlusion named: whole in green.
        let surface = decode(&g.surface);
        assert!(surface.chunks(4).all(|t| t[1] >= 254));
    }

    #[test]
    fn the_retail_ground_holds_retails_masks_and_its_pictures_where_the_pack_has_no_material() {
        use dereth_assets::region::{CodeTexture, TerrainDesc, TexMerge};
        let desc = |t: u32, gid: u32| TerrainDesc {
            terrain_type: t,
            tex_gid: DataId(gid),
            tex_tiling: 2,
            max_vert_bright: 0,
            min_vert_bright: 0,
            max_vert_saturate: 0,
            min_vert_saturate: 0,
            max_vert_hue: 0,
            min_vert_hue: 0,
            detail_tex_tiling: 0,
            detail_tex_gid: DataId(0),
        };
        let code = |c: u32, gid: u32| CodeTexture {
            code: c,
            tex_gid: DataId(gid),
        };
        let tm = TexMerge {
            base_tex_size: 16,
            corner_terrain_maps: vec![code(8, 0x0500_0100), code(8, 0x0500_0101)],
            side_terrain_maps: vec![code(9, 0x0500_0200)],
            // The road's first mask is a corner mask too: it is held once.
            road_maps: vec![code(8, 0x0500_0100), code(10, 0x0500_0300)],
            terrain_desc: vec![
                desc(0, 0x0600_0000),
                desc(1, 0x0600_0001),
                desc(16, 0x0600_0016),
                desc(22, 0x0600_0016),
                desc(32, 0x0600_0020),
            ],
        };
        let mut pack = ArtPack {
            name: String::new(),
            serial: 0,
            models: Vec::new(),
            images: Vec::new(),
            grounds: Vec::new(),
            materials: Vec::new(),
            problems: Vec::new(),
        };
        let one = images::with_levels(4, 4, vec![9; 64], true, None);
        pack.grounds.push(PackGround {
            grounds: vec![Ground::Type(1), Ground::Road],
            albedo: one.clone(),
            normal: one.clone(),
            surface: one,
            size: 4.0,
            normal_scale: 1.0,
            credit: Credit::default(),
        });
        let mut asked = Vec::new();
        let ground = RetailGround::of(&tm, &pack, &mut |id| {
            asked.push(id.0);
            Some((2, 2, vec![id.0.to_le_bytes()[0]; 16]))
        });
        assert_eq!(
            ground.masks.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
            [0x0500_0100, 0x0500_0101, 0x0500_0200, 0x0500_0300]
        );
        assert_eq!(ground.mask(DataId(0x0500_0300)), Some(3));
        assert_eq!(ground.mask(DataId(0x0500_0999)), None);
        // Type 1 and the road have the pack's materials. Every other type is drawn with the
        // first description's picture unless it has one of its own; the water's is shared.
        let by_picture: Vec<Vec<u8>> = ground.pictures.iter().map(|p| p.grounds.clone()).collect();
        assert_eq!(by_picture[1], [16, 22]);
        assert!(by_picture[0].contains(&0) && by_picture[0].contains(&31));
        assert!(!by_picture.iter().flatten().any(|g| *g == 1 || *g == 32));
        // Brought to the pack's size and compressed, a level height, a flat normal, a wet sheen on
        // the water (its roughness and occlusion at half the size, two texels: not compressed).
        let water = &ground.pictures[1];
        assert_eq!((water.albedo.width, water.tiling), (4, 2));
        assert_eq!(water.albedo.format, Some(images::BlockFormat::Bc7));
        let albedo = dereth_render::block_codec::decode(
            images::BlockFormat::Bc7,
            4,
            4,
            &water.albedo.levels[0],
        );
        assert!(albedo.chunks(4).all(|t| t[3].abs_diff(128) <= 1));
        let normal = dereth_render::block_codec::decode(
            images::BlockFormat::Bc5,
            4,
            4,
            &water.normal.levels[0],
        );
        assert!(normal.chunks(4).all(|t| t[0] == 128 && t[1] == 128));
        assert_eq!(water.surface.width, 2);
        assert_eq!(&water.surface.levels[0][..2], &[100, 255]);
        assert_eq!(ground.pictures[0].surface.levels[0][0], 220);
        // A mask's alpha is kept in its red (two texels a side: not compressed).
        assert_eq!(&ground.masks[1].1.levels[0][..4], &[0x01, 0, 0, 255]);
        // Each picture read once.
        assert_eq!(asked.iter().filter(|a| **a == 0x0600_0016).count(), 1);
        // A broad picture for every ground, the pack's too, at its repeats; a 2 by 2 picture has
        // no level 64 texels wide, so it is plain grey.
        assert_eq!(ground.broad.len(), 33);
        assert!(ground
            .broad
            .iter()
            .all(|(i, k)| i.width == BROAD_SIZE && *k == 2));
        assert_eq!(&ground.broad[1].0.levels[0][..4], &[128, 128, 128, 255]);
    }
}
