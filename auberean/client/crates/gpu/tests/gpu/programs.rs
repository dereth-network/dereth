//! How the device draws the normal-mapped pixel programs that read their stages and colors other
//! than the common one does: a detail map for the normal map in stage 1 and the diffuse map in
//! stage 2, an anisotropic map doubling the light again, the diffuse map's alpha masking the
//! layer's color, and a specular toward the direction color 1 carries. Fixture: a made-up flat wall
//! facing the camera, lit from behind the camera by a white sun, its maps single colors.

use std::sync::Arc;

use auberean_draw::material::{d3d, switch, DrawLayer, PixelProgram, Stage, UV_IDENTITY};
use auberean_draw::mesh::MeshVertex;
use auberean_draw::picture::Image;
use auberean_draw::scene::{Camera, Frame, SceneBuilder};
use auberean_gpu::Renderer;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::sky::DayLight;

use crate::common::{checked, install, renderer};
use crate::fade::wall;

const W: u32 = 32;
const H: u32 = 32;
/// The pictures' ids: the normal map, then the stages' other pictures.
const NORMAL: u32 = 0x4100_0001;
const FIRST: u32 = 0x4100_0002;
const SECOND: u32 = 0x4100_0003;
/// The layer's option that keeps color 1 to the other lights alone.
const UNMODULATED: u32 = 0x2000;
const BUMP: u32 = switch::DYNAMIC_LIGHTING | switch::PIXEL_LIGHTING | switch::MATERIAL_DIFFUSE;

/// One picture of a single color, `[b, g, r, a]`.
fn flat(bgra: [u8; 4]) -> Image {
    Image {
        width: 4,
        height: 4,
        bgra: bgra.repeat(16),
        smaller: Vec::new(),
    }
}

fn stage(texture: u32, tex_coord: u32) -> Stage {
    Stage {
        texture,
        color_op: d3d::OP_SELECT_ARG1,
        color_arg1: d3d::ARG_TEXTURE,
        color_arg2: d3d::ARG_CURRENT,
        alpha_op: d3d::OP_SELECT_ARG1,
        alpha_arg1: d3d::ARG_TEXTURE,
        alpha_arg2: d3d::ARG_CURRENT,
        clamp_u: false,
        clamp_v: false,
        tex_coord,
        uv_transform: UV_IDENTITY,
    }
}

/// The color at the middle of a wall 10 m off, facing the camera, drawn with `program` and
/// `switches`, its layer's diffuse `diffuse` and specular `specular`, its third stage reading
/// coordinates `third`; the sun at full brightness behind the camera, the ambient 0.2.
fn drawn(
    program: &str,
    switches: u32,
    diffuse: Color,
    specular: Color,
    third: u32,
    pictures: [Image; 3],
) -> [u8; 3] {
    let mut b = wall(10.0, 1.0, false, 10.0);
    let vertex = |x: f32, z: f32| MeshVertex {
        position: [x, 10.0, z],
        normal: [0.0, -1.0, 0.0],
        diffuse: 0xFFFF_FFFF,
        uv: [[0.5; 2]; 4],
        tangent_u: [1.0, 0.0, 0.0],
        tangent_v: [0.0, 0.0, 1.0],
        to_light: [0.0; 3],
    };
    b.vertices = Arc::new(vec![
        vertex(-100.0, -100.0),
        vertex(100.0, -100.0),
        vertex(-100.0, 100.0),
        vertex(100.0, 100.0),
    ]);
    b.layer = DrawLayer {
        stages: vec![stage(NORMAL, 0), stage(FIRST, 0), stage(SECOND, third)],
        switches,
        ambient: Color::WHITE,
        diffuse,
        specular,
        specular_power: 10.0,
        pixel_program: Some(PixelProgram::from_text(program.as_bytes())),
        ..b.layer
    };
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let camera = Camera {
        position: Vec3::ZERO,
        heading: 0.0,
        pitch: 0.0,
        fov_y: 60.0,
        near: 0.1,
        far: 500.0,
        roll: 0.0,
        orthographic: None,
    };
    let light = DayLight {
        ambient_brightness: 0.2,
        direct_brightness: 1.0,
        light_direction: Vec3::new(0.0, 1.0, 0.0),
        ..DayLight::DEFAULT
    };
    let mut frame: Frame = builder.parts_frame(&[], Vec::new(), camera, light, None);
    frame.objects = vec![b];
    let mut device = renderer();
    let [normal, first, second] = pictures;
    let mut load = |id: u32| match id {
        NORMAL => Some(normal.clone()),
        FIRST => Some(first.clone()),
        SECOND => Some(second.clone()),
        _ => None,
    };
    let device: &mut Renderer = &mut device;
    checked(device, |d| {
        let p = d.render(&frame, W, H, &mut load).expect("read back");
        let at = ((H / 2 * W + W / 2) * 4) as usize;
        [p.rgba[at], p.rgba[at + 1], p.rgba[at + 2]]
    })
}

/// A normal map facing straight out of the surface.
fn facing_out() -> Image {
    flat([255, 128, 128, 255])
}

fn near(got: u8, want: f32) -> bool {
    (f32::from(got) - want).abs() < 8.0
}

