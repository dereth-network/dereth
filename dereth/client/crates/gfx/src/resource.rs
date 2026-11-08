//! Buffers, textures, samplers and the staging memory uploads pass through.

use ash::vk;
use gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme};
use gpu_allocator::MemoryLocation;

use crate::device::{vkr, Device, Garbage};
use crate::graph::{State, Track};
use crate::upload::PendingCopy;
use crate::{
    Address, Buffer, BufferDesc, BufferUsage, Error, Filter, Format, Location, Sampler,
    SamplerDesc, Texture, TextureDesc, TextureUsage,
};

/// A table of live entries by index, with the indices of removed ones reused.
#[derive(Debug)]
pub(crate) struct Slots<T> {
    items: Vec<Option<T>>,
    free: Vec<u32>,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl<T> Slots<T> {
    pub(crate) fn insert(&mut self, item: T) -> u32 {
        if let Some(i) = self.free.pop() {
            self.items[i as usize] = Some(item);
            i
        } else {
            self.items.push(Some(item));
            #[allow(clippy::cast_possible_truncation)] // far fewer than 2^32 resources
            let i = (self.items.len() - 1) as u32;
            i
        }
    }

    pub(crate) fn get(&self, i: u32) -> Option<&T> {
        self.items.get(i as usize).and_then(Option::as_ref)
    }

    pub(crate) fn get_mut(&mut self, i: u32) -> Option<&mut T> {
        self.items.get_mut(i as usize).and_then(Option::as_mut)
    }

    pub(crate) fn remove(&mut self, i: u32) -> Option<T> {
        let item = self.items.get_mut(i as usize)?.take();
        if item.is_some() {
            self.free.push(i);
        }
        item
    }

    pub(crate) fn drain(&mut self) -> Vec<T> {
        self.free.clear();
        std::mem::take(&mut self.items)
            .into_iter()
            .flatten()
            .collect()
    }

    pub(crate) fn len(&self) -> usize {
        self.items.iter().filter(|i| i.is_some()).count()
    }
}

/// A live buffer.
pub(crate) struct BufferEntry {
    pub buffer: vk::Buffer,
    pub allocation: Option<Allocation>,
    pub size: u64,
    pub location: Location,
    pub usage: BufferUsage,
    pub track: Track,
    /// Its index in the bindless table, while it holds one.
    pub bindless: Option<u32>,
}

/// A live texture, or an image lent to the layer.
pub(crate) struct TextureEntry {
    pub image: vk::Image,
    pub view: vk::ImageView,
    /// The view shaders sample: the depth aspect alone of a depth-and-stencil image.
    pub sample_view: vk::ImageView,
    pub allocation: Option<Allocation>,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    pub track: Track,
    pub bindless: Option<u32>,
    /// Whether the image is lent: the layer destroys neither it nor its views.
    pub lent: bool,
}

/// A live sampler.
pub(crate) struct SamplerEntry {
    pub sampler: vk::Sampler,
    pub bindless: Option<u32>,
}

/// One host-visible buffer staged uploads are bump-allocated from.
pub(crate) struct Arena {
    pub buffer: vk::Buffer,
    pub allocation: Option<Allocation>,
    pub capacity: u64,
    pub used: u64,
}

/// The staging arenas: the one being filled, those filled for work not yet recorded, and those
/// free to fill again.
#[derive(Default)]
pub(crate) struct Staging {
    pub current: Vec<Arena>,
    pub free: Vec<Arena>,
}

impl Staging {
    pub(crate) fn into_arenas(self) -> Vec<Arena> {
        self.current.into_iter().chain(self.free).collect()
    }
}

/// The smallest staging arena.
const ARENA_BYTES: u64 = 8 << 20;

impl Device {
    fn alloc(
        &mut self,
        name: &str,
        requirements: vk::MemoryRequirements,
        location: MemoryLocation,
        linear: bool,
    ) -> Result<Allocation, Error> {
        self.allocator
            .as_mut()
            .expect("the allocator lives as long as the device")
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .map_err(|e| Error::Allocation(format!("{name}: {e}")))
    }

