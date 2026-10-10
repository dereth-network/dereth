//! The `wgpu` device: the third backend of [`crate::device::Gpu`], beside Vulkan and Direct3D 12.
//! WebGPU or WebGL 2 in a browser; Metal, Vulkan or Direct3D 12 natively, whichever `wgpu` finds.
//!
//! **What is the same as the other backends.** Every pipeline is built from a [`PipelineKey`] with
//! the key's blend, depth, cull and vertex layout, and the pixel stage is the shared shader source
//! with the same five stage chains, the same constant blocks and the same bindings: the two
//! constant blocks in group 0, the stage-0 and stage-1 textures in groups 1 and 2, and a sampler
//! pair in group 3. Samplers come in the same six banks of eight variants the texture-filtering
//! preference chooses between. Textures live in the same keyed, reference-counted table.
//!
//! **What differs, and why.**
//! - A frame is recorded and replayed at [`Gpu::end_frame`] in one render pass, because a `wgpu`
//!   pass borrows everything it draws with and the device's callers interleave uploads with
//!   draws. Vertex and constant bytes go to two per-frame arenas written once at the end.
//! - A depth clear over a rectangle is a full-rectangle triangle that writes depth 1 and no colour:
//!   a render pass can only clear its whole attachment.
//! - Samplers have no level-of-detail bias, so the bias the bound sampler would carry is handed to
//!   the pixel stage, which samples with it.
//! - Textures are RGBA8 (BGRA8 swizzled on upload), or block-compressed where the device has the
//!   feature and the base level is whole blocks; anything else is decoded on the CPU. Levels an
//!   image texture arrives without are generated on the device by linear filtering, as the other
//!   devices generate them.
//! - In a browser the device can only be made asynchronously, so the page makes it first
//!   (`prepare_canvas`) and hands it over ([`install`](crate::wgpu::install)); [`Gpu::new`]
//!   takes it.

#[cfg(feature = "test-support")]
mod digest;
mod levels;
#[cfg(feature = "hifi")]
pub mod sidecar;
mod terrain;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::descriptor::{
    DescriptorAllocator, DescriptorStats, Released, TextureKey, TextureSpace, TextureTableStats,
    DESCRIPTORS_PER_TEXTURE,
};
use crate::device::{
    AdapterKind, CapturedImage, DescriptorUsage, DeviceConfig, MergeSource, PerDrawConstants,
    PerFrameConstants, TerrainMergeJob, TerrainSplat, TextureSlot, WindowHandles,
};
use crate::{
    Blend, Cull, DrawConstants, PipelineKey, PixelFormatId, PixelShader, RenderError, VertexFormat,
    Viewport, ZFunc,
};
use dereth_primitives::{TextureData, TextureFormat};
use dereth_render_cpu::pso::{
    portal_stamp_diffuse, portal_stamp_mask, CullFace, FrontFace, FRONT_FACE,
    PORTAL_STAMP_FAR_DEPTH,
};
use dereth_render_cpu::sampler::{
    self, AddressMode, FilterCaps, SamplerFilter, BANK_KEYS, BANK_STRIDE, SAMPLER_COUNT,
};
use dereth_render_cpu::vertex::AttributeFormat;

/// The per-frame constant block's size, and its slot in the constant arena.
const FRAME_BYTES: u64 = 192;
const FRAME_BLOCK: usize = 192;
const FRAME_SLOT: usize = 256;
/// The per-draw constant block's size, and its slot.
const DRAW_BYTES: u64 = 432;
const DRAW_SLOT: usize = 512;

/// The depth attachment's format.
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The texture budget, in descriptors: the other devices' heap size, two descriptors a texture.
/// Nothing in `wgpu` runs out at this number; keeping it keeps occupancy comparable.
const SRV_HEAP_SIZE: u32 = 65_536;

/// A depth clear over the viewport: one triangle covering it at depth 1, writing no colour.
const CLEAR_DEPTH_SHADER: &str = r"
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 1.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
";

/// The shader for `format`: the shared source with its markers filled for that vertex layout.
/// Vertex colours are read as RGBA bytes and swizzled, the layout every device can read; texture
/// samples take the bias the bound sampler carries from the frame block's otherwise unused
/// `screen.y`, because a `wgpu` sampler has no bias of its own.
#[must_use]
pub fn shader_source(format: VertexFormat) -> String {
    crate::wgsl::bias_legacy_samples(&dereth_render_cpu::shader::vertex_source(format, false))
}

/// Where the frames go.
enum Target {
    /// A canvas, presented each frame.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Surface {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    /// A texture off screen, which [`Gpu::capture`] reads back.
    Offscreen { texture: wgpu::Texture },
}

/// A device made ahead of the application: a browser can make one only asynchronously, and the
/// application asks for its device synchronously, so the page makes it first and [`Gpu::new`]
/// takes it.
pub struct Prepared {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    adapter_kind: AdapterKind,
    bc: bool,
    anisotropy: bool,
    /// Whether the adapter traces rays: what a device asked for the high-fidelity presentation
    /// takes, whether or not this one was.
    #[cfg(feature = "hifi")]
    rays: bool,
    target: Target,
    format: wgpu::TextureFormat,
    size: (u32, u32),
}

impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prepared")
            .field("adapter", &self.adapter_name)
            .field("kind", &self.adapter_kind)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

thread_local! {
    static PREPARED: RefCell<Option<Prepared>> = const { RefCell::new(None) };
}

/// Hand the next [`Gpu::new`] this device.
pub fn install(prepared: Prepared) {
    PREPARED.with(|p| *p.borrow_mut() = Some(prepared));
}

/// Whether a device is waiting for [`Gpu::new`].
#[must_use]
pub fn installed() -> bool {
    PREPARED.with(|p| p.borrow().is_some())
}

/// An adapter and a device on `instance`, for `surface` or for drawing off screen, with limits a
/// WebGL 2 device can meet and block compression where the adapter has it.
async fn device_for(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
    software: bool,
    hifi: HifiAsk,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let request = |fallback: bool| wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: fallback,
        compatible_surface: surface,
        ..Default::default()
    };
    // A software adapter when one is asked for and the platform has one; the hardware adapter
    // otherwise.
    let adapter = match instance.request_adapter(&request(software)).await {
        Ok(a) => a,
        Err(_) if software => instance
            .request_adapter(&request(false))
            .await
            .map_err(|e| format!("no GPU adapter: {e}"))?,
        Err(e) => return Err(format!("no GPU adapter: {e}")),
    };
    let features = adapter.features() & wgpu::Features::TEXTURE_COMPRESSION_BC;
    // The adapter's own bind-group and storage limits where it has more than WebGL 2's: the
    // landscape splat binds a fifth group, and the landscape composite needs storage buffers.
    let supported = adapter.limits();
    let mut limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(supported.clone());
    limits.max_bind_groups = supported.max_bind_groups;
    limits.max_storage_buffers_per_shader_stage = supported.max_storage_buffers_per_shader_stage;
    limits.max_storage_buffer_binding_size = supported.max_storage_buffer_binding_size;
    limits.max_buffer_size = supported.max_buffer_size;
    limits.max_compute_workgroups_per_dimension = supported.max_compute_workgroups_per_dimension;
    limits.max_compute_invocations_per_workgroup = supported.max_compute_invocations_per_workgroup;
    limits.max_compute_workgroup_size_x = supported.max_compute_workgroup_size_x;
    limits.max_compute_workgroup_size_y = supported.max_compute_workgroup_size_y;
    limits.max_compute_workgroup_size_z = supported.max_compute_workgroup_size_z;
    limits.max_compute_workgroup_storage_size = supported.max_compute_workgroup_storage_size;
    limits.max_sampled_textures_per_shader_stage = supported.max_sampled_textures_per_shader_stage;
    #[cfg(feature = "hifi")]
    let (features, limits, experimental) = if hifi {
        sidecar::widen_request(&adapter, features, limits)
    } else {
        (features, limits, wgpu::ExperimentalFeatures::default())
    };
    #[cfg(not(feature = "hifi"))]
    let () = hifi;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("dereth"),
            required_features: features,
            required_limits: limits,
            #[cfg(feature = "hifi")]
            experimental_features: experimental,
            ..Default::default()
        })
        .await
        .map_err(|e| format!("no GPU device: {e}"))?;
    Ok((adapter, device, queue))
}

/// Whether a device is asked for the high-fidelity request: a flag in a build with it, and
/// nothing at all in a build without it, so such a build's device set-up is exactly as before.
#[cfg(feature = "hifi")]
type HifiAsk = bool;
#[cfg(not(feature = "hifi"))]
type HifiAsk = ();

/// The ordinary request.
#[cfg(feature = "hifi")]
const ORDINARY: HifiAsk = false;
#[cfg(not(feature = "hifi"))]
const ORDINARY: HifiAsk = ();

/// Whether `cfg` asks for the high-fidelity request; never, in a build without it.
#[cfg(feature = "hifi")]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const fn wants_hifi(cfg: &DeviceConfig) -> HifiAsk {
    cfg.hifi
}

