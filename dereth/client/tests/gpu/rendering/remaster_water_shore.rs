//! The remaster's water at its shores:
//!
//! - **Reflecting the scene drawn mirrored in it**: on still water across a lake, what it
//!   reflects is what the reflection found on screen shows, the hills and the shore across the
//!   water, not a band of the mirrored picture taken from the wrong place in it.
//! - **Shallow water shows its bed**: near the shore the bed shows through the water, clearing
//!   toward the edge, while deep water out in the lake takes on the water's own color.
//! - **The water's surface runs over all the ground its bed is lowered under**: over land inside
//!   the water's edge as well as retail's water.
//! - **The surface lies at the world's height, the bed where bodies stand**: over the lake's
//!   shallows the water's surface is drawn at retail's ground, its bed under it.
//! - **The bed's ground blends into the land a pack lays under the water**: with no line along
//!   the side of the cell that lays the bed.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterCapture, RemasterWater};

use dereth_terrain::land::drawn::{Knob, Knobs};

use crate::instruments::stations::{report_station, Shot};

const W: u32 = 960;
const H: u32 = 540;

/// The water's kind among the material's kinds.
const WATER: u8 = 7;
/// The ground's light class.
const GROUND: u8 = 1;

/// A lake between hills at noon, the body standing on its shore.
const LAKE: &str = "dereth report 1; loc 0xA8B60037 [148.139191 164.770142 27.554998] \
                    -0.993797 0.000000 0.000000 -0.111209; time 0.5; clock 306486600.0; \
                    weather clear";

/// The boxes the water is drawn with: the sky, the fog, the drawn ground and the water.
const BOXES: &str = "Remaster=1,RemasterSky=1,RemasterFog=1,RemasterDetail=1,RemasterWater=1";