    pub(crate) fn release_allocation(&mut self, a: Option<Allocation>) {
        if let (Some(a), Some(al)) = (a, self.allocator.as_mut()) {
            let _ = al.free(a);
        }
    }

    /// A raw buffer and its memory.
    pub(crate) fn raw_buffer(
        &mut self,
        name: &str,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Result<(vk::Buffer, Allocation), Error> {
        let info = vk::BufferCreateInfo::default()
            .size(size.max(4))
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // SAFETY: a live local; the buffer is owned by the caller.
        let buffer = vkr("vkCreateBuffer", unsafe {
            self.device.create_buffer(&info, None)
        })?;
        // SAFETY: a pure query on the new buffer.
        let req = unsafe { self.device.get_buffer_memory_requirements(buffer) };
        let allocation = match self.alloc(name, req, location, true) {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: unbound and unreferenced.
                unsafe { self.device.destroy_buffer(buffer, None) };
                return Err(e);
            }
        };
        // SAFETY: the allocation meets the buffer's requirements.
        if let Err(e) = unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
        } {
            self.release_allocation(Some(allocation));
            // SAFETY: unbound and unreferenced.
            unsafe { self.device.destroy_buffer(buffer, None) };
            return Err(Error::Vulkan {
                what: "vkBindBufferMemory",
                result: e,
            });
        }
        Ok((buffer, allocation))
    }

    /// Make a buffer.
    ///
    /// # Errors
    /// Any failure of the driver or the allocator.
    pub fn create_buffer(&mut self, desc: &BufferDesc<'_>) -> Result<Buffer, Error> {
        let mut usage = vk::BufferUsageFlags::empty();
        for (ours, vks) in [
            (BufferUsage::INDEX, vk::BufferUsageFlags::INDEX_BUFFER),
            (BufferUsage::INDIRECT, vk::BufferUsageFlags::INDIRECT_BUFFER),
            (BufferUsage::STORAGE, vk::BufferUsageFlags::STORAGE_BUFFER),
            (
                BufferUsage::TRANSFER_SRC,
                vk::BufferUsageFlags::TRANSFER_SRC,
            ),
            (
                BufferUsage::TRANSFER_DST,
                vk::BufferUsageFlags::TRANSFER_DST,
            ),
            (BufferUsage::VERTEX, vk::BufferUsageFlags::VERTEX_BUFFER),
        ] {
            if desc.usage.contains(ours) {
                usage |= vks;
            }
        }
        // Read by acceleration structure builds, by its address: only where the device traces
        // rays, and a plain buffer elsewhere.
        if desc.usage.contains(BufferUsage::ACCELERATION_INPUT) && self.rt.is_some() {
            usage |= vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR;
        }
        // A device-local buffer is written by staged copies.
        if desc.location == Location::Gpu {
            usage |= vk::BufferUsageFlags::TRANSFER_DST;
        }
        let location = match desc.location {
            Location::Gpu => MemoryLocation::GpuOnly,
            Location::Upload => MemoryLocation::CpuToGpu,
            Location::Readback => MemoryLocation::GpuToCpu,
        };
        let (buffer, allocation) = self.raw_buffer(desc.label, desc.size, usage, location)?;
        let id = self.buffers.insert(BufferEntry {
            buffer,
            allocation: Some(allocation),
            size: desc.size,
            location: desc.location,
            usage: desc.usage,
            track: Track::new(),
            bindless: None,
        });
        self.stats.buffers += 1;
        Ok(Buffer(id))
    }

    /// Destroy a buffer once the work recorded so far is done with it.
    pub fn destroy_buffer(&mut self, b: Buffer) {
        if let Some(e) = self.buffers.remove(b.0) {
            if let Some(i) = e.bindless {
                self.defer(Garbage::BufferIndex(i));
            }
            self.stats.buffers -= 1;
            self.defer(Garbage::Buffer(e));
        }
    }

