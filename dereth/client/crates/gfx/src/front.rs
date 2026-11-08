//! The layer's public device, recording and draw handles: one API over either backend. Each
//! call goes to the backend the device records with, Vulkan ([`crate::device`]) or Direct3D 12
//! (the `d3d12` module); the backends share the plain data in [`crate::types`] and the barrier
//! placement in [`crate::graph`].

use ash::vk;

use crate::recorder::{RasterPass, Touch};
use crate::{
    Backend, Buffer, BufferDesc, Caps, ComputeDesc, DeviceDesc, Error, FrameClock, GraphicsDesc,
    IndexType, Lent, Native, Pipeline, Rect, Sampler, SamplerDesc, Stats, Texture, TextureDesc,
    Ticket, Timing,
};

/// A device: its handles, tables, clock and counters, on the backend it was made or adopted on.
pub struct Device {
    pub(crate) b: Dev,
}

/// The backend a device records with. A process holds few devices, so the sizes don't matter.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Dev {
    Vk(crate::device::Device),
    #[cfg(windows)]
    Dx(crate::d3d12::Device),
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.b {
            Dev::Vk(d) => d.fmt(f),
            #[cfg(windows)]
            Dev::Dx(d) => d.fmt(f),
        }
    }
}

/// Call `$call` on whichever backend `$dev` is.
macro_rules! on {
    ($dev:expr, $d:ident => $call:expr) => {
        match $dev {
            Dev::Vk($d) => $call,
            #[cfg(windows)]
            Dev::Dx($d) => $call,
        }
    };
}

#[cfg(not(windows))]
fn d3d12_unsupported() -> Error {
    Error::Unsupported("Direct3D 12 is Windows' own".into())
}

impl Device {
    /// Make a device of the layer's own, with no window, on the backend `desc` names: the
    /// hardware adapter unless `desc.software` asks for a software rasterizer, the best one with
    /// everything the layer draws with.
    ///
    /// # Errors
    /// [`Error::Unsupported`] when the backend or such an adapter is missing, or any failure of
    /// the driver.
    pub fn create(desc: &DeviceDesc) -> Result<Self, Error> {
        match desc.backend {
            Backend::Vulkan => Ok(Self {
                b: Dev::Vk(crate::device::Device::create(desc)?),
            }),
            #[cfg(windows)]
            Backend::D3d12 => Ok(Self {
                b: Dev::Dx(crate::d3d12::Device::create(desc)?),
            }),
            #[cfg(not(windows))]
            Backend::D3d12 => Err(d3d12_unsupported()),
        }
    }

