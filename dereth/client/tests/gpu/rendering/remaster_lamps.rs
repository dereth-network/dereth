//! Behaviour: none (experimental)
//!
//! The remaster's lamps: the lights each piece of the view lists, and the light they add.
//!
//! - **The lists.** At every station of the light study, with the lamps lighting the indoors and
//!   the night, each piece of the view lists on the device exactly the lamps the processor lists
//!   for it from the same lamps, but where a lamp's reach and the piece only just meet or miss
//!   (within a millimetre); the lamps listed are the light pools' and, at night, the outdoor
//!   lamps within reach; no piece's list is cut short at a station.
//! - **The light.** Indoors, every room's and indoor object's pixel takes the lamps in place of
//!   retail's own light (the alpha is 1), and some of them are lit; outdoors by day nothing is,
//!   and at night the lamps light some of the ground and what stands on it.
//! - **Off.** With the lamps off nothing is listed and nothing is drawn.
//! - **The rooms' brightness.** With the light per pixel and the indoor lamps on, at the four
//!   indoor stations, every room the frame shows 2,000 pixels or more of has its walls, on
//!   average, within [`ROOM_BAND`] display levels of Retail Look's, and the objects indoors
//!   together within [`OBJECT_BAND`]; the lamps change a quarter or more of the indoor pixels.
//! - **Their glow.** At night a lamp's own flame or glass is bright, and a halo stands round it where
//!   its light reaches nothing.
//! - **Nothing kept.** The lamps keep nothing of the rooms from frame to frame: after a walk, their
//!   light is the same whether they were on all along or turned on at the end.
//! - **As bright as retail's light.** With the boxes as the options page ticks them at first, the
//!   indoor lamps light no part of a room brighter than retail's own light does there.
//! - **Broad, soft pools.** At night the outdoor lamps light a broad pool round them, brightest by
//!   the lamp and fading outward; a lamp inside a house lights nothing outdoors.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterHaloSeen, RemasterLampCapture, RemasterLamps};

use crate::instruments::remaster_frames::{self, STATIONS};
use crate::instruments::stations::Shot;

/// The light classes the remaster's material target carries.
const GROUND: u8 = 1;
const SUN: u8 = 2;
const INDOOR: u8 = 3;
const ROOM: u8 = 4;

/// The station `name` drawn six frames by the remaster with its lamps as `lamps` says, and what
/// the lamps drew last, with the remaster's own light classes.
fn lamps_at(
    store: &Arc<RetailDatStore>,
    name: &str,
    lamps: RemasterLamps,
) -> (Option<RemasterLampCapture>, Vec<u8>, u32) {
    let station = remaster_frames::stations(store, &[name])
        .pop()
        .expect("the station");
    let cfg = DeviceConfig {
        width: 960,
        height: 540,
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
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene.draw.set_remaster_lamps(lamps, &mut gpu);
    let built = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .map_or(0, |c| c.pipelines_built);
    for _ in 0..6 {
        shot.step(&mut gpu);
    }
    let capture = shot.scene.draw.remaster_lamp_capture(&mut gpu);
    let class = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew")
        .class;
    let after = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .map_or(0, |c| c.pipelines_built);
    assert_eq!(after, built, "{name}: a pipeline was built after start-up");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{name}: the validation layer: {errors:#?}"
    );
    (capture, class, u32::try_from(after).unwrap_or(0))
}

/// At every station of the light study, with the lamps lighting the indoors and the night: each
/// piece of the view lists on the device the lamps the processor lists for it, but on the edge
/// of a lamp's reach; no list is cut short; indoors every room's and indoor object's pixel
/// takes the lamps' light, and some are lit; outdoors by day no pixel is lit by a lamp, and at
/// night some are.
#[test]
fn every_piece_lists_the_lamps_that_reach_it_and_the_lamps_light_what_they_should() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let all = RemasterLamps {
        indoor: true,
        outdoor: true,
    };
    for name in STATIONS {
        let (capture, class, _) = lamps_at(&store, name, all);
        let c = capture.unwrap_or_else(|| panic!("{name}: the lamps drew"));
        let listed: usize = c.lists.iter().map(Vec::len).sum();
        let longest = c.lists.iter().map(Vec::len).max().unwrap_or(0);
        let worst = c
            .differences
            .iter()
            .map(|d| d.2.abs())
            .fold(0.0f32, f32::max);
        let (mut indoor, mut replaced, mut lit_in, mut outdoor, mut lit_out) = (0, 0, 0, 0, 0);
        for (p, cls) in c.light.iter().zip(&class) {
            let lit = p[0] + p[1] + p[2] > 1.0 / 255.0;
            match *cls {
                INDOOR | ROOM => {
                    indoor += 1;
                    replaced += usize::from(p[3] == 1.0);
                    lit_in += usize::from(lit);
                }
                GROUND | SUN => {
                    outdoor += 1;
                    assert!(
                        p[3] <= 0.0,
                        "{name}: an outdoor pixel took the lamps' place"
                    );
                    lit_out += usize::from(lit);
                }
                _ => assert!(
                    p[0] + p[1] + p[2] + p[3] == 0.0,
                    "{name}: the lamps lit a pixel of the sky or of nothing"
                ),
            }
        }
        eprintln!(
            "{name}: {} lamps, {listed} listings, longest {longest}, {} differ (worst {worst:.5} m); \
             indoors {indoor} pixels, {lit_in} lit; outdoors {outdoor}, {lit_out} lit",
            c.lamps,
            c.differences.len(),
        );
        // The lamps' kinds, by reach and strengths, as the calibration made them.
        let mut kinds: Vec<String> = c
            .strengths
            .iter()
            .map(|(indoor, reach, s)| {
                format!(
                    "{} reach {reach:.1} walls {:.2} objects {:.2} outdoors {:.2}",
                    if *indoor { "indoor" } else { "outdoor" },
                    s[0],
                    s[1],
                    s[2]
                )
            })
            .collect();
        kinds.sort();
        kinds.dedup();
        eprintln!("{name}: {kinds:#?}");
        assert!(
            worst < 1e-3,
            "{name}: a list differs from the processor's away from a lamp's edge: {:?}",
            c.differences
        );
        assert!(
            longest < 63,
            "{name}: a piece's list was cut short ({longest})"
        );
        assert_eq!(
            replaced, indoor,
            "{name}: every indoor pixel takes the lamps"
        );
        if indoor > 10_000 {
            assert!(
                lit_in > indoor / 10,
                "{name}: the lamps lit {lit_in} of {indoor}"
            );
        }
        match name {
            "holtburg" | "forest" | "rain" | "rain-forest" | "snow-village" => {
                assert_eq!(lit_out, 0, "{name}: a lamp lit the outdoors by day");
            }
            "night" | "lamps-street" => {
                assert!(lit_out > 1_000, "{name}: the lamps lit {lit_out} outdoors");
            }
            _ => {}
        }
    }
}

