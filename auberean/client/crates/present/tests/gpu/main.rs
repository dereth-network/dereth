//! The composer on a graphics device: an interface's textured batches are drawn over the frame
//! with the texture, the color and the combining their batch names, those before its first
//! preview space under its viewports and its picture in front.
//!
//! Fixture: a hardware adapter (a software rasteriser is refused). These tests run with
//! `--features retail-dats`, the gpu tier's switch, though they read no data file.
//!
//! Behaviour: none (how this client's overlay batches reach the device, which no game behavior names)

use auberean_gpu::Renderer;
use auberean_present::Composer;
use auberean_shell::interface::{
    Canvas, Overlay, OverlayDepth, OverlayItem, OverlayMaterial, OverlaySampler, OverlaySpace,
    OverlayTexture, OverlayVertex, PreviewSpace, TextureData, TextureFormat, Viewport,
};

const SIZE: u32 = 8;

fn key(key: u64) -> OverlayTexture {
    OverlayTexture {
        space: OverlaySpace::Local,
        key,
    }
}

/// A one-texel picture: blue, green, red, alpha.
fn texel(bgra: [u8; 4]) -> TextureData {
    TextureData {
        width: 1,
        height: 1,
        format: TextureFormat::Bgra8,
        levels: vec![bgra.to_vec()],
    }
}

/// Two triangles over the clip-space rectangle `[left, top, right, bottom]` (`y` up), in `color`.
fn rect(
    texture: OverlayTexture,
    material: OverlayMaterial,
    [l, t, r, b]: [f32; 4],
    color: u32,
) -> OverlayItem {
    let v = |x: f32, y: f32| OverlayVertex {
        position: [x, y, 0.0],
        color,
        uv: [0.5, 0.5],
    };
    OverlayItem::Triangles {
        material,
        texture,
        sampler: OverlaySampler::POINT_CLAMP,
        depth: OverlayDepth::Over,
        vertices: vec![v(l, t), v(r, t), v(l, b), v(r, t), v(r, b), v(l, b)],
    }
}

/// The composed picture's texels, red, green, blue, alpha, row by row from the top.
fn compose(renderer: &mut Renderer, composer: &mut Composer, overlay: &Overlay) -> Vec<u8> {
    let mut encoder = renderer
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let picture = composer.compose(
        renderer,
        (SIZE, SIZE),
        None,
        None,
        None,
        overlay,
        &[],
        &mut |_| None,
        &mut encoder,
    );
    let row = (SIZE * 4).div_ceil(256) * 256;
    let buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &picture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("the picture read back"));
    renderer
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the device finishes");
    let data = slice.get_mapped_range().expect("mapped");
    let mut rgba = Vec::new();
    for y in 0..SIZE {
        let s = (y * row) as usize;
        rgba.extend_from_slice(&data[s..s + (SIZE * 4) as usize]);
    }
    rgba
}

fn at(rgba: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * SIZE + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2]]
}

/// An interface's textured batches are drawn over the frame: an image as its texture times its
/// color, text in its color alone through its texture's coverage, an inverting batch as one less
/// what is beneath; a batch whose texture is not there draws nothing, and a texture let go of
/// leaves the device, its batch then drawing nothing.
#[test]
fn an_interface_s_textured_batches_are_drawn_with_their_texture_color_and_combining() {
    let mut renderer = Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
    let mut composer = Composer::new(&renderer);
    let (white, glyph, missing) = (key(1), key(2), key(3));
    let mut overlay = Overlay::default();
    assert!(overlay.upload(white, &texel([255, 255, 255, 255])));
    // Black, but whole coverage: text takes only its alpha.
    assert!(overlay.upload(glyph, &texel([0, 0, 0, 255])));
    overlay.items = vec![
        rect(
            white,
            OverlayMaterial::Image,
            [-1.0, 1.0, 0.0, -1.0],
            0xFFFF_0000,
        ),
        rect(
            white,
            OverlayMaterial::Invert,
            [-1.0, 0.0, 0.0, -1.0],
            0xFFFF_FFFF,
        ),
        rect(
            glyph,
            OverlayMaterial::Text,
            [0.0, 1.0, 1.0, 0.0],
            0xFF00_FF00,
        ),
        rect(
            missing,
            OverlayMaterial::Image,
            [0.0, 0.0, 1.0, -1.0],
            0xFFFF_FFFF,
        ),
    ];
    let rgba = compose(&mut renderer, &mut composer, &overlay);
    let backdrop = at(&rgba, 6, 6);
    assert!(
        backdrop.iter().all(|c| *c < 16),
        "nothing over the backdrop: {backdrop:?}"
    );
    assert_eq!(at(&rgba, 1, 1), [255, 0, 0], "the image, tinted red");
    assert_eq!(at(&rgba, 1, 6), [0, 255, 255], "the red inverted");
    assert_eq!(at(&rgba, 6, 1), [0, 255, 0], "the text in its color");
    overlay.release(glyph);
    let rgba = compose(&mut renderer, &mut composer, &overlay);
    assert_eq!(at(&rgba, 6, 1), backdrop, "its texture let go of");
    assert_eq!(at(&rgba, 1, 1), [255, 0, 0], "the rest as it was");
}

/// The batches an interface draws before its first preview space stand under its viewports'
/// scenes, and so under its picture in front; those after it are drawn over that picture.
#[test]
fn batches_before_an_interfaces_preview_space_are_drawn_under_its_picture_in_front() {
    let mut renderer = Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
    let mut composer = Composer::new(&renderer);
    let white = key(1);
    let mut overlay = Overlay::default();
    assert!(overlay.upload(white, &texel([255, 255, 255, 255])));
    // The picture in front: blue, wholly opaque, over the whole window.
    overlay.front = Some(std::sync::Arc::new(Canvas {
        width: SIZE,
        height: SIZE,
        bgra: [255, 0, 0, 255].repeat((SIZE * SIZE) as usize),
        revision: 1,
    }));
    overlay.items = vec![
        rect(
            white,
            OverlayMaterial::Image,
            [-1.0, 1.0, 0.0, -1.0],
            0xFFFF_0000,
        ),
        OverlayItem::Preview {
            space: PreviewSpace::PaperDoll,
            rect: Viewport {
                x: 0,
                y: 0,
                width: SIZE,
                height: SIZE,
            },
        },
        rect(
            white,
            OverlayMaterial::Image,
            [0.0, 1.0, 1.0, -1.0],
            0xFF00_FF00,
        ),
    ];
    let rgba = compose(&mut renderer, &mut composer, &overlay);
    assert_eq!(
        at(&rgba, 1, 4),
        [0, 0, 255],
        "the red under the picture in front"
    );
    assert_eq!(at(&rgba, 6, 4), [0, 255, 0], "the green over it");
}
