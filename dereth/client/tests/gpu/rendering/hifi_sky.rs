//! The Remastered sky Better Lighting draws with an art pack: retail's clouds, sun, moons and
//! stars drawn again. It is drawn only with the pack, never with Retail Look; nothing in it moves
//! under a still camera while the game's clock is held, the anti-aliasing's jitter included; the
//! clouds drift with the clock alone and cover as much of the sky as retail's own picture does for
//! the day; the stars are points that twinkle only as the clock runs; the moons stand where retail
//! places them; and the clouds shade the land under them.
//!
//! The pack is made in the test: a flat ground of its own color for each terrain type, so the sky
//! is what changes.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device off screen.
//!
//! Behaviour: none (experimental hi-fi art packs)

#![cfg(all(gpu, feature = "hifi"))]

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{AdapterKind, Backend, DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use crate::instruments::hifi_stations::{capture, write_png, Clock, Place, Shot, Station};

/// The frame's size.
const SIZE: (u32, u32) = (960, 540);

/// The boxes every frame is drawn with: the light and the sky, without the anti-aliasing, so a
/// frame that does not change draws the same pixels again.
const BOXES: &str = "Lighting=1,Shadows=1,Sky=1,NoAntiAliasing=1";

/// The same with the anti-aliasing, which draws each frame a fraction of a pixel off the last.
const JITTERED: &str = "Lighting=1,Shadows=1,Sky=1";

/// A four-texel square PNG of one color.
fn flat_png(rgba: [u8; 4]) -> Vec<u8> {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("header");
        w.write_image_data(&rgba.repeat(16)).expect("data");
    }
    png
}

/// The pack: a flat ground of its own color for each terrain type.
fn ground_pack() -> Arc<ArtPack> {
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut manifest = String::from("[pack]\nname = \"Grounds\"\nformat = 1\n");
    let credit = "author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
    for t in 0..32u8 {
        let name = format!("g{t}.png");
        let h = u32::from(t).wrapping_mul(0x9E37_79B9).rotate_left(7);
        let [a, b, c, _] = h.to_le_bytes();
        files.insert(
            name.clone(),
            flat_png([40 + a / 2, 60 + b / 2, 30 + c / 3, 255]),
        );
        manifest.push_str(&format!(
            "[[terrain]]\nground = {t}\nbase_color = \"{name}\"\nsize = 4\n{credit}"
        ));
    }
    Arc::new(
        ArtPack::open(&manifest, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens"),
    )
}

/// Sixty meters over Holtburg, where nothing stands in the sky, looking `yaw` radians west of
/// north and `pitch` up, at `clock`.
fn holtburg(name: &'static str, yaw: f32, pitch: f32, clock: Clock) -> Station {
    Station {
        name,
        why: "the sky over Holtburg",
        place: Place::Free {
            block: 0xA9B3,
            x: 84.0,
            y: 139.0,
            eye: 60.0,
            yaw,
            pitch,
        },
        clock,
        weather: false,
        land_radius: 2,
    }
}

/// Sixty meters over Holtburg, north over the river and the land beyond it, at noon on the first
/// clear day: the clouds' shadows on the land.
fn over_holtburg() -> Station {
    Station {
        name: "over-holtburg",
        why: "the land under the clouds",
        place: Place::Free {
            block: 0xA9B3,
            x: 84.0,
            y: 139.0,
            eye: 60.0,
            yaw: 0.0,
            pitch: -0.3,
        },
        clock: Clock::Group("Clear", 0.5),
        weather: false,
        land_radius: 2,
    }
}

/// `station` opened on a device of its own with `pack` and `boxes`, the ground the pack lays
/// drawn and settled, the sky's knobs set by `knobs` (the tuning file's form).
fn opened(station: &Station, pack: &Arc<ArtPack>, boxes: &str, knobs: &str) -> (Gpu, Shot) {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: SIZE.0,
        height: SIZE.1,
        hifi: true,
        ..DeviceConfig::default()
    };
    let mut device = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("this test needs a wgpu device and none opened: {e}"));
    assert_eq!(
        device.adapter_kind(),
        AdapterKind::Hardware,
        "{} is a software rasterizer",
        device.adapter_name()
    );
    let gpu = &mut device;
    let mut shot = Shot::open(&store, gpu, station);
    shot.scene
        .set_art_pack(&store, gpu, Some(Arc::clone(pack)))
        .expect("the blocks bake for the pack");
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes parse");
    set_knobs(&mut shot, knobs);
    shot.poll(gpu);
    shot.step(gpu);
    shot.scene.wait_for_drawn_ground();
    shot.poll(gpu);
    for _ in 0..12 {
        shot.step_held(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
    shot.step_held(gpu);
    assert!(gpu.hifi_failed().is_none(), "{:?}", gpu.hifi_failed());
    (device, shot)
}

/// Set the knobs `text` names (the tuning file's form, a line each), the rest at their defaults.
fn set_knobs(shot: &mut Shot, text: &str) {
    let (knobs, notes) = dereth_terrain::land::drawn::Knobs::parse(text);
    assert!(notes.is_empty(), "{notes:?}");
    shot.scene.set_terrain_knobs(knobs);
}

/// Step `shot` `n` frames with the clock held, and capture the last.
fn held(shot: &mut Shot, gpu: &mut Gpu, n: usize) -> Vec<u8> {
    for _ in 0..n {
        shot.step_held(gpu);
    }
    capture(gpu)
}

/// Draw `shot` with `boxes` from now on, once the presentation's pipelines for them are built
/// and its frames settled, the clock held.
fn switch_boxes(shot: &mut Shot, gpu: &mut Gpu, boxes: &str) {
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes parse");
    shot.poll(gpu);
    for _ in 0..12 {
        shot.step_held(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
}

/// Run `shot`'s clock on `steps` frames.
fn run_clock(shot: &mut Shot, gpu: &mut Gpu, steps: usize) {
    for _ in 0..steps {
        shot.step(gpu);
    }
}

/// Where the frames go, in cargo's scratch folder, to look at when a test fails.
fn keep(name: &str, rgba: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hifi-sky");
    std::fs::create_dir_all(&dir).expect("the scratch folder");
    write_png(&dir.join(format!("{name}.png")), SIZE.0, SIZE.1, rgba);
}

/// The presentation's note `name` for the last frame.
fn note(gpu: &Gpu, name: &str) -> u64 {
    gpu.hifi_report()
        .and_then(|r| r.notes.iter().find(|(n, _)| *n == name).map(|(_, v)| *v))
        .unwrap_or(0)
}

/// The pixels in rows `rows` whose color differs by more than `levels` in any channel between
/// `a` and `b`.
fn changed_in(a: &[u8], b: &[u8], rows: std::ops::Range<usize>, levels: u8) -> usize {
    let row = SIZE.0 as usize * 4;
    a[rows.start * row..rows.end * row]
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b[rows.start * row..rows.end * row].as_chunks::<4>().0)
        .filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > levels))
        .count()
}

