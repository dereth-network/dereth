//! An art pack's models, drawn in place of the objects they replace.
//!
//! The scene draws every object as it always does, and notes around the draws of the objects an
//! art pack has models for ([`Mark::ArtPackCovered`]), and where each block that has such objects
//! draws them ([`Mark::ArtPackBlock`]). In a re-shaded frame this pass leaves those draws out of
//! the world's replay and, at the first block's note, draws the pack's models of every block that
//! noted one into the re-shaded targets: the colour the light per pixel divides its old light out
//! of, and the normal with the surface's class. They then take the light, the shadows, the
//! occlusion and the bounced light as everything else does. The sun's shadow cascades take the
//! models in place of the objects too ([`PackCasters`]).
//!
//! Only pixels change. What can be picked, where the camera may go and everything the game reads
//! come from the ordinary frame, which draws the original objects whether or not the pack is
//! drawn. A frame that is not re-shaded, or that notes no block, draws the original objects.
//!
//! Each model is drawn at the level of detail its distance from the eye calls for, the distance
//! divided by its scale, and not at all past its last level's distance; instances of one level
//! are drawn together, and those outside the view are left out.
//!
//! A model of cut-out growth has a [`Crown`], worked out from its leaves when the pack is put on
//! the device: each leaf carries to the light per pixel how open its place in the crown is, so a
//! leaf deep inside sees little of the sky and lets no sun through. The far shadow cascades draw
//! a solid stand-in for the crown in place of its leaves, whose cut-outs would cost the most
//! there, and every cascade draws the models nearest the sun first.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use glam::{Mat4, Vec2, Vec3, Vec3Swizzles, Vec4, Vec4Swizzles};

use crate::graph::{EncodeCx, HifiPass, PrepareCx};

mod far;
use crate::build::Build;
use crate::build::Prebuilt;
use crate::graph::WarmCx;
use crate::passes::lighting::{legacy_light, object_ambient, CASCADES};
use crate::reshade::ReshadePlan;
use crate::shared::camera::HEIGHT_UP;
use crate::shared::pack_image;
use crate::snapshot::{HifiArtPack, HifiPackImage, HifiPackMaterial, HifiPackPlacement};
use crate::{
    Caps, DrawAction, DrawNote, HifiError, HifiFrame, HifiSettings, Mark, ReplayFilter, SkipRule,
};
pub use far::pixels_a_meter;

const PACK_WGSL: &str = include_str!("pack.wgsl");

/// The pack's shader, for validation without a device.
#[must_use]
pub fn shader_source() -> &'static str {
    PACK_WGSL
}

/// The bytes of one vertex of a model.
const VERTEX_BYTES: u64 = (crate::snapshot::PACK_VERTEX_FLOATS * 4) as u64;
/// The bytes of one instance: the three rows of its model-to-render transform, and the share of
/// the screen's dither it keeps.
const INSTANCE_BYTES: u64 = 64;
/// The stride of the shadow cascades' constant blocks.
const CASTER_STRIDE: u64 = 256;
/// The same, as an index into their bytes.
const CASTER_BLOCK: usize = 256;

/// The art pack pass, the handle the sun's shadows draw its models through, and the pass that
/// culls its far trees before the world is drawn.
#[must_use]
pub fn pass() -> (ArtPack, PackCasters, FarCull) {
    let state = Rc::new(RefCell::new(State::default()));
    (
        ArtPack(Rc::clone(&state)),
        PackCasters(Rc::clone(&state)),
        FarCull(state),
    )
}

/// The art pack pass: it leaves the replaced objects out of the world's replay and draws the
/// pack's models in their place.
#[derive(Debug)]
pub struct ArtPack(Rc<RefCell<State>>);

/// The sun's shadows' way to the art pack's models: whether the pack is drawn this frame, and its
/// models drawn into each cascade.
#[derive(Debug, Clone)]
pub struct PackCasters(Rc<RefCell<State>>);

/// The far trees' culling, on the device before the world is drawn: which of the far trees the
/// view keeps, model by model, for the draws in the world.
#[derive(Debug)]
pub struct FarCull(Rc<RefCell<State>>);

/// One run of instances of one level of detail of one model.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Batch {
    model: usize,
    lod: usize,
    instances: Range<u32>,
}

/// One placement of the frame at one level, worked out: its model and level, its transform's
/// rows, its bounding sphere in render space, and the share of the screen's dither the level
/// keeps. A placement in the band where one level gives way to the next is worked out twice,
/// once for each level.
#[derive(Debug, Clone, Copy)]
struct Placed {
    model: usize,
    lod: usize,
    /// How many levels its model has.
    levels: usize,
    rows: [Vec4; 3],
    centre: Vec3,
    radius: f32,
    /// Its block noted it this frame, and it is in the view.
    seen: bool,
    /// The dither's values the level keeps, `lo..hi` of 0..1: all of them away from a change of
    /// level.
    keep: (f32, f32),
    /// The second of a placement's two levels in a band: drawn into the view, but not into the
    /// sun's shadows, which draw the first.
    second: bool,
}

/// Whether the pack's models are drawn: its option is on, with the light, and the frame carries a
/// pack.
fn draws(s: &HifiSettings, f: &HifiFrame) -> bool {
    s.art_pack && s.lighting.is_on() && f.art_pack.is_some()
}

impl State {
    /// Whether the models of the pack `f` carries are on the device, or are not being built:
    /// a frame before they are built is drawn without them.
    fn ready(&self, f: &HifiFrame) -> bool {
        let serial = f.art_pack.as_ref().map(|p| p.serial);
        self.gpu.as_ref().is_some_and(|g| Some(g.serial) == serial) || !self.pre_gpu.waits()
    }
}

#[derive(Debug, Default)]
struct State {
    /// The pack the frames carry.
    pack: Option<Arc<HifiArtPack>>,
    /// Its models on the device.
    gpu: Option<PackGpu>,
    /// The same, built ahead by the warm-up.
    pre_gpu: Prebuilt<PackGpu>,
    /// Each resident block's placements, with the block's build they belong to and whether its
    /// own objects stand in the world.
    blocks: HashMap<u16, (u32, bool, Vec<HifiPackPlacement>)>,
    /// The command ranges of the replaced objects' draws this frame.
    covered: crate::shared::runs::Runs,
    /// The blocks that noted their objects this frame.
    marked: HashSet<u16>,
    /// The first of those notes' commands.
    first_mark: Option<u32>,
    /// Every placement of the resident blocks, worked out for this frame.
    placed: Vec<Placed>,
    /// The runs drawn into the view.
    view: Vec<Batch>,
    /// The runs drawn into each shadow cascade.
    casters: [Vec<Batch>; CASCADES],
    /// Whether the pack is drawn in this frame.
    active: bool,
    /// Whether the models have been drawn into the view this frame.
    drawn: bool,
    /// The instances drawn into the view in the last frame.
    drawn_instances: u64,
    /// The far trees on the device, for the set the frames carry.
    far: Option<far::FarGpu>,
    /// The command the far trees are drawn at: the end of the first landscape cell's ground,
    /// where the drawn ground they stand on is drawn too.
    far_mark: Option<u32>,
    /// Whether the far trees are drawn in this frame, and whether they have been.
    far_active: bool,
    far_drawn: bool,
    /// Microseconds the far trees took on this frame's processor: the last frame's upkeep, and
    /// the last time the set changed, its taking in.
    far_prepare_us: u64,
    far_taken_us: u64,
}

/// A model on the device.
#[derive(Debug)]
struct GpuModel {
    lods: Vec<GpuLod>,
    /// The bounding sphere, model space.
    bounds: (Vec3, f32),
    /// The solid stand-in for the crown of cut-out growth, which the far shadow cascades draw in
    /// place of its leaves: its vertices, indices and their count.
    proxy: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
}

#[derive(Debug)]
struct GpuLod {
    distance: f32,
    primitives: Vec<GpuPrimitive>,
    /// Pictures of the model seen from round it, turned to the eye, rather than its shape.
    pictures: bool,
    /// Drawn into the sun's shadows alone.
    shadows_only: bool,
}

#[derive(Debug)]
struct GpuPrimitive {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    material: wgpu::BindGroup,
    double_sided: bool,
    cut_out: bool,
}

