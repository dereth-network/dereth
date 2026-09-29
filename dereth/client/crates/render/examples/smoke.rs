//! The renderer smoke example: clear, draw a textured quad, present.
//!
//! The captured image must be byte-identical across three runs.
//!
//! ```text
//! cargo run -p dereth-render --example smoke -- --headless --out smoke.ppm
//! ```
//!
//! The output is a binary PPM rather than a PNG: writing PNG would mean adding an encoder
//! dependency to this crate, and the requirement is byte-identical output across runs, which a
//! PPM satisfies exactly as well. A golden-image harness reads whichever it prefers from [`dereth_render::device::CapturedImage`], which is the real interface.

fn main() {
    if let Err(e) = run() {
        eprintln!("smoke: {e}");
        std::process::exit(1);
    }
}

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use dereth_primitives::{TextureData, TextureFormat};
    use dereth_render::device::{DeviceConfig, Gpu, PerDrawConstants, PerFrameConstants};
    use dereth_render::pso::{PipelineKey, SurfaceContext};
    use dereth_render::surface::{surface_type as st, Surface};
    use dereth_render::ui::{self, UiSurface};
    use dereth_render::vertex::VertexFormat;
    use dereth_render::{DrawConstants, ViewParams};

    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .cloned();
    // The pre-game screens run at a forced 800x600.
    let cfg = DeviceConfig {
        width: 800,
        height: 600,
        force_software: headless,
        ..DeviceConfig::default()
    };
    if !headless {
        eprintln!("smoke: only --headless is implemented; a window needs the input track's pump");
        return Ok(());
    }
    let mut gpu = Gpu::new(None, &cfg)?;
    eprintln!("smoke: device on {:?}", gpu.adapter_kind());

    // A 4x4 chequer, so a wrong UV or a wrong byte order is visible rather than plausible.
    let mut bgra = vec![0u8; 4 * 4 * 4];
    for y in 0..4usize {
        for x in 0..4usize {
            let i = (y * 4 + x) * 4;
            let on = (x + y) % 2 == 0;
            // BGRA: red squares on blue.
            bgra[i] = if on { 0 } else { 0xFF };
            bgra[i + 1] = 0;
            bgra[i + 2] = if on { 0xFF } else { 0 };
            bgra[i + 3] = 0xFF;
        }
    }
    let slot = gpu.upload_texture(&TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![bgra],
    })?;

    // The quad, placed by the four UI pixel rules so the example exercises them.
    let surface = UiSurface::create(4, 4, true)?;
    // The client's own clip coordinates, then the D3D12 pixel-centre compensation. Keeping the two
    // steps separate is the point: the rules are the contract, the compensation is this API.
    let rect = ui::update_transform(100, 80, 400, 300, (800, 600), (800, 600));
    let rect = ui::compensate_for_d3d12(rect, (800, 600));
    let (fu, fv) = surface.uv();
    // FVF 0x142: float3 position, D3DCOLOR diffuse, float2 uv -- 24 bytes.
    let mut vertices: Vec<u8> = Vec::new();
    let push = |x: f32, y: f32, u: f32, v: f32, vertices: &mut Vec<u8>| {
        vertices.extend_from_slice(&x.to_le_bytes());
        vertices.extend_from_slice(&y.to_le_bytes());
        vertices.extend_from_slice(&0.5f32.to_le_bytes());
        vertices.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        vertices.extend_from_slice(&u.to_le_bytes());
        vertices.extend_from_slice(&v.to_le_bytes());
    };
    let (l, r, b, t) = (rect.x, rect.right(), rect.y, rect.top());
    // Two triangles in the shared index buffer's winding order, expanded (the dynamic path is
    // unindexed, exactly as the client's dynamic-primitive draw is).
    for (x, y, u, v) in [
        (l, t, 0.0, 0.0),
        (l, b, 0.0, fv),
        (r, b, fu, fv),
        (r, b, fu, fv),
        (r, t, fu, 0.0),
        (l, t, 0.0, 0.0),
    ] {
        push(x, y, u, v, &mut vertices);
    }

    // An opaque textured surface: row 1 of the catalogue.
    let surf = Surface {
        r#type: st::BASE1_IMAGE,
        ..Surface::default()
    };
    let ctx = SurfaceContext {
        vertex_format: VertexFormat::XyzDiffuseTex1,
        ..SurfaceContext::default()
    };
    let (mut key, alpha_ref) = PipelineKey::from_surface(&surf, ctx);
    // The UI quad is drawn with identity matrices and ZFUNC ALWAYS; the world pass state would
    // depth-test the pre-transformed positions against a cleared buffer.
    key.z_func = dereth_render::ZFunc::Always;
    key.z_write = false;

    let view = ViewParams::default();
    let per_frame = PerFrameConstants::from_view(&view);
    // Identity world/view/projection: the vertices above are already in clip space.
    let mut per_frame = per_frame;
    per_frame.view_proj = glam::Mat4::IDENTITY.to_cols_array();
    per_frame.view = glam::Mat4::IDENTITY.to_cols_array();
    let per_draw = PerDrawConstants::identity();

    gpu.begin_frame()?;
    // Sampler 3 = point / clamp, which is the fourth UI pixel rule.
    gpu.bind_texture(slot, 3);
    gpu.draw_dynamic(
        &key,
        &DrawConstants {
            alpha_ref,
            ..DrawConstants::default()
        },
        &per_frame,
        &per_draw,
        &vertices,
    )?;
    gpu.end_frame()?;

    let image = gpu.capture()?;
    let rgba = image.to_rgba();
    let lit = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] != 0 || p[1] != 0 || p[2] != 0)
        .count();
    eprintln!(
        "smoke: {}x{}, {lit} non-black pixels",
        image.width, image.height
    );
    // The four pixel rules, measured: a 400x300 element covers exactly 400*300 pixels.
    if lit != 400 * 300 {
        return Err(format!("the quad covered {lit} pixels, not {}", 400 * 300).into());
    }

    if let Some(path) = out {
        let mut ppm = format!("P6\n{} {}\n255\n", image.width, image.height).into_bytes();
        for px in rgba.as_chunks::<4>().0 {
            ppm.extend_from_slice(&px[..3]);
        }
        std::fs::write(&path, ppm)?;
        eprintln!("smoke: wrote {path}");
    }
    Ok(())
}

#[cfg(not(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12"))))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    Err("the smoke example needs a graphics backend: --features vulkan or d3d12".into())
}