/// With the lamps off, the remaster lists none and draws none.
#[test]
fn with_the_lamps_off_nothing_is_listed_or_drawn() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    for name in ["indoor", "lamps-street"] {
        let (capture, _, _) = lamps_at(&store, name, RemasterLamps::default());
        assert!(capture.is_none(), "{name}: the lamps drew while off");
    }
}

/// The depth retail stamps an opening with, and anything beyond: not the world.
const STAMP: f32 = 0.999_998;

/// One station drawn by the remaster with the light per pixel and its lamps as `lamps` says: the
/// frame's pixels, and on the frame's own grid the remaster's light class, room (the low half of
/// a room's cell id, on its walls, while the lamps draw) and depth.
struct Lit {
    rgba: Vec<u8>,
    class: Vec<u8>,
    room: Vec<u16>,
    depth: Vec<f32>,
}

fn lit(store: &Arc<RetailDatStore>, name: &str, size: (u32, u32), lamps: RemasterLamps) -> Lit {
    let station = remaster_frames::stations(store, &[name])
        .pop()
        .expect("the station");
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_lamps(lamps, &mut gpu);
    for _ in 0..6 {
        shot.step(&mut gpu);
    }
    let rgba = crate::instruments::stations::capture(&mut gpu);
    let (w, h) = gpu.size();
    let c = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    let material = shot
        .scene
        .draw
        .remaster_lamp_capture(&mut gpu)
        .map(|l| l.material);
    let n = (w * h) as usize;
    let (mut class, mut room, mut depth) = (vec![0u8; n], vec![0u16; n], vec![1.0f32; n]);
    for y in 0..c.height {
        for x in 0..c.width {
            let (fx, fy) = (x + c.x, y + c.y);
            if fx < w && fy < h {
                let at = (fy * w + fx) as usize;
                let from = (y * c.width + x) as usize;
                class[at] = c.class[from];
                depth[at] = c.depth[from];
                if let Some(m) = material.as_ref().and_then(|m| m.get(from)) {
                    room[at] = u16::from(m[2]) | (u16::from(m[3]) << 8);
                }
            }
        }
    }
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{name}: the validation layer: {errors:#?}"
    );
    Lit {
        rgba,
        class,
        room,
        depth,
    }
}

/// Some pixels' mean brightness, display levels, as Retail Look and the remaster drew them.
#[derive(Debug, Clone, Copy, Default)]
struct Mean {
    pixels: usize,
    retail: f64,
    lamps: f64,
}

impl Mean {
    fn off(&self) -> f64 {
        self.lamps - self.retail
    }
}