/// An art pack on the device: its models and pictures, the pipelines that draw them, and the
/// frame's constants and instances.
#[derive(Debug)]
struct PackGpu {
    serial: u64,
    models: Vec<GpuModel>,
    by_id: HashMap<u32, usize>,
    _textures: Vec<wgpu::Texture>,
    /// The bytes its pictures take on the device.
    picture_bytes: u64,
    /// The view's pipelines, the faces turned away culled and not.
    view_culled: wgpu::RenderPipeline,
    view_both: wgpu::RenderPipeline,
    /// Pictures of a model seen from round it, turned to the eye.
    view_pictures: wgpu::RenderPipeline,
    caster_solid: wgpu::RenderPipeline,
    caster_cut: wgpu::RenderPipeline,
    frame_uniform: wgpu::Buffer,
    frame_bind: wgpu::BindGroup,
    caster_uniform: wgpu::Buffer,
    caster_bind: wgpu::BindGroup,
    view_instances: Option<(wgpu::Buffer, u64)>,
    caster_instances: Option<(wgpu::Buffer, u64)>,
    /// The far trees' layouts and culling.
    far: far::FarPipelines,
    /// The far trees' pictures, placed from the set the culling kept.
    view_far: wgpu::RenderPipeline,
    /// Each model with far pictures, in the order the far trees name them, and the place of a
    /// retail object's model among them.
    far_models: Vec<far::FarModel>,
    far_of: HashMap<u32, usize>,
}

fn uniform_entry(binding: u32, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// The vertex buffers' layouts: a model's vertices, and an instance's transform.
fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const VERTEX: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32x4
    ];
    const INSTANCE: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4];
    [
        wgpu::VertexBufferLayout {
            array_stride: VERTEX_BYTES,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX,
        },
        wgpu::VertexBufferLayout {
            array_stride: INSTANCE_BYTES,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE,
        },
    ]
}

/// The picture of one texel, `rgba`, as a stand-in where a material names none.
fn one_texel(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    rgba: [u8; 4],
    srgb: bool,
) -> wgpu::Texture {
    upload_image(
        device,
        queue,
        &HifiPackImage {
            width: 1,
            height: 1,
            srgb,
            levels: vec![Arc::new(rgba.to_vec())],
            format: None,
        },
    )
}

fn upload_image(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &HifiPackImage,
) -> wgpu::Texture {
    let levels = u32::try_from(image.levels.len().max(1)).unwrap_or(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("art pack picture"),
        size: wgpu::Extent3d {
            width: image.width.max(1),
            height: image.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: pack_image::format(image, pack_image::samples_blocks(device)),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    pack_image::write(queue, &texture, 0, image, image.levels.len());
    texture
}

fn buffer_with(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    usage: wgpu::BufferUsages,
    bytes: &[u8],
) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (bytes.len() as u64).next_multiple_of(4).max(4),
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, bytes);
    buffer
}

/// Whether `m`, of a model that is cut-out growth when `foliage`, is drawn as foliage: a cut-out
/// material of such a model (its needles or leaves; its trunk is an ordinary surface).
fn is_foliage(m: &HifiPackMaterial, foliage: bool) -> bool {
    foliage && m.alpha_cutoff.is_some()
}

/// How many heights a crown's reach is kept at.
pub const CROWN_RINGS: usize = 8;

/// The shape of a plant's crown, from its leaves, in the model's own space (height up): the axis
/// it stands round, the heights of its lowest and highest leaves, and how far the leaves reach
/// from the axis in each of [`CROWN_RINGS`] bands of height evenly between.
///
/// A leaf near the axis, or under a part of the crown wider than its own reach, is deep in the
/// crown: it sees little of the sky between the leaves round it, and the sun's light does not
/// pass through to it. The light per pixel reads that from its class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Crown {
    /// The axis, across the ground.
    pub axis: Vec2,
    /// The height of the lowest leaves.
    pub base: f32,
    /// The height of the highest leaves.
    pub top: f32,
    /// How far the leaves reach from the axis in each band of height, lowest first: nine in ten
    /// of the band's leaf corners lie within it.
    pub reach: [f32; CROWN_RINGS],
}

