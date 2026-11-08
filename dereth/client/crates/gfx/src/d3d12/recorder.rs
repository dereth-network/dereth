//! Recording on Direct3D 12: command lists of the device's own or lent by an adopted device's
//! owner, the state transitions each pass's declared uses need, the staged copies at a
//! recording's head, the draws, dispatches and copies, and the timestamps.

use windows::core::Interface;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::pipeline::{INDEXED_STRIDE, PLAIN_STRIDE};
use super::resource::{fixed_state, Pending};
use super::{d3d12_state, hr, sampled_state, Device, Garbage, Mapped, Retire};
use crate::recorder::{Load, RasterPass, Touch};
use crate::{
    Buffer, Error, FrameClock, IndexType, Pipeline, Rect, Texture, Ticket, Timing, Use, MARKS,
    PUSH_BYTES,
};

/// One recording's marks: the query heap they are written into, where they are resolved, and
/// their names.
pub(crate) struct TimerSlot {
    heap: ID3D12QueryHeap,
    readback: ID3D12Resource,
    mapped: Mapped,
    labels: Vec<&'static str>,
}

/// The query heaps, and the spans of the last recording read.
#[derive(Default)]
pub(crate) struct Timers {
    free: Vec<(ID3D12QueryHeap, ID3D12Resource, Mapped)>,
    pub last: Vec<Timing>,
}

impl Timers {
    fn take(&mut self, dev: &Device) -> Result<TimerSlot, Error> {
        if let Some((heap, readback, mapped)) = self.free.pop() {
            return Ok(TimerSlot {
                heap,
                readback,
                mapped,
                labels: Vec::new(),
            });
        }
        let desc = D3D12_QUERY_HEAP_DESC {
            Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
            Count: MARKS,
            NodeMask: 0,
        };
        let mut heap: Option<ID3D12QueryHeap> = None;
        // SAFETY: a live local; the heap is the timers' own.
        hr("CreateQueryHeap", unsafe {
            dev.device.CreateQueryHeap(&desc, &mut heap)
        })?;
        let heap = heap.ok_or_else(|| Error::Invalid("the runtime made no query heap".into()))?;
        let buffer = dev.raw_readback(u64::from(MARKS) * 8)?;
        Ok(TimerSlot {
            heap,
            readback: buffer.0,
            mapped: buffer.1,
            labels: Vec::new(),
        })
    }

    /// The recording `slot` was written by is done: read its marks into the last spans.
    pub(crate) fn retired(&mut self, slot: TimerSlot, period_ns: f32) {
        let n = slot.labels.len();
        if n > 0 {
            let mut ticks = vec![0u64; n];
            // SAFETY: the work that resolved the marks into the mapped buffer is done.
            unsafe {
                std::ptr::copy_nonoverlapping(slot.mapped.0.cast::<u64>(), ticks.as_mut_ptr(), n);
            }
            self.last = slot
                .labels
                .iter()
                .zip(ticks.windows(2))
                .map(|(label, w)| {
                    let t = w[1].saturating_sub(w[0]);
                    #[allow(clippy::cast_precision_loss)] // a span of ticks
                    let ms = t as f64 * f64::from(period_ns) / 1.0e6;
                    Timing {
                        label,
                        ticks: t,
                        ms,
                    }
                })
                .collect();
        }
        self.free.push((slot.heap, slot.readback, slot.mapped));
    }
}

/// A state transition of a whole resource.
fn transition(
    resource: &ID3D12Resource,
    before: D3D12_RESOURCE_STATES,
    after: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                // SAFETY: a non-owning copy; the resource outlives the barrier's recording.
                pResource: unsafe { std::mem::transmute_copy(resource) },
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}

/// A barrier between two uses of a resource as storage.
fn uav(resource: &ID3D12Resource) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER {
                // SAFETY: as above.
                pResource: unsafe { std::mem::transmute_copy(resource) },
            }),
        },
    }
}

/// The state `u` puts a resource in: a texture (holding depth, or not) or a buffer.
fn state_for(u: Use, texture: bool, depth: bool) -> D3D12_RESOURCE_STATES {
    match u {
        Use::ColorTarget => D3D12_RESOURCE_STATE_RENDER_TARGET,
        Use::DepthTarget => D3D12_RESOURCE_STATE_DEPTH_WRITE,
        Use::DepthTest => sampled_state(true),
        Use::Sampled => sampled_state(depth),
        Use::StorageRead if texture => sampled_state(depth),
        Use::StorageWrite => D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
        Use::TransferSrc if texture => D3D12_RESOURCE_STATE_COPY_SOURCE,
        Use::TransferDst | Use::HostRead => D3D12_RESOURCE_STATE_COPY_DEST,
        Use::External(s) => d3d12_state(s, depth),
        Use::StorageRead | Use::Indirect | Use::Index | Use::TransferSrc => {
            D3D12_RESOURCE_STATE_GENERIC_READ
        }
    }
}

