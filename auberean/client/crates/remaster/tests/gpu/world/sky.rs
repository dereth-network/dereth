//! Behaviour: none (experimental)
//!
//! The physical sky, the shared passes' scattering atmosphere over the day's dome and the veil of
//! haze over the horizon. Fixture: the stations outdoors, by day and at midnight, the camera
//! raised to look [`UP`] degrees over the level toward the sky.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames without and
//! with the physical sky are written to.

use auberean_remaster::render::SkySettings;
use auberean_remaster::Remaster;

use crate::common::{frame, records, settled, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;
/// How far over the level the camera looks, degrees.
const UP: f32 = 20.0;

/// A station's frame without the physical sky and with it, settled; the light class at each pixel
/// (0 where the sky shows); the frame drawn without it again after; and the device's noise.
struct Drawn {
    off: Vec<u8>,
    on: Vec<u8>,
    classes: Vec<u8>,
    off_again: Vec<u8>,
    noise: usize,
}

fn drawn(name: &str) -> Drawn {
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut remaster = Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    let s = *STATIONS
        .iter()
        .find(|s| s.name == name)
        .expect("the station");
    let mut f = frame(&s);
    f.camera.pitch = UP;
    let mut draw = |remaster: &mut Remaster| {
        remaster
            .render(&f, W, H, &mut load)
            .expect("the frame")
            .rgba
    };
    let (off, off_noise) = settled(|| draw(&mut remaster));
    remaster.set_sky(SkySettings { atmosphere: true });
    let (on, on_noise) = settled(|| draw(&mut remaster));
    let classes = remaster
        .read_classes()
        .expect("the classes")
        .expect("a frame was drawn");
    remaster.set_sky(SkySettings::default());
    let (off_again, again_noise) = settled(|| draw(&mut remaster));
    if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        write_png(&dir.join(format!("{name}-sky-off.png")), W, H, &off);
        write_png(&dir.join(format!("{name}-sky-on.png")), W, H, &on);
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    Drawn {
        off,
        on,
        classes,
        off_again,
        noise: off_noise.max(on_noise).max(again_noise),
    }
}

/// Of the pixels `of` picks, how many there are and how many moved from `a` to `b` by more than
/// `by` in some channel.
fn moved(a: &[u8], b: &[u8], classes: &[u8], by: u8, of: impl Fn(u8) -> bool) -> (usize, usize) {
    let (mut n, mut m) = (0, 0);
    for ((x, y), c) in a.chunks(4).zip(b.chunks(4)).zip(classes) {
        if of(*c) {
            n += 1;
            if (0..3).any(|k| x[k].abs_diff(y[k]) > by) {
                m += 1;
            }
        }
    }
    (n, m)
}

/// **By day the physical sky lies over the dome and leaves the land as it was**: at the daytime
/// stations outdoors, the camera raised toward the sky, with the physical sky on, at least a
/// quarter of the sky's pixels (where the depth's pass found no surface; the day's clouds cover
/// much of the rest) move by more than eight levels in a channel, and under one in twenty of the
/// others (the land and what stands on it, where only an edge blended over the sky, or the water
/// that reflects it, can move) by more than one level; turned off again, it draws the frame it
/// drew before.
#[test]
fn by_day_the_physical_sky_lies_over_the_dome_and_leaves_the_land_as_it_was() {
    let _turn = turn();
    for name in [
        "town-6fa9",
        "town-76b8",
        "camp-c156",
        "open-cc8b",
        "wide-6fa9",
    ] {
        let d = drawn(name);
        let (sky, sky_moved) = moved(&d.off, &d.on, &d.classes, 8, |c| c == 0);
        let (land, land_moved) = moved(&d.off, &d.on, &d.classes, 1, |c| c != 0);
        eprintln!(
            "{name}: sky {sky_moved}/{sky} moved by over 8; land {land_moved}/{land} by over 1 \
             (noise {})",
            d.noise
        );
        assert!(
            sky * 4 >= sky + land,
            "{name}: too little sky in view: {sky}"
        );
        assert!(
            sky_moved * 4 >= sky,
            "{name}: the physical sky moved {sky_moved} of {sky} sky pixels"
        );
        assert!(
            land_moved * 20 < land + d.noise * 20,
            "{name}: {land_moved} of {land} land pixels moved"
        );
        assert!(
            crate::common::repeat_noise(&d.off, &d.off_again) <= d.noise,
            "{name}: turned off again, the frame isn't the one drawn before"
        );
    }
}

/// **At night the dome's dark sky stays**: at the town at midnight the physical sky covers none of
/// the dome, and only the veil low over the horizon moves the sky: under one in ten of the sky's
/// pixels moves by more than eight levels, and none of the land's by more than one but at blended
/// edges.
#[test]
fn at_night_the_domes_dark_sky_stays() {
    let _turn = turn();
    let d = drawn("night-6fa9");
    let (sky, sky_moved) = moved(&d.off, &d.on, &d.classes, 8, |c| c == 0);
    let (land, land_moved) = moved(&d.off, &d.on, &d.classes, 1, |c| c != 0);
    eprintln!(
        "night-6fa9: sky {sky_moved}/{sky} moved by over 8; land {land_moved}/{land} by over 1 \
         (noise {})",
        d.noise
    );
    assert!(sky * 4 >= sky + land, "too little sky in view: {sky}");
    assert!(
        sky_moved * 10 < sky + d.noise * 10,
        "the night's sky moved: {sky_moved} of {sky} pixels"
    );
    assert!(
        land_moved * 100 < land + d.noise * 100,
        "{land_moved} of {land} land pixels moved"
    );
}
