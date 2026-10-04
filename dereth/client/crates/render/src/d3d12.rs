//! **The only module in this crate that uses `unsafe`.** Direct3D 12 and Win32 interop.
//!
//! The device, frame ring, heaps and upload ring, PSO construction, and the headless capture
//! backend. The pure-logic half of each lives outside this module and is tested without a GPU;
//! what is here is the part that cannot be.
//!
//! This backend preserves device creation, presentation, reset, gamma and the fixed-function
//! mapping onto D3D12.
//!
//! # The FPU
//!
//! The retail device is created with `D3DCREATE_FPU_PRESERVE`, so
//! D3D9 never reset the FPU control word and the client ran its whole session at 53-bit precision.
//! **Nothing in this module changes the FPU control word**, and nothing may be added that does. D3D12
//! has no equivalent flag because it has no equivalent behaviour: it does not touch the control word
//! at all. On x86-64 the Rust ABI computes in SSE2, which the FPU control word does not govern, so
//! the invariant is preserved by leaving it alone.

#![allow(clippy::cast_possible_truncation)] // Graphics APIs are full of u32 sizes; each site is bounded.

use std::collections::HashMap;
use std::ffi::CStr;

use windows::core::{Interface, PCSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3};
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_SHADER_MACRO,
};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

use dereth_primitives::{TextureData, TextureFormat};

use crate::descriptor::{
    DescriptorAllocator, DescriptorStats, Released, TextureKey, TextureTable, TextureTableStats,
    DESCRIPTORS_PER_TEXTURE,
};
use crate::pso::{Blend, Cull, PipelineKey, PixelShader, ZFunc};
use crate::vertex::{VertexElement, VertexFormat};
use crate::RenderError;

mod mipgen;
mod samplers;
mod terrain;

// The plain data both backends speak lives in `crate::device`; it is re-exported here so that
// `dereth_render::d3d12::DeviceConfig` and its neighbours name the same types as the Vulkan
// backend's. `AdapterKind::Software` became the shared `AdapterKind::Software`,
// which is what it always meant, and `DeviceConfig::force_software` became `force_software`.
pub use crate::device::{
    hlsl_matrix, AdapterKind, CapturedImage, DescriptorUsage, DeviceConfig, PerDrawConstants,
    PerFrameConstants, RawWindowHandle, TextureSlot, WindowHandles, FRAME_COUNT,
};

use crate::device::{as_bytes, unpack_argb};

/// The back-buffer format. The client picks `X8R8G8B8` on a 32-bit desktop and disables sRGB
/// writes, so matching it requires a **non**-sRGB target. `B8G8R8A8_UNORM` is `A8R8G8B8`'s memory
/// order; the alpha channel is never written because `COLORWRITEENABLE = 7`.
pub const BACK_BUFFER_FORMAT: DXGI_FORMAT = DXGI_FORMAT_B8G8R8A8_UNORM;

/// The depth format. The client enables its stencil buffer and therefore uses the preference order
/// **D24S8 → D32 → D24X8 → D24X4S4**.
pub const DEPTH_FORMAT: DXGI_FORMAT = DXGI_FORMAT_D24_UNORM_S8_UINT;

fn hr<T>(what: &'static str, r: windows::core::Result<T>) -> Result<T, RenderError> {
    r.map_err(|e| RenderError::Device(format!("{what}: {e}")))
}

/// The `HWND` `CreateSwapChainForHwnd` is typed against, out of the backend-neutral handle the
/// client reads off winit. No `unsafe` is involved: `HWND` is a transparent wrapper over a
/// pointer, and `RawWindowHandle::Win32` already carries it as an integer.
///
/// # Errors
/// [`RenderError::Unsupported`] for any other handle variant, which on Windows means a window
/// this device cannot present to.
fn to_hwnd(handle: RawWindowHandle) -> Result<HWND, RenderError> {
    match handle {
        RawWindowHandle::Win32(h) => Ok(HWND(h.hwnd.get() as *mut core::ffi::c_void)),
        _ => Err(RenderError::Unsupported(
            "the D3D12 device needs a Win32 window handle",
        )),
    }
}

/// The adapter name out of a `DXGI_ADAPTER_DESC1`, whose `Description` is a NUL-padded UTF-16
/// array. The Vulkan device reports `VkPhysicalDeviceProperties::deviceName` in the same place.
fn adapter_description(desc: &DXGI_ADAPTER_DESC1) -> String {
    let end = desc
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.Description.len());
    String::from_utf16_lossy(&desc.Description[..end])
}

/// Where a frame is rendered.
#[derive(Debug)]
enum Target {
    /// A DXGI swap chain attached to a window.
    SwapChain {
        chain: IDXGISwapChain3,
        buffers: Vec<ID3D12Resource>,
    },
    /// A single offscreen texture: the headless path.
    Offscreen { texture: ID3D12Resource },
}

/// One frame's slot in the ring: its allocator, its fence value and its share of the upload arena.
#[derive(Debug)]
struct Frame {
    allocator: ID3D12CommandAllocator,
    fence_value: u64,
    /// The dynamic vertex ring. The client grows a dynamic
    /// buffer to the previous frame's high-water mark and never shrinks it within a session; the
    /// same rule applies here.
    upload: Option<ID3D12Resource>,
    upload_capacity: u64,
    upload_used: u64,
    high_water: u64,
    /// Upload arenas this frame outgrew. They stay alive until the fence proves the GPU has
    /// finished with the command list that still references their addresses. See `upload_bytes`.
    retired: Vec<ID3D12Resource>,
}

/// What one texture upload's command list used, kept until the fence passes `fence`.
struct PendingUpload {
    fence: u64,
    _allocator: ID3D12CommandAllocator,
    _list: ID3D12GraphicsCommandList,
    _staging: Vec<ID3D12Resource>,
    _mip_views: Option<mipgen::MipViews>,
}

impl std::fmt::Debug for PendingUpload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingUpload")
            .field("fence", &self.fence)
            .finish_non_exhaustive()
    }
}

/// The Direct3D 12 device, swap chain, frame ring, heaps and PSO cache.
#[derive(Debug)]
pub struct Gpu {
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    factory: IDXGIFactory4,
    target: Target,
    depth: ID3D12Resource,
    rtv_heap: ID3D12DescriptorHeap,
    dsv_heap: ID3D12DescriptorHeap,
    /// The shader-visible CBV/SRV/UAV heap, used as a per-frame ring.
    srv_heap: ID3D12DescriptorHeap,
    sampler_heap: ID3D12DescriptorHeap,
    sampler_descriptions: Vec<D3D12_SAMPLER_DESC>,
    texture_filtering: u32,
    sharp_lod_bias: bool,
    bound_sampler: std::cell::Cell<Option<u32>>,
    rtv_size: u32,
    srv_size: u32,
    sampler_size: u32,
    list: ID3D12GraphicsCommandList,
    fence: ID3D12Fence,
    fence_event: HANDLE,
    next_fence_value: u64,
    frames: Vec<Frame>,
    frame_index: usize,
    root_signature: ID3D12RootSignature,
    vertex_shaders: HashMap<VertexFormat, Vec<u8>>,
    pixel_shaders: HashMap<PixelShader, Vec<u8>>,
    // ORDER-OK: keyed by PipelineKey and only ever looked up, never iterated for anything the
    // rendered result depends on.
    psos: HashMap<PipelineKey, ID3D12PipelineState>,
    /// Actual BGRA8 format capability, queried once per device, and lazy runtime mip PSO.
    imgtex_autogen_supported: bool,
    mip_generator: Option<mipgen::MipGenerator>,
    /// The landscape compositor and splat pipeline, built on first use. See `d3d12/terrain.rs`.
    terrain_merge: Option<terrain::TerrainMerge>,
    terrain_splat: Option<terrain::TerrainSplatState>,
    /// The four texture and sampler tables (root parameters 2 to 5) the last legacy bind set, so a
    /// splat draw, which changes the root signature and with it every root binding, can put them
    /// back. Cleared when a frame begins.
    legacy_tables: std::cell::Cell<[Option<u64>; 4]>,
    /// Live texture resources, keyed by the descriptor slot that views them. An entry lives exactly
    /// as long as [`TextureTable`] says something still links to it.
    // ORDER-OK: keyed by descriptor slot and only ever looked up, never iterated for anything the
    // rendered result depends on.
    textures: HashMap<u32, ID3D12Resource>,
    /// Textures whose last link went away. They stay alive until the fence proves the GPU has
    /// finished with the command lists that referenced them — the same rule `upload_bytes` follows
    /// for outgrown upload arenas, and for the same reason.
    retired_textures: Vec<(u64, ID3D12Resource)>,
    /// Texture uploads submitted without waiting, with the fence value signalled after each: the
    /// command allocator and list, the staging buffers and the sublevel views it used stay alive
    /// until the fence has passed it. See [`Gpu::upload_texture_internal`].
    pending_uploads: Vec<PendingUpload>,
    /// Releases that happened while a frame's command list was open, held until `end_frame` decides
    /// their fence value. See [`Gpu::release_texture`]: the open list may already reference the
    /// descriptor and has *not* been submitted, so a `wait_idle` that an intervening
    /// `upload_texture` performs must not be allowed to retire them.
    released_in_frame: Vec<(u32, Option<ID3D12Resource>)>,
    /// True between `begin_frame` and `end_frame`.
    frame_open: bool,
    /// How many times each of [`Gpu::bind_texture`]'s four samplers has been bound
    /// since the counters were last cleared, so a caller's *choice of sampler* is observable from
    /// a test rather than inferable only by reading the call site. A `Cell` because
    /// `bind_texture` takes `&self` (it records into an already-open command list) and a counter
    /// must not be the thing that makes it `&mut`.
    sampler_binds: std::cell::Cell<[u64; SAMPLER_COUNT as usize]>,
    /// How many draws have put a *detail* texture in stage 1 -- the same argument
    /// as [`Self::sampler_binds`], for the second texture stage.
    stage1_binds: std::cell::Cell<u64>,
    /// The shared and custom texture tables and their link count.
    texture_table: TextureTable,
    /// Descriptor pairs in the shader-visible heap: a free list, not a monotonic counter.
    descriptors: DescriptorAllocator,
    config: DeviceConfig,
    pub adapter_kind: AdapterKind,
    /// The adapter's name, for the startup line.
    pub adapter_name: String,
    /// The frame stamp, bumped once per presented frame.
    pub frame_stamp: u64,
    /// The `SyncInterval` handed to the next swap-chain `Present` call.
    present_sync_interval: u32,
    /// The interval most recently handed to a real swap chain. Offscreen frames leave this `None`.
    last_present_sync_interval: Option<u32>,
    /// The device's gamma brightness value.
    gamma: f32,
    /// The 1x1 opaque-white texture the pre-transformed portal stamp samples, created on first use.
    /// See [`Gpu::draw_portal_poly`] for why a draw that writes no colour still needs one.
    stamp_texture: Option<TextureSlot>,
    /// How many portal depth stamps this device has issued.
    ///
    /// **Not** the client's own portal-stamp counter, which counts only the indoor arm, because
    /// the cell draw reads it to decide
    /// whether to clear the Z buffer. This is every stamp, and it exists because a draw that writes
    /// no colour cannot be seen in a capture: without it "the stamp is issued" and "the stamp is
    /// silently skipped" are the same frame.
    pub portal_stamps: u64,
    /// How many `draw_dynamic` batches this device has issued. The original device keeps no such
    /// counter; this one exists because an ordering change that moves a draw without changing what
    /// it draws is otherwise indistinguishable from one that draws it **twice**, and those are the
    /// two ways to get a deferred-queue flush wrong.
    pub draw_calls: u64,
}

impl Gpu {
    /// Create the device. Reproduces the *shape* of the client's creation ladder:
    /// try the best rung first, fall back, and report which
    /// rung was taken. D3D12 has no software-vertex-processing rung, so the ladder is
    /// hardware → WARP.
    ///
    /// `window` is `None` for the headless path, which renders to an offscreen texture; that is
    /// what makes the capture tests runnable without a display. Only
    /// the `Win32` variant is a window here -- this device exists on Windows only.
    ///
    /// # Errors
    /// [`RenderError::Device`] when no adapter can create a feature-level 11.0 device, and
    /// [`RenderError::Unsupported`] for a window handle that is not a `Win32` one.
    pub fn new(window: Option<WindowHandles>, cfg: &DeviceConfig) -> Result<Self, RenderError> {
        let hwnd = match window {
            Some(w) => Some(to_hwnd(w.window)?),
            None => None,
        };
        if cfg.width == 0 || cfg.height == 0 {
            return Err(RenderError::BadDimensions {
                width: cfg.width,
                height: cfg.height,
                reason: "the back buffer must have a non-zero extent",
            });
        }
        let mut flags = DXGI_CREATE_FACTORY_FLAGS(0);
        if cfg.debug {
            let mut debug: Option<ID3D12Debug> = None;
            // SAFETY: D3D12GetDebugInterface writes an optional out-parameter and is safe to call
            // before any device exists; a failure leaves `debug` as None, which we ignore.
            unsafe {
                if D3D12GetDebugInterface(&mut debug).is_ok() {
                    if let Some(d) = &debug {
                        d.EnableDebugLayer();
                    }
                    flags = DXGI_CREATE_FACTORY_DEBUG;
                }
            }
        }

        // SAFETY: CreateDXGIFactory2 takes a flags word and returns a new COM interface; there are
        // no borrowed pointers involved.
        let factory: IDXGIFactory4 =
            hr("CreateDXGIFactory2", unsafe { CreateDXGIFactory2(flags) })?;

        let (device, adapter_kind, adapter_name) =
            Self::create_device(&factory, cfg.prefers_software() || test_requires_software())?;

        let queue_desc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            ..Default::default()
        };
        // SAFETY: `queue_desc` outlives the call, and the device owns the returned queue.
        let queue: ID3D12CommandQueue = hr("CreateCommandQueue", unsafe {
            device.CreateCommandQueue(&queue_desc)
        })?;

        // Descriptor heaps. One RTV heap for the frame buffers, one DSV, one shader-visible
        // CBV/SRV/UAV heap used as a per-frame ring, and one immutable sampler heap keyed by
        // the exact effective filtering/bias state. Preference changes never rewrite descriptors.
        let rtv_heap = Self::heap(
            &device,
            D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            FRAME_COUNT as u32,
            false,
        )?;
        let dsv_heap = Self::heap(&device, D3D12_DESCRIPTOR_HEAP_TYPE_DSV, 1, false)?;
        let srv_descriptors = cfg.srv_descriptors.unwrap_or(SRV_HEAP_SIZE);
        let srv_heap = Self::heap(
            &device,
            D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            srv_descriptors,
            true,
        )?;
        let sampler_heap = Self::heap(
            &device,
            D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER,
            samplers::DESCRIPTOR_COUNT,
            true,
        )?;

