//! Vulkan implementation of the video-memory AUTOGEN path, not D3DX system-memory BOX.
//! The source requests flags 4, which translates to D3DUSAGE_AUTOGENMIPMAP. D3D9's default
//! autogen filter is LINEAR
//! (Microsoft, IDirect3DBaseTexture9::SetAutoGenFilterType). No CPU rounding or BC encoder is
//! inferred here. Each destination texel samples the preceding level at its normalized centre
//! with linear filtering -- which is exactly what `vkCmdBlitImage` with `VK_FILTER_LINEAR` does
//! for a half-size blit, so the D3D12 build's full-screen-triangle pass is one blit per level
//! here. Cross-driver bytes are not promised.

use super::*;

/// Vulkan has no AUTOGEN bit. This backend requires the actual BGRA8 format to support the
/// operations used to implement it. Failure keeps the original provided-level resource.
pub(super) fn supported(instance: &ash::Instance, physical_device: vk::PhysicalDevice) -> bool {
    // SAFETY: a pure query on a live physical device.
    let props = unsafe {
        instance.get_physical_device_format_properties(physical_device, vk::Format::B8G8R8A8_UNORM)
    };
    let required = vk::FormatFeatureFlags::SAMPLED_IMAGE
        | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
        | vk::FormatFeatureFlags::BLIT_SRC
        | vk::FormatFeatureFlags::BLIT_DST
        | vk::FormatFeatureFlags::TRANSFER_SRC
        | vk::FormatFeatureFlags::TRANSFER_DST;
    props.optimal_tiling_features.contains(required)
}

/// Record the chain. The image starts with level 0 in `TRANSFER_DST_OPTIMAL` (just copied) and
/// every other level in `UNDEFINED`; all levels end in `SHADER_READ_ONLY_OPTIMAL`.
///
/// # Safety
/// `cmd` must be recording, `image` a live 2D image of `levels` levels with `TRANSFER_SRC` and
/// `TRANSFER_DST` usage.
pub(super) unsafe fn record(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    width: u32,
    height: u32,
    levels: u16,
) {
    let (mut w, mut h) = (width, height);
    for level in 1..u32::from(levels) {
        // The source level: done being written (copied or blitted into), now read by the blit.
        transition_level(
            device,
            cmd,
            image,
            level - 1,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_READ,
        );
        let (nw, nh) = (crate::mip::half(w), crate::mip::half(h));
        let blit = vk::ImageBlit::default()
            .src_subresource(layers(level - 1))
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: w as i32,
                    y: h as i32,
                    z: 1,
                },
            ])
            .dst_subresource(layers(level))
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: nw as i32,
                    y: nh as i32,
                    z: 1,
                },
            ]);
        // SAFETY (caller's obligation, restated): the list is open and both levels exist.
        device.cmd_blit_image(
            cmd,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[blit],
            vk::Filter::LINEAR,
        );
        // The source level is finished with for good.
        transition_level(
            device,
            cmd,
            image,
            level - 1,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags::SHADER_READ,
        );
        w = nw;
        h = nh;
    }
    // The last level was only ever written.
    transition_level(
        device,
        cmd,
        image,
        u32::from(levels) - 1,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::AccessFlags::TRANSFER_WRITE,
        vk::AccessFlags::SHADER_READ,
    );
}

/// One colour mip level, as `VkImageSubresourceLayers`.
pub(super) fn layers(level: u32) -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: level,
        base_array_layer: 0,
        layer_count: 1,
    }
}

/// One colour mip level, as `VkImageSubresourceRange`.
pub(super) fn level_range(level: u32) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: level,
        level_count: 1,
        base_array_layer: 0,
        layer_count: 1,
    }
}

/// Issue one layout transition on one mip level, at the transfer/fragment stages this file and
/// its callers use.
///
/// # Safety
/// `cmd` must be recording; the named level must be in `from` and remain alive through execution.
#[allow(clippy::too_many_arguments)] // one parameter per input the call takes
pub(super) unsafe fn transition_level(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    level: u32,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
) {
    let stage = |layout: vk::ImageLayout| match layout {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::ImageLayout::UNDEFINED => vk::PipelineStageFlags::TOP_OF_PIPE,
        _ => vk::PipelineStageFlags::TRANSFER,
    };
    let barrier = vk::ImageMemoryBarrier::default()
        .src_access_mask(src_access)
        .dst_access_mask(dst_access)
        .old_layout(from)
        .new_layout(to)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(level_range(level));
    // SAFETY (caller's obligation, restated): the list is open and the state matches.
    device.cmd_pipeline_barrier(
        cmd,
        stage(from),
        stage(to),
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier],
    );
}