/// The frame's upper three fifths: the sky, with the frame looking up.
fn sky_rows() -> std::ops::Range<usize> {
    0..SIZE.1 as usize * 3 / 5
}

#[test]
fn the_remastered_sky_is_drawn_with_an_art_pack_and_never_with_retail_look() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    let station = holtburg("west", FRAC_PI_2, 0.45, Clock::Sunny(0.5));
    let (mut device, mut shot) = opened(&station, &pack, BOXES, "");
    let gpu = &mut device;
    let remastered = held(&mut shot, gpu, 2);
    assert_eq!(note(gpu, "sky remastered"), 1, "drawn with the pack");
    assert_eq!(note(gpu, "sky clouds drawn"), 1, "its clouds too");
    keep("drawn-remastered", &remastered);
    // Its knob off: retail's own sky is drawn again.
    set_knobs(&mut shot, "sky = false");
    let off = held(&mut shot, gpu, 2);
    assert_eq!(
        note(gpu, "sky remastered"),
        0,
        "not drawn with its knob off"
    );
    keep("drawn-off", &off);
    let differ = changed_in(&remastered, &off, sky_rows(), 16);
    assert!(
        differ > 20_000,
        "the Remastered sky changed only {differ} pixels of the sky"
    );
    // Retail Look: the same frames, stepped alike from the start, with the sky's knob on and
    // off, pixel for pixel. (Retail's own cloud deck drifts frame by frame whatever the clock, so
    // only frames stepped alike are the same frames.)
    drop(shot);
    drop(device);
    let look = format!("{BOXES},RetailLook=1");
    let mut frames = Vec::new();
    for knobs in ["", "sky = false"] {
        let (mut device, mut shot) = opened(&station, &pack, &look, knobs);
        let gpu = &mut device;
        frames.push(held(&mut shot, gpu, 4));
        assert_eq!(note(gpu, "sky remastered"), 0, "never with Retail Look");
    }
    keep("retail-look", &frames[0]);
    let differ = changed_in(&frames[0], &frames[1], 0..SIZE.1 as usize, 0);
    assert_eq!(
        differ, 0,
        "Retail Look differs in {differ} pixels with the sky's knob on"
    );
}

