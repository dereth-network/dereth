//! A part whose mesh is stretched unevenly is lit along its true surface normal, the inverse
//! transpose of its scale applied to the model normal, whether it is drawn as a moving object
//! (the scale in its world matrix) or baked into the landscape (the scale folded into its
//! vertices). A uniformly scaled part is lit exactly as before.
//! Fixture: a full-screen lit quad on a device, one directional light, pixel readback.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::{baked_normal, normal_scale, world_constants, world_constants_scaled};
use dereth_primitives::{Frame, Quat, TextureData, TextureFormat, Vec3};
use dereth_render::device::{Gpu, PerDrawConstants, PerFrameConstants};
use dereth_render::pso::{Blend, Cull, PipelineKey, ZFunc};
use dereth_render::{DrawConstants, StageOps, VertexFormat};

const N: u32 = 64;

/// The part's scale: a quarter along its own x, full size along y and z.
const STRETCH: Vec3 = Vec3::new(0.25, 1.0, 1.0);

fn key() -> PipelineKey {
    PipelineKey {
        vertex_format: VertexFormat::XyzNormalDiffuseTex1,
        src_blend: Blend::One,
        dst_blend: Blend::Zero,
        alpha_blend: false,
        alpha_test: false,
        z_write: true,
        z_func: ZFunc::Less,
        cull: Cull::None,
        stage_ops: StageOps::BASE,
        fog: false,
        lighting: true,
    }
}

fn white() -> TextureData {
    TextureData {
        width: 4,
        height: 4,
        format: TextureFormat::Bgra8,
        levels: vec![[0xFF; 64].to_vec()],
    }
}

/// Two triangles that fill clip space once the world matrix has applied `scale` and the client's
/// y/z swap: the quad lies in the client's x/z plane at y = 0.5 (the depth), every vertex
/// carrying `normal`.
fn quad(scale: Vec3, normal: Vec3) -> Vec<u8> {
    let mut v = Vec::new();
    for (x, z) in [
        (-1.0f32, -1.0f32),
        (1.0, -1.0),
        (1.0, 1.0),
        (-1.0, -1.0),
        (1.0, 1.0),
        (-1.0, 1.0),
    ] {
        for f in [x / scale.x, 0.5 / scale.y, z / scale.z] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        for f in [normal.x, normal.y, normal.z] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        v.extend_from_slice(&0.5f32.to_le_bytes());
        v.extend_from_slice(&0.5f32.to_le_bytes());
    }
    v
}

/// Draw the quad lit by one white directional light shining along the client's -x, so the
/// lit value is the normal's x component; return the centre pixel's red.
fn lit(gpu: &mut Gpu, mut draw: PerDrawConstants, vertices: &[u8]) -> u8 {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let per_frame = PerFrameConstants {
        view_proj: identity,
        view: identity,
        fog_params: [0.0, 1.0, 0.0, 0.0],
        ..PerFrameConstants::default()
    };
    draw.lighting_params = [1.0, 1.0, 0.0, 0.0];
    // A directional light's position slot holds the direction it travels, in device order
    // (client x stays x).
    draw.light_pos[0] = [-1.0, 0.0, 0.0, 0.0];
    draw.light_diffuse[0] = [1.0, 1.0, 1.0, 3.0];
    let slot = gpu.upload_texture(&white()).expect("upload");
    gpu.begin_frame().expect("begin");
    gpu.bind_texture(slot, 0);
    gpu.draw_dynamic(
        &key(),
        &DrawConstants::default(),
        &per_frame,
        &draw,
        vertices,
    )
    .expect("draw");
    gpu.end_frame().expect("end");
    let rgba = gpu.capture().expect("capture").to_rgba();
    rgba[((N / 2) * N + N / 2) as usize * 4]
}

fn unit(v: Vec3) -> Vec3 {
    let l = v.magnitude();
    Vec3::new(v.x / l, v.y / l, v.z / l)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn expected(nx: f32) -> u8 {
    // LINT-OK: a lit value in 0..=1 to a byte.
    (nx.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Behaviour: rendering.lighting.a-stretched-part-is-lit-along-its-true-surface
#[test]
fn an_unevenly_scaled_part_is_lit_along_its_inverse_transpose_normal_moving_or_baked() {
    let _lock = crate::common::gpu_lock();
    let mut gpu = crate::common::software_gpu(N, N);
    let frame = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    // The model normal, half way between x and y. Squashing x to a quarter tilts the true
    // surface normal toward x: n / s = (4, 1, 0) / |..|, whose x is 0.970; the plain world
    // matrix would tilt it the other way, to x = 0.243.
    let n = unit(Vec3::new(1.0, 1.0, 0.0));
    let truth = unit(Vec3::new(n.x / STRETCH.x, n.y / STRETCH.y, n.z / STRETCH.z));
    assert_eq!(baked_normal(n, STRETCH), truth);

    // Moving: the mesh as it is, the scale in the world matrix.
    let moving = lit(
        &mut gpu,
        world_constants_scaled(&frame, STRETCH),
        &quad(STRETCH, n),
    );
    // Baked: the scale already in the vertices and the normal, the world matrix unscaled.
    let baked = lit(
        &mut gpu,
        world_constants(&frame),
        &quad(Vec3::new(1.0, 1.0, 1.0), baked_normal(n, STRETCH)),
    );
    let want = expected(truth.x);
    eprintln!("stretched part: moving {moving}, baked {baked}, the true normal gives {want}");
    assert!(
        moving.abs_diff(want) <= 1 && baked.abs_diff(want) <= 1,
        "moving {moving} and baked {baked} must both be lit as the true normal gives, {want}"
    );

    // A uniform scale takes no inverse-transpose step at all.
    assert_eq!(normal_scale(Vec3::new(2.0, 2.0, 2.0)), [0.0; 4]);
    let u = Vec3::new(2.0, 2.0, 2.0);
    let uniform = lit(&mut gpu, world_constants_scaled(&frame, u), &quad(u, n));
    assert!(
        uniform.abs_diff(expected(n.x)) <= 1,
        "a uniformly scaled part is lit by its own normal: {uniform}"
    );
}
