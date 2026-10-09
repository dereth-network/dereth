//! The re-shaded world: which part of the frame is re-shaded, the filter that hands each of its
//! draws the pipeline derived from its own, and the passes that read what it wrote -- the neutral
//! resolve to the frame target's format, the landscape's smooth normals, and the normal and
//! census views.
//!
//! **The re-shaded world draws the same draws, in the same order, with the same binds.** Each
//! recorded draw of the world is replayed with the pipeline derived from the one it was recorded
//! with, which shades it the same way but writes linear light, unclamped, and the view-space
//! normal with a material class. The neutral resolve writes that linear light back out through a
//! plain sRGB encode, so apart from blending, which is done in linear light, it is the ordinary
//! frame: the parity view.
//!
//! **The sky is drawn as recorded.** Its layers blend over one another, and blended in linear
//! light they would not be the sky the ordinary frame draws; so the sky's first pass, which opens
//! the world, is replayed with its own pipelines and lifted into linear light, and the world is
//! re-shaded over it. (The weather layer, drawn after the landscape, is re-shaded.)
//!
//! **A frame is re-shaded whole or not at all.** Until every draw of the part to re-shade has its
//! derived pipeline, the frame is drawn exactly as the ordinary frame is (the overlay replay), and
//! the missing pipelines are built off the frame's thread meanwhile.
//!
//! **A frame that steps indoors is re-shaded up to that step.** From the first indoor flush,
//! indoor cell or depth clear of the world on, the rest of the world is replayed with its own
//! pipelines over the resolved picture. Screen-space work after that point sees the depth as it
//! stood before the clear. The rooms of buildings seen from outdoors are not such a step: they
//! are drawn into the outdoor world with its depth, and are re-shaded with it; the plan names
//! their commands, so light meant for the outdoors can leave them out.

use std::ops::Range;

use dereth_render::wgpu::sidecar::{
    Census, DrawAction, DrawNote, Mark, ReplayFilter, SideTables, SidecarContext,
};

use crate::derive::cache::PipelineCache;
use crate::derive::pipeline::{HDR_FORMAT, NORMAL_FORMAT};
use crate::derive::{DerivedKey, Variant};
use crate::resources::{Resource, ResourceName, Resources, TextureSpec};
use crate::shared::fullscreen::COLOUR_WGSL;
use crate::shared::terrain_field::{self, FieldSignature, TerrainField};
use crate::{DebugView, HifiError, HifiFrame};

/// How a re-shaded frame's recording is cut up, as command ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReshadePlan {
    /// Before the world: replayed with the recorded pipelines.
    pub pre: Range<u32>,
    /// The sky's first pass, where it opens the world: replayed with the recorded pipelines too,
    /// then lifted into linear light. Empty when the world does not open with it.
    pub sky: Range<u32>,
    /// The world up to the flush of its translucent draws: re-shaded.
    pub opaque: Range<u32>,
    /// The translucent draws, up to the cut: re-shaded, after the opaque screen-space work.
    pub alpha: Range<u32>,
    /// From the cut to the world's end: replayed with the recorded pipelines over the resolved
    /// picture. Empty unless the frame steps indoors.
    pub rest: Range<u32>,
    /// The commands of the rooms of buildings drawn into the outdoor world, each bracket of them
    /// in order.
    pub interiors: Vec<Range<u32>>,
}

