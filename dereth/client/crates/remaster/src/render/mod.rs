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
mod batch;
#[cfg(all(feature = "dlss", windows))]
pub mod dlss;
pub mod drawn;
pub mod far_pictures;
pub mod forest;
pub mod gi;
pub mod grade;
pub mod grass;
pub mod impostor;
pub mod instances;
pub mod intent;
pub mod lamps;
pub mod landscape;
pub mod light;
pub mod lod;
#[cfg(target_os = "macos")]
pub mod metalfx;
pub mod planar;
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
pub mod skin_pass;
pub mod skins;
pub mod sky;
pub mod sky_probe;
pub mod terrain;
pub mod upscale;
pub(crate) mod waiting;
pub mod wake;
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
use crate::assets::{AssetSource, AssetTables, MeshKey};
use crate::scene::{BlockId, FrameDelta, SceneDelta};
use crate::shaders::{spirv, Entry};

use dereth_gfx_passes::frame::{push_words, PerFrame};
/// The G-buffer's targets and their formats: the shared layout every pass reads.
pub use dereth_gfx_passes::gbuffer::{Targets, ALBEDO, COLOR, DEPTH, LIGHT, MATERIAL, NORMAL};

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
    /// The lists the device sorts, each drawn by one pipeline whatever its items' sides: the
    /// cut-outs' edges, and the blended and added surfaces where the device blends both in one.
    edge_any: Pipeline,
    forward_any: Option<Pipeline>,
    /// The sky: opaque, blended, additive; each with front faces culled away, then both drawn.
    /// No depth: the sky is behind everything.
    sky: [[Pipeline; 2]; 3],
    composite: Pipeline,
    cull: Pipeline,
    /// The openings' depth stamps.
    stamp: Pipeline,
    /// Seen from outdoors, the openings clearing what the outdoors drew behind them.
    stamp_behind: Pipeline,
    /// The frame's depth taking the rooms' seen from outdoors, where they drew.
    stamp_rooms: Pipeline,
}

/// The meshes the far trees' pictures are made of that a frame holds, at most.
const FAR_MESHES_A_FRAME: usize = 16;

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
    /// How much of the sky reaches each pixel, a float a pixel at the render size, when the frame
    /// drew the occlusion.
    pub occlusion: Option<Vec<u8>>,
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
    /// Whether the last frame drew water.
    pub last_water: bool,
    /// Bodies posed, their vertices, and the time posing them took, in nanoseconds (an
    /// instrument's count, read from the wall clock).
    pub skin_poses: u64,
    pub skin_vertices: u64,
    pub skin_nanos: u64,
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
    /// The objects drawn with an art pack's body, and their bodies.
    skins: skins::Skins,
    skin_pass: skin_pass::SkinPass,
    /// Whether an art pack's light and grade are laid over the frame.
    pack_look: bool,
    /// Which of the landscape's additions are drawn.
    landscape: self::landscape::LandscapeSettings,
    /// The water pass.
    water: self::water::WaterPass,
    /// The sky all round the eye, which the water reflects.
    sky_probe: self::sky_probe::SkyProbe,
    /// The scene mirrored in the water, which the water reflects where it is not traced.
    planar: self::planar::PlanarReflection,
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
    /// Where the device's draws take no count from a buffer: each bucket's items grouped by
    /// their mesh, one instanced draw a group, and the item each instance draws.
    batched: PerFrame,
    remap: PerFrame,
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
    /// The scene's changes waiting for their pictures.
    waiting: waiting::Waiting,
    /// The last frame's view and projection, and whether it drew the sun's shadows.
    view_projection: glam::Mat4,
    eye: Vec3,
    shadowed: bool,
    /// The lamps' pass.
    pub(crate) lamps: lamps::LampsPass,
    pub(crate) lod: lod::Lod,
    /// The blocks' parts chosen and listed on the device.
    pub(crate) instances: instances::DeviceParts,
    pub(crate) impostors: impostor::ImpostorPass,
    /// The art pack's far trees and forest cover.
    pub(crate) forest: forest::ForestPass,
    /// The grass field.
    pub(crate) grass: grass::GrassPass,
    /// The probe of where instances stand and how far they are lifted, as the G-buffer places
    /// them.
    lift_probe: dereth_gfx::Pipeline,
    /// The instances of the trees the last frame drew as meshes, with lifts onto the ground.
    mesh_trees: Vec<u32>,
    /// How long the last frame's lists of what it drew were, to make this frame's that long at
    /// once.
    drawn_sizes: DrawnSizes,
    /// The last frame's outdoor edges and blended surfaces as the processor drew them, item by
    /// item, and which of the two lists the device sorted instead: for the checks.
    blended: [Vec<u32>; 2],
    blended_sorted: [bool; 2],
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

