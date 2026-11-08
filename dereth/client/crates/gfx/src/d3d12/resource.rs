//! Buffers, textures, samplers, their places in the heaps, and the staging memory uploads pass
//! through, on Direct3D 12.

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::{formats, hr, Device, Garbage, Mapped, BUFFERS_AT};
use crate::{
    Address, Buffer, BufferDesc, BufferUsage, Error, Filter, Format, Location, Sampler,
    SamplerDesc, Texture, TextureDesc, TextureUsage,
};

/// A live buffer.
pub(crate) struct BufferEntry {
    pub resource: ID3D12Resource,
    pub size: u64,
    pub location: Location,
    pub usage: BufferUsage,
    /// The state its last use left it in, and whether that use wrote it as a storage buffer.
    pub state: D3D12_RESOURCE_STATES,
    pub uav_written: bool,
    pub bindless: Option<u32>,
    /// Where the processor reaches it, for an upload or read-back buffer.
    pub mapped: Option<Mapped>,
}

/// A live texture, or a resource lent to the layer.
pub(crate) struct TextureEntry {
    pub resource: ID3D12Resource,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    pub state: D3D12_RESOURCE_STATES,
    pub bindless: Option<u32>,
    /// Its colour target view, and its depth target views (written, read only).
    pub rtv: Option<u32>,
    pub dsv: Option<(u32, u32)>,
    pub lent: bool,
}

/// A live sampler.
pub(crate) struct SamplerEntry {
    pub desc: D3D12_SAMPLER_DESC,
    pub bindless: Option<u32>,
}

/// One upload-heap buffer staged uploads are bump-allocated from.
pub(crate) struct Arena {
    pub resource: ID3D12Resource,
    pub mapped: Mapped,
    pub capacity: u64,
    pub used: u64,
}

/// The staging arenas: those being filled, and those free to fill again.
#[derive(Default)]
pub(crate) struct Staging {
    pub current: Vec<Arena>,
    pub free: Vec<Arena>,
}

/// One staged copy, waiting for the next recording.
pub(crate) enum Pending {
    Buffer {
        src: ID3D12Resource,
        src_offset: u64,
        dst: Buffer,
        dst_offset: u64,
        size: u64,
    },
    Texture {
        src: ID3D12Resource,
        footprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
        dst: Texture,
        level: u32,
    },
    Device {
        src: Buffer,
        dst: Buffer,
        size: u64,
    },
}

/// The smallest staging arena.
const ARENA_BYTES: u64 = 8 << 20;

/// The heap a buffer's memory is in.
fn heap(location: Location) -> D3D12_HEAP_PROPERTIES {
    D3D12_HEAP_PROPERTIES {
        Type: match location {
            Location::Gpu => D3D12_HEAP_TYPE_DEFAULT,
            Location::Upload => D3D12_HEAP_TYPE_UPLOAD,
            Location::Readback => D3D12_HEAP_TYPE_READBACK,
        },
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    }
}

/// The description of a buffer of `size` bytes.
/// The heap of the device's own memory.
pub(super) fn default_heap() -> D3D12_HEAP_PROPERTIES {
    heap(Location::Gpu)
}

pub(super) fn buffer_desc(size: u64, flags: D3D12_RESOURCE_FLAGS) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: size.max(16),
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: flags,
    }
}

/// The state a buffer in `location` stays in: an upload buffer is only ever read, a read-back
/// one only written by copies.
pub(crate) fn fixed_state(location: Location) -> Option<D3D12_RESOURCE_STATES> {
    match location {
        Location::Gpu => None,
        Location::Upload => Some(D3D12_RESOURCE_STATE_GENERIC_READ),
        Location::Readback => Some(D3D12_RESOURCE_STATE_COPY_DEST),
    }
}

