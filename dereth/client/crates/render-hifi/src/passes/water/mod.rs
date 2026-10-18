//! The water: where the drawn ground lays retail's water, the water drawn as water over the lit
//! world, near and far.
//!
//! The drawn ground lowers a bed under the water ([`field`]: the farther inside the water, the
//! deeper, to the `water_depth` knob), and the art pack's ground lays it with the bed's
//! material. Once the light per pixel has lit the world, this pass draws the water over it: for
//! each pixel, where the line of sight passes under the water's surface (at retail's level, level
//! up to the water's edge), the bed and anything standing in the water are seen through the
//! water, bent by its ripples and faded into its own color with the depth passed through; the sky
//! and the land are reflected by the Fresnel share; and the sun glints off the ripples where it
//! reaches the water. The water's own color is retail's water picture's, under the light the bed
//! takes, so it keeps retail's hue by day and at dusk.
//!
//! Toward its edge the water thins to nothing over the `shore_soft` knob, so the shore is a soft
//! change of depth and color with nothing laid along it. The ripples ([`ripples`]) drift with the
//! game's clock alone: a held clock draws a still frame. They calm with distance, so far water is
//! smooth and mirrors the sky without a repeat to see or a pixel to flicker.
//!
//! Only pixels change: collision, physics, what can be picked and everything the game reads keep
//! retail's surface. Without an art pack's drawn ground, or with Retail Look, the pass does not
//! run and retail's water is drawn as it always is.

pub mod field;
pub mod ripples;

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
pub const CONSTANT_FLOATS: usize =
    16 * 3 + 16 * CASCADES + 4 * 8 + 4 * 5 + 4 * 2 + 4 * 4 + 4 * 3 + 4 + 4 * WADERS;
/// The most bodies the water rings round as they stand in it.
pub const WADERS: usize = 8;
/// How far from the eye a body stands for the water to ring round it, meters: farther, its rings
/// are finer than the pixels.
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
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
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
        }
    }
}

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
}

/// The shader's constants for `frame`, with `water` its look and `hues` retail's water under the
/// frame's ordinary light.
#[must_use]
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
    // The bodies the water rings round, nearest the eye first: where they stand, render space.
    let mut bodies: Vec<Vec3> = frame
        .drawn
        .as_ref()
        .map(|d| {
            d.bodies
                .iter()
                .copied()
                .filter(|b| (b.truncate() - cam.eye.truncate()).length() < WADERS_WITHIN)
                .collect()
        })
        .unwrap_or_default();
    bodies.sort_by(|a, b| {
        (a.truncate() - cam.eye.truncate())
            .length()
            .total_cmp(&(b.truncate() - cam.eye.truncate()).length())
    });
    bodies.truncate(WADERS);
    // The rings' own clock, seconds, kept short so the shader's arithmetic stays exact; and how
    // many bodies, and how strongly the water rings.
    // LINT-OK: a wrapped clock and a body count, both small.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    c.extend([
        (frame.time % 600.0) as f32,
        bodies.len() as f32,
        water.rings,
        0.0,
    ]);
    for i in 0..WADERS {
        let b = bodies.get(i).copied().unwrap_or(Vec3::ZERO);
        c.extend([b.x, b.y, b.z, 0.0]);
    }
    c
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
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
            self.pre_gpu.ask(cx.warm, move || Gpu::new(&device, &queue));
        }
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        if self.gpu.is_none() {
            let built = self.pre_gpu.take();
            self.gpu = Some(built.unwrap_or_else(|| Gpu::new(cx.seam.device(), cx.seam.queue())));
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
        let values = constants(
            cx.frame,
            &water,
            &hues,
            &Inputs {
                viewport,
                physical_sky,
                fogged: !(cx.settings.sky.is_on() && cx.frame.sky.outdoor),
                cascades: cascades.as_ref().map(|(size, _)| *size),
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
            },
        );
        assert_eq!(values.len(), CONSTANT_FLOATS);
    }
}
