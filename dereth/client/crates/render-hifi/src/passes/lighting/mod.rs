//! Per-pixel sun and sky light, sun shadows, point lights, high dynamic range, exposure, bloom
//! and tone mapping.
//!
//! Two passes share the work:
//!
//! - [`Lighting`], over the opaque world: it draws the sun's shadow cascades from the retained
//!   landscape and static geometry of every resident block (so hills and houses off screen still
//!   cast), then lights every re-shaded pixel again. The vertex light each surface was drawn with
//!   is divided out of its colour, and the sun (shadowed through soft cascades and short
//!   screen-space contact rays, which catch the characters), a sky hemisphere and the scene's
//!   point lights are put back, in linear high dynamic range.
//! - [`LightingPost`], at the high dynamic range slot: bloom, exposure, a filmic tone map (ACES,
//!   or AgX) and a light grade, written into the picture.

pub mod derive;

use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::shared::camera::{render_from_clip, HEIGHT_UP};
use crate::shared::fullscreen::COLOUR_WGSL;
use crate::{Caps, DebugView, HifiError, HifiFrame, HifiSettings, Level};

const SHADOW_WGSL: &str = include_str!("shadow.wgsl");
const RELIGHT_WGSL: &str = include_str!("relight.wgsl");
const POST_WGSL: &str = include_str!("post.wgsl");

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(crate) const CASCADES: usize = 4;
/// [`CASCADES`], as the shadow map's layer count.
const CASCADE_LAYERS: u32 = 4;
/// The far distance of each cascade, metres.
pub(crate) const SPLITS: [f32; CASCADES] = [14.0, 45.0, 150.0, 560.0];
/// How far toward the sun casters are looked for beyond a cascade's own sphere, metres.
const CASTER_REACH: f32 = 900.0;
const MAX_LIGHTS: usize = 128;
const CASTER_STRIDE: u64 = 256;
const BLOOM_LEVELS: u32 = 6;

pub(crate) fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

pub(crate) fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub(crate) fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        dereth_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
    }
}

/// The sun and ambient light as the vertex light took them, 0..1 per channel.
pub(crate) fn legacy_light(frame: &HifiFrame) -> ([f32; 3], [f32; 3]) {
    let sky = &frame.sky;
    let sun = sky
        .sun_color
        .map(|c| f32::from(c) / 255.0 * sky.sun_brightness);
    let amb = sky
        .ambient_color
        .map(|c| f32::from(c) / 255.0 * sky.ambient_level.max(0.2));
    (sun, amb)
}

/// Whether the sun is up far enough to shade and shadow.
pub(crate) fn sun_up(frame: &HifiFrame) -> bool {
    frame.sky.outdoor && frame.sky.sun_direction.z > 0.02 && frame.sky.sun_brightness > 0.02
}

/// The exposure for the frame's light: a key of the sun and sky, held to a sane range so night
/// stays night and noon is not crushed.
pub(crate) fn exposure(frame: &HifiFrame, sun_gain: f32, amb_gain: f32) -> f32 {
    let (sun, amb) = legacy_light(frame);
    let lum = |c: [f32; 3]| {
        let l = c.map(|x| dereth_primitives::num::math::powf(x.max(0.0), 2.2));
        0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2]
    };
    let s = if sun_up(frame) {
        lum(sun) * sun_gain * frame.sky.sun_direction.z.clamp(0.2, 1.0)
    } else {
        0.0
    };
    let key = s + lum(amb) * amb_gain;
    // A gentle lift where the light is dim, so dusk reads, without turning night into day.
    let lift = (0.35 / key.max(1e-3)).clamp(1.0, 1.6);
    env_f32("DERETH_HIFI_EXPOSURE", 1.8) * lift
}

/// The retained shadow geometry of one block.
#[derive(Debug)]
struct CasterBlock {
    generation: u32,
    buffer: Option<wgpu::Buffer>,
    vertices: u32,
    /// Cut-out batches: positions and texture coordinates, and the texture slot.
    cutouts: Vec<(wgpu::Buffer, u32, u32)>,
}

/// Where a vertex format's first texture coordinates sit, bytes.
fn uv_offset(format: dereth_render::VertexFormat) -> usize {
    if crate::derive::reshade::has_normal(format) {
        28
    } else {
        16
    }
}

fn read_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// A cut-out batch's positions and texture coordinates, five floats a vertex.
fn cutout_vertices(batch: &crate::snapshot::HifiStaticBatch) -> Vec<f32> {
    let stride = batch.format.stride() as usize;
    let count = batch.bytes.len() / stride;
    let usable = count - count % 3;
    let uv = uv_offset(batch.format);
    let mut out = Vec::with_capacity(usable * 5);
    for v in 0..usable {
        let at = v * stride;
        out.extend_from_slice(&[
            read_f32(&batch.bytes, at),
            read_f32(&batch.bytes, at + 4),
            read_f32(&batch.bytes, at + 8),
            read_f32(&batch.bytes, at + uv),
            read_f32(&batch.bytes, at + uv + 4),
        ]);
    }
    out
}