/// The remaster's last frame of the lake, the camera held `back` metres behind the body and `up`
/// over it, pitched `pitch` and turned `heading` radians from north, the clock held, the water
/// still and drawn as `water` changes it.
fn lake(heading: f32, water: impl Fn(RemasterWater) -> RemasterWater) -> RemasterCapture {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let report = VisualReport::parse(LAKE).expect("the report reads");
    let mut shot = Shot::open(&store, &mut gpu, &report_station(&report, Some(8)));
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(BOXES).expect("the boxes read");
    shot.follow = Some([12.0, 3.0, -0.12, heading]);
    shot.poll(&mut gpu);
    let w = shot.scene.draw.remaster_water();
    shot.scene.draw.set_remaster_water(
        water(RemasterWater {
            waves: 0.0,
            rings: false,
            wake: false,
            ..w
        }),
        &mut gpu,
    );
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..8 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

/// **Still water reflects in the scene mirrored in it what it reflects found on screen**: across
/// a lake from its shore, turned two ways, the water drawn with the mirrored scene and the water
/// drawn with the reflection found on screen differ by more than sixteen levels on at most a
/// twentieth of the water: the same hills, shore and sky, mirrored where they stand.
#[test]
fn still_water_reflects_in_the_mirrored_scene_what_it_reflects_on_screen() {
    let _gpu = crate::common::gpu_lock();
    let mut out = Vec::new();
    for heading in [4.19, 4.8] {
        let mirrored = lake(heading, |w| RemasterWater { planar: true, ..w });
        let on_screen = lake(heading, |w| RemasterWater { planar: false, ..w });
        if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
            for (what, c) in [("mirrored", &mirrored), ("on-screen", &on_screen)] {
                crate::instruments::stations::write_png(
                    &std::path::Path::new(&dir).join(format!("water-shore-{what}-{heading}.png")),
                    c.width,
                    c.height,
                    &c.color,
                );
            }
        }
        let (mut apart, mut n) = (0usize, 0usize);
        for i in 0..mirrored.kind.len() {
            if mirrored.kind[i] != WATER || on_screen.kind[i] != WATER {
                continue;
            }
            let d = (0..3)
                .map(|k| u32::from(mirrored.color[i * 4 + k].abs_diff(on_screen.color[i * 4 + k])))
                .sum::<u32>();
            n += 1;
            if d > 3 * 16 {
                apart += 1;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let share = apart as f64 / n.max(1) as f64;
        eprintln!(
            "heading {heading}: {:.1}% of {n} water pixels differ by more than 16 levels",
            share * 100.0
        );
        out.push((heading, share, n));
    }
    for (heading, share, n) in out {
        assert!(
            n > 50_000,
            "heading {heading}: the water fills the view: {n} pixels"
        );
        assert!(
            share < 0.05,
            "heading {heading}: {:.1}% of the water reflects otherwise",
            share * 100.0
        );
    }
}

/// The lake's shallows by a stone pillar at noon, the body standing in them, the camera behind it
/// looking along the bank: the bank to the right, the open lake to the left.
const SHALLOWS: &str = "dereth report 1; loc 0xA9B50015 [59.258690 104.685966 27.904999] \
                        0.723986 0.000000 0.000000 -0.689815; time 0.5; clock 306486600.0; \
                        weather clear";

/// The remaster's last frame of the shallows, the water still and taking the light of the bed
/// seen through it `absorption` times as strongly as it is made to.
fn shallows(absorption: f32) -> RemasterCapture {
    still_water(SHALLOWS, [6.0, 2.2, -0.3, -2.5], absorption)
}

/// The remaster's last frame of the place `line` reports, the camera held behind the body as
/// `follow` says (meters back, meters up, its pitch and its heading), the water still and taking
/// the light of the bed seen through it `absorption` times as strongly as it is made to.
fn still_water(line: &str, follow: [f32; 4], absorption: f32) -> RemasterCapture {
    still_water_placed(line, follow, absorption).0
}

/// [`still_water`], with how far render space lies from the world: a render-space point plus it
/// is the point in global meters east and north.
fn still_water_placed(
    line: &str,
    follow: [f32; 4],
    absorption: f32,
) -> (RemasterCapture, glam::Vec2) {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let report = VisualReport::parse(line).expect("the report reads");
    let mut shot = Shot::open(&store, &mut gpu, &report_station(&report, Some(8)));
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(BOXES).expect("the boxes read");
    shot.follow = Some(follow);
    shot.poll(&mut gpu);
    let w = shot.scene.draw.remaster_water();
    shot.scene.draw.set_remaster_water(
        RemasterWater {
            waves: 0.0,
            rings: false,
            wake: false,
            absorption,
            ..w
        },
        &mut gpu,
    );
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..8 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    let c = shot.scene.character.as_ref().expect("a body");
    let (at, drawn) = (c.position(), c.render_frame().origin);
    let block = at.cell.0 >> 16;
    #[allow(clippy::cast_precision_loss)] // block coordinates, exact in f32
    let global = glam::Vec2::new(
        (block >> 8) as f32 * 192.0 + at.frame.origin.x,
        (block & 0xFF) as f32 * 192.0 + at.frame.origin.y,
    );
    let offset = global - glam::Vec2::new(drawn.x, drawn.y);
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    (capture, offset)
}

/// Where pixel `i` of `c` stands, from its depth.
#[allow(clippy::cast_precision_loss)]
fn place_of(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> glam::Vec3 {
    let (w, h) = (c.width as usize, c.height as usize);
    let (x, y) = ((i % w) as f32, (i / w) as f32);
    let ndc = glam::Vec4::new(
        (x + 0.5) / w as f32 * 2.0 - 1.0,
        1.0 - (y + 0.5) / h as f32 * 2.0,
        c.depth[i],
        1.0,
    );
    let p = *inv * ndc;
    (p / p.w).truncate()
}

/// How far from the water's edge each water pixel of `c` lies, meters: from the nearest place
/// the frame shows dry ground beside the water. `None` for a pixel that is not water.
fn from_edge(c: &RemasterCapture) -> Vec<Option<f32>> {
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let (w, n) = (c.width as usize, c.kind.len());
    let ground = |i: usize| c.class[i] == GROUND;
    let water = |i: usize| c.kind[i] == WATER && ground(i);
    // The dry ground beside the water: a dry ground pixel with water next to it.
    let mut edge = Vec::new();
    for i in 0..n {
        if !ground(i) || water(i) {
            continue;
        }
        let beside = [i.wrapping_sub(1), i + 1, i.wrapping_sub(w), i + w];
        if beside.iter().any(|&j| j < n && water(j)) {
            edge.push(place_of(c, &inv, i).truncate());
        }
    }
    (0..n)
        .map(|i| {
            water(i).then(|| {
                let p = place_of(c, &inv, i).truncate();
                edge.iter()
                    .map(|e| e.distance(p))
                    .fold(f32::INFINITY, f32::min)
            })
        })
        .collect()
}

/// **Shallow water near the shore shows its bed, and deep water does not**: in a lake's shallows
/// by its bank, the water drawn taking the light of its bed as it is made to, as not at all, and
/// as wholly as it can: of how far the water's own color can cover the bed, it covers under 60%
/// within four meters of the water's edge and under 90% six to ten meters out, and over 80%
/// fifteen meters out and farther, where it stands as deep as physics sinks a body.
#[test]
fn shallow_water_near_the_shore_shows_its_bed_and_deep_water_does_not() {
    let _gpu = crate::common::gpu_lock();
    let made = shallows(1.0);
    let clear = shallows(0.0);
    let thick = shallows(50.0);
    let edge = from_edge(&made);
    if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
        crate::instruments::stations::write_png(
            &std::path::Path::new(&dir).join("water-shallows.png"),
            made.width,
            made.height,
            &made.color,
        );
    }
    // Of how far the water's own color can move each pixel from the bed it covers, how far it
    // does, over the pixels `from`..`to` meters from the edge; with how many pixels, and how far
    // it can move them all.
    let covered = |from: f32, to: f32| {
        let (mut moved, mut most, mut n) = (0u64, 0u64, 0usize);
        for (i, d) in edge.iter().enumerate() {
            let Some(d) = d else { continue };
            if *d < from || *d >= to || clear.kind[i] != WATER || thick.kind[i] != WATER {
                continue;
            }
            for k in 0..3 {
                let c = clear.color[i * 4 + k];
                moved += u64::from(made.color[i * 4 + k].abs_diff(c));
                most += u64::from(thick.color[i * 4 + k].abs_diff(c));
            }
            n += 1;
        }
        #[allow(clippy::cast_precision_loss)]
        let share = moved as f64 / most.max(1) as f64;
        (share, n, most)
    };
    let near = covered(0.0, 4.0);
    let mid = covered(6.0, 10.0);
    let deep = covered(15.0, f32::INFINITY);
    println!(
        "the water's own color covers {:.2} of the bed within 4 m of the edge ({} px), {:.2} \
         6-10 m out ({} px), {:.2} past 15 m ({} px)",
        near.0, near.1, mid.0, mid.1, deep.0, deep.1
    );
    for (what, band) in [
        ("near", near),
        ("six to ten meters out", mid),
        ("deep", deep),
    ] {
        assert!(
            band.1 > 2_000 && band.2 > 3 * band.1 as u64,
            "{what}: the band shows the water's color apart from its bed: {band:?}"
        );
    }
    assert!(
        near.0 < 0.6,
        "the shallows show their bed: {:.2} covered",
        near.0
    );
    assert!(
        mid.0 < 0.9,
        "six to ten meters out, the bed shows: {:.2} covered",
        mid.0
    );
    assert!(
        deep.0 > 0.8,
        "deep water takes the water's own color: {:.2}",
        deep.0
    );
}

/// A strip of land at the water's level between two waters, which the bed is lowered under.
const STRIP: &str = "dereth report 1; loc 0xA9B50008 [11.774128 187.334106 27.554998] 0.161152 \
                     0.000000 0.000000 -0.986930; time 0.5; clock 306486600.0; weather clear";

/// **The water's surface runs over all the ground its bed is lowered under**: across the strip of
/// land between two waters, every pixel of ground the bed is drawn lowered under is drawn as
/// water, and none the bed is not lowered under is unless retail's picture lays water there.
#[test]
fn the_waters_surface_runs_over_all_the_ground_its_bed_is_lowered_under() {
    let _gpu = crate::common::gpu_lock();
    let strip = still_water(STRIP, [6.0, 2.2, -0.3, -2.82], 1.0);
    if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
        crate::instruments::stations::write_png(
            &std::path::Path::new(&dir).join("water-strip.png"),
            strip.width,
            strip.height,
            &strip.color,
        );
    }
    let (mut lowered, mut dry, mut flooded) = (0usize, 0usize, 0usize);
    for i in 0..strip.kind.len() {
        if strip.class[i] != GROUND {
            continue;
        }
        let water = strip.kind[i] == WATER;
        // Eight steps, eight centimeters: past where the surface fades in at the water's edge.
        if strip.bed[i] >= 8 {
            lowered += 1;
            if !water {
                dry += 1;
            }
        } else if strip.bed[i] == 0 && water && strip.water[i] < 128 {
            flooded += 1;
        }
    }
    println!(
        "{lowered} ground pixels with the bed lowered, {dry} of them drawn dry; {flooded} drawn \
         water with no bed lowered and retail's land there"
    );
    assert!(
        lowered > 20_000,
        "the frame shows the strip's water: {lowered}"
    );
    assert_eq!(dry, 0, "every pixel the bed is lowered under is water");
    assert_eq!(
        flooded, 0,
        "no land is flooded where the bed is not lowered"
    );
}

/// **The water's surface lies at the world's height, and its bed where bodies stand**: over the
/// lake's shallows, wherever the bed is drawn two fifths of a meter or more under the water within
/// forty meters of the eye, the surface over it is drawn at retail's ground to a few centimeters,
/// and the bed no deeper than physics sinks a body, 0.9 meters.
#[test]
fn the_waters_surface_lies_at_the_worlds_height_and_its_bed_where_bodies_stand() {
    let _gpu = crate::common::gpu_lock();
    let (c, offset) = still_water_placed(SHALLOWS, [6.0, 2.2, -0.3, -2.5], 1.0);
    let store = crate::common::dats();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let land = dereth_world_data::land_source::DatLandSource::new(store, &region)
        .expect("the land source opens");
    // Retail's ground at render-space `(x, y)`.
    let retail = |x: f32, y: f32| {
        let g = glam::Vec2::new(x, y) + offset;
        let (bx, by) = ((g.x / 192.0).floor(), (g.y / 192.0).floor());
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // inside the world
        let id = dereth_primitives::LandblockId(((bx as u16) << 8) | by as u16);
        land.ground_height(id, g.x - bx * 192.0, g.y - by * 192.0)
    };
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let eye = glam::Vec3::from(c.eye);
    let (mut under, mut off, mut deepest, mut worst) = (0usize, 0usize, 0.0f32, 0.0f32);
    for i in 0..c.kind.len() {
        if c.class[i] != GROUND || c.kind[i] != WATER {
            continue;
        }
        let depth = f32::from(c.bed[i]) * 0.01;
        deepest = deepest.max(depth);
        let bed = place_of(&c, &inv, i);
        if depth < 0.4 || bed.distance(eye) > 40.0 {
            continue;
        }
        let Some(ground) = retail(bed.x, bed.y) else {
            continue;
        };
        under += 1;
        let miss = (bed.z + depth - ground).abs();
        worst = worst.max(miss);
        if miss > 0.03 {
            off += 1;
        }
    }
    println!(
        "{under} pixels of bed, {off} with the surface off retail's ground, worst {worst}; \
         deepest {deepest}"
    );
    assert!(under > 10_000, "the frame shows the lake's bed: {under}");
    assert!(
        off * 100 <= under,
        "the surface at retail's ground: {off} of {under} off"
    );
    assert!(
        deepest > 0.8 && deepest < 0.93,
        "the bed as deep as physics sinks a body: {deepest}"
    );
}

/// A pond with lily pads, the camera over a body standing in it, looking down across the side of
/// a cell whose water a pack lays as the bed.
const POND: &str = "dereth report 1; loc 0xA8B60012 [55.615215 30.300829 27.554998] 0.069254 \
                    0.000000 0.000000 -0.997599; camera 0xA8B6 [54.51643 35.022827 32.58985] yaw \
                    -2.9129736 pitch -0.6300002 cell 0xA8B60012; time 0.5; clock 306486600.0; \
                    weather clear";

/// A PNG of four by four texels of `rgba`.
fn flat_png(rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("header");
        writer.write_image_data(&rgba.repeat(16)).expect("data");
    }
    out
}

