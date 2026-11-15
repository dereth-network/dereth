//! Behaviour: none (experimental)
//!
//! The remaster's whole look at once, as a player who ticks the remaster sees it: every pass on
//! together draws every station with the validation layer quiet, and draws it near the frame
//! without the passes.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frame with the look is
//! written to.

use crate::common::{frame, records, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The frames the look is drawn before it is looked at: three of the anti-aliasing's sequences.
const FRAMES: usize = 24;

/// **The whole look draws every station**: with every pass of the look on, after three of the
/// anti-aliasing's sequences, each station's frame is within ten levels of the frame without the
/// passes on average (measured: 2.3 to 6.5; the passes change the light, not what is drawn), and
/// the validation layer reports nothing.
#[test]
fn the_whole_look_draws_every_station() {
    let _turn = turn();
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut plain = auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    let mut looked = auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    looked.look();
    for s in &STATIONS {
        let f = frame(s);
        let off = plain.render(&f, W, H, &mut load).expect("the frame").rgba;
        let mut on = off.clone();
        for _ in 0..FRAMES {
            on = looked.render(&f, W, H, &mut load).expect("the frame").rgba;
        }
        let n = off.len() / 4;
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let mean = off
            .chunks(4)
            .zip(on.chunks(4))
            .map(|(a, b)| f64::from((0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)))
            .sum::<f64>()
            / n as f64;
        eprintln!(
            "{}: the look {mean:.2} levels from the frame without it",
            s.name
        );
        if let Some(dir) = &capture {
            write_png(&dir.join(format!("{}-look.png", s.name)), W, H, &on);
            write_png(&dir.join(format!("{}-plain.png", s.name)), W, H, &off);
        }
        assert!(
            mean < 10.0,
            "{}: the look moved the frame {mean:.2} levels",
            s.name
        );
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