/// Move a resource in `state` (its storage written last when `uav_written`) to the state `u`
/// wants: the barrier that must come first, if one must.
pub(super) fn step(
    resource: &ID3D12Resource,
    state: &mut D3D12_RESOURCE_STATES,
    uav_written: &mut bool,
    want: D3D12_RESOURCE_STATES,
    writes_storage: bool,
) -> Option<D3D12_RESOURCE_BARRIER> {
    let b = if want == *state {
        // Storage written and then used as storage again waits for the writes.
        (want == D3D12_RESOURCE_STATE_UNORDERED_ACCESS && (*uav_written || writes_storage))
            .then(|| uav(resource))
    } else {
        let t = transition(resource, *state, want);
        *state = want;
        Some(t)
    };
    *uav_written = writes_storage;
    b
}

/// Record `barriers`, if there are any.
pub(super) fn put(list: &ID3D12GraphicsCommandList, barriers: &[D3D12_RESOURCE_BARRIER]) {
    if !barriers.is_empty() {
        // SAFETY: the list is open; every resource is live.
        unsafe { list.ResourceBarrier(barriers) };
    }
}

impl Device {
    /// A read-back buffer of `size` bytes, outside the tables.
    pub(crate) fn raw_readback(&self, size: u64) -> Result<(ID3D12Resource, Mapped), Error> {
        let props = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_READBACK,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
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
            ..Default::default()
        };
        let mut resource: Option<ID3D12Resource> = None;
        // SAFETY: live locals.
        hr("CreateCommittedResource", unsafe {
            self.device.CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut resource,
            )
        })?;
        let resource = resource.ok_or_else(|| Error::Allocation("no read-back buffer".into()))?;
        let mut ptr = std::ptr::null_mut();
        // SAFETY: a read-back buffer, mapped for its life.
        hr("Map", unsafe { resource.Map(0, None, Some(&mut ptr)) })?;
        Ok((resource, Mapped(ptr.cast())))
    }

    /// A list and its allocator, free to record, the device's own.
    fn command_list(
        &mut self,
    ) -> Result<(ID3D12CommandAllocator, ID3D12GraphicsCommandList), Error> {
        if let Some((a, l)) = self.commands.pop() {
            // SAFETY: the work that used them is done (they come back through the garbage).
            unsafe {
                hr("Reset(allocator)", a.Reset())?;
                hr("Reset(list)", l.Reset(&a, None))?;
            }
            return Ok((a, l));
        }
        // SAFETY: live device; the allocator and list are the device's own.
        let a: ID3D12CommandAllocator = hr("CreateCommandAllocator", unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        })?;
        // SAFETY: as above; a new list is open.
        let l: ID3D12GraphicsCommandList = hr("CreateCommandList", unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &a, None)
        })?;
        Ok((a, l))
    }

    /// Begin a recording in a command list of the device's own.
    ///
    /// # Errors
    /// Any failure of the runtime.
    pub fn begin(&mut self) -> Result<Recorder<'_>, Error> {
        self.collect();
        let (alloc, list) = self.command_list()?;
        let value = self.signal_next();
        let mut r = Recorder {
            dev: self,
            list,
            alloc: Some(alloc),
            at: Retire { value, frame: -1 },
            own: true,
            timer: None,
            done: false,
        };
        r.start()?;
        Ok(r)
    }

    /// Record into `list`, lent by an adopted device's owner for its frame `clock` names.
    ///
    /// # Safety
    /// See [`crate::Device::record_into_d3d12`].
    ///
    /// # Errors
    /// Any failure of the runtime.
    pub unsafe fn record_into(
        &mut self,
        list: &ID3D12GraphicsCommandList,
        clock: FrameClock,
    ) -> Result<Recorder<'_>, Error> {
        let stamp = i64::try_from(clock.stamp).unwrap_or(i64::MAX);
        let in_flight = i64::try_from(clock.in_flight).unwrap_or(i64::MAX);
        self.finished_frame = self.finished_frame.max(stamp.saturating_sub(in_flight));
        self.last_frame = self.last_frame.max(stamp);
        self.collect();
        let mut r = Recorder {
            dev: self,
            list: list.clone(),
            alloc: None,
            at: Retire {
                value: 0,
                frame: stamp,
            },
            own: false,
            timer: None,
            done: false,
        };
        r.start()?;
        Ok(r)
    }

    /// One level of `t`, read back: tightly packed rows. Submits and waits for a recording of
    /// its own.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a texture the device does not hold; any failure of the runtime.
    pub fn read_texture(&mut self, t: Texture, level: u32) -> Result<Vec<u8>, Error> {
        let e = self.texture(t)?;
        let (w, h) = ((e.width >> level).max(1), (e.height >> level).max(1));
        let bytes = e.format.level_bytes(w, h);
        let readback = self.create_buffer(&crate::BufferDesc {
            label: "read back",
            size: bytes,
            usage: crate::BufferUsage::TRANSFER_DST,
            location: crate::Location::Readback,
        })?;
        let result = (|| {
            let mut r = self.begin()?;
            r.copy_texture_to_buffer(t, level, readback, 0)?;
            let ticket = r.submit()?;
            self.wait(ticket)?;
            self.read_buffer(readback, 0, bytes)
        })();
        self.destroy_buffer(readback);
        result
    }

    /// The spans the last finished recording's marks measured.
    #[must_use]
    pub fn timings(&mut self) -> Vec<Timing> {
        self.collect();
        self.timers.last.clone()
    }

    /// Record every staged copy into `list`, and leave what they wrote ready: textures in the
    /// state shaders sample them in.
    fn flush_uploads(&mut self, list: &ID3D12GraphicsCommandList, at: Retire) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        let mut textures: Vec<Texture> = Vec::new();
        for p in &pending {
            match p {
                Pending::Buffer {
                    src,
                    src_offset,
                    dst,
                    dst_offset,
                    size,
                } => {
                    let Some(e) = self.buffers.get_mut(dst.0) else {
                        continue;
                    };
                    let b = step(
                        &e.resource,
                        &mut e.state,
                        &mut e.uav_written,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                        false,
                    );
                    put(list, &b.into_iter().collect::<Vec<_>>());
                    // SAFETY: both buffers are live, the range inside each.
                    unsafe {
                        list.CopyBufferRegion(&e.resource, *dst_offset, src, *src_offset, *size);
                    }
                }
                Pending::Device { src, dst, size } => {
                    let mut barriers = Vec::new();
                    let s = match self.buffers.get_mut(src.0) {
                        Some(e) => {
                            if fixed_state(e.location).is_none() {
                                barriers.extend(step(
                                    &e.resource,
                                    &mut e.state,
                                    &mut e.uav_written,
                                    D3D12_RESOURCE_STATE_GENERIC_READ,
                                    false,
                                ));
                            }
                            e.resource.clone()
                        }
                        None => continue,
                    };
                    let d = match self.buffers.get_mut(dst.0) {
                        Some(e) => {
                            barriers.extend(step(
                                &e.resource,
                                &mut e.state,
                                &mut e.uav_written,
                                D3D12_RESOURCE_STATE_COPY_DEST,
                                false,
                            ));
                            e.resource.clone()
                        }
                        None => continue,
                    };
                    put(list, &barriers);
                    // SAFETY: both buffers are live and `size` inside each.
                    unsafe { list.CopyBufferRegion(&d, 0, &s, 0, *size) };
                }
                Pending::Texture {
                    src,
                    footprint,
                    dst,
                    level,
                } => {
                    let Some(e) = self.textures.get_mut(dst.0) else {
                        continue;
                    };
                    let mut uw = false;
                    let b = step(
                        &e.resource,
                        &mut e.state,
                        &mut uw,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                        false,
                    );
                    put(list, &b.into_iter().collect::<Vec<_>>());
                    let to = D3D12_TEXTURE_COPY_LOCATION {
                        // SAFETY: a non-owning copy; the resource outlives the call.
                        pResource: unsafe { std::mem::transmute_copy(&e.resource) },
                        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                            SubresourceIndex: *level,
                        },
                    };
                    let from = D3D12_TEXTURE_COPY_LOCATION {
                        // SAFETY: as above.
                        pResource: unsafe { std::mem::transmute_copy(src) },
                        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                            PlacedFootprint: *footprint,
                        },
                    };
                    // SAFETY: the texture is in the copy state; the staged rows are the
                    // footprint's.
                    unsafe { list.CopyTextureRegion(&to, 0, 0, 0, &from, None) };
                    if !textures.contains(dst) {
                        textures.push(*dst);
                    }
                }
            }
        }
        let mut ready = Vec::new();
        for t in &textures {
            let Some(e) = self.textures.get_mut(t.0) else {
                continue;
            };
            let mut uw = false;
            let depth = e.format.is_depth();
            ready.extend(step(
                &e.resource,
                &mut e.state,
                &mut uw,
                sampled_state(depth),
                false,
            ));
        }
        put(list, &ready);
        let arenas = std::mem::take(&mut self.staging.current);
        for a in arenas {
            self.garbage.push((at, Garbage::Staging(a)));
        }
    }
}