/// A pack that lays the bed's ground (gray sand) in blue and every other dry terrain type in
/// green.
fn bed_pack() -> std::sync::Arc<dereth_scene::art_pack::ArtPack> {
    let bed = Knobs::default().get(Knob::WaterBed);
    // LINT-OK: a terrain type, 0 to 31.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bed = bed as u8;
    let dry: Vec<String> = (0..16u8)
        .chain(21..32)
        .filter(|t| *t != bed)
        .map(|t| t.to_string())
        .collect();
    let terrain = |ground: &str, picture: &str| {
        format!(
            "[[terrain]]\nground = [{ground}]\nbase_color = \"{picture}\"\nsize = 4\n\
             author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n"
        )
    };
    let manifest = format!(
        "[pack]\nname = \"Bed\"\nformat = 1\n{}{}",
        terrain(&format!("{}, \"road\"", dry.join(", ")), "green.png"),
        terrain(&bed.to_string(), "blue.png"),
    );
    let (green, blue) = (flat_png([20, 200, 20, 128]), flat_png([20, 20, 200, 128]));
    std::sync::Arc::new(
        dereth_scene::art_pack::ArtPack::open(&manifest, &mut |path| match path {
            "green.png" => Ok(green.clone()),
            "blue.png" => Ok(blue.clone()),
            _ => Err(format!("{path}: missing")),
        })
        .expect("the pack opens"),
    )
}

