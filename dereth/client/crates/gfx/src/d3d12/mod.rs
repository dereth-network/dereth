//! The layer on Direct3D 12: the same buffers, textures, samplers, table, pipelines and
//! recording as on Vulkan, through `windows`' bindings.
//!
//! - **The table** is shader model 6.6's: the shaders index the descriptor heaps themselves
//!   (`ResourceDescriptorHeap`, `SamplerDescriptorHeap`). The resource heap holds every texture
//!   from 0 and every storage buffer from [`BUFFERS_AT`] as a read view and a write view side by
//!   side, as `bindless.slang` reads them; the sampler heap holds the samplers.
//! - **One root signature** for every pipeline: the push block as 32 root constants at `b0`, and
//!   the draw's item as one root constant at `b1`, which a direct draw sets from its first
//!   instance and an indirect one from its command (6 words: the item, then D3D12's 5).
//! - **Barriers** come from the same declared uses as on Vulkan, as resource state transitions.
//! - **Nothing is compiled here.** Pipelines are made from DXIL compiled and signed when the
//!   shaders are built; no shader compiler is loaded.

use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

use crate::bindless::Indices;
use crate::resource::Slots;
use crate::{Caps, DeviceDesc, Error, Format, State, Stats, Ticket};

mod pipeline;
mod recorder;
mod resource;
mod rt;

pub use recorder::{Draw, Recorder};

/// Where the storage buffers start in the resource heap: `DXIL_BUFFERS` in `bindless.slang`.
pub const BUFFERS_AT: u32 = 65_536;
/// The most textures the table holds.
const MAX_TEXTURES: u32 = BUFFERS_AT;
/// The most storage buffers it holds.
const MAX_BUFFERS: u32 = 65_536;
/// Where the top-level acceleration structures start in the resource heap: `DXIL_ACCELS` in
/// `bindless.slang`.
const ACCELS_AT: u32 = BUFFERS_AT + 2 * MAX_BUFFERS;
/// The colour and depth target views the device keeps.
const RTVS: u32 = 4_096;
const DSVS: u32 = 2_048;

/// A Direct3D 12 device that already exists, to adopt, and the queue its owner submits to.
#[derive(Debug, Clone)]
pub struct D3d12Native {
    /// The device.
    pub device: ID3D12Device,
    /// The direct queue its owner submits to; the layer submits its own work there too.
    pub queue: ID3D12CommandQueue,
    /// Whether to trace rays where the device can.
    pub ray_queries: bool,
}

/// A resource lent to the layer by its owner: a 2D texture of one level.
#[derive(Debug, Clone)]
pub struct D3d12Lent {
    /// The resource.
    pub resource: ID3D12Resource,
    /// Its format.
    pub format: Format,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The state its owner left it in.
    pub state: D3D12_RESOURCE_STATES,
}

/// Every library the process has loaded, by its file: for a check that the layer brings no
/// compiler and no library of its own beside the system's.
#[must_use]
pub fn loaded_libraries() -> Vec<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
    use windows::Win32::System::ProcessStatus::K32EnumProcessModules;
    use windows::Win32::System::Threading::GetCurrentProcess;

    // SAFETY: a pseudo-handle to this process, which needs no closing.
    let process = unsafe { GetCurrentProcess() };
    let mut modules = vec![HMODULE::default(); 512];
    loop {
        let mut needed = 0u32;
        let bytes = u32::try_from(std::mem::size_of_val(modules.as_slice())).unwrap_or(u32::MAX);
        // SAFETY: the buffer is live and `bytes` long; the count is a live local.
        if !unsafe { K32EnumProcessModules(process, modules.as_mut_ptr(), bytes, &mut needed) }
            .as_bool()
        {
            return Vec::new();
        }
        let n = needed as usize / std::mem::size_of::<HMODULE>();
        if n <= modules.len() {
            modules.truncate(n);
            break;
        }
        modules.resize(n, HMODULE::default());
    }
    modules
        .iter()
        .filter_map(|m| {
            let mut name = vec![0u16; 2048];
            // SAFETY: a module of this process and a live buffer.
            let len = unsafe { GetModuleFileNameW(Some(*m), &mut name) } as usize;
            (len > 0).then(|| std::ffi::OsString::from_wide(&name[..len]).into())
        })
        .collect()
}

