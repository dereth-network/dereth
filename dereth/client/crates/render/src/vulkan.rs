//! **The only rendering module in this crate that uses `unsafe`.** Vulkan interop, through `ash`.
//!
//! The device, frame ring, heaps and upload ring, pipeline construction, and the headless
//! capture backend, ported from the Direct3D 12 build. The pure-logic half of each
//! lives outside this module and is tested without a GPU; what is here is the part that cannot be.
//!
//! This backend preserves device creation, presentation, reset, gamma and the fixed-function
//! mapping onto Vulkan.
//!
//! # The shape of the port
//!
//! The public surface -- [`Gpu`] and every method on it -- is the D3D12 build's, so the client
//! and its tests are unchanged apart from the module path. Underneath, the D3D12 concepts map:
//!
//! | D3D12 | here |
//! |---|---|
//! | fence + event, `GetCompletedValue` | one **timeline semaphore** (Vulkan 1.2); same values |
//! | swap-chain back buffers as render targets | one offscreen colour image, blitted to the swap chain at `end_frame` |
//! | shader-visible SRV heap, one pair per texture | one descriptor set per texture slot, same free list |
//! | sampler heap, immutable banks | one `VkSampler` per bank entry, pair sets made on first use |
//! | the sampler's mip LOD bias | the same, or, where samplers cannot carry one (MoltenVK over Metal), the pixel shader samples with the bound sampler's bias |
//! | root CBVs at a GPU address | one dynamic-uniform-buffer set per upload arena, offsets per draw |
//! | `D3DCompile` at device creation | `naga` at device creation (`vulkan/shaders.rs`) |
//! | WARP | a `CPU`-type physical device when the loader has one (lavapipe, SwiftShader) |
//!
//! Rendering always goes to the offscreen image, whether or not there is a window, so a capture
//! reads the frame that was just presented rather than the *next* back buffer, and the swap-chain
//! format need not be the render target's.
//!
//! # Clip space
//!
//! Every viewport is installed with a **negative height**, which turns Vulkan's y-down framebuffer
//! space into Direct3D's. The shaders, the pre-transformed portal stamp, the winding-order cull
//! and the half-pixel rules in `ui.rs` are therefore the D3D12 build's, untouched.
//!
//! # The FPU
//!
//! The retail device is created with `D3DCREATE_FPU_PRESERVE`, so
//! D3D9 never reset the FPU control word and the client ran its whole session at 53-bit precision.
//! **Nothing in this module changes the FPU control word**, and nothing may be added that does.
//! On x86-64 the Rust ABI computes in SSE2, which the FPU control word does not govern, so the
//! invariant is preserved by leaving it alone.

#![allow(clippy::cast_possible_truncation)] // Graphics APIs are full of u32 sizes; each site is bounded.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::sync::Mutex;

use ash::vk;
use gpu_allocator::vulkan::{
    Allocation, AllocationCreateDesc, AllocationScheme, Allocator, AllocatorCreateDesc,
};
use gpu_allocator::MemoryLocation;

use dereth_primitives::{TextureData, TextureFormat};

use crate::descriptor::{
    DescriptorAllocator, DescriptorStats, Released, TextureKey, TextureTable, TextureTableStats,
    DESCRIPTORS_PER_TEXTURE,
};
use crate::pso::{Blend, Cull, PipelineKey, PixelShader, ZFunc};
use crate::vertex::VertexFormat;
use crate::RenderError;

mod mipgen;
mod samplers;
mod shaders;
mod terrain_merge;
mod terrain_splat;

// The plain data both backends speak lives in `crate::device`; it is re-exported here so that
// `dereth_render::vulkan::DeviceConfig` and its neighbours name the same types as the D3D12
// backend's.
pub use crate::device::{
    hlsl_matrix, AdapterKind, CapturedImage, DescriptorUsage, DeviceConfig, PerDrawConstants,
    PerFrameConstants, RawDisplayHandle, RawWindowHandle, TextureSlot, WindowHandles, FRAME_COUNT,
};
pub use samplers::{AddressMode, SamplerDescription, SamplerFilter};

use crate::device::{as_bytes, shader_entry_c, unpack_argb};

/// The back-buffer format. The client picks `X8R8G8B8` on a 32-bit desktop and disables sRGB
/// writes, so matching it requires a **non**-sRGB target. `B8G8R8A8_UNORM` is `A8R8G8B8`'s memory
/// order; the alpha channel is never written because `COLORWRITEENABLE = 7`.
pub const BACK_BUFFER_FORMAT: vk::Format = vk::Format::B8G8R8A8_UNORM;

/// The depth format the client prefers when its stencil buffer is enabled:
/// **D24S8 → D32 → D24X8 → D24X4S4**. Vulkan does not guarantee `D24S8` (AMD and Apple GPUs have
/// none), so the ladder here is `D24S8 → D32S8`, chosen per device in [`Gpu::new`]; this constant
/// is the first rung.
pub const DEPTH_FORMAT: vk::Format = vk::Format::D24_UNORM_S8_UINT;

fn vkr<T>(what: &'static str, r: Result<T, vk::Result>) -> Result<T, RenderError> {
    r.map_err(|e| RenderError::Device(format!("{what}: {e}")))
}

/// A buffer and the memory behind it.
struct Buffer {
    buffer: vk::Buffer,
    allocation: Option<Allocation>,
    size: u64,
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Buffer {
    /// Copy `data` in at `offset`. The allocation must be host-visible.
    fn write(&mut self, offset: usize, data: &[u8]) -> Result<(), RenderError> {
        let slice = self
            .allocation
            .as_mut()
            .and_then(Allocation::mapped_slice_mut)
            .ok_or_else(|| RenderError::Device("upload memory is not host-visible".into()))?;
        slice
            .get_mut(offset..offset + data.len())
            .ok_or_else(|| RenderError::Device("upload past the end of an arena".into()))?
            .copy_from_slice(data);
        Ok(())
    }

    /// Read `len` bytes from `offset`. The allocation must be host-visible and the GPU done.
    fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>, RenderError> {
        let ptr = self
            .allocation
            .as_ref()
            .and_then(Allocation::mapped_ptr)
            .ok_or_else(|| RenderError::Device("readback memory is not host-visible".into()))?;
        if (offset + len) as u64 > self.size {
            return Err(RenderError::Device(
                "read past the end of a readback buffer".into(),
            ));
        }
        // SAFETY: the allocation is mapped for its whole life, `offset + len` is inside it, and
        // the caller has waited for the GPU writes that filled it.
        Ok(
            unsafe { std::slice::from_raw_parts(ptr.as_ptr().cast::<u8>().add(offset), len) }
                .to_vec(),
        )
    }
}

/// One dynamic upload arena, and the descriptor set that views it as the two constant buffers.
#[derive(Debug)]
struct UploadArena {
    buffer: Buffer,
    set: vk::DescriptorSet,
}

/// One frame's slot in the ring: its command buffer, its timeline value and its share of the
/// upload arena.
#[derive(Debug)]
struct Frame {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    /// The per-arena constant-buffer sets come from here; reset once the frame has retired.
    desc_pool: vk::DescriptorPool,
    fence_value: u64,
    /// The dynamic vertex ring. The client grows a dynamic
    /// buffer to the previous frame's high-water mark and never shrinks it within a session; the
    /// same rule applies here.
    upload: Option<UploadArena>,
    upload_capacity: u64,
    upload_used: u64,
    high_water: u64,
    /// Upload arenas this frame outgrew. They stay alive until the timeline proves the GPU has
    /// finished with the command buffer that still references their addresses.
    retired: Vec<UploadArena>,
    /// Signalled by the swap-chain acquire; waited on by this frame's submission.
    image_available: vk::Semaphore,
}

/// A live texture: the image, its view, its memory and what it is.
struct Texture {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<Allocation>,
    format: vk::Format,
    tex_format: TextureFormat,
    levels: u16,
    width: u32,
    height: u32,
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture")
            .field("format", &self.tex_format)
            .field("levels", &self.levels)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// The render target: one colour image and one depth image, at the presentation extent.
#[derive(Debug)]
struct Target {
    color: Texture,
    depth: Texture,
    framebuffer: vk::Framebuffer,
    /// Whether a render pass has ever written the colour image, i.e. whether its layout is the
    /// pass's final `TRANSFER_SRC_OPTIMAL` or still `UNDEFINED`.
    rendered: bool,
}

/// The swap chain attached to a window.
#[derive(Debug)]
struct Swapchain {
    handle: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    extent: vk::Extent2D,
    present_mode: vk::PresentModeKHR,
    /// One per swap-chain image, signalled by the submission that blits into it and waited on by
    /// its presentation.
    render_finished: Vec<vk::Semaphore>,
}

/// Validation-layer output, collected so a failed pipeline can say *why*. Static because the
/// messenger callback is a C function pointer with no `self`.
static DEBUG_MESSAGES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The Vulkan device, swap chain, frame ring, descriptor pools and pipeline cache.
pub struct Gpu {
    entry: ash::Entry,
    instance: ash::Instance,
    debug_utils: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface: Option<(ash::khr::surface::Instance, vk::SurfaceKHR)>,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    queue: vk::Queue,
    allocator: Option<Allocator>,
    swapchain_loader: Option<ash::khr::swapchain::Device>,
    swapchain: Option<Swapchain>,
    target: Target,
    depth_format: vk::Format,
    render_pass: vk::RenderPass,
    /// The one-shot pool: texture uploads, captures and readbacks record here, never in a frame's.
    oneshot_pool: vk::CommandPool,
    timeline: vk::Semaphore,
    next_fence_value: u64,
    frames: Vec<Frame>,
    frame_index: usize,
    uniform_set_layout: vk::DescriptorSetLayout,
    texture_set_layout: vk::DescriptorSetLayout,
    sampler_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    vertex_modules: HashMap<VertexFormat, vk::ShaderModule>,
    fragment_modules: HashMap<PixelShader, vk::ShaderModule>,
    // ORDER-OK: keyed by PipelineKey and only ever looked up, never iterated for anything the
    // rendered result depends on.
    pipelines: HashMap<PipelineKey, vk::Pipeline>,
    /// Whether `B8G8R8A8_UNORM` is a legal vertex format on this device; the shaders swizzle when
    /// it is not.
    bgra_vertex_colour: bool,
    /// Whether the device decodes BC1/2/3 itself. When it does not, block-compressed uploads are
    /// decoded to BGRA8 on the CPU (`crate::dxt`) and stored that way -- a declared divergence.
    bc_supported: bool,
    anisotropy_supported: bool,
    max_anisotropy: u32,
    /// Actual BGRA8 format capability, queried once per device.
    imgtex_autogen_supported: bool,
    /// Whether the queue this backend submits to runs compute, which landscape surface
    /// composition on the device needs.
    terrain_merge_supported: bool,
    /// `maxStorageBufferRange`: the largest the compositor's source pool may grow.
    max_storage_buffer_range: u64,
    /// `maxBoundDescriptorSets`. The splat pipeline binds five.
    max_bound_descriptor_sets: u32,
    /// The landscape compositor, built on first use. See `vulkan/terrain_merge.rs`.
    terrain_merge: Option<terrain_merge::TerrainMerge>,
    /// The landscape splat pipeline and its binding cache, built on first use. See
    /// `vulkan/terrain_splat.rs`.
    terrain_splat: Option<terrain_splat::TerrainSplatState>,
    samplers: Vec<vk::Sampler>,
    sampler_descriptions: Vec<SamplerDescription>,
    sampler_pool: vk::DescriptorPool,
    sampler_pair_sets: RefCell<HashMap<(u32, u32), vk::DescriptorSet>>,
    texture_filtering: u32,
    sharp_lod_bias: bool,
    /// Whether the pixel shaders apply the bound sampler's level-of-detail bias, which then goes
    /// to each draw in the frame block, and the samplers carry none. A device whose samplers
    /// cannot carry a bias does this; so does one configured to with
    /// [`DeviceConfig::force_shader_lod_bias`].
    shader_lod_bias: bool,
    bound_sampler: Cell<Option<u32>>,
    /// One descriptor set per texture slot, allocated on first use of the slot and rewritten
    /// each time the slot is reissued -- which the free list only does behind the timeline.
    texture_pool: vk::DescriptorPool,
    texture_sets: Vec<Option<vk::DescriptorSet>>,
    /// Live texture resources, keyed by slot. An entry lives exactly as long as [`TextureTable`]
    /// says something still links to it.
    // ORDER-OK: keyed by slot and only ever looked up, never iterated for anything the rendered
    // result depends on.
    textures: HashMap<u32, Texture>,
    /// Textures whose last link went away. They stay alive until the timeline proves the GPU has
    /// finished with the command buffers that referenced them.
    retired_textures: Vec<(u64, Texture)>,
    /// Texture uploads submitted without waiting, with the timeline value each signals: the
    /// command buffer and the staging buffers it reads stay alive until the device has passed it.
    /// See [`Gpu::one_shot_nowait`].
    pending_uploads: Vec<(u64, vk::CommandBuffer, Vec<Buffer>)>,
    /// Releases that happened while a frame's command buffer was open, held until `end_frame`
    /// decides their fence value. See [`Gpu::release_texture`].
    released_in_frame: Vec<(u32, Option<Texture>)>,
    /// True between `begin_frame` and `end_frame`.
    frame_open: bool,
    /// How many times each of [`Gpu::bind_texture`]'s samplers has been bound since
    /// the counters were last cleared. A `Cell` because `bind_texture` takes `&self`.
    sampler_binds: Cell<[u64; SAMPLER_COUNT as usize]>,
    /// How many draws have put a *detail* texture in stage 1.
    stage1_binds: Cell<u64>,
    /// The shared and custom texture tables and their link count.
    texture_table: TextureTable,
    /// Texture slots: a free list, not a monotonic counter.
    descriptors: DescriptorAllocator,
    config: DeviceConfig,
    pub adapter_kind: AdapterKind,
    /// The adapter's name, for the startup line.
    pub adapter_name: String,
    /// The frame stamp, bumped once per presented frame.
    pub frame_stamp: u64,
    /// The `SyncInterval` the next presentation uses: 0 immediate, 1 vsync.
    present_sync_interval: u32,
    /// The interval most recently handed to a real swap chain. Offscreen frames leave this `None`.
    last_present_sync_interval: Option<u32>,
    /// The device's gamma brightness value.
    gamma: f32,
    /// The 1x1 opaque-white texture the pre-transformed portal stamp samples, created on first use.
    stamp_texture: Option<TextureSlot>,
    /// How many portal depth stamps this device has issued.
    pub portal_stamps: u64,
    /// How many `draw_dynamic` batches this device has issued.
    pub draw_calls: u64,
    /// This crate's tests: the backend lock, released after the device is destroyed.
    #[cfg(all(test, feature = "wgpu"))]
    _backend_lock: std::sync::RwLockReadGuard<'static, ()>,
}

impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gpu")
            .field("adapter_kind", &self.adapter_kind)
            .field("adapter_name", &self.adapter_name)
            .field("size", &(self.config.width, self.config.height))
            .field("frame_stamp", &self.frame_stamp)
            .field("frame_open", &self.frame_open)
            .field("live_textures", &self.textures.len())
            .finish_non_exhaustive()
    }
}

/// The validation layer, when the SDK is installed.
const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

/// The messenger callback: keep warnings and errors, print errors.
unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut c_void,
) -> vk::Bool32 {
    if data.is_null() {
        return vk::FALSE;
    }
    // SAFETY: the loader hands a live callback-data struct whose message is a NUL-terminated
    // string for the duration of the call.
    let text = unsafe {
        let d = &*data;
        if d.p_message.is_null() {
            String::new()
        } else {
            CStr::from_ptr(d.p_message).to_string_lossy().into_owned()
        }
    };
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        tracing::error!("vulkan validation: {text}");
    }
    if severity.intersects(
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
            | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
    ) {
        if let Ok(mut m) = DEBUG_MESSAGES.lock() {
            if m.len() < 4096 {
                m.push(text);
            }
        }
    }
    vk::FALSE
}