    /// Adopt a Vulkan device that already exists. The layer makes its own allocator, command
    /// pool, timeline, descriptor table and pipeline cache on it, and destroys only those.
    ///
    /// # Safety
    /// The handles must be live, the device made with every feature [`crate::REQUIRED`] lists,
    /// and they must outlive the layer. Everything the layer submits goes to `native.queue`, from
    /// the thread that drives the layer, which must be the one its owner submits from.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub unsafe fn adopt(native: Native, desc: &DeviceDesc) -> Result<Self, Error> {
        Ok(Self {
            // SAFETY: the caller's obligations are the backend's.
            b: Dev::Vk(unsafe { crate::device::Device::adopt(native, desc) }?),
        })
    }

    /// Adopt a Direct3D 12 device that already exists, and the queue its owner submits to. The
    /// layer makes its own descriptor heaps, root signature, fence and pipelines on it, and
    /// destroys only those.
    ///
    /// # Safety
    /// The device and queue must be live and outlive the layer; the layer is driven from the
    /// thread that submits to the queue.
    ///
    /// # Errors
    /// [`Error::Unsupported`] for a device without shader model 6.6 and the third resource
    /// binding tier; any failure of the driver.
    #[cfg(windows)]
    pub unsafe fn adopt_d3d12(
        native: crate::d3d12::D3d12Native,
        desc: &DeviceDesc,
    ) -> Result<Self, Error> {
        Ok(Self {
            // SAFETY: the caller's obligations are the backend's.
            b: Dev::Dx(unsafe { crate::d3d12::Device::adopt(native, desc) }?),
        })
    }

    /// The backend the device records with.
    #[must_use]
    pub fn backend(&self) -> Backend {
        match &self.b {
            Dev::Vk(_) => Backend::Vulkan,
            #[cfg(windows)]
            Dev::Dx(_) => Backend::D3d12,
        }
    }

    /// What the device found it can do.
    #[must_use]
    pub fn caps(&self) -> &Caps {
        on!(&self.b, d => d.caps())
    }

    /// What the device has counted since it was made.
    #[must_use]
    pub fn stats(&self) -> Stats {
        on!(&self.b, d => d.stats())
    }

    /// Whether the driver's validation (the Khronos layer, or Direct3D 12's debug layer) is
    /// checking the device's calls.
    #[must_use]
    pub fn validating(&self) -> bool {
        on!(&self.b, d => d.validating())
    }

    /// Whether the device is the layer's own rather than adopted.
    #[must_use]
    pub fn owned(&self) -> bool {
        on!(&self.b, d => d.owned())
    }

    /// The family of the queue the layer submits to, on Vulkan.
    #[must_use]
    pub fn queue_family(&self) -> Option<u32> {
        match &self.b {
            Dev::Vk(d) => Some(d.queue_family()),
            #[cfg(windows)]
            Dev::Dx(_) => None,
        }
    }

    /// The raw Vulkan device, for what the layer does not wrap.
    #[must_use]
    pub fn raw(&self) -> Option<&ash::Device> {
        match &self.b {
            Dev::Vk(d) => Some(d.raw()),
            #[cfg(windows)]
            Dev::Dx(_) => None,
        }
    }

    /// The Vulkan instance and physical device, for what the layer does not wrap.
    #[must_use]
    pub fn raw_instance(&self) -> Option<(vk::Instance, vk::PhysicalDevice)> {
        match &self.b {
            Dev::Vk(d) => Some(d.raw_instance()),
            #[cfg(windows)]
            Dev::Dx(_) => None,
        }
    }

    /// Texture `t`'s Vulkan image, its view, its format and its size, for what the layer does not
    /// wrap.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold.
    pub fn raw_texture(&self, t: Texture) -> Result<Option<RawTexture>, Error> {
        match &self.b {
            Dev::Vk(d) => {
                let e = d.texture(t)?;
                Ok(Some((e.image, e.view, e.format.vk(), (e.width, e.height))))
            }
            #[cfg(windows)]
            Dev::Dx(_) => Ok(None),
        }
    }

    /// The Vulkan loader the device came from.
    #[must_use]
    pub fn entry(&self) -> Option<&ash::Entry> {
        match &self.b {
            Dev::Vk(d) => Some(d.entry()),
            #[cfg(windows)]
            Dev::Dx(_) => None,
        }
    }

    /// How many 32-bit words one indexed indirect draw command takes on this device (see
    /// [`Backend::indexed_command`]).
    #[must_use]
    pub fn indexed_command_words(&self) -> u32 {
        self.backend().indexed_command_words()
    }

    /// One indexed indirect draw command as this device reads it.
    #[must_use]
    pub fn indexed_command(
        &self,
        indices: u32,
        instances: u32,
        first_index: u32,
        vertex_offset: i32,
        item: u32,
    ) -> Vec<u32> {
        self.backend()
            .indexed_command(indices, instances, first_index, vertex_offset, item)
    }

    /// Block until the device has finished the submission `ticket` names, then let go of what
    /// finished work was holding.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn wait(&mut self, ticket: Ticket) -> Result<(), Error> {
        on!(&mut self.b, d => d.wait(ticket))
    }

    /// Block until the device's queue is idle, then let go of everything held.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn wait_idle(&mut self) -> Result<(), Error> {
        on!(&mut self.b, d => d.wait_idle())
    }

    /// Make a buffer.
    ///
    /// # Errors
    /// Any failure of the driver or the allocator.
    pub fn create_buffer(&mut self, desc: &BufferDesc<'_>) -> Result<Buffer, Error> {
        on!(&mut self.b, d => d.create_buffer(desc))
    }

    /// Destroy a buffer once the work recorded so far is done with it.
    pub fn destroy_buffer(&mut self, b: Buffer) {
        on!(&mut self.b, d => d.destroy_buffer(b));
    }

    /// A buffer's size, bytes.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn buffer_size(&self, b: Buffer) -> Result<u64, Error> {
        on!(&self.b, d => d.buffer_size(b))
    }

    /// Write `data` into `b` at `offset`: straight into its memory when the processor can reach
    /// it, else staged and copied at the head of the next recording.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a write past the buffer's end, or a failure to stage it.
    pub fn write_buffer(&mut self, b: Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        on!(&mut self.b, d => d.write_buffer(b, offset, data))
    }

    /// Copy the first `size` bytes of `src` into `dst`, at the head of the next recording, after
    /// every staged write queued before it. `src` must not be destroyed before that recording is
    /// begun.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold, or a size past either's end.
    pub fn copy_buffer_later(&mut self, src: Buffer, dst: Buffer, size: u64) -> Result<(), Error> {
        on!(&mut self.b, d => d.copy_buffer_later(src, dst, size))
    }

    /// Read `len` bytes of a read-back buffer at `offset`. The device must have finished the work
    /// that wrote them, and made them visible to the processor ([`crate::Use::HostRead`]).
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer that is not a read-back one, or a read past its end.
    pub fn read_buffer(&self, b: Buffer, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        on!(&self.b, d => d.read_buffer(b, offset, len))
    }

    /// Make a 2D texture.
    ///
    /// # Errors
    /// [`Error::Unsupported`] for block compression on a device without it; any failure of the
    /// driver or the allocator.
    pub fn create_texture(&mut self, desc: &TextureDesc<'_>) -> Result<Texture, Error> {
        on!(&mut self.b, d => d.create_texture(desc))
    }

    /// Destroy a texture once the work recorded so far is done with it. A lent image is only
    /// forgotten.
    pub fn destroy_texture(&mut self, t: Texture) {
        on!(&mut self.b, d => d.destroy_texture(t));
    }

    /// Lend the layer a Vulkan image its owner keeps (see [`Lent`]).
    ///
    /// # Safety
    /// The image and view must be live while the layer holds them, and `lent.state` must be the
    /// state the owner's work left the image in.
    ///
    /// # Errors
    /// [`Error::Invalid`] on a Direct3D 12 device.
    pub unsafe fn lend_texture(&mut self, lent: &Lent) -> Result<Texture, Error> {
        match &mut self.b {
            // SAFETY: the caller's obligations are the backend's.
            Dev::Vk(d) => Ok(unsafe { d.lend_texture(lent) }),
            #[cfg(windows)]
            Dev::Dx(_) => Err(Error::Invalid(
                "a Vulkan image lent to a Direct3D 12 device".into(),
            )),
        }
    }

    /// Lend the layer a Direct3D 12 resource its owner keeps (see
    /// [`crate::d3d12::D3d12Lent`]).
    ///
    /// # Safety
    /// The resource must be live while the layer holds it, and `lent.state` the state the
    /// owner's work left it in.
    ///
    /// # Errors
    /// [`Error::Invalid`] on a Vulkan device; any failure to make its views.
    #[cfg(windows)]
    pub unsafe fn lend_d3d12_texture(
        &mut self,
        lent: &crate::d3d12::D3d12Lent,
    ) -> Result<Texture, Error> {
        match &mut self.b {
            Dev::Vk(_) => Err(Error::Invalid(
                "a Direct3D 12 resource lent to a Vulkan device".into(),
            )),
            // SAFETY: the caller's obligations are the backend's.
            Dev::Dx(d) => unsafe { d.lend_texture(lent) },
        }
    }

    /// A texture's extent.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold.
    pub fn texture_size(&self, t: Texture) -> Result<(u32, u32), Error> {
        on!(&self.b, d => d.texture_size(t))
    }

    /// Upload one level of a sampled texture: staged, copied at the head of the next recording,
    /// and left ready to sample.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a level past the texture's, or bytes that are not the level's size.
    pub fn upload_texture(&mut self, t: Texture, level: u32, data: &[u8]) -> Result<(), Error> {
        on!(&mut self.b, d => d.upload_texture(t, level, data))
    }

    /// Make a sampler.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<Sampler, Error> {
        on!(&mut self.b, d => d.create_sampler(desc))
    }

    /// Destroy a sampler once the work recorded so far is done with it.
    pub fn destroy_sampler(&mut self, s: Sampler) {
        on!(&mut self.b, d => d.destroy_sampler(s));
    }

    /// How many textures the device holds, lent images included.
    #[must_use]
    pub fn live_textures(&self) -> usize {
        on!(&self.b, d => d.live_textures())
    }

    /// Give texture `t` an index in the table, the one it holds if it holds one.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture made without the sampled use; [`Error::Full`] when no
    /// index is left.
    pub fn bind_texture(&mut self, t: Texture) -> Result<u32, Error> {
        on!(&mut self.b, d => d.bind_texture(t))
    }

    /// Give sampler `s` an index in the table, the one it holds if it holds one.
    ///
    /// # Errors
    /// [`Error::Full`] when no index is left.
    pub fn bind_sampler(&mut self, s: Sampler) -> Result<u32, Error> {
        on!(&mut self.b, d => d.bind_sampler(s))
    }

    /// Give storage buffer `b` an index in the table, read and written alike.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer made without the storage use; [`Error::Full`] when no
    /// index is left.
    pub fn bind_buffer(&mut self, b: Buffer) -> Result<u32, Error> {
        on!(&mut self.b, d => d.bind_buffer(b))
    }

    /// Make a graphics pipeline. Viewport and scissor are set while drawing; vertices are pulled
    /// by the vertex shader.
    ///
    /// # Errors
    /// Any failure of the driver, which is how a shader the device rejects is reported.
    pub fn create_graphics_pipeline(&mut self, desc: &GraphicsDesc<'_>) -> Result<Pipeline, Error> {
        on!(&mut self.b, d => d.create_graphics_pipeline(desc))
    }

    /// Make a compute pipeline.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn create_compute_pipeline(&mut self, desc: &ComputeDesc<'_>) -> Result<Pipeline, Error> {
        on!(&mut self.b, d => d.create_compute_pipeline(desc))
    }

    /// Destroy a pipeline once the work recorded so far is done with it.
    pub fn destroy_pipeline(&mut self, p: Pipeline) {
        on!(&mut self.b, d => d.destroy_pipeline(p));
    }

    /// Begin a recording of the device's own.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn begin(&mut self) -> Result<Recorder<'_>, Error> {
        Ok(Recorder {
            b: match &mut self.b {
                Dev::Vk(d) => Rec::Vk(d.begin()?),
                #[cfg(windows)]
                Dev::Dx(d) => Rec::Dx(d.begin()?),
            },
        })
    }

    /// Record into `cmd`, a Vulkan command buffer an adopted device's owner lent for its frame
    /// `clock` names, outside any rendering. The owner submits it.
    ///
    /// # Safety
    /// `cmd` must be recording, on the adopted device, outside a render pass, and submitted by
    /// its owner after the recording is finished. Every frame up to `clock.stamp -
    /// clock.in_flight` must have finished on the device.
    ///
    /// # Errors
    /// [`Error::Invalid`] on a Direct3D 12 device; any failure of the driver.
    pub unsafe fn record_into(
        &mut self,
        cmd: vk::CommandBuffer,
        clock: FrameClock,
    ) -> Result<Recorder<'_>, Error> {
        match &mut self.b {
            Dev::Vk(d) => Ok(Recorder {
                // SAFETY: the caller's obligations are the backend's.
                b: Rec::Vk(unsafe { d.record_into(cmd, clock) }?),
            }),
            #[cfg(windows)]
            Dev::Dx(_) => Err(Error::Invalid(
                "a Vulkan command buffer lent to a Direct3D 12 device".into(),
            )),
        }
    }

    /// Record into `list`, a Direct3D 12 command list an adopted device's owner lent for its
    /// frame `clock` names. The owner closes and submits it; the layer sets its own root
    /// signature and descriptor heaps on it, which the owner sets back before drawing on.
    ///
    /// # Safety
    /// `list` must be open on the adopted device and submitted by its owner after the recording
    /// is finished. Every frame up to `clock.stamp - clock.in_flight` must have finished.
    ///
    /// # Errors
    /// [`Error::Invalid`] on a Vulkan device; any failure of the driver.
    #[cfg(windows)]
    pub unsafe fn record_into_d3d12(
        &mut self,
        list: &windows::Win32::Graphics::Direct3D12::ID3D12GraphicsCommandList,
        clock: FrameClock,
    ) -> Result<Recorder<'_>, Error> {
        match &mut self.b {
            Dev::Vk(_) => Err(Error::Invalid(
                "a Direct3D 12 command list lent to a Vulkan device".into(),
            )),
            Dev::Dx(d) => Ok(Recorder {
                // SAFETY: the caller's obligations are the backend's.
                b: Rec::Dx(unsafe { d.record_into(list, clock) }?),
            }),
        }
    }

    /// One level of `t`, read back: tightly packed rows. Submits and waits for a recording of
    /// its own.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold; any failure of the driver.
    pub fn read_texture(&mut self, t: Texture, level: u32) -> Result<Vec<u8>, Error> {
        on!(&mut self.b, d => d.read_texture(t, level))
    }

    /// The spans the last finished recording's marks measured.
    #[must_use]
    pub fn timings(&mut self) -> Vec<Timing> {
        on!(&mut self.b, d => d.timings())
    }
}

