//! The water: where the drawn ground lays retail's water, the water drawn as water over the lit
//! world, near and far.
//!
//! The drawn ground lowers a bed under the water ([`field`]: the farther inside the water, the
//! deeper, to the `water_depth` knob), and the ground lays it with the bed's material.
//! Once the light per pixel has lit the world, this pass draws the water over it: for
//! each pixel, where the line of sight passes under the water's surface (at retail's level, level
//! up to the water's edge), the bed and anything standing in the water are seen through the
//! water, bent by its ripples and faded into its own color with the depth passed through; the sky
//! and the land are reflected by the Fresnel share; and the sun glints off the ripples where it
//! reaches the water. The water's own color is retail's water picture's, under the light the bed
//! takes, so it keeps retail's hue by day and at dusk.
//!
//! Toward its edge the water thins to nothing over the `shore_soft` knob, and foam gathers along
//! it. The ripples ([`ripples`]) drift with the game's clock alone: a held clock draws a still
//! frame. They calm with distance, so far water shows no repeat and no pixel to flicker; what they
//! no longer draw is kept as roughness (the `water_far_rough` knob), so far water blurs what it
//! mirrors rather than lying glassy. Over them run waves ([`waves`]) as large as the water is: the
//! sea's swell and surf, a lake's chop by how far it reaches, hardly any on a river. Through shallow
//! water the sun's light ripples over the bed. Near the eye the water is simulated ([`sim`]): a
//! body moving through it stirs it, and its waves spread, cross, meet the shore and die away. The
//! simulation steps in a compute shader, so a device without compute shaders (WebGL 2) draws the
//! water without it, with small rings round each body standing in it instead.
//!
//! Only pixels change: collision, physics, what can be picked and everything the game reads keep
//! retail's surface. Without the drawn ground, which an art pack's ground or the Reflective Water
//! box asks for, or with Retail Look, the pass does not run and retail's water is drawn as it
//! always is.

pub mod field;
pub mod ripples;
pub mod sim;
pub mod waves;

use glam::{Mat4, Vec3};

use crate::build::Build;
use crate::build::Prebuilt;
use crate::graph::WarmCx;
use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::passes::lighting::{legacy_light, CASCADES};
use crate::resources::{Resource, ResourceName};
use crate::shared::camera::{render_from_clip, HEIGHT_UP};
use crate::shared::fullscreen::{COLOUR_WGSL, FULLSCREEN_WGSL};
use crate::snapshot::{HifiGround, HifiWater};
use crate::{Caps, HifiError, HifiFrame, HifiSettings};

/// Where the water lies: the arithmetic the drawn ground's bed, the art pack's wet bank and the
/// water share.
pub const SHORE_WGSL: &str = include_str!("shore.wgsl");
const WATER_WGSL: &str = include_str!("water.wgsl");

/// The pass's shader, for validation without a device: the water's, with the shore's arithmetic
/// and the physical sky's, bound after the water's own.
#[must_use]
pub fn shader_source() -> String {
    let atmosphere = crate::passes::atmosphere::ATMOSPHERE_WGSL.replace(
        "@group(0) @binding(0) var<uniform> atmosphere",
        "@group(0) @binding(11) var<uniform> atmosphere",
    );
    [
        FULLSCREEN_WGSL,
        COLOUR_WGSL,
        &atmosphere,
        SHORE_WGSL,
        WATER_WGSL,
    ]
    .join("\n")
}

/// The floats of the shader's constants.
pub const CONSTANT_FLOATS: usize = 16 * 3
    + 16 * CASCADES
    + 4 * 8
    + 4 * 5
    + 4 * 2
    + 4 * 4
    + 4 * 3
    + 4
    + 4 * WADERS
    + 4 * waves::COUNT
    + 4 * 3
    + 4 * 2;
/// How far from the eye a body stands for it to stir the simulated water, meters.
const STIRS_WITHIN: f32 = 30.0;
/// The most bodies the water rings round as they stand in it, where it is not simulated.
pub const WADERS: usize = 8;
/// How far from the eye a body stands for the water to ring round it, meters.
const WADERS_WITHIN: f32 = 60.0;
/// Where the physical sky's share of the sky drawn sits in the atmosphere's constants, floats.
const SKY_SHARE: usize = 90;

