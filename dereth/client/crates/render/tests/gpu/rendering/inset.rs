//! A draw region: what is drawn while one is set lands in its rectangle of the back buffer, as
//! on a screen of the region's own size, and nowhere else; lifting it mid-frame draws over the
//! whole back buffer again.
//! Behaviour: none (tooling: a host draws the game inside part of its own window)
//! Fixture: an offscreen device, two flat textures, pixel readback.

#![cfg(gpu)]

use dereth_primitives::{Inset, TextureData, TextureFormat, Viewport};
use dereth_render::device::{PerDrawConstants, PerFrameConstants};
use dereth_render::pso::{PipelineKey, SurfaceContext, SurfaceState};
use dereth_render::surface::{surface_type, Surface, SurfaceHandler};
use dereth_render::{DrawConstants, VertexFormat};

const N: u32 = 64;

fn flat(rgb: (u8, u8, u8)) -> TextureData {
    TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![[rgb.2, rgb.1, rgb.0, 0xFF].repeat(16)],
    }
}

fn opaque() -> SurfaceState {
    PipelineKey::state_from_surface(
        &Surface {
            r#type: surface_type::BASE1_IMAGE,
            handler: SurfaceHandler::Database,
            ..Surface::default()
        },
        SurfaceContext {
            vertex_format: VertexFormat::XyzDiffuseTex1,
            texture_is_set: true,
            texture_has_palette: false,
            two_sided: true,
            fog_enabled: false,
            lighting: false,
            ..SurfaceContext::default()
        },
    )
}

/// Two triangles over the whole of clip space at depth `z`, white diffuse.
fn quad(z: f32) -> Vec<u8> {
    let mut v = Vec::new();
    for (x, y, u, t) in [
        (-1.0f32, -1.0f32, 0.0f32, 1.0f32),
        (1.0, -1.0, 1.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
        (-1.0, -1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
        (-1.0, 1.0, 0.0, 0.0),
    ] {
        for f in [x, y, z] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        v.extend_from_slice(&u.to_le_bytes());
        v.extend_from_slice(&t.to_le_bytes());
    }
    v
}

fn constants() -> (PerFrameConstants, PerDrawConstants) {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    (
        PerFrameConstants {
            view_proj: identity,
            view: identity,
            fog_params: [0.0, 1.0, 0.0, 0.0],
            ..PerFrameConstants::default()
        },
        PerDrawConstants::identity(),
    )
}

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 3] {
    let p = &rgba[((y * N + x) * 4) as usize..][..3];
    [p[0], p[1], p[2]]
}

#[test]
#[ignore = "requires an offscreen rendering device; run this GPU check explicitly"]
fn what_is_drawn_into_a_region_lands_in_its_rectangle_scaled_and_lifting_it_draws_everywhere() {
    let mut gpu = crate::common::text_pixels::warp(N, N);
    let (per_frame, draw) = constants();
    let key = opaque().key;
    let white = gpu.upload_texture(&flat((255, 255, 255))).expect("upload");
    let red = gpu.upload_texture(&flat((255, 0, 0))).expect("upload");
    let blue = gpu.upload_texture(&flat((0, 0, 255))).expect("upload");
    // The region: 32 by 24 pixels at (16, 8), standing for a screen of 64 by 48.
    let region = Inset {
        rect: Viewport {
            x: 16,
            y: 8,
            width: 32,
            height: 24,
        },
        extent: (64, 48),
    };
    gpu.set_inset(Some(region));
    assert_eq!((gpu.size(), gpu.target_size()), ((64, 48), (N, N)));
    gpu.begin_frame().expect("begin");
    // The whole of the region's screen, white; then its top-left quarter, red.
    gpu.bind_texture(white, 1);
    gpu.draw_dynamic(
        &key,
        &DrawConstants::default(),
        &per_frame,
        &draw,
        &quad(0.9),
    )
    .expect("draw");
    gpu.set_viewport(Viewport {
        x: 0,
        y: 0,
        width: 32,
        height: 24,
    });
    gpu.bind_texture(red, 1);
    gpu.draw_dynamic(
        &key,
        &DrawConstants::default(),
        &per_frame,
        &draw,
        &quad(0.5),
    )
    .expect("draw");
    gpu.end_frame().expect("end");
    let rgba = gpu.capture().expect("capture").to_rgba();
    for y in 0..N {
        for x in 0..N {
            let in_region = (16..48).contains(&x) && (8..32).contains(&y);
            let in_quarter = (16..32).contains(&x) && (8..20).contains(&y);
            let want = if in_quarter {
                [255, 0, 0]
            } else if in_region {
                [255, 255, 255]
            } else {
                [0, 0, 0]
            };
            assert_eq!(pixel(&rgba, x, y), want, "pixel ({x}, {y})");
        }
    }
    // Lifted in the middle of a frame, the whole back buffer is drawn on again.
    gpu.begin_frame().expect("begin");
    gpu.set_inset(None);
    assert_eq!(gpu.size(), (N, N));
    gpu.bind_texture(blue, 1);
    gpu.draw_dynamic(
        &key,
        &DrawConstants::default(),
        &per_frame,
        &draw,
        &quad(0.5),
    )
    .expect("draw");
    gpu.end_frame().expect("end");
    let rgba = gpu.capture().expect("capture").to_rgba();
    for (x, y) in [(0, 0), (63, 63), (20, 10), (63, 0)] {
        assert_eq!(pixel(&rgba, x, y), [0, 0, 255], "pixel ({x}, {y})");
    }
}