        // SAFETY: GetDescriptorHandleIncrementSize is a pure query on the device.
        let (rtv_size, srv_size, sampler_size) = unsafe {
            (
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV),
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV),
                device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER),
            )
        };

        let target = Self::create_target(&factory, &device, &queue, hwnd, cfg)?;
        let depth = Self::create_depth(&device, cfg)?;

        let mut frames = Vec::with_capacity(FRAME_COUNT);
        for _ in 0..FRAME_COUNT {
            // SAFETY: the allocator is a fresh COM object owned by the returned Frame.
            let allocator = hr("CreateCommandAllocator", unsafe {
                device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
            })?;
            frames.push(Frame {
                allocator,
                fence_value: 0,
                upload: None,
                upload_capacity: 0,
                upload_used: 0,
                high_water: 0,
                retired: Vec::new(),
            });
        }

        // SAFETY: frames[0].allocator outlives the list, which is closed immediately below.
        let list: ID3D12GraphicsCommandList = hr("CreateCommandList", unsafe {
            device.CreateCommandList(
                0,
                D3D12_COMMAND_LIST_TYPE_DIRECT,
                &frames[0].allocator,
                None,
            )
        })?;
        // SAFETY: a freshly created list is open; close it so the first begin_frame can reset it.
        hr("Close", unsafe { list.Close() })?;

        // SAFETY: CreateFence returns a new object; CreateEventA returns an owned handle that
        // `Drop` closes.
        let fence: ID3D12Fence = hr("CreateFence", unsafe {
            device.CreateFence(0, D3D12_FENCE_FLAG_NONE)
        })?;
        // SAFETY: CreateEventW with no attributes and no name returns a fresh owned handle,
        // which `Drop` closes exactly once.
        let event = unsafe { CreateEventW(None, false, false, None) };
        let fence_event = hr("CreateEventW", event)?;

        let root_signature = Self::create_root_signature(&device)?;
        let (vertex_shaders, pixel_shaders) = compile_shaders()?;
        let imgtex_autogen_supported = mipgen::supported(&device);

        let mut gpu = Self {
            device,
            queue,
            factory,
            target,
            depth,
            rtv_heap,
            dsv_heap,
            srv_heap,
            sampler_heap,
            sampler_descriptions: Vec::with_capacity(samplers::DESCRIPTOR_COUNT as usize),
            texture_filtering: crate::sampler::STARTUP_FILTERING,
            sharp_lod_bias: false,
            bound_sampler: std::cell::Cell::new(None),
            rtv_size,
            srv_size,
            sampler_size,
            list,
            fence,
            fence_event,
            next_fence_value: 1,
            frames,
            frame_index: 0,
            root_signature,
            vertex_shaders,
            pixel_shaders,
            psos: HashMap::new(),
            imgtex_autogen_supported,
            mip_generator: None,
            terrain_merge: None,
            terrain_splat: None,
            legacy_tables: std::cell::Cell::new([None; 4]),
            textures: HashMap::new(),
            retired_textures: Vec::new(),
            pending_uploads: Vec::new(),
            released_in_frame: Vec::new(),
            frame_open: false,
            sampler_binds: std::cell::Cell::new([0; SAMPLER_COUNT as usize]),
            stage1_binds: std::cell::Cell::new(0),
            texture_table: TextureTable::new(),
            descriptors: DescriptorAllocator::new(srv_descriptors, DESCRIPTORS_PER_TEXTURE),
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
        gpu.create_views()?;
        gpu.create_samplers();
        Ok(gpu)
    }

    fn create_device(
        factory: &IDXGIFactory4,
        force_software: bool,
    ) -> Result<(ID3D12Device, AdapterKind, String), RenderError> {
        // Rung 0: a hardware adapter, unless the caller demanded WARP.
        if !force_software {
            let mut i = 0u32;
            loop {
                // SAFETY: EnumAdapters1 returns a new interface or an error at the end of the list.
                let adapter: IDXGIAdapter1 = match unsafe { factory.EnumAdapters1(i) } {
                    Ok(a) => a,
                    Err(_) => break,
                };
                i += 1;
                // SAFETY: GetDesc1 fills and returns a plain descriptor struct.
                let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                    continue;
                };
                if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                    continue;
                }
                let mut device: Option<ID3D12Device> = None;
                // SAFETY: the adapter outlives the call; `device` is an out-parameter that is only
                // read on success.
                if unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }
                    .is_ok()
                {
                    if let Some(d) = device {
                        return Ok((d, AdapterKind::Hardware, adapter_description(&desc)));
                    }
                }
            }
        }
        // Rung 1: WARP. Deterministic, and always present on a supported Windows.
        // SAFETY: EnumWarpAdapter returns a new interface; the device call is as above.
        let warp: IDXGIAdapter1 = hr("EnumWarpAdapter", unsafe { factory.EnumWarpAdapter() })?;
        let mut device: Option<ID3D12Device> = None;
        // SAFETY: the adapter outlives the call and `device` is an out-parameter read only on
        // success.
        let made = unsafe { D3D12CreateDevice(&warp, D3D_FEATURE_LEVEL_11_0, &mut device) };
        hr("D3D12CreateDevice(WARP)", made)?;
        // SAFETY: GetDesc1 fills and returns a plain descriptor struct.
        let warp_name = unsafe { warp.GetDesc1() }
            .map_or_else(|_| "WARP".to_string(), |d| adapter_description(&d));
        device
            .map(|d| (d, AdapterKind::Software, warp_name))
            .ok_or_else(|| RenderError::Device("no D3D12 device could be created".into()))
    }

    fn heap(
        device: &ID3D12Device,
        kind: D3D12_DESCRIPTOR_HEAP_TYPE,
        count: u32,
        shader_visible: bool,
    ) -> Result<ID3D12DescriptorHeap, RenderError> {
        let desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: kind,
            NumDescriptors: count,
            Flags: if shader_visible {
                D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
            } else {
                D3D12_DESCRIPTOR_HEAP_FLAG_NONE
            },
            NodeMask: 0,
        };
        // SAFETY: `desc` is a live local; the returned heap is owned by the caller.
        hr("CreateDescriptorHeap", unsafe {
            device.CreateDescriptorHeap(&desc)
        })
    }

    fn create_target(
        factory: &IDXGIFactory4,
        device: &ID3D12Device,
        queue: &ID3D12CommandQueue,
        hwnd: Option<HWND>,
        cfg: &DeviceConfig,
    ) -> Result<Target, RenderError> {
        if let Some(hwnd) = hwnd {
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: cfg.width,
                Height: cfg.height,
                Format: BACK_BUFFER_FORMAT,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: FRAME_COUNT as u32,
                // D3DSWAPEFFECT_DISCARD's modern equivalent.
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                ..Default::default()
            };
            // SAFETY: hwnd is a valid window for the lifetime of the swap chain, which the caller
            // guarantees by keeping the Window alive; all descriptors are live locals.
            let chain1 = hr("CreateSwapChainForHwnd", unsafe {
                factory.CreateSwapChainForHwnd(queue, hwnd, &desc, None, None)
            })?;
            let chain: IDXGISwapChain3 = hr("cast IDXGISwapChain3", chain1.cast())?;
            let mut buffers = Vec::with_capacity(FRAME_COUNT);
            for i in 0..FRAME_COUNT as u32 {
                // SAFETY: i is within BufferCount.
                buffers.push(hr("GetBuffer", unsafe { chain.GetBuffer(i) })?);
            }
            return Ok(Target::SwapChain { chain, buffers });
        }

        // Headless: one offscreen render target.
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: u64::from(cfg.width),
            Height: cfg.height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: BACK_BUFFER_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
            ..Default::default()
        };
        let clear = D3D12_CLEAR_VALUE {
            Format: BACK_BUFFER_FORMAT,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                Color: [0.0, 0.0, 0.0, 0.0],
            },
        };
        let mut texture: Option<ID3D12Resource> = None;
        // SAFETY: every descriptor is a live local; `texture` is only read on success.
        hr("CreateCommittedResource(rt)", unsafe {
            device.CreateCommittedResource(
                &default_heap(),
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                Some(&clear),
                &mut texture,
            )
        })?;
        Ok(Target::Offscreen {
            texture: texture.ok_or_else(|| RenderError::Device("no render target".into()))?,
        })
    }

    fn create_depth(
        device: &ID3D12Device,
        cfg: &DeviceConfig,
    ) -> Result<ID3D12Resource, RenderError> {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: u64::from(cfg.width),
            Height: cfg.height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DEPTH_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Flags: D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
            ..Default::default()
        };
        let clear = D3D12_CLEAR_VALUE {
            Format: DEPTH_FORMAT,
            Anonymous: D3D12_CLEAR_VALUE_0 {
                DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                    Depth: 1.0,
                    Stencil: 0,
                },
            },
        };
        let mut depth: Option<ID3D12Resource> = None;
        // SAFETY: as above.
        hr("CreateCommittedResource(depth)", unsafe {
            device.CreateCommittedResource(
                &default_heap(),
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_DEPTH_WRITE,
                Some(&clear),
                &mut depth,
            )
        })?;
        depth.ok_or_else(|| RenderError::Device("no depth buffer".into()))
    }

    fn create_views(&mut self) -> Result<(), RenderError> {
        // SAFETY: GetCPUDescriptorHandleForHeapStart is a pure query; every offset written below is
        // within the heap's NumDescriptors.
        let mut handle = unsafe { self.rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        match &self.target {
            Target::SwapChain { buffers, .. } => {
                for b in buffers {
                    // SAFETY: `b` is a render-target resource and `handle` is inside the RTV heap.
                    unsafe { self.device.CreateRenderTargetView(b, None, handle) };
                    handle.ptr += self.rtv_size as usize;
                }
            }
            Target::Offscreen { texture } => {
                // SAFETY: as above.
                unsafe { self.device.CreateRenderTargetView(texture, None, handle) };
            }
        }
        // SAFETY: the DSV heap has one descriptor and `self.depth` is a depth-stencil resource.
        unsafe {
            let dsv = self.dsv_heap.GetCPUDescriptorHandleForHeapStart();
            self.device.CreateDepthStencilView(&self.depth, None, dsv);
        }
        Ok(())
    }

    /// The root signature carries the fixed-function state across as two constant buffers, then
    /// **one table per texture stage** and one per sampler stage.
    ///
    /// **The tables are one wide, not two.** A two-wide `t0..t1` and `s0..s1`, filled from a
    /// single texture slot's descriptor pair, would make `t1` only
    /// ever the same image as `t0`, which is fatal for the detail-texture pass, the one caller
    /// of `ps_blendcurrentalpha`. Stage 1 therefore has its own
    /// single-descriptor table, so [`Gpu::bind_stage1_texture`] can put a *different* texture and
    /// a *different* sampler there. [`Gpu::bind_texture`] still fills both stages
    /// (`t1` = the pair's second descriptor, `s1` = the next legacy request), so every draw
    /// that does not ask for a detail texture is bit-identical.
    fn create_root_signature(device: &ID3D12Device) -> Result<ID3D12RootSignature, RenderError> {
        let srv_range0 = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 1,
            BaseShaderRegister: 0,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        };
        let srv_range1 = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            NumDescriptors: 1,
            BaseShaderRegister: 1,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        };
        let sampler_range0 = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
            NumDescriptors: 1,
            BaseShaderRegister: 0,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        };
        let sampler_range1 = D3D12_DESCRIPTOR_RANGE {
            RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
            NumDescriptors: 1,
            BaseShaderRegister: 1,
            RegisterSpace: 0,
            OffsetInDescriptorsFromTableStart: D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND,
        };
        let params = [
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR {
                        ShaderRegister: 0,
                        RegisterSpace: 0,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_CBV,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    Descriptor: D3D12_ROOT_DESCRIPTOR {
                        ShaderRegister: 1,
                        RegisterSpace: 0,
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: std::ptr::addr_of!(srv_range0),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: std::ptr::addr_of!(sampler_range0),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: std::ptr::addr_of!(srv_range1),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
            },
            D3D12_ROOT_PARAMETER {
                ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
                Anonymous: D3D12_ROOT_PARAMETER_0 {
                    DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE {
                        NumDescriptorRanges: 1,
                        pDescriptorRanges: std::ptr::addr_of!(sampler_range1),
                    },
                },
                ShaderVisibility: D3D12_SHADER_VISIBILITY_PIXEL,
            },
        ];
        let desc = D3D12_ROOT_SIGNATURE_DESC {
            NumParameters: params.len() as u32,
            pParameters: params.as_ptr(),
            NumStaticSamplers: 0,
            pStaticSamplers: std::ptr::null(),
            Flags: D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
        };
        let mut blob: Option<ID3DBlob> = None;
        let mut error: Option<ID3DBlob> = None;
        // SAFETY: `desc` and the arrays it points at are live locals that outlive the call; the two
        // out-parameters are only read afterwards.
        let r = unsafe {
            D3D12SerializeRootSignature(
                &desc,
                D3D_ROOT_SIGNATURE_VERSION_1,
                &mut blob,
                Some(&mut error),
            )
        };
        if let Err(e) = r {
            let msg = error.map_or_else(String::new, |b| blob_text(&b));
            return Err(RenderError::Device(format!(
                "D3D12SerializeRootSignature: {e} {msg}"
            )));
        }
        let blob = blob.ok_or_else(|| RenderError::Device("no root signature blob".into()))?;
        // SAFETY: the blob's buffer is valid until the blob is dropped, which is after this call.
        let bytes = unsafe {
            std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
        };
        // SAFETY: `bytes` is a live slice for the duration of the call.
        hr("CreateRootSignature", unsafe {
            device.CreateRootSignature(0, bytes)
        })
    }

    /// Build (or fetch) the pipeline state for a key. This is the PSO catalogue made real: the 15
    /// keys of `crate::pso::CATALOGUE` each construct exactly one of these.
    ///
    /// # Errors
    /// [`RenderError::Device`] when the D3D12 runtime rejects the state object, which is what the
    /// catalogue test is looking for.
    pub fn pipeline_state(
        &mut self,
        key: &PipelineKey,
    ) -> Result<ID3D12PipelineState, RenderError> {
        if let Some(p) = self.psos.get(key) {
            return Ok(p.clone());
        }
        let pso = self.build_pso(key)?;
        self.psos.insert(*key, pso.clone());
        Ok(pso)
    }

    fn build_pso(&self, key: &PipelineKey) -> Result<ID3D12PipelineState, RenderError> {
        let shader = key.stage_ops.pixel_shader();
        let ps = self
            .pixel_shaders
            .get(&shader)
            .ok_or_else(|| RenderError::Device("no pixel shader".into()))?;
        self.build_pso_with(key, ps, &self.root_signature)
    }

    /// [`Self::build_pso`] with the pixel shader and the root signature given rather than taken
    /// from the key: every fixed-function state is still `key`'s.
    fn build_pso_with(
        &self,
        key: &PipelineKey,
        ps: &[u8],
        root: &ID3D12RootSignature,
    ) -> Result<ID3D12PipelineState, RenderError> {
        let vs = self
            .vertex_shaders
            .get(&key.vertex_format)
            .ok_or_else(|| RenderError::Device("no vertex shader for format".into()))?;

        // The input layout, from the five FVF codes. The semantic names are `'static` C string
        // literals, so the pointers the descriptor holds are valid for the whole program and there
        // is no fallible conversion to handle.
        let elements: Vec<D3D12_INPUT_ELEMENT_DESC> = key
            .vertex_format
            .elements()
            .iter()
            .map(|(e, off)| D3D12_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(semantic_name(*e).as_ptr().cast()),
                SemanticIndex: match e {
                    VertexElement::TexCoord(n) => *n,
                    _ => 0,
                },
                Format: match e.attribute_format(true) {
                    crate::vertex::AttributeFormat::Float4 => DXGI_FORMAT_R32G32B32A32_FLOAT,
                    crate::vertex::AttributeFormat::Float3 => DXGI_FORMAT_R32G32B32_FLOAT,
                    crate::vertex::AttributeFormat::Float2 => DXGI_FORMAT_R32G32_FLOAT,
                    crate::vertex::AttributeFormat::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
                    crate::vertex::AttributeFormat::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
                },
                InputSlot: 0,
                AlignedByteOffset: *off,
                InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            })
            .collect();

        let blend_target = D3D12_RENDER_TARGET_BLEND_DESC {
            BlendEnable: key.alpha_blend.into(),
            LogicOpEnable: false.into(),
            SrcBlend: to_d3d12_blend(key.src_blend),
            DestBlend: to_d3d12_blend(key.dst_blend),
            BlendOp: D3D12_BLEND_OP_ADD,
            // D3D9 without SEPARATEALPHABLENDENABLE uses the same factor for alpha, taking each
            // colour factor's alpha channel; D3D12 rejects a colour-manipulating factor in the
            // alpha slots outright, so the analogue has to be named. See `to_d3d12_blend_alpha`.
            SrcBlendAlpha: to_d3d12_blend_alpha(key.src_blend),
            DestBlendAlpha: to_d3d12_blend_alpha(key.dst_blend),
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP,
            // Preserve COLORWRITEENABLE = 7: red | green | blue, with alpha masked off.
            RenderTargetWriteMask: crate::COLOR_WRITE_ENABLE_RGB,
        };

        let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::ManuallyDrop::new(Some(root.clone())),
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vs.as_ptr().cast(),
                BytecodeLength: vs.len(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: ps.as_ptr().cast(),
                BytecodeLength: ps.len(),
            },
            BlendState: D3D12_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: [
                    blend_target,
                    blend_target,
                    blend_target,
                    blend_target,
                    blend_target,
                    blend_target,
                    blend_target,
                    blend_target,
                ],
            },
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: match key.cull.face() {
                    crate::pso::CullFace::None => D3D12_CULL_MODE_NONE,
                    // D3DCULL_CW culls clockwise-wound (front) triangles with the client's winding.
                    crate::pso::CullFace::Front => D3D12_CULL_MODE_FRONT,
                    crate::pso::CullFace::Back => D3D12_CULL_MODE_BACK,
                },
                FrontCounterClockwise: matches!(
                    crate::pso::FRONT_FACE,
                    crate::pso::FrontFace::CounterClockwise
                )
                .into(),
                // Do not add a depth bias. The client relies on its 0.01 terrain z-fight adjustment and
                // `LESSEQUAL` instead.
                DepthBias: 0,
                DepthBiasClamp: 0.0,
                SlopeScaledDepthBias: 0.0,
                DepthClipEnable: true.into(),
                MultisampleEnable: false.into(),
                AntialiasedLineEnable: false.into(),
                ForcedSampleCount: 0,
                ConservativeRaster: D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
            },
            DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(),
                DepthWriteMask: if key.z_write {
                    D3D12_DEPTH_WRITE_MASK_ALL
                } else {
                    D3D12_DEPTH_WRITE_MASK_ZERO
                },
                DepthFunc: to_d3d12_compare(key.z_func),
                // Stencil is disabled everywhere in the client, but D3D12 still validates the
                // op fields and an all-zero D3D12_STENCIL_OP is not a legal value, so the disabled
                // state is spelled out rather than defaulted.
                StencilEnable: false.into(),
                StencilReadMask: D3D12_DEFAULT_STENCIL_READ_MASK as u8,
                StencilWriteMask: D3D12_DEFAULT_STENCIL_WRITE_MASK as u8,
                FrontFace: DISABLED_STENCIL_FACE,
                BackFace: DISABLED_STENCIL_FACE,
            },
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: elements.as_ptr(),
                NumElements: elements.len() as u32,
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            RTVFormats: {
                let mut f = [DXGI_FORMAT_UNKNOWN; 8];
                f[0] = BACK_BUFFER_FORMAT;
                f
            },
            DSVFormat: DEPTH_FORMAT,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        // SAFETY: `desc` and everything it points at (`elements`, `names`, the shader blobs and the
        // root signature) are alive for the duration of this call. The runtime copies what it needs.
        let made = unsafe { self.device.CreateGraphicsPipelineState(&desc) };
        match made {
            Ok(p) => Ok(p),
            Err(e) => Err(RenderError::Device(format!(
                "CreateGraphicsPipelineState({key:?}): {e}{}",
                self.debug_messages()
            ))),
        }
    }

    /// Drain the D3D12 info queue, if the debug layer is on. Without this a rejected PSO reports
    /// only `E_INVALIDARG`, which says nothing about which field the runtime disliked.
    fn debug_messages(&self) -> String {
        let Ok(queue) = self.device.cast::<ID3D12InfoQueue>() else {
            return String::new();
        };
        let mut out = String::new();
        // SAFETY: the queue is a live interface on this device; every call below is a query, and
        // the message buffer is sized by the runtime's own reported length before it is filled.
        unsafe {
            let n = queue.GetNumStoredMessages();
            for i in 0..n {
                let mut len: usize = 0;
                if queue.GetMessage(i, None, &mut len).is_err() {
                    continue;
                }
                let mut buf = vec![0u8; len];
                let msg = buf.as_mut_ptr().cast::<D3D12_MESSAGE>();
                if queue.GetMessage(i, Some(msg), &mut len).is_err() {
                    continue;
                }
                let m = &*msg;
                let text = std::slice::from_raw_parts(
                    m.pDescription.cast::<u8>(),
                    m.DescriptionByteLength,
                );
                out.push('\n');
                out.push_str(&String::from_utf8_lossy(text));
            }
            queue.ClearStoredMessages();
        }
        out
    }

    /// Construct every state in the catalogue, so the D3D12 runtime validates all of them. The
    /// catalogue test's entry point.
    ///
    /// # Errors
    /// The first key the runtime rejects.
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

    /// Set the gamma ramp. D3D12 has no `SetGammaRamp`, so a rebuild must apply the same clamped
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
    /// The retail windowed arm always leaves `PresentationInterval = IMMEDIATE`; only logical full
    /// screen consults the full-screen sync-to-display-refresh setting. DXGI flip presentation
    /// supports interval one, so the native supported-interval ladder collapses to either zero or
    /// one here.
    pub fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
        self.present_sync_interval = u32::from(full_screen && sync_to_refresh);
    }

    /// The interval currently configured for the next real swap-chain presentation.
    #[must_use]
    pub fn present_sync_interval(&self) -> u32 {
        self.present_sync_interval
    }

    /// The argument actually handed to the last `IDXGISwapChain::Present` call.
    ///
    /// `None` means this is the offscreen/headless target; it must not be cited as window proof.
    #[must_use]
    pub fn last_present_sync_interval(&self) -> Option<u32> {
        self.last_present_sync_interval
    }

    /// Begin a frame: wait for this ring slot, reset, clear (flags 7: target, stencil and
    /// z; colour black, z = 1.0) and open the command list.
    ///
    /// # Errors
    /// Any failure from the runtime.
    pub fn begin_frame(&mut self) -> Result<(), RenderError> {
        let want = self.frames[self.frame_index].fence_value;
        // SAFETY: GetCompletedValue is a pure query on the fence.
        if want != 0 && unsafe { self.fence.GetCompletedValue() } < want {
            // SAFETY: the event handle is owned by self and is not signalled by anything else.
            unsafe {
                hr(
                    "SetEventOnCompletion",
                    self.fence.SetEventOnCompletion(want, self.fence_event),
                )?;
                WaitForSingleObject(self.fence_event, INFINITE);
            }
        }
        // Any descriptor slot and texture released far enough back that its fence has completed can
        // now go back into circulation. This is the only place a released slot re-enters it during
        // normal frame pacing; `upload_texture` runs it once more behind its own `wait_idle`.
        self.collect_retired();
        let frame = &mut self.frames[self.frame_index];
        frame.upload_used = 0;
        // The fence wait above proves the GPU is done with last time's command list, so the arenas
        // it outgrew can finally go.
        frame.retired.clear();
        // SAFETY: the GPU is done with this allocator, which the fence wait above established.
        unsafe {
            hr("Reset(allocator)", frame.allocator.Reset())?;
            hr("Reset(list)", self.list.Reset(&frame.allocator, None))?;
        }
        self.frame_open = true;

        let (rtv, back) = self.current_rtv();
        // SAFETY: every handle and resource below is owned by self and alive.
        unsafe {
            if matches!(self.target, Target::SwapChain { .. }) {
                self.transition(
                    &back,
                    D3D12_RESOURCE_STATE_PRESENT,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                );
            }
            let dsv = self.dsv_heap.GetCPUDescriptorHandleForHeapStart();
            self.list
                .OMSetRenderTargets(1, Some(&rtv), false, Some(&dsv));
            // The frame clear is black with the hard-coded alpha 0x66,
            // which is invisible because COLORWRITEENABLE masks the alpha channel; the float clear
            // here writes the same visible result. See crate::pack_clear_colour.
            self.list
                .ClearRenderTargetView(rtv, &[0.0, 0.0, 0.0, 0.4], None);
            self.list.ClearDepthStencilView(
                dsv,
                D3D12_CLEAR_FLAG_DEPTH | D3D12_CLEAR_FLAG_STENCIL,
                1.0,
                0,
                None,
            );
            let vp = D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.config.width as f32,
                Height: self.config.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.list.RSSetViewports(&[vp]);
            // There is no scissor test anywhere in the client (D3DRS_SCISSORTESTENABLE is 0 and
            // SetScissorRect is never called), so the rectangle is the whole target.
            self.list.RSSetScissorRects(&[RECT {
                left: 0,
                top: 0,
                right: self.config.width as i32,
                bottom: self.config.height as i32,
            }]);
            self.list.SetGraphicsRootSignature(&self.root_signature);
            self.legacy_tables.set([None; 4]);
            self.list.SetDescriptorHeaps(&[
                Some(self.srv_heap.clone()),
                Some(self.sampler_heap.clone()),
            ]);
            self.list
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        }
        // At scene begin, not when the filter mode is set: normal bias changes at the scene edge.
        self.sharp_lod_bias = self.texture_filtering == 2;
        Ok(())
    }

    /// End a frame: close, submit, present, advance the fence ring, and bump the frame stamp.
    ///
    /// # Errors
    /// Any failure from the runtime.
    pub fn end_frame(&mut self) -> Result<(), RenderError> {
        let (_, back) = self.current_rtv();
        // SAFETY: the list is open and every resource is alive.
        unsafe {
            if matches!(self.target, Target::SwapChain { .. }) {
                self.transition(
                    &back,
                    D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATE_PRESENT,
                );
            }
            hr("Close", self.list.Close())?;
            let lists = [Some(self.list.cast::<ID3D12CommandList>().map_err(
                |e| RenderError::Device(format!("cast ID3D12CommandList: {e}")),
            )?)];
            self.queue.ExecuteCommandLists(&lists);
        }
        if let Target::SwapChain { chain, .. } = &self.target {
            // SAFETY: the chain is alive and the list has been submitted.
            let sync_interval = self.present_sync_interval;
            // SAFETY: the chain is alive and the list has been submitted.
            hr("Present", unsafe {
                chain.Present(sync_interval, DXGI_PRESENT(0)).ok()
            })?;
            self.last_present_sync_interval = Some(sync_interval);
        }
        let value = self.next_fence_value;
        self.next_fence_value += 1;
        // SAFETY: the queue and fence are both owned by self.
        hr("Signal", unsafe { self.queue.Signal(&self.fence, value) })?;
        {
            let frame = &mut self.frames[self.frame_index];
            frame.fence_value = value;
            frame.high_water = frame.high_water.max(frame.upload_used);
        }
        // The list that may have referenced these descriptors has now been submitted, and `value` is
        // signalled behind it. This — not the release call, and not a `wait_idle` an intervening
        // `upload_texture` performed — is what puts them on the clock.
        self.flush_frame_releases(value);
        self.frame_open = false;
        self.frame_index = match &self.target {
            // SAFETY: GetCurrentBackBufferIndex is a pure query.
            Target::SwapChain { chain, .. } => {
                // SAFETY: a pure query on a live swap chain.
                let i = unsafe { chain.GetCurrentBackBufferIndex() };
                i as usize
            }
            Target::Offscreen { .. } => (self.frame_index + 1) % FRAME_COUNT,
        };
        self.frame_stamp += 1;
        Ok(())
    }

    /// Block until every submitted frame has retired.
    ///
    /// # Errors
    /// Any failure from the runtime.
    pub fn wait_idle(&mut self) -> Result<(), RenderError> {
        let value = self.next_fence_value;
        self.next_fence_value += 1;
        // SAFETY: the queue, fence and event are owned by self.
        unsafe {
            hr("Signal", self.queue.Signal(&self.fence, value))?;
            if self.fence.GetCompletedValue() < value {
                hr(
                    "SetEventOnCompletion",
                    self.fence.SetEventOnCompletion(value, self.fence_event),
                )?;
                WaitForSingleObject(self.fence_event, INFINITE);
            }
        }
        Ok(())
    }

    /// Read the render target back as BGRA8, row by row with the D3D12 row pitch removed.
    ///
    /// A fixed scene rendered offscreen at 800×600 must read back as the same RGBA8 across three
    /// consecutive runs on the same machine.
    ///
    /// # Errors
    /// Any failure from the runtime.
    pub fn capture(&mut self) -> Result<CapturedImage, RenderError> {
        let (w, h) = (self.config.width, self.config.height);
        let row = w as usize * 4;
        let aligned = row.div_ceil(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT as usize)
            * D3D12_TEXTURE_DATA_PITCH_ALIGNMENT as usize;
        let total = (aligned * h as usize) as u64;

        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: total,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut readback: Option<ID3D12Resource> = None;
        // SAFETY: descriptors are live locals; the out-parameter is read only on success.
        hr("CreateCommittedResource(readback)", unsafe {
            self.device.CreateCommittedResource(
                &heap_props(D3D12_HEAP_TYPE_READBACK),
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut readback,
            )
        })?;
        let readback = readback.ok_or_else(|| RenderError::Device("no readback buffer".into()))?;

        let (_, back) = self.current_rtv();
        // A one-shot allocator and list of their own, *not* the frame ring's -- the discipline
        // `upload_texture` and the mip readback already follow. Resetting
        // `frames[frame_index].allocator` and waiting for the GPU *afterwards* would be wrong:
        // `end_frame` has just advanced `frame_index`, so that slot is the one submitted `FRAME_COUNT - 1` frames
        // ago, and its fence has not been waited on by anything. `ID3D12CommandAllocator::Reset`
        // while a list recorded from the allocator is still executing is forbidden (debug layer
        // EXECUTION ERROR #552, `COMMAND_ALLOCATOR_SYNC`); on WARP, whose command memory is this
        // process's heap, it is a write into memory a worker thread is still reading. A fresh
        // allocator has nothing in flight, so there is nothing to synchronise with, and the ring's
        // own `begin_frame` remains the only place a ring allocator is ever reset.
        // SAFETY: CreateCommandAllocator returns a fresh COM object owned by this scope.
        let alloc: ID3D12CommandAllocator = hr("CreateCommandAllocator(readback)", unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        })?;
        // SAFETY: `alloc` outlives the list, which is closed and executed before `wait_idle`
        // below retires it; both are dropped only after that wait.
        let list: ID3D12GraphicsCommandList = hr("CreateCommandList(readback)", unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None)
        })?;
        // SAFETY: the list is created open; every resource below is owned by self and alive, and
        // the copy is queued behind the frame that drew into `back`, on the same queue.
        unsafe {
            let before = match self.target {
                Target::SwapChain { .. } => D3D12_RESOURCE_STATE_PRESENT,
                Target::Offscreen { .. } => D3D12_RESOURCE_STATE_RENDER_TARGET,
            };
            Self::transition_on(&list, &back, before, D3D12_RESOURCE_STATE_COPY_SOURCE);
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&readback),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: BACK_BUFFER_FORMAT,
                            Width: w,
                            Height: h,
                            Depth: 1,
                            RowPitch: aligned as u32,
                        },
                    },
                },
            };
            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: std::mem::transmute_copy(&back),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            Self::transition_on(&list, &back, D3D12_RESOURCE_STATE_COPY_SOURCE, before);
            hr("Close(readback)", list.Close())?;
            let lists =
                [Some(list.cast::<ID3D12CommandList>().map_err(|e| {
                    RenderError::Device(format!("cast: {e}"))
                })?)];
            self.queue.ExecuteCommandLists(&lists);
        }
        self.wait_idle()?;
        drop(list);
        drop(alloc);

        let mut bgra = vec![0u8; row * h as usize];
        // SAFETY: `total` bytes were allocated above and the GPU has finished writing them, which
        // `wait_idle` established. The mapped pointer is used only within this block and unmapped
        // before it ends.
        unsafe {
            let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            hr("Map", readback.Map(0, None, Some(&mut ptr)))?;
            let src = std::slice::from_raw_parts(ptr.cast::<u8>(), total as usize);
            for y in 0..h as usize {
                bgra[y * row..(y + 1) * row].copy_from_slice(&src[y * aligned..y * aligned + row]);
            }
            readback.Unmap(0, None);
        }
        Ok(CapturedImage {
            width: w,
            height: h,
            bgra,
        })
    }

    /// The dynamic vertex ring. Reproduces the client's stream fill and its buffer reset:
    ///
    /// * a frame's dynamic data is **contiguous** — one arena per frame slot, bump-allocated;
    /// * the buffer **only grows**, to the previous frame's high-water mark, and never shrinks
    ///   within a session;
    /// * a request larger than the whole buffer fails rather than wrapping
    ///   (a count above the buffer's vertex capacity returns false).
    ///
    /// Returns the GPU virtual address of the copied bytes.
    ///
    /// # Errors
    /// [`RenderError::Device`] when the arena cannot be created or mapped.
    pub fn upload_bytes(&mut self, data: &[u8]) -> Result<u64, RenderError> {
        // 256-byte alignment keeps a constant-buffer view legal at any returned address.
        const ALIGN: u64 = D3D12_CONSTANT_BUFFER_DATA_PLACEMENT_ALIGNMENT as u64;
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
            let buffer = Self::create_upload_buffer(&self.device, capacity)?;
            let frame = &mut self.frames[index];
            // Retire the old arena; do not release it. Every GPU virtual address already handed
            // out this frame points into it and the command list holding them has not been
            // submitted, so dropping it here is a use-after-free that shows up as a device removal
            // a dozen draws into a terrain frame. `begin_frame` frees these behind the fence.
            if let Some(old) = frame.upload.replace(buffer) {
                frame.retired.push(old);
            }
            frame.upload_capacity = capacity;
            frame.upload_used = 0;
        }
        let frame = &mut self.frames[index];
        let offset = frame.upload_used;
        let buffer = frame
            .upload
            .as_ref()
            .ok_or_else(|| RenderError::Device("no upload arena".into()))?;
        // SAFETY: the arena is an UPLOAD-heap buffer, which is CPU-visible for its whole lifetime;
        // `offset + data.len()` is within `capacity` by the growth above, and the GPU is not reading
        // this slot because `begin_frame` waited on its fence before resetting `upload_used`.
        unsafe {
            let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            // An empty read range: the CPU never reads back from an upload buffer.
            let read = D3D12_RANGE { Begin: 0, End: 0 };
            hr("Map(upload)", buffer.Map(0, Some(&read), Some(&mut ptr)))?;
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                ptr.cast::<u8>().add(offset as usize),
                data.len(),
            );
            buffer.Unmap(0, None);
            let address = buffer.GetGPUVirtualAddress() + offset;
            frame.upload_used += want;
            Ok(address)
        }
    }

    fn create_upload_buffer(
        device: &ID3D12Device,
        bytes: u64,
    ) -> Result<ID3D12Resource, RenderError> {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_UNKNOWN,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        let mut buffer: Option<ID3D12Resource> = None;
        // SAFETY: descriptors are live locals; the out-parameter is read only on success.
        hr("CreateCommittedResource(upload)", unsafe {
            device.CreateCommittedResource(
                &heap_props(D3D12_HEAP_TYPE_UPLOAD),
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_GENERIC_READ,
                None,
                &mut buffer,
            )
        })?;
        buffer.ok_or_else(|| RenderError::Device("no upload buffer".into()))
    }

    /// The high-water mark of dynamic bytes in the current frame slot. The client's equivalent is
    /// its ideal vertex count, which is what its per-frame reset grows the buffer to.
    #[must_use]
    pub fn upload_high_water(&self) -> u64 {
        self.frames[self.frame_index].high_water
    }

    /// Draw one batch of vertices straight out of the dynamic ring, under a given key.
    ///
    /// This is the shape of the client's dynamic-primitive pass: fill the per-format
    /// dynamic stream, then draw. **It draws exactly what it is handed, exactly now** — no sorting,
    /// no culling, no reordering.
    ///
    /// # Errors
    /// Any failure from the runtime.
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
        let count = vertices.len() as u32 / stride;
        let pso = self.pipeline_state(key)?;
        self.draw_calls += 1;

        let mut draw = *per_draw;
        // `.xy` is the `D3DTS_TEXTURE0` translation; `.zw` is left alone, because the
        // `PRETRANSFORMED` permutation reads the viewport extent out of it and
        // [`Self::draw_portal_poly`] is the one caller that puts something there. Every other
        // caller leaves it zero, which is what it was before.
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
        let vb = self.upload_bytes(vertices)?;
        let frame_cb = self.upload_bytes(as_bytes(&frame))?;
        let draw_cb = self.upload_bytes(as_bytes(&draw))?;

        // SAFETY: the list is open (begin_frame opened it), the PSO and root signature are alive,
        // and every address handed over points inside an upload arena that stays alive until this
        // frame's fence retires.
        unsafe {
            self.list.SetPipelineState(&pso);
            self.list.SetGraphicsRootConstantBufferView(0, frame_cb);
            self.list.SetGraphicsRootConstantBufferView(1, draw_cb);
            let view = D3D12_VERTEX_BUFFER_VIEW {
                BufferLocation: vb,
                SizeInBytes: vertices.len() as u32,
                StrideInBytes: stride,
            };
            self.list.IASetVertexBuffers(0, Some(&[view]));
            self.list.DrawInstanced(count, 1, 0, 0);
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
    /// * the primitive is a `D3DPT_TRIANGLEFAN` of `n - 2` triangles. D3D12 has no fan topology, so
    ///   the fan is expanded to the list `(0, i, i+1)` — the same triangles in the same order, which
    ///   for a `CULLMODE_NONE` depth write is the same coverage;
    /// * the vertex holds **screen** coordinates, not clip ones, because that is what `XYZRHW`
    ///   means; the viewport inverse is in the shader (`legacy.hlsl`, `PRETRANSFORMED`);
    /// * the client unbinds texture stage 0. D3D12 has no such state and the pixel shader
    ///   samples `t0` unconditionally, so a 1x1 opaque-white texture stands in for it — created once
    ///   here, on first use, so a build that never stamps never pays a descriptor pair for it. It is
    ///   unobservable either way: the vertex alpha is 0, so `SRCALPHA/INVSRCALPHA` leaves the render
    ///   target exactly as it found it whatever the sample returns.
    ///
    /// # Errors
    /// Any failure from the runtime, including a full descriptor heap on the first call.
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
            // Each point lands at `(viewport.x + xw/w, viewport.y + yw/w)`. The viewport origin
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
            format: dereth_primitives::TextureFormat::Bgra8,
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
    /// Any failure from the runtime.
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
        match &mut self.target {
            Target::SwapChain { chain, buffers } => {
                buffers.clear();
                // SAFETY: every back-buffer reference has just been dropped, which is what
                // ResizeBuffers requires.
                hr("ResizeBuffers", unsafe {
                    chain.ResizeBuffers(
                        FRAME_COUNT as u32,
                        width,
                        height,
                        BACK_BUFFER_FORMAT,
                        DXGI_SWAP_CHAIN_FLAG(0),
                    )
                })?;
                for i in 0..FRAME_COUNT as u32 {
                    // SAFETY: i is within BufferCount.
                    buffers.push(hr("GetBuffer", unsafe { chain.GetBuffer(i) })?);
                }
                // SAFETY: a pure query.
                self.frame_index = (unsafe { chain.GetCurrentBackBufferIndex() }) as usize;
            }
            Target::Offscreen { .. } => {
                self.target = Self::create_target(
                    &self.factory,
                    &self.device,
                    &self.queue,
                    None,
                    &self.config,
                )?;
                self.frame_index = 0;
            }
        }
        self.depth = Self::create_depth(&self.device, &self.config)?;
        self.create_views()?;
        // The client's display-mode-change handler re-applies the gamma after every successful reset.
        let g = self.gamma;
        self.set_gamma(g);
        Ok(())
    }

    /// Upload a decoded texture and return a handle to its shader-resource view.
    ///
    /// Equivalent to [`Self::upload_texture_keyed`] with [`TextureKey::UNCACHED`], which is
    /// When `key == 0`, the texture goes in the uncached texture table,
    /// is never shared with another owner, and lives exactly as long as its one link.
    ///
    /// **The returned slot is owned by the caller and must be handed back** with
    /// [`Self::release_texture`] when its owner goes away — a landblock leaving the streaming
    /// window, a screen closing, an object being destroyed. Nothing else reclaims it.
    ///
    /// # Errors
    /// Any failure from the runtime, a level whose byte count does not match its extent, or a full
    /// descriptor heap (which is counted in [`DescriptorStats::exhaustions`]).
    pub fn upload_texture(&mut self, t: &TextureData) -> Result<TextureSlot, RenderError> {
        self.upload_texture_keyed(TextureKey::UNCACHED, t)
    }

    /// Upload a decoded texture, or take a second reference on the one already cached under `key`.
    ///
    /// This is the client's combined-texture cache: a non-zero key that is already present `AddRef`s and returns
    /// the existing texture without touching the device; a zero key never hits. Build the key with
    /// [`crate::descriptor::combined_texture_key`], whose only inputs are the palette DataID and the
    /// indexed-texture DataID, and wrap it in the [`TextureKey`] constructor for **your own
    /// producer**. Four producers share this table and two of them can compute
    /// identical sixty-four-bit payloads; [`crate::descriptor::TextureSpace`] says which they are
    /// and why the space has to be part of the key rather than an argument about the shipped data.
    ///
    /// Every returned slot — cache hit or fresh upload — is one link that the caller owns and must
    /// [`Self::release_texture`].
    ///
    /// Only `Bgra8` and the three block-compressed formats reach here, which is the whole of
    /// `dereth_primitives::TextureFormat`. Every provided level is copied through staging; this entry
    /// point does not generate levels. World image-texture owners use [`Self::upload_imgtex_keyed`] for
    /// the separate, capability-gated runtime copy.
    ///
    /// # Errors
    /// Any failure from the runtime, a level whose byte count does not match its extent, or a full
    /// descriptor heap.
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
    /// The caller still owns one cache link, with the normal last-release/fence lifetime.
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
        let (format, block) = match t.format {
            TextureFormat::Bgra8 => (DXGI_FORMAT_B8G8R8A8_UNORM, false),
            TextureFormat::Bc1 => (DXGI_FORMAT_BC1_UNORM, true),
            TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => (DXGI_FORMAT_BC2_UNORM, true),
            TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => (DXGI_FORMAT_BC3_UNORM, true),
            // dereth_primitives::TextureFormat is #[non_exhaustive]; a value this crate has not been
            // taught is a change to that shared type, not something to guess at.
            other => {
                return Err(RenderError::Device(format!(
                    "unhandled texture format {other:?}"
                )))
            }
        };
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
        if generate && self.mip_generator.is_none() {
            self.mip_generator = Some(mipgen::MipGenerator::new(&self.device)?);
        }
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: u64::from(t.width),
            Height: t.height,
            DepthOrArraySize: 1,
            MipLevels: levels,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Flags: if generate {
                D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
            } else {
                D3D12_RESOURCE_FLAG_NONE
            },
            ..Default::default()
        };
        let mut texture: Option<ID3D12Resource> = None;
        // SAFETY: descriptors are live locals; the out-parameter is read only on success.
        hr("CreateCommittedResource(texture)", unsafe {
            self.device.CreateCommittedResource(
                &default_heap(),
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut texture,
            )
        })?;
        let texture = texture.ok_or_else(|| RenderError::Device("no texture".into()))?;

        // Stage every level and copy it in on a one-shot command list.
        let mut staging = Vec::new();
        let mip_views;
        // A one-shot allocator and list of its own, *not* the frame's. Resetting `self.list` here
        // would discard everything the caller had already recorded into the open frame, which made
        // creating a texture between `begin_frame` and `end_frame` silently drop the frame -- and
        // that is exactly the lazy path a terrain merge cache wants.
        // SAFETY: CreateCommandAllocator returns a fresh COM object owned by this scope.
        let alloc: ID3D12CommandAllocator = hr("CreateCommandAllocator(copy)", unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        })?;
        // SAFETY: `alloc` outlives the list, which is closed before this function returns.
        let list: ID3D12GraphicsCommandList = hr("CreateCommandList(copy)", unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None)
        })?;
        // SAFETY: `alloc` and `list` are fresh objects owned by this scope; the list is created
        // open, and `wait_idle` below retires the work before the staging buffers are dropped.
        unsafe {
            let (mut w, mut h) = (t.width, t.height);
            for (level, bits) in t.levels.iter().enumerate() {
                let row = if block {
                    w.div_ceil(4) as usize * bytes_per_block(format)
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
                let pitch = row.div_ceil(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT as usize)
                    * D3D12_TEXTURE_DATA_PITCH_ALIGNMENT as usize;
                let buffer = Self::create_upload_buffer(&self.device, (pitch * rows) as u64)?;
                let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
                let read = D3D12_RANGE { Begin: 0, End: 0 };
                hr("Map(staging)", buffer.Map(0, Some(&read), Some(&mut ptr)))?;
                for y in 0..rows {
                    std::ptr::copy_nonoverlapping(
                        bits[y * row..].as_ptr(),
                        ptr.cast::<u8>().add(y * pitch),
                        row,
                    );
                }
                buffer.Unmap(0, None);

                let dst = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&texture),
                    Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                        SubresourceIndex: level as u32,
                    },
                };
                let src = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&buffer),
                    Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                        PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                            Offset: 0,
                            Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                                Format: format,
                                Width: w,
                                Height: h,
                                Depth: 1,
                                RowPitch: pitch as u32,
                            },
                        },
                    },
                };
                list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
                staging.push(buffer);
                w = crate::mip::half(w);
                h = crate::mip::half(h);
            }
            if generate {
                mip_views = Some(
                    self.mip_generator
                        .as_ref()
                        .expect("generator initialized")
                        .record(&self.device, &list, &texture, t.width, t.height, levels)?,
                );
            } else {
                mip_views = None;
                Self::transition_on(
                    &list,
                    &texture,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                );
            }
            hr("Close", list.Close())?;
            let lists =
                [Some(list.cast::<ID3D12CommandList>().map_err(|e| {
                    RenderError::Device(format!("cast: {e}"))
                })?)];
            self.queue.ExecuteCommandLists(&lists);
        }
        // No wait: the queue runs command lists in order, so every later frame's list sees this
        // one's writes, and a texture released meanwhile is retired against a later fence value.
        // What the list used is kept until the fence passes it; see `collect_retired`.
        let value = self.next_fence_value;
        self.next_fence_value += 1;
        // SAFETY: the queue and the fence are owned by self.
        hr("Signal", unsafe { self.queue.Signal(&self.fence, value) })?;
        self.pending_uploads.push(PendingUpload {
            fence: value,
            _allocator: alloc,
            _list: list,
            _staging: staging,
            _mip_views: mip_views,
        });

        self.register_texture(key, texture, format, levels)
    }

    /// Give a finished texture (every level in `PIXEL_SHADER_RESOURCE`) its SRV pair and, under a
    /// non-zero `key`, a cache entry. Called right after the wait that retired its commands.
    fn register_texture(
        &mut self,
        key: TextureKey,
        texture: ID3D12Resource,
        format: DXGI_FORMAT,
        levels: u16,
    ) -> Result<TextureSlot, RenderError> {
        // One SRV pair per texture, from the free list, after taking back every slot whose release
        // the device has passed: a create/release/create cycle then reuses the slot rather than
        // advancing the heap, once the device has caught up with the release.
        self.collect_retired();
        // An upload does not wait for the device, so a slot released since the device last caught
        // up is not yet reusable. When that is all that stands between this texture and a full
        // heap, wait for the device and take the slot back, rather than refuse.
        if self.descriptors.free_slots() == 0
            && self.descriptors.frontier() >= self.descriptors.capacity()
            && self.descriptors.pending_slots() > 0
        {
            if let Err(e) = self.wait_idle() {
                self.retired_textures.push((self.next_fence_value, texture));
                return Err(e);
            }
            self.collect_retired();
        }
        let Some(slot) = self.descriptors.alloc() else {
            // The upload may still be running: retire the texture against the next fence value, as
            // a release does, rather than dropping it now.
            self.retired_textures.push((self.next_fence_value, texture));
            // Counted in `DescriptorStats::exhaustions`, not merely logged. Nothing has been written
            // to the heap.
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
        let srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: format,
            ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: u32::from(levels),
                    PlaneSlice: 0,
                    ResourceMinLODClamp: 0.0,
                },
            },
        };
        // SAFETY: both handles are inside the heap (checked above) and `texture` is a live 2D
        // texture in the pixel-shader-resource state.
        unsafe {
            let mut h = self.srv_heap.GetCPUDescriptorHandleForHeapStart();
            h.ptr += slot as usize * self.srv_size as usize;
            self.device
                .CreateShaderResourceView(&texture, Some(&srv), h);
            // Stage 1 defaults to the same texture, so a single-texture draw has a valid t1.
            h.ptr += self.srv_size as usize;
            self.device
                .CreateShaderResourceView(&texture, Some(&srv), h);
        }
        self.textures.insert(slot, texture);
        self.texture_table.insert(key, slot);
        Ok(TextureSlot(slot))
    }

    /// Take another link on a texture the caller already holds, so two
    /// owners can share one slot and the later of the two releases is the one that frees it.
    ///
    /// Returns the new link count, or `None` for a slot with no live texture — counted in
    /// [`TextureTableStats::unknown_add_refs`].
    pub fn retain_texture(&mut self, slot: TextureSlot) -> Option<u32> {
        self.texture_table.add_ref(slot.0)
    }

    /// Drop one link, and at zero free the texture and give its
    /// descriptor pair back to the heap.
    ///
    /// The slot is **not** reusable at once: it is parked behind a fence value, because a command
    /// list may still be reading the descriptor. `upload_bytes` retires outgrown arenas the same
    /// way, after a use-after-free that showed up as a device removal on WARP.
    ///
    /// *Which* fence value depends on whether a frame is open, and the difference matters:
    ///
    /// * **Outside a frame**, every command list that could reference the descriptor has been
    ///   submitted, so the value the next `Signal` carries — `next_fence_value` — is enough.
    /// * **Inside a frame**, the open list may already have bound the slot and has *not* been
    ///   submitted. `upload_texture` is legal mid-frame and runs a `wait_idle` of its own, which
    ///   would complete a fence value while that list still sits unsubmitted; retiring against it
    ///   would free the resource out from under a list that is about to run. So the release is held
    ///   until `end_frame` has submitted the list and taken its fence value. The slot therefore
    ///   still counts as live, and still occupies budget, until the frame ends.
    ///
    /// After this returns [`Released::Freed`] the caller must not bind the slot again. Binding a
    /// released slot reads a descriptor that is about to be overwritten, which is the whole class of
    /// bug the fence gate exists to prevent on the *other* side.
    ///
    /// A slot with no live texture — a double release, or a handle from a previous device — is
    /// counted in [`TextureTableStats::unknown_releases`] and otherwise ignored, because the callers
    /// of this are teardown paths where a panic would lose more than it reports.
    pub fn release_texture(&mut self, slot: TextureSlot) -> Released {
        let outcome = self.texture_table.release(slot.0);
        if outcome == Released::Freed {
            let resource = self.textures.remove(&slot.0);
            if self.frame_open {
                self.released_in_frame.push((slot.0, resource));
            } else {
                // The value the next Signal will carry. Everything already submitted precedes it.
                let fence = self.next_fence_value;
                self.descriptors.release(slot.0, fence);
                if let Some(resource) = resource {
                    self.retired_textures.push((fence, resource));
                }
            }
        }
        outcome
    }

    /// Put the releases a frame held onto the clock, against a fence value signalled behind that
    /// frame's submission.
    fn flush_frame_releases(&mut self, fence: u64) {
        for (slot, resource) in std::mem::take(&mut self.released_in_frame) {
            self.descriptors.release(slot, fence);
            if let Some(resource) = resource {
                self.retired_textures.push((fence, resource));
            }
        }
    }

    /// Move every descriptor slot and texture resource whose fence has completed back into
    /// circulation. Called at the top of each frame and before each descriptor allocation.
    fn collect_retired(&mut self) {
        // SAFETY: GetCompletedValue is a pure query on a fence owned by self.
        let completed = unsafe { self.fence.GetCompletedValue() };
        self.descriptors.retire(completed);
        self.retired_textures
            .retain(|(fence, _)| *fence > completed);
        self.pending_uploads.retain(|u| u.fence > completed);
    }

    /// Descriptor-heap occupancy and its counters. `frontier` is the number this crate exists to
    /// bound: without slot reuse it would be the count of every texture the session had ever uploaded.
    #[must_use]
    pub fn descriptor_stats(&self) -> DescriptorStats {
        self.descriptors.stats()
    }

    /// Descriptor slots ever taken from fresh heap space, and the budget they come out of.
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

    /// Whether a frame's command list is open. A non-zero [`DescriptorUsage::deferred`] while this
    /// is false means releases are stranded in a frame that was begun and never ended.
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

    /// Every live keyed texture, sorted: the census of which
    /// [`crate::descriptor::TextureSpace`]s a device is actually holding, and whether any two of
    /// them are holding the same sixty-four-bit payload.
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
    /// A single-level source can have a generated runtime mip chain.
    #[must_use]
    pub fn texture_mip_levels(&self, slot: TextureSlot) -> Option<u16> {
        self.textures.get(&slot.0).map(|texture| {
            // SAFETY: the map owns a live resource; GetDesc returns a value without mutation.
            unsafe { texture.GetDesc().MipLevels }
        })
    }

    /// Narrow the descriptor budget so a test can reach exhaustion without filling the real heap.
    #[cfg(test)]
    pub(crate) fn narrow_descriptor_budget(&mut self, slots: u32) {
        self.descriptors.narrow_for_test(slots);
    }

    /// Bind a texture pair and a sampler for the next draw.
    ///
    /// `sampler` names eight request variants:0 linear/wrap,1 linear/clamp,2 point/wrap,3 point/clamp,
    /// and4..7 the corresponding mixed-address axes. Retail's global preference rewrites only
    /// eligible filters; explicit POINT is never promoted. The requested index/census is stable.
    ///
    /// `slot` must still be held: a slot whose last link went away in
    /// [`Self::release_texture`] is on its way back to the free list and its descriptors will be
    /// rewritten by the next upload.
    pub fn bind_texture(&self, slot: TextureSlot, sampler: u32) {
        // Masked once, so the descriptor offset below and the census above cannot
        // disagree about which sampler this draw asked for.
        let which = sampler % SAMPLER_COUNT;
        let mut counts = self.sampler_binds.get();
        counts[which as usize] += 1;
        self.sampler_binds.set(counts);
        let descriptor = self.sampler_descriptor_index(which);
        self.bound_sampler.set(Some(descriptor));
        // SAFETY: the list is open, and both offsets are inside their heaps -- `slot` was returned
        // by `upload_texture`, which bounds-checked it. Both sampler table entries stay inside
        // their immutable effective-state bank, including request7's explicit guard descriptor.
        unsafe {
            let mut gpu = self.srv_heap.GetGPUDescriptorHandleForHeapStart();
            gpu.ptr += u64::from(slot.0) * u64::from(self.srv_size);
            self.list.SetGraphicsRootDescriptorTable(2, gpu);
            let mut s = self.sampler_heap.GetGPUDescriptorHandleForHeapStart();
            s.ptr += u64::from(descriptor) * u64::from(self.sampler_size);
            self.list.SetGraphicsRootDescriptorTable(3, s);
            // Stage 1 has its own table, separate from stage 0's, so it is filled here with what
            // a shared two-wide table would give it — the slot's second descriptor (the same
            // image) and the next legacy sampler request. A draw that wants a real second texture calls `bind_stage1_texture` after
            // this, which is the same order the client issues them in:
            // the base surface first, the detail surface second.
            let mut gpu1 = self.srv_heap.GetGPUDescriptorHandleForHeapStart();
            gpu1.ptr += u64::from(slot.0 + 1) * u64::from(self.srv_size);
            self.list.SetGraphicsRootDescriptorTable(4, gpu1);
            let mut s1 = self.sampler_heap.GetGPUDescriptorHandleForHeapStart();
            s1.ptr += u64::from(descriptor + 1) * u64::from(self.sampler_size);
            self.list.SetGraphicsRootDescriptorTable(5, s1);
            self.legacy_tables
                .set([Some(gpu.ptr), Some(s.ptr), Some(gpu1.ptr), Some(s1.ptr)]);
        }
    }

    /// Put a **second** texture in stage 1,
    /// which is what the detail-texture pass is made of.
    ///
    /// The pass binds the current detail texture, wraps both axes, selects linear minification,
    /// magnification and mip filtering, then enables additive alpha blending and a writable
    /// less-or-equal depth test for stage zero:
    ///
    /// ```text
    /// tex = current_detail_surface.texture_map()
    /// bind_stage_texture(stage, tex)
    /// set_sampler_address_mode(stage, WRAP, WRAP)
    /// set_sampler_filter_mode(stage, LINEAR, LINEAR, LINEAR)
    /// if stage == 0: enable configured additive blending and writable LESS_EQUAL depth testing
    /// ```
    ///
    /// so the single-pass arm (`stage == 1`, the only one this build issues) changes **no** blend
    /// or depth state at all: the detail texture and a WRAP/LINEAR sampler, and nothing else. That
    /// is sampler request 0 in [`Self::bind_texture`]'s table, whatever request the base surface
    /// asked for.
    ///
    /// Call it **after** [`Self::bind_texture`], which rewrites stage 1 as part of restoring the
    /// old single-table behaviour.
    pub fn bind_stage1_texture(&self, slot: TextureSlot) {
        // WRAP/WRAP, LINEAR/LINEAR/LINEAR -- request 0 of the effective bank.
        let descriptor = self.sampler_descriptor_index(0);
        // SAFETY: as in `bind_texture` -- the list is open and both offsets are inside their heaps.
        unsafe {
            let mut gpu = self.srv_heap.GetGPUDescriptorHandleForHeapStart();
            gpu.ptr += u64::from(slot.0) * u64::from(self.srv_size);
            self.list.SetGraphicsRootDescriptorTable(4, gpu);
            let mut s = self.sampler_heap.GetGPUDescriptorHandleForHeapStart();
            s.ptr += u64::from(descriptor) * u64::from(self.sampler_size);
            self.list.SetGraphicsRootDescriptorTable(5, s);
            let mut tables = self.legacy_tables.get();
            tables[2] = Some(gpu.ptr);
            tables[3] = Some(s.ptr);
            self.legacy_tables.set(tables);
        }
        self.stage1_binds.set(self.stage1_binds.get() + 1);
    }

    /// How many draws have had a **detail** texture put in stage 1 by
    /// [`Self::bind_stage1_texture`] since [`Self::clear_stage1_binds`] — the
    /// census that separates "the pass ran" from "the preference was polled".
    #[must_use]
    pub fn stage1_binds(&self) -> u64 {
        self.stage1_binds.get()
    }

    /// Zero [`Self::stage1_binds`], so one frame's detail binds can be counted on their own.
    pub fn clear_stage1_binds(&self) {
        self.stage1_binds.set(0);
    }

    /// How many draws have bound each of the four samplers since
    /// [`Self::clear_sampler_binds`], indexed exactly as [`Self::bind_texture`]'s argument:
    /// 0 linear/wrap, 1 linear/clamp, 2 point/wrap, 3 point/clamp.
    ///
    /// This exists so that "the glyph draw binds POINT" is a **measurement** rather than a reading
    /// of the call site. A constant both the caller and its test resolve through the same symbol
    /// cannot detect a caller that stopped using it, and this counter is the other end of that
    /// observation.
    #[must_use]
    pub fn sampler_binds(&self) -> [u64; SAMPLER_COUNT as usize] {
        self.sampler_binds.get()
    }

    /// Zero [`Self::sampler_binds`], so one frame's binds can be counted on their own.
    pub fn clear_sampler_binds(&self) {
        self.sampler_binds.set([0; SAMPLER_COUNT as usize]);
    }

    /// Restrict the pass to a sub-rectangle of the
    /// render target.
    ///
    /// The UI viewport element brackets its creature render with two of
    /// these: the element's own rectangle on the way in, the saved one on the way out. That is the
    /// whole of "a 3D preview lives inside a UI panel" -- there is no render-to-texture anywhere in
    /// the client.
    ///
    /// **D3D9's viewport clips; D3D12's does not**, so this sets the scissor to the same rectangle.
    /// `begin_frame` leaves both at the full target, and the client's own
    /// `D3DRS_SCISSORTESTENABLE` is 0 precisely because its viewport already did this job.
    ///
    /// The rectangle is clamped to the target the way the client's viewport setter clamps it
    /// ([`crate::camera::clamp_viewport`]); an empty one draws nothing rather than tripping the
    /// runtime.
    pub fn set_viewport(&self, v: crate::camera::Viewport) {
        let v = crate::camera::clamp_viewport(v, self.config.width, self.config.height);
        if v.width == 0 || v.height == 0 {
            return;
        }
        // SAFETY: the list is open and the rectangle is inside the render target, which
        // `clamp_viewport` above guarantees.
        unsafe {
            self.list.RSSetViewports(&[D3D12_VIEWPORT {
                TopLeftX: v.x as f32,
                TopLeftY: v.y as f32,
                Width: v.width as f32,
                Height: v.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]);
            self.list.RSSetScissorRects(&[RECT {
                left: v.x as i32,
                top: v.y as i32,
                right: (v.x + v.width) as i32,
                bottom: (v.y + v.height) as i32,
            }]);
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
    /// stencil and **bit 4 the depth buffer** (flag 4 maps to D3D's depth-clear bit), so
    /// Clearing with flag 4 clears only the depth buffer and leaves the colour
    /// already in the rectangle alone. That is what lets a preview draw over the panel art the UI
    /// blitted underneath it instead of punching a black hole in it.
    pub fn clear_depth(&self, v: crate::camera::Viewport) {
        let v = crate::camera::clamp_viewport(v, self.config.width, self.config.height);
        if v.width == 0 || v.height == 0 {
            return;
        }
        // SAFETY: the list is open, the DSV heap is owned by self, and the rectangle is inside the
        // depth target.
        unsafe {
            let dsv = self.dsv_heap.GetCPUDescriptorHandleForHeapStart();
            self.list.ClearDepthStencilView(
                dsv,
                D3D12_CLEAR_FLAG_DEPTH,
                1.0,
                0,
                Some(&[RECT {
                    left: v.x as i32,
                    top: v.y as i32,
                    right: (v.x + v.width) as i32,
                    bottom: (v.y + v.height) as i32,
                }]),
            );
        }
    }

    fn current_rtv(&self) -> (D3D12_CPU_DESCRIPTOR_HANDLE, ID3D12Resource) {
        // SAFETY: a pure query, offset by an index below the heap's NumDescriptors.
        let mut h = unsafe { self.rtv_heap.GetCPUDescriptorHandleForHeapStart() };
        match &self.target {
            Target::SwapChain { buffers, .. } => {
                h.ptr += self.frame_index * self.rtv_size as usize;
                (h, buffers[self.frame_index].clone())
            }
            Target::Offscreen { texture } => (h, texture.clone()),
        }
    }

    /// Issue one resource barrier.
    ///
    /// # Safety
    /// The command list must be open, and `resource` must currently be in state `from`.
    unsafe fn transition(
        &self,
        resource: &ID3D12Resource,
        from: D3D12_RESOURCE_STATES,
        to: D3D12_RESOURCE_STATES,
    ) {
        let barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: std::mem::transmute_copy(resource),
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: from,
                    StateAfter: to,
                }),
            },
        };
        // SAFETY (caller's obligation, restated): the list is open and the state matches.
        self.list.ResourceBarrier(&[barrier]);
    }

    /// The same barrier, recorded into a caller-supplied list rather than the frame's.
    ///
    /// # Safety
    /// `list` must be open, and `resource` must currently be in state `from`.
    unsafe fn transition_on(
        list: &ID3D12GraphicsCommandList,
        resource: &ID3D12Resource,
        from: D3D12_RESOURCE_STATES,
        to: D3D12_RESOURCE_STATES,
    ) {
        let barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: std::mem::transmute_copy(resource),
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: from,
                    StateAfter: to,
                }),
            },
        };
        // SAFETY (caller's obligation, restated): the list is open and the state matches.
        list.ResourceBarrier(&[barrier]);
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        let _ = self.wait_idle();
        // SAFETY: the handle was created by CreateEventW in `new` and is not closed elsewhere.
        unsafe {
            let _ = CloseHandle(self.fence_event);
        }
    }
}

