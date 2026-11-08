//! The renderer: the retained scene on the device, and each frame's passes.
//!
//! A frame is, in order:
//! 1. the cull: the frame's visible draw items made into indirect draw commands;
//! 2. the G-buffer: the ground, then the meshes, by indirect-count draws, each surface's own
//!    color, normal, light (per vertex, or what the per-pixel resolve needs) and class;
//! 3. a resolve to the world's colors: retail's light per vertex, or per pixel;
//! 4. the forward pass: translucent and additive surfaces, far to near, over the resolved world;
//! 5. the composite: the world's colors and depth into the frame's target, inside the world's
//!    rectangle, for the interface to draw over.
//!
//! Every pipeline is made when the renderer is ([`Renderer::new`]); none is made while drawing.

pub mod ao;
#[cfg(feature = "dlss")]
pub mod dlss;
pub mod drawn;
pub mod forest;
pub mod gi;
pub mod grade;
pub mod impostor;
pub mod intent;
pub mod lamps;
pub mod landscape;
pub mod light;
pub mod lod;
#[cfg(target_os = "macos")]
pub mod metalfx;
pub mod pool;
pub mod post;
mod record;
mod ripples;
pub mod rt;
pub mod rt_ao;
pub mod rt_shadow;
pub mod scene;
pub mod settings;
pub mod shadow;
pub mod sky;
pub mod terrain;
pub mod upscale;
pub mod water;
pub mod weather;

pub use record::{color_back, depth_back, Plan, SceneRecorded};

use dereth_gfx::{
    Blend, Buffer, BufferDesc, BufferUsage, ColorTarget, Compare, ComputeDesc, Cull, DepthState,
    Device, Factor, Format, GraphicsDesc, Location, Pipeline, Sampler, SamplerDesc, Shader,
    Texture, TextureDesc, TextureUsage,
};
use glam::{Vec3, Vec4Swizzles};

use self::light::{LightMode, LightSet};
use self::scene::{Bucket, GpuScene, NONE};
use crate::assets::{AssetSource, AssetTables};
use crate::scene::{BlockId, FrameDelta, SceneDelta};
use crate::shaders::{spirv, Entry};

/// The G-buffer's targets and the world's colors, at the world's size.
#[derive(Debug, Clone, Copy)]
pub struct Targets {
    pub width: u32,
    pub height: u32,
    pub albedo: Texture,
    pub normal: Texture,
    pub light: Texture,
    pub material: Texture,
    pub depth: Texture,
    pub color: Texture,
    /// The outdoor surfaces' depth, kept when the viewer stands inside, before the interior
    /// clears it.
    pub outdoor_depth: Texture,
    /// Each one's index in the table, in the order above.
    pub index: [u32; 7],
}

/// The pipelines, every one made at the start.
#[derive(Debug, Clone, Copy)]
struct Pipelines {
    gbuffer: [Pipeline; 2],
    ground: Pipeline,
    resolve_vertex: Pipeline,
    resolve_pixel: Pipeline,
    /// Blended, then additive; each with front faces culled away, then both drawn.
    forward: [[Pipeline; 2]; 2],
    /// The cut-outs' edges: front faces culled away, then both drawn.
    edge: [Pipeline; 2],
    /// The sky: opaque, blended, additive; each with front faces culled away, then both drawn.
    /// No depth: the sky is behind everything.
    sky: [[Pipeline; 2]; 3],
    composite: Pipeline,
    cull: Pipeline,
    /// The openings' depth stamps.
    stamp: Pipeline,
}

/// The formats of the G-buffer's targets.
pub const ALBEDO: Format = Format::Rgba16Float;
pub const NORMAL: Format = Format::Rg16Snorm;
pub const LIGHT: Format = Format::Rgba16Float;
pub const MATERIAL: Format = Format::Rgba8Unorm;
pub const DEPTH: Format = Format::D32Float;
pub const COLOR: Format = Format::Rgba8Unorm;

/// The commands one cull bucket holds at most.
const BUCKET_CAPACITY: u32 = 1 << 17;

/// The last frame's world as [`Renderer::read_back`] reads it: its size, its corner on the
/// frame's target, and its colors, material bytes and depth, each four bytes a pixel. With an
/// upscaler on, the colors are at the display's size and all the rest at the render size.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBack {
    pub width: u32,
    pub height: u32,
    /// The size the G-buffer's pictures were drawn at.
    pub render_size: (u32, u32),
    pub x: u32,
    pub y: u32,
    pub color: Vec<u8>,
    pub material: Vec<u8>,
    pub depth: Vec<u8>,
    /// The G-buffer's albedo, four half floats a pixel: the color, and the roughness in alpha.
    pub albedo: Vec<u8>,
    /// The G-buffer's normals, two signed 16-bit numbers a pixel: the octahedral encoding.
    pub normal: Vec<u8>,
    /// The frame's view and projection together, and its eye.
    pub view_projection: glam::Mat4,
    pub eye: Vec3,
    /// The sun's share at each pixel, a byte a pixel, when the frame drew the sun's shadows.
    pub sun_visibility: Option<Vec<u8>>,
}

/// One buffer rewritten every frame, grown when a frame needs more.
#[derive(Debug)]
struct PerFrame {
    buffer: Buffer,
    index: u32,
    size: u64,
    usage: BufferUsage,
    label: &'static str,
}

impl PerFrame {
    fn new(
        d: &mut Device,
        label: &'static str,
        size: u64,
        usage: BufferUsage,
    ) -> Result<Self, dereth_gfx::Error> {
        let usage = usage | BufferUsage::STORAGE;
        let buffer = d.create_buffer(&BufferDesc {
            label,
            size,
            usage,
            location: Location::Gpu,
        })?;
        let index = d.bind_buffer(buffer)?;
        Ok(Self {
            buffer,
            index,
            size,
            usage,
            label,
        })
    }

    /// Write `bytes` at the start, growing the buffer first when they do not fit.
    fn write(&mut self, d: &mut Device, bytes: &[u8]) -> Result<(), dereth_gfx::Error> {
        let need = (bytes.len() as u64).max(16);
        if need > self.size {
            let size = need.next_power_of_two();
            let bigger = d.create_buffer(&BufferDesc {
                label: self.label,
                size,
                usage: self.usage,
                location: Location::Gpu,
            })?;
            d.destroy_buffer(std::mem::replace(&mut self.buffer, bigger));
            self.index = d.bind_buffer(self.buffer)?;
            self.size = size;
        }
        if bytes.is_empty() {
            return Ok(());
        }
        d.write_buffer(self.buffer, 0, bytes)
    }
}

/// What the renderer counts, cumulative.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Frames drawn.
    pub frames: u64,
    /// Bytes written to the device for the frames' own state: the camera, the lights, the
    /// visible items, the objects' poses.
    pub frame_bytes: u64,
    /// Draw items drawn by the last frame, in the G-buffer and in the forward pass.
    pub last_items: u32,
    pub last_forward: u32,
    /// The particles' draw items of the last frame.
    pub last_particle_items: u32,
}