/// The rooms' walls' mean brightness, each room by its walls' pixels both drew, then every
/// indoor object's pixel's together.
fn room_means(retail: &remaster_frames::Frame, r: &Lit) -> (Vec<(u16, Mean)>, Mean) {
    // ORDER-OK: reported in room order.
    let mut rooms: std::collections::BTreeMap<u16, (usize, u64, u64)> =
        std::collections::BTreeMap::new();
    let mut objects = (0usize, 0u64, 0u64);
    for i in 0..r.class.len() {
        let drew = retail.depth.get(i).is_some_and(|d| *d < STAMP) && r.depth[i] < 1.0;
        if !drew {
            continue;
        }
        let sum = |px: &[u8]| -> u64 { (0..3).map(|c| u64::from(px[i * 4 + c])).sum() };
        let (a, b) = (sum(&retail.rgba), sum(&r.rgba));
        let e = match r.class[i] {
            ROOM => rooms.entry(r.room[i]).or_default(),
            INDOOR => &mut objects,
            _ => continue,
        };
        e.0 += 1;
        e.1 += a;
        e.2 += b;
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts and sums of bytes
    let mean = |(n, a, b): (usize, u64, u64)| Mean {
        pixels: n,
        retail: a as f64 / (n.max(1) * 3) as f64,
        lamps: b as f64 / (n.max(1) * 3) as f64,
    };
    (
        rooms.into_iter().map(|(id, m)| (id, mean(m))).collect(),
        mean(objects),
    )
}

/// Indoors, with the light per pixel and the indoor lamps on, every room the frame shows 2,000
/// pixels or more of has its walls' mean brightness within [`ROOM_BAND`] of Retail Look's, but
/// for [`ACCEPTED_ROOM`] within [`ACCEPTED_ROOM_BAND`], and
/// the objects indoors theirs within [`OBJECT_BAND`]; and the lamps change a quarter or more of
/// the rooms' and objects' pixels from what the light per pixel draws without them (a third,
/// measured: where no static light reaches, the lamps are retail's own light).
#[test]
fn every_rooms_walls_are_as_bright_as_retail_looks_within_the_band() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (960, 540);
    let on = RemasterLamps {
        indoor: true,
        outdoor: false,
    };
    let (mut checked, mut changed_all, mut inside_all) = (0, 0usize, 0usize);
    for name in ["indoor", "doorway", "dungeon", "dungeon-candles"] {
        let station = remaster_frames::stations(&store, &[name])
            .pop()
            .expect("the station");
        let retail = remaster_frames::run(&store, &station, remaster_frames::Draw::Retail, size);
        let r = lit(&store, name, size, on);
        let off = lit(&store, name, size, RemasterLamps::default());
        let (rooms, objects) = room_means(&retail, &r);
        for (id, m) in &rooms {
            eprintln!(
                "{name} room {id:04x}: {} px, Retail Look {:.2}, lamps {:.2}, {:+.2}",
                m.pixels,
                m.retail,
                m.lamps,
                m.off()
            );
        }
        eprintln!(
            "{name} objects indoors: {} px, Retail Look {:.2}, lamps {:.2}, {:+.2}",
            objects.pixels,
            objects.retail,
            objects.lamps,
            objects.off()
        );
        let (mut inside, mut changed) = (0usize, 0usize);
        for (i, cls) in r.class.iter().enumerate() {
            if matches!(*cls, ROOM | INDOOR) {
                inside += 1;
                changed += usize::from(r.rgba[i * 4..i * 4 + 3] != off.rgba[i * 4..i * 4 + 3]);
            }
        }
        eprintln!("{name}: the lamps changed {changed} of {inside} indoor pixels");
        changed_all += changed;
        inside_all += inside;
        for (id, m) in rooms.iter().filter(|r| r.1.pixels >= 2_000) {
            let band = if ACCEPTED_ROOM == (name, *id) {
                ACCEPTED_ROOM_BAND
            } else {
                ROOM_BAND
            };
            assert!(
                m.off().abs() <= band,
                "{name} room {id:04x} ({} px): {:.2} against Retail Look's {:.2}",
                m.pixels,
                m.lamps,
                m.retail
            );
            checked += 1;
        }
        assert!(
            objects.off().abs() <= OBJECT_BAND,
            "{name}: the objects indoors {:.2} against Retail Look's {:.2}",
            objects.lamps,
            objects.retail
        );
    }
    assert!(checked >= 16, "{checked} rooms checked");
    assert!(
        changed_all * 4 > inside_all,
        "the lamps changed {changed_all} of {inside_all} indoor pixels"
    );
}