#[test]
fn the_sky_holds_still_frame_after_frame_under_a_still_camera_with_the_clock_held() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    // At midnight under a clear sky with the stars at their brightest, any star that flickered
    // would show.
    for (name, clock, knobs) in [
        ("day", Clock::Sunny(0.5), ""),
        ("dusk", Clock::SunUp(5.0), ""),
        ("night", Clock::Sunny(0.1), ""),
        ("midnight", Clock::Sunny(0.0), "cloud_cover = 0\nstars = 4"),
    ] {
        let station = holtburg(name, FRAC_PI_2, 0.45, clock);
        let (mut device, mut shot) = opened(&station, &pack, JITTERED, knobs);
        let gpu = &mut device;
        // Settled: the anti-aliasing's history holds its blend of the jitter.
        let mut frames = vec![held(&mut shot, gpu, 24)];
        assert_eq!(note(gpu, "sky remastered"), 1, "{name}: drawn");
        for _ in 0..8 {
            frames.push(held(&mut shot, gpu, 1));
        }
        let mut most = 0;
        for pair in frames.windows(2) {
            most = most.max(changed_in(&pair[0], &pair[1], sky_rows(), 8));
        }
        keep(&format!("still-{name}"), &frames[0]);
        eprintln!("{name}: at most {most} pixels of the sky change by over 8 levels a frame");
        assert!(
            most <= 20,
            "{name}: {most} pixels of the sky change from one frame to the next"
        );
    }
}

#[test]
fn the_clouds_drift_with_the_clock_and_their_speed_and_nothing_else() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    let station = holtburg("drift", 0.3, 0.9, Clock::Sunny(0.5));
    let (mut device, mut shot) = opened(&station, &pack, BOXES, "");
    let gpu = &mut device;
    // The clock some way on, so the drift is not nought at any speed.
    run_clock(&mut shot, gpu, 90);
    let first = held(&mut shot, gpu, 2);
    let again = held(&mut shot, gpu, 4);
    assert_eq!(
        changed_in(&first, &again, sky_rows(), 0),
        0,
        "the clock held, the clouds hold"
    );
    // At the same held clock, clouds drifting twice as fast lie elsewhere.
    set_knobs(&mut shot, "cloud_speed = 2");
    let faster = held(&mut shot, gpu, 2);
    let moved = changed_in(&first, &faster, sky_rows(), 8);
    keep("drift-first", &first);
    keep("drift-faster", &faster);
    assert!(
        moved > 2_000,
        "only {moved} pixels of the sky moved with the clouds' speed doubled"
    );
    // Held still by their knob, the clock running moves nothing in them.
    set_knobs(&mut shot, "cloud_speed = 0\nstar_twinkle = 0");
    let stopped = held(&mut shot, gpu, 2);
    run_clock(&mut shot, gpu, 30);
    let later = held(&mut shot, gpu, 1);
    let drifted = changed_in(&stopped, &later, sky_rows(), 8);
    assert!(
        drifted <= 20,
        "{drifted} pixels of the held clouds changed as the clock ran"
    );
    // Their own speed again, and the clock run on a second: they drift.
    set_knobs(&mut shot, "");
    let before = held(&mut shot, gpu, 2);
    run_clock(&mut shot, gpu, 30);
    let after = held(&mut shot, gpu, 1);
    let drifted = changed_in(&before, &after, sky_rows(), 8);
    keep("drift-after", &after);
    assert!(
        drifted > 2_000,
        "only {drifted} pixels moved as the clock ran a second"
    );
}

/// The share of the sky's pixels in `rgba` that are cloud: pale and gray, each channel bright and
/// none far from the others.
fn cloud_share(rgba: &[u8]) -> f64 {
    let rows = sky_rows();
    let row = SIZE.0 as usize * 4;
    let px = rgba[rows.start * row..rows.end * row].as_chunks::<4>().0;
    let cloud = px
        .iter()
        .filter(|p| {
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            r.min(g).min(b) > 150 && r.max(g).max(b) - r.min(g).min(b) < 40
        })
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = cloud as f64 / px.len() as f64;
    share
}