/// Retail's water picture's mean color for terrain types 16 to 20, linear, from the retail
/// grounds' pictures; a plain blue where a picture is missing.
#[must_use]
pub fn retail_water(ground: &HifiGround) -> [Vec3; 5] {
    let mut out = [Vec3::new(0.05, 0.16, 0.28); 5];
    for (k, o) in out.iter_mut().enumerate() {
        let Some((image, _)) = ground.broad.get(16 + k) else {
            continue;
        };
        let Some(level) = image.levels.first().filter(|_| image.format.is_none()) else {
            continue;
        };
        let texels = level.len() / 4;
        if texels == 0 {
            continue;
        }
        let mut sum = Vec3::ZERO;
        for px in level.as_chunks::<4>().0 {
            sum += Vec3::new(
                srgb_to_linear(f32::from(px[0]) / 255.0),
                srgb_to_linear(f32::from(px[1]) / 255.0),
                srgb_to_linear(f32::from(px[2]) / 255.0),
            );
        }
        // LINT-OK: a few thousand texels.
        #[allow(clippy::cast_precision_loss)]
        let mean = sum / texels as f32;
        *o = mean;
    }
    out
}

fn srgb_to_linear(c: f32) -> f32 {
    crate::passes::lighting::srgb_to_linear(c)
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * dereth_primitives::num::math::powf(c, 1.0 / 2.4) - 0.055
    }
}

/// `albedo` (linear) as the ordinary picture draws level ground of it under the frame's light:
/// its color in the display's encoding times the vertex light of a surface facing straight up,
/// decoded.
#[must_use]
pub fn as_drawn(frame: &HifiFrame, albedo: Vec3) -> Vec3 {
    let (sun, amb) = legacy_light(frame);
    let up = frame.sky.sun_direction.normalize_or(Vec3::Z).z.max(0.0);
    let light = |k: usize| (amb[k] + sun[k] * up).min(1.0);
    Vec3::new(
        srgb_to_linear(linear_to_srgb(albedo.x.max(0.0)) * light(0)),
        srgb_to_linear(linear_to_srgb(albedo.y.max(0.0)) * light(1)),
        srgb_to_linear(linear_to_srgb(albedo.z.max(0.0)) * light(2)),
    )
}

/// The ripple layers' constants for `water` at `clock` seconds: each layer's repeat in meters,
/// how much it bends the surface, its turn and how far it has drifted.
#[must_use]
pub fn layer_constants(water: &HifiWater, clock: f64) -> [[f32; 4]; 4] {
    ripples::LAYERS.map(|l| {
        [
            l.repeat * water.ripple_scale.max(1e-3),
            l.strength,
            l.turn,
            ripples::drift(&l, clock, water.ripple_scale, water.ripple_speed),
        ]
    })
}

/// The water pass.
#[derive(Debug, Default)]
pub struct Water {
    gpu: Option<Gpu>,
    /// The same, built ahead by the warm-up.
    pre_gpu: Prebuilt<Gpu>,
    /// Whether the water was drawn in the last frame.
    drawn: bool,
    /// Retail's water pictures' mean colors, and the ground they were worked out for: they are
    /// worked out again only for another ground.
    hues: Option<(u64, [Vec3; 5])>,
    /// How many times they have been worked out.
    averaged: u64,
    /// The simulated water near the eye.
    sim: sim::Sim,
    /// Which of the simulation's two pictures holds the water as it stands.
    sim_current: usize,
}

/// The pass.
#[must_use]
pub fn pass() -> Water {
    Water::default()
}

#[derive(Debug)]
struct Gpu {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    constants: wgpu::Buffer,
    atmosphere: wgpu::Buffer,
    ripples: wgpu::TextureView,
    wrap: wgpu::Sampler,
    clamp: wgpu::Sampler,
    shadow_cmp: wgpu::Sampler,
    /// A sky table standing in for the sky's when the physical sky is not drawn.
    no_sky: wgpu::TextureView,
    /// Cascades standing in for the sun's when none were drawn.
    no_shadow: wgpu::TextureView,
    /// The simulated water, on a device that runs compute shaders.
    sim: Option<SimGpu>,
    /// Calm water standing in for the simulated water's picture where it is not simulated.
    no_sim: wgpu::TextureView,
}