/// Whether `cfg` asks for the high-fidelity request; never, in a build without it.
#[cfg(not(feature = "hifi"))]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const fn wants_hifi(_cfg: &DeviceConfig) -> HifiAsk {}

fn prepared(
    adapter: &wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: Target,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) -> Prepared {
    let info = adapter.get_info();
    let adapter_kind = if info.device_type == wgpu::DeviceType::Cpu {
        AdapterKind::Software
    } else {
        AdapterKind::Hardware
    };
    Prepared {
        bc: device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC),
        anisotropy: adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING),
        #[cfg(feature = "hifi")]
        rays: sidecar::adapter_rays(adapter),
        device,
        queue,
        adapter_name: format!("{} on {:?}", info.name, info.backend),
        adapter_kind,
        target,
        format,
        size,
    }
}

/// A device drawing into `canvas`: WebGPU where the browser has it, WebGL 2 where it does not or
/// where `webgl_only` asks for it.
///
/// # Errors
/// No backend comes up, or the canvas has no configuration on it.
#[cfg(target_arch = "wasm32")]
pub async fn prepare_canvas(
    canvas: web_sys::OffscreenCanvas,
    width: u32,
    height: u32,
    webgl_only: bool,
) -> Result<Prepared, String> {
    canvas.set_width(width);
    canvas.set_height(height);
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = if webgl_only {
        wgpu::Backends::GL
    } else {
        wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
    };
    let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas))
        .map_err(|e| format!("no surface: {e}"))?;
    let (adapter, device, queue) = device_for(&instance, Some(&surface), false, ORDINARY).await?;
    let caps = surface.get_capabilities(&adapter);
    let mut config = surface
        .get_default_config(&adapter, width, height)
        .ok_or_else(|| "the surface has no configuration for this adapter".to_string())?;
    // Colours are written as they are computed, with no sRGB encode, as the desktop device writes.
    if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
        config.format = *f;
    }
    config.present_mode = wgpu::PresentMode::Fifo;
    surface.configure(&device, &config);
    let format = config.format;
    Ok(prepared(
        &adapter,
        device,
        queue,
        Target::Surface { surface, config },
        format,
        (width, height),
    ))
}

/// A device drawing off screen at `width` by `height`: a software adapter when `software` asks
/// for one and the platform has one, else the hardware adapter. The native graphics APIs tried are
/// `wgpu`'s own choice, or those `WGPU_BACKEND` names (`dx12`, `vulkan`, `metal`, `gl`), as for a
/// window.
///
/// # Errors
/// No adapter or device on this machine.
pub async fn prepare_offscreen(
    width: u32,
    height: u32,
    software: bool,
) -> Result<Prepared, String> {
    prepare_offscreen_for(width, height, software, ORDINARY).await
}

/// [`prepare_offscreen`], with the high-fidelity request when `hifi` asks for it.
async fn prepare_offscreen_for(
    width: u32,
    height: u32,
    software: bool,
    hifi: HifiAsk,
) -> Result<Prepared, String> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let (adapter, device, queue) = device_for(&instance, None, software, hifi).await?;
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let texture = offscreen_texture(&device, format, width, height);
    Ok(prepared(
        &adapter,
        device,
        queue,
        Target::Offscreen { texture },
        format,
        (width, height),
    ))
}

/// A device drawing into a native window at `width` by `height`.
///
/// # Errors
/// No surface for the window, no adapter or device for it, or no configuration on the surface.
#[cfg(not(target_arch = "wasm32"))]
async fn prepare_window(
    window: WindowHandles,
    width: u32,
    height: u32,
    hifi: HifiAsk,
) -> Result<Prepared, String> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    // SAFETY: the handles are the live window's, and the window outlives the device: the client
    // drops its presentation before the window it was made from.
    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(window.display),
            raw_window_handle: window.window,
        })
    }
    .map_err(|e| format!("no surface: {e}"))?;
    let (adapter, device, queue) = device_for(&instance, Some(&surface), false, hifi).await?;
    let caps = surface.get_capabilities(&adapter);
    let mut config = surface
        .get_default_config(&adapter, width, height)
        .ok_or_else(|| "the surface has no configuration for this adapter".to_string())?;
    if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
        config.format = *f;
    }
    // Immediate where the surface allows it, as the other devices present a windowed frame;
    // the presentation-sync preference is honoured by the frame pacer.
    if caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
        config.present_mode = wgpu::PresentMode::Immediate;
    }
    surface.configure(&device, &config);
    let format = config.format;
    Ok(prepared(
        &adapter,
        device,
        queue,
        Target::Surface { surface, config },
        format,
        (width, height),
    ))
}

fn offscreen_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("back buffer"),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

const fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

/// One resident texture.
struct Texture {
    /// The texture as groups 1 and 2 bind it.
    bind: wgpu::BindGroup,
    /// The resource, for a read-back.
    texture: wgpu::Texture,
    /// What the texels are, as the caller's data names them: BGRA8, or the block format kept.
    stored: TextureFormat,
    levels: u16,
}

/// One recorded step of the frame.
enum Cmd {
    Viewport(Viewport),
    ClearDepth(Viewport),
    Bind {
        group: u32,
        bind: wgpu::BindGroup,
    },
    Draw {
        pipeline: usize,
        vertices: std::ops::Range<u64>,
        count: u32,
        frame: u32,
        draw: u32,
    },
}

/// The graphics device.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: Target,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    /// Hardware or software, as the adapter reports it.
    pub adapter_kind: AdapterKind,
    /// The adapter's own name and the native API under it, for the start-up line.
    pub adapter_name: String,
    bc: bool,

    layout: wgpu::PipelineLayout,
    uniform_layout: wgpu::BindGroupLayout,
    texture_layout: wgpu::BindGroupLayout,
    sampler_layout: wgpu::BindGroupLayout,
    modules: HashMap<VertexFormat, wgpu::ShaderModule>,
    pipelines: Vec<wgpu::RenderPipeline>,
    pipeline_index: HashMap<PipelineKey, usize>,
    clear_depth_pipeline: wgpu::RenderPipeline,

    samplers: Vec<wgpu::Sampler>,
    sampler_biases: Vec<f32>,
    sampler_pairs: RefCell<HashMap<(u32, u32), wgpu::BindGroup>>,
    texture_filtering: u32,
    sharp_lod_bias: bool,
    bound_sampler: Cell<Option<u32>>,
    bound_bias: Cell<f32>,

    textures: HashMap<u32, Texture>,
    /// The texture slots: the other devices' budget and accounting, so occupancy reads the same
    /// on every device. A slot released inside a frame comes back when that frame has ended.
    texture_book: crate::device::TextureBook,
    white: wgpu::BindGroup,
    stamp_texture: Option<TextureSlot>,

    /// The sublevel pass, one pipeline per texture format, made on first use.
    levels: levels::Levels,
    /// The landscape composite, made on first use.
    terrain_merge: Option<terrain::TerrainMerge>,
    /// The landscape splat, made on first use.
    terrain_splat: Option<terrain::TerrainSplatState>,
    /// The device runs compute shaders with storage buffers: the landscape composite needs both.
    compute: bool,
    /// Bind groups a pipeline may use: the landscape splat needs a fifth.
    max_bind_groups: u32,
    commands: RefCell<Vec<Cmd>>,
    vertex_arena: Vec<u8>,
    uniform_arena: Vec<u8>,
    last_frame_block: Option<([u8; FRAME_BLOCK], u32)>,
    vertex_buffer: Option<(wgpu::Buffer, u64)>,
    uniform_buffer: Option<(wgpu::Buffer, wgpu::BindGroup, u64)>,
    depth: Option<(wgpu::TextureView, u32, u32)>,
    upload_high_water: u64,

    frame_open: bool,
    /// Bumped once per presented frame.
    pub frame_stamp: u64,
    /// How many portal depth stamps this device has issued.
    pub portal_stamps: u64,
    /// How many dynamic draws this device has issued.
    pub draw_calls: u64,
    gamma: f32,
    present_sync_interval: u32,
    last_present_sync_interval: Option<u32>,
    sampler_binds: Cell<[u64; SAMPLER_COUNT as usize]>,
    stage1_binds: Cell<u64>,
    /// The high-fidelity sidecar, while one is installed.
    #[cfg(feature = "hifi")]
    sidecar: Option<sidecar::Installed>,
    /// Whether the device was asked for what the high-fidelity presentation draws with: a sidecar
    /// is installed only on such a device.
    #[cfg(feature = "hifi")]
    hifi_requested: bool,
    /// Why the last sidecar was uninstalled, if it failed.
    #[cfg(feature = "hifi")]
    hifi_failed: Option<String>,
    /// Whether the adapter traces rays, which a device asked for the high-fidelity presentation
    /// takes.
    #[cfg(feature = "hifi")]
    adapter_rays: bool,
    /// Test builds: whether each frame's recording is digested, and the last digest.
    #[cfg(feature = "test-support")]
    digest_frames: bool,
    #[cfg(feature = "test-support")]
    last_digest: Option<u64>,
    /// Test builds: how many render passes the device has encoded.
    #[cfg(feature = "test-support")]
    passes_encoded: u64,
    /// This crate's tests: the backend lock, released after everything above is dropped.
    #[cfg(all(test, feature = "vulkan"))]
    backend_lock: Option<std::sync::RwLockWriteGuard<'static, ()>>,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("adapter", &self.adapter_name)
            .field("size", &self.size)
            .field("pipelines", &self.pipelines.len())
            .field("textures", &self.textures.len())
            .finish_non_exhaustive()
    }
}