/// One indexed indirect draw command on `backend`, appended to `out`: `indices` indices from
/// `first_index`, once, the draw's item `item`.
pub(crate) fn command_into(
    backend: dereth_gfx::Backend,
    indices: u32,
    first_index: u32,
    item: u32,
    out: &mut Vec<u8>,
) {
    backend.write_indexed_command(indices, 1, first_index, 0, item, out);
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
            vertex: dereth_gfx_passes::shaders::fullscreen(),
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
        let stamp_behind = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster openings from outdoors",
            vertex: shader(Entry::StampVsMain),
            fragment: Some(shader(Entry::StampFsBehind)),
            color: &[],
            depth: Some(DepthState {
                format: DEPTH,
                compare: Some(Compare::GreaterEqual),
                write: true,
            }),
            cull: Cull::None,
        })?;
        let stamp_rooms = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster rooms' depth from outdoors",
            vertex: dereth_gfx_passes::shaders::fullscreen(),
            fragment: Some(shader(Entry::StampFsRooms)),
            color: &[],
            depth: Some(DepthState {
                format: DEPTH,
                compare: Some(Compare::Always),
                write: true,
            }),
            cull: Cull::None,
        })?;
        Ok(Self {
            gbuffer,
            ground,
            resolve_vertex: scene.resolve_vertex,
            resolve_pixel: scene.resolve_pixel,
            forward: scene.forward,
            edge: scene.edge,
            edge_any: scene.edge_any,
            forward_any: scene.forward_any,
            sky: scene.sky,
            composite,
            cull,
            stamp,
            stamp_behind,
            stamp_rooms,
        })
    }

    /// These pipelines with `scene`'s drawing into the scene's colors.
    fn with_scene(self, scene: &ScenePipelines) -> Self {
        Self {
            resolve_vertex: scene.resolve_vertex,
            resolve_pixel: scene.resolve_pixel,
            forward: scene.forward,
            edge: scene.edge,
            edge_any: scene.edge_any,
            forward_any: scene.forward_any,
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
    edge_any: Pipeline,
    forward_any: Option<Pipeline>,
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
                vertex: dereth_gfx_passes::shaders::fullscreen(),
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
        let edge_any = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster cut-out edges, sorted",
            vertex: shader(Entry::GbufferVsMain),
            fragment: Some(shader(Entry::GbufferFsEdgeAny)),
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
            cull: Cull::None,
        })?;
        // A blended surface's alpha comes with its color and one less it goes with what is drawn,
        // as the blended pipeline has them; an added one's color comes whole and all of what is
        // drawn stays, as the added pipeline has them.
        let forward_any = if d.caps().dual_source_blend {
            Some(d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster forward, sorted",
                vertex: shader(Entry::GbufferVsMain),
                fragment: Some(shader(Entry::GbufferFsForwardAny)),
                color: &[ColorTarget {
                    format,
                    blend: Some(Blend {
                        src: Factor::Src1Alpha,
                        dst: Factor::OneMinusSrc1Color,
                    }),
                    write_alpha: false,
                }],
                depth: Some(DepthState {
                    format: DEPTH,
                    compare: Some(Compare::LessEqual),
                    write: false,
                }),
                cull: Cull::None,
            })?)
        } else {
            None
        };
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
            edge_any,
            forward_any,
            sky,
        })
    }
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
        let grass = grass::GrassPass::new(&mut gfx)?;
        let instances = instances::DeviceParts::new(&mut gfx)?;
        let skin_pass = skin_pass::SkinPass::new(&mut gfx)?;
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
            // Read back by the checks.
            usage: BufferUsage::STORAGE | BufferUsage::INDIRECT | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })?;
        let counts = gfx.create_buffer(&BufferDesc {
            label: "remaster counts",
            size: 16,
            usage: BufferUsage::STORAGE
                | BufferUsage::INDIRECT
                | BufferUsage::TRANSFER_DST
                | BufferUsage::TRANSFER_SRC,
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
            batched: per_frame(&mut gfx, "remaster batched", 1 << 18, BufferUsage::INDIRECT)?,
            remap: per_frame(&mut gfx, "remaster remap", 1 << 18, BufferUsage::default())?,
            ao: self::ao::AoPass::new(&mut gfx)?,
            packs: crate::assets::pack::PackSwaps::default(),
            skins: skins::Skins::default(),
            skin_pass,
            pack_look: false,
            landscape: self::landscape::LandscapeSettings::default(),
            water: self::water::WaterPass::new(&mut gfx)?,
            sky_probe: self::sky_probe::SkyProbe::new(),
            planar: self::planar::PlanarReflection::default(),
            weather: self::weather::WeatherPass::new(&mut gfx)?,
            rt: self::rt::RtPass::new(&mut gfx)?,
            rt_kept: false,
            gi: self::gi::GiPass::new(&mut gfx)?,
            rt_shadow: self::rt_shadow::RtShadowPass::new(&mut gfx, shader(Entry::RtShadowFsMain))?,
            rt_ao: self::rt_ao::RtAoPass::new(
                &mut gfx,
                shader(Entry::RtAoFsRays),
                shader(Entry::RtAoFsFilter),
            )?,
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
            waiting: waiting::Waiting::default(),
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
            instances,
            impostors,
            forest,
            grass,
            lift_probe,
            mesh_trees: Vec::new(),
            drawn_sizes: DrawnSizes::default(),
            blended: Default::default(),
            blended_sorted: [false; 2],
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
            self.atmosphere.shared.tables_drawn,
            self.atmosphere.shared.views_drawn,
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
        let changed = self.intents.set(layer, look);
        self.take_look_tables();
        changed
    }

    /// Every layer's look at once, from the next frame on.
    pub fn set_looks(&mut self, looks: crate::look::LookStack) {
        self.intents.set_all(looks);
        self.take_look_tables();
    }

    /// The grade's tables and what each terrain type grows, as the looks now say.
    fn take_look_tables(&mut self) {
        let looks = self.intents.looks();
        self.post.set_grade_tables(grade::tables_with(looks));
        self.drawn_ground.grass_kinds = grass::kinds_with(looks);
        self.grass.set_kinds(self.drawn_ground.grass_kinds);
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

    /// Whether skinned bodies are posed on the device (on by default) or on the processor.
    pub fn set_skin_gpu(&mut self, on: bool) {
        self.skins.gpu = on;
    }

    /// Whether a skinned body still dying when its object leaves is drawn on to the end of its
    /// death (on by default), for an instrument showing what that adds.
    pub fn set_skin_ghosts(&mut self, on: bool) {
        self.skins.ghosting = on;
    }

    /// What each object's body last played of its own clips ([`skins::Skins::playing`]).
    #[must_use]
    pub fn skin_playing(
        &self,
    ) -> Vec<(
        crate::scene::ObjectKey,
        Option<crate::assets::skin::Slot>,
        f32,
        bool,
    )> {
        self.skins.playing()
    }

    /// The objects drawn as ghosts, and the corpses hidden under them ([`skins::Skins::ghosts`]).
    #[must_use]
    pub fn skin_ghosts(&self) -> (Vec<crate::scene::ObjectKey>, Vec<crate::scene::ObjectKey>) {
        self.skins.ghosts()
    }

    /// Each skinned body drawn, by the id of its object's mesh.
    #[must_use]
    pub fn skin_bodies(
        &self,
    ) -> std::collections::BTreeMap<u32, std::sync::Arc<crate::assets::skin::SkinModel>> {
        self.skins.meshes()
    }

    /// Whether the motion pass is given skinned bodies' last places, vertex by vertex (on by
    /// default; off, a body moves in the motion pass only as its anchor part does).
    pub fn set_skin_motion(&mut self, on: bool) {
        self.skins.motion = on;
        if !on {
            self.scene.previous.clear();
        }
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

    /// How far from the eye the last frame's ground mesh is drawn from its `levels` finest levels of
    /// detail, meters: the grid's vertices within it are no farther apart than its level
    /// `levels - 1`'s. `None` while no frame drew the ground.
    #[must_use]
    pub fn drawn_levels_reach(&self, levels: u32) -> Option<f32> {
        let ground = self.last_drawn.as_ref()?;
        Some(drawn::reach(levels.saturating_sub(1), ground.knobs.detail))
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
    /// carry lifts onto the ground drawn over retail's: those the processor chose, then those
    /// the device chose, read back after it has finished.
    pub fn mesh_trees(&mut self) -> Vec<u32> {
        let mut out = self.mesh_trees.clone();
        if !self.lod.settings().on {
            return out;
        }
        for s in self.part_states() {
            if !s.drawn || s.share >= 1.0 {
                continue;
            }
            let Some(p) = self
                .scene
                .blocks
                .get(&s.block)
                .and_then(|b| b.parts.get(s.part as usize))
            else {
                continue;
            };
            let tree = p
                .degrade
                .is_some_and(|r| self.lod.record_is_tree(s.block, r));
            if p.lifts != [0.0; 2] && tree {
                out.push(p.instance);
            }
        }
        out
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
    /// how far each was lifted onto the ground drawn over retail's: the processor's cards, then
    /// the device's, read back after it has finished.
    pub fn impostor_bases(&mut self) -> Vec<(Vec3, f32)> {
        let mut out = self.impostors.bases().to_vec();
        for (card, p, offset) in self.device_cards() {
            let Some(row) = self.impostors.atlas_row(card.gfxobj) else {
                continue;
            };
            let base = p.transform.w_axis.truncate() + offset;
            let lift = card.centre.z - (p.transform.transform_point3(row.centre) + offset).z;
            out.push((base + Vec3::new(0.0, 0.0, lift), lift));
        }
        out
    }

    /// The last frame's impostor cards: the processor's, then the device's, read back after it
    /// has finished.
    pub fn impostor_cards(&mut self) -> Vec<impostor::DrawnCard> {
        let mut out = self.impostors.drawn().to_vec();
        out.extend(self.device_cards().into_iter().map(|(c, _, _)| c));
        out
    }

    /// The cards the device drew last frame, each with its tree's part and its block's offset
    /// from that frame's origin.
    fn device_cards(&mut self) -> Vec<(impostor::DrawnCard, scene::PartItems, Vec3)> {
        let cards = self.instances.read_cards(&mut self.gfx).unwrap_or_default();
        let origin = self.instances.origin();
        let mut out = Vec::with_capacity(cards.len());
        for c in cards {
            let Some(p) = self
                .scene
                .blocks
                .get(&c.block)
                .and_then(|b| b.parts.get(c.part as usize))
            else {
                continue;
            };
            let Some(gfxobj) = p.degrade.and_then(|r| self.lod.first_level(c.block, r)) else {
                continue;
            };
            let (x, y) = (i32::from(c.block >> 8), i32::from(c.block & 0xFF));
            #[allow(clippy::cast_precision_loss)] // a few hundred blocks
            let offset = Vec3::new(
                (x - origin[0]) as f32 * 192.0,
                (y - origin[1]) as f32 * 192.0,
                0.0,
            );
            let [r0, r1, ..] = c.rows;
            out.push((
                impostor::DrawnCard {
                    gfxobj,
                    centre: Vec3::new(r0[0], r0[1], r0[2]),
                    radius: r0[3],
                    turn: dereth_primitives::num::math::atan2f(r1[1], r1[0]),
                    share: r1[3],
                },
                p.clone(),
                offset,
            ));
        }
        out
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
        let occlusion = self.ao.read_occlusion(&mut self.gfx).ok().flatten();
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
            occlusion,
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
        mut frame: FrameDelta,
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
        self.grass.set_pebbles(src.pebble_grounds());
        // The ground switched between its composites and its splats: every held block's cells
        // are written again once the tables have handed over the other form.
        let switched = self.tables.splat() != self.terrain.splat;
        if switched {
            self.tables.set_splat(self.terrain.splat, src);
        }
        // An art pack's models in place of the placements it has them for, as the blocks come;
        // its bodies in place of the parts of the objects it has them for, as the objects come.
        let deltas = self.packs.swap(deltas, &mut |id| src.pack_model(id));
        // A body still dying when its object leaves is drawn on to the end of its death.
        let deltas = self
            .skins
            .haunt(deltas, &frame, self.packs.settings.characters);
        let deltas = if self.packs.settings.characters {
            self.skins.swap(deltas, &mut |setup, palettes, who| {
                src.pack_skin(setup, palettes, who)
            })
        } else {
            self.skins.forget(&deltas);
            deltas
        };
        let mut with_skins = skins::WithSkins {
            src,
            meshes: self.skins.meshes(),
        };
        let src: &mut dyn AssetSource = &mut with_skins;
        // The changes whose pictures are ready, in order; the rest wait for theirs.
        let deltas = self
            .waiting
            .admit(deltas, &mut frame.levels, &mut self.tables, src);
        self.changes.waiting = self.waiting.len();
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
        // The meshes the far trees' pictures are made of, held once their own pictures are ready.
        for id in self
            .forest
            .wanted_meshes()
            .iter()
            .take(FAR_MESHES_A_FRAME)
            .copied()
            .collect::<Vec<_>>()
        {
            let ready = self
                .tables
                .impostor_pictures_wanted(id, src)
                .into_iter()
                .all(|(k, clip)| self.tables.holds_texture(&k) || src.prepare(&k, clip));
            if ready {
                self.tables.impostor_in(id, src);
            }
        }
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
            if let SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) = d {
                let packs = &self.packs;
                self.scene.mark_cover(b.block, &|m| packs.is_cover(m));
            }
        }
        if let Err(e) = self.instances.take(
            &mut self.gfx,
            (&self.scene, &self.tables),
            &mut self.lod,
            &mut self.impostors,
            &deltas,
        ) {
            tracing::warn!(target: "dereth::remaster", "the parts were not taken: {e}");
        }
        for l in &frame.levels {
            self.scene.levels(l);
            if let Err(e) = self
                .instances
                .levels(&mut self.gfx, &self.scene, &self.lod, l)
            {
                tracing::warn!(target: "dereth::remaster", "retail's levels were not taken: {e}");
            }
        }
        // The ghosts stand in the frame, and the corpses under them are hidden.
        self.skins.with_ghosts(&mut frame);
        // Each body posed by its object's parts this frame, written over its mesh: on the device,
        // where here only each body's joints are worked out (and its data sent the first time);
        // or here, every vertex.
        let started = std::time::Instant::now();
        let vertices = self.skins.posed;
        let (bodies, posed) = if self.skins.gpu {
            let jobs = self.skins.joints(&frame);
            let motion = self.skins.motion;
            let r = self
                .skin_pass
                .receive(&mut self.gfx, &mut self.scene, &jobs, motion);
            if let Err(e) = r {
                tracing::warn!(target: "dereth::remaster", "the bodies were not posed: {e}");
            }
            (jobs.len(), Vec::new())
        } else {
            let posed = self.skins.pose(&frame);
            (posed.len(), posed)
        };
        self.stats.skin_nanos += u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.stats.skin_poses += bodies as u64;
        self.stats.skin_vertices += self.skins.posed - vertices;
        for (id, rows) in posed {
            if self.skins.motion {
                let last = self.skins.last_rows(id, &rows);
                let r = self
                    .scene
                    .write_mesh_previous(&mut self.gfx, MeshKey::GfxObj(id), &last);
                if let Err(e) = r {
                    tracing::warn!(target: "dereth::remaster", "a body's last places were not kept: {e}");
                }
            }
            let r = self
                .scene
                .write_mesh_vertices(&mut self.gfx, MeshKey::GfxObj(id), &rows);
            if let Err(e) = r {
                tracing::warn!(target: "dereth::remaster", "a body was not posed: {e}");
            }
        }
        // What a body dying now is, should its object leave; nothing the bodies hold is drawn.
        self.skins.remember(&frame);
        self.skins.hide_held(&mut frame);
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
///
/// Where the viewer stands outdoors and the walk drew rooms, seen through their openings, the
/// rooms' opaque surfaces are drawn after the outdoors too ([`Self::opaque_inside`]): retail
/// clears the depth inside each opening it looks through before it draws the rooms beyond, so
/// the ground a building stands on never covers its rooms. Their blended surfaces and edges are
/// drawn with the outdoors', against the depth the rooms leave.
#[derive(Debug, Default)]
pub(crate) struct Drawn {
    /// Whether the viewer stands inside with the outdoors drawn.
    pub inside: bool,
    /// Whether the viewer stands outdoors and looks into rooms through their openings.
    pub looking_in: bool,
    /// The G-buffer's items, and for each of the outdoors' its block part's place in the walk:
    /// block by block as the frame drew them, each taking a place for each of its parts (`NONE`
    /// for any other item).
    pub opaque: Vec<u32>,
    pub opaque_walk: Vec<u32>,
    pub opaque_inside: Vec<u32>,
    /// The forward pass's items, far to near, with their buckets.
    pub forward: Vec<(f32, u32, Bucket)>,
    pub forward_inside: Vec<(f32, u32, Bucket)>,
    /// The blended cut-outs' items, far to near, whose partly transparent texels the edge pass
    /// draws.
    pub edges: Vec<(f32, u32)>,
    pub edges_inside: Vec<(f32, u32)>,
    /// The openings' triangles' corners, in the frame's space, when the viewer stands inside or
    /// looks in.
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
    /// The degrading block parts drawn by the levels chosen here that fade, turn or are trees
    /// drawn as meshes: each one's block, its place among the block's parts, what it draws and
    /// the level retail chose.
    pub lod: Vec<(BlockId, usize, lod::PartDraw, u8)>,
    /// What the levels chosen here did, over every degrading block part drawn.
    pub lod_tally: lod::LodTally,
    /// The trees drawn as impostors.
    pub impostors: Vec<impostor::ImpostorDraw>,
}

/// How long a frame's longest lists of what it drew are ([`Drawn`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DrawnSizes {
    opaque: usize,
    forward: usize,
    edges: usize,
    lod: usize,
}

impl Drawn {
    /// How long its longest lists are.
    pub(crate) fn sizes(&self) -> DrawnSizes {
        DrawnSizes {
            opaque: self.opaque.len(),
            forward: self.forward.len(),
            edges: self.edges.len(),
            lod: self.lod.len(),
        }
    }
}

impl Renderer {
    /// What the frame draws: the walk's own set, block by block, room by room and object part by
    /// object part. Its longest lists start as long as the last frame's were.
    pub(crate) fn drawn(&self, frame: &FrameDelta) -> Drawn {
        let sizes = self.drawn_sizes;
        let mut out = Drawn {
            inside: frame.landscape && !frame.light.outdoor,
            looking_in: frame.landscape && frame.light.outdoor && !frame.drawn_cells.is_empty(),
            // Set 0 is every room's: every dynamic light.
            sets: vec![light::room_light_set(
                &frame.light.lights,
                frame.light.statics,
            )],
            opaque: Vec::with_capacity(sizes.opaque),
            opaque_walk: Vec::with_capacity(sizes.opaque),
            forward: Vec::with_capacity(sizes.forward),
            edges: Vec::with_capacity(sizes.edges),
            lod: Vec::with_capacity(sizes.lod),
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
        // stands inside, and the opaque ones when the viewer looks in from outdoors. `fading`: a
        // material is fading them, so whatever they are they blend. `walk`: their block part's
        // place in the walk, `NONE` for any other item.
        let push_items = |out: &mut Drawn,
                          r: &std::ops::Range<u32>,
                          centre: Vec3,
                          (interior, fading): (bool, bool),
                          walk: u32| {
            let late = interior && out.inside;
            let late_opaque = interior && (out.inside || out.looking_in);
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
                        if late_opaque {
                            out.opaque_inside.push(i);
                        } else {
                            out.opaque.push(i);
                            out.opaque_walk.push(walk);
                        }
                        if edge {
                            if late {
                                out.edges_inside.push((far, i));
                            } else {
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
            // The next block's first place in the walk.
            let mut places = 0u32;
            for id in &frame.drawn_blocks {
                let Some(b) = self.scene.blocks.get(id) else {
                    continue;
                };
                let walk = places;
                places = places.saturating_add(u32::try_from(b.parts.len()).unwrap_or(u32::MAX));
                if let Some(g) = self.scene.ground.get(id) {
                    out.ground
                        .push([g.vertices.start, g.cells.start, g.side, b.slot]);
                }
                let o = offset(*id);
                let lod = self.lod.block(*id);
                let lod_on = self.lod.chooses(*id);
                // The parts the device chooses for are left to it.
                let cpu = self.instances.cpu_parts(*id);
                for j in 0..cpu.map_or(b.parts.len(), <[u32]>::len) {
                    let k = cpu.map_or(j, |c| c[j] as usize);
                    let Some(p) = b.parts.get(k) else {
                        continue;
                    };
                    let c = p.transform.transform_point3(Vec3::from(p.sphere.0)) + o;
                    let retail = u8::try_from(level_of(b, p)).unwrap_or(u8::MAX);
                    let d = lod.part(p, retail, true);
                    for l in std::iter::once(d.level).chain(d.fade.map(|f| f.0)) {
                        if let Some(r) = p.levels.get(usize::from(l)) {
                            push_items(
                                &mut out,
                                r,
                                c,
                                (false, false),
                                walk.saturating_add(u32::try_from(k).unwrap_or(u32::MAX)),
                            );
                        }
                    }
                    if let (Some(record), true) = (p.degrade, lod_on || d.turned.is_some()) {
                        out.lod_tally.add(lod_on, &d, retail);
                        // Only a part fading or turned has rows to write, and only a tree drawn
                        // as a mesh and lifted is one of the frame's mesh trees.
                        let tree = d.impostor < 1.0 && p.lifts != [0.0; 2] && lod.is_tree(record);
                        if d.fade.is_some() || d.turned.is_some() || tree {
                            out.lod.push((*id, k, d, retail));
                        }
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
                push_items(&mut out, &r.walls.levels[0], c, (true, false), NONE);
                // Its openings to the outdoors, stamped as a fan of triangles each.
                if out.inside || out.looking_in {
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
                    push_items(&mut out, r, centre, (true, false), NONE);
                }
            }
        }
        // The objects' parts the walk drew, posed as the frame has them.
        let poses: std::collections::HashMap<crate::scene::ObjectKey, &crate::scene::ObjectPose> =
            if frame.drawn_parts.is_empty() {
                std::collections::HashMap::new()
            } else {
                let mut poses = std::collections::HashMap::with_capacity(frame.objects.len());
                for p in &frame.objects {
                    // The first pose of a key, as a search from the front finds it.
                    poses.entry(p.key).or_insert(p);
                }
                poses
            };
        for dp in &frame.drawn_parts {
            let (Some(o), Some(pose)) =
                (self.scene.objects.get(&dp.key), poses.get(&dp.key).copied())
            else {
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
                push_items(&mut out, r, c, (!dp.outdoors, fade > 0.0), NONE);
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
                // With the atmosphere drawn, the sky's own objects stand over the eye: as far off
                // as the sky, they move neither as the eye walks nor when the frame's block
                // changes. Retail stands them at the corner of the viewer's block.
                let mut at = o.frame;
                if self.atmosphere.settings.atmosphere && o.properties & 4 == 0 {
                    at.w_axis.x = frame.camera.eye.x;
                    at.w_axis.y = frame.camera.eye.y;
                }
                out.instances.push((
                    s.instance,
                    GpuScene::instance_bytes_scrolled(
                        at,
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
    /// Whether the blocks' parts are chosen and listed on the device (where it takes a draw count
    /// from a buffer), or every part on the processor: every held block's parts are written again
    /// now.
    pub fn set_parts_on_device(&mut self, on: bool) {
        self.instances.set_on(on);
        if let Err(e) = self.instances.take(
            &mut self.gfx,
            (&self.scene, &self.tables),
            &mut self.lod,
            &mut self.impostors,
            &[],
        ) {
            tracing::warn!(target: "dereth::remaster", "the parts were not taken: {e}");
        }
    }

    /// Draw the frame received into targets of its own, `width` by `height`, and wait for the
    /// device to finish it: for the checks, with no window. Whether a frame was drawn.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn draw_offscreen(&mut self, width: u32, height: u32) -> Result<bool, dereth_gfx::Error> {
        let area = dereth_gfx::Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let Some(plan) = self.prepare(area, 1.0)? else {
            return Ok(false);
        };
        let make = |gfx: &mut Device, format, usage| {
            gfx.create_texture(&TextureDesc {
                label: "remaster offscreen",
                width,
                height,
                mips: 1,
                format,
                usage,
            })
        };
        let color = make(
            &mut self.gfx,
            Format::Bgra8Unorm,
            TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
        )?;
        let depth = make(
            &mut self.gfx,
            DEPTH,
            TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
        )?;
        let mut r = self.gfx.begin()?;
        for b in &plan.retired {
            r.destroy_buffer(*b);
        }
        let recorded = plan.record(
            &mut r,
            color,
            depth,
            dereth_gfx::Use::ColorTarget.state(),
            dereth_gfx::Use::DepthTarget.state(),
        );
        let ticket = r.submit()?;
        recorded?;
        self.gfx.wait(ticket)?;
        self.gfx.destroy_texture(color);
        self.gfx.destroy_texture(depth);
        Ok(true)
    }

    /// How many of the blocks' parts the device chooses and lists.
    #[must_use]
    pub fn parts_on_device(&self) -> usize {
        self.instances.count()
    }

    /// The device's last choice for each of the parts it chooses, after it has finished: for the
    /// checks. Empty when the device refuses.
    pub fn part_states(&mut self) -> Vec<instances::PartState> {
        self.instances
            .read_states(&mut self.gfx)
            .unwrap_or_default()
    }

    /// The draw items each cascade's list of the device's parts held when last drawn again: for
    /// the checks.
    pub fn part_casters(&mut self) -> [Vec<u32>; 4] {
        self.instances
            .read_casters(&mut self.gfx)
            .unwrap_or_default()
    }

    /// What the last frame's choice of levels did, over every degrading part of the blocks it
    /// drew: the processor's choices, and the device's read back after it has finished.
    pub fn lod_stats(&mut self) -> lod::LodStats {
        let mut stats = self.lod.stats();
        if !self.lod.settings().on {
            return stats;
        }
        for s in self.part_states() {
            let Some(b) = self.scene.blocks.get(&s.block) else {
                continue;
            };
            let Some(record) = b.parts.get(s.part as usize).and_then(|p| p.degrade) else {
                continue;
            };
            if !s.drawn {
                continue;
            }
            let retail = b.levels.get(record as usize).copied().unwrap_or(0);
            stats.parts += 1;
            stats.fading += u32::from(s.fade > 0.0);
            stats.finer += u32::from(s.level < retail);
            stats.coarser += u32::from(s.level > retail && s.share == 0.0);
            stats.impostors += u32::from(s.share > 0.0);
        }
        stats
    }

    /// The last frame's outdoor cut-outs' edges and blended surfaces, the items in the order they
    /// were drawn, whether the processor sorted them or the device did, after the device has
    /// finished: for the checks.
    pub fn blended_items(&mut self) -> [Vec<u32>; 2] {
        let mut out = self.blended.clone();
        if self.blended_sorted.contains(&true) {
            let sorted = self.instances.read_lists(&mut self.gfx).unwrap_or_default();
            for (list, (own, theirs)) in out.iter_mut().zip(sorted).enumerate() {
                if self.blended_sorted[list] {
                    *own = theirs;
                }
            }
        }
        out
    }

    /// Whether the device sorts the outdoor blended surfaces' list as well as the cut-outs'
    /// edges', where it chooses the parts.
    #[must_use]
    pub fn sorts_blended(&self) -> bool {
        self.instances.sorts_forward()
    }

    /// The draw items of the G-buffer's two outdoor buckets, front faces culled away and both
    /// drawn, in the order the last frame drew them, after the device has finished: for the
    /// checks.
    pub fn culled_items(&mut self) -> [Vec<u32>; 2] {
        let words = self.gfx.indexed_command_words();
        let read = |gfx: &mut Device, b: Buffer, offset: u64, len: u64| {
            instances::read_back(gfx, b, offset, len).unwrap_or_default()
        };
        let counts = read(&mut self.gfx, self.counts, 0, 16);
        let mut out: [Vec<u32>; 2] = Default::default();
        for (bucket, list) in out.iter_mut().enumerate() {
            let n = counts
                .get(bucket * 4..bucket * 4 + 4)
                .map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
                .min(BUCKET_CAPACITY);
            if n == 0 {
                continue;
            }
            let bytes = words as usize * 4;
            let at = bucket as u64 * u64::from(BUCKET_CAPACITY) * bytes as u64;
            let all = read(
                &mut self.gfx,
                self.commands,
                at,
                u64::from(n) * bytes as u64,
            );
            // The item is each layout's last word.
            for c in all.chunks_exact(bytes) {
                let w = &c[c.len() - 4..];
                list.push(u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
            }
        }
        out
    }

    /// What the processor chooses for each of block `block`'s parts this frame (outdoors), with the
    /// level retail chose: the frame's own choice where the processor makes it. For the checks.
    #[must_use]
    pub fn part_choices(&self, block: BlockId) -> Vec<lod::PartDraw> {
        let Some(b) = self.scene.blocks.get(&block) else {
            return Vec::new();
        };
        let lod = self.lod.block(block);
        b.parts
            .iter()
            .map(|p| {
                let retail = p
                    .degrade
                    .and_then(|d| b.levels.get(d as usize))
                    .copied()
                    .unwrap_or(0);
                lod.part(p, retail, true)
            })
            .collect()
    }

    /// The draw items of each level of block `block`'s part `part`. For the checks.
    #[must_use]
    pub fn part_items(&self, block: BlockId, part: u32) -> Vec<std::ops::Range<u32>> {
        self.scene
            .blocks
            .get(&block)
            .and_then(|b| b.parts.get(part as usize))
            .map(|p| p.levels.clone())
            .unwrap_or_default()
    }

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