/// The simulated water on the device.
#[derive(Debug)]
struct SimGpu {
    /// Its two pictures, each step reading one and writing the other.
    pictures: [wgpu::TextureView; 2],
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    /// Each step's constants, a step to a stride.
    steps: wgpu::Buffer,
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    }
}

fn texture(
    sample_type: wgpu::TextureSampleType,
    dim: wgpu::TextureViewDimension,
) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type,
        view_dimension: dim,
        multisampled: false,
    }
}

impl Gpu {
    #[allow(clippy::too_many_lines)]
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, compute: bool) -> Self {
        use wgpu::TextureSampleType as S;
        use wgpu::TextureViewDimension as D;
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let unfiltered = S::Float { filterable: false };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water"),
            entries: &[
                entry(0, uniform),
                entry(1, texture(S::Depth, D::D2)),
                entry(2, texture(S::Float { filterable: true }, D::D2)),
                entry(3, texture(unfiltered, D::D2)),
                entry(4, texture(unfiltered, D::D2)),
                entry(5, texture(unfiltered, D::D2)),
                entry(6, texture(S::Uint, D::D2)),
                entry(7, texture(S::Float { filterable: true }, D::D2)),
                entry(
                    8,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    9,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(10, texture(S::Float { filterable: true }, D::D2)),
                entry(11, uniform),
                entry(12, texture(S::Depth, D::D2Array)),
                entry(
                    13,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                ),
                entry(14, texture(unfiltered, D::D2)),
                entry(15, texture(S::Float { filterable: true }, D::D2)),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_fullscreen"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_water"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: crate::derive::pipeline::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let buffer = |label: &str, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        // The ripples' picture with its levels.
        let levels = ripples::picture();
        let ripple_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("water ripples"),
            size: wgpu::Extent3d {
                width: ripples::SIDE,
                height: ripples::SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: u32::try_from(levels.len()).unwrap_or(1),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, bytes) in (0u32..).zip(&levels) {
            let side = (ripples::SIDE >> level).max(1);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &ripple_texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: Some(side),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
            );
        }
        let sampler = |address: wgpu::AddressMode, anisotropy: u16| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("water"),
                address_mode_u: address,
                address_mode_v: address,
                address_mode_w: address,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: anisotropy,
                ..wgpu::SamplerDescriptor::default()
            })
        };
        let no_sky = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("water, no sky"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let no_shadow = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("water, no shadow"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: u32::try_from(CASCADES).unwrap_or(4),
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..wgpu::TextureViewDescriptor::default()
            });
        let no_sim = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("water, not simulated"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            layout,
            pipeline,
            constants: buffer("water", (CONSTANT_FLOATS * 4) as u64),
            atmosphere: buffer(
                "water atmosphere",
                crate::passes::atmosphere::model::constants(
                    &HifiFrame::default(),
                    crate::Level::High,
                )
                .len() as u64,
            ),
            ripples: ripple_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            wrap: sampler(wgpu::AddressMode::Repeat, 16),
            clamp: sampler(wgpu::AddressMode::ClampToEdge, 1),
            shadow_cmp: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("water shadow"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                compare: Some(wgpu::CompareFunction::LessEqual),
                ..wgpu::SamplerDescriptor::default()
            }),
            no_sky,
            no_shadow,
            sim: compute.then(|| SimGpu::new(device)),
            no_sim,
        }
    }
}

/// The simulation's shader, for validation without a device: its own, with the shore's
/// arithmetic and the water's vertical derivative worked out once.
#[must_use]
pub fn sim_shader_source() -> String {
    let kernel = sim::kernel()
        .iter()
        .map(|v| format!("{v:e}"))
        .collect::<Vec<_>>()
        .join(", ");
    let n = sim::REACH + 1;
    let constant = format!(
        "const KERNEL: array<f32, {}> = array<f32, {}>({kernel});",
        n * n,
        n * n
    );
    [SHORE_WGSL, sim::SIM_WGSL, &constant].join("\n")
}