/// The vertex positions of a block's retained geometry, block-local, three per triangle.
fn caster_positions(block: &crate::snapshot::HifiStaticBlock) -> Vec<f32> {
    let mut out = Vec::with_capacity(block.terrain_triangles.len() * 9);
    for t in &block.terrain_triangles {
        for p in t {
            out.extend_from_slice(&[p.x, p.y, p.z]);
        }
    }
    for batch in &block.batches {
        if batch.format.is_pre_transformed() || (batch.cutout && batch.texture.is_some()) {
            continue;
        }
        let stride = batch.format.stride() as usize;
        let count = batch.bytes.len() / stride;
        let usable = count - count % 3;
        for v in 0..usable {
            let at = v * stride;
            let f = |k: usize| {
                let b = &batch.bytes[at + k * 4..at + k * 4 + 4];
                f32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            out.extend_from_slice(&[f(0), f(1), f(2)]);
        }
    }
    out
}

#[derive(Debug)]
pub(crate) struct Cascades {
    pub(crate) matrices: [Mat4; CASCADES],
    pub(crate) texel: [f32; CASCADES],
    pub(crate) depth_range: [f32; CASCADES],
}

/// Stabilised cascades around the camera's frustum slices, looking down the sun.
#[allow(clippy::needless_range_loop)] // each cascade fills three tables at its index
pub(crate) fn cascades(frame: &HifiFrame, size: u32) -> Cascades {
    let cam = &frame.camera;
    let inv = render_from_clip(cam.view, cam.projection);
    let un = |x: f32, y: f32| {
        let p = inv * Vec4::new(x, y, 0.5, 1.0);
        p.xyz() / p.w
    };
    let eye = cam.eye;
    let forward = (un(0.0, 0.0) - eye).normalize_or(Vec3::Y);
    let rays: Vec<Vec3> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(x, y)| un(*x, *y) - eye)
        .collect();
    let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
    let up = if l.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
    let light_view = Mat4::look_at_rh(Vec3::ZERO, -l, up);
    let mut out = Cascades {
        matrices: [Mat4::IDENTITY; CASCADES],
        texel: [1.0; CASCADES],
        depth_range: [1.0; CASCADES],
    };
    let mut near = cam.near.max(0.05);
    for c in 0..CASCADES {
        let far = SPLITS[c].min(cam.far.max(near + 1.0));
        let mut corners = Vec::with_capacity(8);
        for d in [near, far] {
            for r in &rays {
                corners.push(eye + *r * (d / r.dot(forward).max(1e-3)));
            }
        }
        let centre = corners.iter().copied().sum::<Vec3>() / 8.0;
        let mut radius = corners
            .iter()
            .map(|p| (*p - centre).length())
            .fold(0.0f32, f32::max);
        radius = (radius * 16.0).ceil() / 16.0;
        let texel = 2.0 * radius / size as f32;
        let mut cs = light_view * centre.extend(1.0);
        cs.x = (cs.x / texel).floor() * texel;
        cs.y = (cs.y / texel).floor() * texel;
        let z_near = -cs.z - radius - CASTER_REACH;
        let z_far = -cs.z + radius;
        let proj = Mat4::orthographic_rh(
            cs.x - radius,
            cs.x + radius,
            cs.y - radius,
            cs.y + radius,
            z_near,
            z_far,
        );
        out.matrices[c] = proj * light_view;
        out.texel[c] = texel;
        out.depth_range[c] = z_far - z_near;
        near = far;
    }
    out
}

#[derive(Debug)]
struct LightingGpu {
    caster_layout: wgpu::BindGroupLayout,
    caster_pipeline: wgpu::RenderPipeline,
    cutout_pipeline: wgpu::RenderPipeline,
    leaf_layout: wgpu::BindGroupLayout,
    leaf_sampler: wgpu::Sampler,
    caster_uniform: wgpu::Buffer,
    caster_capacity: u64,
    caster_bind: wgpu::BindGroup,
    relight_layout: wgpu::BindGroupLayout,
    relight: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    params: wgpu::Buffer,
    lights: wgpu::Buffer,
    cmp: wgpu::Sampler,
}

fn tex_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
    dim: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    }
}

fn buf_entry(binding: u32, stage: wgpu::ShaderStages, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stage,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn fullscreen(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    target: wgpu::ColorTargetState,
) -> wgpu::RenderPipeline {
    fullscreen_targets(device, layout, module, entry, &[Some(target)])
}

fn fullscreen_targets(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    targets: &[Option<wgpu::ColorTargetState>],
) -> wgpu::RenderPipeline {
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("high-fidelity lighting"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(entry),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn caster_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("high-fidelity shadow casters"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: wgpu::BufferSize::new(80),
            }),
        }],
    })
}