/// How far a room's walls' mean brightness may be from Retail Look's, display levels: measured
/// at the four indoor stations at 960 by 540, the farthest is 5.3 over, round the candles of the
/// candles' station, and 4.75 under (the static lamps on walls are retail's burn worked out at
/// every pixel, where retail draws it between the walls' corners).
const ROOM_BAND: f64 = 6.0;

/// The one room outside [`ROOM_BAND`], a station and a room: with the light per pixel its walls
/// draw darker than Retail Look's, and that is accepted.
const ACCEPTED_ROOM: (&str, u16) = ("indoor", 0x0053);

/// How far [`ACCEPTED_ROOM`]'s walls may be from Retail Look's, display levels: measured at 960
/// by 540, 17.3 under.
const ACCEPTED_ROOM_BAND: f64 = 18.0;

/// How far the objects indoors' mean brightness may be from Retail Look's, display levels:
/// measured the same way, the farthest is 2.6.
const OBJECT_BAND: f64 = 4.0;

/// The lamps' light, and the light classes, of the dungeon station's frame once the body has
/// turned for a second and a half and walked for a second and a half, the light per pixel on and
/// the indoor lamps on from the start (`early`) or only for the last two frames.
fn lamps_after_a_walk(store: &Arc<RetailDatStore>, early: bool) -> (Vec<[f32; 4]>, Vec<u8>) {
    let station = remaster_frames::stations(store, &["dungeon"])
        .pop()
        .expect("the station");
    let cfg = DeviceConfig {
        width: 640,
        height: 360,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let on = RemasterLamps {
        indoor: true,
        outdoor: false,
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(dereth_scene::world_scene::RemasterLight::Pixel, &mut gpu);
    if early {
        shot.scene.draw.set_remaster_lamps(on, &mut gpu);
    }
    for _ in 0..6 {
        shot.step(&mut gpu);
    }
    for (turn, forward) in [(true, false), (false, true)] {
        shot.input = dereth_client_runtime::character::CharacterInput {
            turn_right: turn,
            forward,
            ..Default::default()
        };
        for _ in 0..45 {
            shot.step(&mut gpu);
        }
    }
    shot.input = dereth_client_runtime::character::CharacterInput::default();
    shot.scene.draw.set_remaster_lamps(on, &mut gpu);
    for _ in 0..2 {
        shot.step(&mut gpu);
    }
    let light = shot
        .scene
        .draw
        .remaster_lamp_capture(&mut gpu)
        .expect("the lamps drew")
        .light;
    let class = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew")
        .class;
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    (light, class)
}

/// The lamps keep nothing of the rooms from one frame to the next: after the body has turned and
/// walked through the dungeon, the lamps' light is the same to the bit whether they were on all
/// the way or turned on only for the last two frames.
#[test]
fn the_lamps_keep_nothing_of_the_rooms_from_frame_to_frame() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (all_along, class) = lamps_after_a_walk(&store, true);
    let (at_the_end, _) = lamps_after_a_walk(&store, false);
    assert_eq!(all_along.len(), at_the_end.len());
    let (mut indoor, mut lit, mut differ) = (0usize, 0usize, 0usize);
    for ((a, b), c) in all_along.iter().zip(&at_the_end).zip(&class) {
        if matches!(*c, ROOM | INDOOR) {
            indoor += 1;
            lit += usize::from(a[0] + a[1] + a[2] > 1.0 / 255.0);
        }
        differ += usize::from(a.map(f32::to_bits) != b.map(f32::to_bits));
    }
    eprintln!("dungeon: {indoor} indoor pixels, {lit} lit, {differ} differ");
    assert!(lit > indoor / 10, "{lit} of {indoor} lit");
    assert_eq!(differ, 0, "the lamps' light depends on what came before");
}

/// The station the lamps' glow and halos are checked at: four flames and two hung lamps on the
/// block south-east of Holtburg at midnight, some of them behind what stands nearer the eye.
const HALO_STATION: &str = "lamps-torch";

/// The station `name` drawn with `boxes` until it settles: the frame's colors, and what the lamps
/// drew.
fn street_with(
    store: &Arc<RetailDatStore>,
    name: &str,
    boxes: &str,
    size: (u32, u32),
) -> (Vec<u8>, Option<RemasterLampCapture>) {
    let station = remaster_frames::stations(store, &[name])
        .pop()
        .expect("the station");
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        dereth_client_runtime::render_prefs::FidelityPreferences::parse_switch(boxes)
            .expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let rgba = crate::instruments::stations::capture(&mut gpu);
    let lamps = shot.scene.draw.remaster_lamp_capture(&mut gpu);
    drop(shot);
    (rgba, lamps)
}

/// **At night a lamp glows**: at [`HALO_STATION`], with every box ticked, the outdoor lamps' row
/// brightens the lamp's own flame or glass, which is dull without it, and stands a halo where the
/// lamps' light itself reaches nothing.
#[test]
fn at_night_a_lamp_glows_and_stands_a_halo() {
    use crate::instruments::remaster_look_frames::EVERY_BOX;
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (1280, 720);
    let without = EVERY_BOX.replace("RemasterOutdoorLamps=1,", "");
    let (on, lamps) = street_with(&store, HALO_STATION, EVERY_BOX, size);
    let (off, _) = street_with(&store, HALO_STATION, &without, size);
    let lamps = lamps.expect("the lamps drew");
    let (mut bright_on, mut bright_off, mut glowing, mut halo) = (0usize, 0usize, 0usize, 0usize);
    for (i, l) in lamps.light.iter().enumerate() {
        let (a, b) = (&on[i * 4..i * 4 + 3], &off[i * 4..i * 4 + 3]);
        bright_on += usize::from(a[0] >= 170);
        bright_off += usize::from(b[0] >= 170);
        glowing += usize::from(l[3] < 0.0);
        let unlit = l[0].max(l[1]).max(l[2]) < 1.0 / 255.0;
        halo += usize::from(unlit && i32::from(a[0]) - i32::from(b[0]) > 12);
    }
    eprintln!(
        "bright pixels {bright_on} with the lamps, {bright_off} without; {glowing} glow;          {halo} moved by a halo alone"
    );
    assert!(bright_on >= 20, "the flames: {bright_on}");
    assert!(bright_off * 4 <= bright_on, "without them: {bright_off}");
    assert!(glowing >= 10, "the glowing parts: {glowing}");
    assert!(halo >= 200, "the halos: {halo}");
}

/// The boxes with the lamps' row, without what would carry a halo's light to other pixels or over
/// time: bloom and smoothing.
fn halo_boxes() -> String {
    use crate::instruments::remaster_look_frames::EVERY_BOX;
    EVERY_BOX
        .replace("RemasterBloom=1,", "")
        .replace("RemasterSmoothing=1,", "")
}

/// The mean of red over the square `radius` pixels round `at` on a frame `width` wide.
fn red_round(rgba: &[u8], width: u32, at: (f32, f32), radius: i64) -> f64 {
    let (w, h) = (i64::from(width), rgba.len() as i64 / 4 / i64::from(width));
    #[allow(clippy::cast_possible_truncation)] // on the frame
    let (cx, cy) = (at.0 as i64, at.1 as i64);
    let (mut sum, mut n) = (0.0, 0.0);
    for y in (cy - radius).max(0)..=(cy + radius).min(h - 1) {
        for x in (cx - radius).max(0)..=(cx + radius).min(w - 1) {
            sum += f64::from(rgba[usize::try_from((y * w + x) * 4).unwrap_or(0)]);
            n += 1.0;
        }
    }
    sum / n
}

/// Whether something drawn stands between the eye and the lamp, as the halos' own pass decides it.
fn is_cut(h: &RemasterHaloSeen) -> bool {
    h.surface < h.distance - 0.35 * h.size - 0.15
}

/// Whether the lamps' light or a lamp's glow reaches any of the pixels within `radius` of `at` on
/// the grid the world was drawn at.
fn lit_round(lamps: &RemasterLampCapture, at: (f32, f32), radius: i64) -> bool {
    let (w, h) = (i64::from(lamps.width), i64::from(lamps.height));
    #[allow(clippy::cast_possible_truncation)] // on the grid
    let (cx, cy) = (at.0 as i64, at.1 as i64);
    ((cy - radius).max(0)..=(cy + radius).min(h - 1)).any(|y| {
        ((cx - radius).max(0)..=(cx + radius).min(w - 1)).any(|x| {
            let p = lamps.light[usize::try_from(y * w + x).unwrap_or(0)];
            p[0].max(p[1]).max(p[2]) >= 1.0 / 255.0 || p[3] < 0.0
        })
    })
}

/// How far `on` stands over `off` at each halo, its place scaled from the size the world was drawn
/// at to the frame's: the halos something stands in front of where the lamps' light and glow reach
/// none of the lamp's own pixels (so all that could move them is the halo), then the ones the eye
/// can see.
fn halos_moved(
    on: &[u8],
    off: &[u8],
    width: u32,
    lamps: &RemasterLampCapture,
) -> (Vec<f64>, Vec<f64>) {
    #[allow(clippy::cast_precision_loss)] // a window's size
    let scale = width as f32 / lamps.width as f32;
    // The frame's pixels round the lamp's, on the world's grid.
    #[allow(clippy::cast_possible_truncation)] // a few pixels
    let reach = (1.0 / scale).ceil() as i64 + 1;
    let (mut cut, mut seen) = (Vec::new(), Vec::new());
    for h in &lamps.halos {
        let at = (h.at.0 * scale, h.at.1 * scale);
        let moved = red_round(on, width, at, 1) - red_round(off, width, at, 1);
        if !is_cut(h) {
            seen.push(moved);
        } else if !lit_round(lamps, h.at, reach) {
            cut.push(moved);
        }
    }
    (cut, seen)
}

/// **A surface nearer than a lamp cuts its halo**: at [`HALO_STATION`], where the lamp stands
/// behind something its light does not reach, the frame barely moves at the lamp's own pixel, and
/// where the eye can see the lamp it moves by far more.
#[test]
fn a_surface_nearer_than_a_lamp_cuts_its_halo() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (1280, 720);
    let boxes = halo_boxes();
    let (on, lamps) = street_with(&store, HALO_STATION, &boxes, size);
    let (off, _) = street_with(
        &store,
        HALO_STATION,
        &boxes.replace("RemasterOutdoorLamps=1,", ""),
        size,
    );
    let (cut, seen) = halos_moved(&on, &off, size.0, &lamps.expect("the lamps drew"));
    eprintln!("behind something {cut:.1?}, in the open {seen:.1?}");
    assert!(!cut.is_empty(), "lamps behind something: {cut:?}");
    assert!(seen.len() >= 2, "lamps in the open: {seen:?}");
    assert!(cut.iter().all(|m| *m <= HALO_CUT), "{cut:?}");
    assert!(seen.iter().all(|m| *m >= HALO_SEEN), "{seen:?}");
}

