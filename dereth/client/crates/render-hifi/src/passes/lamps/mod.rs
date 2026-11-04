//! Outdoor lamps as light sources after dusk.
//!
//! The snapshot carries the lamps the data places on outdoor objects: their authored point lights,
//! their flames and their warm glowing glass. After dusk they light the world around them: the
//! lighting pass asks this module for a picture of the lamps' light on every lit pixel, traced
//! through the resident blocks' structures so a wall or a roof keeps a lamp's light off whatever
//! stands behind it, and adds it to the relight. The lamps themselves glow: their glass is
//! brightened, and a soft halo is drawn around each one the eye can see.
//!
//! The lamps fade in as the sun goes down, flames flicker, and only the lamps that matter most to
//! the view are lit: the brightest near the eye and inside the view.

use glam::{Mat4, Vec3, Vec4};

use crate::build::Build;
use crate::flicker::Flicker;
use crate::shared::camera::{render_from_clip, HEIGHT_UP};
use crate::snapshot::{HifiLamp, LampSource};
use crate::{HifiFrame, Level};

const LAMPS_WGSL: &str = include_str!("lamps.wgsl");
const FILTER_WGSL: &str = include_str!("filter.wgsl");
const HALO_WGSL: &str = include_str!("halo.wgsl");

const OUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Lamps lit per frame, at most.
const MAX_LAMPS: usize = 256;
/// Lamps given a halo per frame, at most.
const MAX_HALOS: usize = 128;
/// Bytes of one lamp record.
const LAMP_BYTES: u64 = 48;
const HALO_BYTES: u64 = 48;

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// How far the lamps are lit, 0 by day to 1 at night, from the sun's height: they come on as the
/// sun sinks below about ten degrees and are full once it is down to a degree.
#[must_use]
pub fn night(frame: &HifiFrame) -> f32 {
    let e = if frame.sky.sun_brightness > 0.0 {
        frame.sky.sun_direction.z
    } else {
        -1.0
    };
    let t = ((0.18 - e) / 0.16).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far past its own reach a lamp lights the world, metres, at the whole Light Draw Distance.
const LAMP_REACH: f32 = 160.0;

/// How far from the eye a lamp's halo shows, metres, at the whole Light Draw Distance.
const HALO_REACH: f32 = 120.0;

/// The share of a reach over which a lamp's light and halo fade out before its end.
const FADE_SHARE: f32 = 0.25;

/// The Light Draw Distance `frame` asks for, as a multiple of the whole distance.
fn distance_share(frame: &HifiFrame) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a share in percent
    match frame.light_distance {
        0 => 1.0,
        share => share as f32 / 100.0,
    }
}

/// How far the lamps light the world past their own reach in `frame`, metres.
fn lamp_reach(frame: &HifiFrame) -> f32 {
    env_f32("DERETH_HIFI_LAMP_REACH", LAMP_REACH) * distance_share(frame)
}