#[test]
fn the_clouds_cover_as_much_of_the_sky_as_retails_picture_does_for_the_day() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    let mut shares = Vec::new();
    // Retail's first clear day's cloud picture covers a tenth of its sky; its first sunny day's,
    // three quarters.
    for (name, clock) in [
        ("clear", Clock::Group("Clear", 0.5)),
        ("sunny", Clock::Sunny(0.5)),
    ] {
        let station = holtburg(name, 0.0, 0.7, clock);
        let (mut device, mut shot) = opened(&station, &pack, BOXES, "");
        let gpu = &mut device;
        let mut share = 0.0;
        for k in 0..4u8 {
            shot.scene.camera.yaw = f32::from(k) * FRAC_PI_2;
            let frame = held(&mut shot, gpu, 2);
            keep(&format!("cover-{name}-{k}"), &frame);
            share += cloud_share(&frame) / 4.0;
        }
        eprintln!("{name}: clouds over {:.0}% of the sky", share * 100.0);
        shares.push(share);
    }
    let (clear, sunny) = (shares[0], shares[1]);
    assert!(
        clear < 0.35,
        "a clear day's clouds cover {clear:.2} of its sky"
    );
    assert!(
        sunny > 0.4,
        "a sunny day's clouds cover {sunny:.2} of its sky"
    );
    assert!(
        clear < sunny * 0.6,
        "clear {clear:.2} against sunny {sunny:.2}"
    );
}