/// One recording: a command list of the device's own, or one an adopted device's owner lent.
pub struct Recorder<'d> {
    dev: &'d mut Device,
    list: ID3D12GraphicsCommandList,
    alloc: Option<ID3D12CommandAllocator>,
    at: Retire,
    own: bool,
    timer: Option<TimerSlot>,
    done: bool,
}

impl std::fmt::Debug for Recorder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder")
            .field("own", &self.own)
            .finish_non_exhaustive()
    }
}

impl Recorder<'_> {
    /// The heaps and root signatures, the marks' heap, then the staged copies.
    fn start(&mut self) -> Result<(), Error> {
        let heaps = [
            Some(self.dev.heaps.resources.clone()),
            Some(self.dev.heaps.samplers.clone()),
        ];
        // SAFETY: the list is open; the heaps go before the root signatures that index them.
        unsafe {
            self.list.SetDescriptorHeaps(&heaps);
            self.list.SetGraphicsRootSignature(&self.dev.root);
            self.list.SetComputeRootSignature(&self.dev.root);
        }
        if self.dev.caps.timestamps {
            let mut timers = std::mem::take(&mut self.dev.timers);
            let slot = timers.take(self.dev);
            self.dev.timers = timers;
            self.timer = Some(slot?);
        }
        let list = self.list.clone();
        self.dev.flush_uploads(&list, self.at);
        self.dev.build_pending(&list, self.at);
        Ok(())
    }

    /// The device recorded on.
    pub(crate) fn device(&mut self) -> &mut Device {
        self.dev
    }

    /// The list recorded into.
    pub(super) fn list(&self) -> &ID3D12GraphicsCommandList {
        &self.list
    }

    /// Write a mark named `label`.
    pub fn timestamp(&mut self, label: &'static str) {
        let Some(t) = self.timer.as_mut() else {
            return;
        };
        let index = u32::try_from(t.labels.len()).unwrap_or(u32::MAX);
        if index >= MARKS {
            return;
        }
        // SAFETY: the list is open; the heap is this recording's.
        unsafe {
            self.list
                .EndQuery(&t.heap, D3D12_QUERY_TYPE_TIMESTAMP, index);
        }
        t.labels.push(label);
    }

    /// The barriers `t` needs, appended to `out`.
    fn track(&mut self, t: Touch, out: &mut Vec<D3D12_RESOURCE_BARRIER>) -> Result<(), Error> {
        match t {
            Touch::Texture(tex, u) => {
                let e = self
                    .dev
                    .textures
                    .get_mut(tex.0)
                    .ok_or_else(|| Error::Invalid(format!("no texture {}", tex.0)))?;
                let want = state_for(u, true, e.format.is_depth());
                let mut uw = false;
                out.extend(step(&e.resource, &mut e.state, &mut uw, want, false));
            }
            Touch::Buffer(buf, u) => {
                let e = self
                    .dev
                    .buffers
                    .get_mut(buf.0)
                    .ok_or_else(|| Error::Invalid(format!("no buffer {}", buf.0)))?;
                if fixed_state(e.location).is_some() {
                    return Ok(());
                }
                let want = state_for(u, false, false);
                out.extend(step(
                    &e.resource,
                    &mut e.state,
                    &mut e.uav_written,
                    want,
                    u == Use::StorageWrite,
                ));
            }
        }
        Ok(())
    }

    /// Put in the barriers `touches` need, now.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold.
    pub fn touch(&mut self, touches: &[Touch]) -> Result<(), Error> {
        let mut barriers = Vec::new();
        for t in touches {
            self.track(*t, &mut barriers)?;
        }
        put(&self.list, &barriers);
        Ok(())
    }

    /// Record a raster pass.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a pass with no target, or a resource the device does not hold.
    pub fn raster(
        &mut self,
        pass: &RasterPass<'_>,
        draw: impl FnOnce(&mut crate::front::Draw<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut barriers = Vec::new();
        for c in pass.color {
            self.track(Touch::Texture(c.texture, Use::ColorTarget), &mut barriers)?;
        }
        if let Some(d) = pass.depth {
            let u = if d.write {
                Use::DepthTarget
            } else {
                Use::DepthTest
            };
            self.track(Touch::Texture(d.texture, u), &mut barriers)?;
        }
        for t in pass.touches {
            self.track(*t, &mut barriers)?;
        }
        put(&self.list, &barriers);
        let first = pass
            .color
            .first()
            .map(|c| c.texture)
            .or(pass.depth.map(|d| d.texture))
            .ok_or_else(|| Error::Invalid(format!("the pass {} has no target", pass.name)))?;
        let (w, h) = self.dev.texture_size(first)?;
        let area = pass.area.unwrap_or(Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        });
        // A pass clears only its area, as a render pass's area bounds its clears.
        let clear_rect = [RECT {
            left: area.x,
            top: area.y,
            right: area.x + i32::try_from(area.width).unwrap_or(i32::MAX),
            bottom: area.y + i32::try_from(area.height).unwrap_or(i32::MAX),
        }];
        let mut rtvs = Vec::with_capacity(pass.color.len());
        for c in pass.color {
            let e = self.dev.texture(c.texture)?;
            let slot = e
                .rtv
                .ok_or_else(|| Error::Invalid("a colour target made without its use".into()))?;
            let handle = self.dev.heaps.rtv(slot);
            if let Load::Clear(v) = c.load {
                // SAFETY: the list is open; the target is in the render target state.
                unsafe {
                    self.list
                        .ClearRenderTargetView(handle, &v, Some(&clear_rect))
                };
            }
            rtvs.push(handle);
        }
        let dsv = match pass.depth {
            Some(d) => {
                let e = self.dev.texture(d.texture)?;
                let (wv, rv) = e
                    .dsv
                    .ok_or_else(|| Error::Invalid("a depth target made without its use".into()))?;
                let handle = self.dev.heaps.dsv(if d.write { wv } else { rv });
                if let Some(depth) = d.clear {
                    let flags = if e.format.has_stencil() {
                        D3D12_CLEAR_FLAGS(D3D12_CLEAR_FLAG_DEPTH.0 | D3D12_CLEAR_FLAG_STENCIL.0)
                    } else {
                        D3D12_CLEAR_FLAG_DEPTH
                    };
                    // SAFETY: the list is open; the target is in the depth write state.
                    unsafe {
                        self.list
                            .ClearDepthStencilView(handle, flags, depth, 0, Some(&clear_rect));
                    };
                }
                Some(handle)
            }
            None => None,
        };
        // SAFETY: the list is open; the views are live and in the states the barriers set.
        unsafe {
            self.list.OMSetRenderTargets(
                u32::try_from(rtvs.len()).unwrap_or(0),
                (!rtvs.is_empty()).then_some(rtvs.as_ptr()),
                false,
                dsv.as_ref().map(std::ptr::from_ref),
            );
            self.list
                .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        }
        let mut d = Draw {
            dev: &*self.dev,
            list: &self.list,
            compute: false,
        };
        d.set_viewport(area);
        d.set_scissor(area);
        let mut d = crate::front::Draw {
            b: crate::front::DrawB::Dx(d),
        };
        draw(&mut d)
    }

    /// Record a compute pass: its barriers, then `dispatch` inside.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a resource the device does not hold, or what `dispatch` returns.
    pub fn compute(
        &mut self,
        touches: &[Touch],
        dispatch: impl FnOnce(&mut crate::front::Draw<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.touch(touches)?;
        let mut d = crate::front::Draw {
            b: crate::front::DrawB::Dx(Draw {
                dev: &*self.dev,
                list: &self.list,
                compute: true,
            }),
        };
        dispatch(&mut d)
    }

    /// The buffer resource of `b`.
    fn raw_buffer(&self, b: Buffer) -> Result<ID3D12Resource, Error> {
        Ok(self.dev.buffer(b)?.resource.clone())
    }

    /// Fill `size` bytes of `b` from `offset` with copies of `value`: staged and copied.
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
        self.touch(&[Touch::Buffer(b, Use::TransferDst)])?;
        let words = usize::try_from(size / 4).unwrap_or(0);
        let bytes: Vec<u8> = std::iter::repeat_n(value.to_le_bytes(), words)
            .flatten()
            .collect();
        let (src, at) = self.dev.stage(&bytes, 16)?;
        let dst = self.raw_buffer(b)?;
        // SAFETY: the list is open; both buffers are live, the ranges inside each.
        unsafe {
            self.list
                .CopyBufferRegion(&dst, offset, &src, at, bytes.len() as u64)
        };
        Ok(())
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
        self.touch(&[
            Touch::Buffer(src, Use::TransferSrc),
            Touch::Buffer(dst, Use::TransferDst),
        ])?;
        let (s, d) = (self.raw_buffer(src)?, self.raw_buffer(dst)?);
        // SAFETY: the list is open; both buffers are live, the ranges inside each.
        unsafe {
            self.list
                .CopyBufferRegion(&d, dst_offset, &s, src_offset, size)
        };
        Ok(())
    }

    /// Copy one level of `t` into `dst` at `offset`, tightly packed: through a buffer of the
    /// runtime's row pitch where the rows are not already packed so.
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
        self.touch(&[
            Touch::Texture(t, Use::TransferSrc),
            Touch::Buffer(dst, Use::TransferDst),
        ])?;
        let e = self.dev.texture(t)?;
        let (w, h) = ((e.width >> level).max(1), (e.height >> level).max(1));
        let tight_total = e.format.level_bytes(w, h);
        let (fp, rows, row_bytes) = self.dev.footprint(e, level, 0);
        let resource = e.resource.clone();
        let target = self.raw_buffer(dst)?;
        let packed = u64::from(fp.Footprint.RowPitch) == row_bytes && offset.is_multiple_of(512);
        let src = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: a non-owning copy; the resource outlives the call.
            pResource: unsafe { std::mem::transmute_copy(&resource) },
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: level,
            },
        };
        if packed {
            let mut placed = fp;
            placed.Offset = offset;
            let to = D3D12_TEXTURE_COPY_LOCATION {
                // SAFETY: as above.
                pResource: unsafe { std::mem::transmute_copy(&target) },
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: placed,
                },
            };
            // SAFETY: the list is open; the texture is a copy source, the buffer a destination.
            unsafe { self.list.CopyTextureRegion(&to, 0, 0, 0, &src, None) };
            return Ok(());
        }
        // Through a scratch buffer of the runtime's pitch, then row by row.
        let pitch = u64::from(fp.Footprint.RowPitch);
        let scratch = self.scratch(pitch * u64::from(rows))?;
        let to = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: as above.
            pResource: unsafe { std::mem::transmute_copy(&scratch) },
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: fp,
            },
        };
        let tight = tight_total / u64::from(rows.max(1));
        // SAFETY: the list is open; the scratch buffer is a fresh copy destination, then a copy
        // source for the rows.
        unsafe {
            self.list.CopyTextureRegion(&to, 0, 0, 0, &src, None);
            put(
                &self.list,
                &[transition(
                    &scratch,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_COPY_SOURCE,
                )],
            );
            for r in 0..u64::from(rows) {
                self.list
                    .CopyBufferRegion(&target, offset + r * tight, &scratch, r * pitch, tight);
            }
        }
        self.dev.defer(Garbage::Scratch(scratch));
        Ok(())
    }

    /// A device-local buffer of `size` bytes, a copy destination, for this recording alone.
    fn scratch(&self, size: u64) -> Result<ID3D12Resource, Error> {
        let props = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
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
            ..Default::default()
        };
        let mut resource: Option<ID3D12Resource> = None;
        // SAFETY: live locals.
        hr("CreateCommittedResource", unsafe {
            self.dev.device.CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut resource,
            )
        })?;
        resource.ok_or_else(|| Error::Allocation("no scratch buffer".into()))
    }

    /// Copy `src`'s first level into `dst`'s.
    ///
    /// # Errors
    /// [`Error::Invalid`] for textures of different sizes or formats; a handle that is not live.
    pub fn copy_texture(&mut self, src: Texture, dst: Texture) -> Result<(), Error> {
        let (a, b) = (self.dev.texture(src)?, self.dev.texture(dst)?);
        if (a.width, a.height, a.format) != (b.width, b.height, b.format) {
            return Err(Error::Invalid(
                "a copy between textures of different sizes or formats".into(),
            ));
        }
        let (from, to) = (a.resource.clone(), b.resource.clone());
        self.touch(&[
            Touch::Texture(src, Use::TransferSrc),
            Touch::Texture(dst, Use::TransferDst),
        ])?;
        let loc = |r: &ID3D12Resource| D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: a non-owning copy; the resource outlives the call.
            pResource: unsafe { std::mem::transmute_copy(r) },
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: 0,
            },
        };
        // SAFETY: the list is open; the textures are in the copy states.
        unsafe {
            self.list
                .CopyTextureRegion(&loc(&to), 0, 0, 0, &loc(&from), None)
        };
        Ok(())
    }

    /// End and submit a recording of the device's own.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a recording into a lent list; any failure of the runtime.
    pub fn submit(mut self) -> Result<Ticket, Error> {
        if !self.own {
            return Err(Error::Invalid(
                "a lent command list is submitted by its owner".into(),
            ));
        }
        self.close()?;
        Ok(Ticket(self.at.value))
    }

    /// Finish a recording into a lent list; a recording of the device's own is submitted.
    ///
    /// # Errors
    /// Any failure of the runtime.
    pub fn finish(mut self) -> Result<(), Error> {
        self.close()
    }

    fn close(&mut self) -> Result<(), Error> {
        if self.done {
            return Ok(());
        }
        self.done = true;
        if let Some(t) = self.timer.take() {
            let n = u32::try_from(t.labels.len()).unwrap_or(0);
            if n > 0 {
                // SAFETY: the list is open; the read-back buffer stays a copy destination.
                unsafe {
                    self.list.ResolveQueryData(
                        &t.heap,
                        D3D12_QUERY_TYPE_TIMESTAMP,
                        0,
                        n,
                        &t.readback,
                        0,
                    );
                }
            }
            self.dev.garbage.push((self.at, Garbage::Timer(t)));
        }
        self.dev.stats.recordings += 1;
        if !self.own {
            return Ok(());
        }
        // SAFETY: recording is finished; the list is the device's own.
        hr("Close", unsafe { self.list.Close() })?;
        let lists = [Some(hr("cast", self.list.cast::<ID3D12CommandList>())?)];
        // SAFETY: the queue is live and driven from this one thread.
        unsafe { self.dev.queue.ExecuteCommandLists(&lists) };
        // SAFETY: as above.
        let signalled = hr("Signal", unsafe {
            self.dev.queue.Signal(&self.dev.fence, self.at.value)
        });
        if let Some(a) = self.alloc.take() {
            self.dev
                .garbage
                .push((self.at, Garbage::Command(a, self.list.clone())));
        }
        signalled
    }
}