impl Gpu {
    /// A device for `window`, or off screen at the configured size when there is none. In a
    /// browser, the device [`install`] handed over.
    ///
    /// # Errors
    /// [`RenderError::Device`] when no adapter, device or surface comes up;
    /// [`RenderError::Unsupported`] in a browser when no device was prepared.
    pub fn new(window: Option<WindowHandles>, cfg: &DeviceConfig) -> Result<Self, RenderError> {
        #[cfg(all(test, feature = "vulkan"))]
        let backend_lock = crate::backend_lock::wgpu();
        // A device prepared ahead (a browser's canvas) was asked for nothing more.
        let (prepared, widened) = match PREPARED.with(|p| p.borrow_mut().take()) {
            Some(p) => (p, ORDINARY),
            None => (Self::prepare(window, cfg)?, wants_hifi(cfg)),
        };
        let mut gpu = Self::from_prepared(prepared, cfg.srv_descriptors);
        #[cfg(feature = "hifi")]
        {
            gpu.hifi_requested = widened;
        }
        #[cfg(not(feature = "hifi"))]
        let () = widened;
        #[cfg(all(test, feature = "vulkan"))]
        {
            gpu.backend_lock = Some(backend_lock);
        }
        if (cfg.width, cfg.height) != gpu.size && cfg.width > 0 && cfg.height > 0 {
            gpu.resize(cfg.width, cfg.height)?;
        }
        Ok(gpu)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn prepare(window: Option<WindowHandles>, cfg: &DeviceConfig) -> Result<Prepared, RenderError> {
        let (w, h) = (cfg.width.max(1), cfg.height.max(1));
        let hifi = wants_hifi(cfg);
        pollster::block_on(async {
            match window {
                Some(window) => prepare_window(window, w, h, hifi).await,
                None => prepare_offscreen_for(w, h, cfg.prefers_software(), hifi).await,
            }
        })
        .map_err(RenderError::Device)
    }

    #[cfg(target_arch = "wasm32")]
    fn prepare(_: Option<WindowHandles>, _: &DeviceConfig) -> Result<Prepared, RenderError> {
        Err(RenderError::Unsupported("no graphics device was prepared"))
    }

    #[allow(clippy::too_many_lines)]
    fn from_prepared(p: Prepared, srv_descriptors: Option<u32>) -> Self {
        let device = p.device;
        let uniform_entry = |binding: u32, size: u64| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("constants"),
            entries: &[uniform_entry(0, FRAME_BYTES), uniform_entry(1, DRAW_BYTES)],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let sampler_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let sampler_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("samplers"),
            entries: &[sampler_entry(0), sampler_entry(1)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("device"),
            bind_group_layouts: &[
                Some(&uniform_layout),
                Some(&texture_layout),
                Some(&texture_layout),
                Some(&sampler_layout),
            ],
            immediate_size: 0,
        });
        let clear_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth clear"),
            source: wgpu::ShaderSource::Wgsl(CLEAR_DEPTH_SHADER.into()),
        });
        let clear_depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth clear"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &clear_module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &clear_module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: p.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let (samplers, sampler_biases) = create_samplers(&device, p.anisotropy);
        let white_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("white"),
            size: extent(1, 1),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_level(&p.queue, &white_texture, 0, 1, 1, 4, &[255; 4]);
        let white = texture_bind(&device, &texture_layout, &white_texture);
        Self {
            #[cfg(all(test, feature = "vulkan"))]
            backend_lock: None,
            queue: p.queue,
            target: p.target,
            format: p.format,
            size: p.size,
            adapter_kind: p.adapter_kind,
            adapter_name: p.adapter_name,
            bc: p.bc,
            layout,
            uniform_layout,
            texture_layout,
            sampler_layout,
            modules: HashMap::new(),
            pipelines: Vec::new(),
            pipeline_index: HashMap::new(),
            clear_depth_pipeline,
            samplers,
            sampler_biases,
            sampler_pairs: RefCell::new(HashMap::new()),
            texture_filtering: 1,
            sharp_lod_bias: false,
            bound_sampler: Cell::new(None),
            bound_bias: Cell::new(0.0),
            textures: HashMap::new(),
            texture_book: crate::device::TextureBook::new(DescriptorAllocator::new(
                srv_descriptors.unwrap_or(SRV_HEAP_SIZE),
                DESCRIPTORS_PER_TEXTURE,
            )),
            white,
            stamp_texture: None,
            levels: levels::Levels::default(),
            terrain_merge: None,
            terrain_splat: None,
            compute: device.limits().max_storage_buffers_per_shader_stage >= 3
                && device.limits().max_compute_workgroups_per_dimension > 0,
            max_bind_groups: device.limits().max_bind_groups,
            commands: RefCell::new(Vec::new()),
            vertex_arena: Vec::new(),
            uniform_arena: Vec::new(),
            last_frame_block: None,
            vertex_buffer: None,
            uniform_buffer: None,
            depth: None,
            upload_high_water: 0,
            frame_open: false,
            frame_stamp: 0,
            portal_stamps: 0,
            draw_calls: 0,
            gamma: 0.0,
            present_sync_interval: 0,
            last_present_sync_interval: None,
            sampler_binds: Cell::new([0; SAMPLER_COUNT as usize]),
            stage1_binds: Cell::new(0),
            #[cfg(feature = "hifi")]
            sidecar: None,
            #[cfg(feature = "hifi")]
            hifi_requested: false,
            #[cfg(feature = "hifi")]
            hifi_failed: None,
            #[cfg(feature = "hifi")]
            adapter_rays: p.rays,
            #[cfg(feature = "test-support")]
            digest_frames: false,
            #[cfg(feature = "test-support")]
            last_digest: None,
            #[cfg(feature = "test-support")]
            passes_encoded: 0,
            device,
        }
    }

    // --- the frame bracket -------------------------------------------------------------------

    /// Open a frame: nothing recorded, the whole back buffer as the viewport, and the sampler
    /// bank the filtering preference chooses.
    ///
    /// # Errors
    /// Never; the signature is the device's.
    pub fn begin_frame(&mut self) -> Result<(), RenderError> {
        self.texture_book.descriptors.retire(self.frame_stamp);
        self.commands.get_mut().clear();
        self.vertex_arena.clear();
        self.uniform_arena.clear();
        self.last_frame_block = None;
        #[cfg(feature = "hifi")]
        if let Some(s) = &mut self.sidecar {
            s.begin_frame();
        }
        self.frame_open = true;
        self.reset_viewport();
        self.sharp_lod_bias = self.texture_filtering == 2;
        Ok(())
    }

    /// Draw the recorded frame and present it.
    ///
    /// # Errors
    /// Never: a surface that cannot give a frame is reconfigured and the frame is dropped.
    #[allow(clippy::too_many_lines)]
    pub fn end_frame(&mut self) -> Result<(), RenderError> {
        if !self.frame_open {
            return Ok(());
        }
        self.frame_open = false;
        let commands = std::mem::take(self.commands.get_mut());
        #[cfg(feature = "test-support")]
        if self.digest_frames {
            self.last_digest = Some(self.frame_digest(&commands));
        }
        self.upload_high_water = self
            .upload_high_water
            .max((self.vertex_arena.len() + self.uniform_arena.len()) as u64);
        let (surface_texture, view) = match &self.target {
            Target::Surface { surface, config } => match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                    let view = t
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    (Some(t), view)
                }
                _ => {
                    surface.configure(&self.device, config);
                    // The frame's draws are dropped with it, so what it released is free now.
                    self.texture_book.descriptors.retire(self.frame_stamp + 1);
                    return Ok(());
                }
            },
            Target::Offscreen { texture } => (
                None,
                texture.create_view(&wgpu::TextureViewDescriptor::default()),
            ),
        };
        #[cfg(feature = "hifi")]
        if self.sidecar.is_some() {
            return self.end_frame_sidecar(&commands, surface_texture, &view);
        }
        let vertex_buffer = self.vertex_buffer_for_frame();
        let (uniform_buffer, uniform_bind) = self.uniform_buffer_for_frame();
        let depth = self.depth_view();
        let (fw, fh) = self.size;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            #[cfg(feature = "test-support")]
            {
                self.passes_encoded += 1;
            }
            pass.set_bind_group(1, &self.white, &[]);
            pass.set_bind_group(2, &self.white, &[]);
            pass.set_bind_group(3, &self.sampler_pair(0, 1), &[]);
            pass.set_bind_group(0, &uniform_bind, &[0, 0]);
            let mut viewport = Viewport {
                x: 0,
                y: 0,
                width: fw,
                height: fh,
            };
            let mut current = None;
            for cmd in &commands {
                match cmd {
                    Cmd::Viewport(v) => {
                        viewport = *v;
                        apply_viewport(&mut pass, *v);
                    }
                    Cmd::ClearDepth(v) => {
                        apply_viewport(&mut pass, *v);
                        pass.set_pipeline(&self.clear_depth_pipeline);
                        current = None;
                        pass.draw(0..3, 0..1);
                        apply_viewport(&mut pass, viewport);
                    }
                    Cmd::Bind { group, bind } => pass.set_bind_group(*group, bind, &[]),
                    Cmd::Draw {
                        pipeline,
                        vertices,
                        count,
                        frame,
                        draw,
                    } => {
                        let Some(vb) = vertex_buffer.as_ref() else {
                            continue;
                        };
                        if current != Some(*pipeline) {
                            pass.set_pipeline(&self.pipelines[*pipeline]);
                            current = Some(*pipeline);
                        }
                        pass.set_bind_group(0, &uniform_bind, &[*frame, *draw]);
                        pass.set_vertex_buffer(0, vb.slice(vertices.clone()));
                        pass.draw(0..*count, 0..1);
                    }
                }
            }
        }
        drop(uniform_buffer);
        self.queue.submit([encoder.finish()]);
        if let Some(t) = surface_texture {
            self.queue.present(t);
        }
        self.last_present_sync_interval = Some(self.present_sync_interval);
        self.frame_stamp += 1;
        Ok(())
    }

    /// The frame's vertex bytes on the device, in a buffer grown to fit them.
    fn vertex_buffer_for_frame(&mut self) -> Option<wgpu::Buffer> {
        if self.vertex_arena.is_empty() {
            return None;
        }
        let needed = (self.vertex_arena.len() as u64).next_power_of_two();
        if self.vertex_buffer.as_ref().is_none_or(|(_, c)| *c < needed) {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vertices"),
                size: needed,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vertex_buffer = Some((buffer, needed));
        }
        let (buffer, _) = self.vertex_buffer.as_ref()?;
        self.queue.write_buffer(buffer, 0, &self.vertex_arena);
        Some(buffer.clone())
    }

    /// The frame's constant blocks on the device, and the group-0 binding over them.
    fn uniform_buffer_for_frame(&mut self) -> (wgpu::Buffer, wgpu::BindGroup) {
        let needed =
            (self.uniform_arena.len().max(FRAME_SLOT + DRAW_SLOT) as u64).next_power_of_two();
        if self
            .uniform_buffer
            .as_ref()
            .is_none_or(|(_, _, c)| *c < needed)
        {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("constants"),
                size: needed,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let binding = |binding: u32, size: u64| wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size),
                }),
            };
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("constants"),
                layout: &self.uniform_layout,
                entries: &[binding(0, FRAME_BYTES), binding(1, DRAW_BYTES)],
            });
            self.uniform_buffer = Some((buffer, bind, needed));
        }
        let Some((buffer, bind, _)) = self.uniform_buffer.as_ref() else {
            unreachable!("made above")
        };
        if !self.uniform_arena.is_empty() {
            self.queue.write_buffer(buffer, 0, &self.uniform_arena);
        }
        (buffer.clone(), bind.clone())
    }

    fn depth_view(&mut self) -> wgpu::TextureView {
        let (w, h) = self.size;
        match &self.depth {
            Some((view, dw, dh)) if *dw == w && *dh == h => view.clone(),
            _ => {
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("depth"),
                    size: extent(w, h),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.depth = Some((view.clone(), w, h));
                view
            }
        }
    }

    /// Wait for the device to finish what it was given.
    ///
    /// # Errors
    /// The device was lost.
    pub fn wait_idle(&mut self) -> Result<(), RenderError> {
        #[cfg(not(target_arch = "wasm32"))]
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Device(format!("wait: {e}")))?;
        Ok(())
    }

    #[must_use]
    pub fn frame_open(&self) -> bool {
        self.frame_open
    }

    // --- the back buffer ---------------------------------------------------------------------

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// A new back-buffer extent.
    ///
    /// # Errors
    /// [`RenderError::BadDimensions`] for a zero extent.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::BadDimensions {
                width,
                height,
                reason: "a presentation must have a non-zero extent",
            });
        }
        self.size = (width, height);
        match &mut self.target {
            Target::Surface { surface, config } => {
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }
            Target::Offscreen { texture } => {
                *texture = offscreen_texture(&self.device, self.format, width, height);
            }
        }
        Ok(())
    }

    /// Full screen with the refresh-sync preference presents on the refresh; a browser always
    /// does, so this is kept for the interval the device reports.
    pub fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
        self.present_sync_interval = u32::from(full_screen && sync_to_refresh);
    }

    #[must_use]
    pub fn present_sync_interval(&self) -> u32 {
        self.present_sync_interval
    }

    #[must_use]
    pub fn last_present_sync_interval(&self) -> Option<u32> {
        self.last_present_sync_interval
    }

    /// The screen-brightness ramp, clamped as the desktop device clamps it.
    pub fn set_gamma(&mut self, brightness: f32) {
        self.gamma = brightness.clamp(-0.2, 1.0);
    }

    #[must_use]
    pub fn gamma(&self) -> f32 {
        self.gamma
    }

    /// The last presented frame, read back. Only an off-screen device can.
    ///
    /// # Errors
    /// [`RenderError::Unsupported`] on a canvas; the device's own otherwise.
    pub fn capture(&mut self) -> Result<CapturedImage, RenderError> {
        let Target::Offscreen { texture } = &self.target else {
            return Err(RenderError::Unsupported("a canvas cannot be read back"));
        };
        let texture = texture.clone();
        self.read_back(&texture, 0, self.size.0, self.size.1)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_back(
        &mut self,
        texture: &wgpu::Texture,
        level: u32,
        width: u32,
        height: u32,
    ) -> Result<CapturedImage, RenderError> {
        let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read back"),
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            extent(width, height),
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.wait_idle()?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| RenderError::Device(format!("read back: {e}")))?;
        let mut bgra = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let start = (y * row) as usize;
            bgra.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
        }
        drop(mapped);
        if texture.format() == wgpu::TextureFormat::Rgba8Unorm {
            swap_red_blue(&mut bgra);
        }
        Ok(CapturedImage {
            width,
            height,
            bgra,
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn read_back(
        &mut self,
        _texture: &wgpu::Texture,
        _level: u32,
        _width: u32,
        _height: u32,
    ) -> Result<CapturedImage, RenderError> {
        Err(RenderError::Unsupported(
            "a browser cannot wait for a read-back",
        ))
    }

    // --- drawing -----------------------------------------------------------------------------

    /// Draw `vertices` as a triangle list with `key`'s state and the bound textures.
    ///
    /// # Errors
    /// Vertex data that is not whole vertices, or a pipeline the device will not build.
    pub fn draw_dynamic(
        &mut self,
        key: &PipelineKey,
        constants: &DrawConstants,
        per_frame: &PerFrameConstants,
        per_draw: &PerDrawConstants,
        vertices: &[u8],
    ) -> Result<(), RenderError> {
        let stride = key.vertex_format.stride();
        if stride == 0 || !vertices.len().is_multiple_of(stride as usize) {
            return Err(RenderError::Device(
                "vertex data is not a whole number of vertices".into(),
            ));
        }
        if vertices.is_empty() || !self.frame_open {
            return Ok(());
        }
        let pipeline = self.pipeline_state(key)?;
        let bias = self.bound_bias.get();
        self.record_draw(
            pipeline, key, constants, per_frame, per_draw, vertices, bias,
        );
        Ok(())
    }

    /// Record one draw of whole `vertices` with `pipeline`, whose layout is `key`'s, the samples
    /// taking `bias`.
    #[allow(clippy::too_many_arguments)]
    fn record_draw(
        &mut self,
        pipeline: usize,
        key: &PipelineKey,
        constants: &DrawConstants,
        per_frame: &PerFrameConstants,
        per_draw: &PerDrawConstants,
        vertices: &[u8],
        bias: f32,
    ) {
        let stride = key.vertex_format.stride();
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: a frame's vertices fit a u32
        let count = (vertices.len() / stride as usize) as u32;
        self.draw_calls += 1;

        let mut draw = *per_draw;
        draw.uv_offset = [
            constants.uv_offset()[0],
            constants.uv_offset()[1],
            per_draw.uv_offset[2],
            per_draw.uv_offset[3],
        ];
        draw.texture_factor = unpack_argb(constants.texture_factor);
        draw.draw_params = [
            f32::from(constants.alpha_ref) / 255.0,
            if key.alpha_test { 1.0 } else { 0.0 },
            if key.fog { 1.0 } else { 0.0 },
            draw.draw_params[3],
        ];
        let mut frame = *per_frame;
        frame.screen[0] = self.gamma;
        frame.screen[1] = bias;

        let frame_offset = self.push_frame_block(&frame);
        let draw_offset = self.push_draw_block(&draw);
        let start = self.vertex_arena.len() as u64;
        self.vertex_arena.extend_from_slice(vertices);
        let end = self.vertex_arena.len() as u64;
        self.commands.get_mut().push(Cmd::Draw {
            pipeline,
            vertices: start..end,
            count,
            frame: frame_offset,
            draw: draw_offset,
        });
    }

    /// A frame block in the arena, shared with the previous draw's when the bytes are the same.
    fn push_frame_block(&mut self, frame: &PerFrameConstants) -> u32 {
        let mut bytes = [0u8; FRAME_BLOCK];
        let mut at = 0;
        let mut put = |fs: &[f32]| {
            for f in fs {
                bytes[at..at + 4].copy_from_slice(&f.to_le_bytes());
                at += 4;
            }
        };
        put(&frame.view_proj);
        put(&frame.view);
        put(&frame.fog_params);
        put(&frame.fog_color);
        put(&frame.ambient);
        put(&frame.screen);
        if let Some((last, offset)) = &self.last_frame_block {
            if *last == bytes {
                return *offset;
            }
        }
        let offset = self.uniform_arena.len();
        self.uniform_arena.extend_from_slice(&bytes);
        self.uniform_arena.resize(offset + FRAME_SLOT, 0);
        let offset = u32::try_from(offset).unwrap_or(u32::MAX);
        self.last_frame_block = Some((bytes, offset));
        offset
    }

    fn push_draw_block(&mut self, draw: &PerDrawConstants) -> u32 {
        let offset = self.uniform_arena.len();
        let mut put = |fs: &[f32]| {
            for f in fs {
                self.uniform_arena.extend_from_slice(&f.to_le_bytes());
            }
        };
        put(&draw.world);
        put(&draw.uv_offset);
        put(&draw.texture_factor);
        put(&draw.draw_params);
        put(&draw.material_lighting);
        put(&draw.lighting_params);
        for l in &draw.light_pos {
            put(l);
        }
        for l in &draw.light_diffuse {
            put(l);
        }
        put(&draw.detail_params);
        put(&draw.normal_scale);
        self.uniform_arena.resize(offset + DRAW_SLOT, 0);
        u32::try_from(offset).unwrap_or(u32::MAX)
    }

    /// The portal depth stamp: `clip`, a polygon in clip space, fanned into screen-space triangles
    /// and drawn with the stamp's state.
    ///
    /// # Errors
    /// As [`Self::draw_dynamic`].
    pub fn draw_portal_poly(
        &mut self,
        per_frame: &PerFrameConstants,
        clip: &[[f32; 4]],
        mask: u8,
    ) -> Result<(), RenderError> {
        if clip.len() < 3 {
            return Ok(());
        }
        let slot = self.stamp_stage_texture()?;
        self.portal_stamps += 1;
        let key = PipelineKey::portal_stamp(mask);
        let (w, h) = self.size();
        #[allow(clippy::cast_precision_loss)] // a back-buffer extent
        let (fw, fh) = (w as f32, h as f32);
        let diffuse = portal_stamp_diffuse(mask, 0x00FF_FFFF);
        let screen: Vec<[f32; 4]> = clip
            .iter()
            .map(|p| {
                let iw = if p[3] == 0.0 { 0.0 } else { 1.0 / p[3] };
                let x = (p[0] * iw).mul_add(0.5, 0.5) * fw;
                let y = (p[1] * iw).mul_add(-0.5, 0.5) * fh;
                let z = if mask & portal_stamp_mask::CONSTANT_DEPTH != 0 {
                    PORTAL_STAMP_FAR_DEPTH
                } else {
                    p[2] * iw
                };
                [x, y, z, iw]
            })
            .collect();
        let mut vertices: Vec<u8> = Vec::with_capacity((screen.len() - 2) * 3 * 28);
        let push = |v: &[f32; 4], out: &mut Vec<u8>| {
            for f in v {
                out.extend_from_slice(&f.to_le_bytes());
            }
            out.extend_from_slice(&diffuse.to_le_bytes());
            out.extend_from_slice(&0.0f32.to_le_bytes());
            out.extend_from_slice(&0.0f32.to_le_bytes());
        };
        for i in 1..screen.len() - 1 {
            push(&screen[0], &mut vertices);
            push(&screen[i], &mut vertices);
            push(&screen[i + 1], &mut vertices);
        }
        let mut draw = PerDrawConstants::identity();
        draw.uv_offset = [0.0, 0.0, fw, fh];
        self.bind_texture(slot, 3);
        self.draw_dynamic(&key, &DrawConstants::default(), per_frame, &draw, &vertices)
    }

    fn stamp_stage_texture(&mut self) -> Result<TextureSlot, RenderError> {
        if let Some(s) = self.stamp_texture {
            return Ok(s);
        }
        let slot = self.upload_texture(&TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0xFF; 4]],
        })?;
        self.stamp_texture = Some(slot);
        Ok(slot)
    }

    /// Room in the upload arena. The arena here is the frame's and grows as it is written, so
    /// this only answers where the bytes would start.
    ///
    /// # Errors
    /// Never; the signature is the device's.
    pub fn upload_bytes(&mut self, data: &[u8]) -> Result<u64, RenderError> {
        self.upload_high_water = self.upload_high_water.max(data.len() as u64);
        Ok(0)
    }

    #[must_use]
    pub fn upload_high_water(&self) -> u64 {
        self.upload_high_water
    }

    /// Build every pipeline in the catalogue up front, in every vertex layout.
    ///
    /// # Errors
    /// Never: a pipeline the device refuses is the device's error report.
    pub fn build_whole_catalogue(&mut self) -> Result<usize, RenderError> {
        let mut built = 0;
        for row in crate::pso::CATALOGUE {
            for vertex_format in VertexFormat::all() {
                self.pipeline_state(&PipelineKey {
                    vertex_format,
                    src_blend: row.src,
                    dst_blend: row.dst,
                    alpha_blend: row.alpha_blend,
                    alpha_test: row.alpha_test,
                    z_write: row.z_write,
                    z_func: row.z_func,
                    cull: Cull::Cw,
                    stage_ops: crate::StageOps::BASE,
                    fog: true,
                    lighting: false,
                })?;
                built += 1;
            }
        }
        Ok(built)
    }

    /// Draw into `v`, clamped to the back buffer, until the next viewport.
    pub fn set_viewport(&self, v: Viewport) {
        let v = crate::camera::clamp_viewport(v, self.size.0, self.size.1);
        if v.width == 0 || v.height == 0 {
            return;
        }
        self.commands.borrow_mut().push(Cmd::Viewport(v));
    }

    pub fn reset_viewport(&self) {
        self.set_viewport(Viewport {
            x: 0,
            y: 0,
            width: self.size.0,
            height: self.size.1,
        });
    }

    /// Set depth to the far plane over `v`, leaving colour alone.
    pub fn clear_depth(&self, v: Viewport) {
        let v = crate::camera::clamp_viewport(v, self.size.0, self.size.1);
        if v.width == 0 || v.height == 0 {
            return;
        }
        self.commands.borrow_mut().push(Cmd::ClearDepth(v));
    }

    fn pipeline_state(&mut self, key: &PipelineKey) -> Result<usize, RenderError> {
        if let Some(i) = self.pipeline_index.get(key) {
            return Ok(*i);
        }
        let module = self
            .modules
            .entry(key.vertex_format)
            .or_insert_with(|| {
                self.device
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("legacy"),
                        source: wgpu::ShaderSource::Wgsl(shader_source(key.vertex_format).into()),
                    })
            })
            .clone();
        let pipeline = build_pipeline(
            &self.device,
            &self.layout,
            &module,
            pixel_entry(key.stage_ops.pixel_shader()),
            key,
            self.format,
        );
        self.pipelines.push(pipeline);
        let i = self.pipelines.len() - 1;
        self.pipeline_index.insert(*key, i);
        Ok(i)
    }

    // --- textures ----------------------------------------------------------------------------

    /// # Errors
    /// A texture with no extent or short level data.
    pub fn upload_texture(&mut self, t: &TextureData) -> Result<TextureSlot, RenderError> {
        self.upload_texture_keyed(TextureKey::UNCACHED, t)
    }

    /// A texture, or a second link on the one already resident under `key`.
    ///
    /// # Errors
    /// A texture with no extent or short level data.
    pub fn upload_texture_keyed(
        &mut self,
        key: TextureKey,
        t: &TextureData,
    ) -> Result<TextureSlot, RenderError> {
        self.upload_texture_internal(key, t, false)
    }

    /// A world image texture: its whole mip chain, built as the desktop device builds it.
    ///
    /// # Errors
    /// A key outside the world's space, or as [`Self::upload_texture_keyed`].
    pub fn upload_imgtex_keyed(
        &mut self,
        key: TextureKey,
        t: &TextureData,
    ) -> Result<TextureSlot, RenderError> {
        if key.space() != TextureSpace::World {
            return Err(RenderError::Unsupported(
                "runtime image texture mips require a world-owner key",
            ));
        }
        self.upload_texture_internal(key, t, true)
    }

    /// Mips are generated for an image texture, on the CPU.
    #[must_use]
    pub fn imgtex_autogen_supported(&self) -> bool {
        true
    }

    #[must_use]
    pub fn has_texture_key(&self, key: TextureKey) -> bool {
        self.texture_book.contains(key)
    }

    #[allow(clippy::too_many_lines)]
    fn upload_texture_internal(
        &mut self,
        key: TextureKey,
        t: &TextureData,
        imgtex: bool,
    ) -> Result<TextureSlot, RenderError> {
        if let Some(slot) = self.texture_book.get(key) {
            return Ok(TextureSlot(slot));
        }
        let upload = crate::device::PreparedUpload::new(t, imgtex, true)?;
        let t = upload.texture();
        let wanted = upload.wanted_levels(true);
        let compressed = block_format(t.format);
        let native = compressed.is_some()
            && self.bc
            && t.width.is_multiple_of(4)
            && t.height.is_multiple_of(4);
        let (format, levels): (wgpu::TextureFormat, Vec<Vec<u8>>) = match compressed {
            Some((format, block)) if native => {
                check_levels(t, Some(block))?;
                (format, t.levels.clone())
            }
            Some(_) => {
                let decoded = crate::texture::decode_block_chain(t)?;
                (wgpu::TextureFormat::Rgba8Unorm, rgba_levels(&decoded)?)
            }
            None => (wgpu::TextureFormat::Rgba8Unorm, rgba_levels(t)?),
        };
        let (provided, level_count, generate) = crate::device::PreparedUpload::web_levels(
            wanted,
            levels.len(),
            format == wgpu::TextureFormat::Rgba8Unorm,
        );
        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC;
        if generate {
            usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("texture"),
            size: extent(t.width, t.height),
            mip_level_count: level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        for (i, bytes) in levels.iter().take(provided).enumerate() {
            let (w, h) = level_extent(t.width, t.height, i);
            let level = u32::try_from(i).unwrap_or(0);
            match compressed {
                Some((_, block)) if native => {
                    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
                    #[allow(clippy::cast_possible_truncation)] // LINT-OK: 8 or 16
                    let row = bw * block as u32;
                    write_level(&self.queue, &texture, level, bw * 4, bh * 4, row, bytes);
                }
                _ => write_level(&self.queue, &texture, level, w, h, w * 4, bytes),
            }
        }
        if generate {
            // The missing sublevels, each drawn from the one above it with linear filtering, as
            // the other devices generate an image texture's chain.
            let from = u32::try_from(provided).unwrap_or(1);
            self.generate_levels(&texture, format, from, level_count);
        }
        let stored = if native {
            t.format
        } else {
            TextureFormat::Bgra8
        };
        self.register(key, texture, stored, level_count)
    }

    /// Put a made texture in the table under `key`: one link, owned by the caller.
    fn register(
        &mut self,
        key: TextureKey,
        texture: wgpu::Texture,
        stored: TextureFormat,
        levels: u32,
    ) -> Result<TextureSlot, RenderError> {
        self.texture_book.descriptors.retire(self.frame_stamp);
        let Some(slot) = self.texture_book.descriptors.alloc() else {
            return Err(self.texture_book.exhausted("frame"));
        };
        let bind = texture_bind(&self.device, &self.texture_layout, &texture);
        self.textures.insert(
            slot,
            Texture {
                bind,
                texture,
                stored,
                levels: u16::try_from(levels).unwrap_or(u16::MAX),
            },
        );
        self.texture_book.insert(key, slot);
        Ok(TextureSlot(slot))
    }

    pub fn retain_texture(&mut self, slot: TextureSlot) -> Option<u32> {
        self.texture_book.add_ref(slot.0)
    }

    /// Drop one link, freeing the texture at zero. A frame that already bound it keeps it until
    /// the frame is drawn.
    pub fn release_texture(&mut self, slot: TextureSlot) -> Released {
        let outcome = self.texture_book.release(slot.0);
        if outcome == Released::Freed {
            self.textures.remove(&slot.0);
            // A draw this frame recorded may name the slot; it is free again once the frame ends.
            let fence = self.frame_stamp + u64::from(self.frame_open);
            self.texture_book.descriptors.release(slot.0, fence);
        }
        outcome
    }

    #[must_use]
    pub fn texture_keys(&self) -> Vec<TextureKey> {
        self.texture_book.keys()
    }

    #[must_use]
    pub fn texture_table_stats(&self) -> TextureTableStats {
        self.texture_book.stats()
    }

    #[must_use]
    pub fn texture_mip_levels(&self, slot: TextureSlot) -> Option<u16> {
        self.textures.get(&slot.0).map(|t| t.levels)
    }

    #[must_use]
    pub fn live_textures(&self) -> usize {
        self.texture_book.len()
    }

    #[must_use]
    pub fn descriptor_stats(&self) -> DescriptorStats {
        self.texture_book.descriptors.stats()
    }

    /// Slots handed out, free and waiting on the frame, against the budget.
    #[must_use]
    pub fn descriptor_usage(&self) -> DescriptorUsage {
        self.texture_book.usage(0)
    }

    // --- binding and samplers ------------------------------------------------------------------

    /// Bind `slot` to both stages, with sampler variant `sampler` for stage 0 and its successor
    /// for stage 1, from the bank the filtering preference chooses.
    pub fn bind_texture(&self, slot: TextureSlot, sampler: u32) {
        let which = sampler % SAMPLER_COUNT;
        let mut counts = self.sampler_binds.get();
        counts[which as usize] += 1;
        self.sampler_binds.set(counts);
        let descriptor = self.sampler_descriptor_index(which);
        self.bound_sampler.set(Some(descriptor));
        self.bound_bias
            .set(self.sampler_biases[descriptor as usize]);
        let Some(texture) = self.textures.get(&slot.0) else {
            return;
        };
        let pair = self.sampler_pair(descriptor, descriptor + 1);
        let mut commands = self.commands.borrow_mut();
        commands.push(Cmd::Bind {
            group: 1,
            bind: texture.bind.clone(),
        });
        commands.push(Cmd::Bind {
            group: 2,
            bind: texture.bind.clone(),
        });
        commands.push(Cmd::Bind {
            group: 3,
            bind: pair,
        });
    }

    /// Bind `slot` to stage 1 alone, sampled with variant 0.
    pub fn bind_stage1_texture(&self, slot: TextureSlot) {
        let descriptor = self.sampler_descriptor_index(0);
        let stage0 = self.bound_sampler.get().unwrap_or(descriptor);
        let Some(texture) = self.textures.get(&slot.0) else {
            return;
        };
        let pair = self.sampler_pair(stage0, descriptor);
        let mut commands = self.commands.borrow_mut();
        commands.push(Cmd::Bind {
            group: 2,
            bind: texture.bind.clone(),
        });
        commands.push(Cmd::Bind {
            group: 3,
            bind: pair,
        });
        self.stage1_binds.set(self.stage1_binds.get() + 1);
    }

    #[must_use]
    pub fn stage1_binds(&self) -> u64 {
        self.stage1_binds.get()
    }

    pub fn clear_stage1_binds(&self) {
        self.stage1_binds.set(0);
    }

    #[must_use]
    pub fn sampler_binds(&self) -> [u64; SAMPLER_COUNT as usize] {
        self.sampler_binds.get()
    }

    pub fn clear_sampler_binds(&self) {
        self.sampler_binds.set([0; SAMPLER_COUNT as usize]);
    }

    /// The live texture-filtering preference, which chooses the sampler bank.
    pub fn set_texture_filtering(&mut self, preference: u32) {
        self.texture_filtering = preference;
    }

    #[must_use]
    pub fn texture_filtering(&self) -> u32 {
        self.texture_filtering
    }

    /// Start drawing with the sharp bank, where the preference allows it. `true` when it changed,
    /// in which case [`Self::end_preview_sharp`] ends it.
    pub fn begin_preview_sharp(&mut self, enabled: bool) -> bool {
        let changed = sampler::preview_sharp_enabled(self.texture_filtering, enabled);
        if changed {
            self.sharp_lod_bias = true;
        }
        changed
    }

    /// Back to the preference's own bank.
    pub fn end_preview_sharp(&mut self) {
        self.sharp_lod_bias = false;
    }

    fn sampler_descriptor_index(&self, which: u32) -> u32 {
        sampler::bank(self.texture_filtering, self.sharp_lod_bias) * BANK_STRIDE + which
    }

    fn sampler_pair(&self, s0: u32, s1: u32) -> wgpu::BindGroup {
        self.sampler_pairs
            .borrow_mut()
            .entry((s0, s1))
            .or_insert_with(|| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sampler pair"),
                    layout: &self.sampler_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&self.samplers[s0 as usize]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.samplers[s1 as usize]),
                        },
                    ],
                })
            })
            .clone()
    }
}