/// One recording: a command buffer or list of the device's own, or one an adopted device's owner
/// lent.
#[derive(Debug)]
pub struct Recorder<'d> {
    pub(crate) b: Rec<'d>,
}

#[derive(Debug)]
pub(crate) enum Rec<'d> {
    Vk(crate::recorder::Recorder<'d>),
    #[cfg(windows)]
    Dx(crate::d3d12::Recorder<'d>),
}

/// Acceleration structures, where the device traces rays ([`crate::rt`]).
impl Device {
    /// Make a bottom-level structure of `geometries`, built at the head of the next recording.
    ///
    /// # Errors
    /// See [`crate::device::Device::create_blas`].
    pub fn create_blas(
        &mut self,
        label: &str,
        geometries: &[crate::rt::BlasGeometry],
    ) -> Result<crate::rt::Blas, Error> {
        match &mut self.b {
            Dev::Vk(d) => d.create_blas(label, geometries),
            #[cfg(windows)]
            Dev::Dx(d) => d.create_blas(label, geometries),
        }
    }

    /// Destroy a bottom-level structure once the work recorded so far is done with it.
    pub fn destroy_blas(&mut self, b: crate::rt::Blas) {
        match &mut self.b {
            Dev::Vk(d) => d.destroy_blas(b),
            #[cfg(windows)]
            Dev::Dx(d) => d.destroy_blas(b),
        }
    }

    /// Destroy a top-level structure once the work recorded so far is done with it.
    pub fn destroy_tlas(&mut self, t: crate::rt::Tlas) {
        match &mut self.b {
            Dev::Vk(d) => d.destroy_tlas(t),
            #[cfg(windows)]
            Dev::Dx(d) => d.destroy_tlas(t),
        }
    }

    /// Whether a bottom-level structure has been built.
    ///
    /// # Errors
    /// See [`crate::device::Device::blas_built`].
    pub fn blas_built(&self, b: crate::rt::Blas) -> Result<bool, Error> {
        match &self.b {
            Dev::Vk(d) => d.blas_built(b),
            #[cfg(windows)]
            Dev::Dx(d) => d.blas_built(b),
        }
    }

    /// A bottom-level structure's address, as an instance refers to it.
    ///
    /// # Errors
    /// See [`crate::device::Device::blas_address`].
    pub fn blas_address(&self, b: crate::rt::Blas) -> Result<u64, Error> {
        match &self.b {
            Dev::Vk(d) => d.blas_address(b),
            #[cfg(windows)]
            Dev::Dx(d) => d.blas_address(b),
        }
    }

    /// Make a top-level structure with room for `capacity` instances.
    ///
    /// # Errors
    /// See [`crate::device::Device::create_tlas`].
    pub fn create_tlas(&mut self, label: &str, capacity: u32) -> Result<crate::rt::Tlas, Error> {
        match &mut self.b {
            Dev::Vk(d) => d.create_tlas(label, capacity),
            #[cfg(windows)]
            Dev::Dx(d) => d.create_tlas(label, capacity),
        }
    }

    /// The index shaders reach a top-level structure at.
    ///
    /// # Errors
    /// See [`crate::device::Device::bind_tlas`].
    pub fn bind_tlas(&mut self, t: crate::rt::Tlas) -> Result<u32, Error> {
        match &mut self.b {
            Dev::Vk(d) => d.bind_tlas(t),
            #[cfg(windows)]
            Dev::Dx(d) => d.bind_tlas(t),
        }
    }
}

/// Call `$call` on whichever backend the recording `$rec` is.
macro_rules! rec {
    ($rec:expr, $r:ident => $call:expr) => {
        match $rec {
            Rec::Vk($r) => $call,
            #[cfg(windows)]
            Rec::Dx($r) => $call,
        }
    };
}

/// A texture's Vulkan image, its view, its format and its size.
pub type RawTexture = (vk::Image, vk::ImageView, vk::Format, (u32, u32));

impl Recorder<'_> {
    /// The Vulkan command buffer being recorded, for what the layer does not wrap.
    #[must_use]
    pub fn raw(&self) -> Option<vk::CommandBuffer> {
        match &self.b {
            Rec::Vk(r) => Some(r.raw()),
            #[cfg(windows)]
            Rec::Dx(_) => None,
        }
    }

    /// The backend recorded with.
    #[must_use]
    pub fn backend(&self) -> Backend {
        match &self.b {
            Rec::Vk(_) => Backend::Vulkan,
            #[cfg(windows)]
            Rec::Dx(_) => Backend::D3d12,
        }
    }

    /// Destroy buffer `b` once the work recorded so far, this recording's included, is done.
    pub fn destroy_buffer(&mut self, b: Buffer) {
        rec!(&mut self.b, r => r.device().destroy_buffer(b));
    }

    /// Write a mark named `label`: the span from it to the next mark is measured. Nothing on a
    /// device whose queue writes no timestamps, or past [`crate::MARKS`] marks.
    pub fn timestamp(&mut self, label: &'static str) {
        rec!(&mut self.b, r => r.timestamp(label));
    }

    /// Put in the barriers `touches` need, now.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold.
    pub fn touch(&mut self, touches: &[Touch]) -> Result<(), Error> {
        rec!(&mut self.b, r => r.touch(touches))
    }

    /// Record a raster pass: its barriers, its targets begun as `pass` says, `draw` inside, then
    /// its targets ended.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a pass with no target, or a resource the device does not hold.
    pub fn raster(
        &mut self,
        pass: &RasterPass<'_>,
        draw: impl FnOnce(&mut Draw<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        rec!(&mut self.b, r => r.raster(pass, draw))
    }

    /// Record a compute pass: its barriers, then `dispatch` inside.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold, or what `dispatch` returns.
    pub fn compute(
        &mut self,
        touches: &[Touch],
        dispatch: impl FnOnce(&mut Draw<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        rec!(&mut self.b, r => r.compute(touches, dispatch))
    }

    /// Fill `size` bytes of `b` from `offset` with copies of `value`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn fill_buffer(
        &mut self,
        b: Buffer,
        offset: u64,
        size: u64,
        value: u32,
    ) -> Result<(), Error> {
        rec!(&mut self.b, r => r.fill_buffer(b, offset, size, value))
    }

    /// Copy `size` bytes of `src` from `src_offset` into `dst` at `dst_offset`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn copy_buffer(
        &mut self,
        src: Buffer,
        src_offset: u64,
        dst: Buffer,
        dst_offset: u64,
        size: u64,
    ) -> Result<(), Error> {
        rec!(&mut self.b, r => r.copy_buffer(src, src_offset, dst, dst_offset, size))
    }

    /// Copy one level of `t` into `dst` at `offset`, tightly packed.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold.
    pub fn copy_texture_to_buffer(
        &mut self,
        t: Texture,
        level: u32,
        dst: Buffer,
        offset: u64,
    ) -> Result<(), Error> {
        rec!(&mut self.b, r => r.copy_texture_to_buffer(t, level, dst, offset))
    }

    /// Copy `src`'s first level into `dst`'s, every texel: two textures of one size and format.
    ///
    /// # Errors
    /// [`Error::Invalid`] for textures of different sizes or formats; a handle that is not live.
    pub fn copy_texture(&mut self, src: Texture, dst: Texture) -> Result<(), Error> {
        rec!(&mut self.b, r => r.copy_texture(src, dst))
    }

    /// End and submit a recording of the device's own.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a recording into a lent command buffer; any failure of the driver.
    pub fn submit(self) -> Result<Ticket, Error> {
        rec!(self.b, r => r.submit())
    }

    /// Finish a recording into a lent command buffer: what it holds goes when the owner's frame
    /// is done. A recording of the device's own is submitted.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn finish(self) -> Result<(), Error> {
        rec!(self.b, r => r.finish())
    }
}

