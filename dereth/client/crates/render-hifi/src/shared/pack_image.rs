//! An art pack's pictures on the device: eight bits a channel, or blocks where the picture was
//! compressed and the device samples blocks, decoded to eight bits a channel where it does not.
//! Either way the shaders read the same channels.

use dereth_render::block_codec::{self, BlockFormat};

use crate::snapshot::HifiPackImage;

/// Whether `device` samples block-compressed pictures.
#[must_use]
pub fn samples_blocks(device: &wgpu::Device) -> bool {
    device
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
}

/// The texture format `image` is drawn from, on a device that samples blocks (`blocks`) or not.
#[must_use]
pub fn format(image: &HifiPackImage, blocks: bool) -> wgpu::TextureFormat {
    use wgpu::TextureFormat as F;
    match (image.format.filter(|_| blocks), image.srgb) {
        (None, false) => F::Rgba8Unorm,
        (None, true) => F::Rgba8UnormSrgb,
        (Some(BlockFormat::Bc4), _) => F::Bc4RUnorm,
        (Some(BlockFormat::Bc5), _) => F::Bc5RgUnorm,
        (Some(BlockFormat::Bc7), false) => F::Bc7RgbaUnorm,
        (Some(BlockFormat::Bc7), true) => F::Bc7RgbaUnormSrgb,
    }
}

/// The bytes `image`'s levels take on a device that samples blocks (`blocks`) or not.
#[must_use]
pub fn device_bytes(image: &HifiPackImage, blocks: bool) -> u64 {
    if image.format.is_none() || blocks {
        return image.levels.iter().map(|l| l.len() as u64).sum();
    }
    (0..image.levels.len())
        .map(|k| {
            let w = u64::from((image.width >> k).max(1));
            let h = u64::from((image.height >> k).max(1));
            w * h * 4
        })
        .sum()
}

/// Write the first `levels` levels of `image` into layer `layer` of `texture` (made with
/// [`format()`] for `image`), decoding blocks where the texture holds eight bits a channel. A level
/// whose bytes fall short is left unwritten, with the ones after it.
pub fn write(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    layer: u32,
    image: &HifiPackImage,
    levels: usize,
) {
    let blocks = texture.format().block_dimensions() != (1, 1);
    for (level, bytes) in (0u32..).zip(image.levels.iter().take(levels)) {
        let w = (image.width >> level).max(1);
        let h = (image.height >> level).max(1);
        let decoded;
        let (data, row, rows, extent): (&[u8], u32, u32, (u32, u32)) = match image.format {
            Some(f) if blocks => {
                if bytes.len() < f.len(w, h) {
                    break;
                }
                let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
                // LINT-OK: eight or sixteen.
                #[allow(clippy::cast_possible_truncation)]
                let size = f.block_bytes() as u32;
                (bytes.as_slice(), bw * size, bh, (bw * 4, bh * 4))
            }
            Some(f) => {
                if bytes.len() < f.len(w, h) {
                    break;
                }
                decoded = block_codec::decode(f, w, h, bytes);
                (decoded.as_slice(), w * 4, h, (w, h))
            }
            None => {
                if bytes.len() < (w as usize) * (h as usize) * 4 {
                    break;
                }
                (bytes.as_slice(), w * 4, h, (w, h))
            }
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width: extent.0,
                height: extent.1,
                depth_or_array_layers: 1,
            },
        );
    }
}