/// The renderer: the layer's device, the retained scene and each frame's passes.
#[derive(Debug)]
pub struct Renderer {
    pub(crate) gfx: Device,
    pub(crate) tables: AssetTables,
    pub(crate) scene: GpuScene,
    pipelines: Pipelines,
    /// The samplers retail's request table names, by request, at the mip bias of the frame.
    samplers: Vec<(Sampler, u32)>,
    /// Every set of them made so far, by its mip bias in thousandths: a render scale is one of a
    /// few, and a frame still on the device may read a set the next frame leaves.
    sampler_sets: Vec<(i32, Vec<(Sampler, u32)>)>,
    /// What the drawn ground's pictures are read through, repeating and held, and those of each mip
    /// bias asked for so far.
    ground_samplers: [u32; 2],
    ground_sampler_sets: Vec<(i32, [u32; 2])>,
    targets: Option<Targets>,
    /// The detail textures by their picture's id, made on first use, and the ones the last frame
    /// named, with their tilings: the buildings' and the rooms'.
    details: std::collections::BTreeMap<u32, (Texture, u32)>,
    detail: [Option<(u32, f32)>; 2],
    /// Whether the sky's meshes are held.
    sky_held: bool,
    /// The world's corner on the frame's target, as the last frame had it.
    area: (u32, u32),
    frame: Option<FrameDelta>,
    /// Where the last frame handed over drew each of its particles, the frame's space.
    particles: Vec<Vec3>,
    mode: LightMode,
    /// The looks laid over retail's light, and the last frame's intent.
    intents: self::intent::Intents,
    /// The ambient occlusion pass.
    ao: self::ao::AoPass,
    /// The art pack's swaps made.
    packs: crate::assets::pack::PackSwaps,
    /// Whether an art pack's light and grade are laid over the frame.
    pack_look: bool,
    /// Which of the landscape's additions are drawn.
    landscape: self::landscape::LandscapeSettings,
    /// The water pass.
    water: self::water::WaterPass,
    /// The rain, the snow, the wet and the puddles' share.
    weather: self::weather::WeatherPass,
    /// The scene's acceleration structures, where the device traces rays, and whether they are
    /// kept without a pass of the frame's own asking.
    rt: self::rt::RtPass,
    /// The sun's share and the occlusion traced through the acceleration structures.
    rt_shadow: self::rt_shadow::RtShadowPass,
    rt_ao: self::rt_ao::RtAoPass,
    rt_kept: bool,
    /// The bounced light.
    gi: self::gi::GiPass,
    /// The post passes.
    post: self::post::PostPass,
    /// The upscaler.
    upscale: self::upscale::UpscalePass,
    // The frames' own buffers.
    constants: PerFrame,
    /// The frame's constants as the sky is seen, with its far plane further out.
    sky_constants: PerFrame,
    lights: PerFrame,
    sets: PerFrame,
    offsets: PerFrame,
    visible: PerFrame,
    forward: PerFrame,
    ground_drawn: PerFrame,
    overrides: PerFrame,
    /// The openings' triangles' corners.
    stamps: PerFrame,
    commands: Buffer,
    counts: Buffer,
    commands_index: u32,
    counts_index: u32,
    stats: RenderStats,
    /// The sun's shadows.
    pub(crate) shadow: shadow::ShadowPass,
    /// The ground drawn as a grid of patches.
    pub(crate) drawn_ground: drawn::DrawnPass,
    /// Whether the ground's colors far off have been asked of the dats.
    ground_colors: bool,
    /// The last frame's drawn ground, for the checks of its heights.
    last_drawn: Option<crate::scene::DrawnGround>,
    /// The sky's atmosphere.
    pub(crate) atmosphere: sky::SkyPass,
    /// How the ground's cells are drawn, and what the scene's changes counted.
    terrain: terrain::TerrainSettings,
    changes: terrain::ChangeCounts,
    /// The last frame's view and projection, and whether it drew the sun's shadows.
    view_projection: glam::Mat4,
    eye: Vec3,
    shadowed: bool,
    /// The lamps' pass.
    pub(crate) lamps: lamps::LampsPass,
    pub(crate) lod: lod::Lod,
    pub(crate) impostors: impostor::ImpostorPass,
    /// The art pack's far trees and forest cover.
    pub(crate) forest: forest::ForestPass,
    /// The probe of where instances stand and how far they are lifted, as the G-buffer places
    /// them.
    lift_probe: dereth_gfx::Pipeline,
    /// The instances of the trees the last frame drew as meshes, with lifts onto the ground.
    mesh_trees: Vec<u32>,
}

/// A raw shader from the embedded blobs.
fn shader(e: Entry) -> Shader<'static> {
    Shader {
        spirv: spirv(e),
        dxil: e.dxil_bytes(),
        entry: e.name(),
    }
}

/// The bytes one indexed indirect draw command takes on `gfx`'s backend.
pub(crate) fn command_bytes(gfx: &dereth_gfx::Device) -> u32 {
    gfx.indexed_command_words() * 4
}

/// One indexed indirect draw command on `backend`: `indices` indices from `first_index`, once,
/// the draw's item `item`.
pub(crate) fn command(
    backend: dereth_gfx::Backend,
    indices: u32,
    first_index: u32,
    item: u32,
) -> Vec<u8> {
    push_words(&backend.indexed_command(indices, 1, first_index, 0, item))
}

fn gbuffer_targets() -> [ColorTarget; 4] {
    [ALBEDO, NORMAL, LIGHT, MATERIAL].map(|format| ColorTarget {
        format,
        blend: None,
        write_alpha: true,
    })
}

impl Pipelines {
    fn new(d: &mut Device, frame_depth: Format) -> Result<Self, dereth_gfx::Error> {
        let depth = Some(DepthState {
            format: DEPTH,
            compare: Some(Compare::LessEqual),
            write: true,
        });
        let targets = gbuffer_targets();
        let gbuffer = [Cull::Front, Cull::None].map(|cull| {
            d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster g-buffer",
                vertex: shader(Entry::GbufferVsMain),
                fragment: Some(shader(Entry::GbufferFsMain)),
                color: &targets,
                depth,
                cull,
            })
        });
        let [g0, g1] = gbuffer;
        let gbuffer = [g0?, g1?];
        let ground = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster ground",
            vertex: shader(Entry::TerrainVsMain),
            fragment: Some(shader(Entry::TerrainFsMain)),
            color: &targets,
            depth,
            cull: Cull::None,
        })?;
        let scene = ScenePipelines::new(d, COLOR)?;
        let composite = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster composite",
            vertex: shader(Entry::FullscreenVsMain),
            fragment: Some(shader(Entry::CompositeFsMain)),
            color: &[ColorTarget {
                format: Format::Bgra8Unorm,
                blend: None,
                write_alpha: false,
            }],
            depth: Some(DepthState {
                format: frame_depth,
                compare: Some(Compare::Always),
                write: true,
            }),
            cull: Cull::None,
        })?;
        let cull = d.create_compute_pipeline(&ComputeDesc {
            label: "remaster cull",
            shader: shader(Entry::CullCsMain),
        })?;
        let stamp = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster opening stamps",
            vertex: shader(Entry::StampVsMain),
            fragment: None,
            color: &[],
            depth,
            cull: Cull::None,
        })?;
        Ok(Self {
            gbuffer,
            ground,
            resolve_vertex: scene.resolve_vertex,
            resolve_pixel: scene.resolve_pixel,
            forward: scene.forward,
            edge: scene.edge,
            sky: scene.sky,
            composite,
            cull,
            stamp,
        })
    }

    /// These pipelines with `scene`'s drawing into the scene's colors.
    fn with_scene(self, scene: &ScenePipelines) -> Self {
        Self {
            resolve_vertex: scene.resolve_vertex,
            resolve_pixel: scene.resolve_pixel,
            forward: scene.forward,
            edge: scene.edge,
            sky: scene.sky,
            ..self
        }
    }
}

/// The pipelines that draw into the scene's colors: the resolves, the forward pass, the cut-outs'
/// edges and the sky. They are made once for the colors in display values and once for the
/// linear colors the post passes take.
#[derive(Debug, Clone, Copy)]
struct ScenePipelines {
    resolve_vertex: Pipeline,
    resolve_pixel: Pipeline,
    forward: [[Pipeline; 2]; 2],
    edge: [Pipeline; 2],
    sky: [[Pipeline; 2]; 3],
}