impl Crown {
    /// The crown of leaves whose corners are `points`; `None` for fewer than a handful, or for
    /// leaves that stand no higher than a hand's breadth.
    #[must_use]
    pub fn of(points: &[Vec3]) -> Option<Self> {
        if points.len() < 8 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)] // a model's vertex count
        let n = points.len() as f32;
        let axis = points.iter().fold(Vec2::ZERO, |a, p| a + p.xy()) / n;
        let (base, top) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.z), hi.max(p.z))
        });
        if top - base < 0.1 {
            return None;
        }
        let mut bands: [Vec<f32>; CROWN_RINGS] = Default::default();
        for p in points {
            // LINT-OK: a height within the crown, scaled to a band and floored.
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let band = (((p.z - base) / (top - base)) * CROWN_RINGS as f32) as usize;
            bands[band.min(CROWN_RINGS - 1)].push((p.xy() - axis).length());
        }
        let mut reach = [None; CROWN_RINGS];
        for (r, band) in reach.iter_mut().zip(&mut bands) {
            if band.is_empty() {
                continue;
            }
            band.sort_by(f32::total_cmp);
            *r = Some(band[(band.len() * 9 / 10).min(band.len() - 1)]);
        }
        // A band without leaves takes the reach of the nearest bands that have them.
        let known: Vec<(usize, f32)> = reach
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.map(|r| (i, r)))
            .collect();
        let reach = std::array::from_fn(|i| {
            reach[i].unwrap_or_else(|| {
                let below = known.iter().rev().find(|(k, _)| *k < i).map(|(_, r)| *r);
                let above = known.iter().find(|(k, _)| *k > i).map(|(_, r)| *r);
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

    /// The crown of `model`'s leaves at its finest level of detail; `None` for a model that is
    /// not cut-out growth or has no leaves.
    #[must_use]
    pub fn of_model(model: &crate::snapshot::HifiPackModel) -> Option<Self> {
        if !model.foliage {
            return None;
        }
        let lod = model.lods.first()?;
        let stride = crate::snapshot::PACK_VERTEX_FLOATS;
        let points: Vec<Vec3> = lod
            .primitives
            .iter()
            .filter(|p| is_foliage(&p.material, true))
            .flat_map(|p| {
                p.vertices
                    .chunks_exact(stride)
                    .map(|v| Vec3::new(v[0], v[1], v[2]))
                    .collect::<Vec<_>>()
            })
            .collect();
        Self::of(&points)
    }

    /// A closed solid of revolution round the axis that stands in for the crown in the far shadow
    /// cascades: a ring in the middle of each band at `shrink` of the band's reach, a point at the
    /// top and a disc across the bottom, `sides` faces round. Its vertices, in the pack's vertex
    /// layout ([`crate::snapshot::PACK_VERTEX_FLOATS`] floats, an opaque white), and its indices,
    /// counter-clockwise from outside.
    #[must_use]
    pub fn proxy(&self, shrink: f32, sides: u32) -> (Vec<f32>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut push = |p: Vec3| {
            vertices.extend([p.x, p.y, p.z]);
            vertices.extend([0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
            vertices.extend([1.0; 4]);
        };
        let span = self.top - self.base;
        #[allow(clippy::cast_precision_loss)] // a handful of rings and sides
        for (k, reach) in self.reach.iter().enumerate() {
            let z = self.base + span * (k as f32 + 0.5) / CROWN_RINGS as f32;
            for j in 0..sides {
                let a = std::f32::consts::TAU * j as f32 / sides as f32;
                let (sin, cos) = (
                    dereth_primitives::num::math::sinf(a),
                    dereth_primitives::num::math::cosf(a),
                );
                push(Vec3::new(
                    self.axis.x + cos * reach * shrink,
                    self.axis.y + sin * reach * shrink,
                    z,
                ));
            }
        }
        let rings = u32::try_from(CROWN_RINGS).unwrap_or(0);
        let top = rings * sides;
        push(self.axis.extend(self.top));
        let bottom = top + 1;
        push(
            self.axis
                .extend(self.base + span * 0.5 / CROWN_RINGS as f32),
        );
        let mut indices = Vec::new();
        for k in 0..rings.saturating_sub(1) {
            for j in 0..sides {
                let a = k * sides + j;
                let b = k * sides + (j + 1) % sides;
                let (c, d) = (a + sides, b + sides);
                indices.extend([a, b, d, a, d, c]);
            }
        }
        let last = (rings - 1) * sides;
        for j in 0..sides {
            indices.extend([last + j, last + (j + 1) % sides, top]);
            indices.extend([(j + 1) % sides, j, bottom]);
        }
        (vertices, indices)
    }

    /// The crown as the shader reads it: the axis and the heights, then the reach of every band;
    /// all nought for no crown.
    fn constants(crown: Option<&Self>) -> [f32; 4 + CROWN_RINGS] {
        let mut c = [0.0; 4 + CROWN_RINGS];
        if let Some(k) = crown {
            c[..4].copy_from_slice(&[k.axis.x, k.axis.y, k.base, k.top]);
            c[4..].copy_from_slice(&k.reach);
        }
        c
    }
}

/// How a level of pictures of its model is drawn: how many views round the model, the tiles to a
/// row and the rows of its pictures, and its quad's half width, foot and top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pictures {
    /// Views, columns and rows.
    pub grid: [f32; 3],
    /// Half the quad's width, and the heights of its foot and its top.
    pub quad: [f32; 3],
}

impl Pictures {
    /// The layout of a level of `views` (the views and the tiles to a row) whose quad is the
    /// vertices `vertices` ([`crate::snapshot::PACK_VERTEX_FLOATS`] floats each, height up).
    #[must_use]
    pub fn of(views: (u32, u32), vertices: &[f32]) -> Self {
        let (count, columns) = (views.0.max(1), views.1.clamp(1, views.0.max(1)));
        let rows = count.div_ceil(columns);
        let (mut half, mut foot, mut top) = (0.0f32, f32::MAX, f32::MIN);
        for v in vertices
            .as_chunks::<{ crate::snapshot::PACK_VERTEX_FLOATS }>()
            .0
        {
            half = half.max(v[0].abs());
            foot = foot.min(v[2]);
            top = top.max(v[2]);
        }
        #[allow(clippy::cast_precision_loss)] // a handful of views
        Self {
            grid: [count as f32, columns as f32, rows as f32],
            quad: [half, foot.min(top), top.max(foot)],
        }
    }
}

/// A material's constants, as the shader reads them: with the crown of its model's leaves, for a
/// material drawn as foliage, and the layout of a level of pictures.
fn material_constants(
    m: &HifiPackMaterial,
    foliage: bool,
    crown: Option<&Crown>,
    pictures: Option<Pictures>,
    glow: (f32, f32),
    far: usize,
) -> Vec<f32> {
    let foliage = is_foliage(m, foliage);
    let mut c = Vec::with_capacity(12 + 4 + CROWN_RINGS + 8);
    c.extend(m.base_color);
    c.extend([
        m.metallic,
        m.roughness,
        m.normal_scale,
        m.occlusion_strength,
    ]);
    c.extend([
        m.alpha_cutoff.unwrap_or(0.0),
        f32::from(u8::from(foliage)),
        f32::from(u8::from(m.double_sided || foliage)),
        if foliage { glow.0 } else { glow.1 },
    ]);
    c.extend(Crown::constants(crown.filter(|_| foliage)));
    let p = pictures.unwrap_or(Pictures {
        grid: [0.0; 3],
        quad: [0.0; 3],
    });
    #[allow(clippy::cast_precision_loss)] // a model's place among a pack's, far inside f32's range
    c.extend([p.grid[0], p.grid[1], p.grid[2], far as f32]);
    c.extend([p.quad[0], p.quad[1], p.quad[2], 0.0]);
    c
}

/// The one-texel pictures a material that names none of its own is drawn with.
struct Fallbacks {
    /// White, for a base colour.
    base: wgpu::Texture,
    /// White, for roughness, metalness and occlusion: their factors alone.
    data: wgpu::Texture,
    /// A normal straight out of the surface.
    normal: wgpu::Texture,
}

/// What a primitive's material is bound with.
struct Materials<'a> {
    layout: &'a wgpu::BindGroupLayout,
    sampler: &'a wgpu::Sampler,
    images: &'a [wgpu::Texture],
    fallbacks: &'a Fallbacks,
}

impl Materials<'_> {
    /// The view of picture `index`, or of `fallback` when there is none.
    fn view(&self, index: Option<u32>, fallback: &wgpu::Texture) -> wgpu::TextureView {
        index
            .and_then(|i| self.images.get(i as usize))
            .unwrap_or(fallback)
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// `p` on the device: its vertices, indices and material, of `model` (cut-out growth or not,
    /// and the light its retail object gives off), whose leaves make `crown`; of a level of
    /// pictures laid out as `views`, the model's place among those the far trees draw `far`.
    #[allow(clippy::too_many_arguments)]
    fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        p: &crate::snapshot::HifiPackPrimitive,
        model: &crate::snapshot::HifiPackModel,
        crown: Option<&Crown>,
        views: Option<(u32, u32)>,
        far: usize,
    ) -> GpuPrimitive {
        let m = &p.material;
        let pictures = views.map(|v| Pictures::of(v, &p.vertices));
        let constants = buffer_with(
            device,
            queue,
            "art pack material",
            wgpu::BufferUsages::UNIFORM,
            &floats(&material_constants(
                m,
                model.foliage,
                crown,
                pictures,
                model.glow,
                far,
            )),
        );
        let f = self.fallbacks;
        let views = [
            self.view(m.base_color_image, &f.base),
            self.view(m.normal_image, &f.normal),
            self.view(m.metal_rough_image, &f.data),
            self.view(m.occlusion_image, &f.data),
        ];
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: constants.as_entire_binding(),
        }];
        entries.extend(
            (1u32..)
                .zip(&views)
                .map(|(binding, v)| wgpu::BindGroupEntry {
                    binding,
                    resource: wgpu::BindingResource::TextureView(v),
                }),
        );
        entries.push(wgpu::BindGroupEntry {
            binding: 5,
            resource: wgpu::BindingResource::Sampler(self.sampler),
        });
        let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("art pack material"),
            layout: self.layout,
            entries: &entries,
        });
        let index_bytes: Vec<u8> = p.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        GpuPrimitive {
            vertices: buffer_with(
                device,
                queue,
                "art pack vertices",
                wgpu::BufferUsages::VERTEX,
                &floats(&p.vertices),
            ),
            indices: buffer_with(
                device,
                queue,
                "art pack indices",
                wgpu::BufferUsages::INDEX,
                &index_bytes,
            ),
            count: u32::try_from(p.indices.len()).unwrap_or(u32::MAX),
            material,
            double_sided: m.double_sided || is_foliage(m, model.foliage),
            cut_out: m.alpha_cutoff.is_some(),
        }
    }
}

impl PackGpu {
    #[allow(clippy::too_many_lines)] // one pipeline set and every model's upload
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, pack: &HifiArtPack) -> Self {
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("art pack frame"),
            entries: &[uniform_entry(0, false)],
        });
        let caster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("art pack shadow cascade"),
            entries: &[uniform_entry(1, true)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("art pack material"),
            entries: &[
                uniform_entry(0, false),
                texture_entry(1),
                texture_entry(2),
                texture_entry(3),
                texture_entry(4),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("art pack"),
            source: wgpu::ShaderSource::Wgsl(PACK_WGSL.into()),
        });
        let view_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("art pack"),
            bind_group_layouts: &[Some(&frame_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("art pack shadow"),
            bind_group_layouts: &[Some(&caster_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let buffers = vertex_layouts();
        let view_pipeline = |cull: Option<wgpu::Face>, vertex: &str, fragment: &str| {
            let targets = [
                Some(wgpu::ColorTargetState {
                    format: crate::derive::pipeline::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                }),
                Some(wgpu::ColorTargetState {
                    format: crate::derive::pipeline::NORMAL_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ];
            device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("art pack"),
                layout: Some(&view_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &buffers.clone().map(Some),
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: cull,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::SidecarContext::depth_format(),
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let far_pipelines = far::FarPipelines::new(device);
        let far_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("art pack far trees"),
            bind_group_layouts: &[
                Some(&frame_layout),
                Some(&material_layout),
                Some(&far_pipelines.draw_layout),
            ],
            immediate_size: 0,
        });
        let view_far = {
            let targets = [
                Some(wgpu::ColorTargetState {
                    format: crate::derive::pipeline::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                }),
                Some(wgpu::ColorTargetState {
                    format: crate::derive::pipeline::NORMAL_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ];
            device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("art pack far trees"),
                layout: Some(&far_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_far"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(buffers[0].clone())],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::SidecarContext::depth_format(),
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_impostor"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let view_culled = view_pipeline(Some(wgpu::Face::Back), "vs_main", "fs_main");
        let view_both = view_pipeline(None, "vs_main", "fs_main");
        let view_pictures = view_pipeline(None, "vs_impostor", "fs_impostor");
        let unclipped = device
            .features()
            .contains(wgpu::Features::DEPTH_CLIP_CONTROL);
        let caster_pipeline = |cut: bool| {
            device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("art pack shadow"),
                layout: Some(&shadow_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_caster"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &buffers.clone().map(Some),
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    unclipped_depth: unclipped,
                    ..wgpu::PrimitiveState::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 0,
                        slope_scale: 1.5,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: cut.then_some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_caster_cut"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let caster_solid = caster_pipeline(false);
        let caster_cut = caster_pipeline(true);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("art pack"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..wgpu::SamplerDescriptor::default()
        });
        let mut textures: Vec<wgpu::Texture> = pack
            .images
            .iter()
            .map(|i| upload_image(device, queue, i))
            .collect();
        let fallbacks = Fallbacks {
            base: one_texel(device, queue, [255; 4], true),
            data: one_texel(device, queue, [255; 4], false),
            normal: one_texel(device, queue, [128, 128, 255, 255], false),
        };
        let materials = Materials {
            layout: &material_layout,
            sampler: &sampler,
            images: &textures,
            fallbacks: &fallbacks,
        };
        let mut models = Vec::with_capacity(pack.models.len());
        let mut by_id = HashMap::new();
        let mut far_models = Vec::new();
        let mut far_of = HashMap::new();
        for (k, model) in pack.models.iter().enumerate() {
            by_id.insert(model.id, k);
            let crown = Crown::of_model(model);
            // The last level of pictures, with a quad to draw, is what the far trees draw.
            let pictures = model.lods.iter().rposition(|l| {
                l.views.is_some()
                    && l.primitives
                        .first()
                        .is_some_and(|p| p.indices.len() == 6 && !p.vertices.is_empty())
            });
            let far = pictures.map_or(0, |lod| {
                far_of.insert(model.id, far_models.len());
                far_models.push(far::FarModel {
                    model: k,
                    lod,
                    bounds: model.bounds,
                });
                far_models.len() - 1
            });
            let lods = model
                .lods
                .iter()
                .map(|lod| GpuLod {
                    distance: lod.distance,
                    primitives: lod
                        .primitives
                        .iter()
                        .filter(|p| !p.indices.is_empty() && !p.vertices.is_empty())
                        .map(|p| {
                            materials.upload(
                                device,
                                queue,
                                p,
                                model,
                                crown.as_ref(),
                                lod.views,
                                far,
                            )
                        })
                        .collect(),
                    pictures: lod.views.is_some(),
                    shadows_only: lod.shadows_only,
                })
                .collect();
            let proxy = crown.map(|k| {
                let (v, i) = k.proxy(PROXY_SHRINK, PROXY_SIDES);
                let indices: Vec<u8> = i.iter().flat_map(|x| x.to_le_bytes()).collect();
                (
                    buffer_with(
                        device,
                        queue,
                        "art pack crown proxy",
                        wgpu::BufferUsages::VERTEX,
                        &floats(&v),
                    ),
                    buffer_with(
                        device,
                        queue,
                        "art pack crown proxy",
                        wgpu::BufferUsages::INDEX,
                        &indices,
                    ),
                    u32::try_from(i.len()).unwrap_or(0),
                )
            });
            models.push(GpuModel {
                lods,
                bounds: model.bounds,
                proxy,
            });
        }
        let Fallbacks { base, data, normal } = fallbacks;
        textures.extend([base, data, normal]);
        let frame_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("art pack frame"),
            size: 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("art pack frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_uniform.as_entire_binding(),
            }],
        });
        let caster_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("art pack shadow cascades"),
            size: CASTER_STRIDE * CASCADES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let caster_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("art pack shadow cascades"),
            layout: &caster_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &caster_uniform,
                    offset: 0,
                    size: wgpu::BufferSize::new(64),
                }),
            }],
        });
        Self {
            serial: pack.serial,
            models,
            by_id,
            _textures: textures,
            picture_bytes: pack
                .images
                .iter()
                .map(|i| pack_image::device_bytes(i, pack_image::samples_blocks(device)))
                .sum(),
            view_culled,
            view_both,
            view_pictures,
            caster_solid,
            caster_cut,
            frame_uniform,
            frame_bind,
            caster_uniform,
            caster_bind,
            view_instances: None,
            caster_instances: None,
            far: far_pipelines,
            view_far,
            far_models,
            far_of,
        }
    }
}

