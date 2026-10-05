//! A constant BGRA8 texel and DXT1 block drawn full-screen through the world FVFs (unlit and lit-
//! saturated) read back exactly.
//! Fixture: offscreen device rendering and pixel readback.

#![cfg(gpu)]

use dereth_primitives::{TextureData, TextureFormat};
use dereth_render::device::{Gpu, PerDrawConstants, PerFrameConstants};
use dereth_render::pso::{Blend, Cull, PipelineKey, ZFunc};
use dereth_render::{DrawConstants, StageOps, VertexFormat};

const N: u32 = 64;
const TEXEL: (u8, u8, u8) = (99, 84, 59);

fn warp() -> Gpu {
    crate::common::text_pixels::warp(N, N)
}

fn bgra8_constant() -> TextureData {
    let (r, g, b) = TEXEL;
    let mut px = Vec::new();
    for _ in 0..16 {
        px.extend_from_slice(&[b, g, r, 0xFF]);
    }
    TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![px],
    }
}

fn pack565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
}

/// One DXT1 block whose both endpoints are the texel and whose indices all select endpoint 0.
fn dxt1_constant() -> TextureData {
    let (r, g, b) = TEXEL;
    let c = pack565(r, g, b).to_le_bytes();
    let block = vec![c[0], c[1], c[0], c[1], 0, 0, 0, 0];
    TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bc1,
        levels: vec![block],
    }
}

/// What the hardware decoder returns for a 565 endpoint: bit replication.
fn expanded_565() -> (u8, u8, u8) {
    let (r, g, b) = TEXEL;
    let e5 = |v: u8| (v << 3) | (v >> 2);
    let e6 = |v: u8| (v << 2) | (v >> 4);
    (e5(r >> 3), e6(g >> 2), e5(b >> 3))
}

fn key(format: VertexFormat, lighting: bool) -> PipelineKey {
    PipelineKey {
        vertex_format: format,
        src_blend: Blend::One,
        dst_blend: Blend::Zero,
        alpha_blend: false,
        alpha_test: false,
        z_write: true,
        z_func: ZFunc::Less,
        cull: Cull::None,
        stage_ops: StageOps::BASE,
        fog: false,
        lighting,
    }
}

/// Two triangles over the whole of clip space, white diffuse, uv (0,0)..(1,1), z 0.5.
fn quad(format: VertexFormat) -> Vec<u8> {
    let mut v = Vec::new();
    let corners = [
        (-1.0f32, -1.0f32, 0.0f32, 1.0f32),
        (1.0, -1.0, 1.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
        (-1.0, -1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0, 0.0),
        (-1.0, 1.0, 0.0, 0.0),
    ];
    for (x, y, u, t) in corners {
        for f in [x, y, 0.5f32] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        if format == VertexFormat::XyzNormalDiffuseTex1 {
            // A normal facing the viewer, for the lit permutation.
            for f in [0.0f32, 0.0, -1.0] {
                v.extend_from_slice(&f.to_le_bytes());
            }
        }
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        v.extend_from_slice(&u.to_le_bytes());
        v.extend_from_slice(&t.to_le_bytes());
    }
    v
}

fn render(gpu: &mut Gpu, tex: &TextureData, format: VertexFormat, lit: bool) -> Vec<u8> {
    let slot = gpu.upload_texture(tex).expect("upload");
    let mut per_frame = PerFrameConstants {
        view_proj: [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
        view: [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ],
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..PerFrameConstants::default()
    };
    let mut draw = PerDrawConstants::identity();
    if lit {
        per_frame.ambient = [1.0, 1.0, 1.0, 1.0];
        draw.lighting_params = [1.0, 0.0, 0.0, 0.0];
    }
    gpu.begin_frame().expect("begin");
    gpu.bind_texture(slot, 0);
    gpu.draw_dynamic(
        &key(format, lit),
        &DrawConstants::default(),
        &per_frame,
        &draw,
        &quad(format),
    )
    .expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

fn check(name: &str, rgba: &[u8], want: (u8, u8, u8)) {
    let n = (rgba.len() / 4) as f64;
    let mean: Vec<f64> = (0..3)
        .map(|c| {
            rgba.as_chunks::<4>()
                .0
                .iter()
                .map(|p| f64::from(p[c]))
                .sum::<f64>()
                / n
        })
        .collect();
    let centre = &rgba[((N / 2) * N + N / 2) as usize * 4..][..3];
    eprintln!(
        "{name}: want {want:?} centre ({}, {}, {}) mean ({:.2}, {:.2}, {:.2})",
        centre[0], centre[1], centre[2], mean[0], mean[1], mean[2]
    );
    let off = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| {
            p[0].abs_diff(want.0) > 1 || p[1].abs_diff(want.1) > 1 || p[2].abs_diff(want.2) > 1
        })
        .count();
    assert_eq!(
        off,
        0,
        "{name}: {off} of {} pixels are not the texel (centre {centre:?}, want {want:?})",
        rgba.len() / 4
    );
}

/// Behaviour: rendering.texture.a-constant-texel-reads-back-unchanged-through-every-world-pipeline
#[test]
#[ignore = "requires an offscreen software rendering device; run this GPU check explicitly"]
fn a_constant_texel_reads_back_unchanged_through_every_world_permutation() {
    let mut gpu = warp();
    let bgra = bgra8_constant();
    let dxt = dxt1_constant();
    check(
        "bgra8 0x142 unlit",
        &render(&mut gpu, &bgra, VertexFormat::XyzDiffuseTex1, false),
        TEXEL,
    );
    check(
        "bgra8 0x152 unlit",
        &render(&mut gpu, &bgra, VertexFormat::XyzNormalDiffuseTex1, false),
        TEXEL,
    );
    check(
        "bgra8 0x152 lit-saturated",
        &render(&mut gpu, &bgra, VertexFormat::XyzNormalDiffuseTex1, true),
        TEXEL,
    );
    let e = expanded_565();
    check(
        "dxt1 0x142 unlit",
        &render(&mut gpu, &dxt, VertexFormat::XyzDiffuseTex1, false),
        e,
    );
    check(
        "dxt1 0x152 unlit",
        &render(&mut gpu, &dxt, VertexFormat::XyzNormalDiffuseTex1, false),
        e,
    );
    check(
        "dxt1 0x152 lit-saturated",
        &render(&mut gpu, &dxt, VertexFormat::XyzNormalDiffuseTex1, true),
        e,
    );
}