/// 1 short of the last [`FADE_SHARE`] of `reach`, easing to 0 at `reach`, for something `d` away.
fn fade_toward(d: f32, reach: f32) -> f32 {
    let t = ((reach - d) / (reach * FADE_SHARE).max(1e-3)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Whether an art pack's light stands within the lamps' reach of the eye: those shine by day.
#[must_use]
pub fn shine_by_day(frame: &HifiFrame) -> bool {
    let reach = lamp_reach(frame);
    frame
        .lamps
        .iter()
        .any(|l| l.pack.is_some() && (l.position - frame.camera.eye).length() - l.falloff < reach)
}

/// One lamp as it is lit this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LitLamp {
    /// Where, render space.
    pub position: Vec3,
    /// Whether its light moves from frame to frame.
    pub flickers: bool,
    /// Its colour times its brightness, night and flicker folded in.
    pub color: [f32; 3],
    /// Its reach, metres.
    pub range: f32,
    /// The radius of the light itself, for soft shadow edges.
    pub radius: f32,
    /// How far short of the lamp a ray stops, so its own housing does not shade it.
    pub skip: f32,
    /// The radius of its glowing part.
    pub glow: f32,
    /// The size of its halo, metres.
    pub halo: f32,
    /// Where it was found: 0 authored, 1 flame, 2 glow.
    pub source: f32,
    /// How brightly the air about it glows, in place of a halo; 0 for a halo.
    pub air: f32,
    /// How much of its glow it keeps against the daylight, 0 to 1.
    pub presence: f32,
}

/// How much brighter a light's glow is drawn at noon than at night for all its presence: about
/// how far the eye's exposure falls from night to day, so the glow holds its own.
const DAY_GLOW: f32 = 10.0;

/// The fire colour a flame's own sprite colour is pulled toward, so a deep red sprite still lights
/// the ground the colour of firelight.
const FIRE: [f32; 3] = [1.0, 0.52, 0.18];

/// The light of a bed of embers.
const EMBER: [f32; 3] = [1.0, 0.2, 0.04];

/// The warm light most lamps throw on the ground: authored lamp colours are saturated, and are
/// pulled halfway toward it.
const LAMPLIGHT: [f32; 3] = [1.0, 0.56, 0.26];

fn toward(c: [f32; 3], to: [f32; 3], k: f32) -> [f32; 3] {
    let m = [0, 1, 2].map(|i| c[i] * (1.0 - k) + to[i] * k);
    let top = m[0].max(m[1]).max(m[2]).max(1e-4);
    m.map(|v| v / top)
}

/// How a lamp stands in `frame`: as a flame or a bed of embers moves, or steady.
fn flicker(frame: &HifiFrame, lamp: &HifiLamp) -> Flicker {
    if !frame.flame_flicker || !lamp.flicker {
        return Flicker::STEADY;
    }
    if lamp.source == LampSource::Ember {
        return crate::flicker::embers(lamp.seed, frame.time);
    }
    crate::flicker::flame(
        lamp.seed,
        crate::flicker::size_of_a_flame_reaching(lamp.falloff),
        frame.time,
    )
}

/// `lamp` as it stands with `f`.
fn moved(mut lamp: LitLamp, f: Flicker, k: f32) -> LitLamp {
    lamp.position += f.sway;
    for (c, tint) in lamp.color.iter_mut().zip(f.tint) {
        *c *= k * tint;
    }
    lamp
}

/// How one found lamp is lit, before night and flicker.
fn shape(lamp: &HifiLamp) -> LitLamp {
    let range_mul = env_f32("DERETH_HIFI_LAMP_RANGE", 2.0);
    let warmth = env_f32("DERETH_HIFI_LAMP_WARMTH", 0.5);
    if let Some(p) = lamp.pack {
        // An art pack's light is as the pack says, its glow in the air for a halo.
        return LitLamp {
            position: lamp.position,
            flickers: lamp.flicker,
            color: lamp.color.map(|c| c * lamp.intensity),
            range: lamp.falloff,
            radius: (p.size * 0.5).max(0.02),
            skip: if p.shadows { 0.1 } else { lamp.falloff },
            glow: 0.0,
            halo: if p.haze > 0.0 { lamp.falloff } else { 0.0 },
            source: 0.0,
            air: p.haze,
            presence: p.presence,
        };
    }
    let (color, brightness, range, radius, skip, glow, halo, source) = match lamp.source {
        LampSource::Authored => (
            toward(lamp.color, LAMPLIGHT, warmth),
            (lamp.intensity / 100.0).clamp(0.3, 1.0),
            (lamp.falloff * range_mul).clamp(5.0, 16.0),
            0.08,
            0.45,
            0.32,
            1.0,
            0.0,
        ),
        LampSource::Flame => (
            toward(lamp.color, FIRE, 0.6),
            0.9,
            (lamp.falloff * range_mul * 0.75).clamp(4.0, 10.0),
            0.06,
            0.35,
            0.25,
            0.75,
            1.0,
        ),
        LampSource::Ember => (EMBER, 0.18, 4.0, 0.3, 0.3, 0.0, 0.0, 1.0),
        LampSource::Sprite => (
            lamp.color,
            0.45,
            (lamp.falloff * range_mul * 0.5).clamp(3.0, 8.0),
            0.2,
            0.35,
            0.0,
            0.0,
            1.0,
        ),
        LampSource::Glow => (
            toward(lamp.color, LAMPLIGHT, warmth * 0.6),
            (lamp.intensity * 0.55).clamp(0.3, 1.0),
            (lamp.falloff * range_mul * 0.8).clamp(4.0, 12.0),
            0.1,
            0.5,
            0.4,
            0.85,
            2.0,
        ),
    };
    LitLamp {
        position: lamp.position,
        color: color.map(|c| c * brightness),
        range,
        radius,
        skip,
        glow,
        halo,
        source,
        flickers: lamp.flicker,
        air: 0.0,
        presence: 0.0,
    }
}

/// The frustum planes of `m` (render space to clip), each `n·p + d >= 0` inside.
fn planes(m: Mat4) -> [Vec4; 6] {
    let r0 = m.row(0);
    let r1 = m.row(1);
    let r2 = m.row(2);
    let r3 = m.row(3);
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2].map(|p| {
        let l = p.truncate().length().max(1e-6);
        p / l
    })
}