impl ReshadePlan {
    /// The plan for a frame with `tables`, or `None` when it drew no whole world.
    #[must_use]
    pub fn new(tables: &SideTables) -> Option<Self> {
        let world = tables.world_span()?;
        let inside = |at: u32| at > world.start && at < world.end;
        // Where the world steps indoors: an indoor flush, or a cell drawn outside a bracket of
        // building rooms seen from outdoors; and the brackets themselves.
        let mut interiors = Vec::new();
        let mut open: Option<u32> = None;
        let mut indoor: Option<u32> = None;
        for (at, m) in tables
            .marks
            .iter()
            .filter(|(at, _)| *at >= world.start && *at < world.end)
        {
            match m {
                Mark::Interiors => open = Some(*at),
                Mark::InteriorsEnd => {
                    if let Some(start) = open.take() {
                        if *at > start {
                            interiors.push(start..*at);
                        }
                    }
                }
                Mark::IndoorFlush => indoor = Some(indoor.map_or(*at, |i| i.min(*at))),
                Mark::EnvCell { .. } if open.is_none() => {
                    indoor = Some(indoor.map_or(*at, |i| i.min(*at)));
                }
                _ => {}
            }
        }
        let indoor = indoor
            .into_iter()
            .chain(tables.clears.iter().copied().filter(|at| inside(*at)))
            .min();
        let cut = indoor.unwrap_or(world.end).max(world.start);
        let alpha = tables
            .marks
            .iter()
            .find(|(at, m)| *m == Mark::AlphaFlush && *at >= world.start && *at <= world.end)
            .map_or(cut, |(at, _)| (*at).min(cut));
        // The sky's first pass, when the world opens with it.
        let opens = tables
            .marks
            .iter()
            .skip_while(|(at, m)| *at < world.start || *m == Mark::WorldBegin)
            .take_while(|(at, _)| *at == world.start)
            .any(|(_, m)| *m == Mark::SkyBegin(0));
        let sky_end = if opens {
            tables
                .marks
                .iter()
                .find(|(at, m)| *at >= world.start && *m == Mark::SkyEnd(0))
                .map_or(world.start, |(at, _)| (*at).min(alpha))
        } else {
            world.start
        };
        Some(Self {
            pre: 0..world.start,
            sky: world.start..sky_end,
            opaque: sky_end..alpha,
            alpha: alpha..cut,
            rest: cut..world.end,
            interiors: interiors.into_iter().filter(|r| r.start < cut).collect(),
        })
    }

    /// Whether the frame steps indoors, so only its start is re-shaded.
    #[must_use]
    pub fn is_split(&self) -> bool {
        !self.rest.is_empty()
    }

    /// The command range that is re-shaded.
    #[must_use]
    pub fn reshaded(&self) -> Range<u32> {
        self.opaque.start..self.alpha.end
    }

    /// The commands replayed with the recorded pipelines before the re-shade: everything before
    /// the world, and the sky's first pass.
    #[must_use]
    pub fn recorded_before(&self) -> Range<u32> {
        self.pre.start..self.sky.end
    }
}

/// What the re-shade does with one recorded draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawClass {
    /// Its derived pipeline is built: it is re-shaded.
    Reshade(DerivedKey),
    /// Its derived pipeline is asked for and not built yet.
    Waiting(DerivedKey),
    /// Its derived pipeline could not be built.
    Failed(DerivedKey),
    /// It has no note, so nothing can be derived for it.
    Unclassified,
}

/// The derived key a noted draw is re-shaded with.
#[must_use]
pub fn derived_key(note: &DrawNote) -> DerivedKey {
    DerivedKey {
        key: note.key,
        splat: note.splat,
        variant: Variant::Reshade,
    }
}

/// What the re-shade does with the draw `note` describes, given what `cache` holds.
#[must_use]
pub fn classify(note: Option<&DrawNote>, cache: &PipelineCache) -> DrawClass {
    let Some(note) = note else {
        return DrawClass::Unclassified;
    };
    let key = derived_key(note);
    if cache.is_ready(&key) {
        DrawClass::Reshade(key)
    } else if cache.has_failed(&key) {
        DrawClass::Failed(key)
    } else {
        DrawClass::Waiting(key)
    }
}

/// How the draws a plan re-shades stand, as counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Readiness {
    /// Draws whose pipeline is built.
    pub ready: u32,
    /// Draws whose pipeline is still being built.
    pub waiting: u32,
    /// Draws whose pipeline could not be built.
    pub failed: u32,
    /// Draws with no note.
    pub unclassified: u32,
}

impl Readiness {
    /// Whether every draw is ready.
    #[must_use]
    pub fn all_ready(&self) -> bool {
        self.waiting == 0 && self.failed == 0 && self.unclassified == 0
    }
}

