//! The remaster's backend layer: buffers, textures and samplers, one bindless descriptor table,
//! pipelines from precompiled shader blobs, command recording with the barriers a small render
//! graph places, indirect draws, timestamps and read-back, on Vulkan 1.3 and Direct3D 12.
//!
//! **Two backends** behind one interface ([`Backend`]): Vulkan 1.3 through `ash`, and on Windows
//! Direct3D 12 at shader model 6.6 (`d3d12`), where every resource is indexed from the heaps
//! and a draw's item is a root constant. The draws are plain, indexed, indirect and
//! indirect-count; an indirect command's layout is the backend's
//! ([`Device::indexed_command`]). Pipelines are made from blobs compiled ahead of time, SPIR-V or
//! signed DXIL: nothing compiles a shader at run time.
//!
//! **Two ways in.** [`Device::create`] makes a device of the layer's own, with no window (tests,
//! headless work). [`Device::adopt`] (Vulkan) and `Device::adopt_d3d12` take a device that
//! already exists, the ordinary renderer's, and record into the command buffer or list of its
//! frame ([`Device::record_into`], `Device::record_into_d3d12`), so the two draw on one device
//! and one frame.
//!
//! **Depends on** `ash` and `gpu-allocator`, and on Windows `windows`: no crate of the workspace.
//! **Used by** the remaster (`dereth-remaster`), and nothing else.
//!
//! **Must never** reach anything of the game: it holds pictures, buffers and pipelines, and
//! records what it is told to, so what it draws is decided by its user.
//!
//! **How it is used:**
//! - make resources ([`Device::create_buffer`], [`Device::create_texture`],
//!   [`Device::create_sampler`]) and fill them ([`Device::write_buffer`],
//!   [`Device::upload_texture`]); staged writes are copied at the head of the next recording;
//! - give shaders their indices in the one table ([`Device::bind_texture`],
//!   [`Device::bind_sampler`], [`Device::bind_buffer`]) and push them per draw;
//! - make pipelines from shader blobs ([`Device::create_graphics_pipeline`],
//!   [`Device::create_compute_pipeline`]), every one on the same layout;
//! - record passes ([`Recorder::raster`], [`Recorder::compute`]) that say what they touch; the
//!   graph ([`graph`]) puts the barriers in front of each.
//! - where the device traces rays ([`Caps::ray_queries`]), make acceleration structures of the
//!   meshes and the instances that place them ([`rt`]), for shaders' inline ray queries.
//!
//! It is one of the crates permitted `unsafe`, because Vulkan and Direct3D 12 are C ABIs; every
//! block carries a `SAFETY:` comment. It never changes the floating-point environment.

#![allow(clippy::cast_possible_truncation)] // Vulkan sizes are u32; each site is bounded.

mod bindless;
#[cfg(windows)]
pub mod d3d12;
mod device;
mod front;
pub mod graph;
#[cfg(target_os = "macos")]
pub mod metal;
mod pipeline;
mod query;
mod recorder;
mod resource;
pub mod rt;
mod types;
mod upload;

pub use ash::vk;
pub use bindless::SAMPLERS;
pub use device::{take_validation_errors, Caps, DeviceDesc, FrameClock, Native, Ticket, REQUIRED};
pub use front::{Device, Draw, Recorder};
pub use graph::{State, Use};
pub use pipeline::PUSH_BYTES;
pub use query::{Timing, MARKS};
pub use recorder::{ColorAttachment, DepthAttachment, Load, RasterPass, Touch};
pub use resource::Lent;
pub use rt::{Blas, BlasGeometry, Tlas, TlasInstance};
pub use types::*;

/// What went wrong.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The driver refused a call.
    #[error("{what}: {result}")]
    Vulkan {
        /// The call.
        what: &'static str,
        /// What it answered.
        result: vk::Result,
    },
    /// The machine cannot do what was asked.
    #[error("{0}")]
    Unsupported(String),
    /// Memory could not be had.
    #[error("memory: {0}")]
    Allocation(String),
    /// A call the layer refuses: a handle it does not hold, or a range past an end.
    #[error("{0}")]
    Invalid(String),
    /// The bindless table has no index left of a kind.
    #[error("the bindless table's {0} are all taken")]
    Full(&'static str),
    /// Direct3D 12 refused a call.
    #[error("{what}: {detail}")]
    D3d12 {
        /// The call.
        what: &'static str,
        /// What it answered.
        detail: String,
    },
}
