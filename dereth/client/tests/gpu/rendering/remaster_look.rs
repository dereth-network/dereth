//! The remaster's look, laid over retail's light: the identity look draws exactly what the
//! remaster draws with no look; a tint of 0 takes the evening's purple out of the dusk, by counts
//! of the lit pixels' saturation; and Retail Look never sees a look, with one set while it draws
//! and while the remaster is turned on and off.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::RemasterCapture;
use dereth_scene::world_scene::RemasterLookLayer as Layer;

use crate::instruments::remaster_frames;
use crate::instruments::stations::{capture, counted_step, moved, Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

/// A Vulkan device asked for the remaster's features, with the validation layer where it is
/// installed.
fn device() -> Gpu {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    gpu
}

fn station(store: &RetailDatStore, name: &str) -> Station {
    remaster_frames::stations(store, &[name])
        .pop()
        .unwrap_or_else(|| panic!("the {name} station"))
}

/// The remaster's last frame at `station`, retail's light per vertex, with the player's look
/// `look` when there is one.
fn remastered(
    store: &Arc<RetailDatStore>,
    station: &Station,
    look: Option<&str>,
) -> RemasterCapture {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    if let Some(text) = look {
        shot.scene
            .draw
            .set_remaster_look(Layer::Player, Some(text), &mut gpu)
            .expect("a look");
    }
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    for _ in 0..6 {
        shot.step(&mut gpu);
    }
    let c = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    c
}

/// A color's saturation, 0 for gray and 1 for a pure hue.
fn saturation(p: &[u8]) -> f64 {
    let max = f64::from(p[0].max(p[1]).max(p[2]));
    let min = f64::from(p[0].min(p[1]).min(p[2]));
    if max <= 0.0 {
        0.0
    } else {
        (max - min) / max
    }
}

/// The lit pixels' mean saturation (every pixel but the sky's and the cleared ones), and how many
/// there are.
fn lit_saturation(c: &RemasterCapture) -> (f64, usize) {
    let (mut sum, mut n) = (0.0, 0usize);
    for (i, class) in c.class.iter().enumerate() {
        if *class == 0 || *class == 5 {
            continue;
        }
        sum += saturation(&c.color[i * 4..i * 4 + 3]);
        n += 1;
    }
    (sum / n.max(1) as f64, n)
}

/// **The identity look draws exactly what the remaster draws with no look**: a look file that
/// sets every value to what retail has, at the town by day and at dusk, changes no byte of the
/// remaster's frame.
#[test]
fn the_identity_look_draws_what_no_look_draws() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let identity = "sun = 1\nsky = 1\ntint = 1\nfog_density = 1\nnight_floor = 0\n\
                    [sunny]\ntint = 1\n[rainy]\ntint = 1\n";
    for name in ["holtburg", "dusk"] {
        let st = station(&store, name);
        let bare = remastered(&store, &st, None);
        let with = remastered(&store, &st, Some(identity));
        let changed = moved(&bare.color, &with.color);
        eprintln!("{name}: {changed} bytes differ with the identity look");
        assert_eq!(changed, 0, "{name}");
        assert_eq!(bare.class, with.class, "{name}");
    }
}

/// **A tint of 0 takes the evening's purple out of the dusk, and a tint of 1 keeps it**: at the
/// dusk station the remaster's lit pixels are as saturated with the identity look as with a tint
/// of 1, and measurably less saturated with a tint of 0.
#[test]
fn a_tint_of_zero_takes_the_purple_out_of_the_dusk() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "dusk");
    let (kept, lit) = lit_saturation(&remastered(&store, &st, Some("tint = 1\n")));
    let (bare, _) = lit_saturation(&remastered(&store, &st, None));
    let gray = remastered(&store, &st, Some("tint = 0\n"));
    let (grayed, lit_gray) = lit_saturation(&gray);
    eprintln!(
        "dusk: mean saturation {bare:.4} with no look, {kept:.4} at tint 1, {grayed:.4} at tint 0, \
         over {lit} lit pixels"
    );
    assert!(lit > 50_000, "lit pixels: {lit}");
    assert_eq!(lit, lit_gray, "the same pixels are lit");
    assert!((kept - bare).abs() < 1e-9, "a tint of 1 is retail's");
    assert!(grayed < kept - 0.03, "tint 0 {grayed:.4} against {kept:.4}");
}

/// Draw the town by day six frames, the remaster's box as `boxes` says before each and the
/// player's look `look` set before the first; the last frame's pixels, digest and counters.
fn toggled(
    store: &Arc<RetailDatStore>,
    look: Option<&str>,
    boxes: [bool; 6],
) -> (Vec<u8>, Option<u64>, String) {
    let st = station(store, "holtburg");
    let mut gpu = device();
    gpu.set_frame_digest(true);
    let mut shot = Shot::open(store, &mut gpu, &st);
    shot.poll(&mut gpu);
    if let Some(text) = look {
        shot.scene
            .draw
            .set_remaster_look(Layer::Player, Some(text), &mut gpu)
            .expect("a look");
    }
    let mut counted = String::new();
    for (k, on) in boxes.into_iter().enumerate() {
        shot.scene
            .draw
            .set_remaster(on, store, &mut gpu)
            .expect("the remaster turns");
        if k + 1 == boxes.len() {
            counted = counted_step(&mut shot, &mut gpu);
        } else {
            shot.step(&mut gpu);
        }
    }
    let pixels = capture(&mut gpu);
    let digest = gpu.last_frame_digest();
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    (pixels, digest, counted)
}