/// The layer's state for a resource left in Direct3D 12 state `s`, as [`crate::Use::External`]
/// takes it: the resource is handed back in `s`.
#[must_use]
pub fn state(s: D3D12_RESOURCE_STATES) -> State {
    use ash::vk::{AccessFlags2 as A, ImageLayout as L, PipelineStageFlags2 as S};
    let layout = if s == D3D12_RESOURCE_STATE_RENDER_TARGET {
        L::COLOR_ATTACHMENT_OPTIMAL
    } else if s == D3D12_RESOURCE_STATE_DEPTH_WRITE {
        L::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
    } else if s == D3D12_RESOURCE_STATE_DEPTH_READ {
        L::DEPTH_STENCIL_READ_ONLY_OPTIMAL
    } else if s == D3D12_RESOURCE_STATE_COPY_SOURCE {
        L::TRANSFER_SRC_OPTIMAL
    } else if s == D3D12_RESOURCE_STATE_COPY_DEST {
        L::TRANSFER_DST_OPTIMAL
    } else if (s.0 & D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE.0) != 0 {
        L::SHADER_READ_ONLY_OPTIMAL
    } else {
        L::GENERAL
    };
    State {
        layout,
        stages: S::ALL_COMMANDS,
        access: A::MEMORY_READ | A::MEMORY_WRITE,
    }
}

/// The Direct3D 12 state a layer state stands for (the inverse of [`state`]).
pub(crate) fn d3d12_state(s: State, depth: bool) -> D3D12_RESOURCE_STATES {
    use ash::vk::ImageLayout as L;
    match s.layout {
        L::COLOR_ATTACHMENT_OPTIMAL => D3D12_RESOURCE_STATE_RENDER_TARGET,
        L::DEPTH_STENCIL_ATTACHMENT_OPTIMAL | L::DEPTH_ATTACHMENT_OPTIMAL => {
            D3D12_RESOURCE_STATE_DEPTH_WRITE
        }
        L::DEPTH_STENCIL_READ_ONLY_OPTIMAL | L::DEPTH_READ_ONLY_OPTIMAL => {
            D3D12_RESOURCE_STATE_DEPTH_READ
        }
        L::TRANSFER_SRC_OPTIMAL => D3D12_RESOURCE_STATE_COPY_SOURCE,
        L::TRANSFER_DST_OPTIMAL => D3D12_RESOURCE_STATE_COPY_DEST,
        L::SHADER_READ_ONLY_OPTIMAL => sampled_state(depth),
        _ => D3D12_RESOURCE_STATE_COMMON,
    }
}

/// The state a texture is sampled in: by any stage, and depth tested at once where it holds depth.
pub(crate) fn sampled_state(depth: bool) -> D3D12_RESOURCE_STATES {
    let read = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE.0
        | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE.0;
    D3D12_RESOURCE_STATES(if depth {
        read | D3D12_RESOURCE_STATE_DEPTH_READ.0
    } else {
        read
    })
}

/// A Direct3D 12 failure as the layer's error.
pub(crate) fn hr<T>(what: &'static str, r: windows::core::Result<T>) -> Result<T, Error> {
    r.map_err(|e| Error::D3d12 {
        what,
        detail: format!("{} ({:#010x})", e.message(), e.code().0),
    })
}

/// A pointer into a mapped resource, kept with it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mapped(pub *mut u8);

// SAFETY: the pointer is into memory the resource that owns it keeps mapped for its whole life,
// and the layer is driven from one thread at a time.
unsafe impl Send for Mapped {}
// SAFETY: as above.
unsafe impl Sync for Mapped {}