/// How far a lamp's own pixels may move when something its light does not reach stands in front of
/// it, display levels: measured 0.3, where the halo it hides moves them by [`HALO_SEEN`] and more.
const HALO_CUT: f64 = 6.0;

/// How far at least a lamp in the open moves them, display levels: measured from 109 (through FSR).
const HALO_SEEN: f64 = 100.0;

/// **Through FSR a lamp still stands its halo, and what stands in front of it still cuts it**:
/// [`HALO_STATION`] drawn at half the pixels each way.
#[test]
fn a_lamps_halo_stands_and_is_cut_through_fsr() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (1280, 720);
    let boxes = format!("{},RemasterUpscale=1,RemasterScale=50", halo_boxes());
    let (on, lamps) = street_with(&store, HALO_STATION, &boxes, size);
    let (off, _) = street_with(
        &store,
        HALO_STATION,
        &boxes.replace("RemasterOutdoorLamps=1,", ""),
        size,
    );
    let lamps = lamps.expect("the lamps drew");
    assert_eq!(lamps.width, size.0 / 2, "drawn at half the pixels");
    let (cut, seen) = halos_moved(&on, &off, size.0, &lamps);
    eprintln!("through FSR: behind something {cut:.1?}, in the open {seen:.1?}");
    assert!(!cut.is_empty(), "lamps behind something: {cut:?}");
    assert!(seen.len() >= 2, "lamps in the open: {seen:?}");
    assert!(cut.iter().all(|m| *m <= HALO_CUT), "{cut:?}");
    assert!(seen.iter().all(|m| *m >= HALO_SEEN), "{seen:?}");
}