impl Device {
    /// A committed buffer resource, and where the processor reaches it if it can.
    fn raw_buffer(
        &self,
        size: u64,
        location: Location,
        flags: D3D12_RESOURCE_FLAGS,
    ) -> Result<(ID3D12Resource, Option<Mapped>), Error> {
        let props = heap(location);
        let desc = buffer_desc(size, flags);
        let initial = fixed_state(location).unwrap_or(D3D12_RESOURCE_STATE_COMMON);
        let mut resource: Option<ID3D12Resource> = None;
        // SAFETY: live locals; the resource is owned by the caller.
        hr("CreateCommittedResource", unsafe {
            self.device.CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                initial,
                None,
                &mut resource,
            )
        })?;
        let resource =
            resource.ok_or_else(|| Error::Allocation("the runtime made no buffer".into()))?;
        let mapped = if location == Location::Gpu {
            None
        } else {
            let mut ptr = std::ptr::null_mut();
            let none = D3D12_RANGE { Begin: 0, End: 0 };
            let read = if location == Location::Readback {
                None
            } else {
                Some(std::ptr::from_ref(&none))
            };
            // SAFETY: a live buffer in a heap the processor reaches; it stays mapped for its life.
            hr("Map", unsafe { resource.Map(0, read, Some(&mut ptr)) })?;
            Some(Mapped(ptr.cast()))
        };
        Ok((resource, mapped))
    }

    /// Make a buffer.
    ///
    /// # Errors
    /// Any failure of the runtime.
    pub fn create_buffer(&mut self, desc: &BufferDesc<'_>) -> Result<Buffer, Error> {
        let flags = if desc.location == Location::Gpu && desc.usage.contains(BufferUsage::STORAGE) {
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS
        } else {
            D3D12_RESOURCE_FLAG_NONE
        };
        let (resource, mapped) = self.raw_buffer(desc.size, desc.location, flags)?;
        name(&resource, desc.label);
        let state = fixed_state(desc.location).unwrap_or(D3D12_RESOURCE_STATE_COMMON);
        let id = self.buffers.insert(BufferEntry {
            resource,
            size: desc.size,
            location: desc.location,
            usage: desc.usage,
            state,
            uav_written: false,
            bindless: None,
            mapped,
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
    /// [`Error::Invalid`] for a write past the buffer's end.
    pub fn write_buffer(&mut self, b: Buffer, offset: u64, data: &[u8]) -> Result<(), Error> {
        let e = self.buffer(b)?;
        let len = data.len() as u64;
        let (size, mapped) = (e.size, e.mapped);
        if offset + len > size {
            return Err(Error::Invalid(format!(
                "a write of {len} bytes at {offset} into a buffer of {size}"
            )));
        }
        if data.is_empty() {
            return Ok(());
        }
        self.stats.uploaded_bytes += len;
        match mapped {
            Some(m) => {
                // SAFETY: the range is inside the buffer, which stays mapped.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        data.as_ptr(),
                        m.0.add(usize::try_from(offset).unwrap_or(usize::MAX)),
                        data.len(),
                    );
                }
            }
            None => {
                let (src, src_offset) = self.stage(data, 16)?;
                self.pending.push(Pending::Buffer {
                    src,
                    src_offset,
                    dst: b,
                    dst_offset: offset,
                    size: len,
                });
            }
        }
        Ok(())
    }

    /// Copy the first `size` bytes of `src` into `dst`, at the head of the next recording.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold, or a size past either's end.
    pub fn copy_buffer_later(&mut self, src: Buffer, dst: Buffer, size: u64) -> Result<(), Error> {
        if size > self.buffer(src)?.size || size > self.buffer(dst)?.size {
            return Err(Error::Invalid("a copy past a buffer's end".into()));
        }
        self.pending.push(Pending::Device { src, dst, size });
        Ok(())
    }

    /// Read `len` bytes of a read-back buffer at `offset`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer that is not a read-back one, or a read past its end.
    pub fn read_buffer(&self, b: Buffer, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        let e = self.buffer(b)?;
        if offset + len > e.size {
            return Err(Error::Invalid("a read past the buffer's end".into()));
        }
        let m = e
            .mapped
            .ok_or_else(|| Error::Invalid("the buffer is not mapped".into()))?;
        let mut out = vec![0u8; usize::try_from(len).unwrap_or(0)];
        // SAFETY: the range is inside the buffer, which stays mapped; the device has finished
        // writing it (the caller's obligation).
        unsafe {
            std::ptr::copy_nonoverlapping(
                m.0.add(usize::try_from(offset).unwrap_or(usize::MAX)),
                out.as_mut_ptr(),
                out.len(),
            );
        }
        Ok(out)
    }

    /// Copy `data` into the staging memory at an offset aligned to `align`, and where it landed.
    pub(crate) fn stage(
        &mut self,
        data: &[u8],
        align: u64,
    ) -> Result<(ID3D12Resource, u64), Error> {
        let (resource, offset) = self.reserve(data.len() as u64, align)?;
        let arena = self
            .staging
            .current
            .last()
            .expect("an arena was just made current");
        // SAFETY: the reserved range is inside the arena, which stays mapped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                arena
                    .mapped
                    .0
                    .add(usize::try_from(offset).unwrap_or(usize::MAX)),
                data.len(),
            );
        }
        Ok((resource, offset))
    }

    /// `len` bytes of staging memory at an offset aligned to `align`: the arena they are in, and
    /// the offset, in the current arena.
    pub(crate) fn reserve(&mut self, len: u64, align: u64) -> Result<(ID3D12Resource, u64), Error> {
        let fits = |a: &Arena| a.used.next_multiple_of(align) + len <= a.capacity;
        if !self.staging.current.last().is_some_and(fits) {
            let reuse = self
                .staging
                .free
                .iter()
                .position(|a| a.capacity >= len + align);
            let arena = match reuse {
                Some(i) => {
                    let mut a = self.staging.free.swap_remove(i);
                    a.used = 0;
                    a
                }
                None => {
                    let capacity = (len + align).max(ARENA_BYTES).next_power_of_two();
                    let (resource, mapped) =
                        self.raw_buffer(capacity, Location::Upload, D3D12_RESOURCE_FLAG_NONE)?;
                    Arena {
                        resource,
                        mapped: mapped.expect("an upload buffer is mapped"),
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
        let offset = arena.used.next_multiple_of(align);
        arena.used = offset + len;
        Ok((arena.resource.clone(), offset))
    }

    /// Make a 2D texture.
    ///
    /// # Errors
    /// Any failure of the runtime.
    pub fn create_texture(&mut self, desc: &TextureDesc<'_>) -> Result<Texture, Error> {
        let (res_format, _, _) = formats(desc.format);
        let mut flags = D3D12_RESOURCE_FLAG_NONE.0;
        if desc.usage.contains(TextureUsage::COLOR_TARGET) {
            flags |= D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET.0;
        }
        if desc.usage.contains(TextureUsage::DEPTH_TARGET) {
            flags |= D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL.0;
        }
        if desc.usage.contains(TextureUsage::STORAGE) {
            flags |= D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS.0;
        }
        let mips = desc.mips.max(1);
        let rdesc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: u64::from(desc.width),
            Height: desc.height,
            DepthOrArraySize: 1,
            MipLevels: u16::try_from(mips).unwrap_or(1),
            Format: res_format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAGS(flags),
        };
        let props = heap(Location::Gpu);
        let mut resource: Option<ID3D12Resource> = None;
        // SAFETY: live locals; the resource is owned by the entry.
        hr("CreateCommittedResource", unsafe {
            self.device.CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAG_NONE,
                &rdesc,
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut resource,
            )
        })?;
        let resource =
            resource.ok_or_else(|| Error::Allocation("the runtime made no texture".into()))?;
        name(&resource, desc.label);
        let id = self.insert_texture(TextureEntry {
            resource,
            format: desc.format,
            width: desc.width,
            height: desc.height,
            mips,
            state: D3D12_RESOURCE_STATE_COMMON,
            bindless: None,
            rtv: None,
            dsv: None,
            lent: false,
        })?;
        self.stats.textures += 1;
        Ok(id)
    }

    /// Hold `e`, with the target views its resource allows.
    fn insert_texture(&mut self, mut e: TextureEntry) -> Result<Texture, Error> {
        // SAFETY: a pure query on a live resource.
        let flags = unsafe { e.resource.GetDesc() }.Flags;
        let (_, target, _) = formats(e.format);
        if (flags.0 & D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET.0) != 0 {
            let i = self
                .free
                .rtvs
                .take()
                .ok_or(Error::Full("colour target views"))?;
            let view = D3D12_RENDER_TARGET_VIEW_DESC {
                Format: target,
                ViewDimension: D3D12_RTV_DIMENSION_TEXTURE2D,
                Anonymous: D3D12_RENDER_TARGET_VIEW_DESC_0 {
                    Texture2D: D3D12_TEX2D_RTV {
                        MipSlice: 0,
                        PlaneSlice: 0,
                    },
                },
            };
            // SAFETY: a live resource and a slot of the device's own heap.
            unsafe {
                self.device
                    .CreateRenderTargetView(&e.resource, Some(&view), self.heaps.rtv(i));
            }
            e.rtv = Some(i);
        }
        if (flags.0 & D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL.0) != 0 {
            let w = self
                .free
                .dsvs
                .take()
                .ok_or(Error::Full("depth target views"))?;
            let r = self
                .free
                .dsvs
                .take()
                .ok_or(Error::Full("depth target views"))?;
            for (slot, read_only) in [(w, false), (r, true)] {
                let ro = if read_only {
                    let mut f = D3D12_DSV_FLAG_READ_ONLY_DEPTH.0;
                    if e.format.has_stencil() {
                        f |= D3D12_DSV_FLAG_READ_ONLY_STENCIL.0;
                    }
                    D3D12_DSV_FLAGS(f)
                } else {
                    D3D12_DSV_FLAG_NONE
                };
                let view = D3D12_DEPTH_STENCIL_VIEW_DESC {
                    Format: target,
                    ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D,
                    Flags: ro,
                    Anonymous: D3D12_DEPTH_STENCIL_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_DSV { MipSlice: 0 },
                    },
                };
                // SAFETY: a live resource and a slot of the device's own heap.
                unsafe {
                    self.device.CreateDepthStencilView(
                        &e.resource,
                        Some(&view),
                        self.heaps.dsv(slot),
                    );
                }
            }
            e.dsv = Some((w, r));
        }
        Ok(Texture(self.textures.insert(e)))
    }

    /// Destroy a texture once the work recorded so far is done with it. A lent resource is only
    /// forgotten.
    pub fn destroy_texture(&mut self, t: Texture) {
        if let Some(e) = self.textures.remove(t.0) {
            if let Some(i) = e.bindless {
                self.defer(Garbage::TextureIndex(i));
            }
            if let Some(i) = e.rtv {
                self.defer(Garbage::Rtv(i));
            }
            if let Some((w, r)) = e.dsv {
                self.defer(Garbage::Dsv(w));
                self.defer(Garbage::Dsv(r));
            }
            if !e.lent {
                self.stats.textures -= 1;
            }
            self.defer(Garbage::Texture(e));
        }
    }

    /// Lend the layer a resource its owner keeps.
    ///
    /// # Safety
    /// The resource must be live while the layer holds it, and `lent.state` the state its
    /// owner's work left it in.
    ///
    /// # Errors
    /// [`Error::Full`] when no target view is left.
    pub unsafe fn lend_texture(&mut self, lent: &super::D3d12Lent) -> Result<Texture, Error> {
        self.insert_texture(TextureEntry {
            resource: lent.resource.clone(),
            format: lent.format,
            width: lent.width,
            height: lent.height,
            mips: 1,
            state: lent.state,
            bindless: None,
            rtv: None,
            dsv: None,
            lent: true,
        })
    }

    /// A texture's extent.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold.
    pub fn texture_size(&self, t: Texture) -> Result<(u32, u32), Error> {
        let e = self.texture(t)?;
        Ok((e.width, e.height))
    }

    /// Where level `level` of `t` lies in a buffer copy: its footprint at `offset`, its rows,
    /// and each row's bytes.
    pub(crate) fn footprint(
        &self,
        t: &TextureEntry,
        level: u32,
        offset: u64,
    ) -> (D3D12_PLACED_SUBRESOURCE_FOOTPRINT, u32, u64) {
        // SAFETY: pure queries on live interfaces.
        let desc = unsafe { t.resource.GetDesc() };
        let mut fp = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
        let (mut rows, mut row_bytes, mut total) = (0u32, 0u64, 0u64);
        // SAFETY: as above; the out values are locals.
        unsafe {
            self.device.GetCopyableFootprints(
                &desc,
                level,
                1,
                offset,
                Some(&mut fp),
                Some(&mut rows),
                Some(&mut row_bytes),
                Some(&mut total),
            );
        }
        (fp, rows, row_bytes)
    }

    /// Upload one level of a sampled texture: staged with the rows the runtime's copies want,
    /// copied at the head of the next recording, and left ready to sample.
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
        let (fp0, rows, row_bytes) = self.footprint(e, level, 0);
        self.stats.uploaded_bytes += want;
        let pitch = u64::from(fp0.Footprint.RowPitch);
        let (src, offset) = self.reserve(pitch * u64::from(rows), 512)?;
        let arena = self
            .staging
            .current
            .last()
            .expect("an arena was just reserved");
        let tight = usize::try_from(want / u64::from(rows.max(1))).unwrap_or(0);
        for r in 0..rows as usize {
            let from = &data[r * tight..(r * tight + tight).min(data.len())];
            let at = offset + pitch * r as u64;
            // SAFETY: the row lies inside the reserved range of the mapped arena.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    from.as_ptr(),
                    arena
                        .mapped
                        .0
                        .add(usize::try_from(at).unwrap_or(usize::MAX)),
                    from.len()
                        .min(usize::try_from(row_bytes).unwrap_or(usize::MAX)),
                );
            }
        }
        let mut footprint = fp0;
        footprint.Offset = offset;
        self.pending.push(Pending::Texture {
            src,
            footprint,
            dst: t,
            level,
        });
        Ok(())
    }

    /// Make a sampler: its description, written into the heap when it is bound.
    ///
    /// # Errors
    /// None on this backend.
    pub fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<Sampler, Error> {
        let address = |a: Address| match a {
            Address::Repeat => D3D12_TEXTURE_ADDRESS_MODE_WRAP,
            Address::Clamp => D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
        };
        let aniso = desc.anisotropy.clamp(1, 16);
        let filter = if aniso > 1 {
            D3D12_FILTER_ANISOTROPIC
        } else {
            match (desc.filter, desc.mip) {
                (Filter::Nearest, Filter::Nearest) => D3D12_FILTER_MIN_MAG_MIP_POINT,
                (Filter::Nearest, Filter::Linear) => D3D12_FILTER_MIN_MAG_POINT_MIP_LINEAR,
                (Filter::Linear, Filter::Nearest) => D3D12_FILTER_MIN_MAG_LINEAR_MIP_POINT,
                (Filter::Linear, Filter::Linear) => D3D12_FILTER_MIN_MAG_MIP_LINEAR,
            }
        };
        let d = D3D12_SAMPLER_DESC {
            Filter: filter,
            AddressU: address(desc.address),
            AddressV: address(desc.address_v),
            AddressW: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
            MipLODBias: desc.lod_bias,
            MaxAnisotropy: aniso,
            ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
        };
        self.stats.samplers += 1;
        Ok(Sampler(self.samplers.insert(SamplerEntry {
            desc: d,
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
        }
    }

    /// How many textures the device holds, lent resources included.
    #[must_use]
    pub fn live_textures(&self) -> usize {
        self.textures.len()
    }

    /// Give texture `t` an index in the table.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture made without the sampled use; [`Error::Full`].
    pub fn bind_texture(&mut self, t: Texture) -> Result<u32, Error> {
        let e = self.texture(t)?;
        if let Some(i) = e.bindless {
            return Ok(i);
        }
        let (_, _, view_format) = formats(e.format);
        let resource = e.resource.clone();
        let mips = e.mips;
        let index = self.free.textures.take().ok_or(Error::Full("textures"))?;
        let view = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: view_format,
            ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: mips,
                    PlaneSlice: 0,
                    ResourceMinLODClamp: 0.0,
                },
            },
        };
        // SAFETY: a live resource and a slot of the device's own heap no frame in flight reads.
        unsafe {
            self.device.CreateShaderResourceView(
                &resource,
                Some(&view),
                self.heaps.resource(index),
            );
        }
        if let Some(e) = self.textures.get_mut(t.0) {
            e.bindless = Some(index);
        }
        Ok(index)
    }

    /// Give sampler `s` an index in the table.
    ///
    /// # Errors
    /// [`Error::Full`] when no index is left.
    pub fn bind_sampler(&mut self, s: Sampler) -> Result<u32, Error> {
        let e = self
            .samplers
            .get(s.0)
            .ok_or_else(|| Error::Invalid(format!("no sampler {}", s.0)))?;
        if let Some(i) = e.bindless {
            return Ok(i);
        }
        let desc = e.desc;
        let index = self.free.samplers.take().ok_or(Error::Full("samplers"))?;
        // SAFETY: a slot of the device's own heap no frame in flight reads.
        unsafe { self.device.CreateSampler(&desc, self.heaps.sampler(index)) };
        if let Some(e) = self.samplers.get_mut(s.0) {
            e.bindless = Some(index);
        }
        Ok(index)
    }

    /// Give storage buffer `b` an index in the table: its read view, and its write view where
    /// the device can write it.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer made without the storage use; [`Error::Full`].
    pub fn bind_buffer(&mut self, b: Buffer) -> Result<u32, Error> {
        let e = self.buffer(b)?;
        if let Some(i) = e.bindless {
            return Ok(i);
        }
        if !e.usage.contains(BufferUsage::STORAGE) {
            return Err(Error::Invalid("the buffer is not a storage buffer".into()));
        }
        let resource = e.resource.clone();
        let words = u32::try_from(e.size / 4).unwrap_or(u32::MAX);
        let writable = e.location == Location::Gpu;
        let index = self.free.buffers.take().ok_or(Error::Full("buffers"))?;
        let read = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R32_TYPELESS,
            ViewDimension: D3D12_SRV_DIMENSION_BUFFER,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_SRV {
                    FirstElement: 0,
                    NumElements: words,
                    StructureByteStride: 0,
                    Flags: D3D12_BUFFER_SRV_FLAG_RAW,
                },
            },
        };
        let write = D3D12_UNORDERED_ACCESS_VIEW_DESC {
            Format: DXGI_FORMAT_R32_TYPELESS,
            ViewDimension: D3D12_UAV_DIMENSION_BUFFER,
            Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                Buffer: D3D12_BUFFER_UAV {
                    FirstElement: 0,
                    NumElements: if writable { words } else { 0 },
                    StructureByteStride: 0,
                    CounterOffsetInBytes: 0,
                    Flags: D3D12_BUFFER_UAV_FLAG_RAW,
                },
            },
        };
        let at = BUFFERS_AT + index * 2;
        // SAFETY: a live resource and two slots of the device's own heap no frame in flight
        // reads; a buffer the device cannot write gets an empty write view.
        unsafe {
            self.device
                .CreateShaderResourceView(&resource, Some(&read), self.heaps.resource(at));
            let target: Option<&ID3D12Resource> = writable.then_some(&resource);
            self.device.CreateUnorderedAccessView(
                target,
                None::<&ID3D12Resource>,
                Some(&write),
                self.heaps.resource(at + 1),
            );
        }
        if let Some(e) = self.buffers.get_mut(b.0) {
            e.bindless = Some(index);
        }
        Ok(index)
    }

    /// Write an empty view into every slot of the table, so an index no live resource holds reads
    /// nothing rather than anything.
    pub(crate) fn null_views(&mut self) {
        let srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                    PlaneSlice: 0,
                    ResourceMinLODClamp: 0.0,
                },
            },
        };
        // SAFETY: empty views written into the device's own heap before anything reads it.
        unsafe {
            for i in 0..self.caps.bindless_textures {
                self.device.CreateShaderResourceView(
                    None::<&ID3D12Resource>,
                    Some(&srv),
                    self.heaps.resource(i),
                );
            }
        }
    }
}

/// Name `resource` `label` for the driver's tools.
fn name(resource: &ID3D12Resource, label: &str) {
    let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: a NUL-terminated wide string live for the call.
    let _ = unsafe { resource.SetName(windows::core::PCWSTR(wide.as_ptr())) };
}