/// The descriptor budget of the shader-visible heap, in **descriptors**: 65,536 of them, which is
/// 32,768 texture pairs. Slots are reclaimed (see [`crate::descriptor`]), so this bounds how many
/// textures are *live at once* rather than how many a session may ever upload.
///
/// # Why this number, and why it is not 4,096
///
/// **A real session's working set did not fit the earlier budget.** It was 4,096
/// descriptors — 2,048 pairs — which is a rebuild choice with no counterpart in the original: D3D9
/// bound textures by pointer and had no descriptor table at all, so nothing here can be
/// transcribed and the budget has to be justified by measurement instead.
///
/// The measurement, in the client's mesh-eviction test: a full `long-solo-play` replay
/// (1,098.5 s, the longest recording in the corpus) peaks at **2,121 live pairs** — already past
/// the old budget, which is why the scene stopped with `descriptor heap exhausted` at
/// **t = 973.3 s**, 125 s before the recording ends. Two consecutive replays of it on one device
/// peak at 2,122, so that figure is a *working set* and not a running total.
///
/// 32,768 pairs is 2 MiB of descriptor heap and roughly fifteen times the measured peak. D3D12
/// guarantees a shader-visible CBV/SRV/UAV heap of up to 1,000,000 descriptors even at Resource
/// Binding Tier 1, so this is nowhere near a hardware limit.
///
/// **This is a budget, not a tripwire.** Raising it makes every `exhaustions == 0` assertion in the
/// suite less sensitive, so the thing that now catches an unbounded cache is the *peak occupancy*
/// assertion in the mesh-eviction test and the two-pass ratio beside it, not the heap running out.
///
/// # The working set has since come back under the old budget, and this was still not lowered
///
/// Three later fixes closed the gaps that measurement left — a
/// departed landblock's bake, the mesh set an appearance change replaces, and `resolve_surface`
/// uploading every texture `UNCACHED` where the client's combined-texture cache has a
/// (palette DID, texture DID) cache. `long-solo-play`'s peak went **2,121 -> 1,474 pairs** and its
/// final residency **965 -> 327**, so the replay now completes on a 4,096-descriptor heap and
/// the old pin was deleted rather than relaxed (see the mesh-eviction test's
/// `long_solo_play_fits_the_old_four_thousand_ninety_six_descriptor_budget`).
///
/// It was left at 65,536 deliberately. 1,474 is one recording, at `land_radius: 1`, with no UI on
/// the device: the shipped client runs a 7x7 window, uploads ~129 UI images and bakes a font atlas
/// per face into this same heap. 574 pairs of headroom over a single measurement is not a budget,
/// and 2 MiB of descriptors is not worth a ceiling that arrives as a hard stop mid-session.
const SRV_HEAP_SIZE: u32 = 65_536;
/// `{linear, point}` × the four `(AddressU, AddressV)` combinations — **eight**, not four.
/// `crate::ui::pixel_rules::UI_SAMPLER_COUNT` is the same number stated where the
/// index arithmetic lives; the two are asserted equal in `crate::ui`'s tests.
const SAMPLER_COUNT: u32 = crate::sampler::SAMPLER_COUNT;

