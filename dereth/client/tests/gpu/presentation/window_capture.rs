//! A picture taken of a window (the screenshot key, `--capture-at`) is the frame just presented,
//! on every device that can read a window back: not the next back buffer, which holds an older
//! frame or nothing yet.
//!
//! Fixture: the process's invisible native window and a device on it, one at a time (a window
//! holds one swap chain). Each frame fills the window with a flat color of its own, and the
//! picture taken after it must be that color. The `wgpu` device draws a window through a canvas,
//! which cannot be read back, so it is not one of them.

#![cfg(gpu)]

use dereth_primitives::TextureData;
use dereth_primitives::TextureFormat;
use dereth_render::device::{Backend, DeviceConfig, Gpu, PerDrawConstants, PerFrameConstants};
use dereth_render::pso::{PipelineKey, SurfaceContext, SurfaceState};
use dereth_render::surface::{surface_type, Surface, SurfaceHandler};
use dereth_render::{DrawConstants, VertexFormat};

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

/// Two triangles over the whole of clip space, white diffuse.
fn quad() -> Vec<u8> {
    let mut v = Vec::new();
    for (x, y, u, t) in [
        (-1.0f32, -1.0f32, 0.0f32, 1.0f32),
        (1.0, -1.0, 1.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
        (-1.0, -1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
        (-1.0, 1.0, 0.0, 0.0),
    ] {
        for f in [x, y, 0.5] {
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

/// It opens an invisible window: on Linux on the display the run inherits, so a machine with none
/// runs it under Xvfb.
///
/// Behaviour: presentation.screenshot.a-window-picture-is-the-frame-just-presented
#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS makes an event loop only on the process's main thread, which no test runs on"
)]
fn a_window_picture_is_the_frame_just_presented_on_every_device_that_reads_one_back() {
    let handles = crate::common::invisible_window();
    let mut backends = vec![Backend::Vulkan];
    if cfg!(windows) {
        backends.push(Backend::D3d12);
    }
    for backend in backends {
        let cfg = DeviceConfig {
            width: 800,
            height: 600,
            use_stencil_buffer: true,
            ..DeviceConfig::default()
        };
        let mut gpu = match Gpu::new_on(backend, Some(handles), &cfg) {
            Ok(gpu) => gpu,
            Err(dereth_render::RenderError::Unsupported(why)) => {
                eprintln!("{backend:?}: {why}");
                continue;
            }
            Err(e) => panic!("{backend:?}: a device on the window: {e}"),
        };
        let (per_frame, draw) = constants();
        let key = opaque().key;
        let colors = [
            (255, 0, 0),
            (0, 0, 255),
            (0, 255, 0),
            (255, 255, 255),
            (255, 0, 0),
        ];
        let slots: Vec<_> = colors
            .iter()
            .map(|c| gpu.upload_texture(&flat(*c)).expect("upload"))
            .collect();
        for (frame, (color, slot)) in colors.iter().zip(&slots).enumerate() {
            gpu.begin_frame().expect("begin");
            gpu.bind_texture(*slot, 1);
            gpu.draw_dynamic(&key, &DrawConstants::default(), &per_frame, &draw, &quad())
                .expect("draw");
            gpu.end_frame().expect("present");
            let rgba = gpu.capture().expect("capture").to_rgba();
            let at = |x: usize, y: usize| {
                let p = &rgba[(y * 800 + x) * 4..][..3];
                (p[0], p[1], p[2])
            };
            for (x, y) in [(0, 0), (400, 300), (799, 599)] {
                assert_eq!(
                    at(x, y),
                    *color,
                    "{backend:?}: frame {}, pixel ({x}, {y})",
                    frame + 1
                );
            }
        }
    }
}