impl LightingGpu {
    fn new(device: &wgpu::Device) -> Self {
        let caster_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity shadow casters"),
            entries: &[buf_entry(0, wgpu::ShaderStages::VERTEX, true)],
        });
        let shadow_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity shadow casters"),
            source: wgpu::ShaderSource::Wgsl(SHADOW_WGSL.into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity shadow casters"),
            bind_group_layouts: &[Some(&caster_layout)],
            immediate_size: 0,
        });
        let caster_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("high-fidelity shadow casters"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                unclipped_depth: device
                    .features()
                    .contains(wgpu::Features::DEPTH_CLIP_CONTROL),
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
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
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let leaf_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity leaf shadow"),
            entries: &[
                tex_entry(
                    0,
                    wgpu::TextureSampleType::Float { filterable: true },
                    wgpu::TextureViewDimension::D2,
                ),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let cut_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity leaf shadow"),
            bind_group_layouts: &[Some(&caster_layout), Some(&leaf_layout)],
            immediate_size: 0,
        });
        let cutout_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("high-fidelity leaf shadow"),
            layout: Some(&cut_pl),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_cut"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                unclipped_depth: device
                    .features()
                    .contains(wgpu::Features::DEPTH_CLIP_CONTROL),
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
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
            fragment: Some(wgpu::FragmentState {
                module: &shadow_module,
                entry_point: Some("fs_cut"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        let leaf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("high-fidelity leaf shadow"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let caster_capacity = 512;
        let caster_uniform = uniform(
            device,
            "high-fidelity shadow casters",
            caster_capacity * CASTER_STRIDE,
        );
        let caster_bind = caster_bind(device, &caster_layout, &caster_uniform);

        let float = wgpu::TextureSampleType::Float { filterable: false };
        let d2 = wgpu::TextureViewDimension::D2;
        let relight_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity relight"),
            entries: &[
                tex_entry(0, float, d2),
                tex_entry(1, float, d2),
                tex_entry(2, wgpu::TextureSampleType::Depth, d2),
                tex_entry(
                    3,
                    wgpu::TextureSampleType::Depth,
                    wgpu::TextureViewDimension::D2Array,
                ),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                buf_entry(5, wgpu::ShaderStages::FRAGMENT, false),
                buf_entry(6, wgpu::ShaderStages::FRAGMENT, false),
                tex_entry(7, float, d2),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity relight"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{RELIGHT_WGSL}\n{COLOUR_WGSL}\n{COPY_WGSL}").into(),
            ),
        });
        let target = wgpu::ColorTargetState {
            format: HDR,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let relight = fullscreen_targets(
            device,
            &relight_layout,
            &module,
            "fs_relight",
            &[Some(target.clone()), Some(target.clone())],
        );
        let copy = fullscreen(device, &relight_layout, &module, "fs_copy", target);
        let cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("high-fidelity shadow compare"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            caster_layout,
            caster_pipeline,
            cutout_pipeline,
            leaf_layout,
            leaf_sampler,
            caster_uniform,
            caster_capacity,
            caster_bind,
            relight_layout,
            relight,
            copy,
            params: uniform(device, "high-fidelity relight", 1024),
            lights: uniform(device, "high-fidelity lights", (MAX_LIGHTS * 32) as u64),
            cmp,
        }
    }
}

const COPY_WGSL: &str = "
@fragment
fn fs_copy(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(picture, vec2<i32>(p.xy), 0);
}
";

/// A texture and its view, with the size it was made at.
#[derive(Debug)]
struct Held {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: (u32, u32),
}