fn inside(planes: &[Vec4; 6], p: Vec3, r: f32) -> bool {
    planes.iter().all(|pl| pl.truncate().dot(p) + pl.w >= -r)
}

/// The halos of the lamps in the rooms the frame reached, the nearest first, as the outdoor lamps
/// show theirs: the same shape and flicker, at any hour.
#[must_use]
pub fn room_halos(frame: &HifiFrame) -> Vec<(usize, LitLamp)> {
    let cam = &frame.camera;
    let pl = planes(cam.projection * cam.view * HEIGHT_UP);
    let mut near: Vec<(f32, LitLamp)> = Vec::new();
    for lamp in &frame.room_lamps {
        let l = shape(lamp);
        let d = (l.position - cam.eye).length();
        if l.halo <= 0.0 || d > ROOM_HALO_REACH || !inside(&pl, l.position, l.halo) {
            continue;
        }
        if near
            .iter()
            .any(|(_, n)| (n.position - l.position).length() < 0.6)
        {
            continue;
        }
        near.push((d, moved(l, flicker(frame, lamp), 1.0)));
    }
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter()
        .take(MAX_HALOS)
        .enumerate()
        .map(|(i, (_, l))| (i, l))
        .collect()
}

/// How far a room's flame shows its halo, metres.
const ROOM_HALO_REACH: f32 = 60.0;