/// Write `rows` into the instance buffer `slot`, making it larger first when it must be.
fn write_instances(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    slot: &mut Option<(wgpu::Buffer, u64)>,
    rows: &[Vec4],
) {
    let bytes = floats(&rows.iter().flat_map(|r| r.to_array()).collect::<Vec<f32>>());
    let need = (bytes.len() as u64).max(INSTANCE_BYTES);
    if slot.as_ref().is_none_or(|(_, cap)| *cap < need) {
        let cap = need.next_power_of_two();
        *slot = Some((
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("art pack instances"),
                size: cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            cap,
        ));
    }
    if let (Some((buffer, _)), false) = (slot.as_ref(), bytes.is_empty()) {
        queue.write_buffer(buffer, 0, &bytes);
    }
}

/// The level of detail `lods` (their distances, nearest first) draws at `distance` metres from
/// the eye for a model scaled `scale`: the first whose distance reaches the distance over the
/// scale; `None` past the last.
#[must_use]
pub fn level_at(lods: &[f32], distance: f32, scale: f32) -> Option<usize> {
    let d = distance / scale.max(0.05);
    lods.iter().position(|reach| d <= *reach)
}

/// The levels of detail `lods` draws at `distance` metres from the eye for a model scaled
/// `scale`, each with the values of the screen's dither it keeps (`lo..hi` of 0..1). Away from a
/// change of level, one level keeps all of them. Within `band` of a level's distance (a share of
/// it, half before and half after), that level keeps a share that falls across the band from all
/// to none, and the next level, where there is one, keeps the rest: one fades into the other with
/// no pop, and past the last level's distance the model fades out.
#[must_use]
pub fn levels_at(lods: &[f32], distance: f32, scale: f32, band: f32) -> Vec<(usize, f32, f32)> {
    let d = distance / scale.max(0.05);
    for (k, reach) in lods.iter().enumerate() {
        if !reach.is_finite() {
            break;
        }
        let half = reach * band * 0.5;
        if half > 0.0 && d > reach - half && d < reach + half {
            let t = (d - (reach - half)) / (2.0 * half);
            let mut out = vec![(k, t, 1.0)];
            if k + 1 < lods.len() {
                out.push((k + 1, 0.0, t));
            }
            return out;
        }
    }
    level_at(lods, distance, scale)
        .map(|k| vec![(k, 0.0, 1.0)])
        .unwrap_or_default()
}

/// How wide the band is where one level of detail fades into the next, a share of the distance
/// the nearer level reaches. `DERETH_HIFI_PACK_LOD_FADE` asks for another; 0 pops.
fn lod_band() -> f32 {
    std::env::var("DERETH_HIFI_PACK_LOD_FADE")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(LOD_BAND)
}

/// [`lod_band`] when nothing asks for another.
const LOD_BAND: f32 = 0.15;

/// The one level of detail every model is drawn at whatever its distance, to look at that level
/// alone: `DERETH_HIFI_PACK_LOD_ONLY` (0 the first; a model with fewer levels draws its last).
fn lod_only() -> Option<usize> {
    std::env::var("DERETH_HIFI_PACK_LOD_ONLY")
        .ok()
        .and_then(|v| v.trim().parse().ok())
}

/// The least level of detail each shadow cascade draws a model at, nearest cascade first: the
/// shadows of a canopy's needles cost far more than they show, so the cascades draw coarser
/// levels than the view. `DERETH_HIFI_PACK_SHADOW_LEVELS` (four numbers, a comma between) asks
/// for others.
fn caster_floors() -> [usize; CASCADES] {
    std::env::var("DERETH_HIFI_PACK_SHADOW_LEVELS")
        .ok()
        .and_then(|v| {
            let n: Vec<usize> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            <[usize; CASCADES]>::try_from(n).ok()
        })
        .unwrap_or(CASTER_FLOORS)
}

/// The cascades' least levels of detail when nothing asks for others: the view's own in the
/// nearest cascade, where light through the needles is seen, the second level in the next, and
/// the third beyond. In the densest forest the full detail in every cascade costs about one
/// and a quarter times as much again as this.
const CASTER_FLOORS: [usize; CASCADES] = [0, 1, 2, 2];

/// How far the solid stand-in for a crown reaches, against the crown's own reach: the leaves of a
/// crown let some sun between them, which the solid does not.
const PROXY_SHRINK: f32 = 0.8;
/// How many faces round the stand-in for a crown has.
const PROXY_SIDES: u32 = 8;

/// The first shadow cascade that draws a crown's solid stand-in in place of its leaves: the far
/// cascades, where a crown's shadow is a soft patch whatever draws it and its leaves' cut-outs
/// would cost the most. `DERETH_HIFI_PACK_SHADOW_PROXY_FROM` asks for another.
fn proxy_from() -> usize {
    std::env::var("DERETH_HIFI_PACK_SHADOW_PROXY_FROM")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(PROXY_FROM)
}

/// [`proxy_from`] when nothing asks for another.
const PROXY_FROM: usize = 2;

/// The level of detail a shadow cascade draws a model of `levels` levels at, the cascade's least
/// being `floor`: the cascade's, whatever level the view draws the model at. A shadow is drawn
/// at the detail of where it falls, not of where its caster stands: a tree behind the eye that
/// walks out past its first level's distance keeps the leafy shadow it casts at the eye's feet,
/// rather than turning it into its crown's solid stand-in.
#[must_use]
pub fn caster_level(levels: usize, floor: usize) -> usize {
    floor.min(levels.saturating_sub(1))
}

/// The placements `placed` as a shadow cascade whose least level is `floor` draws them: each at
/// [`caster_level`], whole.
fn casters(placed: &[Placed], floor: usize) -> Vec<Placed> {
    placed
        .iter()
        .map(|p| Placed {
            lod: caster_level(p.levels, floor),
            keep: (0.0, 1.0),
            ..*p
        })
        .collect()
}

