//! The wgpu device: a frame of the world drawn to a picture, with pipelines for the terrain's
//! layers, placed objects and the sky, and shaders written from the retail vertex program's
//! fragments.
//!
//! **Depends on** `auberean-draw` (what a frame draws) and `auberean-world` (its light and
//! arithmetic), `wgpu`, and the shared math (`dereth-primitives`). **Used by** the products that draw: the world viewer
//! (`auberean-viewer`) and the desktop client (`auberean-client`), which draws on the device the
//! renderer is made on ([`Renderer::on`]) and shows the picture [`Renderer::draw`] leaves there.
//!
//! The device draws in the order the world is drawn (`auberean_draw::scene::PASSES`): the sky's
//! objects around the viewer (no depth test), the fog veil over them, then the terrain's bottom
//! layers, its blended layers and the far landscape, the objects not seen through, the water, the
//! frills, the objects seen through, the particle systems, the streaks and the sky's late objects;
//! each step of objects farthest from the camera first, each object's draws in their order. At the
//! recommended quality the dynamic objects' volumes of shadow come between the objects that cast
//! none and those that cast them: counted in the stencil where the sight to the nearest surface
//! passes through them, that count's places darkened, the stencil cleared behind. The terrain's
//! shaders are the runtime terrain material's; the objects' are assembled from the vertex
//! program's pieces, chosen per draw by its layer's switches (`shaders/world.wgsl`).
//!
//! Before the frame, when it has them: the sun's shadow map of the near objects (the near terrain
//! is multiplied by it and by the clouds' shadows after its layers, then fogged), and the water's
//! reflection, the same scene from the camera mirrored about the water's plane, which the water's
//! surfaces then show through their ripples.
//!
//! Colors are blended as stored, with no gamma conversion, as the retail device does.
//!
//! The world's frame is kept as the original's device keeps it: its draws write color alone, and
//! its alpha starts at 127 of 255 under everything, changed only by the shores' strips, which
//! write it alone before the water; the draws that blend by the destination's alpha (the water
//! and its reflection) read it. A scene laid over the interface ([`Renderer::draw_laid_over`])
//! keeps in its alpha how much of its picture it covers instead.
//!
//! **Must never** decide what to draw: a frame arrives from `auberean-draw` whole, and this crate
//! only draws it.

// A browser's device holds JavaScript objects, which stay on the thread they were made on: this
// crate's shared views are shared within a thread there.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

use dereth_primitives::num::math;
use std::collections::HashMap;
use std::sync::Arc;

use auberean_draw::cull::Sight;
use auberean_draw::flare::{flare_quads, FlareFade};
use auberean_draw::material::{d3d, DrawLayer, Stage, UV_IDENTITY};
use auberean_draw::picture::Image;
use auberean_draw::scene::{
    in_order, Frame, ObjectBatch, Pass, PointLights, TerrainBatch, Textures, PASSES,
};
use auberean_draw::shadow::{ShadowMap, SHADOW_MAP_SIZE};
use auberean_world::math::Mat4;
use bytemuck::{Pod, Zeroable};

mod cache;
mod pool;
mod targets;
mod work;

pub use work::{Lap, Stages, Work};

/// Why a frame could not be drawn.
#[derive(Debug)]
pub enum GpuError {
    NoAdapter(String),
    NoDevice(String),
    Readback(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter(e) => write!(f, "no graphics adapter: {e}"),
            Self::NoDevice(e) => write!(f, "no device: {e}"),
            Self::Readback(e) => write!(f, "reading the picture back failed: {e}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// A drawn picture: `width * height` texels of red, green, blue, alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniforms {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    fog: [f32; 4],
    fog_color: [f32; 4],
    to_sun: [f32; 4],
    sun_color: [f32; 4],
    ambient: [f32; 4],
    forward: [f32; 4],
    sun_specular: [f32; 4],
    alpha_fog: [f32; 4],
    shadow_uv: [[f32; 4]; 4],
    // the clouds' picture scale, its slide along u and v, 1 when they are drawn
    cloud: [f32; 4],
    // 1 when the near terrain's shadows and fog are laid on after its layers, unused x3
    terrain: [f32; 4],
    // the picture's width and height in pixels, unused x2
    viewport: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniforms {
    model: [[f32; 4]; 4],
    diffuse: [f32; 4],
    lit_diffuse: [f32; 4],
    specular: [f32; 4],
    layer_ambient: [f32; 4],
    params: [f32; 4],
    stage: [u32; 4],
    tfactor: [f32; 4],
    ops: [[u32; 4]; 3],
    program: [u32; 4],
    uv_sets: [u32; 4],
    uv_rows: [[f32; 4]; 6],
    pt_origin: [[f32; 4]; 4],
    pt_diffuse: [[f32; 4]; 4],
    pt_specular: [[f32; 4]; 4],
    shade: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshGpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    diffuse: [f32; 4],
    uv0: [f32; 2],
    uv1: [f32; 2],
    tangent_u: [f32; 3],
    tangent_v: [f32; 3],
    to_light: [f32; 3],
}

/// An attribute at `offset` of a vertex, read by the shader at `location`.
const fn attribute(
    format: wgpu::VertexFormat,
    offset: usize,
    location: u32,
) -> wgpu::VertexAttribute {
    wgpu::VertexAttribute {
        format,
        offset: offset as u64,
        shader_location: location,
    }
}

impl MeshGpuVertex {
    fn of(v: &auberean_draw::mesh::MeshVertex) -> Self {
        let c = auberean_world::color::Color::from_argb(v.diffuse);
        Self {
            position: v.position,
            normal: v.normal,
            diffuse: [c.r, c.g, c.b, c.a],
            uv0: v.uv[0],
            uv1: v.uv[1],
            tangent_u: v.tangent_u,
            tangent_v: v.tangent_v,
            to_light: v.to_light,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerrainGpuVertex {
    position: [f32; 3],
    color: [f32; 4],
    uv: [f32; 2],
    mask_uv: [f32; 2],
}

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Depth and a stencil, which the volumes of shadow count in.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

/// How an object pass blends, tests depth and culls, and what it writes: the pipeline's key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PassKey {
    src: u32,
    dst: u32,
    depth_test: bool,
    depth_write: bool,
    cull: u32,
    target: Target,
    /// The depth test turned over: it passes past what is marked.
    past: bool,
    /// A moved body's draw: its places and facings come from a stream of their own.
    posed: bool,
}

/// What a draw writes in its picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Target {
    /// The world's frame as the original's device keeps it: color alone, blended by the stored
    /// factors (the destination's alpha factors reading the frame's alpha).
    Frame,
    /// That frame's alpha alone, blended by the same factors: the shores' strips.
    FrameAlpha,
    /// A scene laid over the interface: color, and in the alpha how much of the picture it
    /// covers; the destination's alpha factors blend by the alpha the world's frame holds where no
    /// shore lies ([`FRAME_ALPHA`]).
    Coverage,
}

/// The device and everything it keeps between frames.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Why the device was lost, once it has been: it draws nothing more, and another must be made.
    lost: Arc<std::sync::Mutex<Option<String>>>,
    module: wgpu::ShaderModule,
    frame_layout: wgpu::BindGroupLayout,
    /// The sun's shadow map, as the vertices of a body standing on the land read it.
    shadow_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    terrain_pipelines: HashMap<&'static str, wgpu::RenderPipeline>,
    /// The shadow map's pipelines: the casters drawn black, and the lift by the shadow strength.
    shadow_pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
    /// The water's surface laid with its rippled reflection, and the ripples' bump picture.
    water_pipelines: HashMap<PassKey, wgpu::RenderPipeline>,
    /// The sun's share at each spot of the frame's bodies, a texel each.
    spot_pipeline: Option<wgpu::RenderPipeline>,
    /// The ripples laid over the water's surfaces, in their own picture.
    ripple_pipeline: Option<wgpu::RenderPipeline>,
    bump: Option<Arc<wgpu::TextureView>>,
    mesh_pipelines: HashMap<PassKey, wgpu::RenderPipeline>,
    veil: Option<wgpu::RenderPipeline>,
    textures: HashMap<u32, Arc<wgpu::TextureView>>,
    /// The texture bind groups of the draws, by their three textures and whether each clamps:
    /// made once, as long as the textures are kept.
    texture_groups: HashMap<[(u32, bool); 3], wgpu::BindGroup>,
    /// The draws' uniforms of a frame.
    uniforms: pool::UniformPool,
    /// The uniforms of a frame's lights and views.
    frame_uniforms: pool::UniformPool,
    /// The pictures frames are drawn into.
    targets: targets::Targets,
    white: Arc<wgpu::TextureView>,
    wrap: wgpu::Sampler,
    clamp: wgpu::Sampler,
    /// How the textures on the device were taken: their detail, and the samplers' filtering.
    textures_as: Textures,
    /// Trilinear filtering as the first frame drawn asked for it, kept for the device's life.
    trilinear: Option<bool>,
    /// The adapter's name, for the caption.
    pub adapter: String,
    /// Samples per pixel: with antialiasing on two, as the client asks for, or the fewest more the
    /// adapter offers; with it off one.
    pub samples: u32,
    /// The color the world's frame starts as (its alpha is the frame's, whatever this one's); a
    /// scene laid over the interface starts clear.
    pub backdrop: wgpu::Color,
    meshes: cache::MeshCache,
    /// What the frames since it was last taken asked of the device.
    work: std::cell::Cell<Work>,
    /// The sun's shadow map as last drawn.
    shadow_kept: Option<ShadowKept>,
    /// The sky objects' lens flares by their place in the day's list: each one's fade and
    /// whether its object was seen when last looked for; the look under way (the markers drawn at
    /// the objects, whose answers come back a frame or so later); the flares' and the markers'
    /// pipelines.
    flares: HashMap<usize, FlareSlot>,
    flare_scan: Option<FlareScan>,
    /// What the last look took, free again.
    flare_kept: Option<FlareLook>,
    /// Whether the device can look for the flares' objects. wgpu cannot name a render pass's
    /// occlusion queries to a browser's WebGPU, which refuses the pass for them, so there the
    /// flares stay unseen, and the world goes on being drawn.
    flares_seen: bool,
    /// How the device takes the game's pictures.
    pictures_as: PictureFormat,
    flare_pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
    /// The volumes of shadow's pipelines: the volumes counted in the stencil, and the darkening
    /// where the count is not zero.
    volume_pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
}

/// The sun's shadow map as last drawn, and all that decided it.
struct ShadowKept {
    view: wgpu::TextureView,
    signature: Vec<u8>,
    /// The casters' lists, held so their addresses stay theirs while the signature names them.
    lists: Vec<CasterLists>,
}

/// The lists one caster draws.
type CasterLists = (Arc<Vec<auberean_draw::mesh::MeshVertex>>, Arc<Vec<u16>>);

/// One caster of a shadow map: the batch, its pictures and its uniforms.
type Caster<'a> = (&'a ObjectBatch, [(u32, bool); 3], DrawUniforms);

/// A caster's pictures, as bytes for the map's signature.
fn texture_bytes(textures: &[(u32, bool); 3]) -> [u8; 15] {
    let mut out = [0; 15];
    for (o, (id, clamp)) in out.as_chunks_mut::<5>().0.iter_mut().zip(textures) {
        o[..4].copy_from_slice(&id.to_le_bytes());
        o[4] = u8::from(*clamp);
    }
    out
}

/// What a pass last bound.
struct Bound {
    pipeline: *const wgpu::RenderPipeline,
    env: Option<usize>,
    uniforms: Option<pool::Slot>,
    textures: Option<wgpu::BindGroup>,
    vertices: Option<cache::Slice>,
    posed: Option<cache::Slice>,
    indices: Option<cache::Slice>,
}

impl Default for Bound {
    fn default() -> Self {
        Self {
            pipeline: std::ptr::null(),
            env: None,
            uniforms: None,
            textures: None,
            vertices: None,
            posed: None,
            indices: None,
        }
    }
}

/// One lens flare as the device keeps it between frames.
#[derive(Clone, Copy, Debug, Default)]
struct FlareSlot {
    fade: FlareFade,
    seen: bool,
}

/// A look for the flares' objects under way: which flare each marker stands for, and the
/// buffer its answers (samples that passed the depth test) are read back through.
struct FlareScan {
    keys: Vec<usize>,
    look: FlareLook,
    done: Arc<std::sync::atomic::AtomicBool>,
}

/// What one look for the flares' objects takes on the device, kept for the next look at as many.
struct FlareLook {
    queries: wgpu::QuerySet,
    queries_count: u32,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("adapter", &self.adapter)
            .finish_non_exhaustive()
    }
}

/// The alpha the frame holds where no shore lies: the device writes only red, green and blue (but
/// for the shores' alpha), and every clear leaves alpha 127 of 255.
// D2-09812, D2-09817
const FRAME_ALPHA: f64 = 127.0 / 255.0;

/// A stored blend factor as the device blends it for `target`. The destination's alpha factors
/// blend by the frame's alpha, or in a scene laid over the interface by the alpha the frame holds
/// where no shore lies ([`FRAME_ALPHA`], every pass's blend constant).
// D2-09861
fn blend_factor(f: u32, target: Target) -> wgpu::BlendFactor {
    use wgpu::BlendFactor as B;
    match (f, target) {
        (1, _) => B::Zero,
        (3, _) => B::Src,
        (4, _) => B::OneMinusSrc,
        (5, _) => B::SrcAlpha,
        (6, _) => B::OneMinusSrcAlpha,
        (7, Target::Coverage) => B::Constant,
        (8, Target::Coverage) => B::OneMinusConstant,
        (7, _) => B::DstAlpha,
        (8, _) => B::OneMinusDstAlpha,
        (9, _) => B::Dst,
        (10, _) => B::OneMinusDst,
        (11, _) => B::SrcAlphaSaturated,
        _ => B::One,
    }
}

/// How `target` blends and which channels it writes, for the stored factors `src` and `dst`:
/// the world's frame blends its alpha (when it writes it) by the same factors, as the original's
/// device does; a scene laid over the interface lays each draw's coverage over what it covered.
// D2-09839
fn blend_of(src: u32, dst: u32, target: Target) -> (Option<wgpu::BlendState>, wgpu::ColorWrites) {
    let color = wgpu::BlendComponent {
        src_factor: blend_factor(src, target),
        dst_factor: blend_factor(dst, target),
        operation: wgpu::BlendOperation::Add,
    };
    let blend = (src != 2 || dst != 1).then_some(wgpu::BlendState {
        color,
        alpha: match target {
            Target::Coverage => wgpu::BlendComponent::OVER,
            Target::Frame | Target::FrameAlpha => color,
        },
    });
    let writes = match target {
        Target::Frame => wgpu::ColorWrites::COLOR,
        Target::FrameAlpha => wgpu::ColorWrites::ALPHA,
        Target::Coverage => wgpu::ColorWrites::ALL,
    };
    (blend, writes)
}

/// A texture's levels of detail: the picture and its smaller levels as stored.
// D2-10005
fn mips(img: &Image) -> Vec<(u32, u32, &[u8])> {
    std::iter::once(img)
        .chain(&img.smaller)
        .map(|l| (l.width, l.height, l.bgra.as_slice()))
        .collect()
}

fn mat(m: &Mat4) -> [[f32; 4]; 4] {
    m.0
}

/// A right-handed view and projection for a camera at `eye` looking along `forward`, z up, with
/// depth from 0 (near) to 1 (far): row-vector convention, as the world's transforms. In
/// perspective `fov_y` degrees high; straight on (`orthographic`), that many metres high and as
/// wide as the picture's shape makes it.
// D2-10574
#[allow(clippy::too_many_arguments)]
fn view_proj(
    eye: [f32; 3],
    forward: [f32; 3],
    roll: f32,
    fov_y: f32,
    orthographic: Option<f32>,
    aspect: f32,
    near: f32,
    far: f32,
) -> Mat4 {
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let f = norm(forward);
    let (r, u) = auberean_draw::scene::view_axes_of(f, roll);
    // View: world to camera space (x right, y up, z backward).
    let view = Mat4([
        [r[0], u[0], -f[0], 0.0],
        [r[1], u[1], -f[1], 0.0],
        [r[2], u[2], -f[2], 0.0],
        [-dot(r, eye), -dot(u, eye), dot(f, eye), 1.0],
    ]);
    if let Some(height) = orthographic {
        let d = 1.0 / (far - near);
        let proj = Mat4([
            [2.0 / (height * aspect), 0.0, 0.0, 0.0],
            [0.0, 2.0 / height, 0.0, 0.0],
            [0.0, 0.0, -d, 0.0],
            [0.0, 0.0, -near * d, 1.0],
        ]);
        return view.then(&proj);
    }
    let h = 1.0 / math::tanf(fov_y.to_radians() * 0.5);
    let w = h / aspect;
    let q = far / (near - far);
    let proj = Mat4([
        [w, 0.0, 0.0, 0.0],
        [0.0, h, 0.0, 0.0],
        [0.0, 0.0, q, -1.0],
        [0.0, 0.0, q * near, 0.0],
    ]);
    view.then(&proj)
}

/// How the device takes the game's pictures, which keep blue, green, red, alpha: as they are, or
/// where it cannot (a browser's WebGL 2 uploads red, green, blue, alpha alone) each texel's red and
/// blue exchanged on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureFormat {
    Bgra,
    Rgba,
}