/// The stencil state for "stencil is off". D3D12 validates these fields even when `StencilEnable`
/// is false, and zero is not a legal `D3D12_STENCIL_OP`.
const DISABLED_STENCIL_FACE: D3D12_DEPTH_STENCILOP_DESC = D3D12_DEPTH_STENCILOP_DESC {
    StencilFailOp: D3D12_STENCIL_OP_KEEP,
    StencilDepthFailOp: D3D12_STENCIL_OP_KEEP,
    StencilPassOp: D3D12_STENCIL_OP_KEEP,
    StencilFunc: D3D12_COMPARISON_FUNC_ALWAYS,
};

/// Bytes per 4x4 block for the block-compressed formats: 8 for BC1, 16 for BC2/BC3. The same
/// statement as `PixelFormatDesc`'s 4 and 8 bits per pixel.
fn bytes_per_block(format: DXGI_FORMAT) -> usize {
    if format == DXGI_FORMAT_BC1_UNORM {
        8
    } else {
        16
    }
}

/// The D3D semantic name for one vertex element, as a `'static` C string.
///
/// The corresponding D3D names are `POSITION`, `NORMAL`, `COLOR`, and `TEXCOORD`.
const fn semantic_name(e: VertexElement) -> &'static CStr {
    match e {
        VertexElement::Position | VertexElement::PositionTransformed => c"POSITION",
        VertexElement::Normal => c"NORMAL",
        VertexElement::Diffuse => c"COLOR",
        VertexElement::TexCoord(_) => c"TEXCOORD",
    }
}