/// The rows of the model-to-render transform of `p` in a block whose south-west corner is at
/// `origin`.
#[must_use]
pub fn placement_rows(p: &HifiPackPlacement, origin: Vec3) -> [Vec4; 3] {
    let m = Mat4::from_scale_rotation_translation(
        Vec3::splat(p.scale),
        p.rotation.normalize(),
        origin + p.origin,
    );
    [m.row(0), m.row(1), m.row(2)]
}

/// The six planes of the view `clip_from_render` sees through, each `xyz` the inward normal and
/// `w` the offset, for a depth range of 0..1.
#[must_use]
pub fn frustum(clip_from_render: Mat4) -> [Vec4; 6] {
    let r = |i| clip_from_render.row(i);
    let planes = [
        r(3) + r(0),
        r(3) - r(0),
        r(3) + r(1),
        r(3) - r(1),
        r(2),
        r(3) - r(2),
    ];
    planes.map(|p| p / p.xyz().length().max(1e-12))
}

/// Whether a sphere at `centre` of `radius` is at least partly inside `planes`.
#[must_use]
pub fn sphere_seen(planes: &[Vec4; 6], centre: Vec3, radius: f32) -> bool {
    planes.iter().all(|p| p.xyz().dot(centre) + p.w >= -radius)
}

/// Group the placements `chosen` (indices into `placed`) into runs of one model's one level,
/// appending each run's instances to `rows` (the transform's three rows and the dither kept, four
/// to an instance): a run's instances are their places in `rows`.
fn batches(placed: &[Placed], mut chosen: Vec<usize>, rows: &mut Vec<Vec4>) -> Vec<Batch> {
    chosen.sort_by_key(|i| (placed[*i].model, placed[*i].lod));
    let mut out: Vec<Batch> = Vec::new();
    for i in chosen {
        let p = &placed[i];
        let at = u32::try_from(rows.len() / 4).unwrap_or(u32::MAX);
        rows.extend(p.rows);
        rows.push(Vec4::new(p.keep.0, p.keep.1, 0.0, 0.0));
        match out.last_mut() {
            Some(b) if b.model == p.model && b.lod == p.lod => b.instances.end = at + 1,
            _ => out.push(Batch {
                model: p.model,
                lod: p.lod,
                instances: at..at + 1,
            }),
        }
    }
    out
}

impl State {
    /// Work out every placement of the resident blocks for `frame`, and the runs the view draws.
    fn place(&mut self, frame: &HifiFrame, clip_from_render: Mat4) {
        self.placed.clear();
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let planes = frustum(clip_from_render);
        let eye = frame.camera.eye;
        let band = lod_band();
        let only = lod_only();
        for t in &frame.terrain {
            let Some((_, _, placements)) = self.blocks.get(&t.block) else {
                continue;
            };
            let marked = self.marked.contains(&t.block);
            for p in placements {
                let Some(&model) = gpu.by_id.get(&p.id) else {
                    continue;
                };
                let m = &gpu.models[model];
                let rows = placement_rows(p, t.origin);
                let centre = {
                    let c = m.bounds.0.extend(1.0);
                    Vec3::new(rows[0].dot(c), rows[1].dot(c), rows[2].dot(c))
                };
                let radius = m.bounds.1 * p.scale.abs();
                // The view's levels: every level but those for the shadows alone.
                let shown: Vec<usize> = (0..m.lods.len())
                    .filter(|k| !m.lods[*k].shadows_only)
                    .collect();
                let distances: Vec<f32> = shown.iter().map(|k| m.lods[*k].distance).collect();
                let seen = marked && sphere_seen(&planes, centre, radius);
                let far = (t.origin + p.origin).distance(eye);
                let levels = match only {
                    Some(k) => vec![(k.min(m.lods.len().saturating_sub(1)), 0.0, 1.0)],
                    None => levels_at(&distances, far, p.scale, band)
                        .into_iter()
                        .map(|(k, lo, hi)| (shown[k], lo, hi))
                        .collect(),
                };
                for (i, (lod, lo, hi)) in levels.into_iter().enumerate() {
                    self.placed.push(Placed {
                        model,
                        lod,
                        levels: m.lods.len(),
                        rows,
                        centre,
                        radius,
                        seen,
                        keep: (lo, hi),
                        second: i > 0,
                    });
                }
            }
        }
    }

    /// The far trees for `frame`: the set on the device (made again when the set or the pack
    /// changes) and this frame's constants, the blocks whose own objects stand in the world left
    /// out; nothing while the frame carries no far trees.
    fn prepare_far(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &HifiFrame,
        clip_from_render: Mat4,
    ) {
        let (Some(gpu), Some(set), true) = (
            self.gpu.as_ref(),
            frame.far_trees.as_ref(),
            frame.far.trees && frame.far.reach > 0.0,
        ) else {
            self.far = None;
            return;
        };
        if self.far.as_ref().is_none_or(|f| f.serial != set.serial) {
            let started = std::time::Instant::now();
            let of = |id: u32| gpu.far_of.get(&id).copied();
            let taken = self.far.as_mut().is_some_and(|f| f.take(queue, set, &of));
            if !taken {
                // Room for half as many again as the set, so the blocks coming within reach fit
                // however the room the blocks leaving free is cut up, and a thicker forest ahead
                // fits for a good while before the buffers are made again.
                let room = far::FarGpu::room_for(set, &of);
                let room = room.saturating_add(room / 2).max(4096);
                let mut made = far::FarGpu::new(device, &gpu.far, gpu.far_models.clone(), room);
                made.take(queue, set, &of);
                self.far = Some(made);
            }
            self.far_taken_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        }
        let Some(far) = self.far.as_mut() else {
            return;
        };
        let Some(global) = render_origin(frame) else {
            return;
        };
        let left_out: HashSet<u16> = self
            .blocks
            .iter()
            .filter(|(_, (_, objects, _))| *objects)
            .map(|(b, _)| *b)
            .collect();
        let cam = &frame.camera;
        let index_count = |m: &far::FarModel| {
            gpu.models[m.model].lods[m.lod]
                .primitives
                .first()
                .map_or(0, |p| p.count)
        };
        far.prepare(
            queue,
            clip_from_render,
            cam.eye,
            global,
            &frame.far,
            far::pixels_a_meter(cam.projection, cam.viewport[3]),
            &left_out,
            &index_count,
        );
    }

    /// Draw the far trees into `pass`, the re-shaded world's, model by model, each draw's count
    /// written by the culling.
    fn draw_far(&self, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let (Some(gpu), Some(far)) = (self.gpu.as_ref(), self.far.as_ref()) else {
            return false;
        };
        pass.set_pipeline(&gpu.view_far);
        pass.set_bind_group(0, &gpu.frame_bind, &[]);
        pass.set_bind_group(2, &far.draw_bind, &[]);
        for (k, m) in far.models.iter().enumerate() {
            let Some(p) = gpu.models[m.model].lods[m.lod].primitives.first() else {
                continue;
            };
            pass.set_bind_group(1, &p.material, &[]);
            pass.set_vertex_buffer(0, p.vertices.slice(..));
            pass.set_index_buffer(p.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed_indirect(&far.draws, far::FarGpu::draw_offset(k));
        }
        true
    }

    /// Draw the view's runs into `pass`, the re-shaded world's.
    fn draw_view(&self, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(gpu) = self.gpu.as_ref() else {
            return false;
        };
        let Some((instances, _)) = gpu.view_instances.as_ref() else {
            return false;
        };
        if self.view.is_empty() {
            return false;
        }
        pass.set_bind_group(0, &gpu.frame_bind, &[]);
        // The pipeline last set: for pictures, for both faces, or for the front alone.
        let mut current: Option<u8> = None;
        for b in &self.view {
            let lod = &gpu.models[b.model].lods[b.lod];
            for p in &lod.primitives {
                let (which, pipeline) = if lod.pictures {
                    (0, &gpu.view_pictures)
                } else if p.double_sided {
                    (1, &gpu.view_both)
                } else {
                    (2, &gpu.view_culled)
                };
                if current != Some(which) {
                    pass.set_pipeline(pipeline);
                    current = Some(which);
                }
                pass.set_bind_group(1, &p.material, &[]);
                pass.set_vertex_buffer(0, p.vertices.slice(..));
                pass.set_vertex_buffer(1, instances.slice(..));
                pass.set_index_buffer(p.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..p.count, 0, b.instances.clone());
            }
        }
        true
    }
}

impl PackCasters {
    /// Whether the pack's models are drawn in this frame, in place of the objects they replace:
    /// the sun's shadows then take the models, not the objects.
    #[must_use]
    pub fn replacing(&self) -> bool {
        self.0.borrow().active
    }