/// How the draws `plan` re-shades stand in `cache`.
#[must_use]
pub fn readiness(plan: &ReshadePlan, tables: &SideTables, cache: &PipelineCache) -> Readiness {
    let range = plan.reshaded();
    let mut r = Readiness::default();
    // Draws come in runs of one pipeline: each run is looked up once.
    let mut last: Option<(DerivedKey, DrawClass)> = None;
    for note in tables.draws.iter().filter(|d| range.contains(&d.cmd)) {
        let key = derived_key(note);
        let class = match last {
            Some((k, c)) if k == key => c,
            _ => {
                let c = classify(Some(note), cache);
                last = Some((key, c));
                c
            }
        };
        match class {
            DrawClass::Reshade(_) => r.ready += 1,
            DrawClass::Waiting(_) => r.waiting += 1,
            DrawClass::Failed(_) => r.failed += 1,
            DrawClass::Unclassified => r.unclassified += 1,
        }
    }
    let unnoted = tables
        .unnoted
        .iter()
        .filter(|at| range.contains(at))
        .count();
    r.unclassified += u32::try_from(unnoted).unwrap_or(u32::MAX);
    r
}

/// The filter that replays each recorded draw with the pipeline derived from its own.
#[derive(Debug)]
pub struct ReshadeFilter<'c> {
    cache: &'c PipelineCache,
    /// The last draw's key and pipeline: draws come in runs of one pipeline, and a draw may be
    /// asked about twice.
    last: Option<(DerivedKey, Option<&'c wgpu::RenderPipeline>)>,
}

impl<'c> ReshadeFilter<'c> {
    /// The filter drawing with `cache`'s pipelines.
    #[must_use]
    pub fn new(cache: &'c PipelineCache) -> Self {
        Self { cache, last: None }
    }
}

impl ReplayFilter for ReshadeFilter<'_> {
    fn draw(&mut self, _cmd: u32, note: Option<&DrawNote>) -> DrawAction<'_> {
        let Some(note) = note else {
            return DrawAction::Legacy;
        };
        let key = derived_key(note);
        let pipeline = match self.last {
            Some((k, p)) if k == key => p,
            _ => {
                let p = self.cache.get(&key);
                self.last = Some((key, p));
                p
            }
        };
        // A plan is only carried out once every draw in it is ready.
        pipeline.map_or(DrawAction::Legacy, DrawAction::Pipeline)
    }
}

/// The re-shaded world's targets: linear light and the view-space normals with the material
/// class.
#[derive(Debug, Clone)]
pub struct ReshadeTargets {
    /// The colour, linear light, unclamped.
    pub hdr: wgpu::TextureView,
    /// The view-space normal, with the material class in alpha.
    pub normal: wgpu::TextureView,
}

impl ReshadeTargets {
    /// The targets at the frame target's size, made or remade as it changes.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when they do not fit the video memory budget.
    pub fn new(cx: &SidecarContext<'_>, resources: &mut Resources) -> Result<Self, HifiError> {
        let (width, height) = cx.surface_size();
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let hdr = resources.texture(
            cx.device(),
            ResourceName::SceneHdr,
            TextureSpec {
                width,
                height,
                format: HDR_FORMAT,
                usage: usage | wgpu::TextureUsages::COPY_SRC,
            },
        )?;
        let normal = resources.texture(
            cx.device(),
            ResourceName::ViewNormal,
            TextureSpec {
                width,
                height,
                format: NORMAL_FORMAT,
                usage: usage | wgpu::TextureUsages::COPY_SRC,
            },
        )?;
        Ok(Self { hdr, normal })
    }
}

/// The shader of the passes that read the re-shaded targets.
const RESHADE_WGSL: &str = r"
struct Params {
    // The world viewport: x, y, width, height, in target pixels.
    viewport: vec4<f32>,
    // x: 1 when the frame was re-shaded; y: the view the census shows.
    flags: vec4<f32>,
    // The replay's census: legacy, re-shaded, substituted, skipped.
    census: vec4<f32>,
};
@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var normals: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: Params;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