/// Where a program masks the layer's color by the diffuse map's alpha, a surface whose diffuse
/// map is clear shows the light white, however red its layer, and one whose map is full shows it
/// tinted; the ambient is added after the tint either way. Here a quarter-gray map: lit white,
/// `(1 + 0.2) × 2 × 0.25 = 0.6` in every channel; tinted, red keeps 0.6 and green takes
/// `(0.2 + 0.2) × 0.5 = 0.2`.
///
/// Behaviour: draw.material.a-tint-mask-shows-the-layers-color-only-where-its-alpha-is-full
#[test]
fn a_tint_mask_shows_the_layers_color_only_where_its_alpha_is_full() {
    let program = "tex t0\ntex t1\ntex t2\nadd t1.rgb, t1, t2_bias\nlrp t2.rgb, t1.a, c4, c6\n\
                   dp3_sat r0.rgb, t0_bx2, v0_bx2\nmad r0.rgb, r0, c3, v1\nmad r0.rgb, r0, t2, c0\n\
                   mul_x2 r0.rgb, r0, t1\n";
    let red = Color::new(1.0, 0.2, 0.2, 1.0);
    let switches = BUMP | switch::PIXEL_MULTIPLE_LIGHTS | UNMODULATED;
    let with = |alpha: u8| {
        drawn(
            program,
            switches,
            red,
            Color::BLACK,
            0,
            [
                facing_out(),
                flat([64, 64, 64, alpha]),
                flat([128, 128, 128, 255]),
            ],
        )
    };
    let clear = with(0);
    let full = with(255);
    println!("clear {clear:?}, full {full:?}");
    assert!(
        clear.iter().all(|&c| near(c, 0.6 * 255.0)),
        "clear {clear:?}"
    );
    assert!(
        near(full[0], 0.6 * 255.0) && near(full[1], 0.2 * 255.0),
        "full {full:?}"
    );
}

/// Where a program adds a detail map in stage 1 to the normal map, the diffuse map is stage 2's:
/// a surface with a gray detail map and a green diffuse map shows green, the light
/// `(1 + 0.2) × 2` times the map's half green.
///
/// Behaviour: draw.material.a-normal-detail-program-draws-its-third-stage-as-the-diffuse-map
#[test]
fn a_normal_detail_program_draws_its_third_stage_as_the_diffuse_map() {
    let program =
        "tex t0\ntex t1\ntex t2\nadd t0.rgb, t0, t1_bias\ndp3_sat r0.rgb, t0_bx2, v0_bx2\n\
                   mad r0.rgb, r0, c5, v1\nmul_x2 r0.rgb, r0, t2\n";
    let got = drawn(
        program,
        BUMP | switch::PIXEL_MULTIPLE_LIGHTS,
        Color::WHITE,
        Color::BLACK,
        0,
        [
            facing_out(),
            flat([128, 128, 128, 255]),
            flat([0, 128, 0, 255]),
        ],
    );
    println!("{got:?}");
    assert!(got[0] < 8 && got[2] < 8 && got[1] > 240, "{got:?}");
}

/// Where a program doubles the light against an anisotropic map before the diffuse map, a
/// quarter-gray anisotropic map halves the surface: `(1 + 0.2) × 2 × 0.25 × 2 × 0.5 = 0.6`.
///
/// Behaviour: draw.material.an-anisotropic-program-doubles-the-light-by-its-map
#[test]
fn an_anisotropic_program_doubles_the_light_by_its_map() {
    let program =
        "tex t0\ntex t1\ntex t2\ndp3_sat r0.rgb, t0_bx2, v0_bx2\nmad r0.rgb, r0, c5, v1\n\
                   mul_x2 r0.rgb, r0, t2\nmul_x2 r0.rgb, r0, t1\n";
    let got = drawn(
        program,
        BUMP | switch::PIXEL_MULTIPLE_LIGHTS,
        Color::WHITE,
        Color::BLACK,
        0x200,
        [
            facing_out(),
            flat([128, 128, 128, 255]),
            flat([64, 64, 64, 255]),
        ],
    );
    println!("{got:?}");
    assert!(got.iter().all(|&c| near(c, 0.6 * 255.0)), "{got:?}");
}

/// A layer lit by one light with its specular worked out per pixel carries a black color 1, so a
/// program reading its specular toward color 1's direction finds it facing away from the normal
/// map: the surface shows no highlight, only its light, `(1 + 0.2) × 2 × 0.25 = 0.6`, though its
/// specular color is white and the sun shines straight back off it.
///
/// Behaviour: draw.material.a-specular-toward-color-ones-direction-shows-no-highlight
#[test]
fn a_specular_toward_color_ones_direction_shows_no_highlight() {
    let program = "def c6, 0.75, 0.75, 0.75, 0.75\ntex t0\ntex t1\n\
                   dp3_sat r0.rgb, t0_bx2, v0_bx2\ndp3_sat r1.rgb, t0_bx2, v1_bx2\n\
                   mad_x4_sat r1.rgb, r1, r1, -c6\nmad r0.rgb, r0, c1, c0\nmul_x2 r0.rgb, r0, t1\n\
                   mul r1.rgb, r1, c2\nmad r0.rgb, r1, t1.a, r0\n";
    let got = drawn(
        program,
        BUMP | switch::PIXEL_SPECULAR,
        Color::WHITE,
        Color::WHITE,
        0,
        [
            facing_out(),
            flat([64, 64, 64, 255]),
            flat([128, 128, 128, 255]),
        ],
    );
    println!("{got:?}");
    assert!(got.iter().all(|&c| near(c, 0.6 * 255.0)), "{got:?}");
}