/// The formats a [`Format`] is made, drawn into and read with: the resource's, the target view's
/// and the shader view's.
pub(crate) const fn formats(f: Format) -> (DXGI_FORMAT, DXGI_FORMAT, DXGI_FORMAT) {
    match f {
        Format::Rgba8Unorm => same(DXGI_FORMAT_R8G8B8A8_UNORM),
        Format::Bgra8Unorm => same(DXGI_FORMAT_B8G8R8A8_UNORM),
        Format::Rgba8Uint => same(DXGI_FORMAT_R8G8B8A8_UINT),
        Format::Rg16Snorm => same(DXGI_FORMAT_R16G16_SNORM),
        Format::Rgba16Float => same(DXGI_FORMAT_R16G16B16A16_FLOAT),
        Format::Rg11b10Ufloat => same(DXGI_FORMAT_R11G11B10_FLOAT),
        Format::R8Unorm => same(DXGI_FORMAT_R8_UNORM),
        Format::R16Float => same(DXGI_FORMAT_R16_FLOAT),
        Format::R32Float => same(DXGI_FORMAT_R32_FLOAT),
        Format::R32Uint => same(DXGI_FORMAT_R32_UINT),
        Format::D32Float => (
            DXGI_FORMAT_R32_TYPELESS,
            DXGI_FORMAT_D32_FLOAT,
            DXGI_FORMAT_R32_FLOAT,
        ),
        Format::D24UnormS8 => (
            DXGI_FORMAT_R24G8_TYPELESS,
            DXGI_FORMAT_D24_UNORM_S8_UINT,
            DXGI_FORMAT_R24_UNORM_X8_TYPELESS,
        ),
        Format::D32FloatS8 => (
            DXGI_FORMAT_R32G8X24_TYPELESS,
            DXGI_FORMAT_D32_FLOAT_S8X24_UINT,
            DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS,
        ),
        Format::Bc1 => same(DXGI_FORMAT_BC1_UNORM),
        Format::Bc2 => same(DXGI_FORMAT_BC2_UNORM),
        Format::Bc3 => same(DXGI_FORMAT_BC3_UNORM),
        Format::Bc5 => same(DXGI_FORMAT_BC5_UNORM),
        Format::Bc7 => same(DXGI_FORMAT_BC7_UNORM),
    }
}

const fn same(f: DXGI_FORMAT) -> (DXGI_FORMAT, DXGI_FORMAT, DXGI_FORMAT) {
    (f, f, f)
}

/// The layer's format for a DXGI one, when it is one of them.
#[must_use]
pub fn format_of(f: DXGI_FORMAT) -> Option<Format> {
    [
        Format::Rgba8Unorm,
        Format::Bgra8Unorm,
        Format::Rgba8Uint,
        Format::Rg16Snorm,
        Format::Rgba16Float,
        Format::Rg11b10Ufloat,
        Format::R8Unorm,
        Format::R16Float,
        Format::R32Float,
        Format::R32Uint,
        Format::D32Float,
        Format::D24UnormS8,
        Format::D32FloatS8,
        Format::Bc1,
        Format::Bc2,
        Format::Bc3,
        Format::Bc5,
        Format::Bc7,
    ]
    .into_iter()
    .find(|x| {
        let (res, target, view) = formats(*x);
        f == res || f == target || f == view
    })
}

/// When something the device holds may go: the fence value a submission of its own signals, and
/// the stamp of a frame recorded into an adopted list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Retire {
    pub value: u64,
    pub frame: i64,
}

/// What waits to be dropped or reused until the work that used it is done. The resources are
/// held only to be released then.
#[allow(dead_code)]
pub(crate) enum Garbage {
    Buffer(resource::BufferEntry),
    Texture(resource::TextureEntry),
    Pipeline(ID3D12PipelineState),
    Command(ID3D12CommandAllocator, ID3D12GraphicsCommandList),
    Staging(resource::Arena),
    Scratch(ID3D12Resource),
    Timer(recorder::TimerSlot),
    TextureIndex(u32),
    BufferIndex(u32),
    SamplerIndex(u32),
    AccelIndex(u32),
    Rtv(u32),
    Dsv(u32),
}