fn hdr_texture(device: &wgpu::Device, label: &str, size: (u32, u32), mips: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.0.max(1),
            height: size.1.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: HDR,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// The lighting pass over the opaque world.
#[derive(Debug, Default)]
pub struct Lighting {
    gpu: Option<LightingGpu>,
    blocks: HashMap<u16, CasterBlock>,
    /// Whether this frame steps indoors: the pass then leaves it as re-shaded.
    split: bool,
    src: Option<Held>,
    shadow: Option<(
        wgpu::Texture,
        Vec<wgpu::TextureView>,
        wgpu::TextureView,
        u32,
    )>,
    /// The traced scene the lamps' light is traced through, on a device with ray queries.
    rt: crate::passes::rt::RtScene,
    /// A lamp picture of one unlit texel, bound when the lamps draw nothing.
    no_lamps: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The outdoor lamps' light after dusk.
    lamps: crate::passes::lamps::LampLight,
}

/// The side of each shadow cascade's map: one texel, only so the light has a map to bind, when
/// no shadow is drawn.
fn shadow_size(s: &HifiSettings) -> u32 {
    if shadows_wanted(s) {
        4096
    } else {
        1
    }
}

fn shadows_wanted(s: &HifiSettings) -> bool {
    s.shadows.is_on() || matches!(s.lighting, Level::Medium | Level::High | Level::Ultra)
}

impl HifiPass for Lighting {
    fn name(&self) -> &'static str {
        "lighting"
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        s.lighting.is_on()
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        let frame = cx.frame;
        if std::env::var_os("DERETH_HIFI_LIGHTING_TRACE").is_some() {
            let (sun, amb) = legacy_light(frame);
            eprintln!(
                "lighting: sun {:?} b {} col {:?} amb {} {:?} fog {:?} tod {} legacy sun {sun:?} amb {amb:?} exposure {} lights {} feed {} blocks {}",
                frame.sky.sun_direction,
                frame.sky.sun_brightness,
                frame.sky.sun_color,
                frame.sky.ambient_level,
                frame.sky.ambient_color,
                frame.sky.fog,
                frame.sky.time_of_day,
                exposure(frame, 2.6, 0.85),
                frame.lights.len(),
                frame.static_feed.len(),
                self.blocks.len(),
            );
        }
        self.split =
            crate::reshade::ReshadePlan::new(cx.seam.tables()).is_some_and(|p| p.is_split());
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        for block in &frame.static_feed {
            if self
                .blocks
                .get(&block.block)
                .is_some_and(|b| b.generation == block.generation)
            {
                continue;
            }
            let upload = |data: &[f32]| {
                let bytes = floats(data);
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("high-fidelity shadow casters"),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                queue.write_buffer(&buffer, 0, &bytes);
                buffer
            };
            let positions = caster_positions(block);
            #[allow(clippy::cast_possible_truncation)]
            let vertices = (positions.len() / 3) as u32;
            let buffer = (!positions.is_empty()).then(|| upload(&positions));
            let cutouts = block
                .batches
                .iter()
                .filter(|b| b.cutout && !b.format.is_pre_transformed())
                .filter_map(|b| {
                    let slot = b.texture?;
                    let v = cutout_vertices(b);
                    #[allow(clippy::cast_possible_truncation)]
                    let n = (v.len() / 5) as u32;
                    (n > 0).then(|| (upload(&v), n, slot))
                })
                .collect();
            self.blocks.insert(
                block.block,
                CasterBlock {
                    generation: block.generation,
                    buffer,
                    vertices,
                    cutouts,
                },
            );
        }
        self.blocks
            .retain(|id, _| frame.terrain.iter().any(|t| t.block == *id));
        if crate::passes::lamps::LampLight::wanted(cx.settings.lamps, cx.caps.ray_query) {
            self.rt.prepare(&device, &queue, frame);
        }
        Ok(())
    }

    // The casts are offsets into a constant buffer of a few hundred blocks.
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let Some(rt) = cx.reshade else {
            return Ok(());
        };
        if self.split || !cx.frame.sky.outdoor {
            return Ok(());
        }
        let frame = cx.frame;
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        let gpu = self.gpu.get_or_insert_with(|| LightingGpu::new(&device));
        let size = cx.seam.surface_size();

        // --- the shadow cascades ---------------------------------------------------------
        let shadows = shadows_wanted(cx.settings) && sun_up(frame) && !self.blocks.is_empty();
        let map = shadow_size(cx.settings);
        if self.shadow.as_ref().is_none_or(|s| s.3 != map) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("high-fidelity sun shadow"),
                size: wgpu::Extent3d {
                    width: map,
                    height: map,
                    depth_or_array_layers: CASCADE_LAYERS,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SHADOW_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let layers = (0..CASCADE_LAYERS)
                .map(|layer| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..wgpu::TextureViewDescriptor::default()
                    })
                })
                .collect();
            let all = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..wgpu::TextureViewDescriptor::default()
            });
            self.shadow = Some((texture, layers, all, map));
        }
        let cas = cascades(frame, map);
        let Some(shadow) = self.shadow.as_ref() else {
            return Ok(());
        };
        if shadows {
            let origins: HashMap<u16, Vec3> =
                frame.terrain.iter().map(|t| (t.block, t.origin)).collect();
            let draws: Vec<(&CasterBlock, Vec3)> = self
                .blocks
                .iter()
                .filter_map(|(id, b)| origins.get(id).map(|o| (b, *o)))
                .collect();
            let needed = (draws.len() * CASCADES) as u64;
            if needed > gpu.caster_capacity {
                gpu.caster_capacity = needed.next_power_of_two();
                gpu.caster_uniform = uniform(
                    &device,
                    "high-fidelity shadow casters",
                    gpu.caster_capacity * CASTER_STRIDE,
                );
                gpu.caster_bind = caster_bind(&device, &gpu.caster_layout, &gpu.caster_uniform);
            }
            let mut bytes = vec![0u8; (needed * CASTER_STRIDE) as usize];
            for c in 0..CASCADES {
                for (k, (_, origin)) in draws.iter().enumerate() {
                    let at = ((c * draws.len() + k) as u64 * CASTER_STRIDE) as usize;
                    let mut v = cas.matrices[c].to_cols_array().to_vec();
                    v.extend_from_slice(&[origin.x, origin.y, origin.z, 0.0]);
                    bytes[at..at + 80].copy_from_slice(&floats(&v));
                }
            }
            queue.write_buffer(&gpu.caster_uniform, 0, &bytes);
            let mut leaves: HashMap<u32, wgpu::BindGroup> = HashMap::new();
            for (block, _) in &draws {
                for (_, _, slot) in &block.cutouts {
                    if leaves.contains_key(slot) {
                        continue;
                    }
                    let Some(t) = cx.seam.texture(dereth_render::device::TextureSlot(*slot)) else {
                        continue;
                    };
                    let view = t.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: 0,
                        array_layer_count: Some(1),
                        ..wgpu::TextureViewDescriptor::default()
                    });
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("high-fidelity leaf shadow"),
                        layout: &gpu.leaf_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&gpu.leaf_sampler),
                            },
                        ],
                    });
                    leaves.insert(*slot, bg);
                }
            }
            for c in 0..CASCADES {
                let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sun shadow cascade"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &shadow.1[c],
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    // The four cascades are timed as one span.
                    timestamp_writes: cx.timer.span_writes("sun shadows", c, CASCADES),
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                cx.seam.count_pass();
                pass.set_pipeline(&gpu.caster_pipeline);
                for (k, (block, _)) in draws.iter().enumerate() {
                    let Some(buffer) = block.buffer.as_ref() else {
                        continue;
                    };
                    #[allow(clippy::cast_possible_truncation)]
                    let offset = ((c * draws.len() + k) as u64 * CASTER_STRIDE) as u32;
                    pass.set_bind_group(0, &gpu.caster_bind, &[offset]);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..block.vertices, 0..1);
                }
                pass.set_pipeline(&gpu.cutout_pipeline);
                for (k, (block, _)) in draws.iter().enumerate() {
                    #[allow(clippy::cast_possible_truncation)]
                    let offset = ((c * draws.len() + k) as u64 * CASTER_STRIDE) as u32;
                    pass.set_bind_group(0, &gpu.caster_bind, &[offset]);
                    for (buffer, n, slot) in &block.cutouts {
                        let Some(tb) = leaves.get(slot) else {
                            continue;
                        };
                        pass.set_bind_group(1, tb, &[]);
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        pass.draw(0..*n, 0..1);
                    }
                }
            }
        }

        // The cascades are shared with the passes that follow (the occlusion's debug view).
        if shadows {
            #[allow(clippy::cast_lossless)]
            let bytes = u64::from(map) * u64::from(map) * 4 * CASCADES as u64;
            cx.resources.insert(
                crate::resources::ResourceName::SunShadow,
                crate::resources::Resource::Texture(shadow.0.clone(), shadow.2.clone()),
                bytes,
            )?;
        } else {
            cx.resources
                .release(crate::resources::ResourceName::SunShadow);
        }

        // --- the relight -------------------------------------------------------------------
        if self.src.as_ref().is_none_or(|h| h.size != size) {
            let t = hdr_texture(&device, "high-fidelity lit copy", size, 1);
            let view = t.create_view(&wgpu::TextureViewDescriptor::default());
            self.src = Some(Held {
                _texture: t,
                view,
                size,
            });
        }
        let Some(src) = self.src.as_ref() else {
            return Ok(());
        };
        #[allow(clippy::cast_precision_loss)]
        let viewport = cx
            .seam
            .world_viewport()
            .map_or([0.0, 0.0, size.0 as f32, size.1 as f32], |v| {
                [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
            });
        let lamps_on =
            crate::passes::lamps::LampLight::wanted(cx.settings.lamps, cx.caps.ray_query);
        let built = lamps_on && self.rt.build(&device, &queue, cx.encoder, cx.seam, frame);
        // The lamps' light, traced through the same structures.
        let lamp_picture = if lamps_on && built {
            match self.rt.bindings() {
                Some(scene) => self
                    .lamps
                    .encode(
                        &device, &queue, cx.encoder, &scene, frame, HDR, cx.depth, &rt.normal,
                        size, viewport, cx.timer,
                    )
                    .map(|p| p.view),
                None => None,
            }
        } else {
            None
        };
        let no_lamps = &self
            .no_lamps
            .get_or_insert_with(|| {
                let t = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("high-fidelity no lamps"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: HDR,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let v = t.create_view(&wgpu::TextureViewDescriptor::default());
                (t, v)
            })
            .1;
        let lamp_view = lamp_picture.unwrap_or(no_lamps);
        let cam = &frame.camera;
        let (sun, amb) = legacy_light(frame);
        let sun_gain = env_f32("DERETH_HIFI_SUN", 2.6);
        let amb_gain = env_f32("DERETH_HIFI_AMBIENT", 0.85);
        let mut p: Vec<f32> = Vec::with_capacity(256);
        p.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
        p.extend((cam.view * HEIGHT_UP).inverse().to_cols_array());
        p.extend((cam.projection * cam.view * HEIGHT_UP).to_cols_array());
        for m in &cas.matrices {
            p.extend(m.to_cols_array());
        }
        p.extend(SPLITS);
        p.extend(cas.texel);
        p.extend(cas.depth_range);
        p.extend(viewport);
        // The point lights nearest the eye.
        let mut lights: Vec<&crate::snapshot::HifiLight> = frame
            .lights
            .iter()
            .filter(|l| l.falloff > 0.0 && (l.position - cam.eye).length() - l.falloff < 250.0)
            .collect();
        lights.sort_by(|a, b| {
            (a.position - cam.eye)
                .length()
                .total_cmp(&(b.position - cam.eye).length())
        });
        lights.truncate(MAX_LIGHTS);
        #[allow(clippy::cast_precision_loss)]
        p.extend([cam.eye.x, cam.eye.y, cam.eye.z, lights.len() as f32]);
        let sun_on = sun_up(frame);
        let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
        p.extend([l.x, l.y, l.z, f32::from(u8::from(shadows))]);
        // After dark the sky's directional light (the moon's) is kept as the ordinary frame had
        // it, unshadowed: without it the night ground holds only the dim ambient, far below
        // what the vertex light gave it.
        let sun_w = if sun_on {
            sun_gain
        } else if frame.sky.outdoor {
            env_f32("DERETH_HIFI_MOON", 1.0)
        } else {
            0.0
        };
        p.extend([sun[0], sun[1], sun[2], sun_w]);
        p.extend([amb[0], amb[1], amb[2], amb_gain]);
        // Under the physical sky the world is replayed without its authored fog and the sky's
        // air puts the distance back over the whole picture, so the new light is not faded out
        // by a fog that is not drawn.
        let unfogged = cx.settings.sky.is_on() && frame.sky.outdoor;
        match frame.sky.fog.filter(|_| !unfogged) {
            Some(fog) => {
                p.extend([fog.min, fog.max, 1.0, 0.0]);
                p.extend(fog.color.map(|c| srgb_to_linear(f32::from(c) / 255.0)));
                p.push(1.0);
            }
            None => {
                p.extend([0.0, 1.0, 0.0, 0.0]);
                p.extend([0.0; 4]);
            }
        }
        let view_mode = f32::from(u8::from(cx.settings.debug == DebugView::Shadows));
        #[allow(clippy::cast_precision_loss)]
        p.extend([
            env_f32("DERETH_HIFI_POINT", 0.35)
                * if sun_on {
                    1.0 - (frame.sky.sun_direction.z * frame.sky.sun_brightness * 4.0)
                        .clamp(0.0, 1.0)
                } else {
                    1.0
                },
            if sun_on {
                env_f32("DERETH_HIFI_CONTACT", 2.5)
            } else {
                0.0
            },
            map as f32,
            view_mode,
        ]);
        let fwd = {
            let inv = render_from_clip(cam.view, cam.projection);
            let c = inv * Vec4::new(0.0, 0.0, 0.5, 1.0);
            (c.xyz() / c.w - cam.eye).normalize_or(Vec3::Y)
        };
        p.extend([fwd.x, fwd.y, fwd.z, cam.far]);
        // The light through leaves is always on with the light. Without the bounced light (which
        // carries the sun's alone), the sky's own fill stands in for it where the sun or the moon
        // does not reach.
        p.extend([
            1.0,
            env_f32("DERETH_HIFI_SSS", 1.0),
            if cx.settings.global_illumination.is_on() && sun_up(frame) {
                0.0
            } else {
                env_f32("DERETH_HIFI_SHADE_FILL", 0.18)
            },
            env_f32("DERETH_HIFI_CONTACT_THICKNESS", 0.2),
        ]);
        p.extend([
            f32::from(u8::from(lamp_picture.is_some())),
            env_f32("DERETH_HIFI_LAMP_GLOW", 2.5),
            if std::env::var_os("DERETH_HIFI_LAMPS_SHOW").is_some() {
                1.0
            } else {
                0.0
            },
            0.0,
        ]);
        queue.write_buffer(&gpu.params, 0, &floats(&p));
        let mut lb: Vec<f32> = Vec::with_capacity(MAX_LIGHTS * 8);
        for l in &lights {
            let i = l.intensity.max(0.0);
            lb.extend([l.position.x, l.position.y, l.position.z, l.falloff]);
            lb.extend([
                l.color[0] * i,
                l.color[1] * i,
                l.color[2] * i,
                f32::from(u8::from(l.interior)),
            ]);
        }
        if !lb.is_empty() {
            queue.write_buffer(&gpu.lights, 0, &floats(&lb));
        }

        let bind = |picture: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity relight"),
                layout: &gpu.relight_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(picture),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&rt.normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(cx.depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&shadow.2),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&gpu.cmp),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: gpu.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: gpu.lights.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(lamp_view),
                    },
                ],
            })
        };
        let copy_bind = bind(&rt.hdr);
        let relight_bind = bind(&src.view);
        {
            let mut pass = colour_pass(cx.encoder, &src.view, "lit copy", None);
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.copy);
            pass.set_bind_group(0, &copy_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        let surface = cx.resources.texture(
            &device,
            crate::resources::ResourceName::GiSurface,
            crate::resources::TextureSpec {
                width: size.0,
                height: size.1,
                format: HDR,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        )?;
        {
            let writes = cx.timer.writes("relight");
            let attach = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("relight"),
                color_attachments: &[attach(&rt.hdr), attach(&surface)],
                depth_stencil_attachment: None,
                timestamp_writes: writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.relight);
            pass.set_bind_group(0, &relight_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        if lamp_picture.is_some()
            && self.lamps.draw_halos(
                &device, &queue, cx.encoder, frame, cx.depth, &rt.hdr, viewport, cx.timer,
            )
        {
            cx.seam.count_pass();
        }
        Ok(())
    }
}

fn colour_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &str,
    timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: timestamps,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

#[derive(Debug)]
struct PostGpu {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    tonemap: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    params: Vec<wgpu::Buffer>,
}

/// Bloom, exposure and the tone map.
#[derive(Debug, Default)]
pub struct LightingPost {
    gpu: Option<PostGpu>,
    bloom: Option<(Held, Vec<wgpu::TextureView>, u32)>,
}

impl PostGpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let d2 = wgpu::TextureViewDimension::D2;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity bloom and tone map"),
            entries: &[
                tex_entry(0, float, d2),
                tex_entry(1, float, d2),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                buf_entry(3, wgpu::ShaderStages::FRAGMENT, false),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity bloom and tone map"),
            source: wgpu::ShaderSource::Wgsl(format!("{POST_WGSL}\n{COLOUR_WGSL}").into()),
        });
        let hdr = |blend| wgpu::ColorTargetState {
            format: HDR,
            blend,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        Self {
            format,
            down: fullscreen(device, &layout, &module, "fs_down", hdr(None)),
            up: fullscreen(device, &layout, &module, "fs_up", hdr(Some(add))),
            tonemap: fullscreen(
                device,
                &layout,
                &module,
                "fs_tonemap",
                wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                },
            ),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("high-fidelity bloom"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..wgpu::SamplerDescriptor::default()
            }),
            params: (0..(BLOOM_LEVELS * 2 + 1))
                .map(|_| uniform(device, "high-fidelity bloom", 64))
                .collect(),
            layout,
        }
    }
}