/// The lamps worth lighting this frame, the most important first: inside the view, within reach
/// of the eye, ordered by how bright they stand to the eye. And those given a halo.
#[must_use]
pub fn choose(frame: &HifiFrame, night: f32) -> (Vec<LitLamp>, Vec<(usize, LitLamp)>) {
    let cam = &frame.camera;
    let pl = planes(cam.projection * cam.view * HEIGHT_UP);
    let reach = lamp_reach(frame);
    let halo_reach = HALO_REACH * distance_share(frame);
    let mut lit: Vec<(f32, LitLamp)> = Vec::new();
    for lamp in &frame.lamps {
        let l = shape(lamp);
        let d = (l.position - cam.eye).length();
        if d - l.range > reach || !inside(&pl, l.position, l.range) {
            continue;
        }
        // An art pack's light shines day and night; the others come on at dusk.
        let on = if lamp.pack.is_some() { 1.0 } else { night };
        if on <= 0.001 {
            continue;
        }
        let l = moved(
            l,
            flicker(frame, lamp),
            on * fade_toward(d - l.range, reach),
        );
        let peak = l.color[0].max(l.color[1]).max(l.color[2]);
        let near = (d - l.range).max(1.0);
        lit.push((peak * l.range * l.range / (near * near), l));
    }
    lit.sort_by(|a, b| b.0.total_cmp(&a.0));
    // Two objects' lights standing in one place (a torch and its post, say) are one lamp.
    let mut kept: Vec<(f32, LitLamp)> = Vec::with_capacity(lit.len());
    for (s, l) in lit {
        if !kept
            .iter()
            .any(|(_, k)| (k.position - l.position).length() < 0.6)
        {
            kept.push((s, l));
        }
    }
    let mut lit = kept;
    lit.truncate(MAX_LAMPS);
    let halos: Vec<(usize, LitLamp)> = lit
        .iter()
        .enumerate()
        .filter(|(_, (_, l))| {
            l.halo > 0.0
                && (l.position - cam.eye).length() < halo_reach
                && inside(&pl, l.position, l.halo)
        })
        .take(MAX_HALOS)
        .map(|(i, (_, l))| {
            // The halo fades out toward the end of its reach rather than leaving at a line.
            let fade = fade_toward((l.position - cam.eye).length(), halo_reach);
            (
                i,
                LitLamp {
                    color: l.color.map(|c| c * fade),
                    ..*l
                },
            )
        })
        .collect();
    (lit.into_iter().map(|(_, l)| l).collect(), halos)
}

struct LampGpu {
    trace: wgpu::ComputePipeline,
    enclosure: wgpu::ComputePipeline,
    open: wgpu::Buffer,
    trace_layout: wgpu::BindGroupLayout,
    filter: wgpu::ComputePipeline,
    filter_layout: wgpu::BindGroupLayout,
    halo: wgpu::RenderPipeline,
    halo_layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    lamps: wgpu::Buffer,
    steps: Vec<wgpu::Buffer>,
    halo_params: wgpu::Buffer,
    halos: wgpu::Buffer,
}

fn entry(
    binding: u32,
    stages: wgpu::ShaderStages,
    ty: wgpu::BindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: stages,
        ty,
        count: None,
    }
}

fn tex(sample_type: wgpu::TextureSampleType, dim: wgpu::TextureViewDimension) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type,
        view_dimension: dim,
        multisampled: false,
    }
}

fn buf(ty: wgpu::BufferBindingType) -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn storage_out() -> wgpu::BindingType {
    wgpu::BindingType::StorageTexture {
        access: wgpu::StorageTextureAccess::WriteOnly,
        format: OUT,
        view_dimension: wgpu::TextureViewDimension::D2,
    }
}