/// The descriptor heaps: the two the shaders index, and the target views.
#[derive(Debug)]
pub(crate) struct Heaps {
    pub resources: ID3D12DescriptorHeap,
    pub samplers: ID3D12DescriptorHeap,
    pub rtv: ID3D12DescriptorHeap,
    pub dsv: ID3D12DescriptorHeap,
    pub resource_step: usize,
    pub sampler_step: usize,
    pub rtv_step: usize,
    pub dsv_step: usize,
}

impl Heaps {
    fn new(device: &ID3D12Device) -> Result<Self, Error> {
        let heap = |ty, count, visible: bool| -> Result<ID3D12DescriptorHeap, Error> {
            let desc = D3D12_DESCRIPTOR_HEAP_DESC {
                Type: ty,
                NumDescriptors: count,
                Flags: if visible {
                    D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
                } else {
                    D3D12_DESCRIPTOR_HEAP_FLAG_NONE
                },
                NodeMask: 0,
            };
            // SAFETY: a live local description.
            hr("CreateDescriptorHeap", unsafe {
                device.CreateDescriptorHeap(&desc)
            })
        };
        // SAFETY: pure queries on a live device.
        let step = |ty| unsafe { device.GetDescriptorHandleIncrementSize(ty) } as usize;
        Ok(Self {
            resources: heap(
                D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                ACCELS_AT + crate::rt::ACCELS,
                true,
            )?,
            samplers: heap(D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER, crate::SAMPLERS, true)?,
            rtv: heap(D3D12_DESCRIPTOR_HEAP_TYPE_RTV, RTVS, false)?,
            dsv: heap(D3D12_DESCRIPTOR_HEAP_TYPE_DSV, DSVS, false)?,
            resource_step: step(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV),
            sampler_step: step(D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER),
            rtv_step: step(D3D12_DESCRIPTOR_HEAP_TYPE_RTV),
            dsv_step: step(D3D12_DESCRIPTOR_HEAP_TYPE_DSV),
        })
    }

    fn at(heap: &ID3D12DescriptorHeap, step: usize, i: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        // SAFETY: a pure query on a live heap.
        let start = unsafe { heap.GetCPUDescriptorHandleForHeapStart() };
        D3D12_CPU_DESCRIPTOR_HANDLE {
            ptr: start.ptr + step * i as usize,
        }
    }

    pub(crate) fn resource(&self, i: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        Self::at(&self.resources, self.resource_step, i)
    }

    pub(crate) fn sampler(&self, i: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        Self::at(&self.samplers, self.sampler_step, i)
    }

    pub(crate) fn rtv(&self, i: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        Self::at(&self.rtv, self.rtv_step, i)
    }

    pub(crate) fn dsv(&self, i: u32) -> D3D12_CPU_DESCRIPTOR_HANDLE {
        Self::at(&self.dsv, self.dsv_step, i)
    }
}

/// The indices free in each heap.
#[derive(Debug)]
pub(crate) struct Free {
    pub textures: Indices,
    pub buffers: Indices,
    pub samplers: Indices,
    pub rtvs: Indices,
    pub dsvs: Indices,
}