/// Inside a pass: the pipeline, push constants, viewport and the draws or dispatches.
#[derive(Debug)]
pub struct Draw<'r> {
    pub(crate) b: DrawB<'r>,
}

#[derive(Debug)]
pub(crate) enum DrawB<'r> {
    Vk(crate::recorder::Draw<'r>),
    #[cfg(windows)]
    Dx(crate::d3d12::Draw<'r>),
}

/// Call `$call` on whichever backend the draw `$draw` is.
macro_rules! draw {
    ($draw:expr, $d:ident => $call:expr) => {
        match $draw {
            DrawB::Vk($d) => $call,
            #[cfg(windows)]
            DrawB::Dx($d) => $call,
        }
    };
}

/// Acceleration structures, where the device traces rays ([`crate::rt`]).
impl Recorder<'_> {
    /// The index shaders reach `t` at, which a build that grew it changes.
    ///
    /// # Errors
    /// See [`Device::bind_tlas`].
    pub fn bind_tlas(&mut self, t: crate::rt::Tlas) -> Result<u32, Error> {
        rec!(&mut self.b, r => r.device().bind_tlas(t))
    }

    /// Build `t` from `instances`, now.
    ///
    /// # Errors
    /// See [`crate::recorder::Recorder::build_tlas`].
    pub fn build_tlas(
        &mut self,
        t: crate::rt::Tlas,
        instances: &[crate::rt::TlasInstance],
    ) -> Result<(), Error> {
        match &mut self.b {
            Rec::Vk(r) => r.build_tlas(t, instances),
            #[cfg(windows)]
            Rec::Dx(r) => r.build_tlas(t, instances),
        }
    }

    /// Build `t` from `count` instances a shader wrote into `buffer` from `offset`, now.
    ///
    /// # Errors
    /// See [`crate::recorder::Recorder::build_tlas_from`].
    pub fn build_tlas_from(
        &mut self,
        t: crate::rt::Tlas,
        buffer: Buffer,
        offset: u64,
        count: u32,
    ) -> Result<(), Error> {
        match &mut self.b {
            Rec::Vk(r) => r.build_tlas_from(t, buffer, offset, count),
            #[cfg(windows)]
            Rec::Dx(r) => r.build_tlas_from(t, buffer, offset, count),
        }
    }
}