/// The MoltenVK library's file name, in a bundle and in an install alike.
#[cfg(any(target_os = "macos", test))]
const MOLTENVK_LIBRARY: &str = "libMoltenVK.dylib";

/// Where a MoltenVK shipped with the program is, given the executable's path `exe`, in the order
/// they are tried: the application bundle's `Contents/Frameworks` (the executable being in
/// `Contents/MacOS`), then beside the executable itself, for a program unpacked from an archive.
///
/// MoltenVK exports the Vulkan entry points itself, so it is loaded directly as the loader; it
/// has no layers and no driver discovery in front of it, and needs neither to draw.
#[cfg(any(target_os = "macos", test))]
fn bundled_moltenvk_candidates(exe: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Some(dir) = exe.parent() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(contents) = dir.parent() {
        out.push(contents.join("Frameworks").join(MOLTENVK_LIBRARY));
    }
    out.push(dir.join(MOLTENVK_LIBRARY));
    out
}

/// The install locations a macOS Vulkan can be at, tried in turn after the plain `dlopen`.
///
/// The ICD-aware loader comes first; MoltenVK on its own is the last resort, because it exports
/// the Vulkan entry points itself but has no layers and no ICD discovery in front of it.
#[cfg(target_os = "macos")]
fn loader_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
        out.push(
            std::path::Path::new(&sdk)
                .join("lib")
                .join("libvulkan.dylib"),
        );
    }
    for dir in ["/opt/homebrew/lib", "/usr/local/lib"] {
        out.push(std::path::Path::new(dir).join("libvulkan.dylib"));
    }
    for dir in ["/opt/homebrew/lib", "/usr/local/lib"] {
        out.push(std::path::Path::new(dir).join(MOLTENVK_LIBRARY));
    }
    out
}

/// Load the Vulkan loader, with the search path macOS's `dlopen` does not have.
///
/// Windows and Linux ship the loader where the dynamic linker already looks -- `vulkan-1.dll`
/// beside the GPU driver, `libvulkan.so.1` in the system library path -- so `Entry::load`'s bare
/// library name finds it. macOS has no system Vulkan at all. A released program carries its own
/// MoltenVK, so that is looked for first ([`bundled_moltenvk_candidates`]), and a player needs to
/// install nothing. Without one, both ways of installing a Vulkan (the LunarG SDK under
/// `$VULKAN_SDK`, Homebrew under its prefix) land outside `DYLD_FALLBACK_LIBRARY_PATH`, which is
/// `$HOME/lib:/usr/local/lib:/usr/lib`, so the bare name would miss a perfectly good installation
/// and every run would need `DYLD_LIBRARY_PATH` set; [`loader_candidates`] names them.
///
/// # Errors
/// [`RenderError::Device`] when no loader is found, naming where it looked and what to install.
#[cfg(target_os = "macos")]
fn load_entry() -> Result<ash::Entry, RenderError> {
    let bundled = std::env::current_exe()
        .map(|exe| bundled_moltenvk_candidates(&exe))
        .unwrap_or_default();
    // A library that is there but will not load says why; one that is absent is only listed.
    let mut refused = Vec::new();
    let mut try_load = |path: &std::path::Path| -> Option<ash::Entry> {
        if !path.exists() {
            return None;
        }
        // SAFETY: loading a Vulkan loader or implementation library has no preconditions; the
        // `Entry` owns it.
        match unsafe { ash::Entry::load_from(path) } {
            Ok(entry) => Some(entry),
            Err(e) => {
                refused.push(format!("{} ({e})", path.display()));
                None
            }
        }
    };
    if let Some(entry) = bundled.iter().find_map(|p| try_load(p)) {
        return Ok(entry);
    }
    // SAFETY: loading the Vulkan loader library has no preconditions; the `Entry` owns it.
    let plain = match unsafe { ash::Entry::load() } {
        Ok(entry) => return Ok(entry),
        Err(e) => e,
    };
    let candidates = loader_candidates();
    if let Some(entry) = candidates.iter().find_map(|p| try_load(p)) {
        return Ok(entry);
    }
    let looked: Vec<String> = bundled
        .iter()
        .chain(&candidates)
        .map(|p| p.display().to_string())
        .collect();
    let refused = if refused.is_empty() {
        String::new()
    } else {
        format!(" Present but not loaded: {}.", refused.join("; "))
    };
    Err(RenderError::Device(format!(
        "no Vulkan loader: {plain}; nor at {}.{refused} macOS ships no Vulkan: install the LunarG          Vulkan SDK, or `brew install molten-vk vulkan-loader`.",
        looked.join(", ")
    )))
}

/// Load the Vulkan loader by the name the platform's dynamic linker already resolves.
///
/// # Errors
/// [`RenderError::Device`] when no loader is installed.
#[cfg(not(target_os = "macos"))]
fn load_entry() -> Result<ash::Entry, RenderError> {
    // SAFETY: loading the Vulkan loader library has no preconditions; the `Entry` owns it.
    unsafe { ash::Entry::load() }.map_err(|e| RenderError::Device(format!("no Vulkan loader: {e}")))
}

/// The loader and driver, kept loaded for the life of the process.
///
/// Destroying a process's last instance lets the loader unload the driver, and unloading the
/// Vulkan loader library itself does the same. A process that opens and closes devices one after
/// another (every test binary does, once per test) then loads and unloads the driver over and
/// over, and a driver can keep state across that -- per-thread state, or work still finishing
/// after the last instance goes -- that points into the copy it was unloaded from. The measured
/// result was an access violation executing an address inside the driver module, in a process
/// that had already made and dropped many devices, never in the first. So the first device made
/// in a process also makes one bare instance that is never destroyed, with its own handle on the
/// loader: the driver is loaded once and stays loaded.
fn pinned_entry() -> Result<ash::Entry, RenderError> {
    static PINNED: std::sync::OnceLock<(ash::Entry, Option<ash::Instance>)> =
        std::sync::OnceLock::new();
    if let Some((e, _)) = PINNED.get() {
        return Ok(e.clone());
    }
    let entry = load_entry()?;
    let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2);
    let info = vk::InstanceCreateInfo::default().application_info(&app);
    // SAFETY: a bare instance with no layers and no extensions; it is never used for anything
    // and never destroyed, so no handle made from it can outlive it. A failure leaves nothing
    // pinned and the device is opened as before.
    let keep = unsafe { entry.create_instance(&info, None) }.ok();
    Ok(PINNED.get_or_init(|| (entry, keep)).0.clone())
}