/// A device: its interfaces, heaps, root signature, fence and tables.
pub struct Device {
    pub(crate) device: ID3D12Device,
    pub(crate) queue: ID3D12CommandQueue,
    owned: bool,
    debug: bool,
    pub(crate) caps: Caps,
    pub(crate) fence: ID3D12Fence,
    event: HANDLE,
    pub(crate) next_value: u64,
    pub(crate) finished_frame: i64,
    pub(crate) last_frame: i64,
    pub(crate) heaps: Heaps,
    pub(crate) free: Free,
    pub(crate) root: ID3D12RootSignature,
    pub(crate) signatures: pipeline::Signatures,
    pub(crate) buffers: Slots<resource::BufferEntry>,
    pub(crate) textures: Slots<resource::TextureEntry>,
    pub(crate) samplers: Slots<resource::SamplerEntry>,
    pub(crate) pipelines: Vec<Option<pipeline::PipelineEntry>>,
    pub(crate) pending: Vec<resource::Pending>,
    pub(crate) staging: resource::Staging,
    pub(crate) garbage: Vec<(Retire, Garbage)>,
    pub(crate) commands: Vec<(ID3D12CommandAllocator, ID3D12GraphicsCommandList)>,
    pub(crate) timers: recorder::Timers,
    pub(crate) stats: Stats,
    /// What ray tracing keeps, where the device traces rays.
    pub(crate) rt: Option<rt::Rt>,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("backend", &"Direct3D 12")
            .field("caps", &self.caps)
            .field("owned", &self.owned)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

/// Whether `device` has what the layer draws with: shader model 6.6 and the third resource
/// binding tier.
fn missing(device: &ID3D12Device) -> Vec<&'static str> {
    let mut out = Vec::new();
    let mut opts = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
    // SAFETY: the struct is the feature's own, sized as the call says.
    let ok = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_D3D12_OPTIONS,
            std::ptr::from_mut(&mut opts).cast(),
            u32::try_from(std::mem::size_of_val(&opts)).unwrap_or(0),
        )
    };
    if ok.is_err() || opts.ResourceBindingTier.0 < D3D12_RESOURCE_BINDING_TIER_3.0 {
        out.push("resource binding tier 3");
    }
    let mut sm = D3D12_FEATURE_DATA_SHADER_MODEL {
        HighestShaderModel: D3D_SHADER_MODEL_6_6,
    };
    // SAFETY: as above.
    let ok = unsafe {
        device.CheckFeatureSupport(
            D3D12_FEATURE_SHADER_MODEL,
            std::ptr::from_mut(&mut sm).cast(),
            u32::try_from(std::mem::size_of_val(&sm)).unwrap_or(0),
        )
    };
    if ok.is_err() || sm.HighestShaderModel.0 < D3D_SHADER_MODEL_6_6.0 {
        out.push("shader model 6.6");
    }
    out
}

/// The adapter a device runs on: its name and whether it is a software rasterizer.
fn adapter(device: &ID3D12Device) -> (String, bool) {
    // SAFETY: pure queries on live interfaces.
    unsafe {
        let luid = device.GetAdapterLuid();
        let Ok(factory) = CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0)) else {
            return (String::new(), false);
        };
        let Ok(a) = factory.EnumAdapterByLuid::<IDXGIAdapter1>(luid) else {
            return (String::new(), false);
        };
        let Ok(d) = a.GetDesc1() else {
            return (String::new(), false);
        };
        let len = d
            .Description
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(d.Description.len());
        (
            String::from_utf16_lossy(&d.Description[..len]),
            (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
        )
    }
}