impl Drop for Recorder<'_> {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            tracing::error!(target: "dereth::gfx", "a recording could not be submitted: {e}");
        }
    }
}

/// Inside a pass: the pipeline, push constants, viewport and the draws or dispatches.
pub struct Draw<'r> {
    dev: &'r Device,
    list: &'r ID3D12GraphicsCommandList,
    compute: bool,
}

impl std::fmt::Debug for Draw<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Draw")
            .field("compute", &self.compute)
            .finish_non_exhaustive()
    }
}

impl Draw<'_> {
    /// Use pipeline `p` for what follows.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a pipeline the device does not hold, or one of the other kind.
    pub fn set_pipeline(&mut self, p: Pipeline) -> Result<(), Error> {
        let e = self
            .dev
            .pipelines
            .get(p.0 as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| Error::Invalid(format!("no pipeline {}", p.0)))?;
        if e.compute != self.compute {
            return Err(Error::Invalid("a pipeline of the other kind".into()));
        }
        // SAFETY: the list is open; the pipeline is live.
        unsafe { self.list.SetPipelineState(&e.pipeline) };
        Ok(())
    }

    /// Push `bytes` at `offset` into the root constants every stage reads.
    ///
    /// # Errors
    /// [`Error::Invalid`] past [`PUSH_BYTES`], or not whole words.
    pub fn push(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Error> {
        if offset as usize + bytes.len() > PUSH_BYTES as usize {
            return Err(Error::Invalid("push constants past their end".into()));
        }
        if !offset.is_multiple_of(4) || !bytes.len().is_multiple_of(4) {
            return Err(Error::Invalid("push constants not whole words".into()));
        }
        let words: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let n = u32::try_from(words.len()).unwrap_or(0);
        // SAFETY: the list is open; the words lie inside the root constants.
        unsafe {
            if self.compute {
                self.list
                    .SetComputeRoot32BitConstants(0, n, words.as_ptr().cast(), offset / 4);
            } else {
                self.list
                    .SetGraphicsRoot32BitConstants(0, n, words.as_ptr().cast(), offset / 4);
            }
        }
        Ok(())
    }

    /// Draw into `r` of the targets: clip space y up, z from 0 to 1.
    pub fn set_viewport(&mut self, r: Rect) {
        #[allow(clippy::cast_precision_loss)] // a target's extent
        let v = D3D12_VIEWPORT {
            TopLeftX: r.x as f32,
            TopLeftY: r.y as f32,
            Width: r.width as f32,
            Height: r.height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        // SAFETY: the list is open.
        unsafe { self.list.RSSetViewports(&[v]) };
    }

    /// Draw only inside `r`.
    pub fn set_scissor(&mut self, r: Rect) {
        let s = RECT {
            left: r.x,
            top: r.y,
            right: r.x + i32::try_from(r.width).unwrap_or(i32::MAX),
            bottom: r.y + i32::try_from(r.height).unwrap_or(i32::MAX),
        };
        // SAFETY: the list is open.
        unsafe { self.list.RSSetScissorRects(&[s]) };
    }

    fn raw_buffer(&self, b: Buffer) -> Result<&ID3D12Resource, Error> {
        Ok(&self.dev.buffer(b)?.resource)
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
        let e = self.dev.buffer(b)?;
        // SAFETY: a pure query on a live buffer.
        let va = unsafe { e.resource.GetGPUVirtualAddress() };
        let view = D3D12_INDEX_BUFFER_VIEW {
            BufferLocation: va + offset,
            SizeInBytes: u32::try_from(e.size.saturating_sub(offset)).unwrap_or(u32::MAX),
            Format: match ty {
                IndexType::U16 => DXGI_FORMAT_R16_UINT,
                IndexType::U32 => DXGI_FORMAT_R32_UINT,
            },
        };
        // SAFETY: the list is open; the view lives for the call.
        unsafe { self.list.IASetIndexBuffer(Some(&view)) };
        Ok(())
    }

    /// The draw's item, as its root constant.
    fn item(&self, first_instance: u32) {
        // SAFETY: the list is open.
        unsafe { self.list.SetGraphicsRoot32BitConstant(1, first_instance, 0) };
    }

    /// Draw `vertices` vertices, `instances` times; `first_instance` is the draw's item.
    pub fn draw(&mut self, vertices: u32, instances: u32, first_vertex: u32, first_instance: u32) {
        self.item(first_instance);
        // SAFETY: the list is open, inside a pass with a pipeline set.
        unsafe {
            self.list
                .DrawInstanced(vertices, instances, first_vertex, first_instance);
        }
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
        self.item(first_instance);
        // SAFETY: as above, with indices bound.
        unsafe {
            self.list.DrawIndexedInstanced(
                indices,
                instances,
                first_index,
                vertex_offset,
                first_instance,
            );
        }
    }

    fn indirect(
        &self,
        signature: &ID3D12CommandSignature,
        want: u32,
        (b, offset, stride): (Buffer, u64, u32),
        max: u32,
        count: Option<(Buffer, u64)>,
    ) -> Result<(), Error> {
        if stride != want {
            return Err(Error::Invalid(format!(
                "indirect commands {stride} bytes apart, where Direct3D 12's are {want}"
            )));
        }
        let args = self.raw_buffer(b)?;
        let counter = match count {
            Some((c, _)) => Some(self.raw_buffer(c)?),
            None => None,
        };
        // SAFETY: the list is open, inside a pass with a pipeline set; the buffers are live.
        unsafe {
            self.list.ExecuteIndirect(
                signature,
                max,
                args,
                offset,
                counter,
                count.map_or(0, |c| c.1),
            );
        }
        Ok(())
    }

    /// `count` draws from the commands in `b` at `offset`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold, or another stride than its own.
    pub fn draw_indirect(
        &mut self,
        b: Buffer,
        offset: u64,
        count: u32,
        stride: u32,
    ) -> Result<(), Error> {
        self.indirect(
            &self.dev.signatures.plain,
            PLAIN_STRIDE,
            (b, offset, stride),
            count,
            None,
        )
    }

    /// `count` indexed draws from the commands in `b` at `offset`, `stride` bytes apart.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold, or another stride than its own.
    pub fn draw_indexed_indirect(
        &mut self,
        b: Buffer,
        offset: u64,
        count: u32,
        stride: u32,
    ) -> Result<(), Error> {
        self.indirect(
            &self.dev.signatures.indexed,
            INDEXED_STRIDE,
            (b, offset, stride),
            count,
            None,
        )
    }

    /// The indexed draws in `b` at `offset`, as many as the count in `count_buffer` says and at
    /// most `max`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold, or another stride than its own.
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
        self.indirect(
            &self.dev.signatures.indexed,
            INDEXED_STRIDE,
            (b, offset, stride),
            max,
            Some((count_buffer, count_offset)),
        )
    }

    /// The draws in `b` at `offset`, as many as the count in `count_buffer` says and at most
    /// `max`.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a buffer the device does not hold, or another stride than its own.
    pub fn draw_indirect_count(
        &mut self,
        b: Buffer,
        offset: u64,
        count_buffer: Buffer,
        count_offset: u64,
        max: u32,
        stride: u32,
    ) -> Result<(), Error> {
        self.indirect(
            &self.dev.signatures.plain,
            PLAIN_STRIDE,
            (b, offset, stride),
            max,
            Some((count_buffer, count_offset)),
        )
    }

    /// Run `x * y * z` groups of the bound compute pipeline.
    pub fn dispatch(&mut self, x: u32, y: u32, z: u32) {
        // SAFETY: the list is open, in a compute pass with a pipeline set.
        unsafe { self.list.Dispatch(x, y, z) };
    }
}