    /// Work out which models each of the cascades `light_from_render` holds, and hand the
    /// device their transforms.
    pub fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        light_from_render: &[Mat4; CASCADES],
    ) {
        let mut guard = self.0.borrow_mut();
        let s = &mut *guard;
        let mut rows: Vec<Vec4> = Vec::new();
        let mut uniform = vec![0u8; CASTER_BLOCK * CASCADES];
        let floors = caster_floors();
        for (c, m) in light_from_render.iter().enumerate() {
            let at = c * CASTER_BLOCK;
            uniform[at..at + 64].copy_from_slice(&floats(&m.to_cols_array()));
            // How far one metre reaches across the cascade's square, in its clip units.
            let across = m.row(0).xyz().length().max(m.row(1).xyz().length());
            let mut chosen: Vec<(usize, f32)> = s
                .placed
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.second)
                .filter_map(|(i, p)| {
                    let q = *m * p.centre.extend(1.0);
                    let r = p.radius * across;
                    (q.x.abs() <= 1.0 + r && q.y.abs() <= 1.0 + r).then_some((i, q.z))
                })
                .collect();
            // Nearest the sun first, so the leaves behind those already drawn are shaded less.
            chosen.sort_by(|a, b| a.1.total_cmp(&b.1));
            let chosen: Vec<usize> = chosen.into_iter().map(|(i, _)| i).collect();
            let coarse = casters(&s.placed, floors[c]);
            s.casters[c] = batches(&coarse, chosen, &mut rows);
        }
        if let Some(gpu) = s.gpu.as_mut() {
            queue.write_buffer(&gpu.caster_uniform, 0, &uniform);
            write_instances(device, queue, &mut gpu.caster_instances, &rows);
        }
    }

    /// Draw the models cascade `c` holds into `pass`, one of the sun's shadow cascades. The far
    /// trees cast no shadow: each shades itself by its own crown.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, c: usize) {
        let s = self.0.borrow();
        let (Some(gpu), Some(batches)) = (s.gpu.as_ref(), s.casters.get(c)) else {
            return;
        };
        let Some((instances, _)) = gpu.caster_instances.as_ref() else {
            return;
        };
        let offset = u32::try_from(c as u64 * CASTER_STRIDE).unwrap_or(0);
        pass.set_bind_group(0, &gpu.caster_bind, &[offset]);
        let mut cut: Option<bool> = None;
        let proxies = c >= proxy_from();
        for b in batches {
            let model = &gpu.models[b.model];
            // Pictures of the model cast no shadow of their own: its stand-in casts it.
            let pictures = model.lods[b.lod].pictures;
            let proxy = model.proxy.as_ref().filter(|_| proxies || pictures);
            if let (Some((vertices, indices, count)), Some(first)) =
                (proxy, model.lods[b.lod].primitives.first())
            {
                if cut != Some(false) {
                    pass.set_pipeline(&gpu.caster_solid);
                    cut = Some(false);
                }
                pass.set_bind_group(1, &first.material, &[]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_vertex_buffer(1, instances.slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*count, 0, b.instances.clone());
            }
            for p in &model.lods[b.lod].primitives {
                // The stand-in takes the place of the leaves; the trunk is drawn as it is.
                if pictures || (proxy.is_some() && p.cut_out) {
                    continue;
                }
                if cut != Some(p.cut_out) {
                    pass.set_pipeline(if p.cut_out {
                        &gpu.caster_cut
                    } else {
                        &gpu.caster_solid
                    });
                    cut = Some(p.cut_out);
                }
                pass.set_bind_group(1, &p.material, &[]);
                pass.set_vertex_buffer(0, p.vertices.slice(..));
                pass.set_vertex_buffer(1, instances.slice(..));
                pass.set_index_buffer(p.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..p.count, 0, b.instances.clone());
            }
        }
    }
}

/// Where render space's origin lies in the world, meters east and north: the drawn ground's,
/// or worked out from a resident block's place.
fn render_origin(frame: &HifiFrame) -> Option<Vec2> {
    if let Some(d) = frame.drawn.as_ref() {
        return Some(d.global);
    }
    let t = frame.terrain.first()?;
    let block = Vec2::new(f32::from(t.block >> 8), f32::from(t.block & 0xFF)) * 192.0;
    Some(block - t.origin.xy())
}

/// The frame's constants: the transforms to clip and view space, the eye, the sun, the old light
/// the light per pixel divides out, the frame's place in the anti-aliasing's dither, and the
/// ambient light the world's objects are drawn with.
fn frame_constants(frame: &HifiFrame, clip_from_render: Mat4) -> Vec<f32> {
    let cam = &frame.camera;
    let (sun, amb) = legacy_light(frame);
    let objects = object_ambient(frame);
    let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
    let mut c = Vec::with_capacity(56);
    c.extend(clip_from_render.to_cols_array());
    c.extend((cam.view * HEIGHT_UP).to_cols_array());
    c.extend([cam.eye.x, cam.eye.y, cam.eye.z, 0.0]);
    c.extend([l.x, l.y, l.z, frame.pack_light.glow]);
    c.extend([sun[0], sun[1], sun[2], 0.0]);
    c.extend([amb[0], amb[1], amb[2], 0.0]);
    c.extend([cam.dither, 0.0, 0.0, 0.0]);
    c.extend([objects[0], objects[1], objects[2], 0.0]);
    c
}

impl HifiPass for ArtPack {
    fn name(&self) -> &'static str {
        "art pack"
    }

    fn notes(&self, notes: &mut Vec<(&'static str, u64)>) {
        let s = self.0.borrow();
        notes.push(("art pack models drawn", s.drawn_instances));
        notes.push((
            "art pack far trees",
            s.far.as_ref().map_or(0, |f| u64::from(f.standing())),
        ));
        notes.push((
            "art pack far tree bytes",
            s.far.as_ref().map_or(0, |f| f.bytes),
        ));
        notes.push(("art pack far trees prepare us", s.far_prepare_us));
        notes.push(("art pack far trees taken us", s.far_taken_us));
        notes.push(("art pack draws replaced", s.covered.len() as u64));
        notes.push(("art pack runs looked at", s.covered.looked()));
        notes.push((
            "art pack picture bytes",
            s.gpu.as_ref().map_or(0, |g| g.picture_bytes),
        ));
    }

    fn observe(&mut self, cx: &mut PrepareCx<'_, '_>) {
        let mut s = self.0.borrow_mut();
        let frame = cx.frame;
        s.active = false;
        s.drawn = false;
        s.pack.clone_from(&frame.art_pack);
        for b in &frame.pack_feed {
            s.blocks
                .insert(b.block, (b.generation, b.objects, b.placements.clone()));
        }
        let resident = crate::shared::resident::ResidentBlocks::of(&frame.terrain);
        s.blocks.retain(|id, _| resident.holds(*id));
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        draws(s, f) && self.0.borrow().ready(f)
    }

    fn warm(&mut self, cx: &mut WarmCx<'_>) {
        let mut s = self.0.borrow_mut();
        let Some(pack) = cx.frame.art_pack.clone() else {
            return;
        };
        if !draws(cx.settings, cx.frame) || s.gpu.as_ref().is_some_and(|g| g.serial == pack.serial)
        {
            return;
        }
        let (device, queue) = (cx.device.clone(), cx.queue.clone());
        s.pre_gpu.ask_for(pack.serial, cx.warm, move || {
            PackGpu::new(&device, &queue, &pack)
        });
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        let mut guard = self.0.borrow_mut();
        let s = &mut *guard;
        let Some(pack) = s.pack.clone() else {
            return Ok(());
        };
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        if s.gpu.as_ref().is_none_or(|g| g.serial != pack.serial) {
            let built = s.pre_gpu.take().filter(|g| g.serial == pack.serial);
            s.gpu = Some(built.unwrap_or_else(|| PackGpu::new(&device, &queue, &pack)));
        }
        s.covered.clear();
        s.marked.clear();
        s.first_mark = None;
        s.far_mark = None;
        let mut open = None;
        for (at, mark) in &cx.seam.tables().marks {
            match mark {
                Mark::ArtPackCovered => open = Some(*at),
                Mark::ArtPackCoveredEnd => {
                    if let Some(from) = open.take() {
                        s.covered.push(from..*at);
                    }
                }
                Mark::ArtPackBlock { block } => {
                    s.marked.insert(*block);
                    s.first_mark.get_or_insert(*at);
                }
                Mark::TerrainCellEnd => {
                    s.far_mark.get_or_insert(*at);
                }
                _ => {}
            }
        }
        let cam = &cx.frame.camera;
        let clip_from_render = cam.projection * cam.view * HEIGHT_UP;
        s.place(cx.frame, clip_from_render);
        let chosen: Vec<usize> = s
            .placed
            .iter()
            .enumerate()
            .filter(|(_, p)| p.seen)
            .map(|(i, _)| i)
            .collect();
        s.drawn_instances = chosen.len() as u64;
        let mut rows = Vec::with_capacity(chosen.len() * 3);
        s.view = batches(&s.placed, chosen, &mut rows);
        if let Some(gpu) = s.gpu.as_mut() {
            // Drawn at the frame's jitter, as the world is.
            queue.write_buffer(
                &gpu.frame_uniform,
                0,
                &floats(&frame_constants(cx.frame, cam.jittered(clip_from_render))),
            );
            write_instances(&device, &queue, &mut gpu.view_instances, &rows);
        }
        let started = std::time::Instant::now();
        s.prepare_far(&device, &queue, cx.frame, clip_from_render);
        s.far_prepare_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok(())
    }

    fn replay_filter(&mut self) -> Option<&mut dyn ReplayFilter> {
        Some(self)
    }

    fn planned(&mut self, plan: Option<&ReshadePlan>) {
        let mut s = self.0.borrow_mut();
        s.active = s.gpu.is_some()
            && plan.is_some_and(|p| s.first_mark.is_some_and(|m| p.opaque.contains(&m)));
        s.far_active = s.far.is_some()
            && plan.is_some_and(|p| s.far_mark.is_some_and(|m| p.opaque.contains(&m)));
        s.far_drawn = false;
    }

    fn encode(&mut self, _cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }
}