    pub(crate) fn destroy_buffer_now(&mut self, mut e: BufferEntry) {
        let a = e.allocation.take();
        self.release_allocation(a);
        // SAFETY: the work that used it is done.
        unsafe { self.device.destroy_buffer(e.buffer, None) };
    }

    pub(crate) fn buffer(&self, b: Buffer) -> Result<&BufferEntry, Error> {
        self.buffers
            .get(b.0)
            .ok_or_else(|| Error::Invalid(format!("no buffer {}", b.0)))
    }

    pub(crate) fn texture(&self, t: Texture) -> Result<&TextureEntry, Error> {
        self.textures
            .get(t.0)
            .ok_or_else(|| Error::Invalid(format!("no texture {}", t.0)))
    }

    /// A buffer's size, bytes.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold.
    pub fn buffer_size(&self, b: Buffer) -> Result<u64, Error> {
        Ok(self.buffer(b)?.size)
    }

    /// Write `data` into `b` at `offset`: straight into its memory when the processor can reach
    /// it, else staged and copied at the head of the next recording.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a write past the buffer's end, or a failure to stage it.
    pub fn write_buffer(&mut self, b: Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        let e = self.buffer(b)?;
        let len = data.len() as u64;
        let (size, location) = (e.size, e.location);
        if offset + len > size {
            return Err(Error::Invalid(format!(
                "a write of {len} bytes at {offset} into a buffer of {size}"
            )));
        }
        if data.is_empty() {
            return Ok(());
        }
        self.stats.uploaded_bytes += len;
        if location == Location::Gpu {
            let (src, src_offset) = self.stage(data)?;
            self.pending.push(PendingCopy::Buffer {
                src,
                src_offset,
                dst: b,
                dst_offset: offset,
                size: len,
            });
            return Ok(());
        }
        let e = self
            .buffers
            .get_mut(b.0)
            .ok_or_else(|| Error::Invalid("no buffer".into()))?;
        let slice = e
            .allocation
            .as_mut()
            .and_then(Allocation::mapped_slice_mut)
            .ok_or_else(|| Error::Invalid("the buffer is not mapped".into()))?;
        #[allow(clippy::cast_possible_truncation)] // inside a mapped allocation
        slice[offset as usize..(offset + len) as usize].copy_from_slice(data);
        Ok(())
    }

    /// Copy the first `size` bytes of `src` into `dst`, at the head of the next recording, after
    /// every staged write queued before it: how a pool is moved into a bigger buffer. `src` must
    /// not be destroyed before that recording is begun.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold, or a size past either's end.
    pub fn copy_buffer_later(&mut self, src: Buffer, dst: Buffer, size: u64) -> Result<(), Error> {
        if size > self.buffer(src)?.size || size > self.buffer(dst)?.size {
            return Err(Error::Invalid("a copy past a buffer's end".into()));
        }
        self.pending.push(PendingCopy::Device { src, dst, size });
        Ok(())
    }

    /// Read `len` bytes of a read-back buffer at `offset`. The device must have finished the work
    /// that wrote them, and made them visible to the processor ([`crate::Use::HostRead`]).
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer that is not a read-back one, or a read past its end.
    pub fn read_buffer(&self, b: Buffer, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        let e = self.buffer(b)?;
        if offset + len > e.size {
            return Err(Error::Invalid("a read past the buffer's end".into()));
        }
        let slice = e
            .allocation
            .as_ref()
            .and_then(Allocation::mapped_slice)
            .ok_or_else(|| Error::Invalid("the buffer is not mapped".into()))?;
        #[allow(clippy::cast_possible_truncation)] // inside a mapped allocation
        Ok(slice[offset as usize..(offset + len) as usize].to_vec())
    }