impl Gpu {
    /// Create the device. Reproduces the *shape* of the client's creation ladder:
    /// try the best rung first, fall back, and report which
    /// rung was taken. The ladder is discrete GPU → integrated → virtual → CPU rasteriser, or the
    /// reverse with `force_software`.
    ///
    /// # Errors
    /// [`RenderError::Device`] when there is no Vulkan loader, no 1.2 device with a graphics queue
    /// that can present to the window, or a shader fails to compile.
    pub fn new(window: Option<WindowHandles>, cfg: &DeviceConfig) -> Result<Self, RenderError> {
        #[cfg(all(test, feature = "wgpu"))]
        let backend_lock = crate::backend_lock::vulkan();
        if cfg.width == 0 || cfg.height == 0 {
            return Err(RenderError::BadDimensions {
                width: cfg.width,
                height: cfg.height,
                reason: "the back buffer must have a non-zero extent",
            });
        }
        let entry = pinned_entry()?;
        let (instance, debug_utils) = Self::create_instance(&entry, window, cfg.debug)?;
        let surface = match window {
            Some(w) => {
                let loader = ash::khr::surface::Instance::new(&entry, &instance);
                // SAFETY: the handles came from a live window that the caller keeps alive for
                // the lifetime of the device, which is the contract `Gpu::new` states.
                let s = vkr("vkCreateSurface", unsafe {
                    ash_window::create_surface(&entry, &instance, w.display, w.window, None)
                })?;
                Some((loader, s))
            }
            None => None,
        };
        let (physical_device, queue_family, adapter_kind, adapter_name) =
            Self::pick_physical_device(&instance, surface.as_ref(), cfg.prefers_software())?;
        // SAFETY: pure queries on a live physical device.
        let (features, limits) = unsafe {
            let f = instance.get_physical_device_features(physical_device);
            let p = instance.get_physical_device_properties(physical_device);
            (f, p.limits)
        };
        let bc_supported = features.texture_compression_bc == vk::TRUE;
        let anisotropy_supported = features.sampler_anisotropy == vk::TRUE;
        #[allow(clippy::cast_sign_loss)] // a positive device limit
        let max_anisotropy = if anisotropy_supported {
            (limits.max_sampler_anisotropy as u32).clamp(1, 16)
        } else {
            1
        };
        let (device, queue, sampler_lod_bias) = Self::create_device(
            &instance,
            physical_device,
            queue_family,
            surface.is_some(),
            &features,
        )?;
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device,
            debug_settings: gpu_allocator::AllocatorDebugSettings::default(),
            buffer_device_address: false,
            allocation_sizes: gpu_allocator::AllocationSizes::default(),
        })
        .map_err(|e| RenderError::Device(format!("gpu-allocator: {e}")))?;

        // SAFETY: pure format queries.
        let bgra_vertex_colour = unsafe {
            instance
                .get_physical_device_format_properties(physical_device, vk::Format::B8G8R8A8_UNORM)
                .buffer_features
                .contains(vk::FormatFeatureFlags::VERTEX_BUFFER)
        };
        let depth_format = Self::pick_depth_format(&instance, physical_device)?;
        let imgtex_autogen_supported = mipgen::supported(&instance, physical_device);
        // SAFETY: a pure query on a live physical device.
        let terrain_merge_supported =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) }
                .get(queue_family as usize)
                .is_some_and(|f| f.queue_flags.contains(vk::QueueFlags::COMPUTE));
        // SAFETY: a pure query on a live physical device.
        let limits = unsafe { instance.get_physical_device_properties(physical_device) }.limits;
        let max_storage_buffer_range = u64::from(limits.max_storage_buffer_range);
        let max_bound_descriptor_sets = limits.max_bound_descriptor_sets;

        let mut timeline_type = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let timeline_info = vk::SemaphoreCreateInfo::default().push_next(&mut timeline_type);
        // SAFETY: live locals; the semaphore is owned by `self`.
        let timeline = vkr("vkCreateSemaphore(timeline)", unsafe {
            device.create_semaphore(&timeline_info, None)
        })?;

        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family)
            .flags(
                vk::CommandPoolCreateFlags::TRANSIENT
                    | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
            );
        // SAFETY: a live local; owned by `self`.
        let oneshot_pool = vkr("vkCreateCommandPool(one-shot)", unsafe {
            device.create_command_pool(&pool_info, None)
        })?;

        let render_pass = Self::create_render_pass(&device, depth_format)?;
        let (uniform_set_layout, texture_set_layout, sampler_set_layout, pipeline_layout) =
            Self::create_layouts(&device)?;

        // Where the samplers cannot carry the filtering preference's bias, the pixel shaders apply it.
        let shader_lod_bias = !sampler_lod_bias || cfg.force_shader_lod_bias;
        let (vs, ps) = shaders::compile_shaders(bgra_vertex_colour, shader_lod_bias)?;
        let mut vertex_modules = HashMap::new();
        for (f, words) in vs {
            vertex_modules.insert(f, Self::shader_module(&device, &words)?);
        }
        let mut fragment_modules = HashMap::new();
        for (s, words) in ps {
            fragment_modules.insert(s, Self::shader_module(&device, &words)?);
        }

        let srv_descriptors = cfg.srv_descriptors.unwrap_or(SRV_HEAP_SIZE);
        let descriptors = DescriptorAllocator::new(srv_descriptors, DESCRIPTORS_PER_TEXTURE);
        let slots = descriptors.capacity().max(1);
        let texture_pool = Self::descriptor_pool(
            &device,
            slots,
            &[vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: slots,
            }],
        )?;
        let pairs = samplers::DESCRIPTOR_COUNT * samplers::DESCRIPTOR_COUNT;
        let sampler_pool = Self::descriptor_pool(
            &device,
            pairs,
            &[vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: pairs * 2,
            }],
        )?;

        let mut frames = Vec::with_capacity(FRAME_COUNT);
        for _ in 0..FRAME_COUNT {
            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family)
                .flags(vk::CommandPoolCreateFlags::TRANSIENT);
            // SAFETY: live locals; every object is owned by the `Frame`.
            let (pool, cmd, desc_pool, image_available) = unsafe {
                let pool = vkr(
                    "vkCreateCommandPool(frame)",
                    device.create_command_pool(&pool_info, None),
                )?;
                let alloc = vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1);
                let cmd = vkr(
                    "vkAllocateCommandBuffers(frame)",
                    device.allocate_command_buffers(&alloc),
                )?[0];
                let desc_pool = Self::descriptor_pool(
                    &device,
                    ARENA_SETS_PER_FRAME,
                    &[vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
                        descriptor_count: ARENA_SETS_PER_FRAME * 2,
                    }],
                )?;
                let sem = vkr(
                    "vkCreateSemaphore",
                    device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None),
                )?;
                (pool, cmd, desc_pool, sem)
            };
            frames.push(Frame {
                pool,
                cmd,
                desc_pool,
                fence_value: 0,
                upload: None,
                upload_capacity: 0,
                upload_used: 0,
                high_water: 0,
                retired: Vec::new(),
                image_available,
            });
        }

        let swapchain_loader = surface
            .as_ref()
            .map(|_| ash::khr::swapchain::Device::new(&instance, &device));

        let mut gpu = Self {
            #[cfg(all(test, feature = "wgpu"))]
            _backend_lock: backend_lock,
            entry,
            instance,
            debug_utils,
            surface,
            physical_device,
            device,
            queue,
            allocator: Some(allocator),
            swapchain_loader,
            swapchain: None,
            target: Target {
                color: Texture::null(),
                depth: Texture::null(),
                framebuffer: vk::Framebuffer::null(),
                rendered: false,
            },
            depth_format,
            render_pass,
            oneshot_pool,
            timeline,
            next_fence_value: 1,
            frames,
            frame_index: 0,
            uniform_set_layout,
            texture_set_layout,
            sampler_set_layout,
            pipeline_layout,
            vertex_modules,
            fragment_modules,
            pipelines: HashMap::new(),
            bgra_vertex_colour,
            bc_supported,
            anisotropy_supported,
            max_anisotropy,
            imgtex_autogen_supported,
            terrain_merge_supported,
            max_storage_buffer_range,
            max_bound_descriptor_sets,
            terrain_merge: None,
            terrain_splat: None,
            samplers: Vec::with_capacity(samplers::DESCRIPTOR_COUNT as usize),
            sampler_descriptions: Vec::with_capacity(samplers::DESCRIPTOR_COUNT as usize),
            sampler_pool,
            sampler_pair_sets: RefCell::new(HashMap::new()),
            texture_filtering: crate::sampler::STARTUP_FILTERING,
            sharp_lod_bias: false,
            shader_lod_bias,
            bound_sampler: Cell::new(None),
            texture_pool,
            texture_sets: vec![None; slots as usize],
            textures: HashMap::new(),
            retired_textures: Vec::new(),
            pending_uploads: Vec::new(),
            released_in_frame: Vec::new(),
            frame_open: false,
            sampler_binds: Cell::new([0; SAMPLER_COUNT as usize]),
            stage1_binds: Cell::new(0),
            texture_table: TextureTable::new(),
            descriptors,
            config: *cfg,
            adapter_kind,
            adapter_name,
            frame_stamp: 0,
            present_sync_interval: 0,
            last_present_sync_interval: None,
            gamma: 0.0,
            stamp_texture: None,
            portal_stamps: 0,
            draw_calls: 0,
        };
        gpu.create_target()?;
        if gpu.surface.is_some() {
            gpu.create_swapchain()?;
        }
        gpu.create_samplers()?;
        Ok(gpu)
    }

    fn create_instance(
        entry: &ash::Entry,
        window: Option<WindowHandles>,
        debug: bool,
    ) -> Result<
        (
            ash::Instance,
            Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
        ),
        RenderError,
    > {
        // SAFETY: pure loader queries.
        let (available_ext, available_layers, version) = unsafe {
            (
                vkr(
                    "vkEnumerateInstanceExtensionProperties",
                    entry.enumerate_instance_extension_properties(None),
                )?,
                vkr(
                    "vkEnumerateInstanceLayerProperties",
                    entry.enumerate_instance_layer_properties(),
                )?,
                entry
                    .try_enumerate_instance_version()
                    .ok()
                    .flatten()
                    .unwrap_or(vk::API_VERSION_1_0),
            )
        };
        if version < vk::API_VERSION_1_2 {
            return Err(RenderError::Device(format!(
                "the Vulkan loader is {}.{}; 1.2 is required (timeline semaphores)",
                vk::api_version_major(version),
                vk::api_version_minor(version)
            )));
        }
        let has_ext = |name: &CStr| {
            available_ext
                .iter()
                .any(|e| e.extension_name_as_c_str().is_ok_and(|n| n == name))
        };
        let mut extensions: Vec<*const c_char> = Vec::new();
        if let Some(w) = window {
            let required = vkr(
                "ash_window::enumerate_required_extensions",
                ash_window::enumerate_required_extensions(w.display),
            )?;
            extensions.extend_from_slice(required);
        }
        let mut flags = vk::InstanceCreateFlags::empty();
        if has_ext(ash::khr::portability_enumeration::NAME) {
            // MoltenVK is a portability implementation and hides behind this flag otherwise.
            extensions.push(ash::khr::portability_enumeration::NAME.as_ptr());
            flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
        }
        let want_debug = debug && has_ext(ash::ext::debug_utils::NAME);
        if want_debug {
            extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        let mut layers: Vec<*const c_char> = Vec::new();
        if debug
            && available_layers
                .iter()
                .any(|l| l.layer_name_as_c_str().is_ok_and(|n| n == VALIDATION_LAYER))
        {
            layers.push(VALIDATION_LAYER.as_ptr());
        }
        let app = vk::ApplicationInfo::default()
            .application_name(c"dereth-client")
            .engine_name(c"dereth-render")
            .api_version(vk::API_VERSION_1_2);
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&extensions)
            .enabled_layer_names(&layers)
            .flags(flags);
        // SAFETY: every pointer in `info` is to a live local or a `'static` name.
        let instance = vkr("vkCreateInstance", unsafe {
            entry.create_instance(&info, None)
        })?;
        let debug_utils = if want_debug {
            let loader = ash::ext::debug_utils::Instance::new(entry, &instance);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                        | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback));
            // SAFETY: `debug_callback` has the required signature and touches only statics.
            match unsafe { loader.create_debug_utils_messenger(&info, None) } {
                Ok(m) => Some((loader, m)),
                Err(_) => None,
            }
        } else {
            None
        };
        Ok((instance, debug_utils))
    }

    /// Pick the physical device and the queue family that can both draw and present.
    fn pick_physical_device(
        instance: &ash::Instance,
        surface: Option<&(ash::khr::surface::Instance, vk::SurfaceKHR)>,
        force_software: bool,
    ) -> Result<(vk::PhysicalDevice, u32, AdapterKind, String), RenderError> {
        // SAFETY: a pure query.
        let devices = vkr("vkEnumeratePhysicalDevices", unsafe {
            instance.enumerate_physical_devices()
        })?;
        let mut best: Option<(i32, vk::PhysicalDevice, u32, AdapterKind, String)> = None;
        for pd in devices {
            // SAFETY: pure queries on a live physical device.
            let (props, families) = unsafe {
                (
                    instance.get_physical_device_properties(pd),
                    instance.get_physical_device_queue_family_properties(pd),
                )
            };
            if props.api_version < vk::API_VERSION_1_2 {
                continue;
            }
            let mut vk12 = vk::PhysicalDeviceVulkan12Features::default();
            let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut vk12);
            // SAFETY: `features2`'s chain is live for the call.
            unsafe { instance.get_physical_device_features2(pd, &mut features2) };
            if vk12.timeline_semaphore != vk::TRUE {
                continue;
            }
            let family = families.iter().enumerate().find_map(|(i, f)| {
                let i = i as u32;
                if !f.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    return None;
                }
                if let Some((loader, s)) = surface {
                    // SAFETY: a pure query on a live surface.
                    let ok = unsafe { loader.get_physical_device_surface_support(pd, i, *s) }
                        .unwrap_or(false);
                    if !ok {
                        return None;
                    }
                }
                Some(i)
            });
            let Some(family) = family else { continue };
            let kind = if props.device_type == vk::PhysicalDeviceType::CPU {
                AdapterKind::Software
            } else {
                AdapterKind::Hardware
            };
            let rank = match props.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 4,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
                vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                vk::PhysicalDeviceType::CPU => 1,
                _ => 0,
            };
            let rank = if force_software {
                if kind == AdapterKind::Software {
                    10
                } else {
                    rank
                }
            } else {
                rank
            };
            let name = props
                .device_name_as_c_str()
                .map_or_else(|_| String::new(), |n| n.to_string_lossy().into_owned());
            if best.as_ref().is_none_or(|b| rank > b.0) {
                best = Some((rank, pd, family, kind, name));
            }
        }
        best.map(|(_, pd, f, k, n)| (pd, f, k, n)).ok_or_else(|| {
            RenderError::Device(
                "no Vulkan 1.2 device with a graphics queue that can present".into(),
            )
        })
    }

    /// The device and its queue, and whether its samplers can carry a level-of-detail bias.
    fn create_device(
        instance: &ash::Instance,
        pd: vk::PhysicalDevice,
        family: u32,
        with_swapchain: bool,
        features: &vk::PhysicalDeviceFeatures,
    ) -> Result<(ash::Device, vk::Queue, bool), RenderError> {
        // SAFETY: a pure query.
        let available = vkr("vkEnumerateDeviceExtensionProperties", unsafe {
            instance.enumerate_device_extension_properties(pd)
        })?;
        let has = |name: &CStr| {
            available
                .iter()
                .any(|e| e.extension_name_as_c_str().is_ok_and(|n| n == name))
        };
        let mut extensions: Vec<*const c_char> = Vec::new();
        if with_swapchain {
            if !has(ash::khr::swapchain::NAME) {
                return Err(RenderError::Device(
                    "the device cannot present (no VK_KHR_swapchain)".into(),
                ));
            }
            extensions.push(ash::khr::swapchain::NAME.as_ptr());
        }
        // A portability implementation (MoltenVK, over Metal) lists what it cannot do in the
        // portability-subset features, and must have the extension enabled when it advertises it.
        // Everything it can do is enabled; a device without the extension has the whole of Vulkan,
        // sampler bias included.
        let portability = has(ash::khr::portability_subset::NAME);
        let mut subset = vk::PhysicalDevicePortabilitySubsetFeaturesKHR::default();
        if portability {
            extensions.push(ash::khr::portability_subset::NAME.as_ptr());
            let mut query = vk::PhysicalDeviceFeatures2::default().push_next(&mut subset);
            // SAFETY: a pure query on a live physical device into live locals.
            unsafe { instance.get_physical_device_features2(pd, &mut query) };
        }
        let sampler_lod_bias = !portability || subset.sampler_mip_lod_bias == vk::TRUE;
        let priorities = [1.0f32];
        let queue_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let enabled = vk::PhysicalDeviceFeatures::default()
            .sampler_anisotropy(features.sampler_anisotropy == vk::TRUE)
            .texture_compression_bc(features.texture_compression_bc == vk::TRUE);
        let mut vk12 = vk::PhysicalDeviceVulkan12Features::default().timeline_semaphore(true);
        let mut info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_info)
            .enabled_extension_names(&extensions)
            .enabled_features(&enabled)
            .push_next(&mut vk12);
        if portability {
            subset.p_next = std::ptr::null_mut();
            info = info.push_next(&mut subset);
        }
        // SAFETY: every pointer in `info` is to a live local.
        let device = vkr("vkCreateDevice", unsafe {
            instance.create_device(pd, &info, None)
        })?;
        // SAFETY: the family was chosen from the device's own list.
        let queue = unsafe { device.get_device_queue(family, 0) };
        Ok((device, queue, sampler_lod_bias))
    }

    fn pick_depth_format(
        instance: &ash::Instance,
        pd: vk::PhysicalDevice,
    ) -> Result<vk::Format, RenderError> {
        for f in [DEPTH_FORMAT, vk::Format::D32_SFLOAT_S8_UINT] {
            // SAFETY: a pure query.
            let props = unsafe { instance.get_physical_device_format_properties(pd, f) };
            if props
                .optimal_tiling_features
                .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
            {
                return Ok(f);
            }
        }
        Err(RenderError::Device("no depth-stencil format".into()))
    }

    fn create_render_pass(
        device: &ash::Device,
        depth_format: vk::Format,
    ) -> Result<vk::RenderPass, RenderError> {
        let attachments = [
            vk::AttachmentDescription::default()
                .format(BACK_BUFFER_FORMAT)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
            vk::AttachmentDescription::default()
                .format(depth_format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .stencil_load_op(vk::AttachmentLoadOp::CLEAR)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
        ];
        let color_ref = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let depth_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };
        let subpass = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_ref)
            .depth_stencil_attachment(&depth_ref)];
        // The previous frame's blit read the colour image and its pass wrote the depth image;
        // this pass's clears must wait for both. On one queue that is a single dependency.
        let dependencies = [
            vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(
                    vk::PipelineStageFlags::TRANSFER
                        | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                )
                .src_access_mask(
                    vk::AccessFlags::TRANSFER_READ
                        | vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                )
                .dst_stage_mask(
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
                )
                .dst_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                ),
            vk::SubpassDependency::default()
                .src_subpass(0)
                .dst_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ),
        ];
        let info = vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpass)
            .dependencies(&dependencies);
        // SAFETY: live locals; owned by `self`.
        vkr("vkCreateRenderPass", unsafe {
            device.create_render_pass(&info, None)
        })
    }

    /// The four descriptor set layouts and the pipeline layout carry the fixed-function state
    /// across: the two constant buffers (set 0), **one set per texture stage** (sets 1 and 2), and
    /// the sampler pair (set 3). Four sets is the Vulkan minimum guarantee, so this binds on every
    /// conforming device.
    fn create_layouts(
        device: &ash::Device,
    ) -> Result<
        (
            vk::DescriptorSetLayout,
            vk::DescriptorSetLayout,
            vk::DescriptorSetLayout,
            vk::PipelineLayout,
        ),
        RenderError,
    > {
        let both = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
        let uniform_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .descriptor_count(1)
                .stage_flags(both),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .descriptor_count(1)
                .stage_flags(both),
        ];
        let texture_bindings = [vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
        let sampler_bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];
        // SAFETY: live locals; the layouts are owned by `self`.
        unsafe {
            let uniform = vkr(
                "vkCreateDescriptorSetLayout(uniforms)",
                device.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&uniform_bindings),
                    None,
                ),
            )?;
            let texture = vkr(
                "vkCreateDescriptorSetLayout(texture)",
                device.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&texture_bindings),
                    None,
                ),
            )?;
            let sampler = vkr(
                "vkCreateDescriptorSetLayout(samplers)",
                device.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&sampler_bindings),
                    None,
                ),
            )?;
            let sets = [uniform, texture, texture, sampler];
            // The landscape splat pipeline's push-constant range, declared here too although no
            // legacy shader reads it: two pipeline layouts are compatible for the sets they share
            // only when their push-constant ranges match, and this keeps a splat draw from
            // disturbing the texture and sampler sets the legacy draws bind.
            let push = [terrain_splat::push_range()];
            let layout = vkr(
                "vkCreatePipelineLayout",
                device.create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&sets)
                        .push_constant_ranges(&push),
                    None,
                ),
            )?;
            Ok((uniform, texture, sampler, layout))
        }
    }

    fn descriptor_pool(
        device: &ash::Device,
        max_sets: u32,
        sizes: &[vk::DescriptorPoolSize],
    ) -> Result<vk::DescriptorPool, RenderError> {
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(max_sets)
            .pool_sizes(sizes);
        // SAFETY: live locals; owned by `self`.
        vkr("vkCreateDescriptorPool", unsafe {
            device.create_descriptor_pool(&info, None)
        })
    }

    fn shader_module(device: &ash::Device, words: &[u32]) -> Result<vk::ShaderModule, RenderError> {
        let info = vk::ShaderModuleCreateInfo::default().code(words);
        // SAFETY: `words` is live for the call; the module is owned by `self`.
        vkr("vkCreateShaderModule", unsafe {
            device.create_shader_module(&info, None)
        })
    }

    // ---- the render target and the swap chain -------------------------------------------------

    /// Create the colour image, the depth image and the framebuffer at `config`'s extent.
    fn create_target(&mut self) -> Result<(), RenderError> {
        let (w, h) = (self.config.width, self.config.height);
        let color = self.create_image(
            w,
            h,
            1,
            BACK_BUFFER_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
            vk::ImageAspectFlags::COLOR,
            TextureFormat::Bgra8,
            "back buffer",
        )?;
        let depth = self.create_image(
            w,
            h,
            1,
            self.depth_format,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL,
            TextureFormat::Bgra8,
            "depth buffer",
        )?;
        let views = [color.view, depth.view];
        let info = vk::FramebufferCreateInfo::default()
            .render_pass(self.render_pass)
            .attachments(&views)
            .width(w)
            .height(h)
            .layers(1);
        // SAFETY: live locals and live views; owned by `self`.
        let framebuffer = vkr("vkCreateFramebuffer", unsafe {
            self.device.create_framebuffer(&info, None)
        })?;
        let old = std::mem::replace(
            &mut self.target,
            Target {
                color,
                depth,
                framebuffer,
                rendered: false,
            },
        );
        self.destroy_target(old);
        Ok(())
    }

    fn destroy_target(&mut self, t: Target) {
        if t.framebuffer != vk::Framebuffer::null() {
            // SAFETY: the caller waited for the GPU; nothing references the framebuffer.
            unsafe { self.device.destroy_framebuffer(t.framebuffer, None) };
        }
        self.destroy_texture(t.color);
        self.destroy_texture(t.depth);
    }

    /// The present mode `present_sync_interval` asks for, out of what the surface offers.
    fn desired_present_mode(&self) -> vk::PresentModeKHR {
        let Some((loader, surface)) = self.surface.as_ref() else {
            return vk::PresentModeKHR::FIFO;
        };
        // SAFETY: a pure query.
        let modes = unsafe {
            loader.get_physical_device_surface_present_modes(self.physical_device, *surface)
        }
        .unwrap_or_default();
        if self.present_sync_interval == 0 {
            for m in [vk::PresentModeKHR::IMMEDIATE, vk::PresentModeKHR::MAILBOX] {
                if modes.contains(&m) {
                    return m;
                }
            }
        }
        vk::PresentModeKHR::FIFO
    }

    /// (Re)create the swap chain at the surface's current extent, or the configured one where the
    /// surface leaves it to the application (Wayland).
    fn create_swapchain(&mut self) -> Result<(), RenderError> {
        let (loader, surface) = self
            .surface
            .as_ref()
            .ok_or_else(|| RenderError::Device("no surface".into()))?;
        let sc_loader = self
            .swapchain_loader
            .as_ref()
            .ok_or_else(|| RenderError::Device("no swapchain loader".into()))?;
        // SAFETY: pure queries on a live surface.
        let (caps, formats) = unsafe {
            (
                vkr(
                    "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
                    loader.get_physical_device_surface_capabilities(self.physical_device, *surface),
                )?,
                vkr(
                    "vkGetPhysicalDeviceSurfaceFormatsKHR",
                    loader.get_physical_device_surface_formats(self.physical_device, *surface),
                )?,
            )
        };
        let format = formats
            .iter()
            .find(|f| {
                f.format == BACK_BUFFER_FORMAT && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .or_else(|| formats.first())
            .copied()
            .ok_or_else(|| RenderError::Device("the surface offers no formats".into()))?;
        let extent = if caps.current_extent.width == u32::MAX {
            vk::Extent2D {
                width: self.config.width,
                height: self.config.height,
            }
        } else {
            caps.current_extent
        };
        if extent.width == 0 || extent.height == 0 {
            return Err(RenderError::Device(
                "the surface has a zero extent (minimised?)".into(),
            ));
        }
        let mut count = caps.min_image_count + 1;
        if caps.max_image_count > 0 {
            count = count.min(caps.max_image_count);
        }
        let present_mode = self.desired_present_mode();
        let old = self
            .swapchain
            .as_ref()
            .map_or(vk::SwapchainKHR::null(), |s| s.handle);
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(*surface)
            .min_image_count(count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old);
        // SAFETY: live locals; the old chain is retired below, after the new one exists.
        let handle = vkr("vkCreateSwapchainKHR", unsafe {
            sc_loader.create_swapchain(&info, None)
        })?;
        // SAFETY: a pure query on the new chain.
        let images = vkr("vkGetSwapchainImagesKHR", unsafe {
            sc_loader.get_swapchain_images(handle)
        })?;
        let mut render_finished = Vec::with_capacity(images.len());
        for _ in &images {
            // SAFETY: owned by the `Swapchain`.
            render_finished.push(vkr("vkCreateSemaphore", unsafe {
                self.device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            })?);
        }
        let new = Swapchain {
            handle,
            images,
            extent,
            present_mode,
            render_finished,
        };
        if let Some(old) = self.swapchain.replace(new) {
            self.destroy_swapchain(old);
        }
        Ok(())
    }

    fn destroy_swapchain(&mut self, s: Swapchain) {
        // SAFETY: the caller waited for the GPU (or the chain was retired by `old_swapchain`).
        unsafe {
            for sem in s.render_finished {
                self.device.destroy_semaphore(sem, None);
            }
            if let Some(l) = self.swapchain_loader.as_ref() {
                l.destroy_swapchain(s.handle, None);
            }
        }
    }

    // ---- resources ------------------------------------------------------------------------------

    fn allocator(&mut self) -> &mut Allocator {
        self.allocator
            .as_mut()
            .expect("the allocator lives as long as the device")
    }

    fn create_buffer(
        &mut self,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
        name: &str,
    ) -> Result<Buffer, RenderError> {
        let info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // SAFETY: a live local; the buffer is owned by the returned `Buffer`.
        let buffer = vkr("vkCreateBuffer", unsafe {
            self.device.create_buffer(&info, None)
        })?;
        // SAFETY: a pure query on the new buffer.
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let allocation = self
            .allocator()
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| RenderError::Device(format!("allocate({name}): {e}")))?;
        // SAFETY: the allocation satisfies the buffer's requirements by construction.
        let bound = unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
        };
        if let Err(e) = bound {
            let _ = self.allocator().free(allocation);
            // SAFETY: the buffer is unbound and unreferenced.
            unsafe { self.device.destroy_buffer(buffer, None) };
            return Err(RenderError::Device(format!(
                "vkBindBufferMemory({name}): {e}"
            )));
        }
        Ok(Buffer {
            buffer,
            allocation: Some(allocation),
            size,
        })
    }

    fn destroy_buffer(&mut self, mut b: Buffer) {
        if let Some(a) = b.allocation.take() {
            let _ = self.allocator().free(a);
        }
        // SAFETY: the caller has waited for every submission that referenced the buffer.
        unsafe { self.device.destroy_buffer(b.buffer, None) };
    }

    #[allow(clippy::too_many_arguments)]
    fn create_image(
        &mut self,
        width: u32,
        height: u32,
        levels: u16,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        aspect: vk::ImageAspectFlags,
        tex_format: TextureFormat,
        name: &str,
    ) -> Result<Texture, RenderError> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(u32::from(levels))
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY: a live local; the image is owned by the returned `Texture`.
        let image = vkr("vkCreateImage", unsafe {
            self.device.create_image(&info, None)
        })?;
        // SAFETY: a pure query on the new image.
        let requirements = unsafe { self.device.get_image_memory_requirements(image) };
        let allocation = match self.allocator().allocate(&AllocationCreateDesc {
            name,
            requirements,
            location: MemoryLocation::GpuOnly,
            linear: false,
            allocation_scheme: AllocationScheme::GpuAllocatorManaged,
        }) {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: unbound and unreferenced.
                unsafe { self.device.destroy_image(image, None) };
                return Err(RenderError::Device(format!("allocate({name}): {e}")));
            }
        };
        // SAFETY: the allocation satisfies the image's requirements by construction.
        if let Err(e) = unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
        } {
            let _ = self.allocator().free(allocation);
            // SAFETY: unbound and unreferenced.
            unsafe { self.device.destroy_image(image, None) };
            return Err(RenderError::Device(format!(
                "vkBindImageMemory({name}): {e}"
            )));
        }
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: aspect,
                base_mip_level: 0,
                level_count: u32::from(levels),
                base_array_layer: 0,
                layer_count: 1,
            });
        // SAFETY: the image is live and bound.
        let view = match unsafe { self.device.create_image_view(&view_info, None) } {
            Ok(v) => v,
            Err(e) => {
                let _ = self.allocator().free(allocation);
                // SAFETY: unreferenced.
                unsafe { self.device.destroy_image(image, None) };
                return Err(RenderError::Device(format!(
                    "vkCreateImageView({name}): {e}"
                )));
            }
        };
        Ok(Texture {
            image,
            view,
            allocation: Some(allocation),
            format,
            tex_format,
            levels,
            width,
            height,
        })
    }

    fn destroy_texture(&mut self, mut t: Texture) {
        if t.image == vk::Image::null() {
            return;
        }
        // SAFETY: the caller has waited for every submission that referenced the image.
        unsafe {
            self.device.destroy_image_view(t.view, None);
            if let Some(a) = t.allocation.take() {
                let _ = self.allocator().free(a);
            }
            self.device.destroy_image(t.image, None);
        }
    }

    /// Record, submit and wait for one command buffer of its own -- *not* the frame ring's.
    /// Resetting a frame's buffer here would discard everything the caller had already recorded
    /// into the open frame, which made creating a texture between `begin_frame` and `end_frame`
    /// silently drop the frame -- and that is exactly the lazy path a terrain merge cache wants.
    fn one_shot(
        &mut self,
        what: &'static str,
        record: impl FnOnce(&ash::Device, vk::CommandBuffer) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.oneshot_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: the pool is owned by `self`; the buffer is freed below after the wait.
        let cmd = vkr(what, unsafe {
            self.device.allocate_command_buffers(&alloc)
        })?[0];
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: a fresh buffer from a pool that allows individual resets.
        let result = unsafe { vkr(what, self.device.begin_command_buffer(cmd, &begin)) }
            .and_then(|()| record(&self.device, cmd))
            // SAFETY: recording is finished.
            .and_then(|()| vkr(what, unsafe { self.device.end_command_buffer(cmd) }))
            .and_then(|()| self.submit_and_wait(&[cmd]));
        // SAFETY: the wait above (or the failure before any submission) means nothing executes it.
        unsafe { self.device.free_command_buffers(self.oneshot_pool, &[cmd]) };
        result
    }

    /// Submit `cmds` signalling the next timeline value, and block until it completes.
    fn submit_and_wait(&mut self, cmds: &[vk::CommandBuffer]) -> Result<(), RenderError> {
        let value = self.submit(cmds)?;
        self.wait_for(value)
    }

    /// Submit `cmds` signalling the next timeline value, and return that value without waiting.
    fn submit(&mut self, cmds: &[vk::CommandBuffer]) -> Result<u64, RenderError> {
        let value = self.next_fence_value;
        self.next_fence_value += 1;
        let values = [value];
        let mut timeline =
            vk::TimelineSemaphoreSubmitInfo::default().signal_semaphore_values(&values);
        let sems = [self.timeline];
        let submit = vk::SubmitInfo::default()
            .command_buffers(cmds)
            .signal_semaphores(&sems)
            .push_next(&mut timeline);
        // SAFETY: every handle is live and owned by `self`.
        vkr("vkQueueSubmit", unsafe {
            self.device
                .queue_submit(self.queue, &[submit], vk::Fence::null())
        })?;
        Ok(value)
    }

    /// [`Self::one_shot`] without the wait: record and submit one command buffer of its own and
    /// return at once. `keep` -- the staging buffers the commands read -- and the command buffer
    /// itself are freed by [`Self::retire_uploads`] once the device has passed the submission.
    ///
    /// Nothing else has to wait for it either. The queue runs submissions in order, and a
    /// pipeline barrier's second scope reaches every command submitted after it, so the barriers
    /// the recording ends with order its writes before any later frame's reads; and a texture
    /// released meanwhile is retired against a later timeline value than this one.
    fn one_shot_nowait(
        &mut self,
        what: &'static str,
        record: impl FnOnce(&ash::Device, vk::CommandBuffer) -> Result<(), RenderError>,
        keep: Vec<Buffer>,
    ) -> Result<(), RenderError> {
        let alloc = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.oneshot_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let cmd = match vkr(what, unsafe {
            // SAFETY: the pool is owned by `self`; the buffer is freed by `retire_uploads`, or
            // below on failure.
            self.device.allocate_command_buffers(&alloc)
        }) {
            Ok(c) => c[0],
            Err(e) => {
                for b in keep {
                    self.destroy_buffer(b);
                }
                return Err(e);
            }
        };
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: a fresh buffer from a pool that allows individual resets.
        let recorded = unsafe { vkr(what, self.device.begin_command_buffer(cmd, &begin)) }
            .and_then(|()| record(&self.device, cmd))
            // SAFETY: recording is finished.
            .and_then(|()| vkr(what, unsafe { self.device.end_command_buffer(cmd) }));
        match recorded.and_then(|()| self.submit(&[cmd])) {
            Ok(value) => {
                self.pending_uploads.push((value, cmd, keep));
                Ok(())
            }
            Err(e) => {
                // SAFETY: nothing was submitted, so nothing executes it.
                unsafe { self.device.free_command_buffers(self.oneshot_pool, &[cmd]) };
                for b in keep {
                    self.destroy_buffer(b);
                }
                Err(e)
            }
        }
    }

    /// Free what the uploads the device has finished were holding.
    fn retire_uploads(&mut self) {
        if self.pending_uploads.is_empty() {
            return;
        }
        let completed = self.completed_value();
        let (done, pending): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_uploads)
            .into_iter()
            .partition(|(v, _, _)| *v <= completed);
        self.pending_uploads = pending;
        for (_, cmd, keep) in done {
            // SAFETY: the timeline has passed the submission, so the device is done with both.
            unsafe { self.device.free_command_buffers(self.oneshot_pool, &[cmd]) };
            for b in keep {
                self.destroy_buffer(b);
            }
        }
    }

    fn wait_for(&self, value: u64) -> Result<(), RenderError> {
        let sems = [self.timeline];
        let values = [value];
        let info = vk::SemaphoreWaitInfo::default()
            .semaphores(&sems)
            .values(&values);
        // SAFETY: live locals on a live timeline semaphore.
        vkr("vkWaitSemaphores", unsafe {
            self.device.wait_semaphores(&info, u64::MAX)
        })
    }

    fn completed_value(&self) -> u64 {
        // SAFETY: a pure query.
        unsafe { self.device.get_semaphore_counter_value(self.timeline) }.unwrap_or(0)
    }

    // ---- pipelines ------------------------------------------------------------------------------

    /// Build (or fetch) the pipeline for a key. This is the PSO catalogue made real: the 15 keys
    /// of `crate::pso::CATALOGUE` each construct exactly one of these.
    ///
    /// # Errors
    /// [`RenderError::Device`] when the driver rejects the state, which is what the catalogue
    /// test is looking for.
    pub fn pipeline_state(&mut self, key: &PipelineKey) -> Result<vk::Pipeline, RenderError> {
        if let Some(p) = self.pipelines.get(key) {
            return Ok(*p);
        }
        let p = self.build_pipeline(key)?;
        self.pipelines.insert(*key, p);
        Ok(p)
    }

    fn build_pipeline(&self, key: &PipelineKey) -> Result<vk::Pipeline, RenderError> {
        let shader = key.stage_ops.pixel_shader();
        let ps = *self
            .fragment_modules
            .get(&shader)
            .ok_or_else(|| RenderError::Device("no pixel shader".into()))?;
        let entry = CStr::from_bytes_with_nul(shader_entry_c(shader))
            .map_err(|_| RenderError::Device("entry name".into()))?;
        self.build_pipeline_with(key, ps, entry, self.pipeline_layout)
    }

    /// [`Self::build_pipeline`] with the pixel shader and the layout given rather than taken from
    /// the key: every fixed-function state is still `key`'s.
    fn build_pipeline_with(
        &self,
        key: &PipelineKey,
        ps: vk::ShaderModule,
        entry: &CStr,
        layout: vk::PipelineLayout,
    ) -> Result<vk::Pipeline, RenderError> {
        let vs = *self
            .vertex_modules
            .get(&key.vertex_format)
            .ok_or_else(|| RenderError::Device("no vertex shader for format".into()))?;
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vs)
                .name(c"vs_main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(ps)
                .name(entry),
        ];

        // The input layout, from the five FVF codes.
        let bindings = [vk::VertexInputBindingDescription {
            binding: 0,
            stride: key.vertex_format.stride(),
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let attributes: Vec<vk::VertexInputAttributeDescription> = key
            .vertex_format
            .elements()
            .iter()
            .map(|(e, off)| vk::VertexInputAttributeDescription {
                location: e.location(),
                binding: 0,
                format: match e.attribute_format(self.bgra_vertex_colour) {
                    crate::vertex::AttributeFormat::Float4 => vk::Format::R32G32B32A32_SFLOAT,
                    crate::vertex::AttributeFormat::Float3 => vk::Format::R32G32B32_SFLOAT,
                    crate::vertex::AttributeFormat::Float2 => vk::Format::R32G32_SFLOAT,
                    crate::vertex::AttributeFormat::Bgra8Unorm => vk::Format::B8G8R8A8_UNORM,
                    crate::vertex::AttributeFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
                },
                offset: *off,
            })
            .collect();
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attributes);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(match key.cull.face() {
                crate::pso::CullFace::None => vk::CullModeFlags::NONE,
                // D3DCULL_CW culls clockwise-wound (front) triangles with the client's winding.
                crate::pso::CullFace::Front => vk::CullModeFlags::FRONT,
                crate::pso::CullFace::Back => vk::CullModeFlags::BACK,
            })
            // `FrontCounterClockwise = FALSE`, in the D3D framebuffer space the negative viewport
            // reproduces.
            .front_face(match crate::pso::FRONT_FACE {
                crate::pso::FrontFace::Clockwise => vk::FrontFace::CLOCKWISE,
                crate::pso::FrontFace::CounterClockwise => vk::FrontFace::COUNTER_CLOCKWISE,
            })
            // Do not add a depth bias. The client relies on its 0.01 terrain z-fight adjustment and
            // `LESSEQUAL` instead.
            .depth_bias_enable(false)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(key.z_write)
            .depth_compare_op(to_vk_compare(key.z_func))
            .depth_bounds_test_enable(false)
            // Stencil is disabled everywhere in the client.
            .stencil_test_enable(false);
        let blend_attachment = [vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(key.alpha_blend)
            .src_color_blend_factor(to_vk_blend(key.src_blend))
            .dst_color_blend_factor(to_vk_blend(key.dst_blend))
            .color_blend_op(vk::BlendOp::ADD)
            // D3D9 without SEPARATEALPHABLENDENABLE uses the same factor for alpha, taking each
            // colour factor's alpha channel. See `to_vk_blend_alpha`.
            .src_alpha_blend_factor(to_vk_blend_alpha(key.src_blend))
            .dst_alpha_blend_factor(to_vk_blend_alpha(key.dst_blend))
            .alpha_blend_op(vk::BlendOp::ADD)
            // Preserve COLORWRITEENABLE = 7: red | green | blue, with alpha masked off.
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B,
            )];
        let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .attachments(&blend_attachment);
        let dynamic = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic_state)
            .layout(layout)
            .render_pass(self.render_pass)
            .subpass(0);
        // SAFETY: `info` and everything it points at are alive for the call; the driver copies
        // what it needs.
        match unsafe {
            self.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
        } {
            Ok(p) => Ok(p[0]),
            Err((_, e)) => Err(RenderError::Device(format!(
                "vkCreateGraphicsPipelines({key:?}): {e}{}",
                self.debug_messages()
            ))),
        }
    }

    /// Drain what the validation layer said, if it is on. Without this a rejected pipeline
    /// reports only an error code, which says nothing about which field the driver disliked.
    fn debug_messages(&self) -> String {
        let mut out = String::new();
        if let Ok(mut m) = DEBUG_MESSAGES.lock() {
            for text in m.drain(..) {
                out.push('\n');
                out.push_str(&text);
            }
        }
        out
    }

    /// Construct every state in the catalogue, so the driver validates all of them. The
    /// catalogue test's entry point.
    ///
    /// # Errors
    /// The first key the driver rejects.
    pub fn build_whole_catalogue(&mut self) -> Result<usize, RenderError> {
        let mut built = 0;
        for row in crate::pso::CATALOGUE {
            for vf in VertexFormat::all() {
                let key = PipelineKey {
                    vertex_format: vf,
                    src_blend: row.src,
                    dst_blend: row.dst,
                    alpha_blend: row.alpha_blend,
                    alpha_test: row.alpha_test,
                    z_write: row.z_write,
                    z_func: row.z_func,
                    cull: Cull::Cw,
                    stage_ops: crate::pso::StageOps::BASE,
                    fog: true,
                    lighting: false,
                };
                self.pipeline_state(&key)?;
                built += 1;
            }
        }
        Ok(built)
    }

    // ---- device state ---------------------------------------------------------------------------

    /// Set the gamma ramp. Vulkan has no `SetGammaRamp`, so a rebuild must apply the same clamped
    /// curve, `out = 255*i + 510*i*v`, as a post-process or swap-chain colour transform. The ramp
    /// itself is [`crate::gamma_ramp`]; this records the value the post-process would use.
    pub fn set_gamma(&mut self, brightness: f32) {
        self.gamma = brightness.clamp(-0.2, 1.0);
    }

    #[must_use]
    pub fn gamma(&self) -> f32 {
        self.gamma
    }

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Apply the client's presentation-setup synchronization policy.
    ///
    /// The retail windowed arm always leaves `PresentationInterval = IMMEDIATE`; only logical
    /// full screen consults the full-screen sync-to-display-refresh setting. The interval maps to
    /// a present mode -- `IMMEDIATE` (or `MAILBOX`) for 0, `FIFO` for 1 -- and the swap chain is
    /// rebuilt at the next `end_frame` if the mode changed.
    pub fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
        self.present_sync_interval = u32::from(full_screen && sync_to_refresh);
    }

    /// The interval currently configured for the next real swap-chain presentation.
    #[must_use]
    pub fn present_sync_interval(&self) -> u32 {
        self.present_sync_interval
    }

    /// The interval actually in force at the last presentation.
    ///
    /// `None` means this is the offscreen/headless target; it must not be cited as window proof.
    #[must_use]
    pub fn last_present_sync_interval(&self) -> Option<u32> {
        self.last_present_sync_interval
    }

    // ---- the frame bracket ----------------------------------------------------------------------

    /// Begin a frame: wait for this ring slot, reset, clear (flags 7: target, stencil and
    /// z; colour black, z = 1.0) and open the command buffer.
    ///
    /// # Errors
    /// Any failure from the driver, or a frame that is already open: a frame cannot be silently
    /// abandoned and begun again, so a release held inside it is never stranded.
    pub fn begin_frame(&mut self) -> Result<(), RenderError> {
        if self.frame_open {
            return Err(RenderError::Device(
                "begin_frame: a frame is already open (the previous one was never ended)".into(),
            ));
        }
        let want = self.frames[self.frame_index].fence_value;
        if want != 0 {
            self.wait_for(want)?;
        }
        // Any slot and texture released far enough back that its fence has completed can now go
        // back into circulation. This is the only place a released slot re-enters it during normal
        // frame pacing; `upload_texture` runs it once more behind its own `wait_idle`.
        self.collect_retired();
        // The wait above proves the GPU is done with last time's command buffer, so the arenas it
        // outgrew can finally go.
        let retired = std::mem::take(&mut self.frames[self.frame_index].retired);
        for arena in retired {
            self.destroy_buffer(arena.buffer);
        }
        let frame = &mut self.frames[self.frame_index];
        frame.upload_used = 0;
        // SAFETY: the GPU is done with this pool, which the wait above established.
        unsafe {
            vkr(
                "vkResetCommandPool",
                self.device
                    .reset_command_pool(frame.pool, vk::CommandPoolResetFlags::empty()),
            )?;
            vkr(
                "vkResetDescriptorPool",
                self.device
                    .reset_descriptor_pool(frame.desc_pool, vk::DescriptorPoolResetFlags::empty()),
            )?;
        }
        // The arena's set came from the pool just reset; make it again.
        if let Some(arena) = frame.upload.as_mut() {
            let set = Self::arena_set(
                &self.device,
                self.uniform_set_layout,
                frame.desc_pool,
                arena.buffer.buffer,
            )?;
            arena.set = set;
        }
        let cmd = frame.cmd;
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: the pool was reset; the buffer is in the initial state.
        vkr("vkBeginCommandBuffer", unsafe {
            self.device.begin_command_buffer(cmd, &begin)
        })?;
        self.frame_open = true;

        // The frame clear is black with the hard-coded alpha 0x66,
        // which is invisible because COLORWRITEENABLE masks the alpha channel; the float clear
        // here writes the same visible result. See crate::pack_clear_colour.
        let clears = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.4],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        let extent = vk::Extent2D {
            width: self.config.width,
            height: self.config.height,
        };
        let info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.target.framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent,
            })
            .clear_values(&clears);
        // SAFETY: the buffer is recording and every handle is live.
        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &info, vk::SubpassContents::INLINE)
        };
        self.target.rendered = true;
        // There is no scissor test anywhere in the client (D3DRS_SCISSORTESTENABLE is 0 and
        // SetScissorRect is never called), so the rectangle is the whole target.
        self.reset_viewport();
        // At scene begin, not when the filter mode is set: normal bias changes at the scene edge.
        self.sharp_lod_bias = self.texture_filtering == 2;
        Ok(())
    }

    /// End a frame: close, submit, present, advance the ring, and bump the frame stamp.
    ///
    /// # Errors
    /// Any failure from the driver.
    pub fn end_frame(&mut self) -> Result<(), RenderError> {
        let cmd = self.frames[self.frame_index].cmd;
        // SAFETY: the pass was begun by `begin_frame`.
        unsafe { self.device.cmd_end_render_pass(cmd) };

        // The swap chain: acquire, blit the finished frame into the acquired image, present.
        let mut wait_sems: Vec<vk::Semaphore> = Vec::new();
        let mut signal_sems: Vec<vk::Semaphore> = vec![self.timeline];
        let mut acquired: Option<u32> = None;
        if self.swapchain.is_some() {
            if self
                .swapchain
                .as_ref()
                .is_some_and(|s| s.present_mode != self.desired_present_mode())
            {
                self.wait_idle()?;
                self.create_swapchain()?;
            }
            let image_available = self.frames[self.frame_index].image_available;
            acquired = self.acquire(image_available)?;
            if let Some(index) = acquired {
                let sc = self.swapchain.as_ref().expect("checked above");
                let image = sc.images[index as usize];
                let extent = sc.extent;
                wait_sems.push(image_available);
                signal_sems.push(sc.render_finished[index as usize]);
                // SAFETY: the buffer is recording; the colour image is in the pass's final
                // layout and the swap-chain image was just acquired.
                unsafe { self.record_present_blit(cmd, image, extent) };
            }
        }
        // SAFETY: recording is finished.
        vkr("vkEndCommandBuffer", unsafe {
            self.device.end_command_buffer(cmd)
        })?;

        let value = self.next_fence_value;
        self.next_fence_value += 1;
        let signal_values: Vec<u64> = signal_sems
            .iter()
            .map(|s| if *s == self.timeline { value } else { 0 })
            .collect();
        let wait_values: Vec<u64> = wait_sems.iter().map(|_| 0).collect();
        let wait_stages: Vec<vk::PipelineStageFlags> = wait_sems
            .iter()
            .map(|_| vk::PipelineStageFlags::TRANSFER)
            .collect();
        let mut timeline = vk::TimelineSemaphoreSubmitInfo::default()
            .signal_semaphore_values(&signal_values)
            .wait_semaphore_values(&wait_values);
        let cmds = [cmd];
        let submit = vk::SubmitInfo::default()
            .command_buffers(&cmds)
            .wait_semaphores(&wait_sems)
            .wait_dst_stage_mask(&wait_stages)
            .signal_semaphores(&signal_sems)
            .push_next(&mut timeline);
        // SAFETY: every handle is live and owned by `self`.
        vkr("vkQueueSubmit(frame)", unsafe {
            self.device
                .queue_submit(self.queue, &[submit], vk::Fence::null())
        })?;

        if let Some(index) = acquired {
            let sc = self.swapchain.as_ref().expect("acquired from it");
            let (loader, handle, finished) = (
                self.swapchain_loader
                    .as_ref()
                    .expect("a swap chain has a loader"),
                sc.handle,
                sc.render_finished[index as usize],
            );
            let waits = [finished];
            let chains = [handle];
            let indices = [index];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&waits)
                .swapchains(&chains)
                .image_indices(&indices);
            // SAFETY: the submission signalling `finished` was just queued.
            match unsafe { loader.queue_present(self.queue, &present) } {
                Ok(_) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => {}
                Err(e) => return Err(RenderError::Device(format!("vkQueuePresentKHR: {e}"))),
            }
            self.last_present_sync_interval = Some(self.present_sync_interval);
        }
        {
            let frame = &mut self.frames[self.frame_index];
            frame.fence_value = value;
            frame.high_water = frame.high_water.max(frame.upload_used);
        }
        // The buffer that may have referenced these slots has now been submitted, and `value` is
        // signalled behind it. This — not the release call, and not a `wait_idle` an intervening
        // `upload_texture` performed — is what puts them on the clock.
        self.flush_frame_releases(value);
        self.frame_open = false;
        self.frame_index = (self.frame_index + 1) % FRAME_COUNT;
        self.frame_stamp += 1;
        Ok(())
    }

    /// Acquire the next swap-chain image, rebuilding the chain once if the surface changed.
    /// `None` means this frame cannot be presented (the window is minimised); it is still drawn.
    fn acquire(&mut self, image_available: vk::Semaphore) -> Result<Option<u32>, RenderError> {
        for attempt in 0..2 {
            let Some(sc) = self.swapchain.as_ref() else {
                return Ok(None);
            };
            let loader = self
                .swapchain_loader
                .as_ref()
                .expect("a swap chain has a loader");
            // SAFETY: the chain and semaphore are live; the semaphore is unsignalled (its last
            // wait was this slot's previous submission, which the ring wait retired).
            match unsafe {
                loader.acquire_next_image(sc.handle, u64::MAX, image_available, vk::Fence::null())
            } {
                Ok((index, _suboptimal)) => return Ok(Some(index)),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) if attempt == 0 => {
                    self.wait_idle()?;
                    if self.create_swapchain().is_err() {
                        return Ok(None);
                    }
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(None),
                Err(e) => return Err(RenderError::Device(format!("vkAcquireNextImageKHR: {e}"))),
            }
        }
        Ok(None)
    }

    /// Blit the finished colour image into a swap-chain image and leave it presentable.
    ///
    /// # Safety
    /// `cmd` must be recording outside a render pass, after this frame's pass.
    unsafe fn record_present_blit(
        &self,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        extent: vk::Extent2D,
    ) {
        let to_dst = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(mipgen::level_range(0));
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_dst],
        );
        let (w, h) = (self.config.width as i32, self.config.height as i32);
        let blit = vk::ImageBlit::default()
            .src_subresource(mipgen::layers(0))
            .src_offsets([vk::Offset3D::default(), vk::Offset3D { x: w, y: h, z: 1 }])
            .dst_subresource(mipgen::layers(0))
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: extent.width as i32,
                    y: extent.height as i32,
                    z: 1,
                },
            ]);
        self.device.cmd_blit_image(
            cmd,
            self.target.color.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[blit],
            vk::Filter::NEAREST,
        );
        let to_present = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::empty())
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(mipgen::level_range(0));
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_present],
        );
    }

    /// Block until every submitted frame has retired.
    ///
    /// # Errors
    /// Any failure from the driver.
    pub fn wait_idle(&mut self) -> Result<(), RenderError> {
        self.submit_and_wait(&[])
    }

    /// Read the render target back as BGRA8, tightly packed.
    ///
    /// A fixed scene rendered offscreen at 800×600 must read back as the same RGBA8 across three
    /// consecutive runs on the same machine.
    ///
    /// # Errors
    /// Any failure from the driver.
    pub fn capture(&mut self) -> Result<CapturedImage, RenderError> {
        let (w, h) = (self.config.width, self.config.height);
        let total = u64::from(w) * u64::from(h) * 4;
        let readback = self.create_buffer(
            total,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
            "capture",
        )?;
        let buffer = readback.buffer;
        let image = self.target.color.image;
        let rendered = self.target.rendered;
        let result = self.one_shot("capture", |device, cmd| {
            // SAFETY: the buffer is recording; the colour image is either in the pass's final
            // layout or has never been written (in which case it is cleared here first, so a
            // capture before any frame is black rather than undefined).
            unsafe {
                if !rendered {
                    mipgen::transition_level(
                        device,
                        cmd,
                        image,
                        0,
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::AccessFlags::empty(),
                        vk::AccessFlags::TRANSFER_WRITE,
                    );
                    device.cmd_clear_color_image(
                        cmd,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue { float32: [0.0; 4] },
                        &[mipgen::level_range(0)],
                    );
                    mipgen::transition_level(
                        device,
                        cmd,
                        image,
                        0,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::AccessFlags::TRANSFER_WRITE,
                        vk::AccessFlags::TRANSFER_READ,
                    );
                }
                let region = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(mipgen::layers(0))
                    .image_extent(vk::Extent3D {
                        width: w,
                        height: h,
                        depth: 1,
                    });
                device.cmd_copy_image_to_buffer(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer,
                    &[region],
                );
            }
            Ok(())
        });
        self.target.rendered = true;
        let bytes = result.and_then(|()| readback.read(0, total as usize));
        self.destroy_buffer(readback);
        bytes.map(|bgra| CapturedImage {
            width: w,
            height: h,
            bgra,
        })
    }

    // ---- the upload ring ------------------------------------------------------------------------

    /// The dynamic vertex ring. Reproduces the client's stream fill and its buffer reset:
    ///
    /// * a frame's dynamic data is **contiguous** — one arena per frame slot, bump-allocated;
    /// * the buffer **only grows**, to the previous frame's high-water mark, and never shrinks
    ///   within a session;
    /// * a request larger than the whole buffer fails rather than wrapping
    ///   (a count above the buffer's vertex capacity returns false).
    ///
    /// Returns the byte offset of the copied bytes inside the frame's arena (the D3D12 build
    /// returned a GPU virtual address; a caller only ever uses it to reserve room).
    ///
    /// # Errors
    /// [`RenderError::Device`] when the arena cannot be created or mapped.
    pub fn upload_bytes(&mut self, data: &[u8]) -> Result<u64, RenderError> {
        self.upload_internal(data).map(|(_, offset, _)| offset)
    }

    /// The ring itself: the arena's buffer, the offset, and the descriptor set that views the
    /// arena as the two constant buffers.
    fn upload_internal(
        &mut self,
        data: &[u8],
    ) -> Result<(vk::Buffer, u64, vk::DescriptorSet), RenderError> {
        // 256-byte alignment keeps a dynamic uniform-buffer offset legal at any returned address
        // (`minUniformBufferOffsetAlignment` is at most 256).
        const ALIGN: u64 = 256;
        let want = (data.len() as u64).div_ceil(ALIGN) * ALIGN;
        let index = self.frame_index;
        let need = self.frames[index].upload_used + want;
        if need > self.frames[index].upload_capacity {
            // Grow to the high-water mark, doubling so a frame that grows steadily does not
            // reallocate every draw. The original grows once per frame in its dynamic-buffer reset; the
            // observable behaviour to preserve is only that it grows and never shrinks.
            let capacity = need
                .max(self.frames[index].upload_capacity * 2)
                .max(64 * 1024);
            let buffer = self.create_buffer(
                capacity,
                vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::UNIFORM_BUFFER,
                MemoryLocation::CpuToGpu,
                "dynamic arena",
            )?;
            let frame = &mut self.frames[index];
            let set = Self::arena_set(
                &self.device,
                self.uniform_set_layout,
                frame.desc_pool,
                buffer.buffer,
            )?;
            // Retire the old arena; do not release it. Every offset already handed out this frame
            // points into it and the command buffer holding them has not been submitted, so
            // dropping it here is a use-after-free. `begin_frame` frees these behind the fence.
            if let Some(old) = frame.upload.replace(UploadArena { buffer, set }) {
                frame.retired.push(old);
            }
            frame.upload_capacity = capacity;
            frame.upload_used = 0;
        }
        let frame = &mut self.frames[index];
        let offset = frame.upload_used;
        let arena = frame
            .upload
            .as_mut()
            .ok_or_else(|| RenderError::Device("no upload arena".into()))?;
        arena.buffer.write(offset as usize, data)?;
        frame.upload_used += want;
        Ok((arena.buffer.buffer, offset, arena.set))
    }

    /// The descriptor set that views an arena as `PerFrame` at binding 0 and `PerDraw` at
    /// binding 1, each with a dynamic offset.
    fn arena_set(
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
        pool: vk::DescriptorPool,
        buffer: vk::Buffer,
    ) -> Result<vk::DescriptorSet, RenderError> {
        let layouts = [layout];
        let alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: the pool is sized for `ARENA_SETS_PER_FRAME` sets and reset every frame.
        let set = vkr("vkAllocateDescriptorSets(arena)", unsafe {
            device.allocate_descriptor_sets(&alloc)
        })?[0];
        let infos = [
            vk::DescriptorBufferInfo {
                buffer,
                offset: 0,
                range: std::mem::size_of::<PerFrameConstants>() as u64,
            },
            vk::DescriptorBufferInfo {
                buffer,
                offset: 0,
                range: std::mem::size_of::<PerDrawConstants>() as u64,
            },
        ];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .buffer_info(&infos[0..1]),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .buffer_info(&infos[1..2]),
        ];
        // SAFETY: a fresh set nothing has bound yet; the buffer is live.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        Ok(set)
    }

    /// The high-water mark of dynamic bytes in the current frame slot. The client's equivalent is
    /// its ideal vertex count, which is what its per-frame reset grows the buffer to.
    #[must_use]
    pub fn upload_high_water(&self) -> u64 {
        self.frames[self.frame_index].high_water
    }

    // ---- drawing --------------------------------------------------------------------------------

    /// Draw one batch of vertices straight out of the dynamic ring, under a given key.
    ///
    /// This is the shape of the client's dynamic-primitive pass: fill the per-format
    /// dynamic stream, then draw. **It draws exactly what it is handed, exactly now** — no sorting,
    /// no culling, no reordering.
    ///
    /// # Errors
    /// Any failure from the driver.
    pub fn draw_dynamic(
        &mut self,
        key: &PipelineKey,
        constants: &crate::DrawConstants,
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
        let pipeline = self.pipeline_state(key)?;
        let layout = self.pipeline_layout;
        let lod_bias = self
            .bound_sampler
            .get()
            .map_or(0.0, |i| self.sampler_descriptions[i as usize].mip_lod_bias);
        self.draw_prepared(
            key,
            pipeline,
            layout,
            constants,
            per_frame,
            per_draw,
            vertices,
            lod_bias,
            |_, _| {},
        )
    }

    /// The body of [`Self::draw_dynamic`] with the pipeline and its layout chosen by the caller,
    /// and `bind` run after the constant set is bound and before the draw, for anything else the
    /// pipeline reads. The layout's set 0 must be the constant set. `lod_bias` is the bias the
    /// draw's samplers carry, which the pixel shader applies where they cannot.
    #[allow(clippy::too_many_arguments)]
    fn draw_prepared(
        &mut self,
        key: &PipelineKey,
        pipeline: vk::Pipeline,
        layout: vk::PipelineLayout,
        constants: &crate::DrawConstants,
        per_frame: &PerFrameConstants,
        per_draw: &PerDrawConstants,
        vertices: &[u8],
        lod_bias: f32,
        bind: impl FnOnce(&ash::Device, vk::CommandBuffer),
    ) -> Result<(), RenderError> {
        let stride = key.vertex_format.stride();
        let count = vertices.len() as u32 / stride;
        self.draw_calls += 1;

        let mut draw = *per_draw;
        // `.xy` is the `D3DTS_TEXTURE0` translation; `.zw` is left alone, because the
        // `PRETRANSFORMED` permutation reads the viewport extent out of it and
        // [`Self::draw_portal_poly`] is the one caller that puts something there.
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

        // The gamma is *device* state in the client — its gamma brightness value, set
        // once when the gamma is set and then applied by the display hardware
        // to every pixel of every frame until it is set again. Overwriting the field here rather
        // than asking every caller to fill it keeps that property: no draw can opt out of it.
        let mut frame = *per_frame;
        frame.screen[0] = self.gamma;
        // Read only by pixel shaders that apply the bias themselves; the field is otherwise
        // unused.
        frame.screen[1] = if self.shader_lod_bias { lod_bias } else { 0.0 };

        // The two constant blocks go up as one contiguous upload, so they share an arena and the
        // one descriptor set that views it; the vertices go up on their own.
        let mut block = vec![0u8; CONSTANT_BLOCK_BYTES];
        block[..std::mem::size_of::<PerFrameConstants>()].copy_from_slice(as_bytes(&frame));
        block[PER_DRAW_OFFSET..PER_DRAW_OFFSET + std::mem::size_of::<PerDrawConstants>()]
            .copy_from_slice(as_bytes(&draw));
        let (vb, vb_offset, _) = self.upload_internal(vertices)?;
        let (_, cb_offset, set) = self.upload_internal(&block)?;
        let cmd = self.frames[self.frame_index].cmd;
        // SAFETY: the buffer is recording inside the pass (begin_frame opened it), the pipeline
        // and layout are alive, and every offset points inside an upload arena that stays alive
        // until this frame's fence retires.
        unsafe {
            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &[set],
                &[cb_offset as u32, cb_offset as u32 + PER_DRAW_OFFSET as u32],
            );
            bind(&self.device, cmd);
            self.device
                .cmd_bind_vertex_buffers(cmd, 0, &[vb], &[vb_offset]);
            self.device.cmd_draw(cmd, count, 1, 0, 0);
        }
        Ok(())
    }

    /// The **portal depth stamp**,
    /// and the only pre-transformed (`D3DFVF_XYZRHW`, FVF `0x144`) draw in the whole client.
    ///
    /// `clip` is the portal polygon after transformation and polygon clipping, in **clip space**:
    /// `(x·w, y·w, z·w, w)` with `x` and
    /// `y` in `[-w, w]` and `z` in `[0, w]`, which is what `view_proj * v` produces. Fewer than
    /// three vertices draws nothing, which is the client's own vertex-count guard; the caller
    /// owns the clip because the view polygon it clips against belongs to the traversal.
    ///
    /// `mask` is [`crate::pso::portal_stamp_mask::INDOOR`] (6) or `BUILDING` (7). Bit 0 replaces the
    /// polygon's own depth with the constant `0.999999`, bit 1 forces the vertex alpha to 0, bit 2
    /// enables the depth write. Both shipped masks set bit 1, so this writes **depth only**.
    ///
    /// Three details are the client's and are not tidied:
    ///
    /// * the primitive is a `D3DPT_TRIANGLEFAN` of `n - 2` triangles. The fan is expanded to the
    ///   list `(0, i, i+1)` — the same triangles in the same order, which for a `CULLMODE_NONE`
    ///   depth write is the same coverage;
    /// * the vertex holds **screen** coordinates, not clip ones, because that is what `XYZRHW`
    ///   means; the viewport inverse is in the shader (`legacy.wgsl`, `PRETRANSFORMED`);
    /// * the client unbinds texture stage 0. The pixel shader samples `t0`
    ///   unconditionally, so a 1x1 opaque-white texture stands in for it — created once here, on
    ///   first use. It is unobservable either way: the vertex alpha is 0, so `SRCALPHA/INVSRCALPHA`
    ///   leaves the render target exactly as it found it whatever the sample returns.
    ///
    /// # Errors
    /// Any failure from the driver, including a full descriptor budget on the first call.
    pub fn draw_portal_poly(
        &mut self,
        per_frame: &PerFrameConstants,
        clip: &[[f32; 4]],
        mask: u8,
    ) -> Result<(), RenderError> {
        // Fewer than three vertices: a polygon the clipper reduced below a triangle is not drawn.
        if clip.len() < 3 {
            return Ok(());
        }
        let slot = self.stamp_stage_texture()?;
        self.portal_stamps += 1;
        let key = PipelineKey::portal_stamp(mask);
        let (w, h) = self.size();
        #[allow(clippy::cast_precision_loss)] // a back-buffer extent
        let (fw, fh) = (w as f32, h as f32);
        // The client's portal-stamp colour cycles through eight hues starting at 0xFFFFFF. The
        // alpha byte is 0 for both shipped masks, so the hue is never visible on screen and the
        // cycle is not reproduced here.
        let diffuse = crate::pso::portal_stamp_diffuse(mask, 0x00FF_FFFF);

        let mut screen: Vec<[f32; 4]> = Vec::with_capacity(clip.len());
        for p in clip {
            // Each point lands at `(viewportX + xw/w, viewportY + yw/w)`. The viewport origin
            // is (0, 0) here — `Gpu` renders to the whole target — and the NDC-to-pixel scale is the
            // half that the client's start-of-frame transform had already folded into its own
            // screen transform.
            let iw = if p[3] == 0.0 { 0.0 } else { 1.0 / p[3] };
            let x = (p[0] * iw).mul_add(0.5, 0.5) * fw;
            let y = (p[1] * iw).mul_add(-0.5, 0.5) * fh;
            let z = if mask & crate::pso::portal_stamp_mask::CONSTANT_DEPTH != 0 {
                crate::pso::PORTAL_STAMP_FAR_DEPTH
            } else {
                p[2] * iw
            };
            screen.push([x, y, z, iw]);
        }

        // The fan, expanded. `stride = 0x1C`: float4 position, D3DCOLOR diffuse, float2 uv.
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

        // The viewport extent, which the `PRETRANSFORMED` vertex shader needs to undo the screen
        // transform. `uv_offset.xy` is the `D3DTS_TEXTURE0` translation and is zero here: a
        // pre-transformed draw has no texture matrix.
        let mut draw = PerDrawConstants::identity();
        draw.uv_offset = [0.0, 0.0, fw, fh];
        self.bind_texture(slot, 3);
        self.draw_dynamic(
            &key,
            &crate::DrawConstants::default(),
            per_frame,
            &draw,
            &vertices,
        )
    }

    /// The stand-in for an unbound texture stage 0: a 1x1 opaque-white texture, uploaded once.
    fn stamp_stage_texture(&mut self) -> Result<TextureSlot, RenderError> {
        if let Some(s) = self.stamp_texture {
            return Ok(s);
        }
        let t = TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0xFF, 0xFF, 0xFF, 0xFF]],
        };
        let slot = self.upload_texture(&t)?;
        self.stamp_texture = Some(slot);
        Ok(slot)
    }

    /// A presentation change, and the device reset it drives.
    ///
    /// Reproduces the observable order: wait for the GPU, drop every view, resize the swap chain,
    /// rebuild the depth buffer and the views, then re-run the equivalent of
    /// the client's display-mode-change handler (re-apply the gamma). The three black frames the original flushes
    /// are left to the caller, because they are frames rather than device state.
    ///
    /// # Errors
    /// Any failure from the driver.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::BadDimensions {
                width,
                height,
                reason: "a presentation must have a non-zero extent",
            });
        }
        self.wait_idle()?;
        self.config.width = width;
        self.config.height = height;
        self.create_target()?;
        if self.surface.is_some() {
            self.create_swapchain()?;
        }
        // The client's display-mode-change handler re-applies the gamma after every successful reset.
        let g = self.gamma;
        self.set_gamma(g);
        Ok(())
    }

    // ---- textures -------------------------------------------------------------------------------

    /// Upload a decoded texture and return a handle to its descriptor.
    ///
    /// Equivalent to [`Self::upload_texture_keyed`] with [`TextureKey::UNCACHED`], which is
    /// When `key == 0`, the texture goes in the uncached texture table,
    /// is never shared with another owner, and lives exactly as long as its one link.
    ///
    /// **The returned slot is owned by the caller and must be handed back** with
    /// [`Self::release_texture`] when its owner goes away.
    ///
    /// # Errors
    /// Any failure from the driver, a level whose byte count does not match its extent, or a full
    /// descriptor budget (which is counted in [`DescriptorStats::exhaustions`]).
    pub fn upload_texture(&mut self, t: &TextureData) -> Result<TextureSlot, RenderError> {
        self.upload_texture_keyed(TextureKey::UNCACHED, t)
    }

    /// Upload a decoded texture, or take a second reference on the one already cached under `key`.
    ///
    /// This is the client's combined-texture cache: a non-zero key that is already present `AddRef`s and returns
    /// the existing texture without touching the device; a zero key never hits. Build the key with
    /// [`crate::descriptor::combined_texture_key`] and wrap it in the [`TextureKey`] constructor
    /// for **your own producer**.
    ///
    /// Every returned slot — cache hit or fresh upload — is one link that the caller owns and must
    /// [`Self::release_texture`].
    ///
    /// # Errors
    /// Any failure from the driver, a level whose byte count does not match its extent, or a full
    /// descriptor budget.
    pub fn upload_texture_keyed(
        &mut self,
        key: TextureKey,
        t: &TextureData,
    ) -> Result<TextureSlot, RenderError> {
        self.upload_texture_internal(key, t, false)
    }

    /// A world image's video-memory copy. Eligible
    /// single-level uncompressed sources get device-generated LINEAR sublevels; source bytes
    /// remain unchanged. Compressed/provided chains retain their levels. This is deliberately
    /// separate from UI surface, font, movie and debug uploads, which do not request AUTOGEN.
    ///
    /// # Errors
    /// Upload/device failures, or a key from a producer that is not an image-texture/world owner.
    pub fn upload_imgtex_keyed(
        &mut self,
        key: TextureKey,
        t: &TextureData,
    ) -> Result<TextureSlot, RenderError> {
        if key.space() != crate::TextureSpace::World {
            return Err(RenderError::Unsupported(
                "runtime image texture mips require a world-owner key",
            ));
        }
        self.upload_texture_internal(key, t, true)
    }

    /// Whether an upload under `key` would be a cache hit, without taking a link.
    #[must_use]
    pub fn has_texture_key(&self, key: TextureKey) -> bool {
        self.texture_table.contains(key)
    }

    /// Whether the current device's BGRA8 format supports this backend's runtime autogen path.
    #[must_use]
    pub fn imgtex_autogen_supported(&self) -> bool {
        self.imgtex_autogen_supported
    }

    fn upload_texture_internal(
        &mut self,
        key: TextureKey,
        t: &TextureData,
        imgtex: bool,
    ) -> Result<TextureSlot, RenderError> {
        // "if (key != 0 and texture_table[key] exists) { AddRef; return it }" -- before any device
        // work, because the whole point of the cache is that the upload does not happen.
        if let Some(slot) = self.texture_table.get(key) {
            return Ok(TextureSlot(slot));
        }
        // The client builds the compressed system chain before the video-memory copy.
        // Keep this after the real cache hit and on the explicit image-texture owner only.
        let system_chain = if imgtex {
            crate::mip::compressed_system_chain(t)?
        } else {
            None
        };
        let t = system_chain.as_ref().unwrap_or(t);
        // A device without BC support gets the blocks decoded on the CPU. Declared divergence:
        // `capture_texture_level_data` then reports BGRA8 for what the source had as BC.
        let decoded = if !self.bc_supported && is_block_compressed(t.format) {
            Some(decode_bc(t)?)
        } else {
            None
        };
        let t = decoded.as_ref().unwrap_or(t);
        let (format, block) = vk_format(t.format);
        if t.width == 0 || t.height == 0 || t.levels.is_empty() {
            return Err(RenderError::BadDimensions {
                width: t.width,
                height: t.height,
                reason: "a texture needs a non-zero extent and at least one level",
            });
        }
        let levels = if imgtex {
            crate::mip::runtime_level_count(t, self.imgtex_autogen_supported)
        } else {
            t.levels.len()
        };
        let levels = u16::try_from(levels)
            .map_err(|_| RenderError::Unsupported("too many provided texture levels"))?;
        let generate = usize::from(levels) > t.levels.len();

        // Validate every provided level before any device work.
        let (mut w, mut h) = (t.width, t.height);
        let mut level_bytes: Vec<(u32, u32, usize)> = Vec::with_capacity(t.levels.len());
        for bits in &t.levels {
            let row = if block {
                w.div_ceil(4) as usize * bytes_per_block(t.format)
            } else {
                w as usize * 4
            };
            let rows = if block {
                h.div_ceil(4) as usize
            } else {
                h as usize
            };
            if bits.len() < row * rows {
                return Err(RenderError::ShortSourceData {
                    format: crate::PixelFormatId::A8R8G8B8,
                    width: w,
                    height: h,
                    expected: row * rows,
                    actual: bits.len(),
                });
            }
            level_bytes.push((w, h, row * rows));
            w = crate::mip::half(w);
            h = crate::mip::half(h);
        }

        let texture = self.create_image(
            t.width,
            t.height,
            levels,
            format,
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageAspectFlags::COLOR,
            t.format,
            "texture",
        )?;

        // Stage every level and copy it in on a one-shot command buffer.
        let mut staging: Vec<Buffer> = Vec::with_capacity(t.levels.len());
        for (bits, (_, _, len)) in t.levels.iter().zip(&level_bytes) {
            let mut b = match self.create_buffer(
                (*len as u64).max(4),
                vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::CpuToGpu,
                "staging",
            ) {
                Ok(b) => b,
                Err(e) => {
                    for s in staging {
                        self.destroy_buffer(s);
                    }
                    self.destroy_texture(texture);
                    return Err(e);
                }
            };
            if let Err(e) = b.write(0, &bits[..*len]) {
                self.destroy_buffer(b);
                for s in staging {
                    self.destroy_buffer(s);
                }
                self.destroy_texture(texture);
                return Err(e);
            }
            staging.push(b);
        }
        let image = texture.image;
        let buffers: Vec<vk::Buffer> = staging.iter().map(|b| b.buffer).collect();
        let (width, height) = (t.width, t.height);
        let result = self.one_shot_nowait(
            "texture upload",
            |device, cmd| {
                // SAFETY: the buffer is recording; the image is fresh and every staging buffer is
                // live until the wait below.
                unsafe {
                    let whole = vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: u32::from(levels),
                        base_array_layer: 0,
                        layer_count: 1,
                    };
                    let to_dst = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::empty())
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(image)
                        .subresource_range(whole);
                    device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TOP_OF_PIPE,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[to_dst],
                    );
                    for (level, ((lw, lh, _), buffer)) in
                        level_bytes.iter().zip(&buffers).enumerate()
                    {
                        let region = vk::BufferImageCopy::default()
                            .buffer_offset(0)
                            .buffer_row_length(0)
                            .buffer_image_height(0)
                            .image_subresource(mipgen::layers(level as u32))
                            .image_extent(vk::Extent3D {
                                width: *lw,
                                height: *lh,
                                depth: 1,
                            });
                        device.cmd_copy_buffer_to_image(
                            cmd,
                            *buffer,
                            image,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            &[region],
                        );
                    }
                    if generate {
                        mipgen::record(device, cmd, image, width, height, levels);
                    } else {
                        let to_read = vk::ImageMemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                            .dst_access_mask(vk::AccessFlags::SHADER_READ)
                            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .image(image)
                            .subresource_range(whole);
                        device.cmd_pipeline_barrier(
                            cmd,
                            vk::PipelineStageFlags::TRANSFER,
                            vk::PipelineStageFlags::FRAGMENT_SHADER,
                            vk::DependencyFlags::empty(),
                            &[],
                            &[],
                            &[to_read],
                        );
                    }
                }
                Ok(())
            },
            staging,
        );
        if let Err(e) = result {
            self.destroy_texture(texture);
            return Err(e);
        }

        self.register_texture(key, texture)
    }

    /// Give a finished texture a descriptor slot and, under a non-zero `key`, a cache entry.
    /// Called right after the `one_shot` that filled it.
    /// A texture whose registration failed after its upload was submitted: the device may still be
    /// writing it, so it is retired against the next fence value, as a release is, rather than
    /// destroyed now.
    fn retire_unregistered(&mut self, texture: Texture) {
        self.retired_textures.push((self.next_fence_value, texture));
    }

    fn register_texture(
        &mut self,
        key: TextureKey,
        texture: Texture,
    ) -> Result<TextureSlot, RenderError> {
        // One slot per texture, from the free list, after taking back every slot whose release the
        // device has passed: a create/release/create cycle then reuses the slot rather than
        // advancing the frontier, once the device has caught up with the release.
        self.collect_retired();
        // An upload does not wait for the device, so a slot released since the device last caught
        // up is not yet reusable. When that is all that stands between this texture and a full
        // heap, wait for the device and take the slot back, rather than refuse.
        if self.descriptors.free_slots() == 0
            && self.descriptors.frontier() >= self.descriptors.capacity()
            && self.descriptors.pending_slots() > 0
        {
            if let Err(e) = self.wait_idle() {
                self.retire_unregistered(texture);
                return Err(e);
            }
            self.collect_retired();
        }
        let Some(slot) = self.descriptors.alloc() else {
            // Counted in `DescriptorStats::exhaustions`, not merely logged.
            self.retire_unregistered(texture);
            return Err(RenderError::Device(format!(
                "descriptor heap exhausted: {} of {} slots taken, {} live, {} waiting on the fence \
                 ({} refusals so far)",
                self.descriptors.frontier(),
                self.descriptors.capacity(),
                self.descriptors.live(),
                self.descriptors.pending_slots(),
                self.descriptors.stats().exhaustions,
            )));
        };
        if let Err(e) = self.write_texture_set(slot, texture.view) {
            self.descriptors.release(slot, 0);
            self.retire_unregistered(texture);
            return Err(e);
        }
        self.textures.insert(slot, texture);
        self.texture_table.insert(key, slot);
        Ok(TextureSlot(slot))
    }

    /// Point slot `slot`'s descriptor set at `view`, allocating the set on the slot's first use.
    fn write_texture_set(&mut self, slot: u32, view: vk::ImageView) -> Result<(), RenderError> {
        // `slot` is a descriptor index (`slot number * DESCRIPTORS_PER_TEXTURE`, see
        // `DescriptorAllocator`), and this table has one set per slot *number*, so divide. Indexing
        // by the descriptor index made the upper half of every budget unreachable: the o511 replay
        // with the old 4,096-descriptor budget died at its 1,025th live texture with zero exhaustions
        // reported (restructure step 2.0 triage; the D3D12 heap was indexed by descriptor natively).
        let index = (slot / DESCRIPTORS_PER_TEXTURE) as usize;
        if index >= self.texture_sets.len() {
            return Err(RenderError::Device(
                "texture slot outside the descriptor budget".into(),
            ));
        }
        let set = match self.texture_sets[index] {
            Some(s) => s,
            None => {
                let layouts = [self.texture_set_layout];
                let alloc = vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.texture_pool)
                    .set_layouts(&layouts);
                // SAFETY: the pool holds one set per slot of the budget.
                let s = vkr("vkAllocateDescriptorSets(texture)", unsafe {
                    self.device.allocate_descriptor_sets(&alloc)
                })?[0];
                self.texture_sets[index] = Some(s);
                s
            }
        };
        let info = [vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(set)
            .dst_binding(0)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(&info);
        // SAFETY: the slot came off the free list, so no pending command buffer binds this set
        // (the fence gate in `collect_retired` is exactly that guarantee); the view is live.
        unsafe { self.device.update_descriptor_sets(&[write], &[]) };
        Ok(())
    }

    /// Take another link on a texture the caller already holds.
    ///
    /// Returns the new link count, or `None` for a slot with no live texture — counted in
    /// [`TextureTableStats::unknown_add_refs`].
    pub fn retain_texture(&mut self, slot: TextureSlot) -> Option<u32> {
        self.texture_table.add_ref(slot.0)
    }

    /// Drop one link, and at zero free the texture and give its
    /// slot back.
    ///
    /// The slot is **not** reusable at once: it is parked behind a timeline value, because a
    /// command buffer may still be reading the descriptor.
    ///
    /// *Which* value depends on whether a frame is open, and the difference matters:
    ///
    /// * **Outside a frame**, every command buffer that could reference the descriptor has been
    ///   submitted, so the value the next signal carries — `next_fence_value` — is enough.
    /// * **Inside a frame**, the open buffer may already have bound the slot and has *not* been
    ///   submitted. `upload_texture` is legal mid-frame and runs a wait of its own, which would
    ///   complete a value while that buffer still sits unsubmitted; retiring against it would free
    ///   the resource out from under a buffer that is about to run. So the release is held until
    ///   `end_frame` has submitted the buffer and taken its value.
    ///
    /// After this returns [`Released::Freed`] the caller must not bind the slot again.
    ///
    /// A slot with no live texture — a double release, or a handle from a previous device — is
    /// counted in [`TextureTableStats::unknown_releases`] and otherwise ignored.
    pub fn release_texture(&mut self, slot: TextureSlot) -> Released {
        let outcome = self.texture_table.release(slot.0);
        if outcome == Released::Freed {
            let resource = self.textures.remove(&slot.0);
            if self.frame_open {
                self.released_in_frame.push((slot.0, resource));
            } else {
                let fence = self.next_fence_value;
                self.descriptors.release(slot.0, fence);
                if let Some(resource) = resource {
                    self.retired_textures.push((fence, resource));
                }
            }
        }
        outcome
    }

    /// Put the releases a frame held onto the clock, against a value signalled behind that
    /// frame's submission.
    fn flush_frame_releases(&mut self, fence: u64) {
        for (slot, resource) in std::mem::take(&mut self.released_in_frame) {
            self.descriptors.release(slot, fence);
            if let Some(resource) = resource {
                self.retired_textures.push((fence, resource));
            }
        }
    }

    /// Move every slot and texture resource whose value has completed back into circulation.
    /// Called at the top of each frame and before each slot allocation.
    fn collect_retired(&mut self) {
        self.retire_uploads();
        let completed = self.completed_value();
        self.descriptors.retire(completed);
        let (done, pending): (Vec<_>, Vec<_>) = std::mem::take(&mut self.retired_textures)
            .into_iter()
            .partition(|(fence, _)| *fence <= completed);
        self.retired_textures = pending;
        for (_, texture) in done {
            self.destroy_texture(texture);
        }
    }

    /// Descriptor occupancy and its counters.
    #[must_use]
    pub fn descriptor_stats(&self) -> DescriptorStats {
        self.descriptors.stats()
    }

    /// Slots ever taken from fresh space, and the budget they come out of.
    #[must_use]
    pub fn descriptor_usage(&self) -> DescriptorUsage {
        DescriptorUsage {
            live: self.descriptors.live(),
            frontier: self.descriptors.frontier(),
            high_water: self.descriptors.high_water(),
            free: self.descriptors.free_slots(),
            pending: self.descriptors.pending_slots(),
            // LINT-OK: bounded by the descriptor budget, a u32.
            deferred: self.released_in_frame.len() as u32,
            capacity: self.descriptors.capacity(),
        }
    }

    /// Whether a frame's command buffer is open.
    #[must_use]
    pub fn frame_open(&self) -> bool {
        self.frame_open
    }

    /// The texture table's counters: cache hits, links taken and dropped, and the tolerated
    /// failures.
    #[must_use]
    pub fn texture_table_stats(&self) -> TextureTableStats {
        self.texture_table.stats()
    }

    /// Every live keyed texture, sorted.
    #[must_use]
    pub fn texture_keys(&self) -> Vec<TextureKey> {
        self.texture_table.keys()
    }

    /// Live textures — entries with at least one link.
    #[must_use]
    pub fn live_textures(&self) -> usize {
        self.texture_table.len()
    }

    /// Actual resident subresource count, read from the resource rather than a policy counter.
    #[must_use]
    pub fn texture_mip_levels(&self, slot: TextureSlot) -> Option<u16> {
        self.textures.get(&slot.0).map(|t| t.levels)
    }

    /// Narrow the descriptor budget so a test can reach exhaustion without filling the real pool.
    #[cfg(test)]
    pub(crate) fn narrow_descriptor_budget(&mut self, slots: u32) {
        self.descriptors.narrow_for_test(slots);
    }

    /// Bind a texture and a sampler for the next draw.
    ///
    /// `sampler` names eight request variants: 0 linear/wrap, 1 linear/clamp, 2 point/wrap,
    /// 3 point/clamp, and 4..7 the corresponding mixed-address axes. Retail's global preference
    /// rewrites only eligible filters; explicit POINT is never promoted.
    ///
    /// `slot` must still be held: a slot whose last link went away in [`Self::release_texture`]
    /// is on its way back to the free list and its descriptor will be rewritten by the next upload.
    pub fn bind_texture(&self, slot: TextureSlot, sampler: u32) {
        // Masked once, so the descriptor offset below and the census above cannot
        // disagree about which sampler this draw asked for.
        let which = sampler % SAMPLER_COUNT;
        let mut counts = self.sampler_binds.get();
        counts[which as usize] += 1;
        self.sampler_binds.set(counts);
        let descriptor = self.sampler_descriptor_index(which);
        self.bound_sampler.set(Some(descriptor));
        let Some(Some(set)) = self
            .texture_sets
            .get((slot.0 / DESCRIPTORS_PER_TEXTURE) as usize)
            .copied()
        else {
            return;
        };
        // Stage 1 is filled with exactly what the D3D12 two-wide table gave it: the
        // same image and the next legacy sampler request. A draw that wants a real second texture
        // calls `bind_stage1_texture` after this, which is the same order the client's mesh
        // subset draw issues them in: the base surface first, the detail surface second.
        let Ok(pair) = self.sampler_pair_set(descriptor, descriptor + 1) else {
            return;
        };
        let cmd = self.frames[self.frame_index].cmd;
        // SAFETY: the buffer is recording (this is called between begin_frame and end_frame),
        // every set is live, and the slot was bounds-checked above.
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                1,
                &[set, set, pair],
                &[],
            );
        }
    }

    /// Put a **second** texture in stage 1,
    /// which is what the detail-texture pass is made of.
    ///
    /// The single-pass arm (`stage == 1`, the only one this build issues) changes **no** blend or
    /// depth state at all: the detail texture and a WRAP/LINEAR sampler, and nothing else. That is
    /// sampler request 0 in [`Self::bind_texture`]'s table, whatever request the base surface
    /// asked for. Call it **after** [`Self::bind_texture`].
    pub fn bind_stage1_texture(&self, slot: TextureSlot) {
        // WRAP/WRAP, LINEAR/LINEAR/LINEAR -- request 0 of the effective bank.
        let descriptor = self.sampler_descriptor_index(0);
        let stage0 = self.bound_sampler.get().unwrap_or(descriptor);
        let Some(Some(set)) = self
            .texture_sets
            .get((slot.0 / DESCRIPTORS_PER_TEXTURE) as usize)
            .copied()
        else {
            return;
        };
        let Ok(pair) = self.sampler_pair_set(stage0, descriptor) else {
            return;
        };
        let cmd = self.frames[self.frame_index].cmd;
        // SAFETY: as in `bind_texture`.
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                2,
                &[set, pair],
                &[],
            );
        }
        self.stage1_binds.set(self.stage1_binds.get() + 1);
    }

    /// How many draws have had a **detail** texture put in stage 1 since
    /// [`Self::clear_stage1_binds`].
    #[must_use]
    pub fn stage1_binds(&self) -> u64 {
        self.stage1_binds.get()
    }

    /// Zero [`Self::stage1_binds`].
    pub fn clear_stage1_binds(&self) {
        self.stage1_binds.set(0);
    }

    /// How many draws have bound each sampler since [`Self::clear_sampler_binds`],
    /// indexed exactly as [`Self::bind_texture`]'s argument.
    #[must_use]
    pub fn sampler_binds(&self) -> [u64; SAMPLER_COUNT as usize] {
        self.sampler_binds.get()
    }

    /// Zero [`Self::sampler_binds`].
    pub fn clear_sampler_binds(&self) {
        self.sampler_binds.set([0; SAMPLER_COUNT as usize]);
    }

    /// Restrict the pass to a sub-rectangle of the
    /// render target.
    ///
    /// The UI viewport element brackets its creature render with two of
    /// these. **D3D9's viewport clips; Vulkan's does not**, so this sets the scissor to the same
    /// rectangle. The viewport is installed **upside down** (negative height) so that framebuffer
    /// space is Direct3D's; see the module documentation.
    ///
    /// The rectangle is clamped to the target the way the client's viewport setter clamps it
    /// ([`crate::camera::clamp_viewport`]); an empty one draws nothing rather than tripping the
    /// driver.
    pub fn set_viewport(&self, v: crate::camera::Viewport) {
        let v = crate::camera::clamp_viewport(v, self.config.width, self.config.height);
        if v.width == 0 || v.height == 0 {
            return;
        }
        let cmd = self.frames[self.frame_index].cmd;
        #[allow(clippy::cast_precision_loss)] // pixel extents
        let viewport = vk::Viewport {
            x: v.x as f32,
            y: (v.y + v.height) as f32,
            width: v.width as f32,
            height: -(v.height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D {
                x: v.x as i32,
                y: v.y as i32,
            },
            extent: vk::Extent2D {
                width: v.width,
                height: v.height,
            },
        };
        // SAFETY: the buffer is recording and the rectangle is inside the render target, which
        // `clamp_viewport` above guarantees.
        unsafe {
            self.device.cmd_set_viewport(cmd, 0, &[viewport]);
            self.device.cmd_set_scissor(cmd, 0, &[scissor]);
        }
    }

    /// [`Self::set_viewport`] back to the whole render target, which is what `begin_frame` left.
    pub fn reset_viewport(&self) {
        self.set_viewport(crate::camera::Viewport {
            x: 0,
            y: 0,
            width: self.config.width,
            height: self.config.height,
        });
    }

    /// The client's clear restricted to one rectangle -- **depth only**.
    ///
    /// The engine's clear flags are not D3D's: bit 1 is the colour target, bit 2 the
    /// stencil and **bit 4 the depth buffer**, so clearing with flag 4
    /// clears `D3DCLEAR_ZBUFFER` and leaves the colour already in the rectangle alone.
    pub fn clear_depth(&self, v: crate::camera::Viewport) {
        let v = crate::camera::clamp_viewport(v, self.config.width, self.config.height);
        if v.width == 0 || v.height == 0 {
            return;
        }
        let cmd = self.frames[self.frame_index].cmd;
        let attachment = vk::ClearAttachment {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            color_attachment: 0,
            clear_value: vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        };
        let rect = vk::ClearRect {
            rect: vk::Rect2D {
                offset: vk::Offset2D {
                    x: v.x as i32,
                    y: v.y as i32,
                },
                extent: vk::Extent2D {
                    width: v.width,
                    height: v.height,
                },
            },
            base_array_layer: 0,
            layer_count: 1,
        };
        // SAFETY: the buffer is recording inside the pass and the rectangle is inside the target.
        unsafe {
            self.device
                .cmd_clear_attachments(cmd, &[attachment], &[rect])
        };
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        let _ = self.wait_idle();
        // SAFETY: the wait above retired every submission; nothing below is referenced by the GPU,
        // and every handle was created by this struct exactly once. Order: everything that lives
        // on the device, then the device, then the instance-level objects.
        unsafe {
            let _ = self.device.device_wait_idle();
            for (_, t) in std::mem::take(&mut self.textures) {
                self.destroy_texture(t);
            }
            for (_, cmd, keep) in std::mem::take(&mut self.pending_uploads) {
                self.device.free_command_buffers(self.oneshot_pool, &[cmd]);
                for b in keep {
                    self.destroy_buffer(b);
                }
            }
            for (_, t) in std::mem::take(&mut self.retired_textures) {
                self.destroy_texture(t);
            }
            for (_, t) in std::mem::take(&mut self.released_in_frame) {
                if let Some(t) = t {
                    self.destroy_texture(t);
                }
            }
            let target = std::mem::replace(
                &mut self.target,
                Target {
                    color: Texture::null(),
                    depth: Texture::null(),
                    framebuffer: vk::Framebuffer::null(),
                    rendered: false,
                },
            );
            self.destroy_target(target);
            if let Some(s) = self.swapchain.take() {
                self.destroy_swapchain(s);
            }
            let frames = std::mem::take(&mut self.frames);
            for f in frames {
                if let Some(a) = f.upload {
                    self.destroy_buffer(a.buffer);
                }
                for a in f.retired {
                    self.destroy_buffer(a.buffer);
                }
                self.device.destroy_semaphore(f.image_available, None);
                self.device.destroy_descriptor_pool(f.desc_pool, None);
                self.device.destroy_command_pool(f.pool, None);
            }
            if let Some(tm) = self.terrain_merge.take() {
                self.destroy_terrain_merge(tm);
            }
            if let Some(ts) = self.terrain_splat.take() {
                self.destroy_terrain_splat(ts);
            }
            for (_, p) in self.pipelines.drain() {
                self.device.destroy_pipeline(p, None);
            }
            for (_, m) in self.vertex_modules.drain() {
                self.device.destroy_shader_module(m, None);
            }
            for (_, m) in self.fragment_modules.drain() {
                self.device.destroy_shader_module(m, None);
            }
            for s in self.samplers.drain(..) {
                self.device.destroy_sampler(s, None);
            }
            self.device.destroy_descriptor_pool(self.sampler_pool, None);
            self.device.destroy_descriptor_pool(self.texture_pool, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.uniform_set_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.texture_set_layout, None);
            self.device
                .destroy_descriptor_set_layout(self.sampler_set_layout, None);
            self.device.destroy_render_pass(self.render_pass, None);
            self.device.destroy_command_pool(self.oneshot_pool, None);
            self.device.destroy_semaphore(self.timeline, None);
            drop(self.allocator.take());
            self.device.destroy_device(None);
            if let Some((loader, s)) = self.surface.take() {
                loader.destroy_surface(s, None);
            }
            if let Some((loader, m)) = self.debug_utils.take() {
                loader.destroy_debug_utils_messenger(m, None);
            }
            self.instance.destroy_instance(None);
        }
        let _ = &self.entry;
    }
}