/// Every bank's samplers, with the bias each carries.
fn create_samplers(device: &wgpu::Device, anisotropy: bool) -> (Vec<wgpu::Sampler>, Vec<f32>) {
    let caps = FilterCaps {
        min_anisotropic: anisotropy,
        mag_anisotropic: anisotropy,
        max_anisotropy: if anisotropy { 16 } else { 1 },
    };
    let mut samplers = Vec::new();
    let mut biases = Vec::new();
    for (preference, sharp) in BANK_KEYS {
        for which in 0..BANK_STRIDE {
            let d = sampler::describe(which, preference, sharp, caps);
            let (mag, min, mip) = match d.filter {
                SamplerFilter::Point => (
                    wgpu::FilterMode::Nearest,
                    wgpu::FilterMode::Nearest,
                    wgpu::MipmapFilterMode::Nearest,
                ),
                SamplerFilter::LinearMipPoint => (
                    wgpu::FilterMode::Linear,
                    wgpu::FilterMode::Linear,
                    wgpu::MipmapFilterMode::Nearest,
                ),
                SamplerFilter::Linear | SamplerFilter::Anisotropic => (
                    wgpu::FilterMode::Linear,
                    wgpu::FilterMode::Linear,
                    wgpu::MipmapFilterMode::Linear,
                ),
            };
            let aniso = if d.filter == SamplerFilter::Anisotropic {
                u16::try_from(d.max_anisotropy).unwrap_or(16)
            } else {
                1
            };
            let address = |m: AddressMode| match m {
                AddressMode::Clamp => wgpu::AddressMode::ClampToEdge,
                AddressMode::Wrap => wgpu::AddressMode::Repeat,
            };
            samplers.push(device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("bank"),
                address_mode_u: address(d.address_u),
                address_mode_v: address(d.address_v),
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: mag,
                min_filter: min,
                mipmap_filter: mip,
                anisotropy_clamp: aniso,
                ..Default::default()
            }));
            biases.push(d.mip_lod_bias);
        }
    }
    (samplers, biases)
}