/// The lamps' street with `boxes`, settled, then eight frames with its clock going on a little each:
/// each frame's colors and its halos.
fn street_over_time(
    store: &Arc<RetailDatStore>,
    boxes: &str,
    size: (u32, u32),
) -> Vec<(Vec<u8>, Vec<RemasterHaloSeen>)> {
    let station = remaster_frames::stations(store, &["lamps-street"])
        .pop()
        .expect("the station");
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene.draw.cfg.render.fidelity =
        dereth_client_runtime::render_prefs::FidelityPreferences::parse_switch(boxes)
            .expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let mut frames = Vec::new();
    for _ in 0..8 {
        shot.step_by(&mut gpu, 0.09);
        let rgba = crate::instruments::stations::capture(&mut gpu);
        let halos = shot
            .scene
            .draw
            .remaster_lamp_capture(&mut gpu)
            .map_or_else(Vec::new, |l| l.halos);
        frames.push((rgba, halos));
    }
    frames
}

/// **A flame's halo flickers on the screen over time**: as the street's clock goes on, the pixels
/// round a flame in the open rise and fall, while with the lamps' row off nothing in the frame
/// moves, so it is the flames that move them. A lamp that isn't a flame holds still.
#[test]
fn a_flames_halo_flickers_on_the_screen_as_the_clock_goes_on() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (1280, 720);
    let boxes = halo_boxes();
    let frames = street_over_time(&store, &boxes, size);
    let mut flames = 0;
    let range = |v: &[f64]| {
        v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min)
    };
    for (n, h) in frames[0].1.iter().enumerate() {
        let colors: Vec<f64> = frames
            .iter()
            .filter_map(|(_, halos)| halos.get(n).map(|x| f64::from(x.color[0])))
            .collect();
        if is_cut(h) || range(&colors) < 0.05 {
            continue;
        }
        flames += 1;
        let reds: Vec<f64> = frames
            .iter()
            .map(|(rgba, _)| red_round(rgba, size.0, h.at, 6))
            .collect();
        eprintln!(
            "the flame at {:.0},{:.0} ranges {:.2}",
            h.at.0,
            h.at.1,
            range(&reds)
        );
        assert!(range(&reds) >= FLICKER, "the flame at {:?}: {reds:?}", h.at);
    }
    assert!(flames >= 2, "flames in the open: {flames}");

    let still = street_over_time(&store, &boxes.replace("RemasterOutdoorLamps=1,", ""), size);
    let moved = still
        .windows(2)
        .flat_map(|w| w[0].0.iter().zip(&w[1].0))
        .filter(|(a, b)| a.abs_diff(**b) > 3)
        .count();
    assert_eq!(moved, 0, "without the lamps the frame holds still");
}