fn buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl LampGpu {
    #[allow(clippy::too_many_lines)]
    fn new(device: &wgpu::Device, hdr: wgpu::TextureFormat) -> Self {
        let c = wgpu::ShaderStages::COMPUTE;
        let f = wgpu::ShaderStages::FRAGMENT;
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let d2 = wgpu::TextureViewDimension::D2;
        let trace_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity lamp light"),
            entries: &[
                entry(0, c, tex(wgpu::TextureSampleType::Depth, d2)),
                entry(1, c, tex(float, d2)),
                entry(
                    2,
                    c,
                    wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                ),
                entry(3, c, storage_out()),
                entry(4, c, buf(wgpu::BufferBindingType::Uniform)),
                entry(
                    5,
                    c,
                    tex(
                        wgpu::TextureSampleType::Float { filterable: true },
                        wgpu::TextureViewDimension::D2Array,
                    ),
                ),
                entry(
                    6,
                    c,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    7,
                    c,
                    buf(wgpu::BufferBindingType::Storage { read_only: true }),
                ),
                entry(
                    8,
                    c,
                    buf(wgpu::BufferBindingType::Storage { read_only: true }),
                ),
                entry(
                    9,
                    c,
                    buf(wgpu::BufferBindingType::Storage { read_only: true }),
                ),
                entry(10, c, tex(float, d2)),
                entry(
                    11,
                    c,
                    buf(wgpu::BufferBindingType::Storage { read_only: false }),
                ),
            ],
        });
        let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity lamp filter"),
            entries: &[
                entry(0, c, tex(float, d2)),
                entry(1, c, tex(float, d2)),
                entry(2, c, storage_out()),
                entry(3, c, buf(wgpu::BufferBindingType::Uniform)),
                entry(4, c, tex(wgpu::TextureSampleType::Depth, d2)),
            ],
        });
        let halo_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity lamp halos"),
            entries: &[
                entry(0, f, tex(wgpu::TextureSampleType::Depth, d2)),
                entry(1, f, buf(wgpu::BufferBindingType::Uniform)),
                entry(
                    2,
                    f,
                    buf(wgpu::BufferBindingType::Storage { read_only: true }),
                ),
                entry(
                    3,
                    f,
                    buf(wgpu::BufferBindingType::Storage { read_only: true }),
                ),
            ],
        });
        let compute =
            |label: &str, src: &str, layout: &wgpu::BindGroupLayout, entry_point: &str| {
                let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl(src.into()),
                });
                let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(label),
                    bind_group_layouts: &[Some(layout)],
                    immediate_size: 0,
                });
                device.build_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pl),
                    module: &module,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                })
            };
        let trace = compute(
            "high-fidelity lamp light",
            LAMPS_WGSL,
            &trace_layout,
            "main",
        );
        let enclosure = compute(
            "high-fidelity lamp enclosure",
            LAMPS_WGSL,
            &trace_layout,
            "enclosure",
        );
        let filter = compute(
            "high-fidelity lamp filter",
            FILTER_WGSL,
            &filter_layout,
            "filter_step",
        );
        let halo_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity lamp halos"),
            source: wgpu::ShaderSource::Wgsl(HALO_WGSL.into()),
        });
        let halo_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity lamp halos"),
            bind_group_layouts: &[Some(&halo_layout)],
            immediate_size: 0,
        });
        let halo = device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("high-fidelity lamp halos"),
            layout: Some(&halo_pl),
            vertex: wgpu::VertexState {
                module: &halo_module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &halo_module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: hdr,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let u = wgpu::BufferUsages::UNIFORM;
        let s = wgpu::BufferUsages::STORAGE;
        Self {
            trace,
            enclosure,
            open: buffer(
                device,
                "high-fidelity lamp openness",
                MAX_LAMPS as u64 * 4,
                s,
            ),
            trace_layout,
            filter,
            filter_layout,
            halo,
            halo_layout,
            params: buffer(device, "high-fidelity lamp light", 256, u),
            lamps: buffer(
                device,
                "high-fidelity lamps",
                MAX_LAMPS as u64 * LAMP_BYTES,
                s,
            ),
            steps: (0..3)
                .map(|_| buffer(device, "high-fidelity lamp filter", 32, u))
                .collect(),
            halo_params: buffer(device, "high-fidelity lamp halos", 128, u),
            halos: buffer(
                device,
                "high-fidelity lamp halos",
                MAX_HALOS as u64 * HALO_BYTES,
                s,
            ),
        }
    }
}

/// The lamps' accumulated pictures and the size they were made at.
type LampTargets = Option<((u32, u32), Vec<(wgpu::Texture, wgpu::TextureView)>)>;

/// The lamps' light, kept across frames.
#[derive(Default)]
pub struct LampLight {
    gpu: Option<LampGpu>,
    /// The same, built ahead by the warm-up.
    pre_gpu: crate::build::Prebuilt<LampGpu>,
    targets: LampTargets,
    history: usize,
    still: u32,
    last_camera: Option<([f32; 16], [f32; 16])>,
    frame: u32,
    /// The lamps given a halo this frame, each with its place in the lit list.
    halos: Vec<(usize, LitLamp)>,
}