impl Texture {
    /// A placeholder for a not-yet-created target, so `Gpu` can be built before its images.
    fn null() -> Self {
        Self {
            image: vk::Image::null(),
            view: vk::ImageView::null(),
            allocation: None,
            format: vk::Format::UNDEFINED,
            tex_format: TextureFormat::Bgra8,
            levels: 0,
            width: 0,
            height: 0,
        }
    }
}

/// The descriptor budget, in **descriptors**: 65,536 of them, which is 32,768 texture slots.
/// Slots are reclaimed (see [`crate::descriptor`]), so this bounds how many textures are *live at
/// once* rather than how many a session may ever upload.
///
/// # Why this number, and why it is not 4,096
///
/// **A real session's working set did not fit the earlier budget.** It was 4,096
/// descriptors — 2,048 pairs — which is a rebuild choice with no counterpart in the original: D3D9
/// bound textures by pointer and had no descriptor table at all, so nothing here can be
/// transcribed and the budget has to be justified by measurement instead. A full `long-solo-play`
/// replay peaked at **2,121 live pairs**; later cache fixes brought that to 1,474 and the budget
/// was still left at 65,536, because 574 pairs of headroom over a single measurement is not a
/// budget. Here the budget is a descriptor pool of that many sets, which no Vulkan limit caps.
const SRV_HEAP_SIZE: u32 = 65_536;
/// `{linear, point}` × the four `(AddressU, AddressV)` combinations — **eight**, not four.
/// `crate::ui::pixel_rules::UI_SAMPLER_COUNT` is the same number stated where the
/// index arithmetic lives; the two are asserted equal in `crate::ui`'s tests.
const SAMPLER_COUNT: u32 = crate::sampler::SAMPLER_COUNT;
/// How many upload arenas one frame may create before its constant-buffer set pool runs dry. The
/// arena doubles on every growth, so this is astronomically more than a frame can use.
const ARENA_SETS_PER_FRAME: u32 = 64;
/// `PerDraw` sits 256 bytes into the constant block so both dynamic offsets stay aligned.
const PER_DRAW_OFFSET: usize = 256;
/// The constant block: `PerFrame`, padding, `PerDraw`.
const CONSTANT_BLOCK_BYTES: usize = PER_DRAW_OFFSET + std::mem::size_of::<PerDrawConstants>();

