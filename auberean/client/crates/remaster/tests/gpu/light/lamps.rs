//! Behaviour: none (experimental)
//!
//! The lamps, the shared passes' lists and light over the frame's point lights. Fixture: the town's
//! stations with a warm light carried at the look point, 1.5 meters up, lighting every object
//! outdoors as a carried light does (its light falls off to nothing within its range).
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames without and
//! with the lamps are written to.

use auberean_draw::scene::{Frame, PointLights};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::DeviceLight;
use auberean_world::math::Mat4;

use crate::common::{frame, records, settled, turn, write_png, Station, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The carried light's range, meters, and its color.
const RANGE: f32 = 12.0;
const WARM: Color = Color {
    r: 1.0,
    g: 0.7,
    b: 0.4,
    a: 1.0,
};

/// Station `name`'s frame with the carried light at its look point.
fn lit_frame(name: &str) -> (Station, Frame) {
    let s = *STATIONS
        .iter()
        .find(|s| s.name == name)
        .expect("the station");
    let mut f = frame(&s);
    let at = Vec3::new(s.at[0], s.at[1], s.at[2] + 1.5);
    let light = DeviceLight {
        origin: at,
        diffuse: WARM,
        specular: Color::BLACK,
        range: RANGE,
        flicker: None,
    };
    for b in &mut f.objects {
        if !b.points.inside && b.points.count == 0 {
            b.points = PointLights::outdoors(&[light], &Mat4::IDENTITY);
        }
    }
    (s, f)
}

/// Station `name`'s frame with the carried light, without the lamps and with them, settled; the
/// lamps and halos the frame with them had; and the device's noise.
fn with_and_without(name: &str) -> (Vec<u8>, Vec<u8>, (usize, usize), usize) {
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut remaster = auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    let (s, f) = lit_frame(name);
    let mut draw = |remaster: &mut auberean_remaster::Remaster| {
        remaster
            .render(&f, W, H, &mut load)
            .expect("the frame")
            .rgba
    };
    remaster.set_lamps(false);
    let (off, off_noise) = settled(|| draw(&mut remaster));
    remaster.set_lamps(true);
    let (on, on_noise) = settled(|| draw(&mut remaster));
    let lit = remaster.last_lamps();
    if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        write_png(&dir.join(format!("{}-lamps-off.png", s.name)), W, H, &off);
        write_png(&dir.join(format!("{}-lamps-on.png", s.name)), W, H, &on);
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    (off, on, lit, off_noise.max(on_noise))
}

/// Pixels of `b` brighter than in `a` by more than `by` in some channel, and darker.
fn moved(a: &[u8], b: &[u8], by: u8) -> (usize, usize) {
    let (mut up, mut down) = (0, 0);
    for (x, y) in a.chunks(4).zip(b.chunks(4)) {
        if (0..3).any(|c| y[c] > x[c].saturating_add(by)) {
            up += 1;
        }
        if (0..3).any(|c| x[c] > y[c].saturating_add(by)) {
            down += 1;
        }
    }
    (up, down)
}

/// **At night a carried light lights the land round it, and stands a halo**: at the town at
/// midnight, with the lamps on, at least one pixel in fifty is brighter than with them off by more
/// than eight levels in a channel (the land round the light, which retail lights by the sun
/// alone), under one in a thousand is darker by as much, and the light stands a halo.
#[test]
fn at_night_a_carried_light_lights_the_land_round_it_and_stands_a_halo() {
    let _turn = turn();
    let (off, on, lit, noise) = with_and_without("night-6fa9");
    let n = off.len() / 4;
    let (up, down) = moved(&off, &on, 8);
    eprintln!(
        "night-6fa9: {} lamps, {} halos; {up} brighter and {down} darker by over 8 (noise {noise})",
        lit.0, lit.1
    );
    assert_eq!(lit, (1, 1), "the carried light, one lamp with its halo");
    assert!(up * 50 >= n, "the lamps lit almost nothing: {up} pixels");
    assert!(
        down * 1000 < n + noise * 1000,
        "the lamps darkened {down} pixels"
    );
}

/// **By day the lamps keep retail's light**: at the town by day, with the lamps on, the land
/// stays as retail lights it and the objects the light reaches take its light per pixel in place
/// of at their vertices: under one pixel in a hundred is more than eight levels from the frame
/// with them off, and no halo stands.
#[test]
fn by_day_the_lamps_keep_retails_light() {
    let _turn = turn();
    let (off, on, lit, noise) = with_and_without("town-6fa9");
    let n = off.len() / 4;
    let (up, down) = moved(&off, &on, 8);
    eprintln!(
        "town-6fa9: {} lamps, {} halos; {up} brighter and {down} darker by over 8 (noise {noise})",
        lit.0, lit.1
    );
    assert_eq!(
        lit,
        (1, 0),
        "the carried light, one lamp and no halo by day"
    );
    assert!(
        (up + down) * 100 < n + noise * 100,
        "the lamps moved {up} pixels up and {down} down by day"
    );
}