impl HifiPass for LightingPost {
    fn name(&self) -> &'static str {
        "lighting post"
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        s.lighting.is_on()
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let Some(rt) = cx.reshade else {
            return Ok(());
        };
        if *cx.resolved || !cx.frame.sky.outdoor {
            return Ok(());
        }
        if cx.settings.debug == DebugView::Shadows {
            return Ok(());
        }
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        let format = cx.seam.surface_format();
        if self.gpu.as_ref().is_none_or(|g| g.format != format) {
            self.gpu = Some(PostGpu::new(&device, format));
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return Ok(());
        };
        let size = cx.seam.surface_size();
        let half = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        if self.bloom.as_ref().is_none_or(|b| b.0.size != half) {
            let levels = BLOOM_LEVELS.min(32 - half.0.min(half.1).max(1).leading_zeros());
            let t = hdr_texture(&device, "high-fidelity bloom", half, levels);
            let mips = (0..levels)
                .map(|m| {
                    t.create_view(&wgpu::TextureViewDescriptor {
                        base_mip_level: m,
                        mip_level_count: Some(1),
                        ..wgpu::TextureViewDescriptor::default()
                    })
                })
                .collect();
            let view = t.create_view(&wgpu::TextureViewDescriptor::default());
            self.bloom = Some((
                Held {
                    _texture: t,
                    view,
                    size: half,
                },
                mips,
                levels,
            ));
        }
        let Some((_, mips, levels)) = self.bloom.as_ref() else {
            return Ok(());
        };
        let levels = *levels;
        let frame = cx.frame;
        let sun_gain = env_f32("DERETH_HIFI_SUN", 2.6);
        let amb_gain = env_f32("DERETH_HIFI_AMBIENT", 0.85);
        let exposure = exposure(frame, sun_gain, amb_gain);
        let agx = f32::from(u8::from(
            std::env::var("DERETH_HIFI_TONEMAP").is_ok_and(|v| v.eq_ignore_ascii_case("agx")),
        ));
        #[allow(clippy::cast_precision_loss)]
        let viewport = cx
            .seam
            .world_viewport()
            .map_or([0.0, 0.0, size.0 as f32, size.1 as f32], |v| {
                [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
            });
        #[allow(clippy::cast_precision_loss)]
        let level_size = |m: u32| ((half.0 >> m).max(1) as f32, (half.1 >> m).max(1) as f32);
        let write = |i: usize, texel: (f32, f32), first: bool| {
            let v = [
                exposure,
                env_f32("DERETH_HIFI_BLOOM", 0.06),
                agx,
                levels as f32,
                1.0 / texel.0,
                1.0 / texel.1,
                env_f32("DERETH_HIFI_TOE", 0.36),
                0.0,
                env_f32("DERETH_HIFI_SATURATION", 1.08),
                env_f32("DERETH_HIFI_CONTRAST", 1.04),
                env_f32("DERETH_HIFI_VIGNETTE", 0.28),
                f32::from(u8::from(first)),
                viewport[0],
                viewport[1],
                viewport[2],
                viewport[3],
            ];
            queue.write_buffer(&gpu.params[i], 0, &floats(&v));
        };
        let bind = |src: &wgpu::TextureView, bloom: &wgpu::TextureView, i: usize| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity bloom"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(bloom),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&gpu.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: gpu.params[i].as_entire_binding(),
                    },
                ],
            })
        };
        #[allow(clippy::cast_precision_loss)]
        let full = (size.0 as f32, size.1 as f32);
        // Down: the picture into level 0, then each level into the next. The chain, down and
        // up, is timed as one span.
        let chain = (2 * levels as usize).saturating_sub(1);
        let mut slot = 0usize;
        for m in 0..levels {
            let (src, texel) = if m == 0 {
                (&rt.hdr, full)
            } else {
                (&mips[(m - 1) as usize], level_size(m - 1))
            };
            write(slot, texel, m == 0);
            let b = bind(src, &rt.normal, slot);
            let writes = cx.timer.span_writes("bloom", slot, chain);
            let mut pass = colour_pass_clear(cx.encoder, &mips[m as usize], "bloom down", writes);
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.down);
            pass.set_bind_group(0, &b, &[]);
            pass.draw(0..3, 0..1);
            slot += 1;
        }
        // Up: each level added onto the one above.
        for m in (1..levels).rev() {
            write(slot, level_size(m), false);
            let b = bind(&mips[m as usize], &rt.normal, slot);
            let writes = cx.timer.span_writes("bloom", slot, chain);
            let mut pass = colour_pass(cx.encoder, &mips[(m - 1) as usize], "bloom up", writes);
            cx.seam.count_pass();
            pass.set_pipeline(&gpu.up);
            pass.set_bind_group(0, &b, &[]);
            pass.draw(0..3, 0..1);
            slot += 1;
        }
        write(slot, full, false);
        let b = bind(&rt.hdr, &mips[0], slot);
        let writes = cx.timer.writes("tone map");
        let mut pass = colour_pass(cx.encoder, cx.colour, "tone map", writes);
        cx.seam.count_pass();
        pass.set_viewport(viewport[0], viewport[1], viewport[2], viewport[3], 0.0, 1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        pass.set_scissor_rect(
            viewport[0] as u32,
            viewport[1] as u32,
            viewport[2] as u32,
            viewport[3] as u32,
        );
        pass.set_pipeline(&gpu.tonemap);
        pass.set_bind_group(0, &b, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        *cx.resolved = true;
        Ok(())
    }
}

fn colour_pass_clear<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    label: &str,
    timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: timestamps,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// The lighting shaders as the device is handed them, for validation without a device.
#[must_use]
pub fn shaders() -> [(&'static str, String); 4] {
    [
        ("gi", crate::passes::gi::GI_WGSL.to_owned()),
        ("shadow", SHADOW_WGSL.to_owned()),
        (
            "relight",
            format!("{RELIGHT_WGSL}\n{COLOUR_WGSL}\n{COPY_WGSL}"),
        ),
        ("post", format!("{POST_WGSL}\n{COLOUR_WGSL}")),
    ]
}
