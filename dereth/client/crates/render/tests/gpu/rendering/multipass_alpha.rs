//! A clip-mapped surface drawn alpha-tested and then again through surface setup's force-alpha
//! state: the second pass blends exactly the texels the alpha test cut away and leaves the ones it
//! kept untouched, so the edge of a cut-out goes from a hard step to a ramp.
//! Fixture: an offscreen device, a horizontal alpha ramp over an opaque backdrop, pixel readback.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_primitives::{TextureData, TextureFormat};
use dereth_render::device::{Gpu, PerDrawConstants, PerFrameConstants};
use dereth_render::pso::{PipelineKey, SurfaceContext, SurfaceState, ZFunc};
use dereth_render::surface::{surface_type, Surface, SurfaceHandler};
use dereth_render::{DrawConstants, VertexFormat};

const N: u32 = 64;
/// The ramp's colour. Not white, so a second blend over a kept texel would show in blue.
const RAMP: (u8, u8, u8) = (200, 100, 0);
/// The backdrop's colour.
const BACK: (u8, u8, u8) = (0, 0, 255);

/// Column `x`'s alpha: `4x + 2`, so no column sits on the reference of 200.
fn ramp_alpha(x: u32) -> u8 {
    u8::try_from(x * 4 + 2).expect("64 columns fit")
}

fn ramp() -> TextureData {
    let mut px = Vec::new();
    for _ in 0..4 {
        for x in 0..N {
            px.extend_from_slice(&[RAMP.2, RAMP.1, RAMP.0, ramp_alpha(x)]);
        }
    }
    TextureData {
        width: N,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![px],
    }
}

fn backdrop() -> TextureData {
    let mut px = Vec::new();
    for _ in 0..16 {
        px.extend_from_slice(&[BACK.2, BACK.1, BACK.0, 0xFF]);
    }
    TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![px],
    }
}

fn state(t: u32, force_alpha: bool) -> SurfaceState {
    PipelineKey::state_from_surface(
        &Surface {
            r#type: t,
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
            force_alpha,
            ..SurfaceContext::default()
        },
    )
}

/// Two triangles over the whole of clip space at depth `z`, white diffuse, uv (0,0)..(1,1).
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

/// The backdrop, the cut-out, and with `second_pass` the forced-alpha pass of the same cut-out;
/// returns one RGBA row from the middle of the screen.
fn render(gpu: &mut Gpu, second_pass: bool) -> Vec<[u8; 3]> {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let per_frame = PerFrameConstants {
        view_proj: identity,
        view: identity,
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..PerFrameConstants::default()
    };
    let draw = PerDrawConstants::identity();
    let back = gpu.upload_texture(&backdrop()).expect("upload");
    let tex = gpu.upload_texture(&ramp()).expect("upload");
    let opaque = state(surface_type::BASE1_IMAGE, false);
    let clip = surface_type::BASE1_IMAGE | surface_type::BASE1_CLIPMAP;
    let first = state(clip, false);
    let second = state(clip, true);

    gpu.begin_frame().expect("begin");
    gpu.bind_texture(back, 1);
    gpu.draw_dynamic(
        &opaque.key,
        &DrawConstants::default(),
        &per_frame,
        &draw,
        &quad(0.9),
    )
    .expect("draw");
    // Sampler 1 is linear / clamp: the ramp is exactly as wide as the screen, so each pixel
    // centre samples one texel's alpha and the edge columns do not wrap.
    gpu.bind_texture(tex, 1);
    gpu.draw_dynamic(
        &first.key,
        &DrawConstants {
            alpha_ref: first.alpha_ref,
            ..DrawConstants::default()
        },
        &per_frame,
        &draw,
        &quad(0.5),
    )
    .expect("draw");
    if second_pass {
        gpu.draw_dynamic(
            &second.key,
            &DrawConstants {
                alpha_ref: second.alpha_ref,
                ..DrawConstants::default()
            },
            &per_frame,
            &draw,
            &quad(0.5),
        )
        .expect("draw");
    }
    gpu.end_frame().expect("end");
    let rgba = gpu.capture().expect("capture").to_rgba();
    let row = (N / 2) as usize;
    (0..N as usize)
        .map(|x| {
            let p = &rgba[(row * N as usize + x) * 4..][..3];
            [p[0], p[1], p[2]]
        })
        .collect()
}

/// Behaviour: rendering.surfaces.a-forced-alpha-pass-blends-only-what-the-alpha-test-cut
#[test]
#[ignore = "requires an offscreen rendering device; run this GPU check explicitly"]
fn the_forced_alpha_pass_blends_only_the_texels_the_alpha_test_cut_away() {
    let mut gpu = crate::common::text_pixels::warp(N, N);
    let clip = surface_type::BASE1_IMAGE | surface_type::BASE1_CLIPMAP;
    let (first, second) = (state(clip, false), state(clip, true));
    assert_eq!(
        first.alpha_ref, 200,
        "a non-palettised clip map tests at 200"
    );
    assert!(first.key.alpha_test && first.key.z_write);
    assert!(second.key.alpha_blend && !second.key.alpha_test && !second.key.z_write);
    assert_eq!(second.key.z_func, ZFunc::Less);

    let one = render(&mut gpu, false);
    let two = render(&mut gpu, true);
    let mut ramped = 0;
    for x in 0..N {
        let a = f64::from(ramp_alpha(x)) / 255.0;
        let (p1, p2) = (one[x as usize], two[x as usize]);
        if ramp_alpha(x) < 200 {
            // Cut away by the test: the backdrop, untouched, after one pass.
            assert_eq!(
                p1,
                [BACK.0, BACK.1, BACK.2],
                "column {x} (alpha {}): one pass must leave the backdrop",
                ramp_alpha(x)
            );
            // The second pass blends the ramp over the backdrop at the texel's alpha.
            let want = [
                f64::from(RAMP.0) * a,
                f64::from(RAMP.1) * a,
                f64::from(BACK.2) * (1.0 - a),
            ];
            for c in 0..3 {
                assert!(
                    (f64::from(p2[c]) - want[c]).abs() <= 3.0,
                    "column {x} (alpha {}): channel {c} is {} after the second pass, want {:.1}",
                    ramp_alpha(x),
                    p2[c],
                    want[c]
                );
            }
            if p2 != p1 {
                ramped += 1;
            }
        } else {
            // Kept by the test, and the first pass wrote its depth: the second pass fails `LESS`
            // there and changes nothing.
            assert_eq!(
                p2,
                p1,
                "column {x} (alpha {}): the second pass touched a texel the first one kept",
                ramp_alpha(x)
            );
            assert_ne!(
                p1,
                [BACK.0, BACK.1, BACK.2],
                "column {x}: the cut-out is drawn"
            );
        }
    }
    eprintln!("multiple pass alpha: {ramped} of 50 cut-away columns blended by the second pass");
    assert_eq!(
        ramped, 50,
        "every cut-away column with non-zero alpha is blended"
    );
}