/// The pipeline for `key`: its vertex layout, blend, depth test and cull, and the pixel stage its
/// stage chain names, drawn by `entry`.
fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    key: &PipelineKey,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let attributes: Vec<wgpu::VertexAttribute> = key
        .vertex_format
        .elements()
        .iter()
        .map(|(e, offset)| wgpu::VertexAttribute {
            shader_location: e.location(),
            offset: u64::from(*offset),
            format: match e.attribute_format(false) {
                AttributeFormat::Float2 => wgpu::VertexFormat::Float32x2,
                AttributeFormat::Float3 => wgpu::VertexFormat::Float32x3,
                AttributeFormat::Float4 => wgpu::VertexFormat::Float32x4,
                AttributeFormat::Rgba8Unorm => wgpu::VertexFormat::Unorm8x4,
                AttributeFormat::Bgra8Unorm => wgpu::VertexFormat::Unorm8x4Bgra,
            },
        })
        .collect();
    let blend = key.alpha_blend.then(|| wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: blend_factor(key.src_blend),
            dst_factor: blend_factor(key.dst_blend),
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: blend_factor(key.src_blend.alpha_factor()),
            dst_factor: blend_factor(key.dst_blend.alpha_factor()),
            operation: wgpu::BlendOperation::Add,
        },
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("legacy"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: u64::from(key.vertex_format.stride()),
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: match FRONT_FACE {
                FrontFace::Clockwise => wgpu::FrontFace::Cw,
                FrontFace::CounterClockwise => wgpu::FrontFace::Ccw,
            },
            cull_mode: match key.cull.face() {
                CullFace::None => None,
                CullFace::Front => Some(wgpu::Face::Front),
                CullFace::Back => Some(wgpu::Face::Back),
            },
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(key.z_write),
            depth_compare: Some(compare(key.z_func)),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                // Red, green and blue only: the back buffer's alpha is never written.
                write_mask: wgpu::ColorWrites::COLOR,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

const fn pixel_entry(shader: PixelShader) -> &'static str {
    shader.entry_point()
}

