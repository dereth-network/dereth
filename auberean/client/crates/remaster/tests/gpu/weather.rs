//! Behaviour: none (experimental)
//!
//! The rain on the world: what stands out in it soaked, and the overhead map of what stands over
//! each point keeping it off what stands under a roof. Fixture: the stations outdoors by day, in
//! the heaviest rain (their days are dry, so the frame's rain amount is set to 1).
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames dry, wet and
//! wet with every point open are written to.

use auberean_remaster::render::WeatherSettings;
use auberean_remaster::Remaster;

use crate::common::{frame, records, settled, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// A station's frame in the rain: dry (the wet off), wet, and wet with the shelter off (every
/// point open to the sky), settled; the light class at each pixel; and the device's noise.
struct Drawn {
    dry: Vec<u8>,
    wet: Vec<u8>,
    open: Vec<u8>,
    classes: Vec<u8>,
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
    f.rain = 1.0;
    let mut draw = |remaster: &mut Remaster| {
        remaster
            .render(&f, W, H, &mut load)
            .expect("the frame")
            .rgba
    };
    let (dry, dry_noise) = settled(|| draw(&mut remaster));
    remaster.set_weather(WeatherSettings {
        wet: true,
        shelter: true,
    });
    let (wet, wet_noise) = settled(|| draw(&mut remaster));
    let classes = remaster
        .read_classes()
        .expect("the classes")
        .expect("a frame was drawn");
    remaster.set_weather(WeatherSettings {
        wet: true,
        shelter: false,
    });
    let (open, open_noise) = settled(|| draw(&mut remaster));
    if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        write_png(&dir.join(format!("{name}-dry.png")), W, H, &dry);
        write_png(&dir.join(format!("{name}-wet.png")), W, H, &wet);
        write_png(&dir.join(format!("{name}-open.png")), W, H, &open);
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    Drawn {
        dry,
        wet,
        open,
        classes,
        noise: dry_noise.max(wet_noise).max(open_noise),
    }
}

/// Of the pixels whose class `of` picks: how many there are, and how many of them are brighter
/// and darker in `b` than in `a` by more than `by` in some channel.
fn moved(
    a: &[u8],
    b: &[u8],
    classes: &[u8],
    by: u8,
    of: impl Fn(u8) -> bool,
) -> (usize, usize, usize) {
    let (mut n, mut up, mut down) = (0, 0, 0);
    for ((x, y), c) in a.chunks(4).zip(b.chunks(4)).zip(classes) {
        if !of(*c) {
            continue;
        }
        n += 1;
        if (0..3).any(|k| y[k] > x[k].saturating_add(by)) {
            up += 1;
        }
        if (0..3).any(|k| x[k] > y[k].saturating_add(by)) {
            down += 1;
        }
    }
    (n, up, down)
}

/// The land's class and the class of what stands out in the sun.
const GROUND: u8 = 1;
const SUN: u8 = 2;

/// **In the rain what stands out in it is wet, and the sky is untouched**: at the daytime
/// stations in the heaviest rain, with the wet on, at least one in ten of the pixels of the land
/// and of what stands out in the sun darkens by more than four levels in a channel, and under one
/// in a hundred brightens by more than eight (the sheen of the overcast air seen edge on); the
/// sky's pixels don't move.
#[test]
fn in_the_rain_what_stands_out_in_it_is_wet_and_the_sky_is_untouched() {
    let _turn = turn();
    for name in ["town-6fa9", "town-76b8", "camp-c156", "open-cc8b"] {
        let d = drawn(name);
        let (n, up, down) = moved(&d.dry, &d.wet, &d.classes, 4, |c| c == GROUND || c == SUN);
        let (_, bright, _) = moved(&d.dry, &d.wet, &d.classes, 8, |c| c == GROUND || c == SUN);
        let (sky, sky_up, sky_down) = moved(&d.dry, &d.wet, &d.classes, 0, |c| c == 0);
        eprintln!(
            "{name}: of {n} outdoor pixels {down} darker and {up} brighter by over 4, {bright} \
             brighter by over 8; sky {sky_up} up, {sky_down} down of {sky} (noise {})",
            d.noise
        );
        assert!(down * 10 >= n, "{name}: the rain wet {down} of {n} pixels");
        assert!(
            bright * 100 < n + d.noise * 100,
            "{name}: the rain brightened {bright} of {n} pixels"
        );
        assert!(
            sky_up + sky_down <= d.noise,
            "{name}: the rain moved the sky: {sky_up} up, {sky_down} down"
        );
    }
}

/// **What stands under a roof stays dry**: at the daytime stations in the heaviest rain, the
/// frame with the overhead map is nowhere darker than the frame with every point open by more
/// than two levels (but where the device's noise allows), and at the stations that see under a
/// roof some pixels are brighter: kept dry.
#[test]
fn what_stands_under_a_roof_stays_dry() {
    let _turn = turn();
    let mut kept = 0;
    for name in [
        "town-6fa9",
        "town-76b8",
        "camp-c156",
        "open-cc8b",
        "wide-6fa9",
    ] {
        let d = drawn(name);
        let (n, up, down) = moved(&d.open, &d.wet, &d.classes, 2, |c| c == GROUND || c == SUN);
        eprintln!(
            "{name}: with the shelter, of {n} outdoor pixels {up} drier and {down} wetter by over \
             2 (noise {})",
            d.noise
        );
        assert!(
            down <= d.noise,
            "{name}: the shelter wet {down} pixels the open frame left dry"
        );
        kept += up;
    }
    assert!(kept >= 100, "the shelter kept only {kept} pixels dry");
}
