//! Staged uploads: copies waiting for the next recording, recorded at its head.

use ash::vk;

use crate::device::Device;
use crate::graph::{Track, Use};
use crate::{Buffer, Texture};

/// One staged copy, waiting for the next recording.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PendingCopy {
    Buffer {
        src: vk::Buffer,
        src_offset: u64,
        dst: Buffer,
        dst_offset: u64,
        size: u64,
    },
    Texture {
        src: vk::Buffer,
        src_offset: u64,
        dst: Texture,
        level: u32,
        width: u32,
        height: u32,
    },
    /// A copy from one of the device's buffers into another, as a pool grows.
    Device { src: Buffer, dst: Buffer, size: u64 },
}

impl Device {
    /// Record every staged copy into `cmd`, at the head of the recording that retires at `at`,
    /// and leave what they
    /// wrote ready for any later use: buffers made visible to every stage, textures moved to the
    /// layout shaders sample.
    pub(crate) fn flush_uploads(&mut self, cmd: vk::CommandBuffer, at: crate::device::Retire) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        // Every texture written goes to the copy layout first, all levels at once.
        let mut to_dst = Vec::new();
        let mut textures: Vec<Texture> = Vec::new();
        for p in &pending {
            if let PendingCopy::Texture { dst, .. } = p {
                if !textures.contains(dst) {
                    textures.push(*dst);
                }
            }
        }
        for t in &textures {
            let Some(e) = self.textures.get_mut(t.0) else {
                continue;
            };
            if let Some(b) = e.track.to(Use::TransferDst, true) {
                to_dst.push(image_barrier(b, e.image, e.format.aspect(), e.mips));
            }
        }
        // The copies overwrite what earlier frames, still running, may read or write: they wait
        // for all of it.
        let before = [vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ | vk::AccessFlags2::TRANSFER_WRITE)];
        let dep = vk::DependencyInfo::default()
            .memory_barriers(&before)
            .image_memory_barriers(&to_dst);
        // SAFETY: `cmd` is recording outside any rendering; every image is live.
        unsafe { self.device.cmd_pipeline_barrier2(cmd, &dep) };
        for p in &pending {
            match *p {
                PendingCopy::Buffer {
                    src,
                    src_offset,
                    dst,
                    dst_offset,
                    size,
                } => {
                    let Some(e) = self.buffers.get_mut(dst.0) else {
                        continue;
                    };
                    let region = vk::BufferCopy {
                        src_offset,
                        dst_offset,
                        size,
                    };
                    // SAFETY: both buffers are live and the range is inside each.
                    unsafe { self.device.cmd_copy_buffer(cmd, src, e.buffer, &[region]) };
                    e.track = Track {
                        layout: vk::ImageLayout::UNDEFINED,
                        stages: vk::PipelineStageFlags2::ALL_TRANSFER,
                        access: vk::AccessFlags2::TRANSFER_WRITE,
                        written: false,
                    };
                }
                PendingCopy::Device { src, dst, size } => {
                    let (Some(s), Some(d)) = (self.buffers.get(src.0), self.buffers.get(dst.0))
                    else {
                        continue;
                    };
                    let region = vk::BufferCopy {
                        src_offset: 0,
                        dst_offset: 0,
                        size,
                    };
                    // SAFETY: both buffers are live and `size` is inside each.
                    unsafe {
                        self.device
                            .cmd_copy_buffer(cmd, s.buffer, d.buffer, &[region])
                    };
                }
                PendingCopy::Texture {
                    src,
                    src_offset,
                    dst,
                    level,
                    width,
                    height,
                } => {
                    let Some(e) = self.textures.get(dst.0) else {
                        continue;
                    };
                    let region = vk::BufferImageCopy::default()
                        .buffer_offset(src_offset)
                        .image_subresource(vk::ImageSubresourceLayers {
                            aspect_mask: vk::ImageAspectFlags::COLOR,
                            mip_level: level,
                            base_array_layer: 0,
                            layer_count: 1,
                        })
                        .image_extent(vk::Extent3D {
                            width,
                            height,
                            depth: 1,
                        });
                    // SAFETY: the image is in the copy layout; the source range is staged.
                    unsafe {
                        self.device.cmd_copy_buffer_to_image(
                            cmd,
                            src,
                            e.image,
                            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                            &[region],
                        );
                    }
                }
            }
        }
        // What the copies wrote is visible to every later command: the buffers by one memory
        // barrier, the textures by their move to the sampled layout.
        let memory = [vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
            .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)];
        let mut to_read = Vec::new();
        for t in &textures {
            let Some(e) = self.textures.get_mut(t.0) else {
                continue;
            };
            if let Some(b) = e.track.to(Use::Sampled, true) {
                to_read.push(image_barrier(b, e.image, e.format.aspect(), e.mips));
            }
            // Ready for any shader stage: a later sampling needs no barrier of its own.
            e.track.written = false;
        }
        let dep = vk::DependencyInfo::default()
            .memory_barriers(&memory)
            .image_memory_barriers(&to_read);
        // SAFETY: as above.
        unsafe { self.device.cmd_pipeline_barrier2(cmd, &dep) };
        // The arenas the copies read go back to the free list once this recording is done.
        let arenas = std::mem::take(&mut self.staging.current);
        for a in arenas {
            self.garbage.push((at, crate::device::Garbage::Staging(a)));
        }
    }
}

/// One image barrier for the whole of an image's levels.
pub(crate) fn image_barrier(
    b: crate::graph::Barrier,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
    mips: u32,
) -> vk::ImageMemoryBarrier2<'static> {
    vk::ImageMemoryBarrier2::default()
        .src_stage_mask(b.src_stages)
        .src_access_mask(b.src_access)
        .dst_stage_mask(b.dst_stages)
        .dst_access_mask(b.dst_access)
        .old_layout(b.old_layout)
        .new_layout(b.new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: mips,
            base_array_layer: 0,
            layer_count: 1,
        })
}

/// One buffer barrier for the whole of a buffer.
pub(crate) fn buffer_barrier(
    b: crate::graph::Barrier,
    buffer: vk::Buffer,
) -> vk::BufferMemoryBarrier2<'static> {
    vk::BufferMemoryBarrier2::default()
        .src_stage_mask(b.src_stages)
        .src_access_mask(b.src_access)
        .dst_stage_mask(b.dst_stages)
        .dst_access_mask(b.dst_access)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffer)
        .offset(0)
        .size(vk::WHOLE_SIZE)
}