impl PictureFormat {
    /// The textures' format.
    #[must_use]
    pub const fn format(self) -> wgpu::TextureFormat {
        match self {
            Self::Bgra => wgpu::TextureFormat::Bgra8Unorm,
            Self::Rgba => wgpu::TextureFormat::Rgba8Unorm,
        }
    }

    /// A picture's texels `bgra` as the device takes them.
    #[must_use]
    pub fn bytes(self, bgra: &[u8]) -> std::borrow::Cow<'_, [u8]> {
        match self {
            Self::Bgra => bgra.into(),
            Self::Rgba => {
                let mut texels = bgra.to_vec();
                for t in texels.as_chunks_mut::<4>().0 {
                    t.swap(0, 2);
                }
                texels.into()
            }
        }
    }
}

/// How a device on `backend` takes the game's pictures.
fn pictures_on(backend: wgpu::Backend) -> PictureFormat {
    if cfg!(target_arch = "wasm32") && backend == wgpu::Backend::Gl {
        PictureFormat::Rgba
    } else {
        PictureFormat::Bgra
    }
}

/// The limits to ask of `adapter`: the usual ones, or in a browser's WebGL 2 what it can give.
fn limits_on(adapter: &wgpu::Adapter) -> wgpu::Limits {
    if cfg!(target_arch = "wasm32") && adapter.get_info().backend == wgpu::Backend::Gl {
        wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
    } else {
        wgpu::Limits::default()
    }
}