impl ScenePipelines {
    /// Every one, drawing into a color target of `format`.
    fn new(d: &mut Device, format: Format) -> Result<Self, dereth_gfx::Error> {
        let color = [ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        }];
        let resolve = |d: &mut Device, e: Entry| {
            d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster resolve",
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(e)),
                color: &color,
                depth: None,
                cull: Cull::None,
            })
        };
        let resolve_vertex = resolve(d, Entry::ResolveFsVertex)?;
        let resolve_pixel = resolve(d, Entry::ResolveFsPixel)?;
        let blends = [
            Blend {
                src: Factor::SrcAlpha,
                dst: Factor::OneMinusSrcAlpha,
            },
            Blend {
                src: Factor::One,
                dst: Factor::One,
            },
        ];
        let mut forward = [[resolve_vertex; 2]; 2];
        for (b, blend) in blends.into_iter().enumerate() {
            for (c, cull) in [Cull::Front, Cull::None].into_iter().enumerate() {
                forward[b][c] = d.create_graphics_pipeline(&GraphicsDesc {
                    label: "remaster forward",
                    vertex: shader(Entry::GbufferVsMain),
                    fragment: Some(shader(Entry::GbufferFsForward)),
                    color: &[ColorTarget {
                        format,
                        blend: Some(blend),
                        write_alpha: false,
                    }],
                    depth: Some(DepthState {
                        format: DEPTH,
                        compare: Some(Compare::LessEqual),
                        write: false,
                    }),
                    cull,
                })?;
            }
        }
        let mut edge = [resolve_vertex; 2];
        for (c, cull) in [Cull::Front, Cull::None].into_iter().enumerate() {
            edge[c] = d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster cut-out edges",
                vertex: shader(Entry::GbufferVsMain),
                fragment: Some(shader(Entry::GbufferFsEdge)),
                color: &[ColorTarget {
                    format,
                    blend: Some(blends[0]),
                    write_alpha: false,
                }],
                depth: Some(DepthState {
                    format: DEPTH,
                    compare: Some(Compare::LessEqual),
                    write: true,
                }),
                cull,
            })?;
        }
        let mut sky = [[resolve_vertex; 2]; 3];
        for (b, blend) in [None, Some(blends[0]), Some(blends[1])]
            .into_iter()
            .enumerate()
        {
            for (c, cull) in [Cull::Front, Cull::None].into_iter().enumerate() {
                sky[b][c] = d.create_graphics_pipeline(&GraphicsDesc {
                    label: "remaster sky",
                    vertex: shader(Entry::GbufferVsMain),
                    fragment: Some(shader(Entry::GbufferFsForward)),
                    color: &[ColorTarget {
                        format,
                        blend,
                        write_alpha: false,
                    }],
                    depth: None,
                    cull,
                })?;
            }
        }
        Ok(Self {
            resolve_vertex,
            resolve_pixel,
            forward,
            edge,
            sky,
        })
    }
}

/// Little-endian words, as the shaders' push constants read them.
fn push_words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