struct Lifted {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

// The picture drawn so far, as linear light, with no normal: what the world is re-shaded over.
@fragment
fn fs_lift(@builtin(position) p: vec4<f32>) -> Lifted {
    let c = textureLoad(picture, vec2<i32>(p.xy), 0);
    var o: Lifted;
    o.colour = vec4<f32>(srgb_to_linear(c.rgb), c.a);
    o.normal = vec4<f32>(0.0);
    return o;
}

// Linear light to the frame target's display encoding, clamped, nothing else.
@fragment
fn fs_resolve(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(picture, vec2<i32>(p.xy), 0);
    return vec4<f32>(linear_to_srgb(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0))), 1.0);
}

// The view-space normal as a colour: x red, y green, z blue, each from -1..1 to 0..1.
@fragment
fn fs_normals(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let n = textureLoad(normals, vec2<i32>(p.xy), 0);
    if (params.flags.x < 0.5 || dot(n.xyz, n.xyz) < 0.25) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    return vec4<f32>(n.xyz * 0.5 + vec3<f32>(0.5), 1.0);
}

fn class_colour(c: f32) -> vec3<f32> {
    if (c < 0.5) { return vec3<f32>(0.95, 0.85, 0.30); }
    if (c < 1.5) { return vec3<f32>(0.30, 0.85, 0.35); }
    if (c < 2.5) { return vec3<f32>(0.95, 0.55, 0.20); }
    if (c > 3.5) { return vec3<f32>(0.20, 0.90, 0.95); }
    return vec3<f32>(0.55, 0.40, 0.85);
}

// How each pixel was drawn: tinted by material class where the frame was re-shaded, red where
// it was drawn the ordinary way, with the census as four bars at the viewport's top left
// (as recorded red, re-shaded green, re-framed blue, skipped grey).
@fragment
fn fs_census(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    let c = textureLoad(picture, px, 0).rgb;
    let grey = vec3<f32>(dot(c, vec3<f32>(0.299, 0.587, 0.114)));
    let local = p.xy - params.viewport.xy;
    let total = max(params.census.x + params.census.y + params.census.z + params.census.w, 1.0);
    let bar = floor((local.y - 8.0) / 10.0);
    if (local.x >= 8.0 && local.y >= 8.0 && bar < 4.0 && (local.y - 8.0) - bar * 10.0 < 7.0) {
        let share = params.census[i32(bar)] / total;
        let width = (params.viewport.z / 3.0) * share;
        if (local.x - 8.0 < max(width, select(0.0, 2.0, share > 0.0))) {
            let colours = array<vec3<f32>, 4>(
                vec3<f32>(0.9, 0.2, 0.2),
                vec3<f32>(0.2, 0.9, 0.3),
                vec3<f32>(0.2, 0.4, 0.95),
                vec3<f32>(0.6, 0.6, 0.6),
            );
            return vec4<f32>(colours[i32(bar)], 1.0);
        }
    }
    if (params.flags.x < 0.5) {
        return vec4<f32>(mix(grey, vec3<f32>(0.9, 0.15, 0.15), 0.45), 1.0);
    }
    let n = textureLoad(normals, px, 0);
    return vec4<f32>(mix(grey, class_colour(n.w), 0.55), 1.0);
}
";

/// The landscape-normal pass's shader: where the depth lies on the drawn landscape, the view
/// normal becomes the landscape field's smooth normal.
fn ground_wgsl() -> String {
    format!(
        "
struct Ground {{
    // Clip space to render space, height up.
    clip_to_render: mat4x4<f32>,
    // Render space, height up, to view space.
    render_to_view: mat4x4<f32>,
    // The world viewport: x, y, width, height, in target pixels.
    viewport: vec4<f32>,
    // xyz the eye, render space.
    eye: vec4<f32>,
}};
@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var field_height: texture_2d<f32>;
@group(0) @binding(2) var field_normal: texture_2d<f32>;
@group(0) @binding(3) var field_class: texture_2d<u32>;
@group(0) @binding(4) var<uniform> field: TerrainFieldParams;
@group(0) @binding(5) var<uniform> ground: Ground;
@group(0) @binding(6) var prior_normals: texture_2d<f32>;
{field}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}}

