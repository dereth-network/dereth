//! The uniforms of a frame, written into buffers kept between frames: each draw takes the next
//! slot of a buffer, and the slots taken are written to the device in one go before the frame is
//! submitted. A buffer has one bind group, which each draw binds at its slot's offset: moving a
//! binding along its buffer costs a frame's hundreds of draws far less than changing to a group of
//! their own. Without the pool every draw made a buffer and a bind group each frame, which with
//! the hundreds of draws inside a building cost most of a frame.

struct Chunk {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    data: Vec<u8>,
}

/// A slot's place: its buffer's bind group and the offset to bind it at.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Slot {
    pub(crate) group: wgpu::BindGroup,
    pub(crate) offset: u32,
}

/// Uniform slots for the draws of a frame.
pub(crate) struct UniformPool {
    chunks: Vec<Chunk>,
    /// Bytes from one slot to the next: the uniforms' size rounded up to the device's alignment.
    stride: usize,
    /// The uniforms' size.
    size: usize,
    /// Slots in each buffer.
    slots: usize,
    used: usize,
}

impl UniformPool {
    pub(crate) fn new(device: &wgpu::Device, size: usize, slots: usize) -> Self {
        let align = device.limits().min_uniform_buffer_offset_alignment as usize;
        Self {
            chunks: Vec::new(),
            stride: size.div_ceil(align.max(1)) * align.max(1),
            size,
            slots,
            used: 0,
        }
    }

    /// A frame starts: every slot is free again. What earlier frames wrote has already been
    /// handed to the device in submission order, so overwriting it here is safe.
    pub(crate) fn begin(&mut self) {
        self.used = 0;
    }

    /// The slot holding `bytes` once the frame is written: the last one taken when it holds the
    /// same, else the next.
    pub(crate) fn take(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        bytes: &[u8],
    ) -> Slot {
        if let Some(last) = self.used.checked_sub(1) {
            let chunk = &self.chunks[last / self.slots];
            let at = (last % self.slots) * self.stride;
            let n = bytes.len().min(self.size);
            if chunk.data[at..at + n] == bytes[..n] {
                return Slot {
                    group: chunk.group.clone(),
                    offset: u32::try_from(at).unwrap_or(u32::MAX),
                };
            }
        }
        let (c, k) = (self.used / self.slots, self.used % self.slots);
        self.used += 1;
        if c == self.chunks.len() {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("draw uniforms"),
                size: (self.stride * self.slots) as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(self.size as u64),
                    }),
                }],
            });
            self.chunks.push(Chunk {
                buffer,
                group,
                data: vec![0; self.stride * self.slots],
            });
        }
        let chunk = &mut self.chunks[c];
        let at = k * self.stride;
        let n = bytes.len().min(self.size);
        chunk.data[at..at + n].copy_from_slice(&bytes[..n]);
        Slot {
            group: chunk.group.clone(),
            offset: u32::try_from(at).unwrap_or(u32::MAX),
        }
    }

    /// The bytes of the slots taken this frame.
    pub(crate) fn bytes_taken(&self) -> u64 {
        (self.used * self.stride) as u64
    }

    /// Write the slots taken this frame to the device, before the frame is submitted.
    pub(crate) fn write(&self, queue: &wgpu::Queue) {
        let mut left = self.used;
        for chunk in &self.chunks {
            if left == 0 {
                break;
            }
            let n = left.min(self.slots);
            queue.write_buffer(&chunk.buffer, 0, &chunk.data[..n * self.stride]);
            left -= n;
        }
    }
}
