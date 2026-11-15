//! Behaviour: none (experimental)
//!
//! The ambient occlusion, the first of the shared passes the remaster takes: it only takes light
//! away (no pixel is brighter with it than without), and it takes some (what stands close round a
//! pixel shuts out part of its sky).
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames without and
//! with the occlusion are written to.

use dereth_gfx_passes::ao::AoSettings;

use crate::common::{frame, records, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The settle the occlusion takes over still frames: each station is drawn this many times.
const FRAMES: usize = 9;

/// **The occlusion only darkens, and darkens something**: at the outdoor stations, with it on, no
/// pixel is brighter than with it off by more than a level in any channel, and at least one in a
/// ten thousand is darker by more than four.
#[test]
fn the_occlusion_only_darkens_and_darkens_what_stands_close() {
    let _turn = turn();
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut remaster = auberean_remaster::Remaster::new().expect("the remaster's device");
    for s in STATIONS.iter().filter(|s| s.inside.is_none()).take(4) {
        let f = frame(s);
        remaster.set_ao(AoSettings::default());
        let off = remaster.render(&f, W, H, &mut load).expect("the frame");
        remaster.set_ao(AoSettings {
            on: true,
            ..AoSettings::default()
        });
        let mut on = off.clone();
        for _ in 0..FRAMES {
            on = remaster.render(&f, W, H, &mut load).expect("the frame");
        }
        let (mut brighter, mut darker) = (0usize, 0usize);
        for (a, b) in off.rgba.chunks(4).zip(on.rgba.chunks(4)) {
            if (0..3).any(|c| b[c] > a[c].saturating_add(1)) {
                brighter += 1;
            }
            if (0..3).any(|c| a[c] > b[c].saturating_add(4)) {
                darker += 1;
            }
        }
        let n = off.rgba.len() / 4;
        eprintln!(
            "{}: {brighter} pixels brighter, {darker} of {n} darker by over 4",
            s.name
        );
        if let Some(dir) = &capture {
            write_png(&dir.join(format!("{}-ao-off.png", s.name)), W, H, &off.rgba);
            write_png(&dir.join(format!("{}-ao-on.png", s.name)), W, H, &on.rgba);
        }
        assert_eq!(brighter, 0, "{}: the occlusion brightened pixels", s.name);
        assert!(
            darker * 10_000 >= n,
            "{}: the occlusion darkened almost nothing",
            s.name
        );
    }
}
