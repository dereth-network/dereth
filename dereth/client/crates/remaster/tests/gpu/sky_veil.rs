//! A world's sky tints lay their fog over the remaster's physical sky: with the whole of the veil
//! the sky drawn before the land is the fog's color, even and gray as the fog is; with none of it,
//! as on every world without tints, the clear day's air is blue; and half the veil lies between.
//!
//! Fixture: an empty landscape lit per pixel on a clear day at noon, the eye looking up into the sky, the fog
//! gray; a hardware device.
//!
//! Behaviour: none (experimental)

use dereth_gfx::Format;
use dereth_remaster::assets::pack::PackModel;
use dereth_remaster::assets::{
    Appearance, AssetSource, BlendFactor, CellPieceKey, Kind, Material, MaterialKey, Mesh,
    TerrainCell, Texture, TextureKey, Use,
};
use dereth_remaster::render::light::LightMode;
use dereth_remaster::render::sky::SkySettings;
use dereth_remaster::render::Renderer;
use dereth_remaster::scene::{
    BlockId, Camera, Fog, FrameDelta, PartLook, RetailLight, SkyObject, Terrain,
};
use glam::{Mat4, Vec3};

const SIZE: (u32, u32) = (320, 240);
const EYE: Vec3 = Vec3::new(96.0, 96.0, 20.0);
/// The fog's color, a mid gray.
const FOG: [u8; 3] = [128, 128, 128];

/// Nothing but the sky.
struct Source;

impl AssetSource for Source {
    fn gfxobj(&mut self, _id: u32) -> Option<Mesh> {
        None
    }

    fn cell_piece(&mut self, _key: CellPieceKey) -> Option<Mesh> {
        None
    }

    fn appearance(&mut self, _surface: u32, _look: Option<&PartLook>) -> Appearance {
        Appearance::default()
    }

    fn material(&mut self, _key: &MaterialKey, _used: Use) -> Material {
        Material {
            kind: Kind::Item,
            surface_type: 2,
            data_category: Some(1),
            texture: None,
            clip_map: false,
            alpha_test: None,
            vertex_alpha: 255,
            vertex_colored: false,
            translucent: None,
            additive: false,
            blended: false,
            src_blend: BlendFactor::One,
            dst_blend: BlendFactor::Zero,
            z_write: true,
            fogged: true,
            solid: None,
            luminosity: 0.0,
            diffuse: 1.0,
            maps: None,
        }
    }

    fn texture(&mut self, _key: &TextureKey, _clip_map: bool) -> Option<Texture> {
        None
    }

    fn terrain_cells(&mut self, _block: BlockId, _t: &Terrain) -> Vec<TerrainCell> {
        Vec::new()
    }

    fn sky_objects(&mut self) -> Vec<u32> {
        Vec::new()
    }

    fn sky(&mut self, _id: u32) -> Option<Mesh> {
        None
    }

    fn pack_model(&mut self, _id: u32) -> Option<PackModel> {
        None
    }
}

/// The frame: the eye looking north and thirty degrees up at noon on a clear day, in gray fog,
/// with `veil` of the fog laid over the sky.
fn frame(veil: f32) -> FrameDelta {
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    FrameDelta {
        origin: [0xA9, 0xB4],
        landscape: true,
        day_group: "Clear".into(),
        // An untinted dome, which the source draws nothing of: the physical sky alone shows.
        sky: vec![SkyObject {
            gfx: 1,
            properties: 0,
            frame: Mat4::IDENTITY,
            before: true,
            after: false,
            material: None,
            uv_offset: [0.0; 2],
            fogged: false,
            deck: None,
            tint: None,
        }],
        sky_veil: veil,
        clock: 40.0,
        camera: Camera {
            view: Mat4::look_to_lh(EYE, Vec3::new(0.0, 0.866, 0.5).normalize(), Vec3::Z),
            projection: Mat4::perspective_lh(1.0, aspect, 0.3, 1000.0),
            previous_view_projection: None,
            eye: EYE,
            viewport: [0, 0, SIZE.0, SIZE.1],
            fov_y: 1.0,
            aspect,
            near: 0.3,
            far: 1000.0,
        },
        light: RetailLight {
            sun: Vec3::new(0.3, -0.2, 0.9),
            sun_color: [255; 3],
            ambient_level: 0.3,
            ambient_color: [255; 3],
            object_ambient: [0.3; 3],
            fog: Some(Fog {
                start: 100.0,
                end: 400.0,
                color: FOG,
            }),
            outdoor: true,
            sees_outside: true,
            time_of_day: 0.5,
            ..RetailLight::default()
        },
        ..FrameDelta::default()
    }
}

/// The mean color of the sky drawn with `veil` of the fog over it, and the most any pixel's
/// channel stands off that mean.
fn sky(veil: f32) -> ([f64; 3], f64) {
    let mut r = Renderer::new(crate::hardware(), Format::D32Float).expect("a renderer");
    // The physical sky is drawn where the light is worked out per pixel.
    r.set_mode(LightMode::Pixel);
    r.set_sky(SkySettings {
        atmosphere: true,
        ..SkySettings::default()
    });
    let mut src = Source;
    let mut color = Vec::new();
    // A few frames, so the sky's tables are made and settled.
    for k in 0..4u8 {
        let mut f = frame(veil);
        f.clock += f64::from(k) * 0.1;
        r.receive(Vec::new(), f, [None; 2], &mut src);
        assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
        color = r.read_back().expect("a frame").color;
    }
    crate::no_validation_errors();
    let n = (color.len() / 4) as f64;
    let mut mean = [0.0; 3];
    for p in color.chunks(4) {
        for (c, m) in mean.iter_mut().enumerate() {
            *m += f64::from(p[c]) / n;
        }
    }
    let spread = color
        .chunks(4)
        .flat_map(|p| (0..3).map(move |c| (f64::from(p[c]) - mean[c]).abs()))
        .fold(0.0, f64::max);
    (mean, spread)
}

#[test]
fn a_worlds_sky_tints_lay_their_fog_over_the_remasters_sky_and_without_them_the_air_is_blue() {
    let (clear, _) = sky(0.0);
    let (veiled, spread) = sky(1.0);
    let (half, _) = sky(0.5);
    println!(
        "no veil {clear:.1?}, the whole veil {veiled:.1?} (spread {spread:.1}), half {half:.1?}"
    );
    // Without the veil the clear day's air is blue.
    assert!(clear[2] > clear[0] + 20.0, "the air is blue: {clear:?}");
    // With the whole of it the sky is the fog: as gray as the fog, and even.
    let gray = (veiled[0] - veiled[2])
        .abs()
        .max((veiled[1] - veiled[2]).abs());
    assert!(gray < 6.0, "the sky is the fog's gray: {veiled:?}");
    assert!(
        spread < 6.0,
        "the fog covers the whole sky: {spread:.1} off its mean"
    );
    // Half of it lies between.
    let blue = |m: [f64; 3]| m[2] - m[0];
    assert!(
        blue(half) < blue(clear) && blue(half) > blue(veiled),
        "half the veil: {half:?}"
    );
}