fn heap_props(kind: D3D12_HEAP_TYPE) -> D3D12_HEAP_PROPERTIES {
    D3D12_HEAP_PROPERTIES {
        Type: kind,
        ..Default::default()
    }
}

fn default_heap() -> D3D12_HEAP_PROPERTIES {
    heap_props(D3D12_HEAP_TYPE_DEFAULT)
}

/// Translate `D3DBLEND` to `D3D12_BLEND` by name. The names map one-to-one, but the numeric values
/// differ.
fn to_d3d12_blend(b: Blend) -> D3D12_BLEND {
    match b {
        Blend::Zero => D3D12_BLEND_ZERO,
        Blend::One => D3D12_BLEND_ONE,
        Blend::SrcColor => D3D12_BLEND_SRC_COLOR,
        Blend::InvSrcColor => D3D12_BLEND_INV_SRC_COLOR,
        Blend::SrcAlpha => D3D12_BLEND_SRC_ALPHA,
        Blend::InvSrcAlpha => D3D12_BLEND_INV_SRC_ALPHA,
        Blend::DestAlpha => D3D12_BLEND_DEST_ALPHA,
        Blend::InvDestAlpha => D3D12_BLEND_INV_DEST_ALPHA,
        Blend::DestColor => D3D12_BLEND_DEST_COLOR,
        Blend::InvDestColor => D3D12_BLEND_INV_DEST_COLOR,
    }
}