    /// Copy `data` into the staging memory, and where it landed.
    pub(crate) fn stage(&mut self, data: &[u8]) -> Result<(vk::Buffer, u64), Error> {
        const ALIGN: u64 = 16;
        let len = data.len() as u64;
        let fits = |a: &Arena| a.used.next_multiple_of(ALIGN) + len <= a.capacity;
        if !self.staging.current.last().is_some_and(fits) {
            let reuse = self.staging.free.iter().position(|a| a.capacity >= len);
            let arena = match reuse {
                Some(i) => {
                    let mut a = self.staging.free.swap_remove(i);
                    a.used = 0;
                    a
                }
                None => {
                    let capacity = len.max(ARENA_BYTES).next_power_of_two();
                    let (buffer, allocation) = self.raw_buffer(
                        "staging",
                        capacity,
                        vk::BufferUsageFlags::TRANSFER_SRC,
                        MemoryLocation::CpuToGpu,
                    )?;
                    Arena {
                        buffer,
                        allocation: Some(allocation),
                        capacity,
                        used: 0,
                    }
                }
            };
            self.staging.current.push(arena);
        }
        let arena = self
            .staging
            .current
            .last_mut()
            .expect("an arena was just made current");
        let offset = arena.used.next_multiple_of(ALIGN);
        let slice = arena
            .allocation
            .as_mut()
            .and_then(Allocation::mapped_slice_mut)
            .ok_or_else(|| Error::Invalid("staging memory is not mapped".into()))?;
        #[allow(clippy::cast_possible_truncation)] // inside a mapped allocation
        slice[offset as usize..(offset + len) as usize].copy_from_slice(data);
        arena.used = offset + len;
        Ok((arena.buffer, offset))
    }

    pub(crate) fn destroy_arena(&mut self, mut a: Arena) {
        let al = a.allocation.take();
        self.release_allocation(al);
        // SAFETY: the work that used it is done.
        unsafe { self.device.destroy_buffer(a.buffer, None) };
    }