/// The groups of 8-connected pixels in rows `rows` that differ by more than `levels` between `a`
/// and `b`: each group's size.
fn spots(a: &[u8], b: &[u8], rows: std::ops::Range<usize>, levels: u8) -> Vec<usize> {
    let (w, h) = (SIZE.0 as usize, rows.end);
    let px = |x: usize, y: usize| {
        let i = (y * w + x) * 4;
        (0..3).any(|c| a[i + c].abs_diff(b[i + c]) > levels)
    };
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    for y in rows.clone() {
        for x in 0..w {
            if seen[y * w + x] || !px(x, y) {
                continue;
            }
            let mut stack = vec![(x, y)];
            seen[y * w + x] = true;
            let mut n = 0;
            while let Some((cx, cy)) = stack.pop() {
                n += 1;
                for dy in -1isize..=1 {
                    for dx in -1isize..=1 {
                        let (Some(nx), Some(ny)) =
                            (cx.checked_add_signed(dx), cy.checked_add_signed(dy))
                        else {
                            continue;
                        };
                        if ny < rows.start || nx >= w || ny >= h {
                            continue;
                        }
                        if !seen[ny * w + nx] && px(nx, ny) {
                            seen[ny * w + nx] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            out.push(n);
        }
    }
    out
}

#[test]
fn the_stars_are_points_that_twinkle_only_as_the_clock_runs() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    // Before dawn, looking well up away from the moons, the clouds out of the way.
    let station = holtburg("stars", 2.5, 0.9, Clock::Sunny(0.1));
    let (mut device, mut shot) = opened(&station, &pack, BOXES, "cloud_cover = 0");
    let gpu = &mut device;
    let starry = held(&mut shot, gpu, 2);
    set_knobs(&mut shot, "cloud_cover = 0\nstars = 0");
    let dark = held(&mut shot, gpu, 2);
    keep("stars", &starry);
    let mut found = spots(&starry, &dark, sky_rows(), 6);
    found.sort_unstable();
    let biggest = found.last().copied().unwrap_or(0);
    let middle = found.get(found.len() / 2).copied().unwrap_or(0);
    eprintln!(
        "{} stars, the middle one {middle} pixels, the largest {biggest}",
        found.len()
    );
    let most = found.get(found.len() * 19 / 20).copied().unwrap_or(0);
    assert!(found.len() > 50, "only {} stars", found.len());
    assert!(middle <= 6, "the middle star covers {middle} pixels");
    assert!(
        most <= 12,
        "one star in twenty covers more than {most} pixels"
    );
    assert!(biggest <= 60, "a star covers {biggest} pixels");
    // Held, they hold; the clock running, they twinkle.
    set_knobs(&mut shot, "cloud_cover = 0\nstars = 3");
    let first = held(&mut shot, gpu, 2);
    let again = held(&mut shot, gpu, 3);
    assert_eq!(
        changed_in(&first, &again, sky_rows(), 0),
        0,
        "the clock held, the stars hold"
    );
    run_clock(&mut shot, gpu, 15);
    let later = held(&mut shot, gpu, 1);
    let twinkled = changed_in(&first, &later, sky_rows(), 4);
    assert!(
        twinkled > 40,
        "only {twinkled} pixels of the stars changed as the clock ran"
    );
}

/// The box round the bright pixels whose red is over their blue and well over their green,
/// `[x0, y0, x1, y1]`: retail's orange moon against the night.
fn orange_box(rgba: &[u8]) -> Option<[usize; 4]> {
    let w = SIZE.0 as usize;
    let mut b = [usize::MAX, usize::MAX, 0, 0];
    let mut n = 0;
    for y in sky_rows() {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (r, g, bl) = (
                i32::from(rgba[i]),
                i32::from(rgba[i + 1]),
                i32::from(rgba[i + 2]),
            );
            if r > 140 && r > bl + 10 && r > g + 25 {
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                n += 1;
            }
        }
    }
    (n > 20).then_some(b)
}

#[test]
fn the_moons_stand_where_retail_places_them() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    // Before dawn, toward the orange moon, the clouds and the stars out of the way: a warm star
    // tinted by the night's purple reads as orange too.
    let station = holtburg("moon", -0.54, 0.61, Clock::Sunny(0.1));
    let (mut device, mut shot) = opened(&station, &pack, BOXES, "cloud_cover = 0\nstars = 0");
    let gpu = &mut device;
    let remastered = held(&mut shot, gpu, 2);
    assert_eq!(
        note(gpu, "sky remastered models"),
        4,
        "two moons among them"
    );
    switch_boxes(&mut shot, gpu, &format!("{BOXES},RetailLook=1"));
    let retail = held(&mut shot, gpu, 3);
    keep("moon-remastered", &remastered);
    keep("moon-retail", &retail);
    let ours = orange_box(&remastered).expect("the moon drawn again");
    let theirs = orange_box(&retail).expect("retail's moon");
    eprintln!("the moon over {ours:?}, retail's over {theirs:?}");
    // Retail's clouds hide part of its moon, and only the moon's lit face is orange: the two
    // stand within a quarter of the disk of each other, and are about as wide.
    #[allow(clippy::cast_precision_loss)] // pixel coordinates
    let middle = |b: [usize; 4]| ((b[0] + b[2]) as f64 / 2.0, (b[1] + b[3]) as f64 / 2.0);
    let (a, b) = (middle(ours), middle(theirs));
    let off = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let width = (theirs[2] - theirs[0]) as f64;
    eprintln!("{off:.1} pixels apart, retail's {width} wide");
    assert!(
        off < width * 0.3,
        "the moon stands {off:.1} pixels from retail's"
    );
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let ours_wide = (ours[2] - ours[0]) as f64;
    assert!(
        (ours_wide / width - 1.0).abs() < 0.35,
        "the moon is {ours_wide} wide, retail's {width}"
    );
}

#[test]
fn the_clouds_shade_the_land_under_them_in_patches() {
    let _gpu = crate::common::gpu_lock();
    let pack = ground_pack();
    let (mut device, mut shot) = opened(&over_holtburg(), &pack, BOXES, "cloud_shadows = 1");
    let gpu = &mut device;
    let shaded = held(&mut shot, gpu, 2);
    assert_eq!(
        note(gpu, "sky cloud shadows drawn"),
        1,
        "the shadows are drawn"
    );
    set_knobs(&mut shot, "cloud_shadows = 0");
    let open = held(&mut shot, gpu, 2);
    assert_eq!(
        note(gpu, "sky cloud shadows drawn"),
        0,
        "none with their knob at 0"
    );
    keep("shade-shaded", &shaded);
    keep("shade-open", &open);
    // The land below the horizon, against the same land unshaded: the frame's exposure may move
    // all of it alike, so each pixel is weighed against the middle of the whole.
    let land = SIZE.1 as usize / 3..SIZE.1 as usize;
    let row = SIZE.0 as usize * 4;
    let ratios: Vec<f64> = shaded[land.start * row..]
        .as_chunks::<4>()
        .0
        .iter()
        .zip(open[land.start * row..].as_chunks::<4>().0)
        .map(|(p, q)| {
            let sum = |c: &[u8; 4]| f64::from(c[0]) + f64::from(c[1]) + f64::from(c[2]) + 3.0;
            sum(p) / sum(q)
        })
        .collect();
    let mut sorted = ratios.clone();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted[sorted.len() / 2];
    let darker = ratios.iter().filter(|r| **r < middle * 0.8).count();
    let same = ratios
        .iter()
        .filter(|r| (**r / middle - 1.0).abs() < 0.05)
        .count();
    let all = ratios.len();
    eprintln!("of {all} pixels of land, {darker} shaded under the clouds and {same} as the rest");
    assert!(darker > all / 50, "only {darker} pixels of land shaded");
    assert!(
        same > all / 10,
        "the shade falls everywhere: {same} as the rest"
    );
}