const fn blend_factor(b: Blend) -> wgpu::BlendFactor {
    match b {
        Blend::Zero => wgpu::BlendFactor::Zero,
        Blend::One => wgpu::BlendFactor::One,
        Blend::SrcColor => wgpu::BlendFactor::Src,
        Blend::InvSrcColor => wgpu::BlendFactor::OneMinusSrc,
        Blend::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
        Blend::InvSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
        Blend::DestAlpha => wgpu::BlendFactor::DstAlpha,
        Blend::InvDestAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
        Blend::DestColor => wgpu::BlendFactor::Dst,
        Blend::InvDestColor => wgpu::BlendFactor::OneMinusDst,
    }
}

const fn compare(z: ZFunc) -> wgpu::CompareFunction {
    match z {
        ZFunc::Never => wgpu::CompareFunction::Never,
        ZFunc::Less => wgpu::CompareFunction::Less,
        ZFunc::Equal => wgpu::CompareFunction::Equal,
        ZFunc::LessEqual => wgpu::CompareFunction::LessEqual,
        ZFunc::Greater => wgpu::CompareFunction::Greater,
        ZFunc::NotEqual => wgpu::CompareFunction::NotEqual,
        ZFunc::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
        ZFunc::Always => wgpu::CompareFunction::Always,
    }
}