impl Renderer {
    /// A renderer on the system's hardware adapter (never a software one).
    ///
    /// # Errors
    /// When there is no hardware adapter or no device.
    pub fn new() -> Result<Self, GpuError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        Self::on(&instance, None)
    }

    /// A renderer on `instance`'s hardware adapter, one that can present to `surface` when one is
    /// given (a window's).
    ///
    /// # Errors
    /// When there is no such hardware adapter or no device.
    pub fn on(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, GpuError> {
        pollster::block_on(Self::when_ready(instance, surface))
    }

    /// [`Self::on`], the edges smoothed or not: with them smoothed, each pixel is sampled more than
    /// once; without, once.
    ///
    /// # Errors
    /// When there is no such hardware adapter or no device.
    pub fn antialiased(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        antialiasing: bool,
    ) -> Result<Self, GpuError> {
        pollster::block_on(Self::made(instance, surface, antialiasing))
    }

    /// [`Self::on`], for a platform that cannot block for its device: a browser's answers on its
    /// own turn.
    ///
    /// # Errors
    /// When there is no such hardware adapter or no device.
    pub async fn when_ready(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, GpuError> {
        Self::made(instance, surface, true).await
    }

    async fn made(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        antialiasing: bool,
    ) -> Result<Self, GpuError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: surface,
                ..Default::default()
            })
            .await
            .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err(GpuError::NoAdapter(format!(
                "only a software adapter: {}",
                info.name
            )));
        }
        let flags = adapter.get_texture_format_features(COLOR_FORMAT).flags;
        let depth_flags = adapter.get_texture_format_features(DEPTH_FORMAT).flags;
        // Two samples need the adapter's own format features; four every adapter offers.
        let own = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let two = antialiasing
            && adapter.features().contains(own)
            && flags.sample_count_supported(2)
            && depth_flags.sample_count_supported(2);
        let samples = match (antialiasing, two) {
            (false, _) => 1,
            (true, true) => 2,
            (true, false) => 4,
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: if two { own } else { wgpu::Features::empty() },
                required_limits: limits_on(&adapter),
                ..Default::default()
            })
            .await
            .map_err(|e| GpuError::NoDevice(e.to_string()))?;
        let lost = Arc::new(std::sync::Mutex::new(None));
        {
            let lost = Arc::clone(&lost);
            device.set_device_lost_callback(move |reason, message| {
                let why = format!("{reason:?}: {message}");
                if let Ok(mut lost) = lost.lock() {
                    *lost = Some(why);
                }
            });
        }
        {
            // What is asked of a device already lost, and of what it made, goes nowhere and is no
            // fault of the asking. Any other error stops the game, as wgpu's own handler does.
            let lost = Arc::clone(&lost);
            device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                assert!(
                    lost.lock().is_ok_and(|lost| lost.is_some()),
                    "wgpu error: {e}"
                );
            }));
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()),
        });
        let uniform = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[uniform(0)],
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[uniform(0)],
        });
        // The third slot is read by the vertices too: a frill's shadow map.
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: if binding == 4 {
                wgpu::ShaderStages::VERTEX_FRAGMENT
            } else {
                wgpu::ShaderStages::FRAGMENT
            },
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let samp = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[tex(0), samp(1), tex(2), samp(3), tex(4), samp(5)],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&frame_layout),
                Some(&draw_layout),
                Some(&texture_layout),
                Some(&shadow_layout),
            ],
            immediate_size: 0,
        });
        let textures_as = Textures::default();
        let (wrap, clamp) = Self::samplers(&device, textures_as);
        let white = Arc::new(Self::upload(
            &device,
            &queue,
            &Image {
                width: 1,
                height: 1,
                bgra: vec![255; 4],
                smaller: Vec::new(),
            },
            pictures_on(info.backend),
        ));
        let uniforms = pool::UniformPool::new(&device, std::mem::size_of::<DrawUniforms>(), 1024);
        let frame_uniforms =
            pool::UniformPool::new(&device, std::mem::size_of::<FrameUniforms>(), 32);
        let mut r = Self {
            device,
            queue,
            lost,
            module,
            frame_layout,
            shadow_layout,
            draw_layout,
            texture_layout,
            pipeline_layout,
            terrain_pipelines: HashMap::new(),
            shadow_pipelines: None,
            volume_pipelines: None,
            water_pipelines: HashMap::new(),
            spot_pipeline: None,
            ripple_pipeline: None,
            bump: None,
            mesh_pipelines: HashMap::new(),
            veil: None,
            textures: HashMap::new(),
            texture_groups: HashMap::new(),
            uniforms,
            frame_uniforms,
            targets: targets::Targets::default(),
            white,
            wrap,
            clamp,
            textures_as,
            trilinear: None,
            adapter: format!("{} ({:?})", info.name, info.backend),
            samples,
            backdrop: wgpu::Color::BLACK,
            meshes: cache::MeshCache::default(),
            work: std::cell::Cell::default(),
            shadow_kept: None,
            flares: HashMap::new(),
            flare_scan: None,
            flare_kept: None,
            flares_seen: info.backend != wgpu::Backend::BrowserWebGpu,
            pictures_as: pictures_on(info.backend),
            flare_pipelines: None,
        };
        r.build_terrain_pipelines();
        Ok(r)
    }

    /// The device the frames are drawn on, for a host that draws more on it (a window's
    /// interface).
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Why the device was lost, once it has been: it draws nothing more, and another must be made.
    #[must_use]
    pub fn device_lost(&self) -> Option<String> {
        self.lost.lock().ok().and_then(|lost| lost.clone())
    }

    /// How the device takes the game's pictures, for a host that uploads more of them.
    #[must_use]
    pub fn pictures(&self) -> PictureFormat {
        self.pictures_as
    }

    /// The device's queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// What the frames since the last call asked of the device, and start counting again.
    pub fn take_work(&self) -> Work {
        self.work.take()
    }

    /// Add to what the frames ask of the device, for a host that draws more on it.
    pub fn note_work(&self, add: impl FnOnce(&mut Work)) {
        let mut w = self.work.get();
        add(&mut w);
        self.work.set(w);
    }

    /// A buffer of `bytes`, for the count.
    fn note_buffer(&self, bytes: usize) {
        self.note_work(|w| {
            w.buffers += 1;
            w.buffer_bytes += bytes as u64;
        });
    }

    /// A picture to draw into, kept between frames.
    fn target(
        &mut self,
        label: &'static str,
        (width, height, samples): (u32, u32, u32),
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let key = targets::TargetKey {
            label,
            width,
            height,
            samples,
            format,
            usage,
        };
        let (texture, view, made) = self.targets.take(&self.device, key);
        if made {
            self.note_work(|w| {
                w.textures += 1;
                w.texture_bytes += u64::from(width) * u64::from(height) * u64::from(samples) * 4;
            });
        }
        (texture, view)
    }

    /// A picture [`Self::draw`] left is done with: the frames it was part of have been shown, and
    /// what their pictures and lists took can serve the next.
    pub fn end_frame(&mut self) {
        self.meshes.end_frame();
        self.targets.end_frame();
    }

    fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: &Image,
        pictures: PictureFormat,
    ) -> wgpu::TextureView {
        let levels = mips(img);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: img.width,
                height: img.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: u32::try_from(levels.len()).unwrap_or(1),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: pictures.format(),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, (w, h, data)) in levels.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: u32::try_from(level).unwrap_or(0),
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pictures.bytes(data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(*h),
                },
                wgpu::Extent3d {
                    width: *w,
                    height: *h,
                    depth_or_array_layers: 1,
                },
            );
        }
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// The samplers that wrap and that clamp, filtering as `textures` says: between a texture's
    /// two nearest levels with trilinear filtering on, else from the nearest alone; along the
    /// slant as far as the device goes (16, as the client takes the device's greatest) with
    /// anisotropic filtering on. The device filters along the slant only between levels, so
    /// anisotropic filtering with trilinear off filters from the nearest level, not along the
    /// slant.
    // D2-09793, D2-09862, D2-09861
    fn samplers(device: &wgpu::Device, textures: Textures) -> (wgpu::Sampler, wgpu::Sampler) {
        let sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: if textures.trilinear {
                    wgpu::MipmapFilterMode::Linear
                } else {
                    wgpu::MipmapFilterMode::Nearest
                },
                anisotropy_clamp: if textures.anisotropic && textures.trilinear {
                    16
                } else {
                    1
                },
                ..Default::default()
            })
        };
        (
            sampler(wgpu::AddressMode::Repeat),
            sampler(wgpu::AddressMode::ClampToEdge),
        )
    }

    /// Take the textures as `textures` says: at another detail every texture is let go, to be
    /// loaded again at that detail as the client flushes its graphics resources; with other
    /// anisotropic filtering the samplers are made again. Trilinear filtering is taken from the
    /// first frame drawn and kept: the device sets each stage's level filter once, when it is
    /// made, so a change shows only on the next start.
    // D2-09722, D2-09706, D2-09671, D2-10018, D2-09862, D2-09861
    fn take_textures(&mut self, textures: Textures) {
        let textures = match self.trilinear {
            Some(t) => Textures {
                trilinear: t,
                ..textures
            },
            None => {
                self.trilinear = Some(textures.trilinear);
                textures
            }
        };
        if textures == self.textures_as {
            return;
        }
        if textures.detail != self.textures_as.detail {
            self.textures.clear();
        }
        if (textures.trilinear, textures.anisotropic)
            != (self.textures_as.trilinear, self.textures_as.anisotropic)
        {
            (self.wrap, self.clamp) = Self::samplers(&self.device, textures);
        }
        self.texture_groups.clear();
        self.shadow_kept = None;
        self.textures_as = textures;
    }

    /// Picture `id` on the device, loaded with `load` the first time it is asked for, without the
    /// largest levels the texture detail leaves out.
    fn texture(
        &mut self,
        id: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Arc<wgpu::TextureView> {
        if id == 0 {
            return self.white.clone();
        }
        if let Some(t) = self.textures.get(&id) {
            return t.clone();
        }
        let detail = self.textures_as.detail;
        let view = load(id).map_or_else(
            || self.white.clone(),
            |img| {
                Arc::new(Self::upload(
                    &self.device,
                    &self.queue,
                    &img.at_detail(detail),
                    self.pictures_as,
                ))
            },
        );
        self.textures.insert(id, view.clone());
        view
    }

    fn pipeline(
        &self,
        vs: &str,
        fs: &str,
        layout: wgpu::VertexBufferLayout<'_>,
        key: PassKey,
    ) -> wgpu::RenderPipeline {
        self.pipeline_of(vs, fs, &[Some(layout)], key)
    }

    fn pipeline_of(
        &self,
        vs: &str,
        fs: &str,
        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
        key: PassKey,
    ) -> wgpu::RenderPipeline {
        let (blend, write_mask) = blend_of(key.src, key.dst, key.target);
        self.device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: match key.cull {
                        d3d::CULL_CW => Some(wgpu::Face::Back),
                        d3d::CULL_CCW => Some(wgpu::Face::Front),
                        _ => None,
                    },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(key.depth_write),
                    depth_compare: Some(if !key.depth_test {
                        wgpu::CompareFunction::Always
                    } else if key.past {
                        wgpu::CompareFunction::Greater
                    } else {
                        wgpu::CompareFunction::LessEqual
                    }),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: self.samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
    }

    fn terrain_layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2, 3 => Float32x2];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainGpuVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }

    fn mesh_layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32x2, 5 => Float32x3, 6 => Float32x3, 7 => Float32x3];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshGpuVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        }
    }

    /// A moved body's two streams: all a vertex keeps but its place and facing, from the mesh's own
    /// list, whose place and facing are not read; and the place and facing, from the pose's.
    fn posed_layouts() -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
        const KEPT: [wgpu::VertexAttribute; 6] = [
            attribute(
                wgpu::VertexFormat::Float32x4,
                std::mem::offset_of!(MeshGpuVertex, diffuse),
                2,
            ),
            attribute(
                wgpu::VertexFormat::Float32x2,
                std::mem::offset_of!(MeshGpuVertex, uv0),
                3,
            ),
            attribute(
                wgpu::VertexFormat::Float32x2,
                std::mem::offset_of!(MeshGpuVertex, uv1),
                4,
            ),
            attribute(
                wgpu::VertexFormat::Float32x3,
                std::mem::offset_of!(MeshGpuVertex, tangent_u),
                5,
            ),
            attribute(
                wgpu::VertexFormat::Float32x3,
                std::mem::offset_of!(MeshGpuVertex, tangent_v),
                6,
            ),
            attribute(
                wgpu::VertexFormat::Float32x3,
                std::mem::offset_of!(MeshGpuVertex, to_light),
                7,
            ),
        ];
        const POSED: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        [
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<MeshGpuVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &KEPT,
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<auberean_draw::mesh::PosedVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &POSED,
            }),
        ]
    }

    fn build_terrain_pipelines(&mut self) {
        let opaque = PassKey {
            src: 2,
            dst: 1,
            depth_test: true,
            depth_write: true,
            cull: d3d::CULL_CW,
            target: Target::Frame,
            past: false,
            posed: false,
        };
        let blended = PassKey {
            src: d3d::BLEND_SRC_ALPHA,
            dst: d3d::BLEND_INV_SRC_ALPHA,
            depth_test: true,
            depth_write: false,
            cull: d3d::CULL_CW,
            target: Target::Frame,
            past: false,
            posed: false,
        };
        let base = self.pipeline(
            "vs_terrain",
            "fs_terrain_base",
            Self::terrain_layout(),
            opaque,
        );
        let mask = self.pipeline(
            "vs_terrain",
            "fs_terrain_mask",
            Self::terrain_layout(),
            blended,
        );
        let far = self.pipeline(
            "vs_terrain",
            "fs_terrain_far",
            Self::terrain_layout(),
            opaque,
        );
        // Laid over the near terrain after its layers, on its own depth: the shadows multiply
        // what is there, then the fog blends over it.
        let shade = self.pipeline(
            "vs_terrain",
            "fs_terrain_shade",
            Self::terrain_layout(),
            PassKey {
                src: d3d::BLEND_DEST_COLOR,
                dst: d3d::BLEND_ZERO,
                depth_test: true,
                depth_write: false,
                cull: d3d::CULL_CW,
                target: Target::Frame,
                past: false,
                posed: false,
            },
        );
        let fog = self.pipeline(
            "vs_terrain",
            "fs_terrain_fog",
            Self::terrain_layout(),
            blended,
        );
        self.terrain_pipelines.insert("base", base);
        self.terrain_pipelines.insert("mask", mask);
        self.terrain_pipelines.insert("far", far);
        self.terrain_pipelines.insert("shade", shade);
        self.terrain_pipelines.insert("fog", fog);
        let veil = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("veil"),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vs_veil"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some("fs_veil"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: self.samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            });
        self.veil = Some(veil);
    }

    /// The shadow map's two pipelines, on a single-sampled picture with no depth: the casters,
    /// black through their first texture's alpha, blended over what is there; and a full-picture
    /// lift, added.
    // D2-10609, D2-10610
    fn build_shadow_pipelines(&mut self) {
        if self.shadow_pipelines.is_some() {
            return;
        }
        let make = |vs: &str,
                    fs: &str,
                    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                    blend: wgpu::BlendState| {
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(fs),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.module,
                        entry_point: Some(vs),
                        compilation_options: Default::default(),
                        buffers,
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.module,
                        entry_point: Some(fs),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: COLOR_FORMAT,
                            blend: Some(blend),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
        };
        let caster = make(
            "vs_caster",
            "fs_caster",
            &[Some(Self::mesh_layout())],
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let lift = make(
            "vs_veil",
            "fs_lift",
            &[],
            wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            },
        );
        self.shadow_pipelines = Some((caster, lift));
    }

    /// The volumes of shadow's two pipelines. The volumes, no color written, tested against the
    /// depth but not marking it: where a face fails the depth test the stencil counts up for a
    /// face turned away and down for one turned toward the camera, wrapping. Then a full-picture
    /// darkening where the count is not zero: what is there times one less the shadow strength,
    /// the count set back to zero.
    // D2-10611
    fn build_volume_pipelines(&mut self) {
        if self.volume_pipelines.is_some() {
            return;
        }
        let face = |depth_fail_op| wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let shaded = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Less,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Zero,
        };
        let make = |label: &str,
                    vs: &str,
                    fs: &str,
                    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                    target: wgpu::ColorTargetState,
                    depth_compare: wgpu::CompareFunction,
                    stencil: wgpu::StencilState| {
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.module,
                        entry_point: Some(vs),
                        compilation_options: Default::default(),
                        buffers,
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.module,
                        entry_point: Some(fs),
                        compilation_options: Default::default(),
                        targets: &[Some(target)],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(depth_compare),
                        stencil,
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState {
                        count: self.samples,
                        ..Default::default()
                    },
                    multiview_mask: None,
                    cache: None,
                })
        };
        const POSITION: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
        let count = make(
            "volumes",
            "vs_volume",
            "fs_volume",
            &[Some(wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &POSITION,
            })],
            wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            },
            wgpu::CompareFunction::LessEqual,
            wgpu::StencilState {
                // the faces the device draws in its first pass count up, those of its second
                // count down
                front: face(wgpu::StencilOperation::DecrementWrap),
                back: face(wgpu::StencilOperation::IncrementWrap),
                read_mask: 0xFF,
                write_mask: 0xFF,
            },
        );
        let darken = make(
            "volume shadow",
            "vs_veil",
            "fs_darken",
            &[],
            wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::OneMinusSrc,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::COLOR,
            },
            wgpu::CompareFunction::Always,
            wgpu::StencilState {
                front: shaded,
                back: shaded,
                read_mask: 0xFF,
                write_mask: 0xFF,
            },
        );
        self.volume_pipelines = Some((count, darken));
    }

    /// The sun's shadow map of `shadow`'s casters: white, the casters black over it, then lifted
    /// by one less the shadow strength. A map whose casters and sun are as they were when it was
    /// last drawn holds this frame's picture already, and is not drawn again.
    // D2-10929, D2-10609, D2-10610
    fn render_shadow_map(
        &mut self,
        shadow: &ShadowMap,
        frame_u: &FrameUniforms,
        encoder: &mut wgpu::CommandEncoder,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> wgpu::TextureView {
        self.build_shadow_pipelines();
        let view = match &self.shadow_kept {
            Some(k) => k.view.clone(),
            None => {
                self.note_work(|w| w.textures += 1);
                self.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("shadow map"),
                        size: wgpu::Extent3d {
                            width: SHADOW_MAP_SIZE,
                            height: SHADOW_MAP_SIZE,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: COLOR_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default())
            }
        };
        let mut map_u = *frame_u;
        map_u.view_proj = mat(&shadow.view_projection());
        let s = shadow.strength;
        // the lift's color: one less the strength, added at full alpha
        map_u.fog_color = [1.0 - s.r, 1.0 - s.g, 1.0 - s.b, 1.0];
        let black = auberean_world::color::Color::new(0.0, 0.0, 0.0, 1.0);
        // All that decides the map's picture: the sun's view, the lift, and each caster's lists,
        // pictures and uniforms.
        let casters: Vec<Caster<'_>> = shadow
            .casters
            .iter()
            .filter(|b| !b.indices.is_empty())
            .map(|b| {
                let stage = b.layer.stages.first().copied();
                (
                    b,
                    [
                        stage.map_or((0, false), |s| (s.texture, s.clamp_u || s.clamp_v)),
                        (0, false),
                        (0, false),
                    ],
                    Self::draw_uniforms(
                        &b.layer,
                        &b.transform,
                        black,
                        frame_u.sun_color,
                        &PointLights::NONE,
                    ),
                )
            })
            .collect();
        let (mut signature, lists) = self
            .shadow_kept
            .take()
            .map_or_else(Default::default, |k| (k.signature, k.lists));
        let unchanged = {
            let mut was = signature.as_slice();
            let mut same = |bytes: &[u8]| match was.split_at_checked(bytes.len()) {
                Some((head, tail)) if head == bytes => {
                    was = tail;
                    true
                }
                _ => false,
            };
            same(bytemuck::bytes_of(&map_u.view_proj))
                && same(bytemuck::bytes_of(&map_u.fog_color))
                && casters.iter().all(|(b, textures, u)| {
                    same(&Self::caster_lists(b))
                        && same(&texture_bytes(textures))
                        && same(bytemuck::bytes_of(u))
                })
                && was.is_empty()
        };
        if unchanged {
            self.shadow_kept = Some(ShadowKept {
                view: view.clone(),
                signature,
                lists,
            });
            return view;
        }
        signature.clear();
        signature.extend_from_slice(bytemuck::bytes_of(&map_u.view_proj));
        signature.extend_from_slice(bytemuck::bytes_of(&map_u.fog_color));
        for (b, textures, u) in &casters {
            signature.extend_from_slice(&Self::caster_lists(b));
            signature.extend_from_slice(&texture_bytes(textures));
            signature.extend_from_slice(bytemuck::bytes_of(u));
        }
        self.shadow_kept = Some(ShadowKept {
            view: view.clone(),
            signature,
            lists: casters
                .iter()
                .map(|(b, _, _)| (Arc::clone(&b.vertices), Arc::clone(&b.indices)))
                .collect(),
        });
        let map_group =
            self.frame_uniforms
                .take(&self.device, &self.frame_layout, bytemuck::bytes_of(&map_u));
        let mut draws = Vec::new();
        for (b, textures, u) in &casters {
            let texture_group = self.texture_group(*textures, load);
            let draw_group =
                self.uniforms
                    .take(&self.device, &self.draw_layout, bytemuck::bytes_of(u));
            let (vertices, indices) = self.meshes.buffers(&self.device, &b.vertices, &b.indices);
            draws.push((
                vertices,
                indices,
                u32::try_from(b.indices.len()).unwrap_or(0),
                draw_group,
                texture_group,
            ));
        }
        let blank = self.texture_group([(0, false); 3], load);
        let identity = self.uniforms.take(
            &self.device,
            &self.draw_layout,
            bytemuck::bytes_of(&Self::draw_uniforms(
                &dummy_layer(),
                &Mat4::IDENTITY,
                auberean_world::color::Color::WHITE,
                frame_u.sun_color,
                &PointLights::NONE,
            )),
        );
        let Some((caster, lift)) = self.shadow_pipelines.as_ref() else {
            return view;
        };
        let white_shadow = self.shadow_group(&self.white);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow map"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(caster);
        pass.set_bind_group(0, &map_group.group, &[map_group.offset]);
        pass.set_bind_group(3, &white_shadow, &[]);
        for (vb, ib, count, dg, tg) in &draws {
            pass.set_bind_group(1, &dg.group, &[dg.offset]);
            pass.set_bind_group(2, tg, &[]);
            pass.set_vertex_buffer(0, vb.get());
            pass.set_index_buffer(ib.get(), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..*count, 0, 0..1);
        }
        self.note_work(|w| w.draws += u32::try_from(draws.len()).unwrap_or(0) + 1);
        pass.set_pipeline(lift);
        pass.set_bind_group(1, &identity.group, &[identity.offset]);
        pass.set_bind_group(2, &blank, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        view
    }

    /// Which lists a caster draws, as bytes for the map's signature.
    fn caster_lists(b: &ObjectBatch) -> [u8; 16] {
        let (v, i) = (
            Arc::as_ptr(&b.vertices) as u64,
            Arc::as_ptr(&b.indices) as u64,
        );
        bytemuck::cast([v.to_le_bytes(), i.to_le_bytes()])
    }

    /// The ripples' bump picture: 256 x 256 signed offsets, the sum of a ring of waves fading
    /// from the middle (amplitude 64) and two straight wave trains (32 and 16), cosines for the
    /// first offset and sines for the second.
    // D2-10944
    fn bump_picture(&mut self) -> Arc<wgpu::TextureView> {
        if let Some(b) = &self.bump {
            return b.clone();
        }
        let mut data = vec![0u8; 256 * 256 * 2];
        for y in 0..256u32 {
            for x in 0..256u32 {
                #[allow(clippy::cast_precision_loss)]
                let (fx, fy) = (x as f32 / 256.0 - 0.5, y as f32 / 256.0 - 0.5);
                let r = (fx * fx + fy * fy).sqrt();
                let fade = math::expf(-5.0 * r);
                let (a, b, c) = (300.0 * r, 150.0 * (fx + fy), 140.0 * (fx * 0.85 - fy));
                let int = |v: f32| dereth_primitives::num::to_i32_f64(f64::from(v));
                // each wave truncated on its own, then summed in a byte
                let du = int(64.0 * math::cosf(a) * fade)
                    + int(32.0 * math::cosf(b))
                    + int(16.0 * math::cosf(c));
                let dv = int(64.0 * math::sinf(a) * fade)
                    + int(32.0 * math::sinf(b))
                    + int(16.0 * math::sinf(c));
                let k = ((y * 256 + x) * 2) as usize;
                data[k] = du.to_le_bytes()[0];
                data[k + 1] = dv.to_le_bytes()[0];
            }
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ripples"),
            size: wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg8Snorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512),
                rows_per_image: Some(256),
            },
            wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
        );
        let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        self.bump = Some(view.clone());
        view
    }

    fn mesh_pipeline(&mut self, key: PassKey) {
        if !self.mesh_pipelines.contains_key(&key) {
            let p = if key.posed {
                self.pipeline_of("vs_mesh", "fs_mesh", &Self::posed_layouts(), key)
            } else {
                self.pipeline("vs_mesh", "fs_mesh", Self::mesh_layout(), key)
            };
            self.mesh_pipelines.insert(key, p);
        }
    }

    /// How a draw of `layer` blends, tests depth and culls, writing `target`: as stored, but a
    /// sky object's draw neither tests nor marks depth.
    fn object_key(layer: &DrawLayer, sky: bool, target: Target) -> PassKey {
        PassKey {
            src: layer.source_blend,
            dst: layer.dest_blend,
            depth_test: !sky && layer.depth_test != d3d::CMP_ALWAYS,
            depth_write: !sky && layer.depth_write,
            cull: layer.cull,
            target,
            past: false,
            posed: false,
        }
    }

    /// How the water's rippled reflection is laid over a surface of `layer`: blended by the
    /// frame's alpha, tested against depth, culled as the surface is.
    fn water_key(layer: &DrawLayer, target: Target) -> PassKey {
        PassKey {
            src: d3d::BLEND_DEST_ALPHA,
            dst: d3d::BLEND_INV_DEST_ALPHA,
            depth_test: true,
            depth_write: false,
            cull: layer.cull,
            target,
            past: false,
            posed: false,
        }
    }

    fn water_pipeline(&mut self, key: PassKey) {
        if !self.water_pipelines.contains_key(&key) {
            let p = self.pipeline("vs_mesh", "fs_water", Self::mesh_layout(), key);
            self.water_pipelines.insert(key, p);
        }
    }

    /// The lens flares' two pipelines, on the screen: the points added on, and the markers that
    /// look for their objects (tested against depth, drawing nothing).
    fn build_flare_pipelines(&mut self) {
        if self.flare_pipelines.is_some() {
            return;
        }
        let screen = |src, dst, depth_test| PassKey {
            src,
            dst,
            depth_test,
            depth_write: false,
            cull: d3d::CULL_NONE,
            target: Target::Frame,
            past: false,
            posed: false,
        };
        let flare = self.pipeline(
            "vs_screen",
            "fs_screen",
            Self::mesh_layout(),
            screen(d3d::BLEND_SRC_ALPHA, d3d::BLEND_ONE, false),
        );
        let marker = self.pipeline(
            "vs_screen",
            "fs_screen",
            Self::mesh_layout(),
            screen(d3d::BLEND_ZERO, d3d::BLEND_ONE, true),
        );
        self.flare_pipelines = Some((flare, marker));
    }

    /// Build now every pipeline the device makes: its own (the terrain's, the fog's veil, the
    /// shadow map's, the lens flares') and those the draws of `layers` would make the first time
    /// each is drawn, as a world object, as a sky object, as a water surface (laid with its
    /// material, then with the reflection), as a shore's strip in the frame's alpha and as an
    /// object of a scene laid over the interface. Returns how many pipelines the device then
    /// holds.
    pub fn build_pipelines<'l>(
        &mut self,
        layers: impl IntoIterator<Item = &'l DrawLayer>,
    ) -> usize {
        self.build_shadow_pipelines();
        self.build_flare_pipelines();
        self.build_volume_pipelines();
        for layer in layers {
            for sky in [false, true] {
                self.mesh_pipeline(Self::object_key(layer, sky, Target::Frame));
            }
            let mut opaque = layer.clone();
            opaque.source_blend = d3d::BLEND_ONE;
            opaque.dest_blend = d3d::BLEND_ZERO;
            self.mesh_pipeline(Self::object_key(&opaque, false, Target::Frame));
            self.water_pipeline(Self::water_key(layer, Target::Frame));
            for target in [Target::FrameAlpha, Target::Coverage] {
                self.mesh_pipeline(Self::object_key(layer, false, target));
            }
            // As a moved body's part, in the world or in a scene over the interface.
            for target in [Target::Frame, Target::Coverage] {
                self.mesh_pipeline(PassKey {
                    posed: true,
                    ..Self::object_key(layer, false, target)
                });
            }
            // Faded, a layer that tests or blends by alpha marks no depth.
            if layer.alpha_test && layer.depth_write {
                let mut faded = Self::object_key(layer, false, Target::Frame);
                faded.depth_write = false;
                self.mesh_pipeline(faded);
            }
        }
        self.pipeline_count()
    }

    /// How many pipelines the device holds.
    #[must_use]
    pub fn pipeline_count(&self) -> usize {
        self.terrain_pipelines.len()
            + usize::from(self.veil.is_some())
            + 2 * usize::from(self.shadow_pipelines.is_some())
            + 2 * usize::from(self.flare_pipelines.is_some())
            + 2 * usize::from(self.volume_pipelines.is_some())
            + self.mesh_pipelines.len()
            + self.water_pipelines.len()
    }

    fn bind_textures(
        &self,
        t0: &wgpu::TextureView,
        s0: &wgpu::Sampler,
        t1: &wgpu::TextureView,
        s1: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.bind_three(&[(t0, s0), (t1, s1), (&self.white, &self.wrap)])
    }

    /// The bind group of a draw's three textures (picture ids, 0 for none) and whether each clamps,
    /// made the first time they are asked for together.
    fn texture_group(
        &mut self,
        key: [(u32, bool); 3],
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> wgpu::BindGroup {
        if let Some(g) = self.texture_groups.get(&key) {
            return g.clone();
        }
        let views = key.map(|(id, _)| self.texture(id, load));
        let sampler = |clamp: bool| if clamp { &self.clamp } else { &self.wrap };
        let g = self.bind_three(&[
            (&views[0], sampler(key[0].1)),
            (&views[1], sampler(key[1].1)),
            (&views[2], sampler(key[2].1)),
        ]);
        self.texture_groups.insert(key, g.clone());
        g
    }

    fn bind_three(&self, stages: &[(&wgpu::TextureView, &wgpu::Sampler); 3]) -> wgpu::BindGroup {
        self.note_work(|w| w.bind_groups += 1);
        let mut entries = Vec::with_capacity(6);
        for (k, (t, s)) in (0u32..).zip(stages.iter()) {
            entries.push(wgpu::BindGroupEntry {
                binding: 2 * k,
                resource: wgpu::BindingResource::TextureView(t),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 2 * k + 1,
                resource: wgpu::BindingResource::Sampler(s),
            });
        }
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_layout,
            entries: &entries,
        })
    }

    /// The stages a draw binds, in its texture slots: its first three (with the normal-mapped
    /// pixel program: the normal map, the diffuse map and the detail map).
    fn bound_stages(layer: &DrawLayer) -> [Option<Stage>; 3] {
        [0, 1, 2].map(|k| layer.stages.get(k).copied())
    }

    fn draw_uniforms(
        layer: &DrawLayer,
        model: &Mat4,
        tint: auberean_world::color::Color,
        sun: [f32; 4],
        points: &PointLights,
    ) -> DrawUniforms {
        let c = |c: auberean_world::color::Color| {
            [c.r * tint.r, c.g * tint.g, c.b * tint.b, c.a * tint.a]
        };
        let bound = Self::bound_stages(layer);
        let mut uv_sets = [0u32; 4];
        let mut uv_rows = [[0.0f32; 4]; 6];
        for (k, s) in bound.iter().enumerate() {
            let m = s.map_or(UV_IDENTITY, |s| s.uv_transform);
            // A coordinate set (the vertex has two), or coordinates the program works out: the
            // anisotropic ones (source 0x200) or the eye's reflection about the surface (0x100).
            uv_sets[k] = s.map_or(0, |s| match s.tex_coord {
                0x100 => 3,
                0x200 => 2,
                t => t.min(1),
            });
            uv_rows[2 * k] = [m[0], m[1], m[2], 0.0];
            uv_rows[2 * k + 1] = [m[3], m[4], m[5], 0.0];
        }
        let pixel = layer.pixel_program.map_or([0; 4], |p| {
            let mut opts = 0;
            for (on, bit) in [
                (p.detail_on_diffuse, 0x1),
                (p.detail_on_normal, 0x2),
                (p.alpha_from_diffuse, 0x4),
                (p.gloss, 0x8),
                (p.half_vector_specular, 0x10),
                (p.environment_add, 0x20),
                (p.environment_gloss, 0x40),
            ] {
                if on {
                    opts |= bit;
                }
            }
            [1, opts, 0, 0]
        });
        // The pixel programs' lit color: the first light's diffuse (the first point light's, else
        // the sun's, black inside with neither) times the layer's diffuse (and tint), each channel
        // at most 1, alpha the layer's at most 1.
        let sun = if points.count > 0 {
            let f = points.lights[0].diffuse;
            [f.r, f.g, f.b, 1.0]
        } else if points.inside {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            sun
        };
        let d = c(layer.diffuse);
        let lit_diffuse = [
            (sun[0] * d[0]).min(1.0),
            (sun[1] * d[1]).min(1.0),
            (sun[2] * d[2]).min(1.0),
            d[3].min(1.0),
        ];
        // The texture stages' arithmetic, for a layer without a pixel program: each of the first
        // three stages as stored, the rest disabled; a layer with no stages modulates its vertex
        // color twice by white.
        let mut ops = [[d3d::OP_DISABLE, 0, d3d::OP_DISABLE, 0]; 3];
        if layer.stages.is_empty() {
            ops[0] = [
                d3d::OP_MODULATE2X,
                d3d::ARG_TEXTURE | (d3d::ARG_DIFFUSE << 8),
                d3d::OP_SELECT_ARG1,
                d3d::ARG_DIFFUSE,
            ];
        }
        for (o, s) in ops.iter_mut().zip(&layer.stages) {
            *o = [
                s.color_op,
                s.color_arg1 | (s.color_arg2 << 8),
                s.alpha_op,
                s.alpha_arg1 | (s.alpha_arg2 << 8),
            ];
        }
        // The texture factor is the layer's diffuse color (and tint) as a stored color.
        let q = |x: f32| (x.clamp(0.0, 1.0) * 255.0).trunc() / 255.0;
        let tfactor = [q(d[0]), q(d[1]), q(d[2]), q(d[3])];
        DrawUniforms {
            model: mat(model),
            diffuse: d,
            lit_diffuse,
            specular: c(layer.specular),
            layer_ambient: [
                layer.ambient.r,
                layer.ambient.g,
                layer.ambient.b,
                layer.ambient.a,
            ],
            params: [
                layer.specular_power,
                if layer.alpha_test {
                    layer.alpha_ref
                } else {
                    -1.0
                },
                0.0,
                0.0,
            ],
            stage: [
                layer.switches,
                u32::try_from(points.count).unwrap_or(0),
                u32::from(points.inside),
                0,
            ],
            tfactor,
            ops,
            program: pixel,
            uv_sets,
            uv_rows,
            pt_origin: points.lights.map(|l| {
                let r = l.range.max(0.0002);
                [l.origin.x, l.origin.y, l.origin.z, 10.0 / (r * r)]
            }),
            pt_diffuse: points
                .lights
                .map(|l| [l.diffuse.r, l.diffuse.g, l.diffuse.b, 1.0]),
            pt_specular: points
                .lights
                .map(|l| [l.specular.r, l.specular.g, l.specular.b, 1.0]),
            shade: [0.0; 4],
        }
    }

    /// The picture of the sun's share at each of `spots`, a texel each, made through the shadow
    /// map's group `map`: the group the bodies read it through.
    fn draw_body_spots(
        &mut self,
        spots: &HashMap<[u32; 3], usize>,
        map: &wgpu::BindGroup,
        identity: &pool::Slot,
        outdoors_u: &FrameUniforms,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::BindGroup {
        if self.spot_pipeline.is_none() {
            const ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
            self.spot_pipeline = Some(self.device.create_render_pipeline(
                &wgpu::RenderPipelineDescriptor {
                    label: Some("fs_spot"),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.module,
                        entry_point: Some("vs_spot"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: 12,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &ATTRS,
                        })],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &self.module,
                        entry_point: Some("fs_spot"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: COLOR_FORMAT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::PointList,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                },
            ));
        }
        let (_, view) = self.target(
            "the bodies' spots",
            (
                u32::try_from(BODY_SPOTS.0).unwrap_or(1),
                u32::try_from(BODY_SPOTS.1).unwrap_or(1),
                1,
            ),
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let count = spots.len().min(BODY_SPOTS.0 * BODY_SPOTS.1);
        let mut at = vec![[0.0f32; 3]; count];
        for (bits, &k) in spots {
            if let Some(slot) = at.get_mut(k) {
                *slot = bits.map(f32::from_bits);
            }
        }
        let vertices = self
            .meshes
            .passing_vertices
            .put(&self.device, bytemuck::cast_slice(&at));
        let outdoors = self.frame_uniforms.take(
            &self.device,
            &self.frame_layout,
            bytemuck::bytes_of(outdoors_u),
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("the bodies' spots"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(pipeline) = &self.spot_pipeline {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &outdoors.group, &[outdoors.offset]);
                pass.set_bind_group(1, &identity.group, &[identity.offset]);
                pass.set_bind_group(2, &self.spot_textures(), &[]);
                pass.set_bind_group(3, map, &[]);
                pass.set_vertex_buffer(0, vertices.get());
                pass.draw(0..u32::try_from(count).unwrap_or(0), 0..1);
            }
        }
        self.shadow_group(&view)
    }

    /// The textures of a draw that reads none.
    fn spot_textures(&self) -> wgpu::BindGroup {
        self.bind_three(&[
            (&self.white, &self.wrap),
            (&self.white, &self.wrap),
            (&self.white, &self.wrap),
        ])
    }

    /// The group the shadow map `view` is read through.
    fn shadow_group(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.note_work(|w| w.bind_groups += 1);
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.shadow_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            }],
        })
    }

    /// Draw `frame` at `width` x `height`, loading pictures with `load` the first time each is used,
    /// and read the picture back, every texel opaque.
    ///
    /// # Errors
    /// When the picture cannot be read back from the device.
    pub fn render(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<Picture, GpuError> {
        let color = self.draw(frame, width, height, load);
        let picture = self.read_back(&color, width, height, true);
        self.end_frame();
        picture
    }

    /// Draw `frame` as a scene laid over the interface ([`Self::draw_laid_over`]) and read the
    /// picture back, its alpha how much of each texel the scene covers.
    ///
    /// # Errors
    /// When the picture cannot be read back from the device.
    pub fn render_laid_over(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<Picture, GpuError> {
        let color = self.draw_laid_over(frame, width, height, load);
        let picture = self.read_back(&color, width, height, false);
        self.end_frame();
        picture
    }

    /// Draw the world's `frame` at `width` x `height` into a picture left on the device (red,
    /// green, blue, and the frame's alpha; one a shader may sample or a copy may read), loading
    /// pictures with `load` the first time each is used. It starts as [`Self::backdrop`]. The
    /// picture is the host's until it calls [`Self::end_frame`].
    pub fn draw(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> wgpu::Texture {
        self.draw_as(frame, width, height, load, Target::Frame)
    }

    /// Draw `frame` as a scene laid over the interface (a panel's viewport): into a clear picture,
    /// its alpha how much of each texel the scene covers, its draws that blend by the
    /// destination's alpha blending by the alpha the world's frame holds where no shore lies. The
    /// picture is the host's until it calls [`Self::end_frame`].
    pub fn draw_laid_over(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> wgpu::Texture {
        self.draw_as(frame, width, height, load, Target::Coverage)
    }

    /// Draw `frame` writing `target` (the world's frame, or a scene laid over the interface).
    #[allow(clippy::too_many_lines)] // one pass, in the world's drawing order
    fn draw_as(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
        target: Target,
    ) -> wgpu::Texture {
        let mut lap = Lap::start();
        self.take_textures(frame.textures);
        let cam = &frame.camera;
        let (h, p) = (cam.heading.to_radians(), cam.pitch.to_radians());
        let forward = [
            math::sinf(h) * math::cosf(p),
            math::cosf(h) * math::cosf(p),
            math::sinf(p),
        ];
        let eye = [cam.position.x, cam.position.y, cam.position.z];
        #[allow(clippy::cast_precision_loss)]
        let aspect = width as f32 / height as f32;
        let vp = view_proj(
            eye,
            forward,
            cam.roll,
            cam.fov_y,
            cam.orthographic,
            aspect,
            cam.near,
            cam.far,
        );
        // Seen from inside, only what the doorways show at the moment: the cells and objects the
        // walk from the camera's cell reaches, the land and its water when a view out was found,
        // the sky with it or with a cell open to it.
        // D2-11042
        let sight = frame.sight(aspect);
        let outdoors = sight.as_ref().is_none_or(Sight::outdoors);
        let sky_shown = sight.as_ref().is_none_or(Sight::sky);
        let shown = |b: &&ObjectBatch| sight.as_ref().is_none_or(|s| s.shows(b));
        let mut frame_u = FrameUniforms {
            view_proj: mat(&vp),
            eye: [eye[0], eye[1], eye[2], 1.0],
            forward: [forward[0], forward[1], forward[2], 0.0],
            alpha_fog: {
                let [start, end] = frame.frill_fade;
                let span = if end > start {
                    1.0 / (end - start)
                } else {
                    0.0
                };
                [start, end, span, 0.0]
            },
            shadow_uv: frame
                .shadow
                .as_ref()
                .map_or(Mat4::IDENTITY.0, |s| s.texture_coordinates().0),
            cloud: frame.clouds.map_or([0.0; 4], |c| {
                let off = c.offset(frame.uptime);
                [c.scale, off[0], off[1], 1.0]
            }),
            terrain: [
                if frame.shadow.is_some() { 1.0 } else { 0.0 },
                0.0,
                0.0,
                0.0,
            ],
            #[allow(clippy::cast_precision_loss)]
            viewport: [width as f32, height as f32, 0.0, 0.0],
            ..FrameUniforms::zeroed()
        };
        // Each draw is made under one of the frame's lights: its own (the camera's cell's), the
        // outdoors' (inside, for the sky, the landscape, the water and what stands outdoors; the
        // frame's own when it has no other), and the sky's (the outdoors' with no fog).
        let mut out_u = frame_u;
        frame_u.light(&frame.light);
        out_u.light(frame.outside.as_ref().unwrap_or(&frame.light));
        let sun = frame_u.sun_color;
        let out_sun = out_u.sun_color;
        let mut sky_u = out_u;
        sky_u.fog[3] = 0.0;
        self.uniforms.begin();
        self.frame_uniforms.begin();
        self.meshes.begin_draw();
        let frame_group = self.frame_uniforms.take(
            &self.device,
            &self.frame_layout,
            bytemuck::bytes_of(&frame_u),
        );
        let out_group =
            self.frame_uniforms
                .take(&self.device, &self.frame_layout, bytemuck::bytes_of(&out_u));
        let sky_group =
            self.frame_uniforms
                .take(&self.device, &self.frame_layout, bytemuck::bytes_of(&sky_u));
        let mut looked_u = frame_u;
        if let Some(l) = &frame.looked_light {
            looked_u.light(l);
        }
        let looked_sun = looked_u.sun_color;
        let looked_group = self.frame_uniforms.take(
            &self.device,
            &self.frame_layout,
            bytemuck::bytes_of(&looked_u),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let ms = lap.lap();
        self.note_work(|w| {
            w.frames += 1;
            w.ms.setup += ms;
        });
        // The sun's shadow map first: the near terrain is multiplied by it.
        let shadow_view = frame
            .shadow
            .as_ref()
            .filter(|_| outdoors)
            .map(|s| Arc::new(self.render_shadow_map(s, &out_u, &mut encoder, load)));
        let shadow_group = match &shadow_view {
            Some(view) => self.shadow_group(view),
            None => self.shadow_group(&self.white),
        };
        let ms = lap.lap();
        self.note_work(|w| w.ms.shadow += ms);
        let body_spots: std::cell::RefCell<HashMap<[u32; 3], usize>> =
            std::cell::RefCell::new(HashMap::new());
        let cloud_view = frame.clouds.map(|c| self.texture(c.picture, load));

        // The frame is drawn multisampled and resolved into the picture read back.
        let (color, color_view) = self.target(
            "color",
            (width, height, 1),
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let multi_view = (self.samples > 1).then(|| {
            self.target(
                "multisampled color",
                (width, height, self.samples),
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
            .1
        });
        let (_, depth_view) = self.target(
            "depth",
            (width, height, self.samples),
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );

        // Prepare every draw: buffers, uniforms, textures, pipelines.
        struct Draw {
            pipeline: Pipe,
            vertices: cache::Slice,
            posed: Option<cache::Slice>,
            indices: cache::Slice,
            index_count: u32,
            index_format: wgpu::IndexFormat,
            draw_group: pool::Slot,
            texture_group: wgpu::BindGroup,
            env: Env,
            /// It is seen in the frame's own picture.
            shown: bool,
            /// It is seen in the water's reflection.
            reflected: bool,
            /// How the reflection draws it when that differs: nearer the edge of its reach there,
            /// it is fainter.
            faint: Option<(PassKey, pool::Slot)>,
            /// It stands at or under the water, so the reflection shows it only past the water.
            below: bool,
            /// Its culling, for the water's depth laid in the reflection.
            cull: u32,
        }
        enum Pipe {
            Terrain(&'static str),
            Mesh(PassKey),
            Water(PassKey),
        }
        let identity = self.uniforms.take(
            &self.device,
            &self.draw_layout,
            bytemuck::bytes_of(&Self::draw_uniforms(
                &dummy_layer(),
                &Mat4::IDENTITY,
                auberean_world::color::Color::WHITE,
                sun,
                &PointLights::NONE,
            )),
        );
        let _ = &identity;
        let white_group = self.texture_group([(0, false); 3], load);
        // The sky objects' lens flares: last frame's look answered, each flare faded by it and
        // laid along the line through the screen's middle, and a marker at each object in front
        // of the camera to look for it again.
        self.read_flare_scan();
        let mut flare_verts: Vec<MeshGpuVertex> = Vec::new();
        let mut flare_runs: Vec<(wgpu::BindGroup, u32)> = Vec::new();
        let mut markers: Vec<(usize, u32)> = Vec::new();
        let looking = self.flares_seen && self.flare_scan.is_none();
        for f in frame.flares.iter().filter(|_| sky_shown) {
            let c = clip_of(&vp, [f.at.x, f.at.y, f.at.z]);
            let front = c[3] > 0.0;
            let slot = self.flares.entry(f.key).or_default();
            if !front {
                slot.seen = false;
            }
            let level = slot.fade.advance(frame.uptime, f.fade_speed, slot.seen);
            if !front {
                continue;
            }
            let at = (c[0] / c[3], c[1] / c[3]);
            #[allow(clippy::cast_precision_loss)]
            let shape = width as f32 / height.max(1) as f32;
            for q in flare_quads(&f.points, at, shape, level) {
                let start = u32::try_from(flare_verts.len()).unwrap_or(u32::MAX);
                let [l, b, r, t] = q.rect;
                let color = [1.0, 1.0, 1.0, q.alpha];
                for (x, y, u, v) in [
                    (l, t, 0.0, 0.0),
                    (l, b, 0.0, 1.0),
                    (r, b, 1.0, 1.0),
                    (l, t, 0.0, 0.0),
                    (r, b, 1.0, 1.0),
                    (r, t, 1.0, 0.0),
                ] {
                    flare_verts.push(MeshGpuVertex::screen([x, y, 0.0], [u, v], color));
                }
                let group = self.texture_group([(q.texture, true), (0, false), (0, false)], load);
                flare_runs.push((group, start));
            }
            if looking {
                let start = u32::try_from(flare_verts.len()).unwrap_or(u32::MAX);
                for (px, py) in auberean_draw::flare::seen_marker(at, width, height) {
                    flare_verts.push(MeshGpuVertex::screen(
                        [px, py, FLARE_MARKER_DEPTH],
                        [0.0, 0.0],
                        [0.0; 4],
                    ));
                }
                markers.push((f.key, start));
            }
        }
        if !flare_verts.is_empty() {
            self.build_flare_pipelines();
        }
        let flare_buffer = (!flare_verts.is_empty()).then(|| {
            self.meshes
                .passing_vertices
                .put(&self.device, bytemuck::cast_slice(&flare_verts))
        });
        let flare_look = (!markers.is_empty()).then(|| self.flare_look(markers.len()));
        // A frill takes its color from the shadow map about its spot (the map in its third
        // texture slot, which a frill's material leaves free), one group per pair of pictures a
        // frame.
        let mut shaded_groups: HashMap<[(u32, bool); 2], wgpu::BindGroup> = HashMap::new();
        // `under` is the light the batches are drawn under: the sky's or the outdoors', or (as
        // `Env::Own`) each batch's own choice.
        let mut objects = |batches: Vec<&ObjectBatch>,
                           under: Env,
                           this: &mut Self,
                           draws: &mut Vec<Draw>,
                           load: &mut dyn FnMut(u32) -> Option<Image>,
                           writes: Target| {
            for b in batches {
                if b.indices.is_empty() {
                    continue;
                }
                let env = match under {
                    Env::Own if b.outside => Env::Outside,
                    Env::Own if frame.looked_light.is_some() && b.cull.is_looked_in() => {
                        Env::Looked
                    }
                    e => e,
                };
                let sky = env == Env::Sky;
                let sun = match env {
                    Env::Own => sun,
                    Env::Looked => looked_sun,
                    _ => out_sun,
                };
                if !b.drawn() {
                    continue;
                }
                let faded = b.layer.faded(b.tint.a, b.far);
                let mut key = Self::object_key(&b.layer, sky, writes);
                key.depth_write = key.depth_write && faded.depth_write;
                key.posed = b.posed.is_some();
                this.mesh_pipeline(key);
                let bound = Self::bound_stages(&b.layer);
                let textures = bound.map(|s| {
                    (
                        s.map_or(0, |s| s.texture),
                        s.is_some_and(|s| s.clamp_u || s.clamp_v),
                    )
                });
                let mut uniforms =
                    Self::draw_uniforms(&b.layer, &b.transform, b.tint, sun, &b.points);
                uniforms.params[1] = faded.alpha_ref.unwrap_or(-1.0);
                if let (Some(at), Some(_)) = (b.ground, &shadow_view) {
                    let mut spots = body_spots.borrow_mut();
                    let next = spots.len();
                    let k = *spots
                        .entry([at.x.to_bits(), at.y.to_bits(), at.z.to_bits()])
                        .or_insert(next);
                    #[allow(clippy::cast_precision_loss)]
                    if k < BODY_SPOTS.0 * BODY_SPOTS.1 {
                        uniforms.shade = [k as f32, 0.0, 0.0, 2.0];
                    }
                }
                // Only a layer that reads its vertices' color shows it (none of the frills'
                // materials in the data does). The shadow map's texels are read where they lie
                // on it, a texel off the map read as none.
                // D2-10925, D2-10970, D2-09968
                let reads = b.layer.switches & auberean_draw::material::switch::VERTEX_DIFFUSE != 0;
                let shade = match (b.shade, &shadow_view, bound[2]) {
                    (Some(at), Some(map), None) if reads => Some((at, map)),
                    _ => None,
                };
                let texture_group = match shade {
                    Some((at, map)) => {
                        uniforms.shade = [at.x, at.y, at.z, 1.0];
                        let pair = [textures[0], textures[1]];
                        match shaded_groups.get(&pair) {
                            Some(g) => g.clone(),
                            None => {
                                let views = pair.map(|(id, _)| this.texture(id, load));
                                let sampler =
                                    |clamp: bool| if clamp { &this.clamp } else { &this.wrap };
                                let g = this.bind_three(&[
                                    (&views[0], sampler(pair[0].1)),
                                    (&views[1], sampler(pair[1].1)),
                                    (map, &this.clamp),
                                ]);
                                shaded_groups.insert(pair, g.clone());
                                g
                            }
                        }
                    }
                    None => this.texture_group(textures, load),
                };
                let draw_group = this.uniforms.take(
                    &this.device,
                    &this.draw_layout,
                    bytemuck::bytes_of(&uniforms),
                );
                let (vertices, indices) =
                    this.meshes.buffers(&this.device, &b.vertices, &b.indices);
                let posed = b
                    .posed
                    .as_ref()
                    .map(|list| this.meshes.posed_buffer(&this.device, list.shared()));
                // The reflection gathers what stands on the land alone, no cell's.
                // D2-11030
                let in_reflection = b.reflected(frame.camera.position)
                    && !matches!(
                        b.cull,
                        auberean_draw::scene::Cull::Cell(_)
                            | auberean_draw::scene::Cull::InCell { .. }
                            | auberean_draw::scene::Cull::Looked { .. }
                            | auberean_draw::scene::Cull::LookedIn { .. }
                            | auberean_draw::scene::Cull::Body { .. }
                    );
                let above = frame
                    .reflection
                    .is_none_or(|r| sky || b.step.is_some() || b.stands_above(r.height));
                // The reflection reaches its own edge, fading there as the frame does at its own.
                // D2-06658
                let faint = frame.reflection.and_then(|_| {
                    let a = b.reflected_alpha(frame.camera.position);
                    if !in_reflection || (a - b.tint.a).abs() < 0.001 {
                        return None;
                    }
                    let tint = auberean_world::color::Color { a, ..b.tint };
                    let faded = b.layer.faded(a, b.far);
                    let mut key = key;
                    key.depth_write =
                        Self::object_key(&b.layer, sky, writes).depth_write && faded.depth_write;
                    this.mesh_pipeline(key);
                    let mut u = Self::draw_uniforms(&b.layer, &b.transform, tint, sun, &b.points);
                    u.params[1] = faded.alpha_ref.unwrap_or(-1.0);
                    u.shade = uniforms.shade;
                    Some((
                        key,
                        this.uniforms
                            .take(&this.device, &this.draw_layout, bytemuck::bytes_of(&u)),
                    ))
                });
                let below = in_reflection && !above;
                if below {
                    let key = faint.as_ref().map_or(key, |f| f.0);
                    this.mesh_pipeline(PassKey { past: true, ..key });
                }
                draws.push(Draw {
                    pipeline: Pipe::Mesh(key),
                    vertices,
                    posed,
                    indices,
                    index_count: u32::try_from(b.indices.len()).unwrap_or(0),
                    index_format: wgpu::IndexFormat::Uint16,
                    draw_group,
                    texture_group,
                    env,
                    shown: b.shown(),
                    reflected: in_reflection && above,
                    faint,
                    below,
                    cull: b.layer.cull,
                });
            }
        };
        let mut sky_draws = Vec::new();
        let none: &[ObjectBatch] = &[];
        let (sky_before, sky_after) = if sky_shown {
            (&frame.sky_before[..], &frame.sky_after[..])
        } else {
            (none, none)
        };
        let (terrain, water, shore, foam) = if outdoors {
            (
                &frame.terrain[..],
                &frame.water[..],
                &frame.shore[..],
                &frame.foam[..],
            )
        } else {
            (&[][..], none, none, none)
        };
        objects(
            opaque_first(sky_before),
            Env::Sky,
            self,
            &mut sky_draws,
            load,
            target,
        );
        let mut world_draws = Vec::new();
        let terrain_draw = |b: &TerrainBatch,
                            name: &'static str,
                            texture_group: wgpu::BindGroup,
                            this: &mut Self,
                            draws: &mut Vec<Draw>| {
            if b.indices.is_empty() {
                return;
            }
            let (vertices, indices) =
                this.meshes
                    .land_buffers(&this.device, &b.vertices, &b.indices);
            let draw_group = this.uniforms.take(
                &this.device,
                &this.draw_layout,
                bytemuck::bytes_of(&Self::draw_uniforms(
                    &dummy_layer(),
                    &Mat4::IDENTITY,
                    auberean_world::color::Color::WHITE,
                    out_sun,
                    &PointLights::NONE,
                )),
            );
            // The landscape is drawn under the outdoors' light.
            draws.push(Draw {
                pipeline: Pipe::Terrain(name),
                vertices,
                posed: None,
                indices,
                index_count: u32::try_from(b.indices.len()).unwrap_or(0),
                index_format: wgpu::IndexFormat::Uint32,
                draw_group,
                texture_group,
                env: Env::Outside,
                shown: true,
                reflected: true,
                faint: None,
                below: false,
                cull: d3d::CULL_NONE,
            });
        };
        let (near, far): (Vec<&TerrainBatch>, Vec<&TerrainBatch>) =
            terrain.iter().partition(|b| !b.far);
        let clouds = cloud_view.clone().unwrap_or_else(|| self.white.clone());
        for b in near.iter().filter(|b| b.layer == 0) {
            let ground = self.texture(b.ground, load);
            let tg = self.bind_textures(&ground, &self.wrap, &self.white, &self.clamp);
            terrain_draw(b, "base", tg, self, &mut world_draws);
        }
        for b in near.iter().filter(|b| b.layer > 0) {
            let ground = self.texture(b.ground, load);
            let mask = match b.mask {
                Some(m) => self.texture(m, load),
                None => self.white.clone(),
            };
            let tg = self.bind_textures(&ground, &self.wrap, &mask, &self.clamp);
            terrain_draw(b, "mask", tg, self, &mut world_draws);
        }
        // With the shadow map, the near terrain's layers are drawn without fog; then each square
        // once more, multiplied by the shadow map and the clouds, then fogged.
        if let Some(shadow) = &shadow_view {
            for name in ["shade", "fog"] {
                for b in near.iter().filter(|b| b.layer == 0) {
                    let tg = self.bind_textures(shadow, &self.clamp, &clouds, &self.wrap);
                    terrain_draw(b, name, tg, self, &mut world_draws);
                }
            }
        }
        for b in &far {
            let ground = self.texture(b.ground, load);
            let tg = self.bind_textures(&ground, &self.wrap, &clouds, &self.wrap);
            terrain_draw(b, "far", tg, self, &mut world_draws);
        }
        // The land as the water's reflection shows it.
        let mut land_draws = Vec::new();
        if let (true, Some(land)) = (frame.reflection.is_some(), &frame.reflection_land) {
            let ground = self.texture(land.ground, load);
            let tg = self.bind_textures(&ground, &self.wrap, &clouds, &self.wrap);
            terrain_draw(land, "far", tg, self, &mut land_draws);
        }
        // Each step's objects under the frame's own light first, then those standing outdoors;
        // the steps of the world's objects farthest first.
        let mut cell_draws = Vec::new();
        objects(
            in_order(
                in_pass(&frame.objects, Pass::Cells)
                    .into_iter()
                    .filter(shown)
                    .collect(),
                Pass::Cells,
                frame.camera.position,
            ),
            Env::Own,
            self,
            &mut cell_draws,
            load,
            target,
        );
        let mut opaque_draws = Vec::new();
        objects(
            in_order(
                in_pass(&frame.objects, Pass::Opaque)
                    .into_iter()
                    .filter(shown)
                    .collect(),
                Pass::Opaque,
                frame.camera.position,
            ),
            Env::Own,
            self,
            &mut opaque_draws,
            load,
            target,
        );
        // The decals laid on the ground, after the landscape and the cells.
        let mut decal_draws = Vec::new();
        objects(
            frame.decals.iter().collect(),
            Env::Own,
            self,
            &mut decal_draws,
            load,
            target,
        );
        // The objects that cast volumes of shadow: their opaque draws after the volumes.
        let mut caster_draws = Vec::new();
        objects(
            in_order(
                in_pass(&frame.objects, Pass::Casters)
                    .into_iter()
                    .filter(shown)
                    .collect(),
                Pass::Casters,
                frame.camera.position,
            ),
            Env::Own,
            self,
            &mut caster_draws,
            load,
            target,
        );
        // The volumes of shadow, each in its draw's frame, and the darkening by their strength.
        let volume_draws: Vec<(cache::Slice, cache::Slice, u32, pool::Slot)> = frame
            .volumes
            .volumes
            .iter()
            .filter(|v| !v.indices.is_empty())
            .map(|v| {
                let group = self.uniforms.take(
                    &self.device,
                    &self.draw_layout,
                    bytemuck::bytes_of(&Self::draw_uniforms(
                        &dummy_layer(),
                        &v.transform,
                        auberean_world::color::Color::WHITE,
                        sun,
                        &PointLights::NONE,
                    )),
                );
                (
                    self.meshes
                        .passing_vertices
                        .put(&self.device, bytemuck::cast_slice(v.vertices.as_slice())),
                    self.meshes
                        .passing_indices
                        .put(&self.device, bytemuck::cast_slice(v.indices.as_slice())),
                    u32::try_from(v.indices.len()).unwrap_or(0),
                    group,
                )
            })
            .collect();
        if !volume_draws.is_empty() {
            self.build_volume_pipelines();
        }
        let darkening = self.uniforms.take(
            &self.device,
            &self.draw_layout,
            bytemuck::bytes_of(&Self::draw_uniforms(
                &dummy_layer(),
                &Mat4::IDENTITY,
                frame.volumes.strength,
                sun,
                &PointLights::NONE,
            )),
        );
        let mut translucent_draws = Vec::new();
        objects(
            in_order(
                in_pass(&frame.objects, Pass::Translucent)
                    .into_iter()
                    .filter(shown)
                    .collect(),
                Pass::Translucent,
                frame.camera.position,
            ),
            Env::Own,
            self,
            &mut translucent_draws,
            load,
            target,
        );
        // The frills, after the water and before the objects seen through; the particle systems
        // after every object.
        let mut frill_draws = Vec::new();
        objects(
            in_order(
                in_pass(&frame.objects, Pass::Frills)
                    .into_iter()
                    .filter(shown)
                    .collect(),
                Pass::Frills,
                frame.camera.position,
            ),
            Env::Own,
            self,
            &mut frill_draws,
            load,
            target,
        );
        let mut particle_draws = Vec::new();
        objects(
            in_order(
                in_pass(&frame.objects, Pass::Particles)
                    .into_iter()
                    .filter(shown)
                    .collect(),
                Pass::Particles,
                frame.camera.position,
            ),
            Env::Own,
            self,
            &mut particle_draws,
            load,
            target,
        );
        // The streaks objects' moving parts leave, after the particle systems.
        let mut streak_draws = Vec::new();
        objects(
            in_pass(&frame.objects, Pass::Streaks),
            Env::Own,
            self,
            &mut streak_draws,
            load,
            target,
        );
        // The water's step, after the opaque objects and before those seen through: the shores'
        // strips into the frame's alpha alone (how much of the water shows near the shore), the
        // water's own material, its rippled reflection over it when the frame has one, and the
        // foam along the shores.
        // D2-10975, D2-10902, D2-10951
        let reflection = frame.reflection.filter(|_| !water.is_empty());
        let mut water_draws = Vec::new();
        objects(
            shore.iter().collect(),
            Env::Outside,
            self,
            &mut water_draws,
            load,
            Target::FrameAlpha,
        );
        objects(
            opaque_first(water),
            Env::Outside,
            self,
            &mut water_draws,
            load,
            target,
        );
        // The water again, only to mark its depth in the reflection.
        // D2-11030
        let mut water_marks = Vec::new();
        if reflection.is_some() {
            objects(
                opaque_first(water),
                Env::Outside,
                self,
                &mut water_marks,
                load,
                target,
            );
            for d in &mut water_marks {
                let key = PassKey {
                    src: d3d::BLEND_ZERO,
                    dst: d3d::BLEND_ONE,
                    depth_test: true,
                    depth_write: true,
                    cull: d.cull,
                    target,
                    past: true,
                    posed: false,
                };
                self.mesh_pipeline(key);
                d.pipeline = Pipe::Mesh(key);
            }
        }
        let mut foam_draws = Vec::new();
        objects(
            foam.iter().collect(),
            Env::Outside,
            self,
            &mut foam_draws,
            load,
            target,
        );
        let mut late = Vec::new();
        objects(
            opaque_first(sky_after),
            Env::Sky,
            self,
            &mut late,
            load,
            target,
        );

        let outdoors = frame.outside.as_ref().unwrap_or(&frame.light);
        // D2-10936
        let skip_sky = (outdoors.fog_sky - 1.0).abs() < 0.0002;
        let veil_textures = self.texture_group([(0, false); 3], load);
        // One pass of the scene, its steps in the frame's order: the sky, its veil, the
        // landscape, the opaque objects, the water, the objects seen through, the late sky.
        // `groups` holds the frame's lights: its own, the outdoors' and the sky's.
        let record = |pass: &mut wgpu::RenderPass<'_>,
                      this: &Self,
                      groups: [&pool::Slot; 4],
                      water: &[Draw],
                      shadows: bool,
                      reflecting: bool| {
            // What stays the same from one draw to the next is not bound again.
            let mut bound = Bound::default();
            let run = |pass: &mut wgpu::RenderPass<'_>, bound: &mut Bound, d: &Draw| {
                let faint = d.faint.as_ref().filter(|_| reflecting);
                let pipeline = match (&d.pipeline, faint) {
                    (_, Some((k, _))) => &this.mesh_pipelines[k],
                    (Pipe::Terrain(n), None) => &this.terrain_pipelines[n],
                    (Pipe::Mesh(k), None) => &this.mesh_pipelines[k],
                    (Pipe::Water(k), None) => &this.water_pipelines[k],
                };
                if !std::ptr::eq(bound.pipeline, pipeline) {
                    pass.set_pipeline(pipeline);
                    bound.pipeline = pipeline;
                }
                if bound.env != Some(d.env as usize) {
                    pass.set_bind_group(
                        0,
                        &groups[d.env as usize].group,
                        &[groups[d.env as usize].offset],
                    );
                    bound.env = Some(d.env as usize);
                }
                let slot: &pool::Slot = faint.map_or(&d.draw_group, |f| &f.1);
                if bound.uniforms.as_ref() != Some(slot) {
                    pass.set_bind_group(1, &slot.group, &[slot.offset]);
                    bound.uniforms = Some(slot.clone());
                }
                if bound.textures.as_ref() != Some(&d.texture_group) {
                    pass.set_bind_group(2, &d.texture_group, &[]);
                    bound.textures = Some(d.texture_group.clone());
                }
                if bound.vertices.as_ref() != Some(&d.vertices) {
                    pass.set_vertex_buffer(0, d.vertices.get());
                    bound.vertices = Some(d.vertices.clone());
                }
                if let Some(posed) = d.posed.as_ref().filter(|p| bound.posed.as_ref() != Some(p)) {
                    pass.set_vertex_buffer(1, posed.get());
                    bound.posed = Some(posed.clone());
                }
                if bound.indices.as_ref() != Some(&d.indices) {
                    pass.set_index_buffer(d.indices.get(), d.index_format);
                    bound.indices = Some(d.indices.clone());
                }
                pass.draw_indexed(0..d.index_count, 0, 0..1);
                this.note_work(|w| w.draws += 1);
            };
            for step in PASSES {
                let draws: &[Draw] = match step {
                    Pass::Sky if !skip_sky => &sky_draws,
                    Pass::Sky => &[],
                    Pass::Veil => {
                        // The veil is the sky's: no sky drawn, no veil.
                        // D2-10940
                        if sky_shown && outdoors.fog_sky > 0.0 {
                            if let Some(veil) = &this.veil {
                                pass.set_pipeline(veil);
                                pass.set_bind_group(
                                    0,
                                    &groups[Env::Outside as usize].group,
                                    &[groups[Env::Outside as usize].offset],
                                );
                                pass.set_bind_group(1, &identity.group, &[identity.offset]);
                                pass.set_bind_group(2, &veil_textures, &[]);
                                pass.draw(0..3, 0..1);
                                bound = Bound::default();
                            }
                        }
                        &[]
                    }
                    Pass::Terrain if reflecting && !land_draws.is_empty() => &land_draws,
                    Pass::Terrain => &world_draws,
                    Pass::Cells => &cell_draws,
                    Pass::Decals => &decal_draws,
                    Pass::Opaque => &opaque_draws,
                    Pass::Volumes => {
                        if let (true, false, Some((count, darken))) =
                            (shadows, volume_draws.is_empty(), &this.volume_pipelines)
                        {
                            // The reference the darkening compares with, set first under its
                            // pipeline (the only one that reads it, so the only one the device
                            // is told it under), before the counting draws.
                            pass.set_pipeline(darken);
                            pass.set_stencil_reference(0);
                            pass.set_pipeline(count);
                            pass.set_bind_group(
                                0,
                                &groups[Env::Own as usize].group,
                                &[groups[Env::Own as usize].offset],
                            );
                            pass.set_bind_group(2, &white_group, &[]);
                            for (vertices, indices, n, group) in &volume_draws {
                                pass.set_bind_group(1, &group.group, &[group.offset]);
                                pass.set_vertex_buffer(0, vertices.get());
                                pass.set_index_buffer(indices.get(), wgpu::IndexFormat::Uint16);
                                pass.draw_indexed(0..*n, 0, 0..1);
                            }
                            pass.set_pipeline(darken);
                            pass.set_bind_group(1, &darkening.group, &[darkening.offset]);
                            pass.draw(0..3, 0..1);
                            bound = Bound::default();
                        }
                        &[]
                    }
                    Pass::Casters => &caster_draws,
                    Pass::Water => water,
                    Pass::Frills => &frill_draws,
                    Pass::Translucent => &translucent_draws,
                    // D2-11030
                    Pass::Particles | Pass::Streaks if reflecting => &[],
                    Pass::Particles => &particle_draws,
                    Pass::Streaks => &streak_draws,
                    Pass::LateSky => &late,
                };
                for d in draws {
                    if if reflecting { d.reflected } else { d.shown } {
                        run(pass, &mut bound, d);
                    }
                }
            }
        };
        let ms = lap.lap();
        self.note_work(|w| w.ms.prepare += ms);
        // Each body reads the sun's share at its spot from one texel: the 25 of the shadow map
        // about it are read once a frame, where each of its vertices would read them.
        // D2-10925
        let shadow_group = if body_spots.borrow().is_empty() {
            shadow_group
        } else {
            self.draw_body_spots(
                &body_spots.borrow(),
                &shadow_group,
                &identity,
                &out_u,
                &mut encoder,
            )
        };
        let ms = lap.lap();
        self.note_work(|w| w.ms.spots += ms);
        // The reflection: the same scene from the camera mirrored about the water plane (its
        // height mirrored, its pitch turned over): all the land it can see, what lies under the
        // water too, the objects standing above the plane, the sky round the mirrored camera.
        // D2-11030, D2-10552, D2-10553, D2-11031
        if let Some(r) = reflection {
            let h = r.height;
            let m_eye = [eye[0], eye[1], 2.0 * h - eye[2]];
            let m_forward = [forward[0], forward[1], -forward[2]];
            let m_vp = view_proj(
                m_eye,
                m_forward,
                -cam.roll,
                cam.fov_y,
                cam.orthographic,
                aspect,
                cam.near,
                cam.far,
            );
            let mirrored = |u: &FrameUniforms| {
                let mut m = *u;
                m.view_proj = mat(&m_vp);
                m.eye = [m_eye[0], m_eye[1], m_eye[2], 1.0];
                m.forward = [m_forward[0], m_forward[1], m_forward[2], 0.0];
                m
            };
            let m_frame = mirrored(&frame_u);
            let m_out = mirrored(&out_u);
            let mut m_sky = sky_u;
            // the sky's objects stand round the camera: move them to the mirrored one
            let lift = Mat4([
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, m_eye[2] - eye[2], 1.0],
            ]);
            m_sky.view_proj = mat(&lift.then(&m_vp));
            m_sky.eye = m_frame.eye;
            m_sky.forward = m_frame.forward;
            let m_looked = mirrored(&looked_u);
            let [m_frame_group, m_out_group, m_sky_group, m_looked_group] =
                [&m_frame, &m_out, &m_sky, &m_looked].map(|u| {
                    self.frame_uniforms.take(
                        &self.device,
                        &self.frame_layout,
                        bytemuck::bytes_of(u),
                    )
                });
            // The reflection is drawn into a picture of its own, 512 texels square whatever the
            // screen's size, with a depth of its own the same size, cleared as the frame is; the
            // camera keeps the screen's shape, so the picture lies over the screen as it stands.
            let square = REFLECTION_SIZE;
            let (_, refl_view) = self.target(
                "reflection",
                (square, square, 1),
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            );
            let refl_multi_view = (self.samples > 1).then(|| {
                self.target(
                    "multisampled reflection",
                    (square, square, self.samples),
                    COLOR_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                )
                .1
            });
            let (_, refl_depth_view) = self.target(
                "reflection depth",
                (square, square, self.samples),
                DEPTH_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            );
            {
                let mut pass = begin_pass(
                    &mut encoder,
                    "reflection",
                    refl_multi_view.as_ref().unwrap_or(&refl_view),
                    refl_multi_view.as_ref().map(|_| &refl_view),
                    &refl_depth_view,
                    wgpu::Color::BLACK,
                    None,
                );
                pass.set_bind_group(3, &shadow_group, &[]);
                record(
                    &mut pass,
                    self,
                    [&m_frame_group, &m_out_group, &m_sky_group, &m_looked_group],
                    &[],
                    false,
                    true,
                );
            }
            // Past the water: its depth marked alone, the farthest of it where it lies, into a
            // depth emptied for it; then the objects standing at or under it, which show only
            // beyond that, seen from the mirrored camera.
            // D2-11030
            let below: Vec<&Draw> = [&opaque_draws, &caster_draws, &translucent_draws]
                .into_iter()
                .flatten()
                .filter(|d| d.below)
                .collect();
            if !below.is_empty() {
                let view = refl_multi_view.as_ref().unwrap_or(&refl_view);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("past the water"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: refl_multi_view.as_ref().map(|_| &refl_view),
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &refl_depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Store,
                        }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_blend_constant(wgpu::Color {
                    r: FRAME_ALPHA,
                    g: FRAME_ALPHA,
                    b: FRAME_ALPHA,
                    a: FRAME_ALPHA,
                });
                pass.set_bind_group(3, &shadow_group, &[]);
                let groups = [&m_frame_group, &m_out_group, &m_sky_group, &m_looked_group];
                let put =
                    |pass: &mut wgpu::RenderPass<'_>, d: &Draw, key: PassKey, g: &pool::Slot| {
                        pass.set_pipeline(&self.mesh_pipelines[&key]);
                        pass.set_bind_group(
                            0,
                            &groups[d.env as usize].group,
                            &[groups[d.env as usize].offset],
                        );
                        pass.set_bind_group(1, &g.group, &[g.offset]);
                        pass.set_bind_group(2, &d.texture_group, &[]);
                        pass.set_vertex_buffer(0, d.vertices.get());
                        if let Some(posed) = &d.posed {
                            pass.set_vertex_buffer(1, posed.get());
                        }
                        pass.set_index_buffer(d.indices.get(), d.index_format);
                        pass.draw_indexed(0..d.index_count, 0, 0..1);
                    };
                for d in &water_marks {
                    if let Pipe::Mesh(k) = d.pipeline {
                        put(&mut pass, d, k, &d.draw_group);
                    }
                }
                for d in below {
                    let (key, group) = match (&d.faint, &d.pipeline) {
                        (Some((k, g)), _) => (*k, g),
                        (None, Pipe::Mesh(k)) => (*k, &d.draw_group),
                        _ => continue,
                    };
                    put(&mut pass, d, PassKey { past: true, ..key }, group);
                }
            }
            // Over each water surface (its own material already drawn), the reflection where it
            // lies on the screen, its coordinates pushed by the ripples under it, blended by the
            // alpha the frame holds: half the reflection, half what is there.
            let bump = self.bump_picture();
            // the ripples push the reflection only at the reflections' highest setting
            let push = if r.ripples { RIPPLE_PUSH } else { 0.0 };
            // the ripples slide a hundredth of a repeat a second and are drawn at half size
            let slide = {
                let s = 0.01 * frame.uptime;
                #[allow(clippy::cast_possible_truncation)]
                let f = (s - s.floor()) as f32;
                f
            };
            // The ripples are first laid over the water's surfaces into a picture of their own,
            // 512 texels square from the frame's own camera, which holds no value below zero; the
            // reflection reads its push from that picture where it lies on the screen.
            // D2-10598, D2-10587
            let ripple_view = if r.ripples {
                let (_, view) = self.target(
                    "ripples",
                    (square, square, 1),
                    COLOR_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                );
                let multi = (self.samples > 1).then(|| {
                    self.target(
                        "multisampled ripples",
                        (square, square, self.samples),
                        COLOR_FORMAT,
                        wgpu::TextureUsages::RENDER_ATTACHMENT,
                    )
                    .1
                });
                let (_, depth) = self.target(
                    "ripples depth",
                    (square, square, self.samples),
                    DEPTH_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                );
                let key = PassKey {
                    src: d3d::BLEND_ONE,
                    dst: d3d::BLEND_ZERO,
                    depth_test: true,
                    depth_write: true,
                    cull: d3d::CULL_NONE,
                    target: Target::Frame,
                    past: false,
                    posed: false,
                };
                if self.ripple_pipeline.is_none() {
                    self.ripple_pipeline =
                        Some(self.pipeline("vs_mesh", "fs_ripple", Self::mesh_layout(), key));
                }
                let textures = self.bind_three(&[
                    (&bump, &self.wrap),
                    (&self.white, &self.wrap),
                    (&self.white, &self.wrap),
                ]);
                let mut surfaces = Vec::new();
                for b in water {
                    if b.indices.is_empty() {
                        continue;
                    }
                    let mut u =
                        Self::draw_uniforms(&b.layer, &b.transform, b.tint, out_sun, &b.points);
                    u.uv_sets = [0, 0, 0, 0];
                    u.uv_rows[0] = [0.5, 0.0, 0.5 * slide, 0.0];
                    u.uv_rows[1] = [0.0, 0.5, 0.5 * slide, 0.0];
                    let group =
                        self.uniforms
                            .take(&self.device, &self.draw_layout, bytemuck::bytes_of(&u));
                    let (vertices, indices) =
                        self.meshes.buffers(&self.device, &b.vertices, &b.indices);
                    surfaces.push((
                        group,
                        vertices,
                        indices,
                        u32::try_from(b.indices.len()).unwrap_or(0),
                    ));
                }
                if let Some(pipeline) = &self.ripple_pipeline {
                    let mut pass = begin_pass(
                        &mut encoder,
                        "ripples",
                        multi.as_ref().unwrap_or(&view),
                        multi.as_ref().map(|_| &view),
                        &depth,
                        wgpu::Color::BLACK,
                        None,
                    );
                    pass.set_pipeline(pipeline);
                    pass.set_bind_group(0, &out_group.group, &[out_group.offset]);
                    pass.set_bind_group(2, &textures, &[]);
                    pass.set_bind_group(3, &shadow_group, &[]);
                    for (group, vertices, indices, count) in &surfaces {
                        pass.set_bind_group(1, &group.group, &[group.offset]);
                        pass.set_vertex_buffer(0, vertices.get());
                        pass.set_index_buffer(indices.get(), wgpu::IndexFormat::Uint16);
                        pass.draw_indexed(0..*count, 0, 0..1);
                    }
                }
                view
            } else {
                (*self.white).clone()
            };
            // the reflection and the ripples' picture set on the stages as pictures of their
            // own, not by name
            // D2-10090
            let reflected_group = self.bind_three(&[
                (&refl_view, &self.clamp),
                (&ripple_view, &self.clamp),
                (&self.white, &self.wrap),
            ]);
            for b in water {
                if b.indices.is_empty() {
                    continue;
                }
                let key = Self::water_key(&b.layer, target);
                self.water_pipeline(key);
                let mut u = Self::draw_uniforms(&b.layer, &b.transform, b.tint, out_sun, &b.points);
                u.params[2] = push;
                let draw_group =
                    self.uniforms
                        .take(&self.device, &self.draw_layout, bytemuck::bytes_of(&u));
                let (vertices, indices) =
                    self.meshes.buffers(&self.device, &b.vertices, &b.indices);
                water_draws.push(Draw {
                    pipeline: Pipe::Water(key),
                    vertices,
                    posed: None,
                    indices,
                    index_count: u32::try_from(b.indices.len()).unwrap_or(0),
                    index_format: wgpu::IndexFormat::Uint16,
                    draw_group,
                    texture_group: reflected_group.clone(),
                    env: Env::Outside,
                    shown: true,
                    reflected: true,
                    faint: None,
                    below: false,
                    cull: b.layer.cull,
                });
            }
        }
        let ms = lap.lap();
        self.note_work(|w| w.ms.reflection += ms);
        water_draws.extend(foam_draws);
        // The world's frame starts as the backdrop with the frame's alpha, a scene laid over the
        // interface clear. Seen from inside with a cell left out for the interior fog, the
        // background is painted the fog's color, blended by its alpha over the black the frame
        // starts from, before anything else is drawn.
        let (start, alpha) = match target {
            Target::Coverage => (wgpu::Color::TRANSPARENT, 0.0),
            _ => (self.backdrop, FRAME_ALPHA),
        };
        let backdrop = match &sight {
            Some(s) if s.fog => {
                let [r, g, b, a] = s.fog_color;
                wgpu::Color {
                    r: f64::from(r * a),
                    g: f64::from(g * a),
                    b: f64::from(b * a),
                    a: alpha,
                }
            }
            _ => wgpu::Color { a: alpha, ..start },
        };
        {
            // Every frame starts black (or clear): the sky's dome adds itself onto it.
            let mut pass = begin_pass(
                &mut encoder,
                "world",
                multi_view.as_ref().unwrap_or(&color_view),
                multi_view.as_ref().map(|_| &color_view),
                &depth_view,
                backdrop,
                flare_look.as_ref().map(|l| &l.queries),
            );
            pass.set_bind_group(3, &shadow_group, &[]);
            record(
                &mut pass,
                self,
                [&frame_group, &out_group, &sky_group, &looked_group],
                &water_draws,
                true,
                false,
            );
            // The sky objects' lens flares over everything, then the markers that look for the
            // objects (nothing drawn, only counted where they pass the depth test).
            if let (Some((flare, marker)), Some(vb)) = (&self.flare_pipelines, &flare_buffer) {
                pass.set_vertex_buffer(0, vb.get());
                pass.set_bind_group(0, &frame_group.group, &[frame_group.offset]);
                pass.set_bind_group(1, &identity.group, &[identity.offset]);
                pass.set_pipeline(flare);
                for (group, start) in &flare_runs {
                    pass.set_bind_group(2, group, &[]);
                    pass.draw(*start..*start + 6, 0..1);
                }
                pass.set_pipeline(marker);
                pass.set_bind_group(2, &white_group, &[]);
                for (k, (_, start)) in (0u32..).zip(&markers) {
                    pass.begin_occlusion_query(k);
                    pass.draw(*start..*start + 3, 0..1);
                    pass.end_occlusion_query();
                }
            }
        }
        if let Some(l) = &flare_look {
            let n = l.queries_count;
            encoder.resolve_query_set(&l.queries, 0..n, &l.resolve, 0);
            encoder.copy_buffer_to_buffer(&l.resolve, 0, &l.read, 0, u64::from(n) * 8);
        }
        // The draws' uniforms reach the device before the work that reads them.
        self.uniforms.write(&self.queue);
        self.frame_uniforms.write(&self.queue);
        self.meshes.write(&self.queue);
        let commands = encoder.finish();
        let ms = lap.lap();
        self.note_work(|w| w.ms.finish += ms);
        self.queue.submit([commands]);
        let ms = lap.lap();
        self.note_work(|w| w.ms.submit += ms);
        if let Some(look) = flare_look {
            let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = Arc::clone(&done);
            look.read
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| {
                    flag.store(r.is_ok(), std::sync::atomic::Ordering::Release);
                });
            self.flare_scan = Some(FlareScan {
                keys: markers.iter().map(|(key, _)| *key).collect(),
                look,
                done,
            });
        }
        let lists = self.meshes.take_counts();
        self.note_work(|w| {
            w.mesh_hits += lists.hits;
            w.mesh_misses += lists.misses;
            w.buffers += lists.buffers;
            w.buffer_bytes += lists.buffer_bytes;
            w.written_bytes +=
                lists.written + self.uniforms.bytes_taken() + self.frame_uniforms.bytes_taken();
        });
        color
    }

    /// Read a picture [`Self::draw`] left on the device back, every texel opaque when `opaque`.
    fn read_back(
        &self,
        color: &wgpu::Texture,
        width: u32,
        height: u32,
        opaque: bool,
    ) -> Result<Picture, GpuError> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let row = (width * 4).div_ceil(256) * 256;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        rx.recv()
            .map_err(|e| GpuError::Readback(e.to_string()))?
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        let data = slice
            .get_mapped_range()
            .map_err(|e| GpuError::Readback(format!("{e:?}")))?;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let start = (y * row) as usize;
            rgba.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        if opaque {
            for px in rgba.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
        }
        Ok(Picture {
            width,
            height,
            rgba,
        })
    }
}

/// The side of the square picture the water's reflection is drawn into, in texels.
// D2-10544, D2-10546
const REFLECTION_SIZE: u32 = 512;

/// How far the ripples push the reflection's coordinates: the bump stage's matrix, this much times
/// the identity.
const RIPPLE_PUSH: f32 = 0.04;

/// The picture of the frame's bodies' spots, in texels: as many as a frame can hold, and more bodies
/// than that are drawn in the full sun.
const BODY_SPOTS: (usize, usize) = (256, 16);

/// How deep the marker looking for a flare's object stands: just short of the farthest, so
/// anything drawn over the object hides it.
const FLARE_MARKER_DEPTH: f32 = 0.9999;

/// A point `p` of the frame through the camera's view and projection (row vectors): x, y, depth
/// and w, before the divide.
fn clip_of(vp: &Mat4, p: [f32; 3]) -> [f32; 4] {
    let m = &vp.0;
    let mut out = [0.0; 4];
    for (j, o) in out.iter_mut().enumerate() {
        *o = p[0] * m[0][j] + p[1] * m[1][j] + p[2] * m[2][j] + m[3][j];
    }
    out
}

impl MeshGpuVertex {
    /// A vertex already on the screen (-1 to 1 across and up, and its depth), with its picture's
    /// coordinates and its color.
    fn screen(position: [f32; 3], uv: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            normal: [0.0, 0.0, 1.0],
            diffuse: color,
            uv0: uv,
            uv1: uv,
            tangent_u: [0.0; 3],
            tangent_v: [0.0; 3],
            to_light: [0.0; 3],
        }
    }
}

impl Renderer {
    /// The answers of the last look for the flares' objects, once they are back: each object was
    /// seen where some of its marker passed the depth test. A look not yet answered is left to
    /// finish.
    fn read_flare_scan(&mut self) {
        let Some(scan) = &self.flare_scan else {
            return;
        };
        let _ = self.device.poll(wgpu::PollType::Poll);
        if !scan.done.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        if let Ok(data) = scan.look.read.slice(..).get_mapped_range() {
            for (k, key) in scan.keys.iter().enumerate() {
                let passed = data
                    .get(k * 8..k * 8 + 8)
                    .and_then(|b| <[u8; 8]>::try_from(b).ok())
                    .map_or(0, u64::from_le_bytes);
                self.flares.entry(*key).or_default().seen = passed > 0;
            }
        }
        scan.look.read.unmap();
        self.flare_kept = self.flare_scan.take().map(|s| s.look);
    }

    /// What a look for `count` of the flares' objects takes on the device.
    fn flare_look(&mut self, count: usize) -> FlareLook {
        let queries_count = u32::try_from(count).unwrap_or(0);
        if let Some(l) = self
            .flare_kept
            .take()
            .filter(|l| l.queries_count == queries_count)
        {
            return l;
        }
        let bytes = u64::from(queries_count) * 8;
        self.note_buffer(usize::try_from(bytes * 2).unwrap_or(0));
        FlareLook {
            queries: self.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("flare markers"),
                ty: wgpu::QueryType::Occlusion,
                count: queries_count,
            }),
            queries_count,
            resolve: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("flare answers"),
                size: bytes,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("flare answers read"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
}

/// The draws of `batches` that belong to step `pass`, in their order.
fn in_pass(batches: &[ObjectBatch], pass: Pass) -> Vec<&ObjectBatch> {
    batches.iter().filter(|b| b.pass() == pass).collect()
}

/// The light a draw is made under: the frame's own, the outdoors' (seen from inside), or the
/// sky's. The order of the frame's bind groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Env {
    Own = 0,
    Outside = 1,
    Sky = 2,
    /// The light cells looked into from the land are drawn under.
    Looked = 3,
}

impl FrameUniforms {
    /// The light and fog of `l`: the sun's direction and color, the ambient light, the fog's
    /// range and color, and how much the fog veils the sky.
    fn light(&mut self, l: &auberean_world::sky::DayLight) {
        let to_sun =
            auberean_world::math::normalize(auberean_world::math::scale(l.light_direction, -1.0));
        let ambient = l.ambient();
        let fog_on = l.fog_max > l.fog_min;
        let fog_span = if fog_on {
            1.0 / (l.fog_max - l.fog_min)
        } else {
            0.0
        };
        self.fog = [
            l.fog_min,
            l.fog_max,
            fog_span,
            if fog_on { 1.0 } else { 0.0 },
        ];
        self.fog_color = [l.fog_color.r, l.fog_color.g, l.fog_color.b, l.fog_sky];
        self.to_sun = [to_sun.x, to_sun.y, to_sun.z, 0.0];
        self.sun_color = [
            l.direct_color.r * l.direct_brightness,
            l.direct_color.g * l.direct_brightness,
            l.direct_color.b * l.direct_brightness,
            1.0,
        ];
        self.ambient = [ambient.r, ambient.g, ambient.b, 1.0];
        self.sun_specular = [l.direct_color.r, l.direct_color.g, l.direct_color.b, 1.0];
    }
}

/// Every draw of `batches` that are no objects of the world (the sky's, the water's), those that
/// mark how near they are first, each kind in its order (see [`ObjectBatch::by_layer`]).
fn opaque_first(batches: &[ObjectBatch]) -> Vec<&ObjectBatch> {
    let mut out: Vec<&ObjectBatch> = batches
        .iter()
        .filter(|b| b.by_layer() == Pass::Opaque)
        .collect();
    out.extend(batches.iter().filter(|b| b.by_layer() != Pass::Opaque));
    out
}

/// A pass over the picture `view` (resolved into `resolve` when multisampled) and `depth`, both
/// cleared: the color to `clear`, the depth to the farthest; the draws of a scene laid over the
/// interface that blend by the destination's alpha blend by the alpha the frame holds where no
/// shore lies.
fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    label: &'static str,
    view: &'a wgpu::TextureView,
    resolve: Option<&'a wgpu::TextureView>,
    depth: &'a wgpu::TextureView,
    clear: wgpu::Color,
    queries: Option<&'a wgpu::QuerySet>,
) -> wgpu::RenderPass<'a> {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: resolve,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0),
                store: wgpu::StoreOp::Store,
            }),
        }),
        timestamp_writes: None,
        occlusion_query_set: queries,
        multiview_mask: None,
    });
    pass.set_blend_constant(wgpu::Color {
        r: FRAME_ALPHA,
        g: FRAME_ALPHA,
        b: FRAME_ALPHA,
        a: FRAME_ALPHA,
    });
    pass
}

/// A plain layer for draws that take no material: white, unlit.
fn dummy_layer() -> DrawLayer {
    DrawLayer {
        stages: Vec::new(),
        source_blend: d3d::BLEND_ONE,
        dest_blend: d3d::BLEND_ZERO,
        depth_test: 4,
        depth_write: true,
        cull: d3d::CULL_NONE,
        alpha_test: false,
        alpha_ref: 0.0,
        tests_alpha: false,
        switches: 0,
        ambient: auberean_world::color::Color::WHITE,
        diffuse: auberean_world::color::Color::WHITE,
        specular: auberean_world::color::Color::BLACK,
        specular_power: 0.0,
        pixel_program: None,
        light_variants: vec![(0, 0)],
        motion: None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the device's shaders compile: a build check, no behavior of the game)

    /// The world's shaders parse and pass the shader compiler's checks without a device.
    #[test]
    fn the_world_shaders_compile() {
        let module = naga::front::wgsl::parse_str(include_str!("shaders/world.wgsl"))
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(include_str!("shaders/world.wgsl"))));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
    }
}