impl Device {
    /// Make a device of the layer's own: the hardware adapter with the most memory that has
    /// shader model 6.6 and the third binding tier, or the software rasterizer when `desc` asks
    /// for one.
    ///
    /// # Errors
    /// [`Error::Unsupported`] when no adapter has what the layer draws with; any failure of the
    /// runtime.
    pub fn create(desc: &DeviceDesc) -> Result<Self, Error> {
        let debug = desc.debug && Self::enable_debug_layer();
        // SAFETY: a plain factory, released when it goes.
        let factory: IDXGIFactory4 = hr("CreateDXGIFactory2", unsafe {
            CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))
        })?;
        let mut refused = Vec::new();
        let mut best: Option<(u64, ID3D12Device)> = None;
        if desc.software {
            // SAFETY: a pure query.
            let warp: IDXGIAdapter1 = hr("EnumWarpAdapter", unsafe { factory.EnumWarpAdapter() })?;
            let mut device: Option<ID3D12Device> = None;
            // SAFETY: a live adapter and a local out slot.
            hr("D3D12CreateDevice", unsafe {
                D3D12CreateDevice(&warp, D3D_FEATURE_LEVEL_12_0, &mut device)
            })?;
            best = device.map(|d| (0, d));
        } else {
            for i in 0.. {
                // SAFETY: a pure query; it fails past the last adapter.
                let Ok(a) = (unsafe { factory.EnumAdapters1(i) }) else {
                    break;
                };
                // SAFETY: a pure query on a live adapter.
                let Ok(d) = (unsafe { a.GetDesc1() }) else {
                    continue;
                };
                if (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                    continue;
                }
                let mut device: Option<ID3D12Device> = None;
                // SAFETY: a live adapter and a local out slot.
                if unsafe { D3D12CreateDevice(&a, D3D_FEATURE_LEVEL_12_0, &mut device) }.is_err() {
                    continue;
                }
                let Some(device) = device else {
                    continue;
                };
                let lacks = missing(&device);
                if !lacks.is_empty() {
                    let len = d.Description.iter().position(|c| *c == 0).unwrap_or(0);
                    refused.push(format!(
                        "{}: lacks {}",
                        String::from_utf16_lossy(&d.Description[..len]),
                        lacks.join(", ")
                    ));
                    continue;
                }
                let memory = d.DedicatedVideoMemory as u64;
                if best.as_ref().is_none_or(|b| memory > b.0) {
                    best = Some((memory, device));
                }
            }
        }
        let Some((_, device)) = best else {
            return Err(Error::Unsupported(format!(
                "no Direct3D 12 adapter has what the layer draws with ({})",
                refused.join("; ")
            )));
        };
        let qdesc = D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            ..Default::default()
        };
        // SAFETY: a live device and description.
        let queue: ID3D12CommandQueue = hr("CreateCommandQueue", unsafe {
            device.CreateCommandQueue(&qdesc)
        })?;
        Self::build(
            D3d12Native {
                device,
                queue,
                ray_queries: true,
            },
            desc,
            true,
            debug,
        )
    }

    /// Turn the runtime's debug layer on, where it is installed: whether it is.
    fn enable_debug_layer() -> bool {
        let mut debug: Option<ID3D12Debug> = None;
        // SAFETY: a local out slot; the call fails without the SDK layers.
        if unsafe { D3D12GetDebugInterface(&mut debug) }.is_err() {
            return false;
        }
        match debug {
            Some(d) => {
                // SAFETY: a live interface, before any device is made.
                unsafe { d.EnableDebugLayer() };
                true
            }
            None => false,
        }
    }

    /// Adopt a device that already exists.
    ///
    /// # Safety
    /// See [`crate::Device::adopt_d3d12`].
    ///
    /// # Errors
    /// [`Error::Unsupported`] for a device without what the layer draws with.
    pub unsafe fn adopt(native: D3d12Native, desc: &DeviceDesc) -> Result<Self, Error> {
        Self::build(native, desc, false, false)
    }

    fn build(
        native: D3d12Native,
        desc: &DeviceDesc,
        owned: bool,
        debug: bool,
    ) -> Result<Self, Error> {
        let D3d12Native {
            device,
            queue,
            ray_queries,
        } = native;
        let rays = ray_queries.then(|| rt::support(&device)).flatten();
        let lacks = missing(&device);
        if !lacks.is_empty() {
            return Err(Error::Unsupported(format!(
                "the Direct3D 12 device lacks {}",
                lacks.join(", ")
            )));
        }
        let (name, software) = adapter(&device);
        // SAFETY: a pure query on a live queue.
        let frequency = unsafe { queue.GetTimestampFrequency() }.unwrap_or(0);
        let packed_float_targets = {
            let mut s = D3D12_FEATURE_DATA_FORMAT_SUPPORT {
                Format: DXGI_FORMAT_R11G11B10_FLOAT,
                ..Default::default()
            };
            // SAFETY: the struct is the feature's own, sized as the call says.
            let ok = unsafe {
                device.CheckFeatureSupport(
                    D3D12_FEATURE_FORMAT_SUPPORT,
                    std::ptr::from_mut(&mut s).cast(),
                    u32::try_from(std::mem::size_of_val(&s)).unwrap_or(0),
                )
            };
            let want = D3D12_FORMAT_SUPPORT1_RENDER_TARGET.0
                | D3D12_FORMAT_SUPPORT1_BLENDABLE.0
                | D3D12_FORMAT_SUPPORT1_SHADER_SAMPLE.0;
            ok.is_ok() && (s.Support1.0 & want) == want
        };
        #[allow(clippy::cast_precision_loss)] // a tick rate
        let caps = Caps {
            name,
            hardware: !software,
            timestamps: frequency > 0,
            timestamp_period: if frequency > 0 {
                (1.0e9 / frequency as f64) as f32
            } else {
                0.0
            },
            bindless_textures: desc.bindless_textures.min(MAX_TEXTURES),
            bindless_buffers: desc.bindless_buffers.min(MAX_BUFFERS),
            block_compression: true,
            max_anisotropy: 16.0,
            packed_float_targets,
            ray_queries: rays.is_some(),
            indirect_count: true,
        };
        // SAFETY: a live device; the fence is released with the layer.
        let fence: ID3D12Fence = hr("CreateFence", unsafe {
            device.CreateFence(0, D3D12_FENCE_FLAG_NONE)
        })?;
        // SAFETY: an unnamed auto-reset event, closed in `Drop`.
        let event = hr("CreateEventW", unsafe {
            CreateEventW(None, false, false, None)
        })?;
        let heaps = Heaps::new(&device)?;
        let root = pipeline::root_signature(&device)?;
        let signatures = pipeline::Signatures::new(&device, &root)?;
        let mut d = Self {
            device,
            queue,
            owned,
            debug,
            caps,
            fence,
            event,
            next_value: 1,
            finished_frame: -1,
            last_frame: -1,
            heaps,
            free: Free {
                textures: Indices::new(desc.bindless_textures.min(MAX_TEXTURES)),
                buffers: Indices::new(desc.bindless_buffers.min(MAX_BUFFERS)),
                samplers: Indices::new(crate::SAMPLERS),
                rtvs: Indices::new(RTVS),
                dsvs: Indices::new(DSVS),
            },
            root,
            signatures,
            buffers: Slots::default(),
            textures: Slots::default(),
            samplers: Slots::default(),
            pipelines: Vec::new(),
            pending: Vec::new(),
            staging: resource::Staging::default(),
            garbage: Vec::new(),
            commands: Vec::new(),
            timers: recorder::Timers::default(),
            stats: Stats::default(),
            rt: rays,
        };
        d.null_views();
        Ok(d)
    }

    /// What the device found it can do.
    #[must_use]
    pub fn caps(&self) -> &Caps {
        &self.caps
    }

    /// What the device has counted since it was made.
    #[must_use]
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Whether the runtime's debug layer is checking the device's calls.
    #[must_use]
    pub fn validating(&self) -> bool {
        self.debug
    }

    /// Whether the device is the layer's own rather than adopted.
    #[must_use]
    pub fn owned(&self) -> bool {
        self.owned
    }

    /// When work recorded now will have finished.
    pub(crate) fn now(&self) -> Retire {
        Retire {
            value: self.next_value.saturating_sub(1),
            frame: self.last_frame,
        }
    }

    /// The highest fence value the device has passed.
    pub(crate) fn completed(&self) -> u64 {
        // SAFETY: a pure query on a live fence.
        unsafe { self.fence.GetCompletedValue() }
    }

    /// Hold `g` until the work recorded so far is done.
    pub(crate) fn defer(&mut self, g: Garbage) {
        let at = self.now();
        self.garbage.push((at, g));
    }

    /// Drop or reuse what the finished work was holding, and take the debug layer's errors.
    pub(crate) fn collect(&mut self) {
        self.drain_messages();
        if self.garbage.is_empty() {
            return;
        }
        let completed = self.completed();
        let finished = self.finished_frame;
        let (done, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.garbage)
            .into_iter()
            .partition(|(r, _)| r.value <= completed && r.frame <= finished);
        self.garbage = keep;
        for (_, g) in done {
            self.free(g);
        }
    }

    /// Drop or reuse `g` now.
    fn free(&mut self, g: Garbage) {
        match g {
            Garbage::Buffer(_)
            | Garbage::Texture(_)
            | Garbage::Pipeline(_)
            | Garbage::Scratch(_) => {}
            Garbage::Command(a, l) => self.commands.push((a, l)),
            Garbage::Staging(a) => self.staging.free.push(a),
            Garbage::Timer(t) => self.timers.retired(t, self.caps.timestamp_period),
            Garbage::TextureIndex(i) => self.free.textures.release(i),
            Garbage::BufferIndex(i) => self.free.buffers.release(i),
            Garbage::SamplerIndex(i) => self.free.samplers.release(i),
            Garbage::AccelIndex(i) => {
                if let Some(rt) = self.rt.as_mut() {
                    rt.release_index(i);
                }
            }
            Garbage::Rtv(i) => self.free.rtvs.release(i),
            Garbage::Dsv(i) => self.free.dsvs.release(i),
        }
    }

    /// The debug layer's errors since the last look, into the layer's list of them.
    fn drain_messages(&self) {
        if !self.debug {
            return;
        }
        let Ok(queue) = self.device.cast::<ID3D12InfoQueue>() else {
            return;
        };
        // SAFETY: the queue is live; each message is read into a buffer the runtime sized.
        unsafe {
            let n = queue.GetNumStoredMessages();
            for i in 0..n {
                let mut len = 0usize;
                if queue.GetMessage(i, None, &mut len).is_err() || len == 0 {
                    continue;
                }
                let mut buf = vec![0u64; len.div_ceil(8)];
                let msg = buf.as_mut_ptr().cast::<D3D12_MESSAGE>();
                if queue.GetMessage(i, Some(msg), &mut len).is_err() {
                    continue;
                }
                let m = &*msg;
                if m.Severity == D3D12_MESSAGE_SEVERITY_ERROR
                    || m.Severity == D3D12_MESSAGE_SEVERITY_CORRUPTION
                {
                    let text = std::slice::from_raw_parts(
                        m.pDescription.cast::<u8>(),
                        m.DescriptionByteLength.saturating_sub(1),
                    );
                    crate::device::validation_error(String::from_utf8_lossy(text).into_owned());
                }
            }
            queue.ClearStoredMessages();
        }
    }

    /// The next fence value of the device's own.
    pub(crate) fn signal_next(&mut self) -> u64 {
        let v = self.next_value;
        self.next_value += 1;
        v
    }

    /// Block until the fence passes `value`.
    fn wait_for(&self, value: u64) -> Result<(), Error> {
        if self.completed() >= value {
            return Ok(());
        }
        // SAFETY: the event is the device's own and nothing else signals it.
        unsafe {
            hr(
                "SetEventOnCompletion",
                self.fence.SetEventOnCompletion(value, self.event),
            )?;
            WaitForSingleObject(self.event, INFINITE);
        }
        Ok(())
    }

    /// Block until the device has finished the submission `ticket` names, then let go of what
    /// finished work was holding.
    ///
    /// # Errors
    /// Any failure of the runtime.
    pub fn wait(&mut self, ticket: Ticket) -> Result<(), Error> {
        self.wait_for(ticket.0)?;
        self.collect();
        Ok(())
    }

    /// Block until the queue has finished everything submitted to it, then let go of everything
    /// held.
    ///
    /// # Errors
    /// Any failure of the runtime.
    pub fn wait_idle(&mut self) -> Result<(), Error> {
        let v = self.signal_next();
        // SAFETY: the queue and fence are live; the layer is driven from the queue's one thread.
        hr("Signal", unsafe { self.queue.Signal(&self.fence, v) })?;
        self.wait_for(v)?;
        self.finished_frame = self.last_frame;
        self.collect();
        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.wait_idle();
        self.finished_frame = i64::MAX;
        let garbage = std::mem::take(&mut self.garbage);
        for (_, g) in garbage {
            self.free(g);
        }
        if !self.event.is_invalid() {
            // SAFETY: the event is the device's own, and nothing waits on it any more.
            let _ = unsafe { CloseHandle(self.event) };
        }
    }
}