impl SimGpu {
    fn new(device: &wgpu::Device) -> Self {
        let picture = |label: &str| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: sim::SIDE,
                        height: sim::SIDE,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let compute = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let unfiltered = wgpu::TextureSampleType::Float { filterable: false };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water simulation"),
            entries: &[
                compute(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(sim::STEP_BYTES as u64),
                    },
                ),
                compute(1, texture(unfiltered, wgpu::TextureViewDimension::D2)),
                compute(
                    2,
                    wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                ),
                compute(3, texture(unfiltered, wgpu::TextureViewDimension::D2)),
                compute(4, texture(unfiltered, wgpu::TextureViewDimension::D2)),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water simulation"),
            source: wgpu::ShaderSource::Wgsl(sim_shader_source().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water simulation"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.build_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("water simulation"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("cs_step"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        Self {
            pictures: [picture("water simulation a"), picture("water simulation b")],
            layout,
            pipeline,
            steps: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("water simulation steps"),
                size: (sim::STEP_STRIDE * sim::MOST_STEPS) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
}

/// The simulated square's corners where the water is not simulated: none, so none of it is drawn.
pub const NO_SIM_AREA: [f32; 4] = [0.0; 4];

/// What the constants are made from, apart from the frame.
#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    /// The world viewport: x, y, width, height, pixels.
    pub viewport: [f32; 4],
    /// How much of the sky is the physical sky.
    pub physical_sky: f32,
    /// Whether the world is drawn with its authored fog.
    pub fogged: bool,
    /// The cascades' side in texels, when they hold this frame's shadow.
    pub cascades: Option<u32>,
    /// The simulated square in render space, south-west and north-east corners.
    pub sim_area: [f32; 4],
    /// Where render space's origin lies in the world, wrapped to the simulated square's side.
    pub sim_fold: glam::Vec2,
    /// Whether the water rings round the bodies standing in it, as it does where not simulated.
    pub rings: bool,
}

/// The shader's constants for `frame`, with `water` its look and `hues` retail's water under the
/// frame's ordinary light.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn constants(
    frame: &HifiFrame,
    water: &HifiWater,
    hues: &[Vec3; 5],
    inputs: &Inputs,
) -> Vec<f32> {
    let cam = &frame.camera;
    let clip_from_render = cam.jittered(cam.projection * cam.view * HEIGHT_UP);
    let global = frame.drawn.as_ref().map_or(glam::Vec2::ZERO, |d| d.global);
    let horizon = frame
        .drawn
        .as_ref()
        .and_then(|d| d.knobs.far.map(|f| f.3))
        .unwrap_or(cam.far)
        .min(cam.far * 0.97);
    let mut c = Vec::with_capacity(CONSTANT_FLOATS);
    c.extend(clip_from_render.to_cols_array());
    // Back from the depth through the same jittered projection the world was drawn with, so
    // each pixel finds the very point its depth was drawn for; and the steady lines of sight the
    // water's look follows, without the jitter.
    c.extend(clip_from_render.inverse().to_cols_array());
    c.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
    let cascades = inputs
        .cascades
        .map(|size| crate::passes::lighting::cascades(frame, size));
    for i in 0..CASCADES {
        c.extend(
            cascades
                .as_ref()
                .map_or(Mat4::IDENTITY, |cs| cs.matrices[i])
                .to_cols_array(),
        );
    }
    c.extend(inputs.viewport);
    c.extend([cam.eye.x, cam.eye.y, cam.eye.z, horizon]);
    c.extend([
        global.x,
        global.y,
        inputs.physical_sky,
        f32::from(u8::from(inputs.fogged)),
    ]);
    let sun_up = crate::passes::lighting::sun_up(frame);
    let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
    c.extend([l.x, l.y, l.z, f32::from(u8::from(sun_up))]);
    // The sun's light as the light per pixel takes it, less what cloud and a low sun take.
    let (sun, amb) = legacy_light(frame);
    let gain = if sun_up {
        crate::passes::lighting::env_f32("DERETH_HIFI_SUN", 2.6)
            * crate::passes::lighting::DayLight::of(frame).sun
    } else {
        0.0
    };
    let lin = |v: f32| dereth_primitives::num::math::powf(v.max(0.0), 2.2);
    c.extend([
        lin(sun[0]) * gain,
        lin(sun[1]) * gain,
        lin(sun[2]) * gain,
        0.0,
    ]);
    match frame.sky.fog.filter(|_| inputs.fogged) {
        Some(f) => c.extend([f.min, f.max, 0.0, 0.0]),
        None => c.extend([0.0, 1.0, 0.0, 0.0]),
    }
    // The sky where no physical sky is drawn: the fog's color at the horizon, deeper overhead.
    let low = frame.sky.fog.map_or_else(
        || Vec3::new(lin(amb[0]), lin(amb[1]), lin(amb[2])) * 1.5,
        |f| crate::passes::atmosphere::model::srgb_bytes_to_linear(f.color),
    );
    let high = low * Vec3::new(0.55, 0.7, 1.0) * 0.85;
    c.extend(low.extend(0.0).to_array());
    c.extend(high.extend(0.0).to_array());
    for h in hues {
        c.extend(h.extend(0.0).to_array());
    }
    c.extend([water.color, water.clarity, water.depth, water.shelf]);
    c.extend([
        water.ripple_strength,
        water.calm,
        water.reflection,
        water.shore_soft,
    ]);
    for layer in layer_constants(water, frame.time) {
        c.extend(layer);
    }
    c.extend(crate::passes::lighting::SPLITS);
    c.extend(cascades.as_ref().map_or([1.0; 4], |cs| cs.texel));
    // LINT-OK: a shadow map's side, a small number.
    #[allow(clippy::cast_precision_loss)]
    c.extend([
        f32::from(u8::from(cascades.is_some())),
        inputs.cascades.unwrap_or(1) as f32,
        crate::passes::lighting::env_f32("DERETH_HIFI_WATER_VIEW", 0.0),
        0.0,
    ]);
    // The water's own clock, seconds, kept short so the shader's arithmetic stays exact; and,
    // where it is not simulated, how many bodies it rings round and how strongly.
    let mut waders = if inputs.rings {
        near_bodies(frame, WADERS_WITHIN)
    } else {
        Vec::new()
    };
    waders.truncate(WADERS);
    // LINT-OK: a wrapped clock and a count of at most WADERS.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    c.extend([
        (frame.time % 600.0) as f32,
        water.far_rough,
        waders.len() as f32,
        water.rings,
    ]);
    for i in 0..WADERS {
        let b = waders.get(i).copied().unwrap_or(Vec3::ZERO);
        c.extend([b.x, b.y, b.z, 0.0]);
    }
    for w in waves::constants(frame.time, global) {
        c.extend(w);
    }
    let fold = waves::fold(global);
    c.extend([
        water.waves,
        water.foam,
        water.caustics,
        waves::surf(frame.time),
    ]);
    let foam = as_drawn(frame, Vec3::splat(0.8));
    c.extend([foam.x, foam.y, foam.z, 0.0]);
    let beam = if sun_up {
        crate::passes::lighting::DayLight::of(frame).sun
    } else {
        0.0
    };
    c.extend([fold.x, fold.y, beam, 0.0]);
    // LINT-OK: the square's side, a few hundred cells.
    #[allow(clippy::cast_precision_loss)]
    let side = sim::SIDE as f32 * sim::CELL;
    c.extend([1.0 / side, inputs.sim_fold.x, inputs.sim_fold.y, sim::CELL]);
    c.extend(inputs.sim_area);
    c
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// The bodies within `within` meters of the eye, render space, nearest first.
fn near_bodies(frame: &HifiFrame, within: f32) -> Vec<Vec3> {
    let eye = frame.camera.eye.truncate();
    let mut bodies: Vec<Vec3> = frame
        .drawn
        .as_ref()
        .map(|d| {
            d.bodies
                .iter()
                .copied()
                .filter(|b| (b.truncate() - eye).length() < within)
                .collect()
        })
        .unwrap_or_default();
    bodies.sort_by(|a, b| {
        (a.truncate() - eye)
            .length()
            .total_cmp(&(b.truncate() - eye).length())
    });
    bodies
}