impl Draw<'_> {
    /// Use pipeline `p` for what follows. The bindless table is bound with the first.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a pipeline the device does not hold, or one of the other kind.
    pub fn set_pipeline(&mut self, p: Pipeline) -> Result<(), Error> {
        draw!(&mut self.b, d => d.set_pipeline(p))
    }

    /// Push `bytes` at `offset` into the push constants every stage reads.
    ///
    /// # Errors
    /// [`Error::Invalid`] past [`crate::PUSH_BYTES`].
    pub fn push(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Error> {
        draw!(&mut self.b, d => d.push(offset, bytes))
    }

    /// Draw into `r` of the targets. Clip space is the ordinary renderer's: y up, z from 0 to 1.
    pub fn set_viewport(&mut self, r: Rect) {
        draw!(&mut self.b, d => d.set_viewport(r));
    }

    /// Draw only inside `r`.
    pub fn set_scissor(&mut self, r: Rect) {
        draw!(&mut self.b, d => d.set_scissor(r));
    }

    /// Take indices from `b` at `offset`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn bind_index_buffer(
        &mut self,
        b: Buffer,
        offset: u64,
        ty: IndexType,
    ) -> Result<(), Error> {
        draw!(&mut self.b, d => d.bind_index_buffer(b, offset, ty))
    }

    /// Draw `vertices` vertices, `instances` times; `first_instance` is the draw's item.
    pub fn draw(&mut self, vertices: u32, instances: u32, first_vertex: u32, first_instance: u32) {
        draw!(&mut self.b, d => d.draw(vertices, instances, first_vertex, first_instance));
    }

    /// Draw `indices` indices, `instances` times; `first_instance` is the draw's item.
    pub fn draw_indexed(
        &mut self,
        indices: u32,
        instances: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    ) {
        draw!(&mut self.b, d => d.draw_indexed(indices, instances, first_index, vertex_offset, first_instance));
    }

    /// `count` draws from the commands in `b` at `offset`, `stride` bytes apart, each in the
    /// device's layout ([`Backend::command`]).
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn draw_indirect(
        &mut self,
        b: Buffer,
        offset: u64,
        count: u32,
        stride: u32,
    ) -> Result<(), Error> {
        draw!(&mut self.b, d => d.draw_indirect(b, offset, count, stride))
    }

    /// `count` indexed draws from the commands in `b` at `offset`, `stride` bytes apart, each in
    /// the device's layout ([`Backend::indexed_command`]).
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn draw_indexed_indirect(
        &mut self,
        b: Buffer,
        offset: u64,
        count: u32,
        stride: u32,
    ) -> Result<(), Error> {
        draw!(&mut self.b, d => d.draw_indexed_indirect(b, offset, count, stride))
    }

    /// The indexed draws in `b` at `offset`, as many as the count in `count_buffer` at
    /// `count_offset` says and at most `max`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_indirect_count(
        &mut self,
        b: Buffer,
        offset: u64,
        count_buffer: Buffer,
        count_offset: u64,
        max: u32,
        stride: u32,
    ) -> Result<(), Error> {
        draw!(&mut self.b, d => d.draw_indexed_indirect_count(b, offset, count_buffer, count_offset, max, stride))
    }

    /// The draws in `b` at `offset`, as many as the count in `count_buffer` at `count_offset`
    /// says and at most `max`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn draw_indirect_count(
        &mut self,
        b: Buffer,
        offset: u64,
        count_buffer: Buffer,
        count_offset: u64,
        max: u32,
        stride: u32,
    ) -> Result<(), Error> {
        draw!(&mut self.b, d => d.draw_indirect_count(b, offset, count_buffer, count_offset, max, stride))
    }

    /// Run `x * y * z` workgroups of the bound compute pipeline.
    pub fn dispatch(&mut self, x: u32, y: u32, z: u32) {
        draw!(&mut self.b, d => d.dispatch(x, y, z));
    }
}