@fragment
fn fs_ground(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
    let d = textureLoad(depth, vec2<i32>(p.xy), 0);
    if (d >= 1.0) {{
        discard;
    }}
    let v = ground.viewport;
    let uv = (p.xy - v.xy) / v.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let w = ground.clip_to_render * ndc;
    let world = w.xyz / w.w;
    let far = length(world - ground.eye.xyz);
    // Only what was drawn as landscape (unlit, from its composite) is landscape: a building's
    // floor, a wall's foot or a rock at ground height keeps its own class and normal.
    let prior = textureLoad(prior_normals, vec2<i32>(p.xy), 0);
    if (prior.w > 0.5) {{
        discard;
    }}
    let rise = world.z - plane_height(world.xy);
    let tolerance = 0.05 + far * 0.002;
    if (abs(rise) > tolerance) {{
        // Where blocks drawn at different detail meet, the strip that closes the gap between
        // them lies off the field's surface by up to a metre or two far out: a surface drawn
        // unlit and facing up that close to it is still the ground.
        let up = (transpose(ground.render_to_view) * vec4<f32>(prior.xyz, 0.0)).z;
        let seam = 0.5 + far * 0.01;
        if (abs(rise) > seam || dot(prior.xyz, prior.xyz) < 0.25 || up < 0.5) {{
            discard;
        }}
    }}
    if (field_class_at(world.xy) == 0u) {{
        discard;
    }}
    let n = (ground.render_to_view * vec4<f32>(field_normal_at(world.xy), 0.0)).xyz;
    return vec4<f32>(normalize(n), {terrain:.1});
}}
",
        field = terrain_field::wgsl(),
        terrain = crate::derive::reshade::MaterialClass::Terrain.encoded(),
    )
}

/// The shaders of the passes that read the re-shaded targets, for the device-free validation
/// test: the resolve and debug views, and the landscape-normal pass.
#[must_use]
pub fn shaders() -> [String; 2] {
    [format!("{RESHADE_WGSL}\n{COLOUR_WGSL}"), ground_wgsl()]
}

/// The pipelines of the passes that read the re-shaded targets, made on first use.
#[derive(Debug, Default)]
pub struct Reshader {
    views: Option<Views>,
    ground: Option<Ground>,
    /// What the landscape field on the device was built from, and its shader parameters.
    field: Option<(FieldSignature, [u8; 32])>,
    /// The bindings made for the pictures the passes read, kept while those pictures are.
    binds: Vec<(Vec<wgpu::TextureView>, wgpu::BindGroup)>,
}

/// The landscape field's three textures, by the names they are held under.
const FIELD: [ResourceName; 3] = [
    ResourceName::TerrainHeight,
    ResourceName::TerrainNormal,
    ResourceName::TerrainClass,
];

#[derive(Debug)]
struct Views {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    resolve: wgpu::RenderPipeline,
    lift: wgpu::RenderPipeline,
    normals: wgpu::RenderPipeline,
    census: wgpu::RenderPipeline,
    params: wgpu::Buffer,
}

#[derive(Debug)]
struct Ground {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    field_params: wgpu::Buffer,
    params: wgpu::Buffer,
}

fn fullscreen_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    format: wgpu::TextureFormat,
    write_mask: wgpu::ColorWrites,
) -> wgpu::RenderPipeline {
    fullscreen_pipeline_into(
        device,
        layout,
        module,
        entry,
        &[Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask,
        })],
    )
}

fn fullscreen_pipeline_into(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    targets: &[Option<wgpu::ColorTargetState>],
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("high-fidelity re-shade"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("high-fidelity re-shade"),
        layout: Some(&pipeline_layout),
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

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32, size: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    }
}

fn uniform_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("high-fidelity re-shade"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn make_views(device: &wgpu::Device, format: wgpu::TextureFormat) -> Views {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("high-fidelity re-shade"),
        source: wgpu::ShaderSource::Wgsl(shaders()[0].clone().into()),
    });
    let float = wgpu::TextureSampleType::Float { filterable: false };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("high-fidelity re-shade"),
        entries: &[
            texture_entry(0, float),
            texture_entry(1, float),
            uniform_entry(2, 48),
        ],
    });
    Views {
        format,
        resolve: fullscreen_pipeline(
            device,
            &layout,
            &module,
            "fs_resolve",
            format,
            // The ordinary frame never writes the target's alpha.
            wgpu::ColorWrites::COLOR,
        ),
        lift: fullscreen_pipeline_into(
            device,
            &layout,
            &module,
            "fs_lift",
            &crate::derive::pipeline::reshade_targets(None, false).map(Some),
        ),
        normals: fullscreen_pipeline(
            device,
            &layout,
            &module,
            "fs_normals",
            format,
            wgpu::ColorWrites::COLOR,
        ),
        census: fullscreen_pipeline(
            device,
            &layout,
            &module,
            "fs_census",
            format,
            wgpu::ColorWrites::COLOR,
        ),
        layout,
        params: uniform_buffer(device, 48),
    }
}