impl std::fmt::Debug for LampLight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LampLight")
            .field("still", &self.still)
            .finish_non_exhaustive()
    }
}

/// The lamps' light picture of a frame: rgb the light on each pixel, alpha how much of it is a
/// lamp's own glowing part (negative where nothing is lit).
#[derive(Debug)]
pub struct LampPicture<'a> {
    pub view: &'a wgpu::TextureView,
}

impl LampLight {
    /// Whether the lamps are lit for `level` on a device with ray queries.
    #[must_use]
    pub fn wanted(level: Level, ray_query: bool) -> bool {
        ray_query && level.is_on() && std::env::var_os("DERETH_HIFI_LAMPS_OFF").is_none()
    }

    /// Whether the lamps' device objects are made, or are not being built.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.gpu.is_some() || !self.pre_gpu.waits()
    }

    /// Ask for the lamps' device objects to be built ahead, when they are drawn (`wanted`).
    pub fn warm(&mut self, cx: &mut crate::graph::WarmCx<'_>, wanted: bool) {
        if wanted && self.gpu.is_none() {
            let device = cx.device.clone();
            self.pre_gpu.ask(cx.warm, move || {
                LampGpu::new(&device, crate::derive::pipeline::HDR_FORMAT)
            });
        }
    }

    /// Lights the frame's lamps; `None` by day or with no lamp in view.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &crate::passes::rt::SceneBindings<'_>,
        frame: &HifiFrame,
        hdr: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
        normals: &wgpu::TextureView,
        size: (u32, u32),
        viewport: [f32; 4],
        timer: &mut crate::timing::GpuTimer,
    ) -> Option<LampPicture<'_>> {
        self.halos.clear();
        let n = night(frame);
        if frame.lamps.is_empty() || n <= 0.001 && !shine_by_day(frame) {
            return None;
        }
        let (lit, halos) = choose(frame, n);
        if std::env::var_os("DERETH_HIFI_LAMPS_TRACE").is_some() {
            eprintln!(
                "lamps: {} in the frame, {} lit, {} halos, night {n:.2}",
                frame.lamps.len(),
                lit.len(),
                halos.len()
            );
        }
        if std::env::var_os("DERETH_HIFI_LAMPS_LIST").is_some() {
            let cam = &frame.camera;
            for l in &frame.lamps {
                eprintln!(
                    "  lamp {:?} at ({:.1},{:.1},{:.1}) {:.0} m from the eye, lit {}",
                    l.source,
                    l.position.x,
                    l.position.y,
                    l.position.z,
                    (l.position - cam.eye).length(),
                    lit.iter()
                        .any(|k| (k.position - l.position).length() < 0.01)
                );
            }
            eprintln!("  eye ({:.1},{:.1},{:.1})", cam.eye.x, cam.eye.y, cam.eye.z);
        }
        self.halos = halos;
        if lit.is_empty() {
            return None;
        }
        let pre = &mut self.pre_gpu;
        let gpu = self.gpu.get_or_insert_with(|| {
            pre.take()
                .filter(|_| hdr == crate::derive::pipeline::HDR_FORMAT)
                .unwrap_or_else(|| LampGpu::new(device, hdr))
        });

        // --- the targets: 0-1 the accumulated pair, 2-3 the filter's ---------------------------
        let fresh = self.targets.as_ref().is_none_or(|t| t.0 != size);
        if fresh {
            let mk = || {
                let t = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("high-fidelity lamp light"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: OUT,
                    usage: wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let v = t.create_view(&wgpu::TextureViewDescriptor::default());
                (t, v)
            };
            self.targets = Some((size, (0..4).map(|_| mk()).collect()));
        }
        let cam = &frame.camera;
        let key = (cam.view.to_cols_array(), cam.projection.to_cols_array());
        let still = !fresh && self.last_camera.as_ref().is_some_and(|k| *k == key);
        self.last_camera = Some(key);
        self.still = if still { (self.still + 1).min(255) } else { 0 };
        let history = self.history;
        let write = 1 - history;
        self.history = write;
        self.frame = self.frame.wrapping_add(1);

        // --- the lamps -------------------------------------------------------------------------
        let mut lb: Vec<f32> = Vec::with_capacity(lit.len() * 12);
        for l in &lit {
            lb.extend([l.position.x, l.position.y, l.position.z, l.range]);
            lb.extend([l.color[0], l.color[1], l.color[2], l.radius]);
            lb.extend([l.skip, l.glow, l.source, 0.0]);
        }
        queue.write_buffer(&gpu.lamps, 0, &floats(&lb));
        let mut p: Vec<f32> = Vec::with_capacity(64);
        p.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
        p.extend((cam.view * HEIGHT_UP).inverse().to_cols_array());
        p.extend(viewport);
        #[allow(clippy::cast_precision_loss)]
        p.extend([cam.eye.x, cam.eye.y, cam.eye.z, lit.len() as f32]);
        // Flickering lamps change from frame to frame, so a still view gathers only a few, or it
        // would smooth their flicker away.
        let gather = if frame.flame_flicker && lit.iter().any(|l| l.flickers) {
            3
        } else {
            24
        };
        #[allow(clippy::cast_precision_loss)]
        p.extend([
            env_f32("DERETH_HIFI_LAMP_GAIN", 6.0),
            (self.frame % 4096) as f32,
            self.still.min(gather) as f32,
            0.0,
        ]);
        p.extend([
            env_f32("DERETH_HIFI_LAMP_CORE", 0.6),
            if std::env::var_os("DERETH_HIFI_LAMPS_NO_RAYS").is_some() {
                0.0
            } else {
                1.0
            },
            if std::env::var_os("DERETH_HIFI_LAMPS_MARK").is_some() {
                1.0
            } else {
                0.0
            },
            0.0,
        ]);
        queue.write_buffer(&gpu.params, 0, &floats(&p));

        let targets = &self.targets.as_ref()?.1;
        let tv = |i: usize| wgpu::BindingResource::TextureView(&targets[i].1);
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("high-fidelity lamp light"),
            layout: &gpu.trace_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(normals),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: scene.tlas.as_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: tv(write),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: gpu.params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(scene.atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(scene.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: scene.cuts.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: scene.uvs.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: gpu.lamps.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: tv(history),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: gpu.open.as_entire_binding(),
                },
            ],
        });
        let groups = (size.0.div_ceil(8), size.1.div_ceil(8));
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("lamp enclosure"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&gpu.enclosure);
            pass.set_bind_group(0, &bind, &[]);
            #[allow(clippy::cast_possible_truncation)]
            pass.dispatch_workgroups((lit.len() as u32).div_ceil(64), 1, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("lamp light"),
                timestamp_writes: timer.compute_writes("lamp light"),
            });
            pass.set_pipeline(&gpu.trace);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        }

        // --- the filter, fewer steps as a still view gathers -----------------------------------
        let steps = match self.still {
            0..=3 => 3,
            4..=11 => 2,
            _ => 1,
        };
        let a = cam.projection.z_axis.z;
        let b = cam.projection.w_axis.z;
        let mut src = write;
        for (i, step) in gpu.steps.iter().take(steps).enumerate() {
            let mut bytes: Vec<u8> = [1i32 << i, 0, 0, 0]
                .iter()
                .flat_map(|x| x.to_le_bytes())
                .collect();
            bytes.extend(floats(&[a, b, 0.0, 0.0]));
            queue.write_buffer(step, 0, &bytes);
            let dst = 2 + (i % 2);
            let fb = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity lamp filter"),
                layout: &gpu.filter_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: tv(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(normals),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: tv(dst),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: step.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("lamp filter"),
                timestamp_writes: if i == 0 {
                    timer.compute_writes("lamp filter")
                } else {
                    None
                },
            });
            pass.set_pipeline(&gpu.filter);
            pass.set_bind_group(0, &fb, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
            src = dst;
        }
        Some(LampPicture {
            view: &targets[src].1,
        })
    }

    /// Readies the halos of the room's flames, while no lamps are lit. Whether there are any.
    pub fn prepare_room_halos(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &HifiFrame,
        hdr: wgpu::TextureFormat,
    ) -> bool {
        self.halos = room_halos(frame);
        if self.halos.is_empty() {
            return false;
        }
        let pre = &mut self.pre_gpu;
        let gpu = self.gpu.get_or_insert_with(|| {
            pre.take()
                .filter(|_| hdr == crate::derive::pipeline::HDR_FORMAT)
                .unwrap_or_else(|| LampGpu::new(device, hdr))
        });
        // A wall in front of a flame hides its halo by depth, so each stands fully open.
        queue.write_buffer(&gpu.open, 0, &floats(&vec![1.0; self.halos.len()]));
        true
    }

    /// Draws the halos of the lamps lit this frame into `target`, the lit picture.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_halos(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &HifiFrame,
        depth: &wgpu::TextureView,
        target: &wgpu::TextureView,
        viewport: [f32; 4],
        timer: &mut crate::timing::GpuTimer,
    ) -> bool {
        let gain = env_f32("DERETH_HIFI_LAMP_HALO", 0.5);
        if self.halos.is_empty() || gain <= 0.0 {
            return false;
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return false;
        };
        let cam = &frame.camera;
        let day = 1.0 - night(frame);
        let mut hb: Vec<f32> = Vec::with_capacity(self.halos.len() * 12);
        for (i, l) in &self.halos {
            hb.extend([l.position.x, l.position.y, l.position.z, l.halo]);
            #[allow(clippy::cast_precision_loss)]
            hb.extend([l.color[0], l.color[1], l.color[2], *i as f32]);
            let held = 1.0 + l.presence * (DAY_GLOW - 1.0) * day;
            hb.extend([l.air, held, l.range, l.radius]);
        }
        queue.write_buffer(&gpu.halos, 0, &floats(&hb));
        let mut p: Vec<f32> = Vec::with_capacity(32);
        p.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
        p.extend(viewport);
        #[allow(clippy::cast_precision_loss)]
        p.extend([cam.eye.x, cam.eye.y, cam.eye.z, self.halos.len() as f32]);
        p.extend([
            env_f32("DERETH_HIFI_LAMP_HALO_CORE", 0.14),
            env_f32("DERETH_HIFI_LAMP_HALO_WIDE", 0.7),
            env_f32("DERETH_HIFI_LAMP_HALO_SHARE", 0.10),
            gain,
        ]);
        // Indoors a lamp's own frame stands before its halo, so the light comes from within.
        let air = env_f32("DERETH_HIFI_LAMP_AIR", 0.12);
        p.extend(if frame.sky.outdoor {
            [0.35, 0.15, air, 0.0]
        } else {
            [0.0, 0.03, air, 0.0]
        });
        queue.write_buffer(&gpu.halo_params, 0, &floats(&p));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("high-fidelity lamp halos"),
            layout: &gpu.halo_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: gpu.halo_params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: gpu.halos.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: gpu.open.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lamp halos"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timer.writes("lamp halos"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&gpu.halo);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_viewport(viewport[0], viewport[1], viewport[2], viewport[3], 0.0, 1.0);
        pass.draw(0..3, 0..1);
        true
    }
}

/// The lamp shaders' sources, for validation.
#[must_use]
pub fn shaders() -> [(&'static str, String); 3] {
    [
        ("lamp light", LAMPS_WGSL.to_owned()),
        ("lamp filter", FILTER_WGSL.to_owned()),
        ("lamp halos", HALO_WGSL.to_owned()),
    ]
}