fn apply_viewport(pass: &mut wgpu::RenderPass<'_>, v: Viewport) {
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

/// The block-compressed format for `format`, and its block size in bytes; `None` for BGRA8.
const fn block_format(format: TextureFormat) -> Option<(wgpu::TextureFormat, usize)> {
    match format {
        TextureFormat::Bc1 => Some((wgpu::TextureFormat::Bc1RgbaUnorm, 8)),
        TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => {
            Some((wgpu::TextureFormat::Bc2RgbaUnorm, 16))
        }
        TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => {
            Some((wgpu::TextureFormat::Bc3RgbaUnorm, 16))
        }
        _ => None,
    }
}

const fn level_extent(width: u32, height: u32, level: usize) -> (u32, u32) {
    let mut w = width;
    let mut h = height;
    let mut i = 0;
    while i < level {
        w = if w > 1 { w / 2 } else { 1 };
        h = if h > 1 { h / 2 } else { 1 };
        i += 1;
    }
    (w, h)
}

/// Every level long enough for its extent, in blocks of `block` bytes or in BGRA8 texels.
fn check_levels(t: &TextureData, block: Option<usize>) -> Result<(), RenderError> {
    for (i, bits) in t.levels.iter().enumerate() {
        let (w, h) = level_extent(t.width, t.height, i);
        let expected = match block {
            Some(b) => w.div_ceil(4) as usize * h.div_ceil(4) as usize * b,
            None => w as usize * h as usize * 4,
        };
        if bits.len() < expected {
            return Err(RenderError::ShortSourceData {
                format: PixelFormatId::A8R8G8B8,
                width: w,
                height: h,
                expected,
                actual: bits.len(),
            });
        }
    }
    Ok(())
}

/// A BGRA8 chain's levels, cut to their extents and swizzled to RGBA.
fn rgba_levels(t: &TextureData) -> Result<Vec<Vec<u8>>, RenderError> {
    check_levels(t, None)?;
    Ok(t.levels
        .iter()
        .enumerate()
        .map(|(i, bits)| {
            let (w, h) = level_extent(t.width, t.height, i);
            let mut level = bits[..w as usize * h as usize * 4].to_vec();
            swap_red_blue(&mut level);
            level
        })
        .collect())
}

fn swap_red_blue(pixels: &mut [u8]) {
    for p in pixels.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
}

fn write_level(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    level: u32,
    width: u32,
    height: u32,
    bytes_per_row: u32,
    bytes: &[u8],
) {
    let rows = if texture.format().is_compressed() {
        height / 4
    } else {
        height
    };
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bytes[..(bytes_per_row * rows) as usize],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: Some(rows),
        },
        extent(width, height),
    );
}

fn texture_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("texture"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    })
}