fn make_ground(device: &wgpu::Device) -> Ground {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("high-fidelity landscape normals"),
        source: wgpu::ShaderSource::Wgsl(ground_wgsl().into()),
    });
    let float = wgpu::TextureSampleType::Float { filterable: false };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("high-fidelity landscape normals"),
        entries: &[
            texture_entry(0, wgpu::TextureSampleType::Depth),
            texture_entry(1, float),
            texture_entry(2, float),
            texture_entry(3, wgpu::TextureSampleType::Uint),
            uniform_entry(4, 32),
            uniform_entry(5, 160),
            texture_entry(6, float),
        ],
    });
    Ground {
        pipeline: fullscreen_pipeline(
            device,
            &layout,
            &module,
            "fs_ground",
            NORMAL_FORMAT,
            wgpu::ColorWrites::ALL,
        ),
        layout,
        field_params: uniform_buffer(device, 32),
        params: uniform_buffer(device, 160),
    }
}

fn viewport_of(cx: &SidecarContext<'_>) -> [f32; 4] {
    #[allow(clippy::cast_precision_loss)] // pixel extents
    cx.world_viewport().map_or_else(
        || {
            let (w, h) = cx.surface_size();
            [0.0, 0.0, w as f32, h as f32]
        },
        |v| [v.x as f32, v.y as f32, v.width as f32, v.height as f32],
    )
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// A colour attachment of `view`, loaded and stored.
fn load(view: &wgpu::TextureView) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
    })
}

/// Begin a pass over `colour` (loaded) clipped to the world viewport.
fn viewport_pass<'e>(
    cx: &mut SidecarContext<'_>,
    encoder: &'e mut wgpu::CommandEncoder,
    label: &str,
    colour: &wgpu::TextureView,
    timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[load(colour)],
        depth_stencil_attachment: None,
        timestamp_writes: timestamps,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    cx.count_pass();
    if let Some(v) = cx.world_viewport() {
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
    pass
}

impl Reshader {
    fn views(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) -> &Views {
        if self.views.as_ref().is_none_or(|v| v.format != format) {
            self.views = Some(make_views(device, format));
        }
        self.views.as_ref().expect("made above")
    }

    /// Bring the landscape field up to date with `frame`'s blocks, held in `resources`.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when the field does not fit the budget.
    pub fn prepare_field(
        &mut self,
        cx: &SidecarContext<'_>,
        frame: &HifiFrame,
        resources: &mut Resources,
    ) -> Result<(), HifiError> {
        let signature = TerrainField::signature(&frame.terrain);
        let held = FIELD.iter().all(|n| resources.get(*n).is_some());
        if held && self.field.as_ref().is_some_and(|(s, _)| *s == signature) {
            for n in FIELD {
                resources.touch(n);
            }
            return Ok(());
        }
        self.field = None;
        for n in FIELD {
            resources.release(n);
        }
        let Some(field) = TerrainField::build(&frame.terrain) else {
            return Ok(());
        };
        resources.check(field.device_bytes())?;
        for (name, (texture, view, bytes)) in
            FIELD.into_iter().zip(field.upload(cx.device(), cx.queue()))
        {
            resources.insert(name, Resource::Texture(texture, view), bytes)?;
        }
        self.field = Some((signature, field.params()));
        Ok(())
    }

    /// Forget every binding and the landscape field, so nothing here holds a released resource.
    pub fn forget(&mut self) {
        self.binds.clear();
        self.field = None;
    }

    /// Whether the landscape field is on the device.
    #[must_use]
    pub fn has_field(&self) -> bool {
        self.field.is_some()
    }