/// The same mapping for the **alpha** blend slots.
///
/// D3D9 with `SEPARATEALPHABLENDENABLE` off applies the one blend factor to both channels, and for a
/// colour factor the alpha channel uses that factor's alpha. D3D12 refuses a colour-manipulating
/// factor in `SrcBlendAlpha`/`DestBlendAlpha` at PSO creation ("is trying to use a D3D11_BLEND value
/// that manipulates color, which is invalid"), so each colour factor is replaced by its alpha
/// analogue. This is unobservable in the back buffer -- `COLORWRITEENABLE = 7` masks the alpha
/// channel -- but the state object still has to be legal.
///
/// It is what makes row 15 of the catalogue (`DESTCOLOR` / `INVSRCALPHA`, the building and env-cell
/// detail pass) constructible at all.
fn to_d3d12_blend_alpha(b: Blend) -> D3D12_BLEND {
    to_d3d12_blend(b.alpha_factor())
}

fn to_d3d12_compare(z: ZFunc) -> D3D12_COMPARISON_FUNC {
    match z {
        ZFunc::Never => D3D12_COMPARISON_FUNC_NEVER,
        ZFunc::Less => D3D12_COMPARISON_FUNC_LESS,
        ZFunc::Equal => D3D12_COMPARISON_FUNC_EQUAL,
        ZFunc::LessEqual => D3D12_COMPARISON_FUNC_LESS_EQUAL,
        ZFunc::Greater => D3D12_COMPARISON_FUNC_GREATER,
        ZFunc::NotEqual => D3D12_COMPARISON_FUNC_NOT_EQUAL,
        ZFunc::GreaterEqual => D3D12_COMPARISON_FUNC_GREATER_EQUAL,
        ZFunc::Always => D3D12_COMPARISON_FUNC_ALWAYS,
    }
}

fn blob_text(b: &ID3DBlob) -> String {
    // SAFETY: an ID3DBlob's buffer is valid for the blob's lifetime, which spans this call.
    unsafe {
        let s = std::slice::from_raw_parts(b.GetBufferPointer().cast::<u8>(), b.GetBufferSize());
        String::from_utf8_lossy(s).into_owned()
    }
}

/// The HLSL source, compiled at device-creation time.
const SHADER_SOURCE: &str = include_str!("shaders/legacy.hlsl");

/// The compiled shader set: one vertex-shader permutation per FVF code, and the five pixel shaders.
type ShaderSet = (
    HashMap<VertexFormat, Vec<u8>>,
    HashMap<PixelShader, Vec<u8>>,
);

/// Compile the shader set.
///
/// **A deviation worth stating.** The client has two vertex shaders, one per FVF family.
/// D3D12 requires every input the vertex shader declares to be present in the input layout, so one
/// vertex shader cannot serve both a layout with a `NORMAL` and one without. There are two vertex
/// *shaders* in the source — the world-space one and the pre-transformed one — compiled into five
/// permutations, one per FVF code. The pixel shaders really are five.
fn compile_shaders() -> Result<ShaderSet, RenderError> {
    let mut vs = HashMap::new();
    for f in VertexFormat::all() {
        let has_normal = matches!(
            f,
            VertexFormat::XyzNormalDiffuseTex1 | VertexFormat::XyzNormalDiffuseTex2
        );
        let two_uv = f.tex_coord_sets() == 2;
        let one: &CStr = c"1";
        let zero: &CStr = c"0";
        let defines = [
            (c"HAS_NORMAL", if has_normal { one } else { zero }),
            (c"HAS_TEXCOORD1", if two_uv { one } else { zero }),
            (
                c"PRETRANSFORMED",
                if f.is_pre_transformed() { one } else { zero },
            ),
        ];
        vs.insert(f, compile(&defines, c"vs_main", c"vs_5_0")?);
    }
    let mut ps = HashMap::new();
    for s in PixelShader::all() {
        ps.insert(s, compile(&[], s.entry_point_c(), c"ps_5_0")?);
    }
    Ok((vs, ps))
}

fn compile(
    defines: &[(&'static CStr, &'static CStr)],
    entry: &'static CStr,
    target: &'static CStr,
) -> Result<Vec<u8>, RenderError> {
    compile_source(SHADER_SOURCE.as_bytes(), defines, entry, target)
}

fn compile_source(
    source: &[u8],
    defines: &[(&'static CStr, &'static CStr)],
    entry: &'static CStr,
    target: &'static CStr,
) -> Result<Vec<u8>, RenderError> {
    // Every string here is a `'static` C string literal, so the D3D_SHADER_MACRO array can point
    // straight at them and the list needs only its NULL terminator appended.
    let mut macros: Vec<D3D_SHADER_MACRO> = defines
        .iter()
        .map(|(n, v)| D3D_SHADER_MACRO {
            Name: PCSTR(n.as_ptr().cast()),
            Definition: PCSTR(v.as_ptr().cast()),
        })
        .collect();
    macros.push(D3D_SHADER_MACRO {
        Name: PCSTR::null(),
        Definition: PCSTR::null(),
    });

    let (entry_c, target_c) = (entry, target);
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: the source slice, the macro array and the two CStrings all outlive the call; the two
    // out-parameters are read only afterwards.
    let r = unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            None,
            Some(macros.as_ptr()),
            None,
            PCSTR(entry_c.as_ptr().cast()),
            PCSTR(target_c.as_ptr().cast()),
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(e) = r {
        let msg = errors.map_or_else(String::new, |b| blob_text(&b));
        let name = entry.to_string_lossy();
        return Err(RenderError::Device(format!(
            "D3DCompile({name}): {e}\n{msg}"
        )));
    }
    let code = code.ok_or_else(|| RenderError::Device("no shader bytecode".into()))?;
    // SAFETY: the blob outlives this slice, which is copied immediately.
    Ok(unsafe {
        std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize())
            .to_vec()
    })
}