impl Renderer {
    /// A renderer on `gfx`, drawing into frames whose depth target is `frame_depth`: every
    /// pipeline made, the scene's pools and the frames' buffers made, nothing held.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(mut gfx: Device, frame_depth: Format) -> Result<Self, dereth_gfx::Error> {
        let pipelines = Pipelines::new(&mut gfx, frame_depth)?;
        let lamps = lamps::LampsPass::new(&mut gfx)?;
        let impostors = impostor::ImpostorPass::new(&mut gfx)?;
        let forest = forest::ForestPass::new(&mut gfx)?;
        let lift_probe = gfx.create_compute_pipeline(&dereth_gfx::ComputeDesc {
            label: "remaster lift probe",
            shader: shader(Entry::LiftCsBases),
        })?;
        let scene = GpuScene::new(&mut gfx)?;
        let samplers = make_samplers(&mut gfx, 0.0, dereth_render::sampler::SAMPLER_COUNT)?;
        let ground_samplers = make_ground_samplers(&mut gfx, 0.0)?;
        let commands = gfx.create_buffer(&BufferDesc {
            label: "remaster commands",
            size: u64::from(scene::CULLED_BUCKETS * BUCKET_CAPACITY)
                * u64::from(command_bytes(&gfx)),
            usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
            location: Location::Gpu,
        })?;
        let counts = gfx.create_buffer(&BufferDesc {
            label: "remaster counts",
            size: 16,
            usage: BufferUsage::STORAGE | BufferUsage::INDIRECT | BufferUsage::TRANSFER_DST,
            location: Location::Gpu,
        })?;
        let commands_index = gfx.bind_buffer(commands)?;
        let counts_index = gfx.bind_buffer(counts)?;
        let per_frame =
            |gfx: &mut Device, label, size, usage| PerFrame::new(gfx, label, size, usage);
        Ok(Self {
            constants: per_frame(&mut gfx, "remaster frame", 512, BufferUsage::default())?,
            sky_constants: per_frame(&mut gfx, "remaster sky frame", 512, BufferUsage::default())?,
            lights: per_frame(&mut gfx, "remaster lights", 1 << 14, BufferUsage::default())?,
            sets: per_frame(
                &mut gfx,
                "remaster light sets",
                1 << 16,
                BufferUsage::default(),
            )?,
            offsets: per_frame(
                &mut gfx,
                "remaster block offsets",
                1 << 16,
                BufferUsage::default(),
            )?,
            visible: per_frame(
                &mut gfx,
                "remaster visible",
                1 << 20,
                BufferUsage::default(),
            )?,
            forward: per_frame(&mut gfx, "remaster forward", 1 << 18, BufferUsage::INDIRECT)?,
            ao: self::ao::AoPass::new(&mut gfx)?,
            packs: crate::assets::pack::PackSwaps::default(),
            pack_look: false,
            landscape: self::landscape::LandscapeSettings::default(),
            water: self::water::WaterPass::new(&mut gfx)?,
            weather: self::weather::WeatherPass::new(&mut gfx)?,
            rt: self::rt::RtPass::new(&mut gfx)?,
            rt_kept: false,
            gi: self::gi::GiPass::new(&mut gfx)?,
            rt_shadow: self::rt_shadow::RtShadowPass::new(&mut gfx)?,
            rt_ao: self::rt_ao::RtAoPass::new(&mut gfx)?,
            post: self::post::PostPass::new(&mut gfx)?,
            upscale: self::upscale::UpscalePass::new(&mut gfx)?,
            ground_drawn: per_frame(
                &mut gfx,
                "remaster ground drawn",
                1 << 16,
                BufferUsage::default(),
            )?,
            overrides: per_frame(
                &mut gfx,
                "remaster overrides",
                1 << 16,
                BufferUsage::default(),
            )?,
            stamps: per_frame(
                &mut gfx,
                "remaster opening stamps",
                1 << 14,
                BufferUsage::default(),
            )?,
            commands,
            counts,
            commands_index,
            counts_index,
            shadow: shadow::ShadowPass::new(&mut gfx)?,
            drawn_ground: {
                let depth = |format| {
                    Some(DepthState {
                        format,
                        compare: Some(Compare::LessEqual),
                        write: true,
                    })
                };
                drawn::DrawnPass::new(&mut gfx, &gbuffer_targets(), depth(DEPTH), depth(DEPTH))?
            },
            ground_colors: false,
            last_drawn: None,
            atmosphere: {
                let linear = post::scene_format(&gfx);
                sky::SkyPass::new(&mut gfx, COLOR, linear)?
            },
            terrain: terrain::TerrainSettings::default(),
            changes: terrain::ChangeCounts::default(),
            view_projection: glam::Mat4::IDENTITY,
            eye: Vec3::ZERO,
            shadowed: false,
            gfx,
            tables: AssetTables::new(),
            scene,
            pipelines,
            sampler_sets: vec![(0, samplers.clone())],
            ground_samplers,
            ground_sampler_sets: vec![(0, ground_samplers)],
            samplers,
            targets: None,
            details: std::collections::BTreeMap::new(),
            detail: [None; 2],
            sky_held: false,
            area: (0, 0),
            frame: None,
            particles: Vec::new(),
            mode: LightMode::Vertex,
            intents: self::intent::Intents::new(),
            stats: RenderStats::default(),
            lamps,
            lod: lod::Lod::default(),
            impostors,
            forest,
            lift_probe,
            mesh_trees: Vec::new(),
        })
    }

    /// How the light is worked out from the next frame on.
    pub fn set_mode(&mut self, mode: LightMode) {
        self.mode = mode;
    }

    /// How the light is worked out.
    #[must_use]
    pub fn mode(&self) -> LightMode {
        self.mode
    }

    /// The sun's shadows' settings, from the next frame on. They are drawn when the light is
    /// worked out per pixel.
    pub fn set_shadows(&mut self, s: shadow::ShadowSettings) {
        self.shadow.settings = s;
    }

    /// The sky's atmosphere's settings, from the next frame on. It is drawn when the light is
    /// worked out per pixel.
    pub fn set_sky(&mut self, s: sky::SkySettings) {
        self.atmosphere.settings = s;
    }

    /// The sky's atmosphere: its settings, and how many times its air's tables and its view
    /// table were drawn.
    #[must_use]
    pub fn sky(&self) -> (sky::SkySettings, u64, u64) {
        (
            self.atmosphere.settings,
            self.atmosphere.tables_drawn,
            self.atmosphere.views_drawn,
        )
    }

    /// The sun's shadows: their settings and what they counted.
    #[must_use]
    pub fn shadows(&self) -> (shadow::ShadowSettings, shadow::ShadowStats) {
        (self.shadow.settings, self.shadow.stats)
    }

    /// Set one layer's look, or take it away, from the next frame on. Returns whether the looks
    /// changed.
    pub fn set_look(
        &mut self,
        layer: crate::look::Layer,
        look: Option<crate::look::LookToml>,
    ) -> bool {
        self.intents.set(layer, look)
    }

    /// Every layer's look at once, from the next frame on.
    pub fn set_looks(&mut self, looks: crate::look::LookStack) {
        self.intents.set_all(looks);
    }

    /// The looks laid over retail's light.
    #[must_use]
    pub fn looks(&self) -> &crate::look::LookStack {
        self.intents.looks()
    }

    /// The last frame's intent: its light inputs with the look laid over them.
    #[must_use]
    pub fn intent(&self) -> &self::intent::Intent {
        self.intents.last()
    }

    /// What the renderer has counted.
    #[must_use]
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// The layer's device, for its counts.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.gfx
    }

    /// The asset tables.
    #[must_use]
    pub fn tables(&self) -> &AssetTables {
        &self.tables
    }

    /// The bytes the retained scene has written to the device.
    #[must_use]
    pub fn scene_bytes(&self) -> u64 {
        self.scene.uploaded
    }

    /// The bytes the retained scene holds on the device: its pools' buffers and its textures.
    #[must_use]
    pub fn resident_bytes(&self) -> u64 {
        self.scene.resident_bytes()
    }

    /// The bytes the ground drawn as a grid of patches holds on the device: the world's vertices
    /// and the near offset.
    #[must_use]
    pub fn ground_bytes(&self) -> u64 {
        self.drawn_ground.bytes()
    }

    /// Each of the retained scene's pools: its name, the bytes in use and the bytes held.
    #[must_use]
    pub fn pool_bytes(&self) -> Vec<(&'static str, u64, u64)> {
        self.scene.pool_bytes()
    }

    /// Each pass's time on the device in the last finished frame, milliseconds, where the device
    /// writes timestamps.
    pub fn pass_timings(&mut self) -> Vec<(&'static str, f64)> {
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut timings: Vec<(&'static str, f64)> = self
            .gfx
            .timings()
            .into_iter()
            .map(|t| (t.label, t.ms))
            .collect();
        #[cfg(target_os = "macos")]
        {
            let scaler = metalfx::last_ms();
            if scaler > 0.0 {
                timings.push(("metalfx", scaler));
            }
        }
        timings
    }

    /// The drawn height at `points` (x and y in the frame's space, and the distance from the eye)
    /// as the device works it out for `ground` (the last frame's, when `None`), without the fine
    /// relief: for the checks that the ground is drawn where the scene stands what is on it. Empty
    /// when there is no ground to work it out for.
    pub fn drawn_heights(
        &mut self,
        ground: Option<&crate::scene::DrawnGround>,
        points: &[[f32; 3]],
    ) -> Vec<f32> {
        let Some(ground) = ground.or(self.last_drawn.as_ref()).cloned() else {
            return Vec::new();
        };
        let g = ground.global;
        let points: Vec<[f32; 3]> = points
            .iter()
            .map(|p| [p[0] + g.x, p[1] + g.y, p[2]])
            .collect();
        let points = points.as_slice();
        let frame = FrameDelta {
            landscape: true,
            drawn: Some(ground),
            ..FrameDelta::default()
        };
        let settings = terrain::TerrainSettings {
            grid: true,
            drawn: true,
            horizon: true,
            ..self.terrain
        };
        self.drawn_ground
            .heights_at(&mut self.gfx, &self.scene, &frame, settings, points)
            .unwrap_or_default()
    }

    /// The height the last frame's ground mesh is drawn at, at each of `points` (x and y in the
    /// frame's space), as its triangles lay it between their vertices, without the fine relief:
    /// for the checks that what stands on the ground meets it. `None` where the last frame drew
    /// no patch over a point.
    pub fn drawn_mesh_heights(&mut self, points: &[glam::Vec2]) -> Vec<Option<f32>> {
        let Some(ground) = self.last_drawn.clone() else {
            return vec![None; points.len()];
        };
        let g = ground.global;
        let points: Vec<glam::Vec2> = points.iter().map(|p| *p + g).collect();
        let frame = FrameDelta {
            landscape: true,
            drawn: Some(ground),
            ..FrameDelta::default()
        };
        let settings = terrain::TerrainSettings {
            grid: true,
            drawn: true,
            horizon: true,
            ..self.terrain
        };
        self.drawn_ground
            .mesh_heights(&mut self.gfx, &self.scene, &frame, settings, &points)
            .unwrap_or_else(|_| vec![None; points.len()])
    }

    /// Where the last frame drew each particle it was handed (the frame's space), in the order
    /// handed.
    #[must_use]
    pub fn particle_origins(&self) -> Vec<Vec3> {
        self.particles.clone()
    }

    /// Draw the water with no ripples' picture: for the check that its ripples are that
    /// picture's.
    pub fn set_water_without_picture(&mut self, without: bool) {
        self.water.without_picture = without;
    }

    /// Turn every ground cell `quarters` more quarter turns than its own, from the next frame: for
    /// the checks that a cell's pictures and their maps turn with it. Retail's grounds that blend
    /// layers never turn a cell.
    pub fn set_turned_cells(&mut self, quarters: u8) {
        self.scene.turn = quarters & 3;
        if let Err(e) = self.scene.rewrite_ground_cells(&mut self.gfx, &self.tables) {
            tracing::warn!(target: "dereth::remaster", "the ground's cells were not written: {e}");
        }
    }

    /// Retail's height at each of `points` (x and y in the frame's space), over the world's
    /// vertices the last frame's ground was drawn from: for the checks. Empty while it drew none.
    #[must_use]
    pub fn retail_heights(&self, points: &[glam::Vec2]) -> Vec<f32> {
        let Some(ground) = self.last_drawn.as_ref() else {
            return Vec::new();
        };
        points
            .iter()
            .map(|p| drawn::retail_height(&ground.world.heights, *p + ground.global))
            .collect()
    }

    /// The instances of the trees the last frame drew as meshes (not wholly as impostors) that
    /// carry lifts onto the ground drawn over retail's.
    #[must_use]
    pub fn mesh_trees(&self) -> &[u32] {
        &self.mesh_trees
    }

    /// Where each of `instances` stands (the frame's space) and how far it is drawn above it, as
    /// the G-buffer's vertex shader places it over the last frame's constants; and how far the
    /// impostor cards' arithmetic lifts it. Empty when the device refuses the probe.
    pub fn instance_bases(&mut self, instances: &[u32]) -> Vec<(Vec3, f32, f32)> {
        let gpu = self.probe_bases(instances).unwrap_or_default();
        let frame = FrameDelta {
            drawn: self.last_drawn.clone(),
            camera: crate::scene::Camera {
                eye: self.eye,
                ..crate::scene::Camera::default()
            },
            ..FrameDelta::default()
        };
        let lifts: std::collections::HashMap<u32, [f32; 2]> = self
            .scene
            .blocks
            .values()
            .flat_map(|b| b.parts.iter().map(|p| (p.instance, p.lifts)))
            .collect();
        gpu.into_iter()
            .zip(instances)
            .map(|((origin, lift), i)| {
                let cards = lifts
                    .get(i)
                    .map_or(0.0, |l| light::lifted(&frame, *l, origin));
                (origin, lift, cards)
            })
            .collect()
    }

    /// The probe of [`Self::instance_bases`]: each instance's origin and lift, worked out by the
    /// device.
    fn probe_bases(&mut self, instances: &[u32]) -> Result<Vec<(Vec3, f32)>, dereth_gfx::Error> {
        use dereth_gfx::{Touch, Use};
        let d = &mut self.gfx;
        let count = u32::try_from(instances.len()).unwrap_or(0);
        let mut input = push_words(&[count, 0, 0, 0]);
        for i in instances {
            input.extend(i.to_le_bytes());
        }
        let list = d.create_buffer(&BufferDesc {
            label: "remaster lift probe instances",
            size: input.len() as u64,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        d.write_buffer(list, 0, &input)?;
        let list_index = d.bind_buffer(list)?;
        let size = u64::from(count.max(1)) * 16;
        let out = d.create_buffer(&BufferDesc {
            label: "remaster lift probe bases",
            size,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })?;
        let out_index = d.bind_buffer(out)?;
        let back = d.create_buffer(&BufferDesc {
            label: "remaster lift probe read-back",
            size,
            usage: BufferUsage::TRANSFER_DST,
            location: Location::Readback,
        })?;
        let words = [
            self.constants.index,
            self.scene.instances.index,
            self.offsets.index,
            list_index,
            out_index,
        ];
        let touches = [
            Touch::Buffer(list, Use::StorageRead),
            Touch::Buffer(out, Use::StorageWrite),
            Touch::Buffer(self.constants.buffer, Use::StorageRead),
            Touch::Buffer(self.scene.instances.buffer, Use::StorageRead),
            Touch::Buffer(self.offsets.buffer, Use::StorageRead),
        ];
        let pipeline = self.lift_probe;
        let mut r = d.begin()?;
        r.compute(&touches, |c| {
            c.set_pipeline(pipeline)?;
            c.push(0, &push_words(&words))?;
            c.dispatch(count.div_ceil(64).max(1), 1, 1);
            Ok(())
        })?;
        r.copy_buffer(out, 0, back, 0, size)?;
        r.touch(&[Touch::Buffer(back, Use::HostRead)])?;
        let ticket = r.submit()?;
        d.wait(ticket)?;
        let bytes = d.read_buffer(back, 0, size)?;
        for b in [list, out, back] {
            d.destroy_buffer(b);
        }
        Ok(bytes
            .as_chunks::<16>()
            .0
            .iter()
            .take(instances.len())
            .map(|w| {
                let f = |k: usize| f32::from_le_bytes([w[k], w[k + 1], w[k + 2], w[k + 3]]);
                (Vec3::new(f(0), f(4), f(8)), f(12))
            })
            .collect())
    }

    /// Where the last frame's impostor cards stand their trees' bases (the frame's space), and
    /// how far each was lifted onto the ground drawn over retail's.
    #[must_use]
    pub fn impostor_bases(&self) -> Vec<(Vec3, f32)> {
        self.impostors.bases().to_vec()
    }

    /// How the ground's cells are drawn, from the next frame the scene hands over.
    pub fn set_terrain(&mut self, s: terrain::TerrainSettings) {
        self.terrain = s;
    }

    /// How the ground's cells are drawn.
    #[must_use]
    pub fn terrain(&self) -> terrain::TerrainSettings {
        self.terrain
    }

    /// What the scene's changes have counted.
    #[must_use]
    pub fn changes(&self) -> terrain::ChangeCounts {
        self.changes
    }

    /// The blocks held.
    #[must_use]
    pub fn blocks(&self) -> usize {
        self.scene.blocks.len()
    }

    /// The blocks whose ground is held.
    #[must_use]
    pub fn grounds(&self) -> usize {
        self.scene.ground.len()
    }

    /// The objects held.
    #[must_use]
    pub fn objects(&self) -> usize {
        self.scene.objects.len()
    }

    /// The meshes held.
    #[must_use]
    pub fn meshes(&self) -> usize {
        self.scene.meshes.len()
    }

    /// The last frame's world, read back after the device has finished it. `None` before a
    /// frame was drawn.
    pub fn read_back(&mut self) -> Option<ReadBack> {
        let t = self.targets?;
        let (x, y) = self.area;
        self.gfx.wait_idle().ok()?;
        let sun_visibility = if self.shadowed {
            self.shadow.read_visibility(&mut self.gfx).ok().flatten()
        } else {
            None
        };
        let (color, (width, height)) = self
            .post
            .upscaled_output()
            .unwrap_or((t.color, (t.width, t.height)));
        Some(ReadBack {
            width,
            height,
            render_size: (t.width, t.height),
            x,
            y,
            color: self.gfx.read_texture(color, 0).ok()?,
            material: self.gfx.read_texture(t.material, 0).ok()?,
            depth: self.gfx.read_texture(t.depth, 0).ok()?,
            albedo: self.gfx.read_texture(t.albedo, 0).ok()?,
            normal: self.gfx.read_texture(t.normal, 0).ok()?,
            view_projection: self.view_projection,
            eye: self.eye,
            sun_visibility,
        })
    }

    /// The world's targets, once a frame has drawn.
    #[must_use]
    pub fn targets(&self) -> Option<Targets> {
        self.targets
    }

    /// Take one frame of the scene: its changes, with their assets read from `src`, then its
    /// state, which the next recording draws. `detail` names the detail pictures the buildings'
    /// shells and the rooms' walls are drawn under, with their tilings, when they are.
    pub fn receive(
        &mut self,
        deltas: Vec<SceneDelta>,
        frame: FrameDelta,
        detail: [Option<(u32, f32)>; 2],
        src: &mut dyn AssetSource,
    ) {
        if !self.sky_held {
            self.tables.sky_in(src);
            self.sky_held = true;
        }
        if !self.ground_colors {
            if let Some(c) = src.ground_colors() {
                self.drawn_ground.colors = c;
                self.ground_colors = true;
            }
        }
        // The ground switched between its composites and its splats: every held block's cells
        // are written again once the tables have handed over the other form.
        let switched = self.tables.splat() != self.terrain.splat;
        if switched {
            self.tables.set_splat(self.terrain.splat, src);
        }
        // An art pack's models in place of the placements it has them for, as the blocks come.
        let deltas = self.packs.swap(deltas, &mut |id| src.pack_model(id));
        for d in &deltas {
            self.tables.apply(d, src);
            match d {
                SceneDelta::BlockIn(_) => self.changes.blocks_in += 1,
                SceneDelta::BlockChanged(_) => self.changes.blocks_changed += 1,
                SceneDelta::GroundChanged(_) => self.changes.grounds_changed += 1,
                _ => {}
            }
        }
        self.lamps.take(&deltas);
        self.lod.take(&deltas, &frame.levels);
        self.tables
            .particles_in(frame.particles.iter().map(|p| p.gfx), src);
        self.scene.upload(&mut self.gfx, &mut self.tables);
        if switched {
            if let Err(e) = self.scene.rewrite_ground_cells(&mut self.gfx, &self.tables) {
                tracing::warn!(target: "dereth::remaster", "the ground's cells were not written: {e}");
            }
        }
        for (id, _) in detail.iter().flatten() {
            if self.details.contains_key(id) {
                continue;
            }
            let key = crate::assets::TextureKey::Dat {
                picture: *id,
                palette: 0,
            };
            if let Some(t) = src.texture(&key, false) {
                match self.detail_texture(&t) {
                    Ok(made) => {
                        self.details.insert(*id, made);
                    }
                    Err(e) => {
                        tracing::warn!(target: "dereth::remaster", "a detail picture was not made: {e}");
                    }
                }
            }
        }
        self.detail = detail;
        for o in &frame.sky {
            if let Err(e) = self.scene.sky_in(&mut self.gfx, o.gfx) {
                tracing::warn!(target: "dereth::remaster", "a sky object was not made: {e}");
            }
        }
        for d in &deltas {
            let r = match d {
                SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) => {
                    self.scene.block_in(&mut self.gfx, &self.tables, b)
                }
                SceneDelta::GroundChanged(g) => {
                    self.scene
                        .ground_changed(&mut self.gfx, &self.tables, g.block, &g.terrain)
                }
                SceneDelta::BlockOut(id) => {
                    self.scene.block_out(*id);
                    Ok(())
                }
                SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) => {
                    self.scene.object_in(&mut self.gfx, &self.tables, o)
                }
                SceneDelta::ObjectRemoved(k) => {
                    self.scene.object_out(*k);
                    Ok(())
                }
            };
            if let Err(e) = r {
                tracing::warn!(target: "dereth::remaster", "the scene could not take a change: {e}");
            }
        }
        for l in &frame.levels {
            self.scene.levels(l);
        }
        self.particles = frame
            .particles
            .iter()
            .map(|p| p.transform.w_axis.truncate())
            .collect();
        self.frame = Some(frame);
    }

    /// The targets for a world of `width` by `height`, made again when its size changes.
    fn targets_for(&mut self, width: u32, height: u32) -> Result<Targets, dereth_gfx::Error> {
        if let Some(t) = self.targets {
            if (t.width, t.height) == (width, height) {
                return Ok(t);
            }
            for x in [
                t.albedo,
                t.normal,
                t.light,
                t.material,
                t.depth,
                t.color,
                t.outdoor_depth,
            ] {
                self.gfx.destroy_texture(x);
            }
        }
        let mut make = |format, usage| {
            self.gfx.create_texture(&TextureDesc {
                label: "remaster target",
                width,
                height,
                mips: 1,
                format,
                usage,
            })
        };
        let target =
            TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC;
        let albedo = make(ALBEDO, target)?;
        let normal = make(NORMAL, target)?;
        let light = make(LIGHT, target)?;
        let material = make(MATERIAL, target)?;
        let depth = make(
            DEPTH,
            TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC,
        )?;
        let color = make(COLOR, target)?;
        let outdoor_depth = make(
            DEPTH,
            TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        )?;
        let mut index = [0; 7];
        for (k, t) in [albedo, normal, light, material, depth, color, outdoor_depth]
            .into_iter()
            .enumerate()
        {
            index[k] = self.gfx.bind_texture(t)?;
        }
        let t = Targets {
            width,
            height,
            albedo,
            normal,
            light,
            material,
            depth,
            color,
            outdoor_depth,
            index,
        };
        self.targets = Some(t);
        Ok(t)
    }

    /// A detail picture on the device, every level.
    fn detail_texture(
        &mut self,
        t: &crate::assets::Texture,
    ) -> Result<(Texture, u32), dereth_gfx::Error> {
        let mips = u32::try_from(t.levels.len()).unwrap_or(1).max(1);
        let tex = self.gfx.create_texture(&TextureDesc {
            label: "remaster detail",
            width: t.width,
            height: t.height,
            mips,
            format: scene::gpu_format(t.format),
            usage: TextureUsage::SAMPLED,
        })?;
        for (k, level) in t.levels.iter().enumerate() {
            self.gfx
                .upload_texture(tex, u32::try_from(k).unwrap_or(0), level)?;
        }
        let index = self.gfx.bind_texture(tex)?;
        Ok((tex, index))
    }

    /// The detail textures' indices and tilings for the push constants: the buildings', then the
    /// rooms'; `NONE` where there is none.
    fn detail_push(&self) -> [u32; 4] {
        let one = |d: Option<(u32, f32)>| {
            d.and_then(|(id, tiling)| self.details.get(&id).map(|(_, i)| [*i, tiling.to_bits()]))
                .unwrap_or([NONE, 1.0f32.to_bits()])
        };
        let [a, b] = [one(self.detail[0]), one(self.detail[1])];
        [a[0], a[1], b[0], b[1]]
    }

    /// The samplers at mip bias `bias` from this frame on, made the first time it is asked for.
    fn use_mip_bias(&mut self, bias: f32) -> Result<(), dereth_gfx::Error> {
        let key = dereth_primitives::num::to_i32((bias * 1000.0).round());
        self.ground_samplers = match self.ground_sampler_sets.iter().find(|(k, _)| *k == key) {
            Some((_, set)) => *set,
            None => {
                let set = make_ground_samplers(&mut self.gfx, bias)?;
                self.ground_sampler_sets.push((key, set));
                set
            }
        };
        if let Some((_, set)) = self.sampler_sets.iter().find(|(k, _)| *k == key) {
            self.samplers.clone_from(set);
            return Ok(());
        }
        // Only the two a frame's surfaces read take the bias.
        let mut set = self.samplers.clone();
        for (slot, made) in set.iter_mut().zip(make_samplers(&mut self.gfx, bias, 2)?) {
            *slot = made;
        }
        self.samplers.clone_from(&set);
        self.sampler_sets.push((key, set));
        Ok(())
    }

    /// The request a subset's texture is sampled by: 0 linear and repeating, 1 linear and held.
    fn sampler(&self, which: usize) -> u32 {
        self.samplers.get(which).map_or(0, |(_, i)| *i)
    }
}