impl HifiPass for FarCull {
    fn name(&self) -> &'static str {
        "far trees cull"
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        draws(s, f) && f.far_trees.is_some() && self.0.borrow().ready(f)
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let s = self.0.borrow();
        let (Some(gpu), Some(far), true) = (s.gpu.as_ref(), s.far.as_ref(), s.far_active) else {
            return Ok(());
        };
        let timestamps = cx.timer.compute_writes("far trees cull");
        let mut pass = cx.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("far trees cull"),
            timestamp_writes: timestamps,
        });
        far.cull(&mut pass, &gpu.far);
        drop(pass);
        cx.seam.count_pass();
        Ok(())
    }
}

impl ReplayFilter for ArtPack {
    fn draw(&mut self, cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        let s = self.0.borrow();
        if s.active && s.covered.contains(cmd) {
            DrawAction::Skip(SkipRule::ReplacedByPass)
        } else {
            DrawAction::Legacy
        }
    }

    fn mark(&mut self, mark: Mark, pass: &mut wgpu::RenderPass<'_>) -> bool {
        if !matches!(mark, Mark::ArtPackBlock { .. } | Mark::TerrainCellEnd) {
            return false;
        }
        let mut s = self.0.borrow_mut();
        let mut drew = false;
        // The far trees at the end of the first landscape cell's ground in the opaque world,
        // once, where the drawn ground is drawn.
        if matches!(mark, Mark::TerrainCellEnd) && s.far_active && !s.far_drawn {
            s.far_drawn = true;
            drew |= s.draw_far(pass);
        }
        if !matches!(mark, Mark::ArtPackBlock { .. }) || !s.active || s.drawn {
            return drew;
        }
        // The first block's note in the opaque world: every noted block's models are drawn
        // here, once.
        s.drawn = true;
        drew | s.draw_view(pass)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;
    use dereth_primitives::num::math::{cosf, sinf};
    use glam::Quat;

    #[test]
    fn the_pack_shader_parses_and_validates_with_its_entry_points() {
        let module = naga::front::wgsl::parse_str(PACK_WGSL)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(PACK_WGSL)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(
            entries,
            [
                "vs_main",
                "fs_main",
                "vs_impostor",
                "vs_far",
                "fs_impostor",
                "vs_caster",
                "fs_caster_cut"
            ]
        );
    }

    #[test]
    fn the_far_trees_culling_shader_parses_and_validates() {
        let module = naga::front::wgsl::parse_str(far::FAR_WGSL)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(far::FAR_WGSL)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(entries, ["cs_cull"]);
    }

    #[test]
    fn a_pack_surface_is_drawn_with_the_brighter_ambient_the_worlds_objects_take() {
        let mut frame = HifiFrame::default();
        frame.sky.ambient_level = 0.35;
        frame.sky.ambient_color = [255, 204, 102];
        frame.sky.sun_brightness = 1.5;
        frame.sky.sun_color = [255, 255, 255];
        let c = frame_constants(&frame, Mat4::IDENTITY);
        assert_eq!(
            c.len(),
            56,
            "two transforms and six rows, as the shader's frame"
        );
        assert!(c.len() * 4 <= 256, "inside the frame's uniform");
        // The landscape's ambient, which the light per pixel divides out, and the objects': a
        // fifth of the sun's brightness more.
        let (landscape, objects) = (&c[44..47], &c[52..55]);
        for (k, colour) in [1.0f32, 0.8, 0.4].into_iter().enumerate() {
            assert!((landscape[k] - colour * 0.35).abs() < 1e-5, "{landscape:?}");
            assert!(
                (objects[k] - colour * (0.35 + 0.3)).abs() < 1e-5,
                "{objects:?}"
            );
        }
    }

    /// The leaf corners of a cone of a crown round (1, -1): forty heights from 2 to 10 metres,
    /// at each a ring of sixteen at the cone's reach (4 metres at the bottom to none at the top)
    /// and one of eight at half of it.
    fn cone() -> Vec<Vec3> {
        let mut points = Vec::new();
        for h in 0..40u8 {
            let z = 2.0 + 8.0 * f32::from(h) / 39.0;
            let reach = 4.0 * (1.0 - (z - 2.0) / 8.0);
            for (n, r) in [(16u8, reach), (8, reach * 0.5)] {
                for k in 0..n {
                    let a = std::f32::consts::TAU * f32::from(k) / f32::from(n);
                    points.push(Vec3::new(1.0 + cosf(a) * r, -1.0 + sinf(a) * r, z));
                }
            }
        }
        points
    }

    #[test]
    fn a_crowns_reach_follows_its_leaves_band_by_band() {
        let crown = Crown::of(&cone()).expect("a crown");
        assert!(
            crown.axis.distance(Vec2::new(1.0, -1.0)) < 1e-3,
            "{crown:?}"
        );
        assert!((crown.base - 2.0).abs() < 1e-5 && (crown.top - 10.0).abs() < 1e-5);
        // Nine in ten of each band's corners: the outer ring, near the bottom of the band.
        assert!(crown.reach[0] > 3.4 && crown.reach[0] <= 4.0, "{crown:?}");
        assert!(
            crown.reach.windows(2).all(|w| w[1] < w[0]),
            "a cone narrows as it rises: {crown:?}"
        );
    }

    #[test]
    fn a_band_without_leaves_takes_the_reach_of_the_bands_round_it() {
        let ring = |z: f32, r: f32| {
            (0..16u8).map(move |k| {
                let a = std::f32::consts::TAU * f32::from(k) / 16.0;
                Vec3::new(cosf(a) * r, sinf(a) * r, z)
            })
        };
        // Leaves only in the lowest and the highest eighth of the height.
        let points: Vec<Vec3> = ring(0.0, 4.0).chain(ring(8.0, 2.0)).collect();
        let crown = Crown::of(&points).expect("a crown");
        assert!((crown.reach[0] - 4.0).abs() < 1e-4, "{crown:?}");
        assert!((crown.reach[7] - 2.0).abs() < 1e-4, "{crown:?}");
        for r in &crown.reach[1..7] {
            assert!((r - 3.0).abs() < 1e-4, "{crown:?}");
        }
    }

    #[test]
    fn a_handful_of_leaves_or_a_flat_spread_of_them_makes_no_crown() {
        assert_eq!(Crown::of(&cone()[..5]), None);
        let flat: Vec<Vec3> = (0..20u8)
            .map(|k| Vec3::new(f32::from(k), 0.0, 1.0 + f32::from(k) * 0.001))
            .collect();
        assert_eq!(Crown::of(&flat), None);
    }

    #[test]
    fn a_crowns_solid_stand_in_is_closed_and_inside_the_crown() {
        let crown = Crown::of(&cone()).expect("a crown");
        let (vertices, indices) = crown.proxy(0.8, 8);
        let stride = crate::snapshot::PACK_VERTEX_FLOATS;
        assert_eq!(vertices.len() % stride, 0);
        let count = vertices.len() / stride;
        assert_eq!(count, CROWN_RINGS * 8 + 2);
        assert_eq!(indices.len() % 3, 0);
        assert!(indices.iter().all(|i| (*i as usize) < count));
        // Closed: every edge is shared by exactly two triangles.
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for t in indices.as_chunks::<3>().0 {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edges.values().all(|n| *n == 2), "{edges:?}");
        // Within the crown: no wider than its reach, no lower or higher than its leaves.
        let widest = crown.reach.iter().copied().fold(0.0, f32::max);
        for v in vertices.chunks_exact(stride) {
            let p = Vec3::new(v[0], v[1], v[2]);
            assert!((p.xy() - crown.axis).length() <= widest * 0.8 + 1e-4);
            assert!(p.z >= crown.base - 1e-4 && p.z <= crown.top + 1e-4);
            // An opaque white.
            assert_eq!(&v[12..16], &[1.0; 4]);
        }
    }

    #[test]
    fn a_model_draws_the_first_level_whose_distance_reaches_it_over_its_scale() {
        let lods = [40.0, 120.0, f32::INFINITY];
        assert_eq!(level_at(&lods, 10.0, 1.0), Some(0));
        assert_eq!(level_at(&lods, 40.0, 1.0), Some(0));
        assert_eq!(level_at(&lods, 41.0, 1.0), Some(1));
        assert_eq!(level_at(&lods, 5000.0, 1.0), Some(2));
        // A tree twice the size keeps its detail twice as far.
        assert_eq!(level_at(&lods, 70.0, 2.0), Some(0));
        assert_eq!(level_at(&lods, 50.0, 0.5), Some(1));
        // Past a last level with a limit, nothing is drawn.
        assert_eq!(level_at(&[40.0, 120.0], 300.0, 1.0), None);
    }

    #[test]
    fn a_shadow_cascade_draws_a_model_at_its_own_level_however_far_the_model_stands() {
        assert_eq!(caster_level(3, 0), 0);
        assert_eq!(caster_level(3, 2), 2);
        assert_eq!(caster_level(3, 1), 1);
        assert_eq!(caster_level(2, 2), 1, "no level past the model's last");
        assert_eq!(caster_level(1, 2), 0);
        // A tree the view draws at its first level, and the same tree walked out to where the
        // view draws its pictures: each cascade casts the same shadow for both, so a shadow at
        // the eye's feet does not turn into the crown's solid stand-in as its tree recedes.
        let tree = |lod: usize| Placed {
            model: 0,
            lod,
            levels: 3,
            rows: [Vec4::X, Vec4::Y, Vec4::Z],
            centre: Vec3::ZERO,
            radius: 5.0,
            seen: false,
            keep: (0.0, 1.0),
            second: false,
        };
        for floor in CASTER_FLOORS {
            let near = casters(&[tree(0)], floor);
            let far = casters(&[tree(2)], floor);
            assert_eq!(near[0].lod, far[0].lod, "floor {floor}");
            assert_eq!(near[0].lod, floor.min(2));
        }
    }

    #[test]
    fn a_placement_moves_turns_and_scales_its_model_into_render_space() {
        let p = HifiPackPlacement {
            id: 0x0200_0258,
            origin: Vec3::new(10.0, 20.0, 5.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: 2.0,
        };
        let rows = placement_rows(&p, Vec3::new(192.0, 384.0, 0.0));
        let at = |v: Vec3| {
            let q = v.extend(1.0);
            Vec3::new(rows[0].dot(q), rows[1].dot(q), rows[2].dot(q))
        };
        assert!(at(Vec3::ZERO).distance(Vec3::new(202.0, 404.0, 5.0)) < 1e-4);
        // A quarter turn about the vertical takes east to north, at twice the length.
        assert!(at(Vec3::X).distance(Vec3::new(202.0, 406.0, 5.0)) < 1e-4);
        assert!(at(Vec3::Z).distance(Vec3::new(202.0, 404.0, 7.0)) < 1e-4);
    }

    #[test]
    fn a_sphere_is_seen_while_any_of_it_is_inside_the_view() {
        let view = Mat4::look_at_lh(Vec3::ZERO, Vec3::Y, Vec3::Z);
        let projection = Mat4::perspective_lh(1.2, 1.5, 0.5, 1000.0);
        let planes = frustum(projection * view);
        assert!(sphere_seen(&planes, Vec3::new(0.0, 50.0, 0.0), 1.0));
        assert!(
            !sphere_seen(&planes, Vec3::new(0.0, -50.0, 0.0), 1.0),
            "behind the eye"
        );
        assert!(
            !sphere_seen(&planes, Vec3::new(500.0, 50.0, 0.0), 1.0),
            "far to the side"
        );
        assert!(
            sphere_seen(&planes, Vec3::new(0.0, -2.0, 0.0), 3.0),
            "round the eye"
        );
        assert!(
            !sphere_seen(&planes, Vec3::new(0.0, 2000.0, 0.0), 1.0),
            "past the far plane"
        );
    }

    #[test]
    fn instances_of_one_level_of_one_model_are_drawn_together() {
        let at = |model, lod| Placed {
            model,
            lod,
            levels: 3,
            rows: [Vec4::ZERO; 3],
            centre: Vec3::ZERO,
            radius: 1.0,
            seen: true,
            keep: (0.25, 1.0),
            second: false,
        };
        let placed = [at(1, 0), at(0, 2), at(1, 0), at(0, 2), at(0, 1)];
        // Ten instances of an earlier run are already in the buffer.
        let mut rows = vec![Vec4::ONE; 40];
        let runs = batches(&placed, vec![0, 1, 2, 3, 4], &mut rows);
        assert_eq!(
            runs,
            [
                Batch {
                    model: 0,
                    lod: 1,
                    instances: 10..11
                },
                Batch {
                    model: 0,
                    lod: 2,
                    instances: 11..13
                },
                Batch {
                    model: 1,
                    lod: 0,
                    instances: 13..15
                },
            ]
        );
        assert_eq!(rows.len(), 60);
        // Each instance's fourth row is the dither its level keeps.
        assert_eq!(rows[43], Vec4::new(0.25, 1.0, 0.0, 0.0));
    }

    #[test]
    fn a_level_of_pictures_reads_its_grid_and_its_quad_from_its_vertices() {
        let stride = crate::snapshot::PACK_VERTEX_FLOATS;
        let mut quad = vec![0.0; stride * 4];
        for (k, (x, z)) in [(-8.0, -1.5), (8.0, -1.5), (8.0, 22.5), (-8.0, 22.5)]
            .into_iter()
            .enumerate()
        {
            quad[k * stride] = x;
            quad[k * stride + 2] = z;
        }
        let p = Pictures::of((16, 8), &quad);
        assert_eq!(p.grid, [16.0, 8.0, 2.0]);
        assert_eq!(p.quad, [8.0, -1.5, 22.5]);
        // Fewer tiles to a row than views makes more rows; more is held to one row.
        assert_eq!(Pictures::of((12, 5), &quad).grid, [12.0, 5.0, 3.0]);
        assert_eq!(Pictures::of((6, 9), &quad).grid, [6.0, 6.0, 1.0]);
    }

    #[test]
    fn away_from_a_change_of_level_one_level_keeps_the_whole_dither() {
        let lods = [40.0, 120.0, f32::INFINITY];
        assert_eq!(levels_at(&lods, 10.0, 1.0, 0.2), [(0, 0.0, 1.0)]);
        assert_eq!(levels_at(&lods, 80.0, 1.0, 0.2), [(1, 0.0, 1.0)]);
        assert_eq!(levels_at(&lods, 5000.0, 1.0, 0.2), [(2, 0.0, 1.0)]);
        // With no band the levels pop, as `level_at` has them.
        assert_eq!(levels_at(&lods, 41.0, 1.0, 0.0), [(1, 0.0, 1.0)]);
    }

    #[test]
    fn across_a_change_of_level_one_level_fades_into_the_next() {
        let lods = [40.0, 120.0, f32::INFINITY];
        // The band runs 36..44 metres: at its start the nearer level keeps all, at its middle
        // half, near its end almost none, and the two always share the whole dither.
        let at = |d: f32| levels_at(&lods, d, 1.0, 0.2);
        let start = at(36.01);
        assert_eq!(start.len(), 2);
        assert!(start[0].1 < 0.01 && start[1].2 < 0.01, "{start:?}");
        let middle = at(40.0);
        assert_eq!((middle[0].0, middle[1].0), (0, 1));
        assert!((middle[0].1 - 0.5).abs() < 1e-5, "{middle:?}");
        assert_eq!(middle[0].1, middle[1].2, "the shares meet");
        assert_eq!(
            (middle[0].2, middle[1].1),
            (1.0, 0.0),
            "and cover the whole dither"
        );
        let end = at(43.99);
        assert!(end[0].1 > 0.99, "{end:?}");
        // A model twice the size changes level twice as far.
        assert_eq!(levels_at(&lods, 80.0, 2.0, 0.2).len(), 2);
    }

    #[test]
    fn past_a_last_level_with_a_limit_the_model_fades_out() {
        let lods = [40.0, 120.0];
        let fading = levels_at(&lods, 120.0, 1.0, 0.2);
        assert_eq!(fading.len(), 1);
        assert_eq!(fading[0].0, 1);
        assert!((fading[0].1 - 0.5).abs() < 1e-5, "{fading:?}");
        assert!(levels_at(&lods, 140.0, 1.0, 0.2).is_empty());
    }
}