    /// Write the landscape field's smooth normal into `targets.normal` wherever `depth` lies on
    /// the drawn landscape.
    #[allow(clippy::too_many_arguments)]
    pub fn ground_normals(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        targets: &ReshadeTargets,
        depth: &wgpu::TextureView,
        frame: &HifiFrame,
        resources: &mut Resources,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let Some((_, field_params)) = self.field.as_ref() else {
            return;
        };
        let (width, height) = cx.surface_size();
        let Ok(prior) = resources.texture(
            cx.device(),
            ResourceName::ScratchNormal,
            TextureSpec {
                width,
                height,
                format: NORMAL_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            },
        ) else {
            return;
        };
        let (Some(Resource::Texture(from, _)), Some(Resource::Texture(to, _))) = (
            resources.get(ResourceName::ViewNormal),
            resources.get(ResourceName::ScratchNormal),
        ) else {
            return;
        };
        encoder.copy_texture_to_texture(
            from.as_image_copy(),
            to.as_image_copy(),
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = |n| match resources.get(n) {
            Some(Resource::Texture(_, v)) => Some(v.clone()),
            _ => None,
        };
        let (Some(height), Some(normal), Some(class)) =
            (view(FIELD[0]), view(FIELD[1]), view(FIELD[2]))
        else {
            return;
        };
        let field_params = *field_params;
        if self.ground.is_none() {
            self.ground = Some(make_ground(cx.device()));
        }
        let Some(g) = self.ground.as_ref() else {
            return;
        };
        let camera = &frame.camera;
        let mut params = floats(
            &crate::shared::camera::render_from_clip(camera.view, camera.projection)
                .to_cols_array(),
        );
        params.extend(floats(
            &crate::shared::camera::view_from_render(camera.view).to_cols_array(),
        ));
        params.extend(floats(&viewport_of(cx)));
        params.extend(floats(&[camera.eye.x, camera.eye.y, camera.eye.z, 0.0]));
        cx.queue().write_buffer(&g.params, 0, &params);
        cx.queue().write_buffer(&g.field_params, 0, &field_params);
        let views = vec![
            depth.clone(),
            height.clone(),
            normal.clone(),
            class.clone(),
            prior.clone(),
        ];
        let cached = self
            .binds
            .iter()
            .find(|(k, _)| *k == views)
            .map(|(_, b)| b.clone());
        let bind = if let Some(b) = cached {
            b
        } else {
            let b = cx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity landscape normals"),
                layout: &g.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&height),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&class),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: g.field_params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: g.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&prior),
                    },
                ],
            });
            self.keep_bind(views, b.clone());
            b
        };
        let Some(g) = self.ground.as_ref() else {
            return;
        };
        let mut pass = viewport_pass(
            cx,
            encoder,
            "landscape normals",
            &targets.normal,
            timestamps,
        );
        pass.set_pipeline(&g.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }

    fn bind(
        &mut self,
        cx: &SidecarContext<'_>,
        picture: &wgpu::TextureView,
        normals: &wgpu::TextureView,
        params: &[u8],
    ) -> wgpu::BindGroup {
        let device = cx.device().clone();
        cx.queue()
            .write_buffer(&self.views(&device, cx.surface_format()).params, 0, params);
        let views = vec![picture.clone(), normals.clone()];
        if let Some((_, b)) = self.binds.iter().find(|(k, _)| *k == views) {
            return b.clone();
        }
        let v = self.views(&device, cx.surface_format());
        let b = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("high-fidelity re-shade"),
            layout: &v.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(picture),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(normals),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: v.params.as_entire_binding(),
                },
            ],
        });
        self.keep_bind(views, b.clone());
        b
    }

    /// Keep `bind` for `views`, forgetting the oldest beyond a handful: the pictures are remade
    /// only when the target's size changes.
    fn keep_bind(&mut self, views: Vec<wgpu::TextureView>, bind: wgpu::BindGroup) {
        if self.binds.len() >= 8 {
            self.binds.remove(0);
        }
        self.binds.push((views, bind));
    }

    /// Lift `picture`, the world drawn so far in the frame target's encoding, into `targets` as
    /// linear light with no normal, over the whole target.
    pub fn lift(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        picture: &wgpu::TextureView,
        targets: &ReshadeTargets,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let params = floats(&[viewport_of(cx), [1.0, 0.0, 0.0, 0.0], [0.0; 4]].concat());
        // The normals target is written, so the picture stands in as the bound second texture.
        let bind = self.bind(cx, picture, picture, &params);
        let Some(v) = self.views.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky lift"),
            color_attachments: &[load(&targets.hdr), load(&targets.normal)],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.count_pass();
        pass.set_pipeline(&v.lift);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The neutral resolve: write `targets.hdr` into `colour` inside the world viewport, encoded
    /// as the frame target is, clamped, with nothing else done to it.
    pub fn resolve(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        targets: &ReshadeTargets,
        colour: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let params = floats(&[viewport_of(cx), [1.0, 0.0, 0.0, 0.0], [0.0; 4]].concat());
        let bind = self.bind(cx, &targets.hdr, &targets.normal, &params);
        let mut pass = viewport_pass(cx, encoder, "neutral resolve", colour, timestamps);
        let Some(v) = self.views.as_ref() else {
            return;
        };
        pass.set_pipeline(&v.resolve);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Draw the normal or census view over `target` inside the world viewport, from `picture`
    /// and, where the frame was re-shaded, `targets`; `census` is the frame's replay census.
    #[allow(clippy::too_many_arguments)]
    pub fn debug_view(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        view: DebugView,
        picture: &wgpu::TextureView,
        targets: Option<&ReshadeTargets>,
        census: &Census,
        target: &wgpu::TextureView,
    ) {
        if !matches!(view, DebugView::Normals | DebugView::Census) {
            return;
        }
        #[allow(clippy::cast_precision_loss)] // draw counts
        let counts = [
            census.legacy as f32,
            census.reshaded as f32,
            census.reframed as f32,
            (census.skipped_sky_dome
                + census.skipped_covered_by_feed
                + census.skipped_replaced_by_pass) as f32,
        ];
        let flags = [f32::from(u8::from(targets.is_some())), 0.0, 0.0, 0.0];
        let params = floats(&[viewport_of(cx), flags, counts].concat());
        let normals = targets.map_or(picture, |t| &t.normal);
        let bind = self.bind(cx, picture, normals, &params);
        let mut pass = viewport_pass(cx, encoder, "re-shade view", target, None);
        let Some(v) = self.views.as_ref() else {
            return;
        };
        pass.set_pipeline(if view == DebugView::Normals {
            &v.normals
        } else {
            &v.census
        });
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Begin a re-shade pass over `targets` and `depth`, drawing over what they hold.
pub fn reshade_pass<'e>(
    cx: &mut SidecarContext<'_>,
    encoder: &'e mut wgpu::CommandEncoder,
    targets: &ReshadeTargets,
    depth: &wgpu::TextureView,
    label: &str,
    timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> wgpu::RenderPass<'e> {
    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[load(&targets.hdr), load(&targets.normal)],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: timestamps,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    cx.count_pass();
    pass
}

/// A copy of the re-shaded picture as it stands, for a pass that reads the picture while it draws
/// into it: the pass reads the copy and writes the picture.
///
/// # Errors
/// [`HifiError::Budget`] when the copy does not fit the video memory budget;
/// [`HifiError::NotReady`] when there is no re-shaded picture this frame.
pub fn hdr_copy(
    cx: &SidecarContext<'_>,
    resources: &mut Resources,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<wgpu::TextureView, HifiError> {
    let (width, height) = cx.surface_size();
    let copy = resources.texture(
        cx.device(),
        ResourceName::ScratchHdr,
        TextureSpec {
            width,
            height,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        },
    )?;
    let (Some(Resource::Texture(from, _)), Some(Resource::Texture(to, _))) = (
        resources.get(ResourceName::SceneHdr),
        resources.get(ResourceName::ScratchHdr),
    ) else {
        return Err(HifiError::NotReady("no re-shaded picture to copy"));
    };
    encoder.copy_texture_to_texture(
        from.as_image_copy(),
        to.as_image_copy(),
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    Ok(copy)
}