/// The first `count` of the samplers retail's request table names, by request, sharper or softer
/// by `bias` levels.
fn make_samplers(
    gfx: &mut Device,
    bias: f32,
    count: u32,
) -> Result<Vec<(Sampler, u32)>, dereth_gfx::Error> {
    let caps = dereth_render::sampler::FilterCaps {
        min_anisotropic: gfx.caps().max_anisotropy > 1.0,
        mag_anisotropic: gfx.caps().max_anisotropy > 1.0,
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        max_anisotropy: gfx.caps().max_anisotropy.min(16.0) as u32,
    };
    let mut samplers = Vec::new();
    for which in 0..count {
        let s = dereth_render::sampler::describe(
            which,
            dereth_render::sampler::STARTUP_FILTERING,
            false,
            caps,
        );
        let (filter, mip, aniso) = match s.filter {
            dereth_render::sampler::SamplerFilter::Point => {
                (dereth_gfx::Filter::Nearest, dereth_gfx::Filter::Nearest, 1)
            }
            dereth_render::sampler::SamplerFilter::LinearMipPoint => {
                (dereth_gfx::Filter::Linear, dereth_gfx::Filter::Nearest, 1)
            }
            dereth_render::sampler::SamplerFilter::Linear => {
                (dereth_gfx::Filter::Linear, dereth_gfx::Filter::Linear, 1)
            }
            dereth_render::sampler::SamplerFilter::Anisotropic => (
                dereth_gfx::Filter::Linear,
                dereth_gfx::Filter::Linear,
                s.max_anisotropy,
            ),
        };
        let address = |a: dereth_render::sampler::AddressMode| match a {
            dereth_render::sampler::AddressMode::Wrap => dereth_gfx::Address::Repeat,
            dereth_render::sampler::AddressMode::Clamp => dereth_gfx::Address::Clamp,
        };
        let sampler = gfx.create_sampler(&SamplerDesc {
            filter,
            mip,
            address: address(s.address_u),
            address_v: address(s.address_v),
            anisotropy: aniso,
            lod_bias: s.mip_lod_bias + bias,
        })?;
        let index = gfx.bind_sampler(sampler)?;
        samplers.push((sampler, index));
    }
    Ok(samplers)
}