/// How far the pixels round a flame range over the frames, display levels: measured from 11.
const FLICKER: f64 = 4.0;

/// The stations the indoor lamps are checked at, with the boxes as the options page ticks them at
/// first: a room of a town building, a house's doorway, a dungeon's corridor and a dungeon's hall
/// lit by candles.
const INDOOR_STATIONS: [&str; 4] = ["indoor", "doorway", "dungeon", "dungeon-candles"];

/// How much brighter, display levels, a part of the indoors may be on average with the indoor lamps
/// than with retail's own light: measured at most 1.8 at [`INDOOR_STATIONS`] at 960 by 540, where
/// lamps added past retail's clamp made a house's walls 22.2 brighter.
const INDOOR_OVER: f64 = 4.0;

/// **Indoors the lamps are retail's light, no brighter**: with the boxes as the options page ticks
/// them at first, at [`INDOOR_STATIONS`], every room the frame shows 2,000 pixels or more of, the
/// objects indoors, and the brightest quarter of the indoor pixels are on average no brighter with
/// the indoor lamps than with retail's own light (the indoor lamps' box off) beyond
/// [`INDOOR_OVER`].
#[test]
fn indoors_the_lamps_light_no_brighter_than_retails_own_light() {
    use crate::instruments::lamp_level_frames::{at_first, drawn, level, mean};
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (960, 540);
    let (mut checked, mut over) = (0, Vec::new());
    for name in INDOOR_STATIONS {
        let station = remaster_frames::stations(&store, &[name])
            .pop()
            .expect("the station");
        let on = drawn(&store, &station, &at_first(""), size);
        let off = drawn(&store, &station, &at_first("RemasterIndoorLamps=0"), size);
        // ORDER-OK: reported in order.
        let mut parts: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for i in 0..on.class.len() {
            if on.class[i] != off.class[i] {
                continue;
            }
            match on.class[i] {
                ROOM => parts
                    .entry(format!("room {:04x}", on.room[i]))
                    .or_default()
                    .push(i),
                INDOOR => parts.entry("objects".to_owned()).or_default().push(i),
                _ => {}
            }
        }
        let mut top: Vec<usize> = parts.values().flatten().copied().collect();
        top.sort_by(|a, b| level(&off.rgba, *b).total_cmp(&level(&off.rgba, *a)));
        top.truncate(top.len() / 4);
        parts.insert("the brightest quarter".to_owned(), top);
        for (part, px) in parts.iter().filter(|p| p.1.len() >= 2_000) {
            let (lamps, retail) = (
                mean(px, |i| level(&on.rgba, i)),
                mean(px, |i| level(&off.rgba, i)),
            );
            eprintln!(
                "{name} {part}: {} px, the lamps {lamps:.1}, retail's light {retail:.1}, {:+.1}",
                px.len(),
                lamps - retail
            );
            if lamps - retail > INDOOR_OVER {
                over.push(format!(
                    "{name} {part} ({} px): {lamps:.1} with the lamps against {retail:.1}",
                    px.len()
                ));
            }
            checked += 1;
        }
    }
    assert!(checked >= 20, "{checked} parts checked");
    assert!(over.is_empty(), "brighter with the lamps: {over:#?}");
}

/// The stations the outdoor lamps are checked at, at midnight with the boxes as the options page
/// ticks them at first: Holtburg's north street of lamp posts, its west end, and a square ringed by
/// lanterns.
const OUTDOOR_STATIONS: [&str; 3] = ["lamps-street", "lamps-west", "lanterns-square"];

/// How much brighter at least, display levels, the ground within 12 m of a lamp is on average with
/// the outdoor lamps than without: measured 5.8 to 10.6 at [`OUTDOOR_STATIONS`] at 960 by 540,
/// where pools of the square of the distance from a strength of their own lit it 2.3 to 3.8.
const OUTDOOR_RISE: f64 = 4.0;