/// Bytes per 4x4 block for the block-compressed formats: 8 for BC1, 16 for BC2/BC3. The same
/// statement as `PixelFormatDesc`'s 4 and 8 bits per pixel.
fn bytes_per_block(format: TextureFormat) -> usize {
    if format == TextureFormat::Bc1 {
        8
    } else {
        16
    }
}

fn is_block_compressed(format: TextureFormat) -> bool {
    !matches!(format, TextureFormat::Bgra8)
}

/// The device format and whether it is block-compressed, per `dereth_primitives::TextureFormat`.
fn vk_format(format: TextureFormat) -> (vk::Format, bool) {
    match format {
        TextureFormat::Bgra8 => (vk::Format::B8G8R8A8_UNORM, false),
        TextureFormat::Bc1 => (vk::Format::BC1_RGBA_UNORM_BLOCK, true),
        TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => (vk::Format::BC2_UNORM_BLOCK, true),
        TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => (vk::Format::BC3_UNORM_BLOCK, true),
        // dereth_primitives::TextureFormat is #[non_exhaustive]; a value this crate has not been taught
        // is a change to that shared type, not something to guess at. BGRA8 keeps the copy well-formed.
        _ => (vk::Format::B8G8R8A8_UNORM, false),
    }
}

/// Decode a block-compressed chain to BGRA8 on the CPU, for a device with no BC support.
fn decode_bc(t: &TextureData) -> Result<TextureData, RenderError> {
    let id = crate::texture::block_source_format(t.format)
        .ok_or_else(|| RenderError::Device(format!("unhandled texture format {:?}", t.format)))?;
    let (mut w, mut h) = (t.width, t.height);
    let mut levels = Vec::with_capacity(t.levels.len());
    for bits in &t.levels {
        levels.push(crate::dxt::decode(id, bits, w, h)?);
        w = crate::mip::half(w);
        h = crate::mip::half(h);
    }
    Ok(TextureData {
        width: t.width,
        height: t.height,
        format: TextureFormat::Bgra8,
        levels,
    })
}

