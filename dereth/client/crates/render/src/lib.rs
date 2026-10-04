//! The graphics device and renderer core: the window surface, the Vulkan, Direct3D 12 and wgpu
//! backends, the pipeline state and draw submission.
//!
//! **Depends on** `dereth-primitives`, the contract (`dereth-client-contract`) and its device-free
//! half (`dereth-render-cpu`, every item of which it re-exports). **Used by** the client
//! (`dereth-client`).
//!
//! **Must never** decide *what* to draw or in what order: `RenderBackend::draw` draws exactly the
//! batch it is handed, exactly then, and never sorts, culls or reorders, because submission order
//! is the observable. It must never reach the client runtime (`cargo xtask seams`, `seam: client
//! crates`), and it must never change the FPU control word: the original client kept its configured
//! precision for the whole session, and if the graphics runtime changed it every physics comparison
//! would become noise.
//!
//! This crate is the device: [`device`], one enum over the three backends (`vulkan`, the default,
//! `d3d12`, Windows only, and `wgpu`), the shader sources, the Win32 window surface
//! ([`window_proc`], [`debug`]) and the dat cursor's image ([`cursor`]). The codecs, pipeline
//! descriptions, font atlas, UI quad path and camera are [`dereth_render_cpu`], re-exported here so
//! callers need not know about the split.
//!
//! It is one of the crates permitted `unsafe`, because Vulkan and Win32 are C ABIs: it lives in
//! [`vulkan`], in [`debug`]'s top-level exception filter and the native backend modules, each block with a `SAFETY:` comment.

#![doc(html_no_source)]

pub mod cursor;
pub mod device;

#[cfg(feature = "vulkan")]
pub mod vulkan;

/// The `wgpu` device: WebGPU or WebGL 2 in a browser, and Metal, Vulkan or Direct3D 12 natively.
/// [`device::Gpu`] is how anything outside this crate reaches it; a browser also hands it a device
/// made ahead of time ([`wgpu::install`]).
#[cfg(feature = "wgpu")]
pub mod wgpu;

/// The WGSL the Vulkan and `wgpu` backends share: the landscape composite and splat.
#[cfg(any(feature = "vulkan", feature = "wgpu"))]
mod wgsl;

/// The Direct3D 12 device. Windows only, and only when this build compiled the
/// `d3d12` feature; [`device::Gpu`] is how anything outside this crate reaches it.
#[cfg(all(windows, feature = "d3d12"))]
pub mod d3d12;

/// The top-level exception filter, so that a fault is
/// reported rather than read as silence. Win32 only; a no-op elsewhere.
pub mod debug;

pub mod window_proc;

/// Keeps this crate's own tests from opening a `wgpu` device while a Vulkan-backend device is
/// open on another thread. Devices of one backend may overlap freely; a `wgpu` instance created
/// while the Vulkan backend's devices are being made and destroyed on other threads has hung the
/// test process in the driver.
#[cfg(all(test, feature = "vulkan", feature = "wgpu"))]
pub(crate) mod backend_lock {
    use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

    static LOCK: RwLock<()> = RwLock::new(());

    /// Held by a Vulkan-backend device for its lifetime.
    pub(crate) fn vulkan() -> RwLockReadGuard<'static, ()> {
        LOCK.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Held by a `wgpu` device for its lifetime.
    pub(crate) fn wgpu() -> RwLockWriteGuard<'static, ()> {
        LOCK.write().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub use device::Gpu;
pub use device::{
    hlsl_matrix, AdapterKind, Backend, CapturedImage, DescriptorUsage, DeviceConfig, MergeSource,
    PerDrawConstants, PerFrameConstants, TerrainMergeJob, TerrainMergeOverlay, TerrainSplat,
    TerrainSplatOverlay, TextureSlot, WindowHandles, FRAME_COUNT,
};

// ---------------------------------------------------------------------------------------------
// The CPU half. `dereth-render-cpu` holds these modules; they are re-exported here so that
// callers -- inside the crate or outside -- name them under `dereth_render::`. See that crate's module documentation for what is in each.
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "mock")]
pub use dereth_render_cpu::mock;
pub use dereth_render_cpu::{
    camera, descriptor, dxt, font, jpeg, mip, palette, pixel_format, pso, sampler, surface,
    texture, ui, vertex,
};

pub use dereth_render_cpu::ExpandedPalette;
pub use dereth_render_cpu::{
    combined_texture_key, DescriptorAllocator, DescriptorStats, Released, TextureKey, TextureSpace,
    TextureTable, TextureTableStats, DESCRIPTORS_PER_TEXTURE, UNCACHED,
};
pub use dereth_render_cpu::{
    compute_aspect_for_viewport, fov_y_from_preference, projection, view_from_frame,
    AspectPreference, FogParams, LightBlock, ViewParams, Viewport,
};
pub use dereth_render_cpu::{decode_surface, select_surface_format, Caps, SourcePixels};
pub use dereth_render_cpu::{
    gamma_ramp, pack_clear_colour, DrawConstants, RenderError, COLOR_WRITE_ENABLE_RGB,
};
pub use dereth_render_cpu::{
    Blend, Cull, PipelineKey, PixelShader, StageOps, SurfaceContext, SurfaceState, ZFunc,
};
pub use dereth_render_cpu::{PixelFormatDesc, PixelFormatId};
pub use dereth_render_cpu::{Surface, SurfaceHandler};
pub use dereth_render_cpu::{VertexFormat, VertexLayoutInfo};

#[cfg(feature = "mock")]
pub use dereth_render_cpu::{RecordedCall, RecordingBackend};