/// How many samples the drawn ground's repeating pictures and its held ones take across a pixel's
/// longest reach. Retail's request table filters evenly: on a steep face, seen square on, a pixel
/// reaches across the fall line much farther into the picture than down it, and an even filter
/// blurs the picture down the fall line by that longer reach, so the material runs into streaks.
const GROUND_ANISOTROPY: u32 = 16;
const GROUND_HELD_ANISOTROPY: u32 = 4;

/// The drawn ground's samplers, repeating and held, sharper or softer by `bias` levels.
fn make_ground_samplers(gfx: &mut Device, bias: f32) -> Result<[u32; 2], dereth_gfx::Error> {
    let mut made = |address, anisotropy| {
        let sampler = gfx.create_sampler(&SamplerDesc {
            filter: dereth_gfx::Filter::Linear,
            mip: dereth_gfx::Filter::Linear,
            address,
            address_v: address,
            anisotropy,
            lod_bias: bias,
        })?;
        gfx.bind_sampler(sampler)
    };
    Ok([
        made(dereth_gfx::Address::Repeat, GROUND_ANISOTROPY)?,
        made(dereth_gfx::Address::Clamp, GROUND_HELD_ANISOTROPY)?,
    ])
}

/// What one frame draws, worked out on the processor from the frame's state.
///
/// Where the viewer stands inside and the outdoors shows through the openings, the frame is drawn
/// as retail draws it: the outdoors first, then the depth cleared and the openings stamped into
/// it at their own depth, then the interior. Each list then has the outdoors' part and the
/// interior's (the `_inside` lists); otherwise everything is in the first.
#[derive(Debug, Default)]
pub(crate) struct Drawn {
    /// Whether the viewer stands inside with the outdoors drawn.
    pub inside: bool,
    /// The G-buffer's items.
    pub opaque: Vec<u32>,
    pub opaque_inside: Vec<u32>,
    /// The forward pass's items, far to near, with their buckets.
    pub forward: Vec<(f32, u32, Bucket)>,
    pub forward_inside: Vec<(f32, u32, Bucket)>,
    /// The blended cut-outs' items, far to near, whose partly transparent texels the edge pass
    /// draws.
    pub edges: Vec<(f32, u32)>,
    pub edges_inside: Vec<(f32, u32)>,
    /// The openings' triangles' corners, in the frame's space, when the viewer stands inside.
    pub stamps: Vec<[f32; 4]>,
    /// The particles' draw items, written this frame from the first of the scene's particle
    /// items: each one's row, bucket and command.
    pub particle_items: Vec<(Vec<u8>, Bucket, scene::ItemDraw)>,
    /// The sky's items, in the order the sky draws them, with their buckets, and how many of the
    /// first are the authored dome's.
    pub sky: Vec<(u32, Bucket)>,
    pub sky_dome_items: usize,
    /// The ground's blocks: first vertex, first cell, cells a side, slot.
    pub ground: Vec<[u32; 4]>,
    /// The light sets.
    pub sets: Vec<LightSet>,
    /// Each instance written this frame: its index and its row.
    pub instances: Vec<(u32, Vec<u8>)>,
    /// The objects' material overrides, a row each.
    pub overrides: Vec<[f32; 4]>,
    /// Each degrading block part drawn by the levels chosen here: its block, its place among the
    /// block's parts, what it draws and the level retail chose.
    pub lod: Vec<(BlockId, usize, lod::PartDraw, u8)>,
    /// The trees drawn as impostors.
    pub impostors: Vec<impostor::ImpostorDraw>,
}

