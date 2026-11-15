//! Behaviour: none (experimental)
//!
//! The temporal anti-aliasing, the shared passes' over the remaster's frame: while the view holds
//! still it settles to a still picture, which is the frame drawn without it but at the edges it
//! smooths.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames without and
//! with the anti-aliasing are written to.

use dereth_gfx_passes::post::PostSettings;

use crate::common::{frame, records, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The frames a still view is drawn before it is looked at: three of the anti-aliasing's
/// sequences.
const FRAMES: usize = 24;

/// Each pixel's greatest difference in a channel between `a` and `b`, levels.
fn apart(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.chunks(4)
        .zip(b.chunks(4))
        .map(|(x, y)| (0..3).map(|c| x[c].abs_diff(y[c])).max().unwrap_or(0))
        .collect()
}

/// **The anti-aliasing settles to the frame without it, smoothed at its edges**: at the outdoor
/// daytime stations, after three of its sequences a further frame differs from the last by under
/// half a level on average; and the settled frame is within six levels of the frame drawn
/// without it on average (the ferns' and the ground's finest detail softens a little), while at
/// least one pixel in two hundred differs by more than eight (the edges it smooths).
#[test]
fn the_anti_aliasing_settles_to_the_frame_without_it_smoothed_at_its_edges() {
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
        remaster.set_post(PostSettings::default());
        let off = remaster.render(&f, W, H, &mut load).expect("the frame");
        remaster.set_post(PostSettings {
            taa: true,
            ..PostSettings::default()
        });
        let mut on = off.clone();
        for _ in 0..FRAMES {
            on = remaster.render(&f, W, H, &mut load).expect("the frame");
        }
        let next = remaster.render(&f, W, H, &mut load).expect("the frame");
        let n = off.rgba.len() / 4;
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let mean = |d: &[u8]| d.iter().map(|x| f64::from(*x)).sum::<f64>() / n as f64;
        let settle = apart(&on.rgba, &next.rgba);
        let smooth = apart(&off.rgba, &on.rgba);
        let edges = smooth.iter().filter(|d| **d > 8).count();
        eprintln!(
            "{}: settled within {:.2} levels; {:.2} levels from the frame without it, {edges} of \
             {n} pixels by over 8",
            s.name,
            mean(&settle),
            mean(&smooth)
        );
        if let Some(dir) = &capture {
            write_png(
                &dir.join(format!("{}-taa-off.png", s.name)),
                W,
                H,
                &off.rgba,
            );
            write_png(&dir.join(format!("{}-taa-on.png", s.name)), W, H, &on.rgba);
        }
        assert!(mean(&settle) < 0.5, "{}: the frames do not settle", s.name);
        assert!(
            mean(&smooth) < 6.0,
            "{}: the anti-aliasing changed the picture",
            s.name
        );
        assert!(edges * 200 >= n, "{}: no edges were smoothed", s.name);
    }
}