/// **The bed's ground blends into the land a pack lays under the water, with no line between**:
/// in a pond, with the water not drawn and a pack that lays the bed blue and the land beside the
/// water green, the ground within twenty-five meters of the eye goes from one to the other, and
/// all but one in five thousand pairs of pixels side by side there, clear of what stands on it,
/// differ by no more than a fifth of the way between them.
#[test]
fn the_beds_ground_blends_into_the_land_under_the_water_with_no_line_between() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let report = VisualReport::parse(POND).expect("the report reads");
    let mut shot = Shot::open(&store, &mut gpu, &report_station(&report, Some(3)));
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(Some(bed_pack()), None);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch("Remaster=1,RemasterDetail=1,RemasterArtPack=1")
            .expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..8 {
        shot.step_held(&mut gpu);
    }
    let c = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
        crate::instruments::stations::write_png(
            &std::path::Path::new(&dir).join("bed-edge.png"),
            c.width,
            c.height,
            &c.color,
        );
    }
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let eye = glam::Vec3::from(c.eye);
    let (w, n) = (c.width as usize, c.kind.len());
    // How blue each near ground pixel is, of its color: 0 the land, 1 the bed.
    // Ground all round, clear of the edges of what stands on it.
    let h = n / w;
    let clear = |i: usize| {
        let (x, y) = (i % w, i / w);
        x > 2
            && y > 2
            && x + 3 < w
            && y + 3 < h
            && (0..49).all(|k| c.class[(y + k / 7 - 3) * w + x + k % 7 - 3] == GROUND)
    };
    let blue: Vec<Option<f32>> = (0..n)
        .map(|i| {
            if !clear(i) || place_of(&c, &inv, i).distance(eye) > 25.0 {
                return None;
            }
            let [r, g, b] = [0, 1, 2].map(|k| f32::from(c.color[i * 4 + k]));
            // Too dark to tell the two apart: in a shadow or a crease.
            if r + g + b < 60.0 {
                return None;
            }
            let share = b / (r + g + b).max(1.0);
            // A tenth blue is the green; nine tenths the blue.
            Some(((share - 0.1) / 0.8).clamp(0.0, 1.0))
        })
        .collect();
    let (mut bed, mut land, mut pairs, mut steps, mut worst) = (0, 0, 0, 0, 0.0f32);
    let mut marked = c.color.clone();
    for i in 0..n {
        let Some(a) = blue[i] else { continue };
        if a > 0.9 {
            bed += 1;
        } else if a < 0.1 {
            land += 1;
        }
        for j in [i + 1, i + w] {
            if j >= n || (j == i + 1 && j % w == 0) {
                continue;
            }
            let Some(b) = blue[j] else { continue };
            pairs += 1;
            worst = worst.max((a - b).abs());
            if (a - b).abs() > 0.2 {
                steps += 1;
                marked[i * 4..i * 4 + 3].copy_from_slice(&[255, 0, 255]);
            }
        }
    }
    println!("{bed} bed pixels, {land} land; {steps} steps of {pairs} pairs, worst {worst}");
    if let Some(dir) = std::env::var_os("DERETH_REMASTER_CAPTURE_DIR") {
        crate::instruments::stations::write_png(
            &std::path::Path::new(&dir).join("bed-edge-steps.png"),
            c.width,
            c.height,
            &marked,
        );
    }
    assert!(
        bed > 2_000 && land > 2_000,
        "the bed and the land are in view: {bed}, {land}"
    );
    assert!(
        steps <= pairs / 5000,
        "a line between them: {steps} of {pairs}"
    );
}
