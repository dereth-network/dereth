//! Behaviour: none (experimental)
//!
//! The sun's shadows, the shared passes' cascades over the frame's own casters: they only take
//! light away (no pixel is brighter with them than without), and they take some (what stands
//! between a pixel and the sun shuts its sunlight out).
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames without and
//! with the shadows are written to.

use auberean_remaster::render::ShadowSettings;

use crate::common::{frame, records, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// **The sun's shadows only darken, and darken something**: at the outdoor daytime stations, with
/// them on, no pixel is brighter than with them off by more than a level in any channel, and at
/// least one in a thousand is darker by more than eight.
#[test]
fn the_suns_shadows_only_darken_and_darken_what_stands_out_of_the_sun() {
    let _turn = turn();
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut remaster = auberean_remaster::Remaster::new().expect("the remaster's device");
    for s in STATIONS
        .iter()
        .filter(|s| s.inside.is_none() && !s.midnight)
    {
        let f = frame(s);
        remaster.set_shadows(ShadowSettings::default());
        let off = remaster.render(&f, W, H, &mut load).expect("the frame");
        remaster.set_shadows(ShadowSettings {
            on: true,
            ..ShadowSettings::default()
        });
        let on = remaster.render(&f, W, H, &mut load).expect("the frame");
        if let (Some(dir), Ok(Some(v))) = (&capture, remaster.read_sun()) {
            let rgba: Vec<u8> = v.iter().flat_map(|&x| [x, x, x, 255]).collect();
            write_png(&dir.join(format!("{}-sun.png", s.name)), W, H, &rgba);
        }
        let (mut brighter, mut darker) = (0usize, 0usize);
        for (a, b) in off.rgba.chunks(4).zip(on.rgba.chunks(4)) {
            if (0..3).any(|c| b[c] > a[c].saturating_add(1)) {
                brighter += 1;
            }
            if (0..3).any(|c| a[c] > b[c].saturating_add(8)) {
                darker += 1;
            }
        }
        let n = off.rgba.len() / 4;
        eprintln!(
            "{}: {brighter} pixels brighter, {darker} of {n} darker by over 8",
            s.name
        );
        if let Some(dir) = &capture {
            write_png(
                &dir.join(format!("{}-shadows-off.png", s.name)),
                W,
                H,
                &off.rgba,
            );
            write_png(
                &dir.join(format!("{}-shadows-on.png", s.name)),
                W,
                H,
                &on.rgba,
            );
        }
        assert_eq!(brighter, 0, "{}: the shadows brightened pixels", s.name);
        assert!(
            darker * 1000 >= n,
            "{}: the shadows darkened almost nothing",
            s.name
        );
    }
}