/// **Retail Look never sees a look**: with a look that changes everything it can set before the
/// first frame, Retail Look's last frame is the frame it draws with no look, byte for byte, with
/// the same submitted draws and counters, both when the remaster never draws and when it is
/// turned on and off between Retail Look's frames.
#[test]
fn retail_looks_frame_is_the_same_with_any_look() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let strong = "sun = 2.5\nsky = 0.4\ntint = 0\nfog_density = 3\nfog_color = [255, 0, 0]\n\
                  night_floor = 0.6\nexposure = 2\n";
    let (base, base_digest, base_counted) = toggled(&store, None, [false; 6]);
    // The Vulkan device digests what it submits; the Direct3D 12 one digests nothing.
    assert_eq!(
        base_digest.is_some(),
        crate::common::remaster_backend() == dereth_render::device::Backend::Vulkan,
        "the digest"
    );
    for boxes in [[false; 6], [false, true, true, false, true, false]] {
        let (pixels, digest, counted) = toggled(&store, Some(strong), boxes);
        let changed = moved(&pixels, &base);
        eprintln!("{boxes:?}: {changed} bytes differ with the look");
        assert_eq!(changed, 0, "{boxes:?}: the pixels");
        assert_eq!(digest, base_digest, "{boxes:?}: the submitted frame");
        assert_eq!(counted, base_counted, "{boxes:?}: the counters");
    }
}

/// **The player's look file is read while the remaster is on, and read again when it changes**:
/// at dusk, a file saying `tint = 0` grays the remaster's next frames, and the file rewritten to
/// `tint = 1` brings the purple back within a few polls, counted in polls, not time.
#[test]
fn the_players_look_file_is_read_again_when_it_changes() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "dusk");
    let dir = std::env::temp_dir().join(format!("dereth-look-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let file = dir.join("look.toml");
    std::fs::write(&file, "tint = 0\n").expect("written");
    let mut gpu = device();
    let mut shot = Shot::open(&store, &mut gpu, &st);
    shot.scene.draw.set_remaster_look_file(Some(file.clone()));
    // The remaster's box ticked, as the preference poll applies it.
    shot.scene.draw.cfg.render.fidelity =
        dereth_client_runtime::render_prefs::FidelityPreferences::parse_switch("Remaster=1")
            .expect("a fidelity spec");
    // A frame: the preference poll, which reads the file, then the frame drawn.
    let frame = |shot: &mut Shot, gpu: &mut Gpu| {
        shot.poll(gpu);
        shot.step(gpu);
    };
    for _ in 0..6 {
        frame(&mut shot, &mut gpu);
    }
    let looks = shot.scene.draw.remaster_looks();
    let tint = |looks: &dereth_scene::world_scene::RemasterLooks| looks.look(None, 0.5).tint;
    assert_eq!(tint(&looks), 0.0, "read on the first poll");
    let c = shot.scene.draw.remaster_capture(&mut gpu).expect("drawn");
    let (grayed, _) = lit_saturation(&c);
    std::fs::write(&file, "tint = 1\n").expect("rewritten");
    let mut polls = 0;
    while tint(&shot.scene.draw.remaster_looks()) != 1.0 {
        frame(&mut shot, &mut gpu);
        polls += 1;
        assert!(polls <= 30, "the file was not read again within 30 polls");
    }
    for _ in 0..6 {
        frame(&mut shot, &mut gpu);
    }
    let c = shot.scene.draw.remaster_capture(&mut gpu).expect("drawn");
    let (kept, _) = lit_saturation(&c);
    eprintln!("read again after {polls} polls; saturation {grayed:.4} then {kept:.4}");
    assert!(kept > grayed + 0.03, "{kept:.4} against {grayed:.4}");
    assert!(shot.scene.draw.remaster_on(), "the box kept it on");
    drop(shot);
    drop(gpu);
    std::fs::remove_dir_all(&dir).ok();
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// How far apart two colors are: the most in any channel.
fn apart(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0, f64::max)
}

/// **With every box ticked the game still looks like itself**: at noon the sky is as bright as
/// retail's and so is the ground where both draw ground, rooms are as dim as retail's, what stands
/// in them is lit as retail lights it, and a lake seen at a low angle keeps its blue. The limits are
/// what is drawn now, a few levels on.
#[test]
fn with_every_box_ticked_the_sky_the_ground_the_rooms_and_the_water_stay_near_retail() {
    use crate::instruments::remaster_look_frames::{beside_retail, PARTS};
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let part = |name| PARTS.iter().position(|p| *p == name).expect("a part");
    for (place, what, limit) in [
        ("holtburg", "sky", 12.0),
        ("holtburg", "ground", 8.0),
        ("vista", "ground", 11.0),
        ("dungeon", "rooms", 9.0),
        ("dungeon", "indoor", 9.0),
        ("indoor", "rooms", 8.0),
    ] {
        let (retail, drawn) = beside_retail(&store, place);
        let (r, d) = (retail[part(what)].levels(), drawn[part(what)].levels());
        eprintln!("{place} {what}: retail {r:.0?}, drawn {d:.0?}");
        assert!(
            apart(r, d) <= limit,
            "{place} {what}: {r:.0?} against {d:.0?}"
        );
    }
    let (_, lake) = beside_retail(&store, "lake-noon");
    let water = lake[part("water")].levels();
    eprintln!("the lake's water: {water:.0?}");
    assert!(water[2] > water[0] + 15.0, "blue, not gray: {water:.0?}");
}