/// A packed `0xAARRGGBB` as four channels in `0..1`, red first.
fn unpack_argb(c: u32) -> [f32; 4] {
    let channel = |shift: u32| f32::from(u8::try_from((c >> shift) & 0xFF).unwrap_or(0)) / 255.0;
    [channel(16), channel(8), channel(0), channel(24)]
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::StageOps;

    /// A device off screen on this machine's GPU, or `None` with a printed line where there is
    /// none, so a machine without one still passes.
    pub(super) fn device(width: u32, height: u32) -> Option<Gpu> {
        let cfg = DeviceConfig {
            width,
            height,
            ..DeviceConfig::default()
        };
        match Gpu::new(None, &cfg) {
            Ok(gpu) => Some(gpu),
            Err(e) => {
                eprintln!("skipped: {e}");
                None
            }
        }
    }

    /// Four screen-space vertices of a `w` by `h` rectangle at `(x, y)`, as two triangles.
    pub(super) fn quad(x: f32, y: f32, w: f32, h: f32, z: f32, diffuse: u32) -> Vec<u8> {
        let corner = |cx: f32, cy: f32, u: f32, v: f32| {
            let mut out = Vec::new();
            for f in [cx, cy, z, 1.0] {
                out.extend_from_slice(&f.to_le_bytes());
            }
            out.extend_from_slice(&diffuse.to_le_bytes());
            out.extend_from_slice(&u.to_le_bytes());
            out.extend_from_slice(&v.to_le_bytes());
            out
        };
        let (a, b, c, d) = (
            corner(x, y, 0.0, 0.0),
            corner(x + w, y, 1.0, 0.0),
            corner(x + w, y + h, 1.0, 1.0),
            corner(x, y + h, 0.0, 1.0),
        );
        [a.clone(), b, c.clone(), a, c, d].concat()
    }

    pub(super) fn screen_key(z_func: ZFunc) -> PipelineKey {
        PipelineKey {
            vertex_format: VertexFormat::XyzRhwDiffuseTex1,
            src_blend: Blend::One,
            dst_blend: Blend::Zero,
            alpha_blend: false,
            alpha_test: false,
            z_write: true,
            z_func,
            cull: Cull::None,
            stage_ops: StageOps::BASE,
            fog: false,
            lighting: false,
        }
    }

    pub(super) fn pixel(image: &CapturedImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * image.width + x) * 4) as usize;
        [
            image.bgra[i],
            image.bgra[i + 1],
            image.bgra[i + 2],
            image.bgra[i + 3],
        ]
    }

    /// A headless device that asks for the software rasteriser comes up on the same kind of
    /// adapter as one that does not, in a test build: test devices prefer the machine's GPU
    /// unless `DERETH_TEST_GPU=software` says otherwise, as the other two backends' do.
    #[test]
    fn a_test_build_prefers_the_gpu_even_where_the_caller_asked_for_software() {
        if crate::device::test_adapter_override() == Some(AdapterKind::Software) {
            eprintln!("skipped: DERETH_TEST_GPU asks for the software rasteriser");
            return;
        }
        // One device at a time: a test device holds the backend lock while it lives.
        let Some((plain_kind, plain_name)) =
            device(64, 64).map(|g| (g.adapter_kind, g.adapter_name.clone()))
        else {
            return;
        };
        let cfg = DeviceConfig {
            width: 64,
            height: 64,
            force_software: true,
            ..DeviceConfig::default()
        };
        let asked = Gpu::new(None, &cfg).expect("the same machine opens a second device");
        assert_eq!(
            asked.adapter_kind, plain_kind,
            "{} against {plain_name}",
            asked.adapter_name
        );
    }

    /// Every pipeline in the catalogue, in every vertex layout, and every stage chain builds from
    /// the desktop client's shader source on this device.
    #[test]
    fn every_catalogue_pipeline_and_stage_chain_builds() {
        let Some(mut gpu) = device(64, 64) else {
            return;
        };
        let built = gpu.build_whole_catalogue().expect("the catalogue");
        assert_eq!(
            built,
            crate::pso::CATALOGUE.len() * VertexFormat::all().len()
        );
        for stage_ops in [
            StageOps::BASE,
            StageOps::SINGLE_PASS_DETAIL,
            StageOps::UI_OPAQUE,
            StageOps::UI_ALPHA,
            StageOps::TEXT,
        ] {
            for vertex_format in VertexFormat::all() {
                gpu.pipeline_state(&PipelineKey {
                    vertex_format,
                    stage_ops,
                    ..screen_key(ZFunc::LessEqual)
                })
                .expect("a pipeline");
            }
        }
        gpu.wait_idle().expect("the device finishes");
    }

    /// A textured screen-space quad draws the texture modulated by its vertex colour where it
    /// covers, and the frame's black clear everywhere else.
    #[test]
    fn a_screen_quad_draws_its_texture_times_its_colour() {
        let Some(mut gpu) = device(64, 64) else {
            return;
        };
        let texture = gpu
            .upload_texture(&TextureData {
                width: 2,
                height: 2,
                format: TextureFormat::Bgra8,
                // Blue, green, red, alpha: every texel white.
                levels: vec![vec![0xFF; 16]],
            })
            .expect("a texture");
        gpu.begin_frame().expect("a frame");
        gpu.bind_texture(texture, 1);
        let red = 0xFFFF_0000;
        gpu.draw_dynamic(
            &screen_key(ZFunc::LessEqual),
            &DrawConstants::default(),
            &PerFrameConstants::default(),
            &PerDrawConstants {
                uv_offset: [0.0, 0.0, 64.0, 64.0],
                ..PerDrawConstants::identity()
            },
            &quad(16.0, 16.0, 32.0, 32.0, 0.5, red),
        )
        .expect("a draw");
        gpu.end_frame().expect("the frame");
        let image = gpu.capture().expect("a capture");
        assert_eq!(pixel(&image, 32, 32)[..3], [0, 0, 0xFF], "inside is red");
        assert_eq!(pixel(&image, 4, 4)[..3], [0, 0, 0], "outside is the clear");
    }

    /// A depth clear over a rectangle lets a farther quad draw there, and only there, over a
    /// nearer one; outside it the nearer quad still wins the depth test.
    #[test]
    fn a_depth_clear_reopens_only_its_rectangle() {
        let Some(mut gpu) = device(64, 64) else {
            return;
        };
        let texture = gpu
            .upload_texture(&TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![vec![0xFF; 4]],
            })
            .expect("a texture");
        let per_draw = PerDrawConstants {
            uv_offset: [0.0, 0.0, 64.0, 64.0],
            ..PerDrawConstants::identity()
        };
        let draw = |gpu: &mut Gpu, z: f32, colour: u32| {
            gpu.draw_dynamic(
                &screen_key(ZFunc::Less),
                &DrawConstants::default(),
                &PerFrameConstants::default(),
                &per_draw,
                &quad(0.0, 0.0, 64.0, 64.0, z, colour),
            )
            .expect("a draw");
        };
        gpu.begin_frame().expect("a frame");
        gpu.bind_texture(texture, 1);
        draw(&mut gpu, 0.2, 0xFFFF_0000);
        gpu.clear_depth(Viewport {
            x: 0,
            y: 0,
            width: 32,
            height: 64,
        });
        draw(&mut gpu, 0.8, 0xFF00_FF00);
        gpu.end_frame().expect("the frame");
        let image = gpu.capture().expect("a capture");
        assert_eq!(
            pixel(&image, 16, 32)[..3],
            [0, 0xFF, 0],
            "cleared half is green"
        );
        assert_eq!(
            pixel(&image, 48, 32)[..3],
            [0, 0, 0xFF],
            "the rest stays red"
        );
    }

    /// A keyed upload of a key already resident is a second link on the same slot, and the
    /// texture is freed only when the last link goes.
    #[test]
    fn a_keyed_texture_is_shared_and_freed_at_its_last_link() {
        let Some(mut gpu) = device(8, 8) else {
            return;
        };
        let key = TextureKey::world(0x0400_0001_0500_0001);
        let t = TextureData {
            width: 4,
            height: 4,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0x80; 64]],
        };
        let a = gpu.upload_texture_keyed(key, &t).expect("first");
        let b = gpu.upload_texture_keyed(key, &t).expect("second");
        assert_eq!(a, b);
        assert_eq!(gpu.release_texture(a), Released::StillLinked(1));
        assert!(gpu.has_texture_key(key));
        assert_eq!(gpu.release_texture(b), Released::Freed);
        assert!(!gpu.has_texture_key(key));
    }

    /// A world image texture gets its whole mip chain, generated on the CPU.
    #[test]
    fn an_image_texture_gets_its_whole_mip_chain() {
        let Some(mut gpu) = device(8, 8) else {
            return;
        };
        let key = TextureKey::world(0x0400_0002_0500_0002);
        let slot = gpu
            .upload_imgtex_keyed(
                key,
                &TextureData {
                    width: 16,
                    height: 8,
                    format: TextureFormat::Bgra8,
                    levels: vec![vec![0x40; 16 * 8 * 4]],
                },
            )
            .expect("an image texture");
        assert_eq!(gpu.texture_mip_levels(slot), Some(5));
    }

    /// Every vertex layout's shader has its markers filled.
    #[test]
    fn no_marker_survives_substitution() {
        for f in VertexFormat::all() {
            assert!(!shader_source(f).contains("{{"), "{f:?}");
        }
    }
}