/// What the simulated water runs this frame: where its square lies and each step's constants,
/// stirred by the bodies near the eye. `None` where it is not simulated, which leaves it calm.
fn stir(
    state: &mut sim::Sim,
    frame: &HifiFrame,
    water: &HifiWater,
    simulated: bool,
) -> Option<sim::Plan> {
    if !simulated {
        return None;
    }
    let bodies = near_bodies(frame, STIRS_WITHIN);
    Some(state.plan(&sim::Disturbance {
        clock: frame.time,
        global: frame.drawn.as_ref().map_or(glam::Vec2::ZERO, |d| d.global),
        eye: frame.camera.eye,
        bodies: &bodies,
        stir: water.rings,
    }))
}

impl HifiPass for Water {
    fn name(&self) -> &'static str {
        "water"
    }

    fn notes(&self, notes: &mut Vec<(&'static str, u64)>) {
        notes.push(("water drawn", u64::from(self.drawn)));
        notes.push(("retail water colors worked out", self.averaged));
    }

    fn observe(&mut self, _cx: &mut PrepareCx<'_, '_>) {
        self.drawn = false;
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        s.art_pack
            && s.lighting.is_on()
            && f.sky.sees_outside
            && f.ground.is_some()
            && f.drawn.as_ref().is_some_and(|d| d.knobs.water.is_some())
            && (self.gpu.is_some() || !self.pre_gpu.waits())
    }

    fn warm(&mut self, cx: &mut WarmCx<'_>) {
        // Whether the viewer sees outside is left out: the water is wanted as soon as they do.
        let (s, f) = (cx.settings, cx.frame);
        let drawn = s.art_pack
            && s.lighting.is_on()
            && f.ground.is_some()
            && f.drawn.as_ref().is_some_and(|d| d.knobs.water.is_some());
        if drawn && self.gpu.is_none() {
            let (device, queue) = (cx.device.clone(), cx.queue.clone());
            let compute = cx.caps.compute;
            self.pre_gpu
                .ask(cx.warm, move || Gpu::new(&device, &queue, compute));
        }
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        if self.gpu.is_none() {
            let built = self.pre_gpu.take();
            self.gpu =
                Some(built.unwrap_or_else(|| {
                    Gpu::new(cx.seam.device(), cx.seam.queue(), cx.caps.compute)
                }));
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let (Some(gpu), Some(rt)) = (self.gpu.as_ref(), cx.reshade) else {
            return Ok(());
        };
        if *cx.resolved {
            return Ok(());
        }
        let (Some(water), Some(ground)) = (
            cx.frame.drawn.as_ref().and_then(|d| d.knobs.water),
            cx.frame.ground.as_ref(),
        ) else {
            return Ok(());
        };
        let view = |name| match cx.resources.get(name) {
            Some(Resource::Texture(_, v)) => Some(v.clone()),
            _ => None,
        };
        let (Some(heights), Some(field), Some(words), Some(ordinary)) = (
            view(ResourceName::DrawnGroundHeights),
            view(ResourceName::DrawnGroundWater),
            view(ResourceName::DrawnGroundWords),
            view(ResourceName::LitCopy),
        ) else {
            return Ok(());
        };
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        // The physical sky, where it is drawn: its table and the atmosphere's constants.
        let sky_table = view(ResourceName::SkyViewLut)
            .filter(|_| cx.settings.sky.is_on() && cx.frame.sky.outdoor);
        let mut physical_sky = 0.0;
        if sky_table.is_some() {
            let air = crate::passes::atmosphere::model::constants(cx.frame, cx.settings.sky);
            physical_sky = air
                .get(SKY_SHARE * 4..SKY_SHARE * 4 + 4)
                .and_then(|b| b.try_into().ok())
                .map_or(0.0, f32::from_le_bytes)
                .clamp(0.0, 1.0);
            if air.len() as u64 <= gpu.atmosphere.size() {
                queue.write_buffer(&gpu.atmosphere, 0, &air);
            }
        }
        // The sun's shadow cascades, when the light per pixel drew them this frame.
        let cascades = match cx.resources.touch(ResourceName::SunShadow) {
            Some(Resource::Texture(t, v)) if crate::passes::lighting::sun_up(cx.frame) => {
                Some((t.width(), v.clone()))
            }
            _ => None,
        };
        #[allow(clippy::cast_precision_loss)] // pixel extents
        let viewport = cx.seam.world_viewport().map_or_else(
            || {
                let (w, h) = cx.seam.surface_size();
                [0.0, 0.0, w as f32, h as f32]
            },
            |v| [v.x as f32, v.y as f32, v.width as f32, v.height as f32],
        );
        let hues = match self.hues {
            Some((serial, hues)) if serial == ground.serial => hues,
            _ => {
                let hues = retail_water(ground);
                self.hues = Some((ground.serial, hues));
                self.averaged += 1;
                hues
            }
        }
        .map(|h| as_drawn(cx.frame, h));
        // The simulated water, stepped as far as the clock has run: each step reading one of its
        // pictures and writing the other.
        let plan = stir(&mut self.sim, cx.frame, &water, gpu.sim.is_some());
        if let (Some(plan), Some(sim)) = (&plan, &gpu.sim) {
            for (k, step) in plan.steps.iter().enumerate() {
                queue.write_buffer(&sim.steps, (k * sim::STEP_STRIDE) as u64, step);
            }
            for k in 0..plan.steps.len() {
                let (from, to) = (self.sim_current, 1 - self.sim_current);
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("water simulation"),
                    layout: &sim.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &sim.steps,
                                offset: 0,
                                size: wgpu::BufferSize::new(sim::STEP_BYTES as u64),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&sim.pictures[from]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&sim.pictures[to]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(&heights),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&field),
                        },
                    ],
                });
                let mut pass = cx.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("water simulation"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&sim.pipeline);
                // LINT-OK: a step's offset, a few hundred bytes in.
                #[allow(clippy::cast_possible_truncation)]
                pass.set_bind_group(0, &bind, &[(k * sim::STEP_STRIDE) as u32]);
                pass.dispatch_workgroups(sim::SIDE / 16, sim::SIDE / 16, 1);
                drop(pass);
                self.sim_current = to;
            }
        }
        let values = constants(
            cx.frame,
            &water,
            &hues,
            &Inputs {
                viewport,
                physical_sky,
                fogged: !(cx.settings.sky.is_on() && cx.frame.sky.outdoor),
                cascades: cascades.as_ref().map(|(size, _)| *size),
                sim_area: plan.as_ref().map_or(NO_SIM_AREA, |p| p.area),
                sim_fold: plan.as_ref().map_or(glam::Vec2::ZERO, |p| p.fold),
                rings: plan.is_none(),
            },
        );
        queue.write_buffer(&gpu.constants, 0, &floats(&values));
        // The lit picture as it stands, read while the water is drawn into it.
        let picture = crate::reshade::hdr_copy(cx.seam, cx.resources, cx.encoder)?;
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water"),
            layout: &gpu.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: gpu.constants.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(cx.depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&picture),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&ordinary),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&heights),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&field),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&words),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&gpu.ripples),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&gpu.wrap),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::Sampler(&gpu.clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(
                        sky_table.as_ref().unwrap_or(&gpu.no_sky),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: gpu.atmosphere.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(
                        cascades.as_ref().map_or(&gpu.no_shadow, |(_, v)| v),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::Sampler(&gpu.shadow_cmp),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&rt.normal),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: wgpu::BindingResource::TextureView(
                        gpu.sim
                            .as_ref()
                            .map_or(&gpu.no_sim, |sim| &sim.pictures[self.sim_current]),
                    ),
                },
            ],
        });
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("water"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &rt.hdr,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: cx.timer.writes("water"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.seam.count_pass();
        if let Some(v) = cx.seam.world_viewport() {
            #[allow(clippy::cast_precision_loss)] // pixel extents
            pass.set_viewport(
                v.x as f32,
                v.y as f32,
                v.width as f32,
                v.height as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(v.x, v.y, v.width, v.height);
        }
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.drawn = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the water, an art-pack presentation's own)
    use super::*;

    #[test]
    fn the_water_shader_parses_and_validates_with_its_entry_points() {
        let source = shader_source();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
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
        assert_eq!(entries, ["vs_fullscreen", "fs_water"]);
        let block = module
            .types
            .iter()
            .find(|(_, t)| t.name.as_deref() == Some("Water"))
            .map(|(_, t)| t.inner.size(module.to_ctx()));
        assert_eq!(block, Some(u32::try_from(CONSTANT_FLOATS * 4).unwrap()));
    }

    #[test]
    fn the_simulations_shader_parses_and_validates_with_its_step() {
        let source = sim_shader_source();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
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
        assert_eq!(entries, ["cs_step"]);
    }

    #[test]
    fn without_compute_the_water_is_never_simulated() {
        let water = HifiWater::default();
        let at = |time: f64| HifiFrame {
            time,
            ..HifiFrame::default()
        };
        let mut calm = pass();
        assert!(stir(&mut calm.sim, &at(10.0), &water, false).is_none());
        assert!(stir(&mut calm.sim, &at(10.5), &water, false).is_none());
        assert_eq!(calm.sim, sim::Sim::default(), "the simulation never ran");
        // Unsimulated, the square covers nothing, so the shader draws none of it.
        let values = constants(
            &at(10.5),
            &water,
            &[Vec3::ONE; 5],
            &Inputs {
                viewport: [0.0, 0.0, 1920.0, 1080.0],
                physical_sky: 1.0,
                fogged: false,
                cascades: None,
                sim_area: NO_SIM_AREA,
                sim_fold: glam::Vec2::ZERO,
                rings: true,
            },
        );
        let [x0, y0, x1, y1] = values[CONSTANT_FLOATS - 4..] else {
            unreachable!()
        };
        assert!(x1 <= x0 && y1 <= y0);
        // With compute it steps as the clock runs.
        let mut stirred = pass();
        assert!(stir(&mut stirred.sim, &at(10.0), &water, true).is_some());
        let plan = stir(&mut stirred.sim, &at(10.5), &water, true).expect("simulated");
        assert_eq!(plan.steps.len(), sim::MOST_STEPS);
    }

    #[test]
    fn unsimulated_the_water_rings_round_the_bodies_standing_in_it() {
        let water = HifiWater {
            rings: 0.7,
            ..HifiWater::default()
        };
        let frame = HifiFrame {
            drawn: Some(crate::snapshot::HifiDrawnGround {
                world: std::sync::Arc::default(),
                tiles: Vec::new(),
                reach: 3,
                global: glam::Vec2::ZERO,
                knobs: crate::snapshot::HifiGroundKnobs::default(),
                bodies: vec![Vec3::new(500.0, 0.0, 0.0), Vec3::new(3.0, 0.0, -0.5)],
                cover: None,
            }),
            ..HifiFrame::default()
        };
        let wake = CONSTANT_FLOATS - 4 * (2 + 3 + waves::COUNT + WADERS + 1);
        let with = |rings| {
            constants(
                &frame,
                &water,
                &[Vec3::ONE; 5],
                &Inputs {
                    viewport: [0.0, 0.0, 1920.0, 1080.0],
                    physical_sky: 1.0,
                    fogged: false,
                    cascades: None,
                    sim_area: NO_SIM_AREA,
                    sim_fold: glam::Vec2::ZERO,
                    rings,
                },
            )
        };
        // The near body only, and as strongly as the knob says.
        let ringed = with(true);
        assert_eq!(ringed[wake + 2..wake + 4], [1.0, 0.7]);
        assert_eq!(ringed[wake + 4..wake + 7], [3.0, 0.0, -0.5]);
        // Simulated, the simulation stirs the water instead.
        assert_eq!(with(false)[wake + 2], 0.0);
    }

    #[test]
    fn the_constants_fill_the_shaders_block_exactly() {
        let frame = HifiFrame::default();
        let values = constants(
            &frame,
            &HifiWater::default(),
            &[Vec3::ONE; 5],
            &Inputs {
                viewport: [0.0, 0.0, 1920.0, 1080.0],
                physical_sky: 1.0,
                fogged: false,
                cascades: Some(2048),
                sim_area: NO_SIM_AREA,
                sim_fold: glam::Vec2::ZERO,
                rings: true,
            },
        );
        assert_eq!(values.len(), CONSTANT_FLOATS);
    }
}