/// How much brighter at least the ground 8 m from a lamp is with the outdoor lamps: measured 5.2 to
/// 10.1, where pools held at retail's clamp within 5 m and ended at 1.3 times the lamp's falloff lit
/// it 0.6 to 2.3.
const POOL_FAR: f64 = 4.0;

/// How much brighter a ring of the ground may be than the ring nearer the lamp, display levels:
/// where two lamps' pools meet, a pixel's nearest lamp is not all its light.
const POOL_SLACK: f64 = 1.0;

/// **At night the outdoor lamps light a broad, soft pool round them**: at [`OUTDOOR_STATIONS`],
/// the ground within 12 m of a lamp is brighter with the outdoor lamps than without by
/// [`OUTDOOR_RISE`] display levels or more on average; the pool is brightest by the lamp and fades
/// outward, each of the rings at 1, 3, 5 and 8 m from the nearest lamp brightened no more than the
/// one inside it (but for [`POOL_SLACK`]); and 8 m out the ground is still brightened by
/// [`POOL_FAR`] or more.
#[test]
fn at_night_the_outdoor_lamps_light_a_broad_soft_pool_round_them() {
    use crate::instruments::lamp_level_frames::{
        at_first, drawn, level, mean, placed_lamps, rings, RINGS,
    };
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let size = (960, 540);
    let mut short = Vec::new();
    for name in OUTDOOR_STATIONS {
        let station = remaster_frames::stations(&store, &[name])
            .pop()
            .expect("the station");
        let on = drawn(&store, &station, &at_first(""), size);
        let off = drawn(&store, &station, &at_first("RemasterOutdoorLamps=0"), size);
        let lamps = placed_lamps(&on);
        assert!(lamps.len() >= 2, "{name}: {} lamps", lamps.len());
        let rings = rings(&on);
        let rise: Vec<f64> = rings
            .iter()
            .map(|px| {
                let px: Vec<usize> = px
                    .iter()
                    .copied()
                    .filter(|&i| off.class[i] == GROUND)
                    .collect();
                assert!(px.len() >= 200, "{name}: {} pixels in a ring", px.len());
                mean(&px, |i| level(&on.rgba, i)) - mean(&px, |i| level(&off.rgba, i))
            })
            .collect();
        eprintln!(
            "{name}: {} lamps; the ground brightened {:.1} within 12 m, at {RINGS:?} m {:.1?}",
            lamps.len(),
            rise[RINGS.len()],
            &rise[..RINGS.len()]
        );
        if rise[RINGS.len()] < OUTDOOR_RISE {
            short.push(format!("{name}: within 12 m {:.1}", rise[RINGS.len()]));
        }
        for k in 1..4 {
            if rise[k] > rise[k - 1] + POOL_SLACK {
                short.push(format!(
                    "{name}: {:.1} at {} m, past {:.1} at {} m",
                    rise[k],
                    RINGS[k],
                    rise[k - 1],
                    RINGS[k - 1]
                ));
            }
        }
        if rise[3] < POOL_FAR {
            short.push(format!("{name}: {:.1} at 8 m", rise[3]));
        }
    }
    assert!(short.is_empty(), "{short:#?}");
}

/// The lanterns hung inside the house at Holtburg's north-east corner, across its block, metres.
const HOUSE_LANTERNS: [[f32; 2]; 3] = [[148.1, 133.8], [155.2, 126.7], [160.6, 132.3]];

/// **A lamp inside a house lights nothing outdoors**: at the lanterns' station at midnight, with
/// the boxes at first, none of [`HOUSE_LANTERNS`] is among the outdoor lamps, while the lamp posts
/// in reach are.
#[test]
fn a_lamp_inside_a_house_lights_nothing_outdoors() {
    use crate::instruments::lamp_level_frames::{at_first, drawn, placed_lamps};
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = remaster_frames::stations(&store, &["lamps-lantern"])
        .pop()
        .expect("the station");
    let on = drawn(&store, &station, &at_first(""), (960, 540));
    let lamps = placed_lamps(&on);
    let inside: Vec<glam::Vec3> = lamps
        .iter()
        .map(|l| l.0)
        .filter(|p| {
            HOUSE_LANTERNS
                .iter()
                .any(|h| glam::Vec2::from_array(*h).distance(p.truncate()) < 0.5)
        })
        .collect();
    eprintln!("{} outdoor lamps; inside the house {inside:?}", lamps.len());
    assert!(lamps.len() >= 4, "{} outdoor lamps", lamps.len());
    assert!(inside.is_empty(), "lamps inside the house: {inside:?}");
}