    /// Make a 2D texture.
    ///
    /// # Errors
    /// [`Error::Unsupported`] for block compression on a device without it; any failure of the
    /// driver or the allocator.
    pub fn create_texture(&mut self, desc: &TextureDesc<'_>) -> Result<Texture, Error> {
        if desc.format.is_block() && !self.caps.block_compression {
            return Err(Error::Unsupported(
                "the device does not sample block-compressed textures".into(),
            ));
        }
        let mut usage = vk::ImageUsageFlags::empty();
        for (ours, vks) in [
            (TextureUsage::SAMPLED, vk::ImageUsageFlags::SAMPLED),
            (
                TextureUsage::COLOR_TARGET,
                vk::ImageUsageFlags::COLOR_ATTACHMENT,
            ),
            (TextureUsage::STORAGE, vk::ImageUsageFlags::STORAGE),
            (
                TextureUsage::DEPTH_TARGET,
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            ),
            (
                TextureUsage::TRANSFER_SRC,
                vk::ImageUsageFlags::TRANSFER_SRC,
            ),
            (
                TextureUsage::TRANSFER_DST,
                vk::ImageUsageFlags::TRANSFER_DST,
            ),
        ] {
            if desc.usage.contains(ours) {
                usage |= vks;
            }
        }
        // A sampled texture is filled by staged copies.
        if desc.usage.contains(TextureUsage::SAMPLED) && !desc.format.is_depth() {
            usage |= vk::ImageUsageFlags::TRANSFER_DST;
        }
        let mips = desc.mips.max(1);
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(desc.format.vk())
            .extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            })
            .mip_levels(mips)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY: a live local; the image is owned by the entry.
        let image = vkr("vkCreateImage", unsafe {
            self.device.create_image(&info, None)
        })?;
        // SAFETY: a pure query on the new image.
        let req = unsafe { self.device.get_image_memory_requirements(image) };
        let allocation = match self.alloc(desc.label, req, MemoryLocation::GpuOnly, false) {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: unbound and unreferenced.
                unsafe { self.device.destroy_image(image, None) };
                return Err(e);
            }
        };
        // SAFETY: the allocation meets the image's requirements.
        if let Err(e) = unsafe {
            self.device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
        } {
            self.release_allocation(Some(allocation));
            // SAFETY: unbound and unreferenced.
            unsafe { self.device.destroy_image(image, None) };
            return Err(Error::Vulkan {
                what: "vkBindImageMemory",
                result: e,
            });
        }
        // A texture only copied to and from has no view: a view needs a use that reads or draws
        // through one.
        let viewed = desc.usage.contains(TextureUsage::SAMPLED)
            || desc.usage.contains(TextureUsage::STORAGE)
            || desc.usage.contains(TextureUsage::COLOR_TARGET)
            || desc.usage.contains(TextureUsage::DEPTH_TARGET);
        let views = if viewed {
            self.views(image, desc.format, mips)
        } else {
            Ok((vk::ImageView::null(), vk::ImageView::null()))
        };
        let (view, sample_view) = match views {
            Ok(v) => v,
            Err(e) => {
                self.release_allocation(Some(allocation));
                // SAFETY: unreferenced.
                unsafe { self.device.destroy_image(image, None) };
                return Err(e);
            }
        };
        let id = self.textures.insert(TextureEntry {
            image,
            view,
            sample_view,
            allocation: Some(allocation),
            format: desc.format,
            width: desc.width,
            height: desc.height,
            mips,
            track: Track::new(),
            bindless: None,
            lent: false,
        });
        self.stats.textures += 1;
        Ok(Texture(id))
    }

    /// The image's view of every aspect, and the view shaders sample.
    fn views(
        &self,
        image: vk::Image,
        format: Format,
        mips: u32,
    ) -> Result<(vk::ImageView, vk::ImageView), Error> {
        let make = |aspect: vk::ImageAspectFlags| {
            let info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.vk())
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: aspect,
                    base_mip_level: 0,
                    level_count: mips,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            // SAFETY: the image is live and bound.
            vkr("vkCreateImageView", unsafe {
                self.device.create_image_view(&info, None)
            })
        };
        let view = make(format.aspect())?;
        if format.has_stencil() {
            match make(vk::ImageAspectFlags::DEPTH) {
                Ok(s) => Ok((view, s)),
                Err(e) => {
                    // SAFETY: unreferenced.
                    unsafe { self.device.destroy_image_view(view, None) };
                    Err(e)
                }
            }
        } else {
            Ok((view, view))
        }
    }

    /// Destroy a texture once the work recorded so far is done with it. A lent image is only
    /// forgotten.
    pub fn destroy_texture(&mut self, t: Texture) {
        if let Some(e) = self.textures.remove(t.0) {
            if let Some(i) = e.bindless {
                self.defer(Garbage::TextureIndex(i));
            }
            if !e.lent {
                self.stats.textures -= 1;
                self.defer(Garbage::Texture(e));
            }
        }
    }

    pub(crate) fn destroy_texture_now(&mut self, mut e: TextureEntry) {
        if e.lent {
            return;
        }
        let a = e.allocation.take();
        self.release_allocation(a);
        // SAFETY: the work that used it is done.
        unsafe {
            if e.sample_view != e.view {
                self.device.destroy_image_view(e.sample_view, None);
            }
            self.device.destroy_image_view(e.view, None);
            self.device.destroy_image(e.image, None);
        }
    }

    /// Lend the layer an image its owner keeps: its view, format and extent, and the state its
    /// owner left it in. The layer tracks it like its own texture until it is forgotten
    /// ([`Self::destroy_texture`]), and destroys neither it nor its view.
    ///
    /// # Safety
    /// The image and view must be live while the layer holds them, and `state` must be the
    /// state the owner's work left the image in.
    pub unsafe fn lend_texture(&mut self, lent: &Lent) -> Texture {
        let id = self.textures.insert(TextureEntry {
            image: lent.image,
            view: lent.view,
            sample_view: lent.view,
            allocation: None,
            format: lent.format,
            width: lent.width,
            height: lent.height,
            mips: 1,
            track: Track::lent(lent.state),
            bindless: None,
            lent: true,
        });
        Texture(id)
    }

    /// A texture's extent.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold.
    pub fn texture_size(&self, t: Texture) -> Result<(u32, u32), Error> {
        let e = self.texture(t)?;
        Ok((e.width, e.height))
    }

    /// Upload one level of a sampled texture: staged, copied at the head of the next recording,
    /// and left ready to sample.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a level past the texture's, or bytes that are not the level's size.
    pub fn upload_texture(&mut self, t: Texture, level: u32, data: &[u8]) -> Result<(), Error> {
        let e = self.texture(t)?;
        if level >= e.mips {
            return Err(Error::Invalid(format!("level {level} of {}", e.mips)));
        }
        let (w, h) = ((e.width >> level).max(1), (e.height >> level).max(1));
        let want = e.format.level_bytes(w, h);
        if data.len() as u64 != want {
            return Err(Error::Invalid(format!(
                "level {level} is {want} bytes, not {}",
                data.len()
            )));
        }
        self.stats.uploaded_bytes += want;
        let (src, src_offset) = self.stage(data)?;
        self.pending.push(PendingCopy::Texture {
            src,
            src_offset,
            dst: t,
            level,
            width: w,
            height: h,
        });
        Ok(())
    }

    /// Make a sampler.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<Sampler, Error> {
        let filter = |f: Filter| match f {
            Filter::Nearest => vk::Filter::NEAREST,
            Filter::Linear => vk::Filter::LINEAR,
        };
        let address = |a: Address| match a {
            Address::Repeat => vk::SamplerAddressMode::REPEAT,
            Address::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        };
        #[allow(clippy::cast_precision_loss)] // a small anisotropy
        let aniso = (desc.anisotropy.max(1) as f32).min(self.caps.max_anisotropy);
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(filter(desc.filter))
            .min_filter(filter(desc.filter))
            .mipmap_mode(match desc.mip {
                Filter::Nearest => vk::SamplerMipmapMode::NEAREST,
                Filter::Linear => vk::SamplerMipmapMode::LINEAR,
            })
            .address_mode_u(address(desc.address))
            .address_mode_v(address(desc.address_v))
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .mip_lod_bias(desc.lod_bias)
            .anisotropy_enable(aniso > 1.0)
            .max_anisotropy(aniso)
            .min_lod(0.0)
            .max_lod(vk::LOD_CLAMP_NONE);
        // SAFETY: a live local; owned by the entry.
        let sampler = vkr("vkCreateSampler", unsafe {
            self.device.create_sampler(&info, None)
        })?;
        self.stats.samplers += 1;
        Ok(Sampler(self.samplers.insert(SamplerEntry {
            sampler,
            bindless: None,
        })))
    }

    /// Destroy a sampler once the work recorded so far is done with it.
    pub fn destroy_sampler(&mut self, s: Sampler) {
        if let Some(e) = self.samplers.remove(s.0) {
            if let Some(i) = e.bindless {
                self.defer(Garbage::SamplerIndex(i));
            }
            self.stats.samplers -= 1;
            self.defer(Garbage::Sampler(e.sampler));
        }
    }

    /// How many textures the device holds, lent images included.
    #[must_use]
    pub fn live_textures(&self) -> usize {
        self.textures.len()
    }
}

/// An image lent to the layer by its owner.
#[derive(Debug, Clone, Copy)]
pub struct Lent {
    /// The image.
    pub image: vk::Image,
    /// Its view of every aspect.
    pub view: vk::ImageView,
    /// Its format.
    pub format: Format,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The state its owner left it in.
    pub state: State,
}