impl Renderer {
    /// What the frame draws: the walk's own set, block by block, room by room and object part by
    /// object part.
    pub(crate) fn drawn(&self, frame: &FrameDelta) -> Drawn {
        let mut out = Drawn {
            inside: frame.landscape && !frame.light.outdoor,
            // Set 0 is every room's: every dynamic light.
            sets: vec![light::room_light_set(
                &frame.light.lights,
                frame.light.statics,
            )],
            ..Drawn::default()
        };
        let eye = frame.camera.eye;
        let offset = |b: BlockId| -> Vec3 {
            let (x, y) = (i32::from(b >> 8), i32::from(b & 0xFF));
            #[allow(clippy::cast_precision_loss)] // a few hundred blocks
            Vec3::new(
                (x - frame.origin[0]) as f32 * 192.0,
                (y - frame.origin[1]) as f32 * 192.0,
                0.0,
            )
        };
        // `interior`: the items are the interior's, drawn after the stamps when the viewer
        // stands inside. `fading`: a material is fading them, so whatever they are they blend.
        let push_items = |out: &mut Drawn,
                          r: &std::ops::Range<u32>,
                          centre: Vec3,
                          interior: bool,
                          fading: bool| {
            let late = interior && out.inside;
            for i in r.clone() {
                let far = centre.distance_squared(eye);
                let bucket = self.scene.buckets.get(i as usize).copied();
                let bucket = match bucket {
                    Some(Bucket::Additive) => Some(Bucket::Additive),
                    Some(_) | None if fading => Some(Bucket::Blended),
                    b => b,
                };
                match bucket {
                    Some(b @ (Bucket::Blended | Bucket::Additive)) => {
                        if late {
                            out.forward_inside.push((far, i, b));
                        } else {
                            out.forward.push((far, i, b));
                        }
                    }
                    _ => {
                        let edge = self
                            .scene
                            .item_draws
                            .get(i as usize)
                            .is_some_and(|d| d.edge);
                        if late {
                            out.opaque_inside.push(i);
                            if edge {
                                out.edges_inside.push((far, i));
                            }
                        } else {
                            out.opaque.push(i);
                            if edge {
                                out.edges.push((far, i));
                            }
                        }
                    }
                }
            }
        };
        let level_of = |b: &scene::BlockGpu, p: &scene::PartItems| -> usize {
            p.degrade
                .and_then(|d| b.levels.get(d as usize))
                .map_or(0, |l| usize::from(*l))
        };
        if frame.landscape {
            for id in &frame.drawn_blocks {
                let Some(b) = self.scene.blocks.get(id) else {
                    continue;
                };
                if let Some(g) = self.scene.ground.get(id) {
                    out.ground
                        .push([g.vertices.start, g.cells.start, g.side, b.slot]);
                }
                let o = offset(*id);
                for (k, p) in b.parts.iter().enumerate() {
                    let c = p.transform.transform_point3(Vec3::from(p.sphere.0)) + o;
                    let retail = u8::try_from(level_of(b, p)).unwrap_or(u8::MAX);
                    let d = self.lod.part(*id, p, retail, true);
                    for l in std::iter::once(d.level).chain(d.fade.map(|f| f.0)) {
                        if let Some(r) = p.levels.get(usize::from(l)) {
                            push_items(&mut out, r, c, false, false);
                        }
                    }
                    if p.degrade.is_some() && (self.lod.settings().on || d.turned.is_some()) {
                        out.lod.push((*id, k, d, retail));
                    }
                    if let (true, Some(g)) = (
                        d.impostor > 0.0,
                        p.degrade.and_then(|r| self.lod.first_level(*id, r)),
                    ) {
                        out.impostors.push(impostor::ImpostorDraw {
                            block: *id,
                            part: k,
                            share: d.impostor,
                            gfxobj: g,
                        });
                    }
                }
            }
        }
        // The rooms the walk drew, their walls and the parts of their statics it drew.
        let mut rooms = std::collections::BTreeMap::new();
        for (id, b) in &self.scene.blocks {
            for r in &b.rooms {
                rooms.insert(r.id, (*id, b, r));
            }
        }
        for cell in &frame.drawn_cells {
            if let Some((id, _, r)) = rooms.get(cell) {
                let o = offset(*id);
                let c = r.walls.transform.w_axis.xyz() + o;
                push_items(&mut out, &r.walls.levels[0], c, true, false);
                // Its openings to the outdoors, stamped as a fan of triangles each.
                if out.inside {
                    for poly in &r.openings {
                        for k in 1..poly.len().saturating_sub(1) {
                            for v in [poly[0], poly[k], poly[k + 1]] {
                                let v = v + o;
                                out.stamps.push([v.x, v.y, v.z, 1.0]);
                            }
                        }
                    }
                }
            }
        }
        for (cell, k) in &frame.drawn_cell_parts {
            let Some((id, b, r)) = rooms.get(cell) else {
                continue;
            };
            let Some(p) = r.statics.get(*k as usize) else {
                continue;
            };
            let o = offset(*id);
            let centre = p.transform.transform_point3(Vec3::from(p.sphere.0)) + o;
            let scale = p
                .transform
                .x_axis
                .length()
                .max(p.transform.y_axis.length())
                .max(p.transform.z_axis.length());
            let set = light::object_light_set(
                &frame.light.lights,
                frame.light.statics,
                centre,
                p.sphere.1 * scale,
            );
            let s = u32::try_from(out.sets.len()).unwrap_or(0);
            out.sets.push(set);
            let retail = u8::try_from(level_of(b, p)).unwrap_or(u8::MAX);
            let d = self.lod.part(*id, p, retail, false);
            out.instances
                .extend(lod::turned_rows(p, &d, |transform, fade| {
                    GpuScene::instance_bytes_faded(
                        transform,
                        p.scale,
                        b.slot,
                        scene::class::INDOOR,
                        s,
                        NONE,
                        fade,
                    )
                }));
            for l in std::iter::once(d.level).chain(d.fade.map(|f| f.0)) {
                if let Some(r) = p.levels.get(usize::from(l)) {
                    push_items(&mut out, r, centre, true, false);
                }
            }
        }
        // The objects' parts the walk drew, posed as the frame has them.
        for dp in &frame.drawn_parts {
            let (Some(o), Some(pose)) = (
                self.scene.objects.get(&dp.key),
                frame.objects.iter().find(|p| p.key == dp.key),
            ) else {
                continue;
            };
            let (Some(p), Some(pp)) = (
                o.parts.get(dp.part as usize),
                pose.parts.get(dp.part as usize),
            ) else {
                continue;
            };
            if pp.hidden {
                continue;
            }
            // A part wholly faded out, as the player's body is with the camera closed in on it:
            // retail blends it at no alpha. Fading, it blends.
            let fade = pp.material.map_or(0.0, |m| m.translucency);
            if fade >= 1.0 {
                continue;
            }
            let (class, set) = if dp.outdoors {
                (scene::class::SUN, 0)
            } else {
                let set = LightSet {
                    lights: dp.lights,
                    count: dp.count.min(8),
                };
                let s = u32::try_from(out.sets.len()).unwrap_or(0);
                out.sets.push(set);
                (scene::class::INDOOR, s)
            };
            let over = pp.material.map_or(NONE, |m| {
                let i = u32::try_from(out.overrides.len()).unwrap_or(0);
                out.overrides
                    .push([m.luminosity, m.diffusion, m.translucency, 1.0]);
                i
            });
            let scale = Vec3::new(
                pp.transform.x_axis.xyz().length(),
                pp.transform.y_axis.xyz().length(),
                pp.transform.z_axis.xyz().length(),
            );
            out.instances.push((
                p.instance,
                GpuScene::instance_bytes(pp.transform, scale, NONE, class, set, over),
            ));
            if let Some(r) = p.levels.get(usize::from(pp.level)) {
                let c = pp.transform.transform_point3(Vec3::from(p.sphere.0));
                push_items(&mut out, r, c, !dp.outdoors, fade > 0.0);
            }
        }
        // The particles, each an instance and its card's draw items of its own, written this
        // frame, lit as the walk lit it and faded as it has faded.
        let mut slots = self.scene.particle_instances.clone();
        let mut next = self.scene.particle_items.start;
        for p in &frame.particles {
            let (Some(mesh), Some(materials)) = (
                self.scene
                    .meshes
                    .get(&crate::assets::MeshKey::GfxObj(p.gfx)),
                self.tables.particle_materials(p.gfx),
            ) else {
                continue;
            };
            let count = u32::try_from(mesh.subsets.len()).unwrap_or(u32::MAX);
            if next.saturating_add(count) > self.scene.particle_items.end {
                break;
            }
            let Some(instance) = slots.next() else {
                break;
            };
            let (class, set) = if p.outdoors || !p.lit {
                (scene::class::SUN, 0)
            } else {
                let s = u32::try_from(out.sets.len()).unwrap_or(0);
                out.sets.push(LightSet {
                    lights: p.lights,
                    count: p.count.min(8),
                });
                (scene::class::INDOOR, s)
            };
            let over = u32::try_from(out.overrides.len()).unwrap_or(0);
            let m = p.material;
            out.overrides
                .push([m.luminosity, m.diffusion, m.translucency, 1.0]);
            let scale = Vec3::new(
                p.transform.x_axis.xyz().length(),
                p.transform.y_axis.xyz().length(),
                p.transform.z_axis.xyz().length(),
            );
            out.instances.push((
                instance,
                GpuScene::instance_bytes(p.transform, scale, NONE, class, set, over),
            ));
            let far = p.transform.w_axis.xyz().distance_squared(eye);
            let late = out.inside && !p.outdoors;
            for (k, s) in mesh.subsets.iter().enumerate() {
                let Some(g) = materials
                    .get(k)
                    .and_then(|key| self.scene.material_index.get(key))
                else {
                    continue;
                };
                // A card is drawn blended, faded by its translucency, or added where its surface
                // adds.
                let bucket = if g.additive {
                    Bucket::Additive
                } else {
                    Bucket::Blended
                };
                out.particle_items.push((
                    GpuScene::item_row(instance, mesh, s, g.index, bucket),
                    bucket,
                    scene::ItemDraw {
                        index_count: s.index_count,
                        first_index: mesh.indices.start + s.first_index,
                        two_sided: s.two_sided,
                        edge: false,
                        ..scene::ItemDraw::default()
                    },
                ));
                if late {
                    out.forward_inside.push((far, next, bucket));
                } else {
                    out.forward.push((far, next, bucket));
                }
                next += 1;
            }
        }
        // The sky's objects drawn before the ground, in their order, each posed as the frame has
        // it and with its own look.
        if frame.landscape {
            for (index, o) in frame.sky.iter().enumerate().filter(|(_, o)| o.before) {
                // The game's own falling weather gives way to the remaster's while it falls.
                if self.weather.leaves_out(o.properties) {
                    continue;
                }
                let Some(s) = self.scene.sky.get(&o.gfx) else {
                    continue;
                };
                if sky::is_dome(index, o) {
                    out.sky_dome_items = s.items.len();
                }
                let over = o.material.map_or(NONE, |m| {
                    let i = u32::try_from(out.overrides.len()).unwrap_or(0);
                    out.overrides
                        .push([m.luminosity, m.diffusion, m.translucency, 1.0]);
                    i
                });
                out.instances.push((
                    s.instance,
                    GpuScene::instance_bytes_scrolled(
                        o.frame,
                        Vec3::ONE,
                        NONE,
                        scene::class::SKY,
                        u32::from(o.fogged),
                        over,
                        o.uv_offset,
                    ),
                ));
                for i in s.items.clone() {
                    let b = self
                        .scene
                        .buckets
                        .get(i as usize)
                        .copied()
                        .unwrap_or(Bucket::Opaque);
                    out.sky.push((i, b));
                }
            }
        }
        // Far to near.
        for list in [&mut out.forward, &mut out.forward_inside] {
            list.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        for list in [&mut out.edges, &mut out.edges_inside] {
            list.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        out
    }
}

impl Renderer {
    /// What of an art pack is drawn, from the next blocks that come on: only models of a kind
    /// switched on are swapped in.
    pub fn set_packs(&mut self, s: crate::assets::pack::PackSettings) {
        self.packs.settings = s;
    }

    /// Whether an art pack's light and grade are laid over the frame, which stands in linear light:
    /// an art pack is drawn.
    pub fn set_pack_look(&mut self, on: bool) {
        self.pack_look = on;
        self.post.set_pack_look(on);
    }

    /// What of an art pack is drawn.
    #[must_use]
    pub fn packs(&self) -> crate::assets::pack::PackSettings {
        self.packs.settings
    }

    /// How far from the eye the art pack says the level whose mesh is `mesh` is drawn (infinite
    /// for a model's last); `None` for any other mesh.
    #[must_use]
    pub fn pack_level_distance(&self, mesh: u32) -> Option<f32> {
        self.packs.distance(mesh)
    }

    /// What of the art pack the renderer holds, against what its swaps drew.
    #[must_use]
    pub fn pack_counts(&self) -> crate::assets::pack::PackCounts {
        self.packs.counts(&self.tables)
    }
}