/// Translate `D3DBLEND` to `VkBlendFactor` by name. The names map one-to-one, but the numeric
/// values differ.
fn to_vk_blend(b: Blend) -> vk::BlendFactor {
    match b {
        Blend::Zero => vk::BlendFactor::ZERO,
        Blend::One => vk::BlendFactor::ONE,
        Blend::SrcColor => vk::BlendFactor::SRC_COLOR,
        Blend::InvSrcColor => vk::BlendFactor::ONE_MINUS_SRC_COLOR,
        Blend::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
        Blend::InvSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        Blend::DestAlpha => vk::BlendFactor::DST_ALPHA,
        Blend::InvDestAlpha => vk::BlendFactor::ONE_MINUS_DST_ALPHA,
        Blend::DestColor => vk::BlendFactor::DST_COLOR,
        Blend::InvDestColor => vk::BlendFactor::ONE_MINUS_DST_COLOR,
    }
}

/// The same mapping for the **alpha** blend slots.
///
/// D3D9 with `SEPARATEALPHABLENDENABLE` off applies the one blend factor to both channels, and for a
/// colour factor the alpha channel uses that factor's alpha. Vulkan would accept the colour factor
/// in the alpha slot, but the D3D12 build had to name the analogue and the intent is clearer kept:
/// each colour factor is replaced by its alpha analogue. Unobservable in the back buffer --
/// `COLORWRITEENABLE = 7` masks the alpha channel.
fn to_vk_blend_alpha(b: Blend) -> vk::BlendFactor {
    to_vk_blend(b.alpha_factor())
}

fn to_vk_compare(z: ZFunc) -> vk::CompareOp {
    match z {
        ZFunc::Never => vk::CompareOp::NEVER,
        ZFunc::Less => vk::CompareOp::LESS,
        ZFunc::Equal => vk::CompareOp::EQUAL,
        ZFunc::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
        ZFunc::Greater => vk::CompareOp::GREATER,
        ZFunc::NotEqual => vk::CompareOp::NOT_EQUAL,
        ZFunc::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
        ZFunc::Always => vk::CompareOp::ALWAYS,
    }
}

#[cfg(test)]
mod tests;