#[cfg(test)]
thread_local! {
    /// Set by the one test instrument that needs a slow rasteriser to hold a timing window open.
    static REQUIRE_SOFTWARE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the calling test asked for the software rasteriser explicitly. Test devices otherwise
/// prefer the hardware GPU; outside tests this is always false.
fn test_requires_software() -> bool {
    #[cfg(test)]
    {
        REQUIRE_SOFTWARE.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    include!("d3d12/mipgen_tests.rs");
    include!("d3d12/sampler_tests.rs");

    /// A D3D12 device for these tests: the hardware adapter, or WARP on a machine without one (the
    /// adapter ladder decides). D3D12 is present on every supported Windows, so failing to make a
    /// device is a failure, never a skip.
    fn warp(width: u32, height: u32) -> Option<Gpu> {
        let cfg = DeviceConfig {
            width,
            height,
            ..DeviceConfig::default()
        };
        Some(Gpu::new(None, &cfg).expect("a D3D12 device (hardware, or WARP where there is none)"))
    }

    /// A 1x1 opaque-white texture, so `ps_modulate`'s `t * i.color` is the vertex colour.
    fn white() -> TextureData {
        TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0xFFu8; 4]],
        }
    }

    /// Identity everywhere: with `view_proj` and `world` both identity, a `0x142` vertex's position
    /// *is* its clip-space position, so a test can name device depths directly.
    fn identity_frame() -> PerFrameConstants {
        PerFrameConstants {
            view_proj: hlsl_matrix(glam::Mat4::IDENTITY),
            view: hlsl_matrix(glam::Mat4::IDENTITY),
            // Fog end 1, fog disabled.
            fog_params: [0.0, 1.0, 0.0, 0.0],
            ..PerFrameConstants::default()
        }
    }

    /// Two triangles covering `[x0, x1] x [y0, y1]` of clip space at depth `z`, FVF `0x142`.
    fn quad_142(x0: f32, y0: f32, x1: f32, y1: f32, z: f32, argb: u32) -> Vec<u8> {
        let mut v = Vec::new();
        let mut push = |x: f32, y: f32| {
            for f in [x, y, z] {
                v.extend_from_slice(&f.to_le_bytes());
            }
            v.extend_from_slice(&argb.to_le_bytes());
            v.extend_from_slice(&0.0f32.to_le_bytes());
            v.extend_from_slice(&0.0f32.to_le_bytes());
        };
        for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y0), (x1, y1), (x0, y1)] {
            push(x, y);
        }
        v
    }

    /// The opaque state every catalogue row 1 draw uses: `One/Zero`, no blend, no alpha test,
    /// depth `LESS` with the write on.
    fn opaque_key() -> PipelineKey {
        PipelineKey {
            vertex_format: VertexFormat::XyzDiffuseTex1,
            src_blend: Blend::One,
            dst_blend: Blend::Zero,
            alpha_blend: false,
            alpha_test: false,
            z_write: true,
            z_func: ZFunc::Less,
            cull: Cull::None,
            stage_ops: crate::StageOps::BASE,
            fog: false,
            lighting: false,
        }
    }

    /// Oracle: the portal depth stamp runs with a mask of 7 —
    /// depth test `DEPTHTEST_ALWAYS` with depth writes on and, from bit 0, the stamped depth is the
    /// constant `0.999999`. This resets the depth *inside each visible opening* to the far plane,
    /// undoing what the landscape already wrote behind the building, with `DEPTHTEST_ALWAYS`,
    /// z-write on and no colour.
    ///
    /// This is the mechanism on its own, with a depth buffer the test controls rather than one a
    /// landscape happens to produce:
    ///
    /// 1. a **near** red surface fills the frame and writes depth 0.2;
    /// 2. the stamp covers the middle third at depth `0.999999`;
    /// 3. a **far** green surface at depth 0.9 is drawn with `LESS`.
    ///
    /// The green must appear exactly where the stamp went and nowhere else. Without step 2 the
    /// green fails the depth test everywhere and the frame stays red — which is the control the
    /// second half of this test runs, so "delete the stamp and it goes red" is a thing this
    /// assertion does rather than a thing a person has to remember to check.
    #[test]
    fn the_portal_stamp_resets_the_depth_inside_its_polygon_and_nowhere_else() {
        const N: u32 = 64;
        let Some(mut gpu) = warp(N, N) else { return };
        let tex = gpu.upload_texture(&white()).expect("upload");
        let per_frame = identity_frame();
        let draw = PerDrawConstants::identity();
        let red = 0xFFFF_0000u32;
        let green = 0xFF00_FF00u32;
        // The stamped rectangle, in clip space and then in pixels. Clip +y is the top row.
        let (sx0, sy0, sx1, sy1) = (-0.5f32, -0.5f32, 0.5f32, 0.5f32);

        let render = |gpu: &mut Gpu, stamp: bool| -> Vec<u8> {
            gpu.begin_frame().expect("begin");
            gpu.bind_texture(tex, 0);
            gpu.draw_dynamic(
                &opaque_key(),
                &crate::DrawConstants::default(),
                &per_frame,
                &draw,
                &quad_142(-1.0, -1.0, 1.0, 1.0, 0.2, red),
            )
            .expect("the near surface");
            if stamp {
                let poly: Vec<[f32; 4]> = [(sx0, sy0), (sx1, sy0), (sx1, sy1), (sx0, sy1)]
                    .into_iter()
                    .map(|(x, y)| [x, y, 0.5, 1.0])
                    .collect();
                gpu.draw_portal_poly(&per_frame, &poly, crate::pso::portal_stamp_mask::BUILDING)
                    .expect("the stamp");
            }
            gpu.bind_texture(tex, 0);
            gpu.draw_dynamic(
                &opaque_key(),
                &crate::DrawConstants::default(),
                &per_frame,
                &draw,
                &quad_142(-1.0, -1.0, 1.0, 1.0, 0.9, green),
            )
            .expect("the far surface");
            gpu.end_frame().expect("end");
            gpu.capture().expect("capture").to_rgba()
        };

        let control = render(&mut gpu, false);
        assert_eq!(gpu.portal_stamps, 0);
        let stamped = render(&mut gpu, true);
        assert_eq!(gpu.portal_stamps, 1, "one portal-polygon draw was issued");

        for (i, px) in control.as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(
                (px[0], px[1], px[2]),
                (255, 0, 0),
                "pixel {i} of the control is {px:?}, not the near surface"
            );
        }

        // The subject: green inside the stamped rectangle, red outside it.
        #[allow(clippy::cast_precision_loss)] // a 64-pixel extent
        let fn_ = N as f32;
        let (mut inside_green, mut outside_red) = (0u32, 0u32);
        for (i, px) in stamped.as_chunks::<4>().0.iter().enumerate() {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            // LINT-OK: a pixel index into a 64x64 buffer, back to clip space.
            let (cx, cy) = (
                ((i as u32 % N) as f32 + 0.5) / fn_ * 2.0 - 1.0,
                1.0 - ((i as u32 / N) as f32 + 0.5) / fn_ * 2.0,
            );
            let inside = cx > sx0 && cx < sx1 && cy > sy0 && cy < sy1;
            if inside {
                assert_eq!(
                    (px[0], px[1], px[2]),
                    (0, 255, 0),
                    "pixel {i} is inside the stamp and is {px:?}: the far surface did not get through"
                );
                inside_green += 1;
            } else {
                assert_eq!(
                    (px[0], px[1], px[2]),
                    (255, 0, 0),
                    "pixel {i} is outside the stamp and is {px:?}: the stamp cleared depth it does not own"
                );
                outside_red += 1;
            }
        }
        assert_eq!(
            inside_green,
            N * N / 4,
            "the stamp covers a quarter of the frame"
        );
        assert_eq!(inside_green + outside_red, N * N);
        gpu.release_texture(tex);
        gpu.wait_idle().expect("idle");
    }

    /// Oracle: the same function's vertex loop — bit 1 of the mask forces the vertex alpha to 0,
    /// and both shipped masks set it, so `SRCALPHA/INVSRCALPHA` leaves the render target exactly as
    /// it found it. The stamp writes **depth only**.
    ///
    /// Asserted by stamping over a frame with nothing else drawn after it: the capture must be
    /// byte-identical to one that never stamped at all. A stamp that wrote its debug hue would
    /// paint a coloured quad over a quarter of the screen and this would fail on every pixel of it.
    #[test]
    fn the_portal_stamp_writes_no_colour() {
        const N: u32 = 64;
        let Some(mut gpu) = warp(N, N) else { return };
        let tex = gpu.upload_texture(&white()).expect("upload");
        let per_frame = identity_frame();
        let draw = PerDrawConstants::identity();

        let render = |gpu: &mut Gpu, stamp: bool| -> Vec<u8> {
            gpu.begin_frame().expect("begin");
            gpu.bind_texture(tex, 0);
            gpu.draw_dynamic(
                &opaque_key(),
                &crate::DrawConstants::default(),
                &per_frame,
                &draw,
                &quad_142(-1.0, -1.0, 1.0, 1.0, 0.2, 0xFF33_66AA),
            )
            .expect("a surface");
            if stamp {
                let poly: Vec<[f32; 4]> = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]
                    .into_iter()
                    .map(|(x, y): (f32, f32)| [x, y, 0.5, 1.0])
                    .collect();
                gpu.draw_portal_poly(&per_frame, &poly, crate::pso::portal_stamp_mask::BUILDING)
                    .expect("the stamp");
            }
            gpu.end_frame().expect("end");
            gpu.capture().expect("capture").to_rgba()
        };

        let plain = render(&mut gpu, false);
        let stamped = render(&mut gpu, true);
        assert_eq!(plain, stamped, "the stamp put colour in the frame buffer");
        gpu.release_texture(tex);
        gpu.wait_idle().expect("idle");
    }

    /// Oracle: the portal-polygon draw draws nothing when the clipper left
    /// fewer than three vertices. A polygon that clipped away is not a degenerate triangle to be
    /// rasterised; it is not drawn at all, and the counter must say so.
    #[test]
    fn a_portal_polygon_below_three_vertices_is_not_stamped() {
        let Some(mut gpu) = warp(64, 64) else { return };
        gpu.begin_frame().expect("begin");
        for n in 0..3usize {
            let poly: Vec<[f32; 4]> = (0..n).map(|i| [i as f32 * 0.1, 0.0, 0.5, 1.0]).collect();
            gpu.draw_portal_poly(
                &identity_frame(),
                &poly,
                crate::pso::portal_stamp_mask::BUILDING,
            )
            .expect("no draw, no error");
        }
        assert_eq!(
            gpu.portal_stamps, 0,
            "a sub-triangle polygon must not reach the device"
        );
        gpu.end_frame().expect("end");
        gpu.wait_idle().expect("idle");
    }

    /// A 4x4 BGRA texture with one level. Small on purpose: these tests are about descriptor
    /// bookkeeping, not about pixels, and every upload costs a `wait_idle`.
    fn tiny_texture() -> TextureData {
        TextureData {
            width: 4,
            height: 4,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0x7Fu8; 4 * 4 * 4]],
        }
    }

    // Oracle: the stated requirement -- "a create/release/create cycle reuses the slot rather than
    // advancing the counter" -- against the real device rather than the pure allocator, so that the
    // wiring (fence value, retirement point, resource lifetime) is proved and not just the algebra.
    #[test]
    fn a_texture_released_and_re_uploaded_gets_its_descriptor_pair_back() {
        let Some(mut gpu) = warp(64, 64) else { return };
        let t = tiny_texture();
        let first = gpu.upload_texture(&t).expect("upload");
        assert_eq!(gpu.descriptor_usage().frontier, 1);
        assert_eq!(gpu.live_textures(), 1);
        assert_eq!(gpu.release_texture(first), Released::Freed);
        assert_eq!(gpu.live_textures(), 0);
        // Released, but not yet reusable: it is parked behind the fence the next signal carries.
        assert_eq!(gpu.descriptor_usage().pending, 1);
        assert_eq!(gpu.descriptor_usage().free, 0);
        // The next upload's own `wait_idle` advances that fence, so the slot comes back.
        // Uploads do not wait for the device, so a released slot comes back once the device has
        // passed the release; in the running client every frame does that. This wait stands in.
        gpu.wait_idle().expect("idle");
        let second = gpu.upload_texture(&t).expect("upload");
        assert_eq!(second, first, "the same descriptor pair");
        let usage = gpu.descriptor_usage();
        assert_eq!(usage.frontier, 1, "the heap did not grow");
        assert_eq!(usage.live, 1);
        assert_eq!(gpu.descriptor_stats().reuses, 1);
        gpu.release_texture(second);
        gpu.wait_idle().expect("idle");
    }

    // Oracle: the stated requirement -- "N releases followed by N uploads leaves the high-water mark
    // at N, not 2N". N = 32 rather than 2,048 because each upload runs a real command list and a
    // `wait_idle`; the ceiling itself is proved at the algebraic level in `crate::descriptor`, and
    // what this adds is that the device wiring reuses at all.
    #[test]
    fn n_device_releases_then_n_uploads_leave_the_heap_high_water_at_n() {
        const N: usize = 32;
        let Some(mut gpu) = warp(64, 64) else { return };
        let t = tiny_texture();
        let first: Vec<TextureSlot> = (0..N)
            .map(|_| gpu.upload_texture(&t).expect("upload"))
            .collect();
        assert_eq!(gpu.descriptor_usage().frontier, N as u32);
        for s in &first {
            assert_eq!(gpu.release_texture(*s), Released::Freed);
        }
        assert_eq!(gpu.live_textures(), 0);
        // Uploads do not wait for the device, so a released slot comes back once the device has
        // passed the release; in the running client every frame does that. This wait stands in.
        gpu.wait_idle().expect("idle");
        let second: Vec<TextureSlot> = (0..N)
            .map(|_| gpu.upload_texture(&t).expect("upload"))
            .collect();
        let usage = gpu.descriptor_usage();
        assert_eq!(
            usage.frontier, N as u32,
            "2N would be the unbounded behaviour this replaces"
        );
        assert_eq!(usage.high_water, N as u32);
        assert_eq!(usage.live, N as u32);
        assert_eq!(gpu.descriptor_stats().reuses, N as u64);
        let mut a: Vec<u32> = first.iter().map(|s| s.0).collect();
        let mut b: Vec<u32> = second.iter().map(|s| s.0).collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "the same set of slots, not a fresh run of them");
        for s in second {
            gpu.release_texture(s);
        }
        gpu.wait_idle().expect("idle");
    }

    // Oracle: the stated requirement -- "a slot released this frame is not handed out again until
    // the fence has passed, which is the property that would otherwise be a use-after-free". This is
    // the device half of the assertion `crate::descriptor` makes algebraically, and it covers the
    // case the algebra cannot see: `upload_texture` is legal mid-frame and runs a `wait_idle` of its
    // own, so a release made inside an open frame must NOT be governed by the value that
    // `wait_idle` completes -- the frame's own command list has not been submitted yet, and it may
    // already have bound the descriptor. Retiring against the intervening `wait_idle` would free the
    // texture out from under a list that is about to run. Found by inspection while wiring this up;
    // the first cut had exactly that bug.
    #[test]
    fn a_slot_released_inside_a_frame_survives_an_intervening_wait_idle() {
        let Some(mut gpu) = warp(64, 64) else { return };
        let t = tiny_texture();
        let slot = gpu.upload_texture(&t).expect("upload");
        gpu.begin_frame().expect("begin");
        // The open command list binds the descriptor; the GPU may read it as soon as the frame is
        // submitted, which has not happened yet.
        gpu.bind_texture(slot, 0);
        assert_eq!(gpu.release_texture(slot), Released::Freed);
        let usage = gpu.descriptor_usage();
        assert_eq!(
            usage.deferred, 1,
            "held until the frame that may read it is submitted"
        );
        assert_eq!(usage.live, 1, "and still occupying budget until then");
        assert_eq!(usage.pending, 0);
        assert_eq!(usage.free, 0);
        // A mid-frame upload: its own `wait_idle` signals and completes a fence value while the open
        // list still sits unsubmitted. The released slot must survive it.
        let other = gpu.upload_texture(&t).expect("a mid-frame upload is legal");
        assert_ne!(
            other, slot,
            "the released slot must not be reissued to this upload"
        );
        assert_eq!(
            gpu.descriptor_usage().free,
            0,
            "reissuing here is the use-after-free"
        );
        assert_eq!(gpu.descriptor_usage().deferred, 1, "still held");
        gpu.end_frame().expect("end");
        // Now submitted, and only now on the clock.
        let usage = gpu.descriptor_usage();
        assert_eq!(usage.deferred, 0);
        assert_eq!(usage.pending + usage.free, 1);
        gpu.wait_idle().expect("idle");
        gpu.begin_frame().expect("begin");
        let usage = gpu.descriptor_usage();
        assert_eq!(usage.pending, 0, "the fence has passed");
        assert_eq!(usage.free, 1);
        assert_eq!(gpu.descriptor_stats().retired, 1);
        assert!(gpu.frame_open(), "inside the frame this test just began");
        gpu.end_frame().expect("end");
        assert!(!gpu.frame_open());
        gpu.release_texture(other);
        gpu.wait_idle().expect("idle");
    }

    // Oracle: the D3D12 runtime itself, which refuses `ID3D12CommandAllocator::Reset` while a list
    // recorded from it is still open. This is why a release held inside a frame needs no recovery
    // path: a frame cannot be silently abandoned and begun again -- the runtime rejects it, loudly,
    // and the held release is still there for the eventual `end_frame`. `DescriptorUsage::deferred`
    // is the number that says so; a non-zero one outside a frame is the visible symptom.
    #[test]
    fn a_frame_cannot_be_abandoned_so_a_held_release_is_never_stranded_silently() {
        let Some(mut gpu) = warp(64, 64) else { return };
        let t = tiny_texture();
        let slot = gpu.upload_texture(&t).expect("upload");
        gpu.begin_frame().expect("begin");
        assert_eq!(gpu.release_texture(slot), Released::Freed);
        assert_eq!(gpu.descriptor_usage().deferred, 1);
        // A second begin_frame with no end_frame is not a thing that quietly happens.
        let e = gpu
            .begin_frame()
            .expect_err("the runtime refuses to reset a live allocator");
        assert!(format!("{e}").contains("Reset(allocator)"), "{e}");
        assert!(
            gpu.frame_open(),
            "the frame is still the one that was begun"
        );
        assert_eq!(
            gpu.descriptor_usage().deferred,
            1,
            "and the release is still held, not lost"
        );
        // Ending that frame still puts it on the clock.
        gpu.end_frame().expect("end");
        assert_eq!(gpu.descriptor_usage().deferred, 0);
        gpu.wait_idle().expect("idle");
        gpu.begin_frame().expect("begin");
        assert_eq!(gpu.descriptor_usage().free, 1, "and it came back");
        gpu.end_frame().expect("end");
        gpu.wait_idle().expect("idle");
    }

    // Oracle: the stated requirement -- "heap exhaustion still fails cleanly and countably rather
    // than corrupting." The budget is narrowed to four slots so the path is reached without 2,048
    // real uploads; the heap itself is still the full 4,096 descriptors, so nothing here depends on
    // the narrowing being anything but an earlier refusal.
    #[test]
    fn a_full_descriptor_heap_fails_cleanly_and_counts_the_refusal() {
        let Some(mut gpu) = warp(64, 64) else { return };
        gpu.narrow_descriptor_budget(4);
        let t = tiny_texture();
        let held: Vec<TextureSlot> = (0..4)
            .map(|_| gpu.upload_texture(&t).expect("upload"))
            .collect();
        for i in 0..3 {
            let e = gpu.upload_texture(&t).expect_err("the budget is full");
            assert!(matches!(e, RenderError::Device(_)), "{e}");
            assert!(format!("{e}").contains("descriptor heap exhausted"), "{e}");
            assert_eq!(
                gpu.descriptor_stats().exhaustions,
                i + 1,
                "every refusal is counted"
            );
        }
        // Nothing was corrupted by the failures: the four live textures are untouched and the
        // device still works.
        assert_eq!(gpu.live_textures(), 4);
        assert_eq!(gpu.descriptor_usage().live, 4);
        gpu.begin_frame().expect("the device still renders");
        for s in &held {
            gpu.bind_texture(*s, 0);
        }
        gpu.end_frame().expect("end");
        // And releasing one makes room again, so the failure was a budget, not a wedge.
        assert_eq!(gpu.release_texture(held[0]), Released::Freed);
        let again = gpu.upload_texture(&t).expect("a released slot makes room");
        assert_eq!(again, held[0]);
        assert_eq!(
            gpu.descriptor_stats().exhaustions,
            3,
            "and no further refusals"
        );
        for s in &held[1..] {
            gpu.release_texture(*s);
        }
        gpu.release_texture(again);
        gpu.wait_idle().expect("idle");
    }

    // The combined-texture cache returns and retains an existing nonzero-key entry:
    // "if (key != 0 and texture_table[key]
    // exists) { AddRef; return it }". The device-level consequence is that the second request costs
    // no descriptor pair at all.
    #[test]
    fn a_keyed_upload_of_a_cached_texture_costs_no_descriptors() {
        let Some(mut gpu) = warp(64, 64) else { return };
        let t = tiny_texture();
        let key = TextureKey::world(crate::descriptor::combined_texture_key(
            0x0400_0001,
            0x0600_0002,
        ));
        let a = gpu.upload_texture_keyed(key, &t).expect("upload");
        let b = gpu.upload_texture_keyed(key, &t).expect("cache hit");
        assert_eq!(a, b);
        assert_eq!(gpu.descriptor_usage().frontier, 1, "one texture, one pair");
        assert_eq!(gpu.texture_table_stats().hits, 1);
        assert_eq!(gpu.live_textures(), 1);
        // Two links, so the first release does not free it.
        assert_eq!(gpu.release_texture(a), Released::StillLinked(1));
        assert_eq!(gpu.descriptor_usage().live, 1);
        assert_eq!(gpu.release_texture(b), Released::Freed);
        assert_eq!(gpu.descriptor_usage().live, 0);
        // An uncached upload of the same bits is a second texture, as the client's key-0 path is.
        let c = gpu.upload_texture(&t).expect("upload");
        let d = gpu.upload_texture(&t).expect("upload");
        assert_ne!(c, d, "custom_texture_table entries are never shared");
        assert_eq!(
            gpu.texture_table_stats().hits,
            1,
            "and neither was a cache hit"
        );
        gpu.release_texture(c);
        gpu.release_texture(d);
        gpu.wait_idle().expect("idle");
    }

    // Oracle: "If you make something tolerant of failure, give it a counter and assert on that
    // counter." Releasing a slot twice is a caller bug that must not free the descriptor pair twice
    // -- that would hand the same descriptors to two owners -- and must not panic in a teardown.
    #[test]
    fn a_double_release_at_the_device_is_counted_and_frees_the_pair_once() {
        let Some(mut gpu) = warp(64, 64) else { return };
        let t = tiny_texture();
        let slot = gpu.upload_texture(&t).expect("upload");
        assert_eq!(gpu.release_texture(slot), Released::Freed);
        assert_eq!(gpu.release_texture(slot), Released::Unknown);
        assert_eq!(gpu.release_texture(TextureSlot(9998)), Released::Unknown);
        assert_eq!(gpu.texture_table_stats().unknown_releases, 2);
        assert_eq!(gpu.texture_table_stats().frees, 1);
        gpu.wait_idle().expect("idle");
        gpu.begin_frame().expect("begin");
        gpu.end_frame().expect("end");
        assert_eq!(
            gpu.descriptor_stats().retired,
            1,
            "one slot came back, not two"
        );
        assert_eq!(
            gpu.descriptor_stats().invalid_releases,
            0,
            "and the allocator saw one release"
        );
        gpu.wait_idle().expect("idle");
    }

    /// A long session of screens stays bounded by what is resident.
    #[test]
    fn a_long_session_of_screens_stays_bounded_by_what_is_resident() {
        let Some(mut gpu) = warp(64, 64) else { return };
        let t = tiny_texture();
        // Twelve screens of 16 images each: 192 uploads, one screen resident at a time.
        let mut resident: Vec<TextureSlot> = Vec::new();
        for _ in 0..12 {
            for s in resident.drain(..) {
                assert_eq!(gpu.release_texture(s), Released::Freed);
            }
            // Uploads do not wait for the device, so a released slot comes back once the device has
            // passed the release; in the running client every frame does that. This wait stands in.
            gpu.wait_idle().expect("idle");
            for _ in 0..16 {
                resident.push(
                    gpu.upload_texture(&t)
                        .expect("a screen must not exhaust the heap"),
                );
            }
        }
        let usage = gpu.descriptor_usage();
        assert_eq!(usage.live, 16);
        assert_eq!(usage.high_water, 16);
        assert_eq!(
            usage.frontier, 16,
            "192 uploads across 12 screens used {} slots; the monotonic version used 192",
            usage.frontier
        );
        assert_eq!(gpu.descriptor_stats().allocations, 192);
        assert_eq!(gpu.descriptor_stats().reuses, 176);
        assert_eq!(gpu.descriptor_stats().exhaustions, 0);
        for s in resident.drain(..) {
            gpu.release_texture(s);
        }
        gpu.wait_idle().expect("idle");
    }

    /// Every catalogue row constructs a pipeline state D3D12 accepts.
    #[test]
    fn every_catalogue_row_constructs_a_pipeline_state_d3d12_accepts() {
        let cfg = DeviceConfig {
            width: 64,
            height: 64,
            debug: true,
            ..DeviceConfig::default()
        };
        let Ok(mut gpu) = Gpu::new(None, &cfg) else {
            eprintln!("skipping: no D3D12 WARP device available");
            return;
        };
        let built = gpu
            .build_whole_catalogue()
            .expect("the runtime must accept every row");
        // 15 rows x 5 FVF families.
        assert_eq!(built, 15 * 5);
    }

    /// A capture waits for the slot it resets rather than resetting under the GPU.
    #[test]
    fn a_capture_waits_for_the_slot_it_resets_rather_than_resetting_under_the_gpu() {
        // Enough full-frame fills per frame that WARP is still rasterising frame 0 when the CPU
        // has submitted frame 2 and moved on: 400 fills of 800x600 on the host this was measured
        // on. The CPU's cost is the recording (the debug layer validates every draw), the GPU's is
        // the pixels, so on a host whose WARP outruns that the window is closed and the instrument
        // blind: the calibration below then retries on a larger target, the same draws with four
        // times the pixels, and fails only when even the largest cannot hold the window open.
        const HEAVY: usize = 400;
        const TARGETS: [(u32, u32); 3] = [(800, 600), (1600, 1200), (3200, 2400)];
        // The window this instrument watches stays open only while the GPU is slower than the
        // recording, so it asks for WARP explicitly; a hardware GPU closes it at once.
        REQUIRE_SOFTWARE.with(|r| r.set(true));
        let mut calibration = String::new();
        for (width, height) in TARGETS {
            let cfg = DeviceConfig {
                width,
                height,
                debug: true,
                ..DeviceConfig::default()
            };
            let Ok(mut gpu) = Gpu::new(None, &cfg) else {
                eprintln!("skipping: no D3D12 WARP device available");
                return;
            };
            // The instrument. `ID3D12InfoQueue` exists only when the SDK layers are installed and
            // the debug layer came up; without it this test cannot look, and says so rather than
            // passing.
            if gpu.device.cast::<ID3D12InfoQueue>().is_err() {
                eprintln!(
                    "skipping: the D3D12 debug layer is not installed, so COMMAND_ALLOCATOR_SYNC \
                     cannot be observed"
                );
                return;
            }
            let tex = gpu.upload_texture(&white()).expect("upload");
            let per_frame = identity_frame();
            let draw = PerDrawConstants::identity();
            let _ = gpu.debug_messages();
            for _ in 0..FRAME_COUNT {
                gpu.begin_frame().expect("begin");
                gpu.bind_texture(tex, 0);
                for _ in 0..HEAVY {
                    gpu.draw_dynamic(
                        &opaque_key(),
                        &crate::DrawConstants::default(),
                        &per_frame,
                        &draw,
                        &quad_142(-1.0, -1.0, 1.0, 1.0, 0.5, 0xFF20_40FF),
                    )
                    .expect("draw");
                }
                gpu.end_frame().expect("end");
            }
            // Calibration: the slot `capture` is about to reset is frame 0's, and frame 0 must
            // still be in flight for the assertion below to have been able to fail.
            let slot = gpu.frame_index;
            let want = gpu.frames[slot].fence_value;
            // SAFETY: GetCompletedValue is a pure query on the fence.
            let completed = unsafe { gpu.fence.GetCompletedValue() };
            assert!(want != 0, "the ring never advanced");
            if completed >= want {
                calibration = format!(
                    "the instrument could not look: WARP had already retired frame 0 (fence \
                     {completed} >= {want}) before the capture at {width}x{height}"
                );
                gpu.wait_idle().expect("idle");
                continue;
            }
            let image = gpu.capture().expect("capture");
            assert_eq!(image.bgra.len(), width as usize * height as usize * 4);
            let messages = gpu.debug_messages();
            assert!(
                !messages.contains("COMMAND_ALLOCATOR_SYNC"),
                "capture reset a command allocator the GPU was still executing from:{messages}"
            );
            gpu.wait_idle().expect("idle");
            return;
        }
        panic!("{calibration}; raise HEAVY");
    }

    /// A thousand frames cycle the ring without stalling.
    #[test]
    fn a_thousand_frames_cycle_the_ring_without_stalling() {
        let Some(mut gpu) = warp(64, 64) else { return };
        for _ in 0..1000 {
            gpu.begin_frame().expect("begin");
            gpu.end_frame().expect("end");
        }
        assert_eq!(
            gpu.frame_stamp, 1000,
            "the frame stamp is bumped once per frame"
        );
        gpu.wait_idle().expect("idle");
    }

    /// The headless capture is deterministic across runs.
    #[test]
    fn the_headless_capture_is_deterministic_across_runs() {
        let mut captures = Vec::new();
        for _ in 0..3 {
            let Some(mut gpu) = warp(800, 600) else {
                return;
            };
            gpu.begin_frame().expect("begin");
            gpu.end_frame().expect("end");
            captures.push(gpu.capture().expect("capture").to_rgba());
        }
        assert_eq!(captures[0].len(), 800 * 600 * 4);
        assert_eq!(captures[0], captures[1]);
        assert_eq!(captures[1], captures[2]);
        // The clear really did happen: the frame is black and opaque.
        assert!(captures[0]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        assert!(captures[0].as_chunks::<4>().0.iter().all(|p| p[3] == 0xFF));
    }

    // Oracle: `D3DBLEND` names map one-to-one onto `D3D12_BLEND` names, but their numeric values
    // differ, so translate by name.
    #[test]
    fn blend_and_compare_translate_by_name_not_by_value() {
        assert_eq!(to_d3d12_blend(Blend::SrcAlpha), D3D12_BLEND_SRC_ALPHA);
        assert_eq!(
            to_d3d12_blend(Blend::InvSrcAlpha),
            D3D12_BLEND_INV_SRC_ALPHA
        );
        assert_eq!(to_d3d12_blend(Blend::DestColor), D3D12_BLEND_DEST_COLOR);
        // A finding, recorded rather than asserted away: for the ten D3DBLEND values the
        // client actually uses, the numbers happen to coincide with D3D12_BLEND's, so a by-value
        // cast would pass this test today. It would still be wrong -- the enums diverge above
        // D3DBLEND_INVDESTCOLOR (11), where D3D9 has BOTHSRCALPHA and D3D12 has SRC_ALPHA_SAT --
        // so this mapping deliberately uses a match rather than a cast.
        assert_eq!(to_d3d12_blend(Blend::Zero), D3D12_BLEND_ZERO);
        assert_eq!(to_d3d12_blend(Blend::One), D3D12_BLEND_ONE);
        assert_eq!(
            to_d3d12_blend(Blend::InvDestColor),
            D3D12_BLEND_INV_DEST_COLOR
        );
        assert_eq!(to_d3d12_compare(ZFunc::Less), D3D12_COMPARISON_FUNC_LESS);
        assert_eq!(
            to_d3d12_compare(ZFunc::LessEqual),
            D3D12_COMPARISON_FUNC_LESS_EQUAL
        );
        assert_eq!(
            to_d3d12_compare(ZFunc::Always),
            D3D12_COMPARISON_FUNC_ALWAYS
        );
    }

    // Oracle: the D3D12 runtime's own validation, which rejects a colour-manipulating factor in the
    // alpha blend slots -- discovered by the debug layer while constructing catalogue row 15
    // (DESTCOLOR / INVSRCALPHA, the building and env-cell detail pass). D3D9 had no such rule, so
    // the alpha analogue of each colour factor is what preserves the intent.
    #[test]
    fn colour_blend_factors_become_their_alpha_analogues_in_the_alpha_slots() {
        assert_eq!(
            to_d3d12_blend_alpha(Blend::DestColor),
            D3D12_BLEND_DEST_ALPHA
        );
        assert_eq!(
            to_d3d12_blend_alpha(Blend::InvDestColor),
            D3D12_BLEND_INV_DEST_ALPHA
        );
        assert_eq!(to_d3d12_blend_alpha(Blend::SrcColor), D3D12_BLEND_SRC_ALPHA);
        assert_eq!(
            to_d3d12_blend_alpha(Blend::InvSrcColor),
            D3D12_BLEND_INV_SRC_ALPHA
        );
        // Everything else is untouched, including the factors the legacy path actually uses most.
        for b in [
            Blend::Zero,
            Blend::One,
            Blend::SrcAlpha,
            Blend::InvSrcAlpha,
            Blend::DestAlpha,
        ] {
            assert_eq!(to_d3d12_blend_alpha(b), to_d3d12_blend(b), "{b:?}");
        }
    }

    // Oracle: matching the client's disabled sRGB writes requires a non-sRGB back buffer, and its
    // X8R8G8B8 choice has B, G, R, A memory order.
    #[test]
    fn the_back_buffer_format_is_non_srgb_and_bgra_ordered() {
        assert_eq!(BACK_BUFFER_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM);
        assert_ne!(BACK_BUFFER_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB);
        assert_eq!(
            DEPTH_FORMAT, DXGI_FORMAT_D24_UNORM_S8_UINT,
            "D24S8 is the client's first choice"
        );
    }

    // Oracle: the gamma/brightness value is clamped to [-0.2, 1.0].
    #[test]
    fn the_gamma_value_is_clamped_the_way_setgamma_clamps_it() {
        let Some(mut gpu) = warp(16, 16) else { return };
        gpu.set_gamma(5.0);
        assert_eq!(gpu.gamma(), 1.0);
        gpu.set_gamma(-5.0);
        assert!((gpu.gamma() - (-0.2)).abs() < 1e-6);
    }

    // Oracle: the retail presentation setup's BackBufferCount = 1 with D3DSWAPEFFECT_DISCARD, and
    // the rebuild note about the dynamic ring. Three frames is the rebuild's choice, stated here so
    // a change is deliberate.
    #[test]
    fn the_frame_ring_is_three_deep() {
        assert_eq!(FRAME_COUNT, 3);
    }
}