impl Gpu {
    /// Read an actual BGRA8 texture subresource for diagnostics, without rebuilding its pixels
    /// from source data. Uses its own command buffer and restores shader-read state. Does not add
    /// a cache link or expose/rewrite any persistent descriptor.
    ///
    /// # Errors
    /// An absent slot, non-BGRA resource, invalid level, or device/readback failure.
    pub fn capture_texture_level(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<CapturedImage, RenderError> {
        let texture = self
            .textures
            .get(&slot.0)
            .ok_or(RenderError::Unsupported("texture slot is not live"))?;
        if texture.format != vk::Format::B8G8R8A8_UNORM {
            return Err(RenderError::Unsupported(
                "readback requires an existing BGRA8 mip",
            ));
        }
        let mut data = self.capture_texture_level_data(slot, level)?;
        Ok(CapturedImage {
            width: data.width,
            height: data.height,
            bgra: data.levels.remove(0),
        })
    }

    /// Copy one actual resident mip, preserving compressed block bytes when applicable.
    /// Tightly packed rows straight out of `vkCmdCopyImageToBuffer`; no decode/re-encode or
    /// policy recomputation.
    /// # Errors
    /// An absent slot, unsupported format, invalid level, or device/readback failure.
    pub fn capture_texture_level_data(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<TextureData, RenderError> {
        let (image, tex_format, levels, width, height) = {
            let t = self
                .textures
                .get(&slot.0)
                .ok_or(RenderError::Unsupported("texture slot is not live"))?;
            (t.image, t.tex_format, t.levels, t.width, t.height)
        };
        let format = match tex_format {
            TextureFormat::Bgra8 => TextureFormat::Bgra8,
            TextureFormat::Bc1 => TextureFormat::Bc1,
            TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => TextureFormat::Bc2,
            TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => TextureFormat::Bc3,
            _ => {
                return Err(RenderError::Unsupported(
                    "unsupported texture readback format",
                ))
            }
        };
        if level >= levels {
            return Err(RenderError::Unsupported(
                "readback requires an existing mip",
            ));
        }
        let w = (width >> level).max(1);
        let h = (height >> level).max(1);
        let block = is_block_compressed(format);
        let row = if block {
            w.div_ceil(4) as usize * crate::texture::block_bytes(format).unwrap_or(16)
        } else {
            w as usize * 4
        };
        let rows = if block {
            h.div_ceil(4) as usize
        } else {
            h as usize
        };
        let total = (row * rows) as u64;
        let readback = self.create_buffer(
            total.max(4),
            vk::BufferUsageFlags::TRANSFER_DST,
            gpu_allocator::MemoryLocation::GpuToCpu,
            "texture readback",
        )?;
        let buffer = readback.buffer;
        let result = self.one_shot("texture readback", |device, cmd| {
            // SAFETY: the texture's upload completed before publication; every resident
            // subresource is shader-readable. Transition only the requested mip, then restore it.
            unsafe {
                transition_level(
                    device,
                    cmd,
                    image,
                    u32::from(level),
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::SHADER_READ,
                    vk::AccessFlags::TRANSFER_READ,
                );
                let region = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(layers(u32::from(level)))
                    .image_extent(vk::Extent3D {
                        width: w,
                        height: h,
                        depth: 1,
                    });
                device.cmd_copy_image_to_buffer(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer,
                    &[region],
                );
                transition_level(
                    device,
                    cmd,
                    image,
                    u32::from(level),
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::AccessFlags::SHADER_READ,
                );
            }
            Ok(())
        });
        let bytes = result.and_then(|()| readback.read(0, total as usize));
        self.destroy_buffer(readback);
        bytes.map(|levels| TextureData {
            width: w,
            height: h,
            format,
            levels: vec![levels],
        })
    }
}
